"""Archive reset pilot (collab LANES/spy-archive-reset-pilot-20261012): training configs.

Arms per lineage, all from the parent's complete checkpoint with the reset pilot's frozen
162-update schedule (spy-reset-pilot-v1 unit seeds, opponents and rules):
  O  every unit a full ordinary game (identical to the reset pilot's O);
  U  every Spy-focal unit (own deck 4, all j, both seats) resets into a plain parent
     guide of the reset pilot's bank (search-free history), cut at the cycled quantile;
  S  the same slots reset into a 2,048,000-budget search game from the Spy discovery
     archive (ordinary opening, matching opposing deck and seat), chosen by hash among
     the archive games completed at the freeze, all outcomes, cut at the same quantile.
Non-Spy units are full games in every arm. Learning uses fresh suffixes only (the
trainer excludes prefix rows); archive actions and outcomes are never labels.

CLI:
  config  --lineage r1 --arm O|U|S --decks D --pbank BANK --archive ARCHIVE.jsonl
          --parent P --a48 A --workers W --output-directory DIR --out CFG [--updates N]
ARCHIVE.jsonl holds one replay row per archive game (s4a-budget-replay output with
starts) plus its case fields; it is frozen before training.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from fractions import Fraction
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import pilot  # noqa: E402

NS = "spy-archive-reset-pilot-v1"
SPY = pilot.SPY


def h(*parts) -> str:
    return hashlib.sha256(json.dumps(list(parts), separators=(",", ":")).encode()).hexdigest()


def load_pbank(path: Path, lineage: str) -> dict:
    rows = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip():
            r = json.loads(line)
            if r.get("kind") == "bank_source" and r.get("lineage") == lineage and r["own"] == SPY:
                rows[(r["other"], r["seat"])] = r
    return rows


def load_archive(path: Path, lineage: str) -> dict:
    """(other, seat) -> archive games in stable id order."""
    cells = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip():
            r = json.loads(line)
            if r["lineage"] == lineage and r.get("usable"):
                cells.setdefault((r["other"], r["seat"]), []).append(r)
    for v in cells.values():
        v.sort(key=lambda r: r["id"])
    return cells


def archive_cut(game: dict, q: Fraction) -> dict:
    starts = game["starts"]
    start = starts[pilot.cut_index(q, len(starts))]
    prefix = game["actions"][: start["step"]]
    return {"guide": f"{game['id']}/A/q{q}", "game_seed": game["seed"], "prefix": prefix,
            "cut_physical_decision_id": start["physical_decision_id"],
            "cut_observation_sha256": start["obs_hash"], "cut_menu_sha256": start["menu_hash"],
            "cut_state_hash": start["state_hash"]}


def iterations(lineage, arm, decks, pbank, archive):
    plan = pilot.schedule(lineage)
    uses: dict = {}
    out = []
    summary = {"full": 0, "reset": 0, "missing_cell_full": 0, "unique_cuts": set(), "repeated_cuts": 0,
               "unique_sources": set(), "archive_sources_with_sequence": set()}
    for update in plan["updates"]:
        items = []
        for e in update:
            reset, start = None, e["starting_player"]
            if arm != "O" and e["own"] == SPY:
                cell = (e["other"], e["seat"])
                n = uses.setdefault(cell, 0)
                uses[cell] = n + 1
                q = pilot.Q[n % len(pilot.Q)]
                if arm == "U":
                    row = pbank.get(cell)
                    if row is not None and row["usable"]:
                        reset, start = pilot.cut_of(row, "P", q), row["starting_player"]
                else:
                    games = archive.get(cell, [])
                    if games:
                        g = min(games, key=lambda r: h(NS, "pick", lineage, *cell, n, r["id"]))
                        reset, start = archive_cut(g, q), g["starting_player"]
                        if (g.get("sequence") or {}).get("complete"):
                            summary["archive_sources_with_sequence"].add(g["id"])
                if reset is None:
                    summary["missing_cell_full"] += 1
            if reset is None:
                summary["full"] += 1
            else:
                summary["reset"] += 1
                k = (reset["guide"].rsplit("/q", 1)[0], len(reset["prefix"]))
                summary["repeated_cuts"] += k in summary["unique_cuts"]
                summary["unique_cuts"].add(k)
                summary["unique_sources"].add(reset["guide"].split("/")[0])
            items.append({"episode": pilot.episode(lineage, decks, e, reset, start),
                          "opponent": pilot.opponent_assignment(e["opponent"])})
        out.append({"episodes": items})
    for k in ("unique_cuts", "unique_sources", "archive_sources_with_sequence"):
        summary[k] = len(summary[k])
    return out, summary


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    c = sub.add_parser("config")
    c.add_argument("--lineage", required=True)
    c.add_argument("--arm", required=True, choices=("O", "U", "S"))
    c.add_argument("--decks", type=Path, required=True)
    c.add_argument("--pbank", type=Path, required=True)
    c.add_argument("--archive", type=Path, required=True)
    c.add_argument("--parent", type=Path, required=True)
    c.add_argument("--a48", type=Path, required=True)
    c.add_argument("--workers", type=int, required=True)
    c.add_argument("--output-directory", required=True)
    c.add_argument("--updates", type=int, default=pilot.UPDATES)
    c.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    decks = json.loads(a.decks.read_text())
    its, summary = iterations(a.lineage, a.arm, decks, load_pbank(a.pbank, a.lineage),
                              load_archive(a.archive, a.lineage))
    ndb = pilot.ndb
    config = {"schema": ndb.SCHEMA, "initial_source": json.loads(a.parent.read_text()),
              "opponents": [{"id": pilot.FIXED_OPPONENT_ID, "source": json.loads(a.a48.read_text())}],
              "iterations": its[: a.updates], **json.loads(json.dumps(ndb.FROZEN)),
              "update_backend": {"kind": "cpu"}, "collection_workers": a.workers,
              "preparation_workers": a.workers, "output_directory": a.output_directory}
    print(json.dumps({**pilot.write_json(a.out, config), "summary": summary}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
