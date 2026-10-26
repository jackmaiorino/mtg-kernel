"""DraftZero-shaped driver for mtg-kernel Limited matches (issue #110, milestone 7).

`MtgKernel` mirrors `draftzero.engine.Gorge`: the same deck directory and pool.tsv inputs, the same
`play()`/`coverage()`/`version()` calls and the same games.jsonl rows, over the
`kernel_limited_match_v1` binary instead of `dzgorge`. Policy specs are mtg-kernel's own:

    random                          uniform over the legal actions
    pimc:sims=N:depth=D:closedlist  fair determinized search; the opponent's unseen cards come
                                    from the belief corpus (-belief-*, default: the train split)
    pimc:...:truelist | :peek       comparison modes that leak the opponent's list or all hidden cards

Commands:

    python limited_match_v1.py fixture-pool OUT_DIR       the two FDN fixture decks as a deck dir + pool.tsv
    python limited_match_v1.py smoke --binary PATH        coverage, two short matches and an exact replay
"""
from __future__ import annotations

import argparse
from dataclasses import dataclass, field
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time
from typing import Sequence

from limited_decks_v1 import parse_dck

REPO_ROOT = Path(__file__).resolve().parents[2]
FIXTURES = REPO_ROOT / "data/limited/fdn_v1"
FIXTURE_DECKS = ("FDN_top_04956_UG", "FDN_top_20626_WG")
SCHEMA = "mtg-kernel-limited-match/v1"
ROW_KEYS = {"game", "pair", "leg", "seed", "decks", "side", "pols", "winner", "result", "turns", "ms"}
RESULTS = {"A", "B", "draw", "stall", "error"}


class MtgKernelError(RuntimeError):
    """The driver refused a run (bad spec, missing file, unplayable pool)."""


@dataclass
class Belief:
    """A closedlist seat's belief corpus: deck JSONs + pool.tsv, and the split its decks come from."""
    decks: Path
    pool: Path
    split: str = "train"


@dataclass
class PlayResult:
    games: list[dict]
    summary: dict
    games_path: Path
    corpus_path: Path | None = None
    skipped: list[int] = field(default_factory=list)
    runs: list[dict] = field(default_factory=list)
    wall_s: float = 0.0


