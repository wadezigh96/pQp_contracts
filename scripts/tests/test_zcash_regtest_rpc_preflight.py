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

    def test_accepts_exact_loopback_docker_mapping(self):
        PREFLIGHT.verify_docker_mapping(json.dumps({"18232/tcp": [{"HostIp": "127.0.0.1", "HostPort": "18232"}]}))

    def test_rejects_non_loopback_docker_mapping(self):
        with self.assertRaisesRegex(RuntimeError, "not exactly host-loopback"):
            PREFLIGHT.verify_docker_mapping(json.dumps({"18232/tcp": [{"HostIp": "0.0.0.0", "HostPort": "18232"}]}))

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
