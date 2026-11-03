"""W5 runtime receipt under amendment A1 (collab LANES/lead/w5/RULINGS-r0.md, "Amendment A1" and "A1 correction").

The raw parity checks of `runtime_parity.py` run unchanged and are reported as they are. Exactly three named
differences may then be accepted as declared exceptions, each only under its stated conditions; any other failed
raw check fails the receipt. Raw failures and accepted exceptions are kept in separate fields.

Accepted exceptions (A1):
- `spellbench-neutral-choices`: covers the raw check `neutral_choices_equal`, when the board declares exactly
  `:neutral-v2` and the A1 census result (sha256 below) is supplied.
- `graveyard-order-profile`: covers `training_rules_profile_is_default`, when every training episode uses exactly
  `resolution-boundary-v1`, the board applies no profile, and no card in any training or board deck has Delve
  (the only reader of graveyard order; `data/cards_v1.json` mechanics at each revision).
- `buffered-completion-selection`: covers `selection_equal`, when the board wrapper selects buffered groups by
  `deterministic-completion-logprob/v1` over `mtg-kernel-spellbench-completion-proposals/v1`.

Further raw checks added here (never excepted): the H panels and the S0 recording run the training runtime.

Recorded (A1 condition 1): the amendment (file, sha256, commit); the actual runtimes of training, search (the
w5-xact search runs inside the collection session), H panels and S0; the frozen configs and the pool hash; the
engine, wrapper and scorer identities, with the engine and wrapper bound to the board run's `engine_files`; the
board snapshot, with the run bound to its sources; each exception's evidence.

  runtime_receipt.py --training-config CFG.json [...] --board-run RUN_DIR --board-snapshot SNAPSHOT_DIR
      --repo MTG_KERNEL_REPO [--training-revision REV] --amendment RULINGS-r0.md --amendment-commit SHA
      --census w5-neutral-census.json --h-plan PLAN.json [...] --s0-config CFG.json [...]
      --engine-binary EXE --wrapper kernel_engine_v2.py --scorer EXE --bot kernel_flat_bot.py
      --collab-repo COLLAB_CHECKOUT [--out RECEIPT.json]

Bindings (all must hold): the engine binary and the wrapper are the board run's `agent_bridge_v1.exe` and
`kernel_engine_v2.py` by `engine_files` role; the board run is `pauper-ladder-v2` (a v1 receipt is informative
only); the run is a source of the snapshot; the census is A1's; the amendment file is `RULINGS-r0.md` at the given
collab commit. The raw receipt is recomputed every time and checked against the supplied inputs. H runtimes come from
every match of every pinned H request. The scorer is recorded (the board run records no scorer hash).
"""
from __future__ import annotations

import argparse
import ast
import contextlib
import hashlib
import io
import json
import subprocess
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import runtime_parity as rp  # noqa: E402

SCHEMA = "w5-runtime-receipt-a1/v1"
CENSUS_SHA256 = "811d9af07e8da9365089b30f2e9352336c747b000a502335e22fb3d776ac88e1"
NEUTRAL_ID = ":neutral-v2"
PROFILE = "resolution-boundary-v1"
PROPOSALS = "mtg-kernel-spellbench-completion-proposals/v1"
MAPPING = "deterministic-completion-logprob/v1"
AMENDMENT_MARKERS = ("## Amendment A1: runtime parity", "### A1 correction")
AMENDMENT_PATH = "LANES/lead/w5/RULINGS-r0.md"
BOARD_V2 = "pauper-ladder-v2"
STEPWISE = "sampled-wide-v1"
EXCEPTIONS = {
    "spellbench-neutral-choices": "neutral_choices_equal",
    "graveyard-order-profile": "training_rules_profile_is_default",
    "buffered-completion-selection": "selection_equal",
}


class ReceiptError(Exception):
    pass


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def runtime_profile(configs: list[dict]) -> dict[str, int]:
    out = Counter()
    for config in configs:
        for ep in rp.episodes(config):
            out["default" if ep.get("runtime_rules") is None else str(ep["runtime_rules"])] += 1
    return dict(out)


def plan_runtime(plan: dict) -> str:
    return "default" if plan.get("runtime_rules") is None else str(plan["runtime_rules"])


