#!/usr/bin/env python3
"""Verify host-loopback reachability and authenticated Regtest RPC without logging secrets."""
import base64
import json
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


def check(path, timeout=45):
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
        check(os.environ["QPC_ZCASH_CONF"])
    except Exception as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