class MtgKernel:
    """The kernel_limited_match_v1 driver with one run's engine settings.

    `command` is the binary path, or an argv prefix (tests use a Python stub)."""

    def __init__(self, command: Path | str | Sequence[str], decks: Path, pool: Path, *, max_turns: int = 100,
                 max_decisions: int = 20000):
        self.command = [str(command)] if isinstance(command, (str, Path)) else [str(c) for c in command]
        self.decks = Path(decks).absolute()
        self.pool = Path(pool).absolute()
        self.max_turns, self.max_decisions = int(max_turns), int(max_decisions)

    def _run(self, args: Sequence[str], timeout_s: float | None = None) -> subprocess.CompletedProcess:
        done = subprocess.run(self.command + [str(a) for a in args], capture_output=True, text=True,
                              timeout=timeout_s)
        if done.returncode != 0:
            raise MtgKernelError(f"{' '.join(args[:1])} exited {done.returncode}: {done.stderr[-2000:]}")
        return done

    def version(self) -> dict:
        return json.loads(self._run(["version"]).stdout)

    def coverage(self) -> dict:
        """{cards, decks, unsupported_cards, failed_decks, supported_decks, unsupported: {card: {decks, missing}},
        supported: [deck names]}, the same shape as Gorge.coverage()."""
        with tempfile.TemporaryDirectory(prefix="mkcov-") as tmp:
            out = Path(tmp) / "supported.txt"
            text = self._run(["coverage", "-decks", self.decks, "-pool", self.pool, "-out", out]).stdout
            supported = out.read_text().split() if out.exists() else []
        return parse_coverage(text, supported)

    def play_argv(self, a: str, b: str, *, pairs: int, seed: int, out: Path, split: str = "train",
                  first_pair: int = 0, workers: int = 2, belief: Belief | None = None,
                  single: bool = False) -> list[str]:
        argv = ["play", "-decks", str(self.decks), "-pool", str(self.pool), "-split", split,
                "-seed", str(int(seed)), "-first-pair", str(int(first_pair)), "-pairs", str(int(pairs)),
                "-workers", str(int(workers)), "-a", a, "-b", b, "-out", str(out),
                "-max-turns", str(self.max_turns), "-max-decisions", str(self.max_decisions)]
        if belief is not None:
            argv += ["-belief-decks", str(belief.decks), "-belief-pool", str(belief.pool),
                     "-belief-split", belief.split]
        if single:
            argv.append("-single")
        return argv

    def play(self, a: str, b: str, *, pairs: int, seed: int, out_dir: Path, split: str = "train",
             first_pair: int = 0, workers: int = 2, belief: Belief | None = None, single: bool = False,
             timeout_s: float | None = None) -> PlayResult:
        """Play pairs [first_pair, first_pair + pairs) of (seed, split) into out_dir/games.jsonl."""
        if pairs <= 0:
            raise ValueError(f"pairs={pairs}: nothing to play")
        out_dir = Path(out_dir)
        out_dir.mkdir(parents=True, exist_ok=True)
        games_path = out_dir / "games.jsonl"
        started = time.monotonic()
        self._run(self.play_argv(a, b, pairs=pairs, seed=seed, out=games_path, split=split,
                                 first_pair=first_pair, workers=workers, belief=belief, single=single),
                  timeout_s=timeout_s)
        wall = time.monotonic() - started
        games = [json.loads(line) for line in games_path.read_text().splitlines() if line.strip()]
        summary = json.loads(Path(str(games_path) + ".summary.json").read_text())
        return PlayResult(games=games, summary=summary, games_path=games_path,
                          runs=[{"first_pair": first_pair, "pairs": pairs, "returncode": 0, "killed": False,
                                 "wall_s": wall, "games": len(games)}], wall_s=wall)


def parse_coverage(text: str, supported: list[str]) -> dict:
    import re
    res: dict = {"unsupported": {}, "supported": supported}
    for line in text.splitlines():
        if m := re.match(r"unsupported: (.+?)\s+in\s+(\d+) decks\s+missing \[(.*)\]\s*$", line):
            res["unsupported"][m[1]] = {"decks": int(m[2]), "missing": m[3]}
        elif m := re.match(r"(\d+) distinct cards in (\d+) decks; (\d+) unsupported; (\d+) decks failed to "
                           r"resolve; (\d+) of \d+ decks play only supported cards", line):
            res.update(cards=int(m[1]), decks=int(m[2]), unsupported_cards=int(m[3]), failed_decks=int(m[4]),
                       supported_decks=int(m[5]))
    if "decks" not in res:
        raise MtgKernelError(f"coverage: no summary line in its output: {text[-2000:]}")
    return res


def check_rows(games: list[dict], legs: int, pairs: int) -> None:
    """The games.jsonl contract DraftZero's paired scoring reads."""
    if len(games) != legs * pairs:
        raise MtgKernelError(f"{len(games)} rows, expected {legs * pairs}")
    for i, g in enumerate(games):
        missing = ROW_KEYS - g.keys()
        if missing:
            raise MtgKernelError(f"row {i} lacks {sorted(missing)}")
        if g["game"] != i or g["result"] not in RESULTS or sorted(g["side"]) != ["A", "B"]:
            raise MtgKernelError(f"row {i} is malformed: {g}")


