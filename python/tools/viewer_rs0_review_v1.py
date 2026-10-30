"""RS0 position review: judge by eye the Spy roots where forward search misses a certified win.

usage:
  viewer_rs0_review_v1.py roots --roots ROOTS.jsonl --out SUBSET.jsonl [--roles spy:k2,spy:k1]
  viewer_rs0_review_v1.py build --roots SUBSET.jsonl --export EXPORT.jsonl --rs0 RUN_DIR
                                --checker ROWS_GLOB --out POSITIONS.json
  viewer_rs0_review_v1.py serve --positions POSITIONS.json --verdicts VERDICTS.jsonl [--port N] [--open]

`roots` selects reach roots (tools/reach planner rows) by stratum:role, in role
order. Run regret_census_v1 mode=s4a-review-export with REVIEW_ROOTS=SUBSET on
them; its rows carry each root's full board, the recorded decisions up to the
certified decision and the legal actions at both ends. `build` joins those rows
with the RS0 search rows (found or not per budget and seed) and the step 1
win-check rows (the certified oracle and robust lines). `serve` shows the
positions on 127.0.0.1 and appends one verdict per line to VERDICTS; the latest
verdict for a root wins.

Verdicts are evaluation and design input only: they never become training data
or goal states. Loopback only, because positions carry hidden game state.
"""
from __future__ import annotations

import argparse
import glob
import hashlib
import json
import re
import threading
import time
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

SCHEMA = "mtg-kernel-rs0-review/v1"
PAGE = Path(__file__).with_suffix(".html")
CHOICES = {
    "real_win": {"yes", "no", "hidden"},
    "human_find": {"yes", "maybe", "no"},
    "how": {"backward", "forward", "both"},
}
MAX_VERDICT_BYTES = 16 * 1024
CARD = re.compile(r"CardStableRefV1 \{ arena_id: (\d+), card_db_id: (\d+), owner: P(\d), "
                  r"controller: P\d, zone: (\w+), zone_change_count: \d+ \}")
PLAYER = re.compile(r"Player \{ player: P(\d) \}")


