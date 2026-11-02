"""W5 Stage 0b: the budget contract (collab LANES/lead/W5-DESIGN.md v1.3 section 6.1; E8).

Reads capture records of the Stage 0b panel source (b512 episodes under C's recipe and schedule, chain-seed
namespace `w5/0b`, one record per source episode in episode order) and applies section 6.1 in full:

- **Placement** (section 3): per learner seat-episode, k = 1 + floor(128 u), u the top 53 bits over 2^53 of the
  first 8 bytes (little-endian) of SHA256(compact json(["w5/placement", chain, update, seed, episode_slot])), the
  convention of EX1's `ex1_hash_u64_v1`. `placement_index` is the reference the engine hook must match
  (`fixtures` writes test vectors). An episode that does not reach its k-th eligible decision contributes none.
- **Panel:** the first 300 reached states in episode order, one per source game (games are the clusters).
- **Candidates:** s4a at 16k and 64k, search seeds 0 and 1. q_B from `ex1-target-v1` (tools/ex1/target_ref.py,
  recomputed here from the recorded logit bits and edge counts, and checked bit for bit against the engine's q
  when the record carries it). A failed search or no target gives q = p, so gain 0.
- **Reference:** Q_ref(a) = mean terminal score (win 1, draw 1/2, loss 0) of 128 SELF b512 rollouts after a, on
  seeds independent of the candidates. It must cover every action in the union of all candidates' supported sets
  (n >= 8) and the policy's argmax; the tool refuses a record that does not.
- **Gain:** G_B(s) = mean over the 2 seeds of sum_a (q_B(a) - p(a)) Q_ref(a); G_B = mean over states.
- **Cost:** c_B = mean in-box CPU-s per attempted search, failures included.
- **Interval:** percentile bootstrap over states, 10,000 replicates, both budgets resampled with the same state
  indices; the stream is SplitMix64 seeded by the first 8 bytes (little-endian) of SHA256("w5/0b/boot") with
  Spellbench's unbiased index. G_B's lower bound is the sorted replicate at index (n - 1) // 10.
- **Rule** (the only statement): B is eligible iff its lower bound > 0 and G_B >= 0.5 G_64k (point estimates).
  B = argmax G_B / c_B over eligible budgets, the ratios compared at 4 significant figures (Python's
  format(x, ".3e")), ties to the larger B. No eligible budget: W5 stops before Stage 1.

It reads terminal rollout outcomes of b512's continuation only: never a rating, panel score or CP7.

  stage0b.py select --captures DIR [--out RESULT.json]        the budget decision (B or stop) and its receipt
  stage0b.py gate --captures DIR --budget B [--out OUT.json]  section 7.4 gate (c): G at the arm's B on 100
                                                              fresh states, lower bound above 0
  stage0b.py placement --chain C --update U --seed S --slot J
  stage0b.py fixtures [--out FILE]                            placement test vectors for the engine hook

Capture record (`w5-0b-capture/v1`, JSON lines in DIR/capture-*.jsonl):
  {"schema", "episode": <ordinal in episode order>, "game": <source game id>, "chain", "update", "seed", "slot",
   "k": <placed index>, "reached": bool, and when reached:
   "policy": {"logit_bits": [f32 bits per legal action, live menu order]},
   "candidates": [{"budget": 16000|64000, "search_seed": 0|1, "status": "ok"|"failed", "cpu_s": float,
                   "n": [...], "wins": [...], "draws": [...], "q_bits": [...] (optional, engine's q)}],
   "qref": {"rollouts": 128, "actions": [index...], "wins": [...], "draws": [...], "n": [...]}}
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "ex1"))
sys.path.insert(0, str(HERE))
import target_ref  # noqa: E402  (tools/ex1: ex1-target-v1's Python mirror)
from joint_read import SplitMix64, unbiased_index  # noqa: E402

CAPTURE_SCHEMA = "w5-0b-capture/v1"
RESULT_SCHEMA = "w5-0b-result/v1"
BUDGETS = (16000, 64000)
SEEDS = (0, 1)
PANEL = 300
GATE_PANEL = 100
ROLLOUTS = 128
REPLICATES = 10_000
N_MAX = 128
FLOOR_FRACTION = 0.5
PLACEMENT_TAG = "w5/placement"
BOOT_TAG = "w5/0b/boot"


class StageError(Exception):
    pass


def require(ok: bool, message: str) -> None:
    if not ok:
        raise StageError(message)


# ---------------------------------------------------------------- placement (section 3)

def hash_u64(tag: str, chain: str, update: int, seed: int, slot: int) -> int:
    text = json.dumps([tag, chain, update, seed, slot], separators=(",", ":"))
    return int.from_bytes(hashlib.sha256(text.encode("utf-8")).digest()[:8], "little")


def placement_index(chain: str, update: int, seed: int, slot: int) -> int:
    """The 1-based index of the eligible learner decision searched in this seat-episode (1..128)."""
    u = target_ref.uniform(hash_u64(PLACEMENT_TAG, chain, update, seed, slot))
    return 1 + int(N_MAX * u)


def tag_seed(tag: str) -> int:
    return int.from_bytes(hashlib.sha256(tag.encode("utf-8")).digest()[:8], "little")


FIXTURE_CASES = [("w5/chain1", 0, 0, 0), ("w5/chain1", 1, 1, 9), ("w5/0b", 0, 12345, 3),
                 ("w5/chain6", 83000, 18446744073709551615, 9), ("w5/filler2", 7, 9007199254740993, 0)]


def fixtures() -> dict:
    rows = []
    for chain, update, seed, slot in FIXTURE_CASES:
        h = hash_u64(PLACEMENT_TAG, chain, update, seed, slot)
        rows.append({"chain": chain, "update": update, "seed": seed, "slot": slot, "hash": h,
                     "uniform_bits": target_ref.bits64(target_ref.uniform(h)),
                     "k": placement_index(chain, update, seed, slot)})
    return {"version": "w5-placement-v1", "tag": PLACEMENT_TAG, "n_max": N_MAX, "cases": rows}


# ---------------------------------------------------------------- per-state gain

def policy(record: dict) -> list[float]:
    p, _, _ = target_ref.softmax([target_ref.f32(b) for b in record["policy"]["logit_bits"]])
    return p


def candidate_q(cand: dict, logit_bits: list[int], p: list[float]) -> tuple[list[float], str]:
    """q_B for one search: ex1-target-v1, or p for a failed search or no target."""
    if cand["status"] != "ok":
        return p, "failed"
    result = target_ref.target(logit_bits, cand["n"], cand["wins"], cand["draws"])
    if isinstance(result, tuple):
        return p, "no_target:" + result[1]
    if cand.get("q_bits") is not None:
        require([target_ref.bits64(x) for x in result["q"]] == cand["q_bits"],
                "engine q differs from the ex1-target-v1 mirror")
    return result["q"], "target"


def state_gains(record: dict) -> dict:
    """Per budget: G_B(s), attempted-search CPU and target outcomes; refuses an incomplete record."""
    require(record.get("schema") == CAPTURE_SCHEMA, "wrong capture schema")
    logit_bits = record["policy"]["logit_bits"]
    p = policy(record)
    cands = {(c["budget"], c["search_seed"]): c for c in record["candidates"]}
    require(len(cands) == len(record["candidates"]) and set(cands) == {(b, s) for b in BUDGETS for s in SEEDS},
            f"episode {record['episode']}: candidates must be 16k and 64k at search seeds 0 and 1")
    qref = record["qref"]
    require(qref["rollouts"] == ROLLOUTS and all(n == ROLLOUTS for n in qref["n"]),
            f"episode {record['episode']}: Q_ref needs {ROLLOUTS} rollouts per action")
    ref = {a: (w + 0.5 * d) / n for a, w, d, n in zip(qref["actions"], qref["wins"], qref["draws"], qref["n"])}
    supported = {a for c in cands.values() if c["status"] == "ok"
                 for a, n in enumerate(c["n"]) if n >= target_ref.SUPPORT_MIN}
    argmax = max(range(len(p)), key=lambda a: (p[a], -a))
    require(supported | {argmax} <= set(ref), f"episode {record['episode']}: Q_ref misses actions "
            f"{sorted((supported | {argmax}) - set(ref))}")
    out = {}
    for budget in BUDGETS:
        gains, outcomes, cpu = [], [], []
        for seed in SEEDS:
            cand = cands[(budget, seed)]
            q, outcome = candidate_q(cand, logit_bits, p)
            gain = 0.0
            for a in range(len(p)):
                if q[a] != p[a]:
                    require(a in ref, f"episode {record['episode']}: q moves action {a} without Q_ref")
                    gain += (q[a] - p[a]) * ref[a]
            gains.append(gain)
            outcomes.append(outcome)
            cpu.append(float(cand["cpu_s"]))
        out[budget] = {"gain": (gains[0] + gains[1]) / 2, "cpu_s": cpu, "outcomes": outcomes}
    return out


# ---------------------------------------------------------------- panel, bootstrap and rule

def load_records(directory: Path) -> list[dict]:
    records = []
    for path in sorted(Path(directory).glob("capture-*.jsonl")):
        for line in path.read_text(encoding="utf-8").splitlines():
            if line.strip():
                records.append(json.loads(line))
    records.sort(key=lambda r: r["episode"])
    episodes = [r["episode"] for r in records]
    require(len(set(episodes)) == len(episodes), "duplicate episode ordinal")
    return records


def draw_panel(records: list[dict], size: int) -> list[dict]:
    """The first `size` reached states in episode order; every earlier episode must be present (no gaps)."""
    panel, games = [], set()
    for expected, record in enumerate(records):
        require(record["episode"] == expected, f"episode ordinals have a gap at {expected}")
        require(record["k"] == placement_index(record["chain"], record["update"], record["seed"], record["slot"]),
                f"episode {expected}: k differs from the placement hash")
        if not record["reached"]:
            continue
        require(record["game"] not in games, f"game {record['game']} gives two states")
        games.add(record["game"])
        panel.append(record)
        if len(panel) == size:
            return panel
    raise StageError(f"only {len(panel)} reached states; the panel needs {size}")


def lower_index(n: int) -> int:
    return (n - 1) // 10


def bootstrap(per_state: list[dict], budgets, seed: int, replicates: int) -> dict[int, list[float]]:
    """Percentile bootstrap over states, every budget resampled with the same indices."""
    rng = SplitMix64(seed)
    n = len(per_state)
    out = {b: [] for b in budgets}
    for _ in range(replicates):
        idx = [unbiased_index(n, rng.next_u64) for _ in range(n)]
        for b in budgets:
            total = 0.0
            for i in idx:
                total += per_state[i][b]["gain"]
            out[b].append(total / n)
    return out


def sig4(x: float) -> str:
    return format(x, ".3e")


def decide(summary: dict) -> dict:
    """Section 6.1's formal rule on per-budget point gains, lower bounds and costs."""
    g64 = summary[64000]["G"]
    eligible = []
    for b in BUDGETS:
        s = summary[b]
        s["clause_lower_bound_positive"] = s["lower_10pct"] > 0
        s["clause_half_gain_floor"] = s["G"] >= FLOOR_FRACTION * g64
        s["eligible"] = s["clause_lower_bound_positive"] and s["clause_half_gain_floor"]
        s["ratio"] = s["G"] / s["c"]
        s["ratio_4sf"] = sig4(s["ratio"])
        if s["eligible"]:
            eligible.append(b)
    if not eligible:
        return {"decision": "stop", "B": None,
                "reason": "no measurable policy improvement from s4a at an affordable budget"}
    best = max(eligible, key=lambda b: (float(summary[b]["ratio_4sf"]), b))
    return {"decision": "budget", "B": best, "eligible": eligible}


