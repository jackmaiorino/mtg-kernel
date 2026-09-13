"""Local browser interface to the fixed-seat native human match process."""
from __future__ import annotations

import argparse
import json
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
import queue
import secrets
import subprocess
import threading
import time

MAX_RESPONSE = 4 * 1024 * 1024


class MatchClient:
    def __init__(self, executable: Path, config: Path, stderr: Path):
        self.queue = queue.Queue(maxsize=2)
        self.counter = 0
        self.error_log = stderr.open("xb")
        try:
            self.process = subprocess.Popen(
                [str(executable), "--config", str(config)], stdin=subprocess.PIPE,
                stdout=subprocess.PIPE, stderr=self.error_log,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
        except Exception:
            self.error_log.close()
            raise
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        while True:
            raw = self.process.stdout.readline(MAX_RESPONSE + 1)
            self.queue.put(raw)
            if not raw or len(raw) > MAX_RESPONSE:
                return

    def command(self, command):
        if self.process.poll() is not None:
            raise RuntimeError("The match process stopped. Its record has been preserved.")
        self.counter += 1
        request = {**command, "request_id": f"ui-{self.counter}"}
        self.process.stdin.write(json.dumps(request, separators=(",", ":")).encode() + b"\n")
        self.process.stdin.flush()
        try:
            raw = self.queue.get(timeout=120)
        except queue.Empty as error:
            raise RuntimeError("The match is taking longer than expected. Do not resubmit an action.") from error
        if not raw or len(raw) > MAX_RESPONSE or not raw.endswith(b"\n"):
            raise RuntimeError("The match response was incomplete. Its record has been preserved.")
        response = json.loads(raw)
        if response.get("request_id") != request["request_id"]:
            raise RuntimeError("The match response did not match this request.")
        return response

    def close(self):
        try:
            self.process.stdin.close()
            self.process.wait(timeout=3)
        except (OSError, subprocess.TimeoutExpired):
            self.process.kill()
            self.process.wait(timeout=3)
        finally:
            self.error_log.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", required=True, type=Path)
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--card-text", type=Path)
    args = parser.parse_args()
    args.runtime_dir.mkdir(parents=True, exist_ok=False)
    token = secrets.token_urlsafe(24)
    cache = {"response": None, "error": None}
    ui = (Path(__file__).parent / "index.html").read_text(encoding="utf-8").replace("__SESSION_TOKEN__", token).encode()
    card_db = json.loads((Path(__file__).resolve().parents[3] / "data/cards_v1.json").read_text(encoding="utf-8"))
    # This is static public card information, not a live deck/hand inventory.
    cards = {card["name"]: card for card in card_db["cards"]}
    if args.card_text:
        for name, public in json.loads(args.card_text.read_text(encoding="utf-8")).items():
            if name in cards:
                cards[name]["oracle_text"] = public.get("oracle_text") or "\n\n".join(
                    face.get("oracle_text", "") for face in public.get("card_faces") or [])

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def send(self, status, value, content_type="application/json; charset=utf-8"):
            data = value if isinstance(value, bytes) else json.dumps(value, allow_nan=False).encode()
            self.send_response(status)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Cache-Control", "no-store")
            self.send_header("X-Content-Type-Options", "nosniff")
            self.send_header("Referrer-Policy", "no-referrer")
            self.send_header("Content-Security-Policy", "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'")
            self.end_headers()
            self.wfile.write(data)

        def valid_host(self):
            return self.headers.get("Host") in {f"127.0.0.1:{self.server.server_port}", f"localhost:{self.server.server_port}"}

        def do_GET(self):
            if not self.valid_host():
                return self.send(403, {"error": "Local interface only."})
            if self.path == "/":
                return self.send(200, ui, "text/html; charset=utf-8")
            if self.path == "/api/cards":
                return self.send(200, cards)
            if self.path == "/api/state":
                if cache["response"] is None and cache["error"] is None:
                    try:
                        cache["response"] = client.command({"command": "current"})
                    except Exception as error:
                        cache["error"] = str(error)
                return self.send(200, cache)
            return self.send(404, {"error": "Not found."})

        def do_POST(self):
            if not self.valid_host() or self.path != "/api/command" or self.headers.get("X-Session-Token") != token:
                return self.send(403, {"error": "Refresh the local match page."})
            origin = self.headers.get("Origin")
            if origin is not None and origin not in {f"http://127.0.0.1:{self.server.server_port}", f"http://localhost:{self.server.server_port}"}:
                return self.send(403, {"error": "Invalid request origin."})
            try:
                length = int(self.headers.get("Content-Length", "0"))
                if not 0 < length <= 16384:
                    return self.send(400, {"error": "Invalid request length."})
                command = json.loads(self.rfile.read(length))
                if not isinstance(command, dict) or not isinstance(command.get("command"), str):
                    return self.send(400, {"error": "Invalid command."})
                if cache["error"] is not None:
                    return self.send(409, cache)
                cache["response"] = client.command(command)
            except Exception as error:
                cache["error"] = str(error)
                return self.send(500, cache)
            # A browser disconnect after mutation must not poison a healthy match.
            # The accepted response remains available through GET /api/state.
            try:
                self.send(200, cache)
            except (BrokenPipeError, ConnectionResetError):
                pass

    server = HTTPServer(("127.0.0.1", args.port), Handler)
    client = None
    try:
        client = MatchClient(args.engine.resolve(), args.config.resolve(), args.runtime_dir / "engine-stderr.log")
        url = f"http://127.0.0.1:{server.server_port}/"
        (args.runtime_dir / "server.json").write_text(json.dumps({"url": url, "started_at": time.time(), "pid": __import__("os").getpid(), "engine_pid": client.process.pid}, indent=2), encoding="utf-8")
        print(url, flush=True)
        server.serve_forever()
    finally:
        server.server_close()
        if client is not None:
            client.close()


if __name__ == "__main__":
    main()
