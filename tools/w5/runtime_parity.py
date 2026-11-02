"""W5 runtime parity receipt (collab LANES/lead/W5-DESIGN.md v1.3 section 2 "Runtime", lead ruling 7; E11).

The training runtime must equal the board's: the Spellbench v2 adapter plays with mulligans off, as the bridge
declares, and W5 trains the same way. This tool compares, from their declarations and data, never from an outcome:

- **Mulligans:** the board's resolved `information_rules.rules.mulligan` and the engine's
  `engine_profile.rules_supported.mulligan` (run manifest) against every training episode's `opening` and
  `mulligan_modes` (both absent: the keep-seven deal, mulligans off).
- **Runtime rules:** every training episode's `runtime_rules` absent (the engine's default rules), and the board
  bridge source at the board's engine revision applying no runtime-rules profile and refusing the opt-in
  semantics (London mulligans, attack targets, Foundations combat).
- **Starting player:** the board assigns it (`host_assigned`, no choice); every training episode fixes it.
- **Decks:** each of the board's pool decks, as the board engine materializes it (`data/runtime_decks_v1.json`
  at the board revision), equals exactly one training deck label's mainboard (card ids named by
  `data/cards_v1.json` at the training revision), as a multiset of card names. Training decks without a board deck
  are listed (W5: Terror). Every training episode is game 1 (`postboard` false), as the board's Bo1.
- **Recorded beside, not compared:** game limits on both sides, the board extensions and engine identity.

  runtime_parity.py --training-config CFG.json [...] --board-run RUN_DIR --repo MTG_KERNEL_REPO
                    [--training-revision REV] [--out RECEIPT.json]

`--board-run` is a Spellbench run directory on the board (manifest.json and config.json; for v2, Phase A's run).
`--repo` is an mtg-kernel checkout holding both revisions; the board revision is the manifest's
`engine.source_revision`.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from collections import Counter
from pathlib import Path

SCHEMA = "w5-runtime-parity/v1"
BRIDGE = "mtg-kernel/src/agent_bridge_v1.rs"
LOCKSTEP = "mtg-kernel/src/agent_bridge_flat_v4.rs"  # the FlatV4 bot's training-style session copy
OPT_IN_SEMANTICS = ("ChooseLondonMulligan", "ChooseLondonBottom", "ChooseAttackTarget", "ChooseCombatDamageRange")


class ParityError(Exception):
    pass


def git_show(repo: Path, revision: str, path: str) -> bytes:
    out = subprocess.run(["git", "-C", str(repo), "show", f"{revision}:{path}"], capture_output=True)
    if out.returncode != 0:
        raise ParityError(f"git show {revision}:{path} failed: {out.stderr.decode(errors='replace').strip()}")
    return out.stdout


def git_rev(repo: Path, revision: str) -> str:
    out = subprocess.run(["git", "-C", str(repo), "rev-parse", "--verify", f"{revision}^{{commit}}"],
                         capture_output=True, text=True)
    if out.returncode != 0:
        raise ParityError(f"unknown revision {revision}")
    return out.stdout.strip()


def episodes(config: dict):
    for item in config["iterations"]:
        for entry in item["episodes"]:
            yield entry["episode"] if "episode" in entry else entry


def training_side(configs: list[dict], card_names: list[str]) -> dict:
    """Mulligan profile, runtime rules, starting players, Bo1 and decks of every training episode."""
    profiles, rules, starts, postboard, limits = Counter(), Counter(), Counter(), Counter(), Counter()
    decks: dict[str, Counter] = {}
    for config in configs:
        for ep in episodes(config):
            opening, modes = ep.get("opening"), ep.get("mulligan_modes")
            profiles["none (keep seven)" if opening is None and modes is None else f"opening={opening}"] += 1
            rules["default" if ep.get("runtime_rules") is None else str(ep["runtime_rules"])] += 1
            starts[ep["starting_player"]] += 1
            postboard[bool(ep.get("postboard"))] += 1
            limits[(ep["max_physical_decisions"], ep["max_policy_steps"])] += 1
            for deck in ep["selected"]:
                named = Counter(card_names[i] for i in deck["mainboard"])
                if deck["label"] in decks and decks[deck["label"]] != named:
                    raise ParityError(f"training deck {deck['label']} has two different mainboards")
                decks[deck["label"]] = named
    return {"mulligan_profiles": dict(profiles), "runtime_rules": dict(rules),
            "starting_players": {str(k): v for k, v in starts.items()},
            "postboard": {str(k): v for k, v in postboard.items()},
            "limits": [{"max_physical_decisions": a, "max_policy_steps": b, "episodes": n}
                       for (a, b), n in sorted(limits.items())], "decks": decks}


def board_decks(runtime_decks: dict, pool: list[str]) -> dict[str, Counter]:
    by_id = {deck["id"]: deck for deck in runtime_decks["decks"]}
    out = {}
    for deck_id in pool:
        if deck_id not in by_id:
            raise ParityError(f"board deck {deck_id} is not in the board engine's runtime decks")
        out[deck_id] = Counter(row["name"] for row in by_id[deck_id]["materialized_mainboard"])
    return out


def bridge_rules(source: str) -> dict:
    """The bridge's runtime-rules facts at the board revision (static: what the source can do)."""
    return {"applies_runtime_rules_profile": "set_runtime_rules" in source,
            "refuses_opt_in_semantics": all(name in source for name in OPT_IN_SEMANTICS)
            and "semantic outside the Pauper neutral contract" in source}


