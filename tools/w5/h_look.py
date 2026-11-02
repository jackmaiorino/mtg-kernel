"""W5 H looks for Linux-trained endpoints on Windows hosts (collab LANES/lead/W5-DESIGN.md v1.3 sections 5, 7.4
and row E12).

The H panel is frozen as W4 Step C's namespace a shape (`w4c-panel-20261031a`: 81 ordered deck pairs, 12 seed
families, 2 seats, 1,944 games, raw policy, search off) against Step C's reference H, on Step C's evaluator pin.
Endpoints are trained on the Linux class and played here on a Windows host, so a look is admitted only with:
- the endpoint record (`w5-endpoint/v1`): chain, arm, look, update, ledger value, the checkpoint and its evaluator
  source by hash, and the training host class (Linux);
- the frozen panel definition (`w5-h-panel/v1`): namespace, opponent H source, decks, the evaluator's and the
  same build's census hashes, shards;
- a passing logit parity receipt (`logit_parity.py compare`) for this checkpoint's bytes, from the endpoint's
  Linux class to this host's Windows class, whose Windows dump ran on the panel build's census binary.

  h_look.py plan --endpoint ENDPOINT.json --panel-def H-PANEL.json --parity RECEIPT.json --out DIR
  h_look.py run --look DIR/look.json --evaluator EXE        qualify (fresh receipt) and run, in the panel runner's
                                                            own host reservation (nine_deck_public_panel_v1 launch)
  h_look.py rows --look DIR/look.json                       rows file and the look receipt; rows are not read here

The plan, qualification, run, recovery and rows are tools/spy_reset/panel.py and
python/tools/nine_deck_public_panel_v1.py unchanged; this driver adds the W5 admission and receipts only.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
sys.path.insert(0, str(HERE))
from logit_parity import host_class, sha256_file  # noqa: E402

ENDPOINT_SCHEMA = "w5-endpoint/v1"
PANEL_SCHEMA = "w5-h-panel/v1"
LOOK_SCHEMA = "w5-h-look/v1"
NAMESPACE = "w4c-panel-20261031a"
LOOKS = ("50pct", "endpoint")
RUNNER = REPO / "python" / "tools" / "nine_deck_public_panel_v1.py"


class LookError(Exception):
    pass


def require(ok: bool, message: str) -> None:
    if not ok:
        raise LookError(message)


def checked(ref: dict, what: str) -> Path:
    path = Path(ref["path"])
    require(path.exists() and sha256_file(path) == ref["sha256"], f"{what} differs from its pin: {path}")
    return path


def admit(endpoint: dict, panel: dict, parity: dict, this_host: str) -> None:
    """Every W5 condition for playing this endpoint's H look on this host, before any plan is written."""
    require(endpoint.get("schema") == ENDPOINT_SCHEMA, "not a W5 endpoint record")
    require(panel.get("schema") == PANEL_SCHEMA, "not a W5 H panel definition")
    require(panel["namespace"] == NAMESPACE, "the H panel is frozen on Step C's namespace a")
    require(endpoint["look"] in LOOKS, "look must be 50pct or endpoint")
    require(endpoint["training_host_class"].startswith("linux-"), "the endpoint was not trained on a Linux class")
    require(this_host.startswith("windows-"), "H looks run on Windows hosts")
    require(parity.get("schema") == "w5-logit-parity/v1" and parity["pass"], "no passing logit parity receipt")
    require(parity["linux"]["host_class"] == endpoint["training_host_class"],
            "the parity receipt's Linux side is not the endpoint's training class")
    require(parity["windows"]["host_class"] == this_host, "the parity receipt is for another Windows host class")
    require(endpoint["checkpoint"]["sha256"] in parity["checkpoint_pins"],
            "the parity receipt does not cover this checkpoint's bytes")
    require(parity["windows"]["binary"]["sha256"] == panel["census"]["sha256"],
            "the parity receipt's Windows dump did not run on the panel build")