def h_plan_runtimes(plan_path: Path) -> list[str]:
    """The runtime of every match an H plan actually executes: each request verified by its pin and read."""
    plan = json.loads(Path(plan_path).read_text(encoding="utf-8"))
    runtimes = [plan_runtime(plan)]
    for item in plan.get("requests", []):
        ref = item["request"]
        data = Path(ref["path"]).read_bytes()
        if sha256_bytes(data) != ref["sha256"]:
            raise ReceiptError(f"H request {ref['path']} differs from its pin")
        for match in json.loads(data)["matches"]:
            runtimes.append(plan_runtime(match["config"]))
    if len(runtimes) == 1:
        raise ReceiptError(f"H plan {plan_path} executes no requests")
    return runtimes


def delve_cards(cards: list[dict], deck_ids) -> list[str]:
    return sorted({cards[i]["name"] for i in deck_ids if "delve" in cards[i].get("mechanics", [])})


def module_constants(source: str) -> dict[str, str]:
    """Module-level string constants (`NAME = "..."`), read from the syntax tree, so comments and other strings
    cannot stand in for them."""
    out = {}
    for node in ast.parse(source).body:
        if isinstance(node, ast.Assign) and isinstance(node.value, ast.Constant) and isinstance(node.value.value, str):
            for target in node.targets:
                if isinstance(target, ast.Name):
                    out[target.id] = node.value.value
    return out


def selection_facts(wrapper_source: str, bot_source: str) -> dict:
    """How the deployed board selects: the wrapper's buffered groups and the bot's completion mapping."""
    wrapper, bot = module_constants(wrapper_source), module_constants(bot_source)
    return {"buffered_completion_proposals": wrapper.get("PROPOSALS") == PROPOSALS,
            "wrapper_proposals_constant": wrapper.get("PROPOSALS"),
            "bot_reads_completion_proposals": bot.get("PROPOSAL_SCHEMA") == PROPOSALS,
            "bot_completion_mapping": bot.get("PROPOSAL_MAPPING"),
            "bot_stepwise_sampler": bot.get("SAMPLED")}