def compare(training: dict, board_manifest: dict, board_config: dict, board_pool_decks: dict, bridge: dict) -> dict:
    rules = board_manifest["information_rules"]["rules"]
    supported = board_manifest["engine_profile"]["rules_supported"]
    checks = {}
    checks["mulligans_off_on_board"] = rules["mulligan"] == "none" and supported["mulligan"] == ["none"]
    checks["mulligans_off_in_training"] = set(training["mulligan_profiles"]) == {"none (keep seven)"}
    checks["training_rules_profile_is_board_default"] = set(training["runtime_rules"]) == {"default"}
    checks["board_rules_profile_is_default"] = (not bridge["applies_runtime_rules_profile"]
                                        and bridge["refuses_opt_in_semantics"])
    checks["starting_player_assigned"] = (rules["starting_player"] == "host_assigned"
                                          and set(training["starting_players"]) <= {"0", "1"})
    checks["game_one_only"] = set(training["postboard"]) == {"False"}
    matches, unmatched_board = {}, []
    for deck_id, cards in board_pool_decks.items():
        labels = [label for label, named in training["decks"].items() if named == cards]
        if len(labels) == 1:
            matches[deck_id] = labels[0]
        else:
            unmatched_board.append({"deck": deck_id, "training_labels": labels})
    checks["every_board_deck_equals_one_training_deck"] = not unmatched_board
    training_only = sorted(set(training["decks"]) - set(matches.values()))
    return {"checks": checks, "deck_matches": matches, "unmatched_board_decks": unmatched_board,
            "training_only_decks": training_only,
            "recorded": {"board_limits": board_config.get("limits"), "training_limits": training["limits"],
                         "board_extensions": rules.get("extensions"), "board_engine": board_manifest["engine"],
                         "board_engine_defaults": board_manifest["information_rules"].get("engine_defaults")}}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--training-config", type=Path, action="append", required=True)
    parser.add_argument("--board-run", type=Path, required=True)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--training-revision", default="HEAD")
    parser.add_argument("--out", type=Path)
    args = parser.parse_args(argv)
    try:
        raw_configs = [p.read_bytes() for p in args.training_config]
        configs = [json.loads(raw) for raw in raw_configs]
        manifest_raw = (args.board_run / "manifest.json").read_bytes()
        config_raw = (args.board_run / "config.json").read_bytes()
        manifest, board_config = json.loads(manifest_raw), json.loads(config_raw)
        board_rev = git_rev(args.repo, manifest["engine"]["source_revision"])
        training_rev = git_rev(args.repo, args.training_revision)
        cards_raw = git_show(args.repo, training_rev, "data/cards_v1.json")
        card_names = [card["name"] for card in json.loads(cards_raw)["cards"]]
        decks_raw = git_show(args.repo, board_rev, "data/runtime_decks_v1.json")
        bridge_raw = git_show(args.repo, board_rev, BRIDGE)
        try:
            bridge_raw += b"\n" + git_show(args.repo, board_rev, LOCKSTEP)
        except ParityError:
            pass  # a revision without the FlatV4 lockstep: the bridge file alone
        pool = [d["catalog_id"] for d in board_config["deck_pool"]]
        training = training_side(configs, card_names)
        result = compare(training, manifest, board_config, board_decks(json.loads(decks_raw), pool),
                         bridge_rules(bridge_raw.decode("utf-8")))
    except (ParityError, KeyError) as exc:
        print(f"error: {exc!r}", file=sys.stderr)
        return 1
    receipt = {"schema": SCHEMA, "pass": all(result["checks"].values()), **result,
               "training": {k: v for k, v in training.items() if k != "decks"},
               "inputs": {"training_configs": [{"path": str(p), "sha256": sha256(raw)}
                                               for p, raw in zip(args.training_config, raw_configs)],
                          "board_manifest_sha256": sha256(manifest_raw), "board_config_sha256": sha256(config_raw),
                          "board_revision": board_rev, "training_revision": training_rev,
                          "cards_v1_sha256": sha256(cards_raw), "board_runtime_decks_sha256": sha256(decks_raw),
                          "board_bridge_sha256": sha256(bridge_raw)}}
    text = json.dumps(receipt, indent=2) + "\n"
    if args.out:
        args.out.write_text(text, encoding="utf-8", newline="\n")
    print(text, end="")
    return 0 if receipt["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