def read_jsonl(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def select_roots(rows: list[dict], roles: list[str]) -> list[dict]:
    out = []
    for wanted in roles:
        stratum, role = wanted.split(":")
        out += sorted((r for r in rows if any(x["stratum"] == stratum and x["role"] == role for x in r["roles"])),
                      key=lambda r: r["root_id"])
    return out


def describe(action: str | None, names: dict[int, str], focal: int) -> str | None:
    """A readable label for an engine action's Debug text (names from the game's own boards)."""
    if action is None:
        return None
    text = CARD.sub(lambda m: f"{names.get(int(m[2]), 'card ' + m[2])} ({m[4].lower()})", action)
    text = PLAYER.sub(lambda m: "you" if int(m[1]) == focal else "opponent", text)
    text = re.sub(r"actor: P\d(, )?", "", text)
    text = re.sub(r"Object \{ object: ([^{}]*?) \}", r"\1", text)
    text = re.sub(r", (cost_target: None|granted_ability: GrantedAbilityPublicV1\(None\))", "", text)
    text = re.sub(r"Some\(([^()]*)\)", r"\1", text)
    kind, _, rest = text.partition(" ")
    kind = re.sub(r"(?<!^)(?=[A-Z])", " ", kind).lower()
    rest = rest.strip().removeprefix("{").removesuffix("}").strip()
    return f"{kind}: {rest}" if rest else kind


def card_names(rows: list[dict]) -> dict[int, str]:
    names: dict[int, str] = {}

    def walk(v: object) -> None:
        if isinstance(v, dict):
            if isinstance(v.get("db"), int) and isinstance(v.get("name"), str):
                names[v["db"]] = v["name"]
            for x in v.values():
                walk(x)
        elif isinstance(v, list):
            for x in v:
                walk(x)

    walk([[row["root"]["board"], row["certified"]["board"]] for row in rows])
    for row in rows:
        names.update({int(k): v for k, v in row.get("names", {}).items()})
    return names


def rs0_results(run: Path) -> dict[str, dict]:
    """Per root and budget: which seeds found a verified Spy completion (o1_spy)."""
    out: dict[str, dict] = {}
    for f in sorted(glob.glob(str(run / "main-*.jsonl")) + glob.glob(str(run / "large-*.jsonl"))):
        for r in read_jsonl(Path(f)):
            if r.get("kind") != "search":
                continue
            seeds = out.setdefault(r["root_id"], {}).setdefault(str(r["budget"]), {})
            discovery = (r.get("selection") or {}).get("discovery") or {}
            seeds[str(r["seed"])] = {"found": bool(r["o1_spy"]) and not r["failed"], "failed": r["failed"],
                                     "completions": discovery.get("completions_natural"),
                                     "trajectories": discovery.get("trajectories")}
    return out


def checker_rows(pattern: str) -> dict[tuple[str, int], dict]:
    out = {}
    for f in sorted(glob.glob(pattern)):
        for line in Path(f).read_text(encoding="utf-8").splitlines():
            try:
                r = json.loads(line)
            except json.JSONDecodeError:
                continue
            if r.get("kind") == "decision":
                out[(r["id"], r["index"])] = r
    return out


def build(roots: Path, export: Path, run: Path, checker: str, out: Path) -> dict:
    exported = {r["root_id"]: r for r in read_jsonl(export)}
    results = rs0_results(run)
    checks = checker_rows(checker)
    positions, problems = [], []
    # Names come from every exported board, so a card seen only between two snapshots still has one.
    names = card_names([r for r in exported.values() if r.get("kind") == "review"])
    for root in read_jsonl(roots):
        row = exported.get(root["root_id"])
        if row is None or row.get("kind") != "review":
            problems.append(f"{root['root_id']}: {row.get('error') if row else 'not exported'}")
            continue
        focal = row["focal"]
        check = checks.get((row["game_id"], row["certified_index"]))
        if check is None or check["oracle"]["verdict"] != "proven":
            problems.append(f"{root['root_id']}: no proven win-check row at {row['certified_index']}")
            continue
        legal = row["certified"]["legal"]
        line = check["oracle"]["line"]
        if not line or line[0] not in legal:
            problems.append(f"{root['root_id']}: the certified line does not start with a legal action")
            continue
        robust = check.get("robust") or {}
        positions.append({
            **{k: row[k] for k in ("root_id", "game_id", "role", "stratum", "opportunity_turn", "index",
                                   "certified_index", "focal", "deck_ids", "winner")},
            "root": {**row["root"], "legal": [describe(a, names, focal) for a in row["root"]["legal"]]},
            "certified": {**row["certified"], "legal": [describe(a, names, focal) for a in legal],
                          "chosen": describe(legal[row["certified"]["selected"]], names, focal)},
            "path": [{**p, "action": describe(p["action"], names, focal)} for p in row["path"]],
            "line": [describe(a, names, focal) for a in line],
            "line_raw": line,
            "robust": {"verdict": robust.get("verdict"), "unknown_reason": robust.get("unknown_reason"),
                       "same_line": robust.get("line") == line if robust.get("line") else None},
            "oracle_nodes": check["oracle"]["nodes"],
            "rs0": results.get(row["root_id"], {}),
        })
    bundle = {"schema": SCHEMA, "positions": positions, "problems": problems,
              "sources": {"roots": sha256(roots), "export": sha256(export), "rs0_run": str(run).replace("\\", "/"),
                          "checker": checker}}
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(bundle, separators=(",", ":")), encoding="utf-8", newline="\n")
    return bundle