def write_fixture_pool(out: Path) -> tuple[Path, Path]:
    """The two original FDN fixtures as gorge deck files plus a pool.tsv, both in the train split."""
    decks = out / "decks"
    decks.mkdir(parents=True, exist_ok=True)
    rows = []
    for stem in FIXTURE_DECKS:
        imported = parse_dck((FIXTURES / f"{stem}.dck").read_text())
        counts: dict[str, int] = {}
        for entry in imported.mainboard:
            counts[entry.name] = counts.get(entry.name, 0) + entry.count
        cards = [{"name": n, "count": c} for n, c in counts.items()]
        (decks / f"{stem}.json").write_text(json.dumps({"name": stem, "format": "limited", "cards": cards}))
        rows.append(f"{stem}\ttrain\t{stem.rsplit('_', 1)[1]}\t{stem.rsplit('_', 1)[1]}\t{sum(counts.values())}")
    pool = out / "pool.tsv"
    pool.write_text("deck\tsplit\tcolors\tmain_colors\tcards\n" + "".join(r + "\n" for r in rows))
    return decks, pool


def smoke(binary: Path) -> dict:
    """Coverage, a random match, a fair-search match and an exact replay on the fixture pool."""
    with tempfile.TemporaryDirectory(prefix="mkmatch-") as tmp:
        decks, pool = write_fixture_pool(Path(tmp))
        kernel = MtgKernel(binary, decks, pool, max_decisions=4000)
        version = kernel.version()
        if version.get("schema") != SCHEMA:
            raise MtgKernelError(f"unexpected version record {version}")
        cov = kernel.coverage()
        if cov["supported_decks"] != 2 or cov["unsupported_cards"] != 0:
            raise MtgKernelError(f"fixture coverage {cov}")
        report = {"version": version, "coverage": {k: cov[k] for k in ("cards", "decks", "supported_decks")}}
        for name, a, b, pairs in (("random", "random", "pimc:sims=1:depth=0:peek", 2),
                                  ("fair", "pimc:sims=4:depth=8:closedlist", "random", 1)):
            first = kernel.play(a, b, pairs=pairs, seed=11, out_dir=Path(tmp) / name, workers=2)
            again = kernel.play(a, b, pairs=pairs, seed=11, out_dir=Path(tmp) / f"{name}-again", workers=1)
            check_rows(first.games, 2, pairs)
            strip = lambda rows: [{k: v for k, v in r.items() if k != "ms"} for r in rows]
            if strip(first.games) != strip(again.games):
                raise MtgKernelError(f"{name}: replay with another worker count differs")
            # An engine refusal in natural play is an engine coverage gap (DraftZero drops the pair);
            # any other error is this driver's own failure.
            errors = [g.get("error", "") for g in first.games if g["result"] == "error"]
            own = [e for e in errors if not e.startswith("fail_closed:")]
            if own:
                raise MtgKernelError(f"{name}: driver errors {own}")
            decisions = sum(g.get("decisions", 0) for g in first.games)
            if "belief_overlap" not in first.summary or "playable_decks_sha256" not in first.summary:
                raise MtgKernelError(f"{name}: summary lacks belief/deal provenance")
            report[name] = {"games": len(first.games), "results": [g["result"] for g in first.games],
                            "turns": [g["turns"] for g in first.games], "decisions": decisions,
                            "engine_refusals": errors, "wall_s": first.summary["wall_s"],
                            "rollout_errors": sum(g.get("rollout_errors", 0) for g in first.games),
                            "belief_overlap": first.summary["belief_overlap"],
                            "decisions_per_s": decisions / first.summary["wall_s"] if first.summary["wall_s"] else 0,
                            "games_per_hour": first.summary["games_per_hour"]}
        return report


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    fixture = sub.add_parser("fixture-pool")
    fixture.add_argument("out", type=Path)
    check = sub.add_parser("smoke")
    check.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.command == "fixture-pool":
        decks, pool = write_fixture_pool(args.out)
        print(json.dumps({"decks": str(decks), "pool": str(pool)}))
    else:
        print(json.dumps(smoke(args.binary), indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