def evaluate(records: list[dict], size: int, budgets, boot_tag: str, replicates: int) -> dict:
    panel = draw_panel(records, size)
    per_state = [state_gains(r) for r in panel]
    seed = tag_seed(boot_tag)
    samples = bootstrap(per_state, budgets, seed, replicates)
    summary = {}
    for b in budgets:
        gains = [s[b]["gain"] for s in per_state]
        cpu = [c for s in per_state for c in s[b]["cpu_s"]]
        ordered = sorted(samples[b])
        outcomes = [o for s in per_state for o in s[b]["outcomes"]]
        summary[b] = {"G": sum(gains) / len(gains), "lower_10pct": ordered[lower_index(len(ordered))],
                      "c": sum(cpu) / len(cpu), "attempted": len(cpu),
                      "targets": outcomes.count("target"), "failed": outcomes.count("failed"),
                      "no_target": sum(o.startswith("no_target") for o in outcomes)}
    panel_ids = [[r["episode"], r["game"]] for r in panel]
    return {"panel_size": size, "panel_sha256": hashlib.sha256(json.dumps(panel_ids).encode()).hexdigest(),
            "panel_last_episode": panel[-1]["episode"], "boot_seed": seed, "boot_tag": boot_tag,
            "replicates": replicates, "summary": summary, "per_state": [
                {"episode": r["episode"], "game": r["game"], **{str(b): s[b]["gain"] for b in budgets}}
                for r, s in zip(panel, per_state)]}