def plan(endpoint_path: Path, panel_path: Path, parity_path: Path, out: Path) -> dict:
    endpoint = json.loads(endpoint_path.read_text(encoding="utf-8"))
    panel = json.loads(panel_path.read_text(encoding="utf-8"))
    parity = json.loads(parity_path.read_text(encoding="utf-8"))
    this_host = host_class()
    admit(endpoint, panel, parity, this_host)
    checked(endpoint["checkpoint"], "checkpoint")
    source = json.loads(checked(endpoint["source"], "endpoint source").read_text(encoding="utf-8"))
    opponent = json.loads(checked(panel["opponent"], "opponent H").read_text(encoding="utf-8"))
    decks = json.loads(checked(panel["decks"], "decks").read_text(encoding="utf-8"))
    sys.path.insert(0, str(REPO / "tools" / "spy_reset"))
    import panel as panel_tool
    look_id = f"w5-{endpoint['chain']}-{endpoint['arm']}-{endpoint['look']}"
    require(not (out / "plan.json").exists(), f"{out} already holds a plan")
    panel_tool.build(look_id, source, "H", opponent, decks, out, int(panel["shards"]), namespace=NAMESPACE,
                     annotations={"w5_look": {"endpoint_sha256": sha256_file(endpoint_path),
                                              "panel_def_sha256": sha256_file(panel_path),
                                              "parity_sha256": sha256_file(parity_path)}})
    look = {"schema": LOOK_SCHEMA, "look_id": look_id, "endpoint": endpoint, "panel_def": panel,
            "parity": {"path": str(parity_path), "sha256": sha256_file(parity_path),
                       "max_abs_logit_diff": parity["max_abs_logit_diff"], "states": parity["states"]},
            "host_class": this_host, "plan": {"path": str(out / "plan.json"),
                                              "sha256": sha256_file(out / "plan.json")},
            "driver_sha256": sha256_file(__file__)}
    (out / "look.json").write_text(json.dumps(look, indent=2) + "\n", encoding="utf-8", newline="\n")
    return look


def load_look(path: Path) -> dict:
    look = json.loads(Path(path).read_text(encoding="utf-8"))
    require(look.get("schema") == LOOK_SCHEMA, "not a W5 look")
    checked(look["plan"], "look plan")
    require(host_class() == look["host_class"], "the look was planned on another host class")
    return look


def runner(*args: str) -> None:
    done = subprocess.run([sys.executable, str(RUNNER), *args])
    require(done.returncode == 0, f"panel runner {args[0]} {args[1] if len(args) > 1 else ''} failed")


def run(look_path: Path, evaluator: Path) -> None:
    look = load_look(look_path)
    require(sha256_file(evaluator) == look["panel_def"]["evaluator"]["sha256"], "evaluator is not the panel pin")
    root = Path(look["plan"]["path"]).parent
    receipt = root / "qualification" / "receipt.json"
    if not receipt.exists():
        runner("launch", "qualify", "--plan", look["plan"]["path"], "--sample", "16", "--workers", "1,2,4,8",
               "--root", str(root / "qualification"), "--executable", str(evaluator))
    runner("launch", "run", "--plan", look["plan"]["path"], "--receipt", str(receipt), "--executable",
           str(evaluator))


def rows(look_path: Path) -> dict:
    look = load_look(look_path)
    root = Path(look["plan"]["path"]).parent
    out = root / "rows.jsonl"
    runner("rows", "--plan", look["plan"]["path"], "--out", str(out))
    receipt = {"schema": "w5-h-look-receipt/v1", "look_id": look["look_id"],
               "look_sha256": sha256_file(look_path), "rows": {"path": str(out), "sha256": sha256_file(out)},
               "host_class": look["host_class"], "training_host_class": look["endpoint"]["training_host_class"],
               "evaluator_sha256": look["panel_def"]["evaluator"]["sha256"], "parity": look["parity"]}
    (root / "look-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8", newline="\n")
    return receipt


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("plan")
    for name in ("--endpoint", "--panel-def", "--parity", "--out"):
        p.add_argument(name, type=Path, required=True)
    p = sub.add_parser("run")
    p.add_argument("--look", type=Path, required=True)
    p.add_argument("--evaluator", type=Path, required=True)
    p = sub.add_parser("rows")
    p.add_argument("--look", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == "plan":
            result = plan(args.endpoint, args.panel_def, args.parity, args.out)
        elif args.command == "run":
            run(args.look, args.evaluator)
            result = {"ran": str(args.look)}
        else:
            result = rows(args.look)
    except LookError as exc:
        print(f"refused: {exc}", file=sys.stderr)
        return 2
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
