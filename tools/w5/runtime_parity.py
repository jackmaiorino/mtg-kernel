"""W5 runtime parity receipt (collab LANES/lead/W5-DESIGN.md v1.3 section 2 "Runtime", lead ruling 7; E11).

The training runtime must equal the board's: the Spellbench v2 adapter plays with mulligans off, as the bridge
declares, and W5 trains the same way. This tool compares, from declarations, data and source, never from an
outcome. Every check must hold for the receipt to pass:

- **Mulligans:** the board run's resolved `information_rules.rules.mulligan` is `none` (and `none` is supported);
  every training episode has no `opening` and no `mulligan_modes` (the keep-seven deal).
- **Runtime rules profile:** every training episode's `runtime_rules` is absent (the engine's default rules), and
  the bridge sources at the board revision never call `set_runtime_rules_v1` (the FlatV4 lockstep resets with the
  default profile). A static source check: it cannot see a default changed elsewhere in the engine.
- **Neutral choices:** the board engine declares Spellbench neutral choices when its `rules_snapshot_id` carries
  `:neutral-`; training sessions (`episode_session_v1`) never enable them, so the two expose different decisions
  (singleton choices) and parity needs both off.
- **Ordinary starts:** no training episode starts from a recorded history (`reset`); every episode is game 1
  (`postboard` false) with two registrations of 60-card mainboards (sideboards at most 15) and `selected` equal to
  `registered`.
- **Starting player:** the board assigns it (`host_assigned`) and swaps seats within each pair; training uses both
  starting seats and the learner both plays first and second.
- **Limits:** training `max_physical_decisions` and `max_policy_steps` equal the board's `max_decisions` and
  `max_steps` (the board's per-seat caps are recorded beside).
- **Decks:** each board pool deck, as the board revision materializes it (`data/runtime_decks_v1.json`: card ids
  and names), equals at least one training label's mainboard by card id and by name (names from
  `data/cards_v1.json` at each side's revision), so native ids and cards agree. Training-only decks are listed
  (W5: Terror).

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
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

SCHEMA = "w5-runtime-parity/v1"
BRIDGE_FILES = ("mtg-kernel/src/agent_bridge_v1.rs", "mtg-kernel/src/agent_bridge_flat_v4.rs")
RULES_CALL = re.compile(r"\bset_runtime_rules_v1\s*\(")
RAW_STRING = re.compile(r'[bc]?r(#*)"')


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


def deck_key(ids, card_names: list[str]) -> Counter:
    """A mainboard as a multiset of (native card id, card name)."""
    return Counter((i, card_names[i]) for i in ids)


def training_side(configs: list[dict], card_names: list[str]) -> dict:
    """Mulligan profile, rules, starts, limits and decks of every training episode."""
    profiles, rules, starts, limits, learner_starts = Counter(), Counter(), Counter(), Counter(), Counter()
    resets = postboard = registered_differs = malformed = 0
    decks: dict[str, Counter] = {}
    for config in configs:
        for ep in episodes(config):
            opening, modes = ep.get("opening"), ep.get("mulligan_modes")
            profiles["none" if opening is None and modes is None else f"opening={opening}"] += 1
            rules["default" if ep.get("runtime_rules") is None else str(ep["runtime_rules"])] += 1
            starts[ep["starting_player"]] += 1
            learner_starts["first" if ep["learner_seat"] == ep["starting_player"] else "second"] += 1
            limits[(ep["max_physical_decisions"], ep["max_policy_steps"])] += 1
            resets += ep.get("reset") is not None
            postboard += bool(ep.get("postboard"))
            malformed += not (len(ep["registered"]) == len(ep["selected"]) == 2 and all(
                len(d["mainboard"]) == 60 and len(d.get("sideboard", [])) <= 15
                for d in ep["registered"] + ep["selected"]))
            for registered, selected in zip(ep["registered"], ep["selected"]):
                registered_differs += (registered["label"], registered["mainboard"]) != (
                    selected["label"], selected["mainboard"])
            for deck in ep["selected"]:
                named = deck_key(deck["mainboard"], card_names)
                if deck["label"] in decks and decks[deck["label"]] != named:
                    raise ParityError(f"training deck {deck['label']} has two different mainboards")
                decks[deck["label"]] = named
    return {"mulligan_profiles": dict(profiles), "runtime_rules": dict(rules),
            "starting_players": {str(k): v for k, v in sorted(starts.items())},
            "learner_plays": dict(learner_starts), "malformed_registrations": malformed,
            "limits": [{"max_physical_decisions": a, "max_policy_steps": b, "episodes": n}
                       for (a, b), n in sorted(limits.items())],
            "reset_episodes": resets, "postboard_episodes": postboard,
            "selected_differs_from_registered": registered_differs, "decks": decks}


def board_decks(runtime_decks: dict, pool: list[str], card_names: list[str]) -> dict[str, Counter]:
    by_id = {deck["id"]: deck for deck in runtime_decks["decks"]}
    out = {}
    for deck_id in pool:
        if deck_id not in by_id:
            raise ParityError(f"board deck {deck_id} is not in the board engine's runtime decks")
        rows = by_id[deck_id]["materialized_mainboard"]
        for row in rows:
            if card_names[row["card_id"]] != row["name"]:
                raise ParityError(f"board card id {row['card_id']} is not {row['name']} at the board revision")
        out[deck_id] = Counter((row["card_id"], row["name"]) for row in rows)
    return out


def strip_rust(source: str) -> str:
    """Rust source without comments (nested block comments included), string, raw-string and char literals."""
    out, i, n = [], 0, len(source)
    while i < n:
        two = source[i:i + 2]
        if two == "//":
            end = source.find("\n", i)
            i = n if end < 0 else end
        elif two == "/*":
            depth, i = 1, i + 2
            while i < n and depth:
                if source[i:i + 2] == "/*":
                    depth, i = depth + 1, i + 2
                elif source[i:i + 2] == "*/":
                    depth, i = depth - 1, i + 2
                else:
                    i += 1
        elif source[i] in "rbc" and (i == 0 or not (source[i - 1].isalnum() or source[i - 1] == "_")) \
                and RAW_STRING.match(source, i):
            raw = RAW_STRING.match(source, i)
            hashes = raw.group(1)
            end = source.find('"' + hashes, raw.end())
            i = n if end < 0 else end + 1 + len(hashes)
        elif source[i] == '"':
            i += 1
            while i < n and source[i] != '"':
                i += 2 if source[i] == "\\" else 1
            i += 1
        elif source[i] == "'" and i + 2 < n and (source[i + 2] == "'" or (source[i + 1] == "\\" and "'" in source[i + 2:i + 12])):
            i = source.index("'", i + 2) + 1  # a char literal ('x' or an escape); lifetimes have no closing quote
        else:
            out.append(source[i])
            i += 1
            continue
        out.append(" ")
    return "".join(out)


def bridge_rules(sources: list[str]) -> dict:
    """The board bridge's runtime-rules facts at its revision (static: calls in code, not comments or strings)."""
    return {"calls_set_runtime_rules": any(RULES_CALL.search(strip_rust(source)) for source in sources)}


