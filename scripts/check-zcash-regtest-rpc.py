#!/usr/bin/env python3
"""Verify host-loopback reachability and authenticated Regtest RPC without logging secrets."""
import base64
import json
import ipaddress
import os
import socket
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ENDPOINT = "http://127.0.0.1:18232/"


class NoRedirectHandler(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *_args, **_kwargs):
        raise RuntimeError("RPC protocol failure: redirect rejected")


OPENER = urllib.request.build_opener(NoRedirectHandler)


def credentials(path):
    values = {}
    for line in Path(path).read_text().splitlines():
        key, separator, value = line.partition("=")
        if separator and key in ("rpcuser", "rpcpassword"):
            values[key] = value
    if not values.get("rpcuser") or not values.get("rpcpassword"):
        raise ValueError("private RPC credentials missing")
    return values["rpcuser"], values["rpcpassword"]


def verify_docker_internal_port(raw, port):
    try:
        exposed_raw, bindings_raw, active_raw, networks_raw = raw.split("|", 3)
        exposed = json.loads(exposed_raw)
        bindings = json.loads(bindings_raw)
        active = json.loads(active_raw)
        networks = json.loads(networks_raw)
    except (ValueError, json.JSONDecodeError):
        raise RuntimeError("Docker returned malformed internal port data") from None
    key = f"{port}/tcp"
    if not isinstance(exposed, dict) or key not in exposed:
        raise RuntimeError("Docker service does not expose the expected internal TCP port")
    if isinstance(bindings, dict) and bindings.get(key) not in (None, []):
        raise RuntimeError("Regtest service port must not be published on a host interface")
    if isinstance(active, dict) and active.get(key) not in (None, []):
        raise RuntimeError("Regtest service port unexpectedly has an active host mapping")
    if not isinstance(networks, dict) or len(networks) != 1:
        raise RuntimeError("Regtest service must attach only to its isolated Docker network")
    try:
        address = next(iter(networks.values()))["IPAddress"]
        ip = ipaddress.ip_address(address)
    except (StopIteration, KeyError, ValueError, TypeError):
        raise RuntimeError("Docker did not report a valid isolated Regtest container address") from None
    if ip not in ipaddress.ip_network("172.30.8.0/24"):
        raise RuntimeError("Regtest container address is outside the configured isolated bridge")
    return str(ip)


def check(path, timeout=45, report=True):
    user, password = credentials(path)
    token = base64.b64encode(f"{user}:{password}".encode()).decode()
    body = json.dumps({"jsonrpc": "1.0", "id": "qpc-rpc-preflight", "method": "getblockchaininfo", "params": []}).encode()
    request = urllib.request.Request(
        ENDPOINT, data=body, headers={"Authorization": f"Basic {token}", "Content-Type": "application/json"}
    )
    deadline = time.monotonic() + timeout
    last_error = None
    while time.monotonic() < deadline:
        try:
            with OPENER.open(request, timeout=2) as response:
                if response.status != 200:
                    raise RuntimeError("RPC protocol failure: unexpected HTTP status")
                payload = json.loads(response.read())
            if payload.get("error") is not None or not isinstance(payload.get("result"), dict):
                raise RuntimeError("RPC protocol failure: malformed JSON-RPC response")
            result = payload["result"]
            if result.get("chain") != "regtest" or not isinstance(result.get("blocks"), int):
                raise RuntimeError("RPC identity failure: endpoint did not report Regtest")
            if report:
                print(f"Authenticated full-node RPC reachable at {ENDPOINT}; chain=regtest, height={result['blocks']}")
            return
        except urllib.error.HTTPError as error:
            if error.code in (401, 403):
                raise RuntimeError("RPC authentication failure: credentials rejected") from None
            raise RuntimeError(f"RPC protocol failure: HTTP status {error.code}") from None
        except urllib.error.URLError as error:
            reason = error.reason
            if isinstance(reason, ConnectionRefusedError):
                last_error = "connection refused"
            elif isinstance(reason, (TimeoutError, socket.timeout)):
                last_error = "connection timed out"
            else:
                last_error = "connection failed"
            time.sleep(0.5)
        except (TimeoutError, socket.timeout):
            last_error = "connection timed out"
            time.sleep(0.5)
    raise RuntimeError(f"full-node RPC preflight failed: {last_error or 'connection failed'}")


if __name__ == "__main__":
    try:
        if len(sys.argv) == 4 and sys.argv[1] == "--verify-internal-port":
            print(verify_docker_internal_port(sys.argv[3], int(sys.argv[2])))
        elif len(sys.argv) == 1:
            check(os.environ["QPC_ZCASH_CONF"])
        else:
            raise RuntimeError("invalid RPC preflight invocation")
    except Exception as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
