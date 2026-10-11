#!/usr/bin/env python3
"""Minimal kaspa wRPC-JSON client: no third-party deps, one request per call.

The fleet host has python3 and no `websockets` module, and the measurement this
serves has to run on the node itself (the JSON listener is bound to 127.0.0.1).
So the WebSocket handshake and framing are inline: text frames only, client
frames masked as RFC 6455 requires, no continuation or compression.
"""
import base64, json, os, socket, struct, sys


class WsRpc:
    def __init__(self, host="127.0.0.1", port=26314, path="/", timeout=20.0):
        self.sock = socket.create_connection((host, port), timeout=timeout)
        self.sock.settimeout(timeout)
        key = base64.b64encode(os.urandom(16)).decode()
        req = (
            f"GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\n"
            f"Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        )
        self.sock.sendall(req.encode())
        self.buf = b""
        while b"\r\n\r\n" not in self.buf:
            chunk = self.sock.recv(4096)
            if not chunk:
                raise RuntimeError("the server closed during the handshake")
            self.buf += chunk
        head, self.buf = self.buf.split(b"\r\n\r\n", 1)
        if b"101" not in head.split(b"\r\n")[0]:
            raise RuntimeError(f"handshake refused: {head.split(chr(13).encode())[0]!r}")
        self.next_id = 1

    def _send_text(self, payload: bytes):
        mask = os.urandom(4)
        masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
        n = len(payload)
        if n < 126:
            header = struct.pack("!BB", 0x81, 0x80 | n)
        elif n < (1 << 16):
            header = struct.pack("!BBH", 0x81, 0x80 | 126, n)
        else:
            header = struct.pack("!BBQ", 0x81, 0x80 | 127, n)
        self.sock.sendall(header + mask + masked)

    def _read(self, n):
        while len(self.buf) < n:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise RuntimeError("the server closed the connection")
            self.buf += chunk
        out, self.buf = self.buf[:n], self.buf[n:]
        return out

    def _recv_frame(self):
        b0, b1 = self._read(2)
        opcode, length = b0 & 0x0F, b1 & 0x7F
        if length == 126:
            length = struct.unpack("!H", self._read(2))[0]
        elif length == 127:
            length = struct.unpack("!Q", self._read(8))[0]
        payload = self._read(length) if length else b""
        if b1 & 0x80:  # a server frame must not be masked, but unmask rather than fail
            mask = payload[:4]
            payload = bytes(b ^ mask[i % 4] for i, b in enumerate(payload[4:]))
        return opcode, payload

    def call(self, method, params=None):
        rid = self.next_id
        self.next_id += 1
        self._send_text(json.dumps({"id": rid, "method": method, "params": params or {}}).encode())
        while True:
            opcode, payload = self._recv_frame()
            if opcode == 0x8:
                raise RuntimeError("the server sent a close frame")
            if opcode in (0x9, 0xA):
                continue
            msg = json.loads(payload.decode())
            if msg.get("id") != rid:
                continue  # a notification, or an answer to somebody else
            if "error" in msg and msg["error"] is not None:
                raise RuntimeError(f"{method}: {msg['error']}")
            return msg.get("params", msg.get("result"))

    def close(self):
        try:
            self.sock.close()
        except OSError:
            pass


if __name__ == "__main__":
    port = int(os.environ.get("WRPC_PORT", "26314"))
    c = WsRpc(port=port)
    for method in sys.argv[1:] or ["getSystemInfo"]:
        if "=" in method:
            name, raw = method.split("=", 1)
            print(name, "->", json.dumps(c.call(name, json.loads(raw)), indent=2)[:2000])
        else:
            print(method, "->", json.dumps(c.call(method), indent=2)[:2000])
    c.close()