def select(records: list[dict], replicates: int = REPLICATES) -> dict:
    result = evaluate(records, PANEL, BUDGETS, BOOT_TAG, replicates)
    result.update(decide(result["summary"]))
    result["schema"] = RESULT_SCHEMA
    result["law"] = ("outcome-adaptive development tuning on terminal rollout outcomes of b512's continuation; "
                     "no rating, panel score or CP7 read")
    result["summary"] = {str(b): v for b, v in result["summary"].items()}
    return result


def gate(records: list[dict], budget: int, arm: str, replicates: int = REPLICATES) -> dict:
    """Section 7.4 gate (c): G at the arm's B on 100 fresh states, 10th percentile above 0."""
    require(budget in BUDGETS, "the gate reads the arm's frozen B")
    result = evaluate(records, GATE_PANEL, (budget,), f"w5/gate-c/boot/{arm}", replicates)
    result["pass"] = result["summary"][budget]["lower_10pct"] > 0
    result["summary"] = {str(b): v for b, v in result["summary"].items()}
    return result


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("select")
    p.add_argument("--captures", type=Path, required=True)
    p.add_argument("--out", type=Path)
    p = sub.add_parser("gate")
    p.add_argument("--captures", type=Path, required=True)
    p.add_argument("--budget", type=int, required=True)
    p.add_argument("--arm", required=True)
    p.add_argument("--out", type=Path)
    p = sub.add_parser("placement")
    for name, kind in (("--chain", str), ("--update", int), ("--seed", int), ("--slot", int)):
        p.add_argument(name, type=kind, required=True)
    p = sub.add_parser("fixtures")
    p.add_argument("--out", type=Path)
    args = parser.parse_args(argv)
    try:
        if args.command == "placement":
            print(placement_index(args.chain, args.update, args.seed, args.slot))
            return 0
        if args.command == "fixtures":
            result = fixtures()
        elif args.command == "select":
            result = select(load_records(args.captures))
        else:
            result = gate(load_records(args.captures), args.budget, args.arm)
    except StageError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    text = json.dumps(result, indent=2) + "\n"
    if getattr(args, "out", None):
        args.out.write_text(text, encoding="utf-8", newline="\n")
    print(text if args.command != "select" else json.dumps({k: result[k] for k in ("decision", "B", "summary")},
                                                            indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