class Review:
    def __init__(self, positions: Path, verdicts: Path) -> None:
        self.positions, self.verdicts = positions, verdicts
        self.digest = sha256(positions)
        self.ids = {p["root_id"]: p for p in json.loads(positions.read_text(encoding="utf-8"))["positions"]}
        self.lock = threading.Lock()

    def latest(self) -> dict[str, dict]:
        if not self.verdicts.exists():
            return {}
        return {r["root_id"]: r for r in read_jsonl(self.verdicts)}

    def append(self, record: object) -> dict:
        if not isinstance(record, dict) or record.get("root_id") not in self.ids:
            raise ValueError("verdict needs a known root_id")
        for field, allowed in CHOICES.items():
            if record.get(field) not in allowed:
                raise ValueError(f"{field} must be one of {sorted(allowed)}")
        if not isinstance(record.get("note", ""), str):
            raise ValueError("note must be text")
        p = self.ids[record["root_id"]]
        out = {"root_id": p["root_id"], "game_id": p["game_id"], "role": p["role"], "root_index": p["index"],
               "certified_index": p["certified_index"], **{k: record[k] for k in CHOICES},
               "note": record.get("note", ""), "positions_sha256": self.digest,
               "reviewed_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "use": "evaluation and design input only"}
        with self.lock:
            self.verdicts.parent.mkdir(parents=True, exist_ok=True)
            with self.verdicts.open("a", encoding="utf-8", newline="\n") as stream:
                stream.write(json.dumps(out, ensure_ascii=False, separators=(",", ":")) + "\n")
        return out


def handler(review: Review) -> type[BaseHTTPRequestHandler]:
    class Handler(BaseHTTPRequestHandler):
        def send(self, status: int, body: bytes, kind: str) -> None:
            self.send_response(status)
            self.send_header("Content-Type", kind)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self) -> None:
            path = self.path.split("?", 1)[0]
            if path in ("/", "/index.html"):
                return self.send(200, PAGE.read_bytes(), "text/html; charset=utf-8")
            if path == "/api/positions":
                return self.send(200, review.positions.read_bytes(), "application/json")
            if path == "/api/verdicts":
                return self.send(200, json.dumps(review.latest()).encode(), "application/json")
            self.send(404, b"not found", "text/plain")

        def do_POST(self) -> None:
            if self.path != "/api/verdicts":
                return self.send(404, b"not found", "text/plain")
            length = int(self.headers.get("Content-Length") or 0)
            if not 0 < length <= MAX_VERDICT_BYTES:
                return self.send(413, b"verdict too large", "text/plain")
            try:
                saved = review.append(json.loads(self.rfile.read(length)))
            except ValueError as exc:
                return self.send(400, str(exc).encode(), "text/plain")
            self.send(200, json.dumps(saved).encode(), "application/json")

        def log_message(self, format: str, *args: object) -> None:
            if self.command == "POST":
                super().log_message(format, *args)

    return Handler


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("roots")
    p.add_argument("--roots", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--roles", default="spy:k2,spy:k1")
    p = sub.add_parser("build")
    p.add_argument("--roots", type=Path, required=True)
    p.add_argument("--export", type=Path, required=True)
    p.add_argument("--rs0", type=Path, required=True)
    p.add_argument("--checker", required=True, help="glob of step 1 win-check rows")
    p.add_argument("--out", type=Path, required=True)
    p = sub.add_parser("serve")
    p.add_argument("--positions", type=Path, required=True)
    p.add_argument("--verdicts", type=Path, required=True)
    p.add_argument("--port", type=int, default=8766)
    p.add_argument("--open", action="store_true")
    a = parser.parse_args(argv)
    if a.command == "roots":
        rows = select_roots(read_jsonl(a.roots), a.roles.split(","))
        a.out.write_text("".join(json.dumps(r) + "\n" for r in rows), encoding="utf-8", newline="\n")
        print(json.dumps({"roots": len(rows), "out": str(a.out)}))
    elif a.command == "build":
        bundle = build(a.roots, a.export, a.rs0, a.checker, a.out)
        print(json.dumps({"positions": len(bundle["positions"]), "problems": bundle["problems"]}))
    else:
        server = ThreadingHTTPServer(("127.0.0.1", a.port), handler(Review(a.positions, a.verdicts)))
        url = f"http://127.0.0.1:{server.server_address[1]}/"
        print(f"RS0 review at {url} (verdicts -> {a.verdicts}); Ctrl+C to stop", flush=True)
        if a.open:
            webbrowser.open(url)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
