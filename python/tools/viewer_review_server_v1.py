"""Serve Decision Studio in misplay-review mode on localhost and append reviewer flags to a JSONL file.

usage: viewer_review_server_v1.py --games DIR --flags FILE [--dist DIR] [--port N] [--open]

DIR holds viewer recordings plus index.json ([{id, file, title, subtitle}]), as
written by a recording converter. The server lists them in demo/manifest.json
with their SHA-256 (the viewer verifies each before rendering) and marks the
manifest as a review session, which turns on the game list, trivial-decision
skipping, jump-to-turn and the M flag key. GET api/flags returns every flag
record so far; POST api/flags appends one record per line. Loopback only:
recordings can carry hidden game state.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import mimetypes
import re
import threading
import time
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

CATEGORIES = {"tactical", "sequencing", "mana", "misread", "strategic", "other"}
FILE = re.compile(r"^[-a-zA-Z0-9_]+\.json$")
MAX_FLAG_BYTES = 64 * 1024


class Review:
    def __init__(self, games: Path, flags: Path, dist: Path) -> None:
        self.games, self.flags, self.dist = games, flags, dist.resolve()
        self.lock = threading.Lock()

    def manifest(self) -> dict:
        entries = json.loads((self.games / "index.json").read_text(encoding="utf-8"))
        games = []
        for e in entries:
            if FILE.match(e["file"]) and (self.games / e["file"]).is_file():
                digest = hashlib.sha256((self.games / e["file"]).read_bytes()).hexdigest()
                games.append({"id": e["id"], "file": e["file"], "sha256": digest, "title": e.get("title"), "subtitle": e.get("subtitle")})
        return {"games": games, "review": {"flags": "api/flags"}}

    def read_flags(self) -> list[dict]:
        if not self.flags.exists():
            return []
        return [json.loads(line) for line in self.flags.read_text(encoding="utf-8").splitlines() if line.strip()]

    def append_flag(self, record: dict) -> None:
        if (not isinstance(record, dict) or record.get("category") not in CATEGORIES
                or not isinstance(record.get("game"), str) or not isinstance(record.get("decision"), str)
                or type(record.get("index")) is not int or not isinstance(record.get("note", ""), str)):
            raise ValueError("flag needs game, decision, index and a known category")
        record = {**record, "flagged_at": time.strftime("%Y-%m-%dT%H:%M:%S%z")}
        with self.lock:
            self.flags.parent.mkdir(parents=True, exist_ok=True)
            with self.flags.open("a", encoding="utf-8", newline="\n") as stream:
                stream.write(json.dumps(record, ensure_ascii=False, separators=(",", ":")) + "\n")


def handler(review: Review) -> type[BaseHTTPRequestHandler]:
    class Handler(BaseHTTPRequestHandler):
        def send(self, status: int, body: bytes, kind: str) -> None:
            self.send_response(status)
            self.send_header("Content-Type", kind)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(body)

        def send_json(self, value: object) -> None:
            self.send(200, json.dumps(value).encode(), "application/json")

        def do_GET(self) -> None:
            path = self.path.split("?", 1)[0].split("#", 1)[0].lstrip("/")
            if path == "demo/manifest.json":
                return self.send_json(review.manifest())
            if path == "api/flags":
                return self.send_json(review.read_flags())
            if path.startswith("demo/") and FILE.match(path[5:]):
                target = review.games / path[5:]
                if target.is_file():
                    return self.send(200, target.read_bytes(), "application/json")
                return self.send(404, b"not found", "text/plain")
            target = (review.dist / (path or "index.html")).resolve()
            if not target.is_relative_to(review.dist) or not target.is_file():
                target = review.dist / "index.html"
            self.send(200, target.read_bytes(), mimetypes.guess_type(target.name)[0] or "application/octet-stream")

        def do_POST(self) -> None:
            if self.path != "/api/flags":
                return self.send(404, b"not found", "text/plain")
            length = int(self.headers.get("Content-Length") or 0)
            if not 0 < length <= MAX_FLAG_BYTES:
                return self.send(413, b"flag too large", "text/plain")
            try:
                review.append_flag(json.loads(self.rfile.read(length)))
            except ValueError as exc:
                return self.send(400, str(exc).encode(), "text/plain")
            self.send_json({"ok": True})

        def log_message(self, format: str, *args: object) -> None:
            if self.command == "POST":
                super().log_message(format, *args)

    return Handler


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--games", type=Path, required=True)
    parser.add_argument("--flags", type=Path, required=True)
    parser.add_argument("--dist", type=Path, default=Path(__file__).resolve().parents[2] / "viewer" / "dist")
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--open", action="store_true", help="open the review in the default browser")
    args = parser.parse_args(argv)
    if not (args.dist / "index.html").is_file():
        parser.error(f"{args.dist} has no built viewer; run npm run build in viewer/")
    if not (args.games / "index.json").is_file():
        parser.error(f"{args.games} has no index.json")
    server = ThreadingHTTPServer(("127.0.0.1", args.port), handler(Review(args.games, args.flags, args.dist)))
    url = f"http://127.0.0.1:{server.server_address[1]}/"
    print(f"misplay review at {url} (flags -> {args.flags}); Ctrl+C to stop", flush=True)
    if args.open:
        webbrowser.open(url)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
