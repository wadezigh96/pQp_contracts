import base64
import importlib.util
import json
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import patch
import socket
import urllib.error

SCRIPT = Path(__file__).resolve().parents[1] / "check-zcash-regtest-rpc.py"
SPEC = importlib.util.spec_from_file_location("qpc_rpc_preflight", SCRIPT)
PREFLIGHT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PREFLIGHT)
PROXY_SPEC = importlib.util.spec_from_file_location(
    "qpc_regtest_proxy", Path(__file__).resolve().parents[1] / "forward-zcash-regtest-port.py"
)
PROXY = importlib.util.module_from_spec(PROXY_SPEC)
PROXY_SPEC.loader.exec_module(PROXY)


class Handler(BaseHTTPRequestHandler):
    status = 200
    body = {"error": None, "result": {"chain": "regtest", "blocks": 0}}
    expected_auth = ""

    def do_POST(self):
        if self.headers.get("Authorization") != self.expected_auth:
            self.send_response(401)
            self.end_headers()
            return
        self.send_response(self.status)
        self.end_headers()
        self.wfile.write(json.dumps(self.body).encode())

    def log_message(self, *_args):
        pass


class RpcPreflightTests(unittest.TestCase):
    def setUp(self):
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.previous_endpoint = PREFLIGHT.ENDPOINT
        PREFLIGHT.ENDPOINT = f"http://127.0.0.1:{self.server.server_port}/"
        Handler.status = 200
        Handler.body = {"error": None, "result": {"chain": "regtest", "blocks": 0}}
        Handler.expected_auth = "Basic " + base64.b64encode(b"test-user:test-password").decode()
        self.tmp = TemporaryDirectory()
        self.config = Path(self.tmp.name) / "zcashd.conf"
        self.config.write_text("rpcuser=test-user\nrpcpassword=test-password\n")

    def tearDown(self):
        PREFLIGHT.ENDPOINT = self.previous_endpoint
        self.server.shutdown()
        self.thread.join()
        self.server.server_close()
        self.tmp.cleanup()

    def inspect_data(self, port=18232, address="172.30.8.2", bindings=None, active=None):
        exposed = {f"{port}/tcp": {}}
        networks = {"qpc-regtest-demo": {"IPAddress": address}}
        return "|".join(map(json.dumps, (exposed, bindings or {}, active or {f"{port}/tcp": None}, networks)))

    def test_accepts_internal_only_container_rpc(self):
        self.assertEqual(PREFLIGHT.verify_docker_internal_port(self.inspect_data(), 18232), "172.30.8.2")

    def test_rejects_published_host_rpc(self):
        bindings = {"18232/tcp": [{"HostIp": "127.0.0.1", "HostPort": "18232"}]}
        with self.assertRaisesRegex(RuntimeError, "must not be published"):
            PREFLIGHT.verify_docker_internal_port(self.inspect_data(bindings=bindings), 18232)

    def test_rejects_non_isolated_container_address(self):
        with self.assertRaisesRegex(RuntimeError, "outside the configured isolated bridge"):
            PREFLIGHT.verify_docker_internal_port(self.inspect_data(address="192.168.1.3"), 18232)

    def test_node_explicitly_enables_deprecated_coinbase_funding_rpc(self):
        compose = (SCRIPT.parents[1] / "zcash" / "regtest" / "docker-compose.yml").read_text()
        self.assertIn("-allowdeprecated=getnewaddress", compose)

    def test_proxy_accepts_only_qpc_internal_targets(self):
        self.assertEqual(PROXY.validate_target(18232, "172.30.8.2"), "172.30.8.2")
        with self.assertRaisesRegex(ValueError, "isolated QPC Regtest bridge"):
            PROXY.validate_target(18232, "127.0.0.1")
        with self.assertRaisesRegex(ValueError, "isolated QPC Regtest bridge"):
            PROXY.validate_target(80, "172.30.8.2")

    def test_accepts_authenticated_regtest_rpc(self):
        PREFLIGHT.check(self.config, timeout=1, report=False)

    def test_distinguishes_authentication_failure(self):
        Handler.expected_auth = "Basic " + base64.b64encode(b"wrong:credentials").decode()
        with self.assertRaisesRegex(RuntimeError, "authentication failure"):
            PREFLIGHT.check(self.config, timeout=1, report=False)

    def test_distinguishes_rpc_protocol_failure(self):
        Handler.body = {"error": {"code": -1}, "result": None}
        with self.assertRaisesRegex(RuntimeError, "protocol failure"):
            PREFLIGHT.check(self.config, timeout=1, report=False)

    def test_distinguishes_connection_refused(self):
        self.server.shutdown()
        self.server.server_close()
        with self.assertRaisesRegex(RuntimeError, "connection refused"):
            PREFLIGHT.check(self.config, timeout=0.1, report=False)

    def test_distinguishes_connection_timeout(self):
        with patch.object(PREFLIGHT.OPENER, "open", side_effect=urllib.error.URLError(socket.timeout())):
            with self.assertRaisesRegex(RuntimeError, "connection timed out"):
                PREFLIGHT.check(self.config, timeout=0.1, report=False)


if __name__ == "__main__":
    unittest.main()