def evaluate(raw: dict, training_profile: dict, h_runtimes: list[str], s0_profile: dict, board_manifest: dict,
             selection: dict, delve: dict, census_ok: bool) -> dict:
    """Raw checks (as computed) plus A1's three exceptions, each with its conditions."""
    checks = dict(raw["checks"])
    training_runtimes = set(training_profile)
    # Equal only for a known training-style deployment: no buffered groups and the stepwise sampler training uses.
    checks["selection_equal"] = (selection.get("wrapper_proposals_constant") is None
                                 and not selection["buffered_completion_proposals"]
                                 and selection.get("bot_stepwise_sampler") == STEPWISE)
    checks["h_panel_runtime_equals_training"] = bool(h_runtimes) and set(h_runtimes) == training_runtimes
    checks["s0_runtime_equals_training"] = bool(s0_profile) and set(s0_profile) == training_runtimes
    conditions = {
        "spellbench-neutral-choices": {
            "board_declares_neutral_v2": board_manifest["engine"].get("rules_snapshot_id", "").endswith(NEUTRAL_ID),
            "census_evidence_matches": census_ok},
        "graveyard-order-profile": {
            "training_profile_exactly_resolution_boundary": training_runtimes == {PROFILE},
            "board_applies_no_profile": raw["checks"]["board_rules_profile_is_default"],
            "no_delve_card_in_any_deck": not delve["training"] and not delve["board"]},
        "buffered-completion-selection": {
            "wrapper_buffers_with_completion_proposals": selection["buffered_completion_proposals"],
            "bot_reads_completion_proposals": selection["bot_reads_completion_proposals"],
            "bot_maps_by_completion_logprob": selection["bot_completion_mapping"] == MAPPING},
    }
    raw_failures = sorted(k for k, v in checks.items() if not v)
    accepted, refused = [], []
    for name, check in EXCEPTIONS.items():
        if check in raw_failures:
            entry = {"exception": name, "covers": check, "conditions": conditions[name]}
            (accepted if all(conditions[name].values()) else refused).append(entry)
    covered = {e["covers"] for e in accepted}
    unexcepted = [k for k in raw_failures if k not in covered]
    return {"raw_checks": checks, "raw_failures": raw_failures, "accepted_exceptions": accepted,
            "refused_exceptions": refused, "unexcepted_failures": unexcepted}


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--training-config", type=Path, action="append", required=True)
    parser.add_argument("--board-run", type=Path, required=True)
    parser.add_argument("--board-snapshot", type=Path, required=True)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--training-revision", default="HEAD")
    parser.add_argument("--amendment", type=Path, required=True)
    parser.add_argument("--amendment-commit", required=True)
    parser.add_argument("--census", type=Path, required=True)
    parser.add_argument("--h-plan", type=Path, action="append", required=True)
    parser.add_argument("--s0-config", type=Path, action="append", required=True)
    parser.add_argument("--engine-binary", type=Path, required=True)
    parser.add_argument("--wrapper", type=Path, required=True)
    parser.add_argument("--scorer", type=Path, required=True)
    parser.add_argument("--bot", type=Path, required=True, help="the deployed bot (kernel_flat_bot.py)")
    parser.add_argument("--collab-repo", type=Path, required=True, help="a collab checkout holding the amendment")
    parser.add_argument("--out", type=Path)
    args = parser.parse_args(argv)
    try:
        raw_receipt_path = (args.out or Path("runtime-parity-raw.json")).with_suffix(".raw.json")
        raw_receipt_path.unlink(missing_ok=True)  # never reuse an earlier raw receipt
        config_args = [x for p in args.training_config for x in ("--training-config", str(p))]
        with contextlib.redirect_stdout(io.StringIO()):  # the raw receipt goes to its own file
            code = rp.main([*config_args, "--board-run", str(args.board_run), "--repo", str(args.repo),
                            "--training-revision", args.training_revision, "--out", str(raw_receipt_path)])
        if not raw_receipt_path.exists():
            raise ReceiptError(f"the raw parity receipt could not be computed (exit {code})")
        raw = json.loads(raw_receipt_path.read_text(encoding="utf-8"))
        supplied = [sha256_bytes(p.read_bytes()) for p in args.training_config]
        if ([c["sha256"] for c in raw["inputs"]["training_configs"]] != supplied
                or raw["inputs"]["board_manifest_sha256"] != sha256_bytes((args.board_run / "manifest.json").read_bytes())):
            raise ReceiptError("the raw receipt was computed from other inputs")
        amendment = args.amendment.read_bytes()
        amendment_text = amendment.decode("utf-8")
        missing = [m for m in AMENDMENT_MARKERS if m not in amendment_text]
        if missing:
            raise ReceiptError(f"the amendment file lacks {missing}")
        # The committed bytes against the supplied bytes, line endings normalized explicitly (a CRLF working copy
        # of the committed LF file matches); no checkout filter takes part.
        committed = subprocess.run(["git", "-C", str(args.collab_repo), "show",
                                    f"{args.amendment_commit}:{AMENDMENT_PATH}"], capture_output=True)
        if committed.returncode != 0 or (committed.stdout.replace(b"\r\n", b"\n")
                                         != amendment.replace(b"\r\n", b"\n")):
            raise ReceiptError(f"the amendment file is not {AMENDMENT_PATH} at commit {args.amendment_commit}")
        census_sha = sha256_bytes(args.census.read_bytes())
        configs = [json.loads(p.read_bytes()) for p in args.training_config]
        s0_configs = [json.loads(p.read_bytes()) for p in args.s0_config]
        h_plans = [json.loads(p.read_bytes()) for p in args.h_plan]
        h_runtimes = [r for p in args.h_plan for r in h_plan_runtimes(p)]
        bot_raw = args.bot.read_bytes()
        manifest_raw = (args.board_run / "manifest.json").read_bytes()
        manifest = json.loads(manifest_raw)
        snapshot_raw = (args.board_snapshot / "manifest.json").read_bytes()
        snapshot = json.loads(snapshot_raw)
        wrapper_raw = args.wrapper.read_bytes()
        engine_sha, wrapper_sha, scorer_sha = (sha256_bytes(p.read_bytes()) for p in
                                               (args.engine_binary, args.wrapper, args.scorer))
        engine_files = {f.get("file_name"): f["sha256"] for f in manifest.get("engine_files", [])}
        training_rev = raw["inputs"]["training_revision"]
        board_rev = raw["inputs"]["board_revision"]
        cards_t = json.loads(rp.git_show(args.repo, training_rev, "data/cards_v1.json"))["cards"]
        cards_b = json.loads(rp.git_show(args.repo, board_rev, "data/cards_v1.json"))["cards"]
        decks_b = json.loads(rp.git_show(args.repo, board_rev, "data/runtime_decks_v1.json"))
        pool = [d["catalog_id"] for d in json.loads((args.board_run / "config.json").read_bytes())["deck_pool"]]
        training_ids = [i for c in configs for ep in rp.episodes(c) for d in ep["selected"] for i in d["mainboard"]]
        board_ids = [r["card_id"] for d in decks_b["decks"] if d["id"] in pool for r in d["materialized_mainboard"]]
        delve = {"training": delve_cards(cards_t, training_ids), "board": delve_cards(cards_b, board_ids)}
        training_profile = runtime_profile(configs)
        selection = selection_facts(wrapper_raw.decode("utf-8"), bot_raw.decode("utf-8"))
        result = evaluate(raw, training_profile, h_runtimes, runtime_profile(s0_configs), manifest, selection, delve,
                          census_sha == CENSUS_SHA256)
    except (ReceiptError, rp.ParityError, KeyError, ValueError, SyntaxError) as exc:
        print(f"error: {exc!r}", file=sys.stderr)
        return 1
    bindings = {
        "engine_binary_is_the_board_bridge": engine_files.get("agent_bridge_v1.exe", engine_files.get("agent_bridge_v1"))
        == engine_sha,
        "wrapper_is_the_board_wrapper": engine_files.get("kernel_engine_v2.py") == wrapper_sha,
        "board_is_v2": manifest.get("run", {}).get("benchmark_id") == BOARD_V2,
        "run_is_a_snapshot_source": sha256_bytes(manifest_raw) in {s.get("manifest_sha256")
                                                                   for s in snapshot.get("sources", [])},
        "census_hash_is_a1s": census_sha == CENSUS_SHA256,
    }
    pool_hash = sha256_bytes(json.dumps(sorted({d["label"]: d["mainboard"] for c in configs for ep in rp.episodes(c)
                                                for d in ep["selected"]}.items()),
                                        separators=(",", ":")).encode())
    search = [{"config": str(p), "ex1": c.get("ex1")} for p, c in zip(args.training_config, configs)]
    receipt = {
        "schema": SCHEMA,
        "pass": not result["unexcepted_failures"] and not result["refused_exceptions"] and all(bindings.values()),
        **result, "bindings": bindings,
        "amendment": {"path": str(args.amendment), "sha256": sha256_bytes(amendment), "commit": args.amendment_commit,
                      "sections": list(AMENDMENT_MARKERS)},
        "runtimes": {
            "training": training_profile,
            "search": {"runs_in": "the collection session (w5-xact-v1 hook), so it shares the training runtime",
                       "configs": search},
            "h_panels": [{"plan": str(p), "sha256": sha256_bytes(p.read_bytes()), "runtime_rules": plan_runtime(h),
                          "seed_namespace": h.get("seed_namespace"),
                          "match_runtimes": dict(Counter(h_plan_runtimes(p)[1:]))}
                         for p, h in zip(args.h_plan, h_plans)],
            "s0": runtime_profile(s0_configs)},
        "configs": {"training": [{"path": str(p), "sha256": sha256_bytes(p.read_bytes())} for p in args.training_config],
                    "s0": [{"path": str(p), "sha256": sha256_bytes(p.read_bytes())} for p in args.s0_config],
                    "training_pool_sha256": pool_hash,
                    "board_runtime_decks_sha256": raw["inputs"]["board_runtime_decks_sha256"]},
        "identities": {"engine_binary": {"path": str(args.engine_binary), "sha256": engine_sha},
                       "wrapper": {"path": str(args.wrapper), "sha256": wrapper_sha},
                       "scorer": {"path": str(args.scorer), "sha256": scorer_sha,
                                  "binding": "recorded only: the board run records no scorer hash"},
                       "bot": {"path": str(args.bot), "sha256": sha256_bytes(bot_raw),
                               "binding": "recorded only: the board run records no bot hash"},
                       "board_engine": manifest.get("engine"), "board_engine_files": manifest.get("engine_files")},
        "board_snapshot": {"path": str(args.board_snapshot), "manifest_sha256": sha256_bytes(snapshot_raw),
                           "run_manifest_sha256": sha256_bytes(manifest_raw)},
        "exception_evidence": {"census": {"path": str(args.census), "sha256": census_sha},
                               "delve_cards": delve, "selection": selection},
        "raw_receipt": {"path": str(raw_receipt_path), "sha256": sha256_bytes(raw_receipt_path.read_bytes())},
        "limitation": ("A1 condition 3: whether the mismatch affects the arms differently is unmeasured; the Elves and "
                       "Spy partition decisions are plausibly outcome-equivalent, unverified."),
    }
    text = json.dumps(receipt, indent=2) + "\n"
    if args.out:
        args.out.write_text(text, encoding="utf-8", newline="\n")
    print(text, end="")
    return 0 if receipt["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