def compare(training: dict, board_manifest: dict, board_config: dict, board_pool_decks: dict, bridge: dict) -> dict:
    rules = board_manifest["information_rules"]["rules"]
    supported = board_manifest["engine_profile"]["rules_supported"]
    board_limits = board_config.get("limits") or board_manifest.get("tournament", {}).get("limits") or {}
    snapshot_id = board_manifest["engine"].get("rules_snapshot_id", "")
    board_neutral = ":neutral-" in snapshot_id
    checks = {
        "mulligans_off_on_board": rules["mulligan"] == "none" and "none" in supported["mulligan"],
        "mulligans_off_in_training": set(training["mulligan_profiles"]) == {"none"},
        "training_rules_profile_is_default": set(training["runtime_rules"]) == {"default"},
        "board_rules_profile_is_default": not bridge["calls_set_runtime_rules"],
        "neutral_choices_equal": not board_neutral,  # training sessions never enable them
        "ordinary_starts_game_one": training["reset_episodes"] == 0 and training["postboard_episodes"] == 0
        and training["selected_differs_from_registered"] == 0 and training["malformed_registrations"] == 0,
        "starting_player_assigned_both_seats": rules["starting_player"] == "host_assigned"
        and set(training["starting_players"]) == {"0", "1"} and set(training["learner_plays"]) == {"first", "second"},
        "limits_equal": [(x["max_physical_decisions"], x["max_policy_steps"]) for x in training["limits"]]
        == [(board_limits.get("max_decisions"), board_limits.get("max_steps"))],
    }
    matches, unmatched_board = {}, []
    for deck_id, cards in board_pool_decks.items():
        labels = sorted(label for label, named in training["decks"].items() if named == cards)
        if labels:
            matches[deck_id] = labels
        else:
            unmatched_board.append(deck_id)
    checks["every_board_deck_equals_a_training_deck"] = not unmatched_board
    matched = {label for labels in matches.values() for label in labels}
    return {"checks": checks, "deck_matches": matches, "unmatched_board_decks": unmatched_board,
            "training_only_decks": sorted(set(training["decks"]) - matched),
            "recorded": {"board_limits": board_limits, "training_limits": training["limits"],
                         "board_rules": {k: rules.get(k) for k in ("mulligan", "starting_player", "starting_seat",
                                                                   "extensions")},
                         "board_rules_supported": supported, "board_engine": board_manifest["engine"],
                         "board_neutral_choices": board_neutral,
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
        board_cards_raw = git_show(args.repo, board_rev, "data/cards_v1.json")
        board_card_names = [card["name"] for card in json.loads(board_cards_raw)["cards"]]
        decks_raw = git_show(args.repo, board_rev, "data/runtime_decks_v1.json")
        bridge_raw = {}
        for path in BRIDGE_FILES:
            try:
                bridge_raw[path] = git_show(args.repo, board_rev, path)
            except ParityError:
                pass  # a revision without the FlatV4 lockstep file
        if BRIDGE_FILES[0] not in bridge_raw:
            raise ParityError(f"{BRIDGE_FILES[0]} is missing at the board revision")
        pool = [d["catalog_id"] for d in board_config["deck_pool"]]
        training = training_side(configs, card_names)
        result = compare(training, manifest, board_config,
                         board_decks(json.loads(decks_raw), pool, board_card_names),
                         bridge_rules([raw.decode("utf-8") for raw in bridge_raw.values()]))
    except (ParityError, KeyError, IndexError) as exc:
        print(f"error: {exc!r}", file=sys.stderr)
        return 1
    receipt = {"schema": SCHEMA, "pass": all(result["checks"].values()), **result,
               "training": {k: v for k, v in training.items() if k != "decks"},
               "inputs": {"training_configs": [{"path": str(p), "sha256": sha256(raw)}
                                               for p, raw in zip(args.training_config, raw_configs)],
                          "board_manifest_sha256": sha256(manifest_raw), "board_config_sha256": sha256(config_raw),
                          "board_revision": board_rev, "training_revision": training_rev,
                          "training_cards_v1_sha256": sha256(cards_raw),
                          "board_cards_v1_sha256": sha256(board_cards_raw),
                          "board_runtime_decks_sha256": sha256(decks_raw),
                          "board_bridge_sha256": {k: sha256(v) for k, v in bridge_raw.items()}}}
    text = json.dumps(receipt, indent=2) + "\n"
    if args.out:
        args.out.write_text(text, encoding="utf-8", newline="\n")
    print(text, end="")
    return 0 if receipt["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
