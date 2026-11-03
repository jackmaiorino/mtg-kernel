"""Measurement only: how Spellbench neutral choices change the decision surface of the board's mtg-kernel bridge.

Plays the same games through `agent_bridge_v1 --x-kernel-flat-v4` twice: once with `--private-v2-support` (the
board's neutral-choice mode, `:neutral-v2`) and once without (the training session's automatic choices). The
policy is a fixed hash of (game seed, ordinal of the decision among decisions with two or more candidates), so
if neutral mode only poses single-option choices, both modes take the same multi-option decisions and play the
same game. Reported per mode and game: decisions, decisions with exactly one candidate by kind, multi-option
decisions, the V4 model input at every multi-option decision of each seat (hashed), and the outcome.

  neutral_census.py --binary agent_bridge_v1 --out RESULT.json [--seeds 2] [--decks Wildfire,...]
"""
import argparse
import hashlib
import json
import subprocess
import sys
from collections import Counter

BOARD_DECKS = ["Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "CawGates", "Faeries"]


def choose(seed: int, ordinal: int, count: int) -> int:
    digest = hashlib.sha256(json.dumps(["w5/neutral-census", seed, ordinal]).encode()).digest()
    return int.from_bytes(digest[:8], "little") % count


class Bridge:
    def __init__(self, binary: str, neutral: bool):
        args = [binary, "--x-kernel-flat-v4"] + (["--private-v2-support"] if neutral else [])
        self.proc = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
        self.n = 0

    def call(self, request: dict) -> dict:
        self.n += 1
        request = dict(request, protocol="spellbench/v1", request_id=f"r-{self.n}")
        self.proc.stdin.write(json.dumps(request, separators=(",", ":")) + "\n")
        self.proc.stdin.flush()
        return json.loads(self.proc.stdout.readline())

    def close(self):
        self.proc.stdin.close()
        self.proc.wait(timeout=60)


def play(bridge: Bridge, game_id: str, decks, seed: int) -> dict:
    response = bridge.call({"request_type": "reset", "game_id": game_id, "format": "pauper-bo1",
                            "game_seed": seed, "max_decisions": 10000, "max_steps": 100000,
                            "seats": [{"seat": "p0", "deck": {"catalog_id": decks[0]}},
                                      {"seat": "p1", "deck": {"catalog_id": decks[1]}}]})
    singles, multi_inputs, decisions, ordinal = Counter(), [], 0, 0
    while response.get("response_type") == "decision":
        candidates = response["candidates"]
        decisions += 1
        if len(candidates) == 1:
            pick = 0
            singles[candidates[0]["semantic"]["kind"]] += 1
        else:
            pick = choose(seed, ordinal, len(candidates))
            ordinal += 1
            flat = response.get("extensions", {}).get("x_kernel_flat_v4")
            tensor = None if flat is None else hashlib.sha256(
                json.dumps(flat.get("tensor"), sort_keys=True).encode()).hexdigest()[:16]
            multi_inputs.append([response["acting_seat"], len(candidates),
                                 sorted({c["semantic"]["kind"] for c in candidates}), tensor])
        chosen = candidates[pick]
        response = bridge.call({"request_type": "step", "game_id": game_id, "expected_step": response["step"],
                                "selection": {"candidate_id": chosen["candidate_id"],
                                              "semantic_echo": chosen["semantic"]}})
    return {"decisions": decisions, "singles": dict(singles), "multi": len(multi_inputs),
            "multi_inputs": multi_inputs, "terminal": {k: response.get(k) for k in
                                                       ("response_type", "outcome", "winner", "reason",
                                                        "classification", "error")}}


def compare(a: dict, b: dict) -> dict:
    """Neutral (a) against automatic (b): the same multi-option decisions and model inputs, and the outcome."""
    ia, ib = a["multi_inputs"], b["multi_inputs"]
    first = next((i for i, (x, y) in enumerate(zip(ia, ib)) if x != y), None)
    if first is None and len(ia) != len(ib):
        first = min(len(ia), len(ib))
    return {"identical_multi_option_sequence": first is None, "first_divergence": first,
            "at_divergence": None if first is None else {"neutral": ia[first] if first < len(ia) else None,
                                                         "automatic": ib[first] if first < len(ib) else None},
            "same_terminal": a["terminal"] == b["terminal"]}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--seeds", type=int, default=2)
    parser.add_argument("--decks", default=",".join(BOARD_DECKS))
    args = parser.parse_args()
    decks = args.decks.split(",")
    games = []
    bridges = {mode: Bridge(args.binary, mode == "neutral") for mode in ("neutral", "automatic")}
    for i, d0 in enumerate(decks):
        for j, d1 in enumerate(decks):
            for s in range(args.seeds):
                seed = 20261103 + 1000 * (i * len(decks) + j) + s
                result = {"decks": [d0, d1], "seed": seed}
                for mode, bridge in bridges.items():
                    result[mode] = play(bridge, f"g-{mode}-{i}-{j}-{s}", (d0, d1), seed)
                result["comparison"] = compare(result["neutral"], result["automatic"])
                games.append(result)
                print(f"{d0} vs {d1} seed {seed}: neutral {result['neutral']['decisions']} "
                      f"automatic {result['automatic']['decisions']} {result['comparison']}", flush=True)
    for bridge in bridges.values():
        bridge.close()
    n = len(games)
    singles = Counter()
    for g in games:
        singles.update(g["neutral"]["singles"])
    auto_singles = Counter()
    for g in games:
        auto_singles.update(g["automatic"]["singles"])
    summary = {
        "games": n,
        "neutral_decisions_per_game": sum(g["neutral"]["decisions"] for g in games) / n,
        "automatic_decisions_per_game": sum(g["automatic"]["decisions"] for g in games) / n,
        "neutral_single_option_per_game": sum(sum(g["neutral"]["singles"].values()) for g in games) / n,
        "automatic_single_option_per_game": sum(sum(g["automatic"]["singles"].values()) for g in games) / n,
        "neutral_single_option_by_kind": dict(singles.most_common()),
        "automatic_single_option_by_kind": dict(auto_singles.most_common()),
        "games_with_identical_multi_option_sequence": sum(g["comparison"]["identical_multi_option_sequence"]
                                                          for g in games),
        "games_with_same_terminal": sum(g["comparison"]["same_terminal"] for g in games),
        "divergences": [{"decks": g["decks"], "seed": g["seed"], **g["comparison"]} for g in games
                        if not g["comparison"]["identical_multi_option_sequence"]][:40],
        "terminals": dict(Counter(json.dumps(g["neutral"]["terminal"].get("reason")) for g in games)),
    }
    for g in games:
        for mode in ("neutral", "automatic"):
            g[mode].pop("multi_inputs")
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump({"summary": summary, "games": games}, handle, indent=1)
    print(json.dumps(summary, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
