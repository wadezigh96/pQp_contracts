#!/usr/bin/env python3
"""Forward one allowlisted local port to a container on QPC's isolated bridge."""
import ipaddress
import select
import socket
import socketserver
import sys

NETWORK = ipaddress.ip_network("172.30.8.0/24")
ALLOWED_PORTS = {9067, 18232}


def validate_target(port, address):
    try:
        ip = ipaddress.ip_address(address)
    except ValueError:
        raise ValueError("container address must be a literal IPv4 address") from None
    if port not in ALLOWED_PORTS or ip not in NETWORK or ip.version != 4:
        raise ValueError("target must be an allowlisted service on the isolated QPC Regtest bridge")
    return str(ip)


def serve(listen_port, target, target_port):
    if listen_port not in ALLOWED_PORTS or target_port != listen_port:
        raise ValueError("only matching allowlisted Regtest service ports may be forwarded")
    target = validate_target(target_port, target)

    class Handler(socketserver.BaseRequestHandler):
        def handle(self):
            with socket.create_connection((target, target_port), timeout=5) as upstream:
                self.request.settimeout(60)
                upstream.settimeout(60)
                sockets = [self.request, upstream]
                while True:
                    readable, _, _ = select.select(sockets, [], [], 60)
                    if not readable:
                        return
                    for source in readable:
                        data = source.recv(65536)
                        if not data:
                            return
                        destination = upstream if source is self.request else self.request
                        destination.sendall(data)

    class LoopbackServer(socketserver.ThreadingTCPServer):
        allow_reuse_address = False
        daemon_threads = True

    with LoopbackServer(("127.0.0.1", listen_port), Handler) as server:
        server.serve_forever()


if __name__ == "__main__":
    try:
        if len(sys.argv) != 4:
            raise ValueError("usage: forward-zcash-regtest-port.py PORT CONTAINER_IP PORT")
        port = int(sys.argv[1])
        serve(port, sys.argv[2], int(sys.argv[3]))
    except Exception as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
