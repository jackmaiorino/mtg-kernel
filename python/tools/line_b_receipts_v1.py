#!/usr/bin/env python3
"""Line (b) engineering receipts through the release training CLI.

Receipts check identity, determinism, telemetry and timing; they make no
outcome claim. Engineering seeds only (namespace engineering/line-b/v1,
never the screen's labels). Batches follow the screen's structure: a
template iteration of the broader config (d2e4ace8) gives ten games, the
learner's published-list games get teacher seeds and the canonical ones
null, and opponents follow the parity rule (even slots the frozen g115,
odd slots A48). The template and the model pins are read, never written;
everything is written under --root/<name>/, which must not exist.

Modes:
  acceptance  one matched batch collected once from g115; control,
              reverse-kl and forward-kl updates on it per backend, each
              replayed; head distances; a serial collection replay.
  chain       per arm a chain of updates, each collecting from the
              previous checkpoint, replayed; control collections also
              collected serially (identity smokes).
  throughput  the teach step on one fixed batch of published-list games
              at several worker counts (same seeds, identical packets).
  diagnostics independent first updates from g115 over consecutive
              template iterations (reverse-kl): per root p_max and entropy
              of the student and the target, census by list, per-rollout
              decisions, head-norm ratios.
"""
import argparse
import hashlib
import json
import math
import struct
import subprocess
import time
from pathlib import Path

BIN = Path("D:/cargo-target/opus-exit-teacher/release/expanded_deck_training_v1.exe")
TEMPLATE = Path("E:/mtg-postboard-campaign-20260920/broader-exposure-pilot-001/broader/config.json")
TEMPLATE_SHA256 = "d2e4ace81253e6a393d5b3687250f77f5c4af433afd28dda97fa5ee2ab533529"
G115_CHECKPOINT_SHA256 = "88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1"
G115_STATE_SHA256 = "8139016ca561961714f25e22a9d6f7fc888548fc332e45b6bce402dfc43159f2"
G115_ADAM_STEP = 32400
A48_ID = "fresh-a-block48-end"
LOSS = {"kind": "gae_advantage_value_v1", "gamma": 1.0, "lambda": 0.9, "entropy_coefficient": 0.0}
MASK = "line-b-head-only-mask-v1"
SAMPLER = "unclamped-softmax-f64-icdf-u53-v1"
BACKENDS = {"cpu": None, "cuda1": {"kind": "cuda", "device_ordinal": 1}}
ARMS = ("control", "reverse-kl", "forward-kl")


def seed(label):
    digest = hashlib.sha256(("engineering/line-b/v1|" + label).encode()).digest()
    return int.from_bytes(digest[:8], "big")


def slot_label(name, update, slot, kind):
    return f"{name}/update/{update:03}/slot/{slot:02}/{kind}"


def run(command, directory, name, affinity=None):
    """Runs one command at BelowNormal priority; `affinity` (a list of
    logical processors) pins the process right after it starts."""
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / f"{name}.command.json"
    path.write_text(json.dumps(command, indent=1))
    started = time.perf_counter()
    process = subprocess.Popen(
        [str(BIN), str(path)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS,
    )
    if affinity is not None:
        import psutil

        psutil.Process(process.pid).cpu_affinity(affinity)
    stdout, stderr = process.communicate()
    seconds = time.perf_counter() - started
    (directory / f"{name}.stderr.txt").write_text(stderr)
    if process.returncode != 0:
        raise SystemExit(f"{name} failed ({process.returncode}): {stderr[-2000:]}")
    result = json.loads(stdout)
    (directory / f"{name}.result.json").write_text(json.dumps(result, indent=1))
    return result, seconds


def affinity_from_mask(mask):
    return None if mask is None else [cpu for cpu in range(64) if (mask >> cpu) & 1]


def host_load():
    """Host CPU busy percent over one second (the quiet-host check)."""
    import psutil

    return psutil.cpu_percent(interval=1.0)


def template():
    raw = TEMPLATE.read_bytes()
    if hashlib.sha256(raw).hexdigest() != TEMPLATE_SHA256:
        raise SystemExit("template SHA-256 differs")
    config = json.loads(raw)
    initial = config["initial_source"]
    if initial["checkpoint"]["sha256"] != G115_CHECKPOINT_SHA256:
        raise SystemExit("template initial source is not the frozen g115")
    a48 = next(entry["source"] for entry in config["opponents"] if entry["id"] == A48_ID)
    return initial, a48, config["iterations"]


def learner_list(episode):
    return episode["selected"][episode["learner_seat"]]["label"]


def batch(name, update, entries, initial, a48):
    """Episodes and teacher entries for one update from template entries."""
    episodes, games = [], []
    for slot, entry in enumerate(entries):
        episode = dict(entry["episode"])
        episode["id"] = f"engineering-line-b-{name}-u{update:03}-s{slot:02}"
        episode["seed"] = seed(slot_label(name, update, slot, "episode"))
        episode["opponent"] = initial if slot % 2 == 0 else a48
        episodes.append(episode)
        published = learner_list(episode).startswith("published-")
        games.append(
            {
                "root_seed": seed(slot_label(name, update, slot, "root")),
                "teacher_seed": seed(slot_label(name, update, slot, "rollout")),
            }
            if published
            else None
        )
    return episodes, games


def collect_command(student, episodes, workers, output):
    return {
        "mode": "collect_parallel",
        "source": student,
        "episodes": episodes,
        "workers": workers,
        "collection_sampler": SAMPLER,
        "output_directory": str(output),
    }


def teacher(direction, games, args, permuted_control=True):
    options = {
        "direction": direction,
        "coefficient": 0.1,
        "temperature": 0.25,
        "rollouts": args.rollouts,
        "workers": args.workers,
        "games": games,
    }
    if permuted_control:
        options["permuted_control"] = True
    return options


def update_command(student, trajectories, backend, options, output):
    line_b = {"optimizer_mask": MASK}
    if options is not None:
        line_b["teacher"] = options
    command = {
        "mode": "update_prepared",
        "source": student,
        "trajectories": trajectories,
        "learning_rate": 0.0001,
        "value_coefficient": 0.5,
        "loss_selection": LOSS,
        "preparation_workers": 4,
        "line_b": line_b,
        "output_directory": str(output),
    }
    if BACKENDS[backend] is not None:
        command["update_backend"] = BACKENDS[backend]
    return command


def summarize(result, seconds):
    line_b = result["line_b"]
    return {
        "seconds": round(seconds, 3),
        "before_state_sha256": result["before_state_sha256"],
        "after_state_sha256": result["after_state_sha256"],
        "adam_step": result["adam_step"],
        "checkpoint": result["checkpoint"],
        "frozen_tensor_sha256": line_b["frozen_tensor_sha256"],
        "scorer_bias_gauge": line_b["scorer_bias_gauge"],
        "teacher": line_b.get("teacher"),
    }


def acceptance_checks(record):
    updates = record["updates"]
    found = {
        "one_frozen_tensor_sha256": len({u["frozen_tensor_sha256"] for u in updates.values()}) == 1,
        "before_state_is_g115": all(
            u["before_state_sha256"] == G115_STATE_SHA256 for u in updates.values()
        ),
        "one_combined_adam_step": all(u["adam_step"] == G115_ADAM_STEP + 1 for u in updates.values()),
        "gauge_within_bound": all(u["scorer_bias_gauge"]["within_bound"] for u in updates.values()),
        "gauge_anchor_preserved": all(
            u["scorer_bias_gauge"]["anchor_preserved"] for u in updates.values()
        ),
    }
    replays, packets = {}, {}
    for label, u in updates.items():
        backend = label.split("-", 1)[0]
        arm = label[len(backend) + 1 : -2]
        replays.setdefault(f"{backend}/{arm}", []).append(u["after_state_sha256"])
        if u["teacher"] is not None:
            packets.setdefault(arm, set()).add(u["teacher"]["packet"]["sha256"])
    found["replays_identical"] = {
        key: len(states) > 1 and len(set(states)) == 1 for key, states in replays.items()
    }
    found["one_packet_per_arm"] = {arm: len(shas) == 1 for arm, shas in packets.items()}
    envelopes = [
        u["teacher"]["cuda_envelope"]
        for u in updates.values()
        if u["teacher"] is not None and "cuda_envelope" in u["teacher"]
    ]
    found["cuda_envelope_within_bound"] = (
        all(e["within_bound"] for e in envelopes) if envelopes else None
    )
    for backend in sorted({label.split("-", 1)[0] for label in updates}):
        control = updates.get(f"{backend}-control-a")
        for arm in ARMS[1:]:
            treated = updates.get(f"{backend}-{arm}-a")
            if control and treated:
                found[f"{backend}/{arm}_differs_from_control"] = (
                    treated["after_state_sha256"] != control["after_state_sha256"]
                )
    if "serial_collect" in record:
        found["parallel_collection_equals_serial"] = (
            record["serial_collect"]["trajectories_sha256"] == record["collect"]["trajectories_sha256"]
        )
    return found


def acceptance(args, directory):
    initial, a48, iterations = template()
    episodes, games = batch(args.name, 0, iterations[args.iteration]["episodes"], initial, a48)
    collect = collect_command(initial, episodes, args.collect_workers, directory / "collect")
    collected, seconds = run(collect, directory, "collect")
    trajectories = collected["trajectories"]
    record = {
        "schema": "line-b-engineering-acceptance-receipt/v1",
        "name": args.name,
        "claim": "engineering receipt: identity, determinism and telemetry only; no outcome claim",
        "template": {"path": str(TEMPLATE), "sha256": TEMPLATE_SHA256, "iteration": args.iteration},
        "student": initial,
        "rollouts": args.rollouts,
        "teacher_workers": args.workers,
        "games": games,
        "learner_lists": [learner_list(e) for e in episodes],
        "collect": {
            "seconds": round(seconds, 3),
            "workers": args.collect_workers,
            "trajectories_sha256": [pin["sha256"] for pin in trajectories],
        },
        "updates": {},
        "head_distance": {},
    }

    def save():
        record["checks"] = acceptance_checks(record)
        (directory / "receipt.json").write_text(json.dumps(record, indent=1))

    for backend in args.backends:
        for arm in ARMS:
            for replay in "ab"[: args.replays]:
                label = f"{backend}-{arm}-{replay}"
                options = None if arm == "control" else teacher(arm, games, args)
                command = update_command(initial, trajectories, backend, options, directory / label)
                result, seconds = run(command, directory, label)
                record["updates"][label] = summarize(result, seconds)
                save()
                print(label, round(seconds, 1), "s", result["after_state_sha256"][:12], flush=True)
        for arm in ARMS[1:]:
            distance = {
                "mode": "line_b_head_distance",
                "treatment": dict(initial, checkpoint=record["updates"][f"{backend}-{arm}-a"]["checkpoint"]),
                "control": dict(initial, checkpoint=record["updates"][f"{backend}-control-a"]["checkpoint"]),
            }
            result, _ = run(distance, directory, f"{backend}-{arm}-head-distance")
            record["head_distance"][f"{backend}/{arm}"] = result
            save()
    if args.serial_collect:
        serial = dict(collect, workers=1, output_directory=str(directory / "collect-serial"))
        collected, seconds = run(serial, directory, "collect-serial")
        record["serial_collect"] = {
            "seconds": round(seconds, 3),
            "trajectories_sha256": [pin["sha256"] for pin in collected["trajectories"]],
        }
    save()
    print(json.dumps(record["checks"], indent=1))


def chain(args, directory):
    initial, a48, iterations = template()
    backend = args.backends[0]
    record = {
        "schema": "line-b-engineering-chain-receipt/v1",
        "name": args.name,
        "claim": "engineering receipt: identity and determinism only; no outcome claim",
        "template": {"path": str(TEMPLATE), "sha256": TEMPLATE_SHA256},
        "rollouts": args.rollouts,
        "teacher_workers": args.workers,
        "backend": backend,
        "chains": {},
    }
    for arm in ARMS:
        for replay in "ab"[: args.replays]:
            student = initial
            steps = []
            for update in range(args.updates):
                label = f"{arm}-{replay}-u{update:03}"
                entries = iterations[args.iteration + update]["episodes"]
                episodes, games = batch(args.name, update, entries, initial, a48)
                collect = collect_command(
                    student, episodes, args.collect_workers, directory / f"{label}-collect"
                )
                collected, collect_seconds = run(collect, directory, f"{label}-collect")
                step = {
                    "collect_seconds": round(collect_seconds, 3),
                    "trajectories_sha256": [pin["sha256"] for pin in collected["trajectories"]],
                }
                if arm == "control" and replay == "a":
                    serial = dict(
                        collect, workers=1, output_directory=str(directory / f"{label}-collect-serial")
                    )
                    serial_result, _ = run(serial, directory, f"{label}-collect-serial")
                    step["serial_equals_parallel"] = [
                        pin["sha256"] for pin in serial_result["trajectories"]
                    ] == step["trajectories_sha256"]
                options = None if arm == "control" else teacher(arm, games, args, False)
                command = update_command(
                    student, collected["trajectories"], backend, options, directory / label
                )
                result, seconds = run(command, directory, label)
                step.update(summarize(result, seconds))
                steps.append(step)
                student = dict(initial, checkpoint=result["checkpoint"])
                print(label, round(seconds, 1), "s", result["after_state_sha256"][:12], flush=True)
            record["chains"][f"{arm}-{replay}"] = steps
            (directory / "receipt.json").write_text(json.dumps(record, indent=1))
    found = {}
    for arm in ARMS:
        runs = [record["chains"].get(f"{arm}-{replay}") for replay in "ab"[: args.replays]]
        if len(runs) > 1 and all(runs):
            found[f"{arm}_replays_identical"] = all(
                a["after_state_sha256"] == b["after_state_sha256"]
                and a["trajectories_sha256"] == b["trajectories_sha256"]
                and (a["teacher"] or {}).get("packet") == (b["teacher"] or {}).get("packet")
                for a, b in zip(*runs)
            )
    control = record["chains"].get("control-a", [])
    found["control_serial_equals_parallel"] = bool(control) and all(
        step.get("serial_equals_parallel") for step in control
    )
    steps = [s for chain_steps in record["chains"].values() for s in chain_steps]
    found["frozen_tensor_sha256_constant"] = len({s["frozen_tensor_sha256"] for s in steps}) == 1
    found["gauge_within_bound"] = all(
        s["scorer_bias_gauge"]["within_bound"] and s["scorer_bias_gauge"]["anchor_preserved"]
        for s in steps
    )
    record["checks"] = found
    (directory / "receipt.json").write_text(json.dumps(record, indent=1))
    print(json.dumps(found, indent=1))


def packet_roots(pin):
    """Per root: list, student and target p_max and entropy, status and
    per-rollout physical decisions and outcome kinds."""
    packet = json.loads(Path(pin["path"]).read_text())
    roots = []
    for game in packet["games"]:
        if game["kind"] != "root":
            continue
        logits = [bits_to_f32(bits) for bits in game["collection_logits"]]
        student = softmax(logits)
        status_kind = next(iter(game["status"]))
        target = None
        if status_kind == "complete":
            target = [math.exp(v) for v in game["status"]["complete"]["log_target"]]
        roots.append(
            {
                "trajectory_index": game["trajectory_index"],
                "width": len(logits),
                "status": status_kind,
                "student_p_max": max(student),
                "student_entropy": entropy(student),
                "target_p_max": max(target) if target else None,
                "target_entropy": entropy(target) if target else None,
                "rollout_decisions": [r["physical_decisions"] for r in game["rollouts"]],
                "rollout_outcomes": [next(iter(r["outcome"])) for r in game["rollouts"]],
            }
        )
    return roots


def bits_to_f32(bits):
    return struct.unpack("<f", struct.pack("<I", bits))[0]


def softmax(logits):
    top = max(logits)
    weights = [math.exp(z - top) for z in logits]
    total = sum(weights)
    return [w / total for w in weights]


def entropy(probabilities):
    return -sum(p * math.log(p) for p in probabilities if p > 0.0)


def throughput(args, directory):
    initial, a48, iterations = template()
    entries = [
        entry
        for iteration in iterations[args.iteration :]
        for entry in iteration["episodes"]
        if learner_list(entry["episode"]).startswith("published-")
    ][: args.roots]
    episodes, games = batch(args.name, 0, entries, initial, a48)
    collect = collect_command(initial, episodes, args.collect_workers, directory / "collect")
    collected, seconds = run(collect, directory, "collect")
    affinity = affinity_from_mask(args.affinity_mask)
    record = {
        "schema": "line-b-engineering-throughput-receipt/v1",
        "name": args.name,
        "claim": "engineering timing of the teach step on one fixed batch; no outcome claim",
        "template": {"path": str(TEMPLATE), "sha256": TEMPLATE_SHA256},
        "roots_requested": args.roots,
        "rollouts": args.rollouts,
        "backend": args.backends[0],
        "placement": {
            "priority": "below_normal",
            "affinity": affinity if affinity is not None else "all logical processors",
        },
        "collect_seconds": round(seconds, 3),
        "runs": {},
    }
    for workers in args.worker_counts:
        label = f"workers-{workers:02}"
        options = teacher(
            "reverse-kl", games, argparse.Namespace(rollouts=args.rollouts, workers=workers), False
        )
        command = update_command(
            initial, collected["trajectories"], args.backends[0], options, directory / label
        )
        load_before = host_load()
        result, seconds = run(command, directory, label, affinity)
        taught = result["line_b"]["teacher"]
        roots = packet_roots(taught["packet"])
        decisions = [d for root in roots for d in root["rollout_decisions"]]
        outcomes = {}
        for root in roots:
            for kind in root["rollout_outcomes"]:
                outcomes[kind] = outcomes.get(kind, 0) + 1
        teacher_seconds = taught["teacher_seconds"]
        record["runs"][label] = {
            "workers": workers,
            "host_cpu_percent_before": load_before,
            "update_seconds": round(seconds, 3),
            "teacher_seconds": teacher_seconds,
            "roots": len(roots),
            "rollouts": len(decisions),
            "rollouts_per_second": len(decisions) / teacher_seconds if teacher_seconds else None,
            "physical_decisions_mean": sum(decisions) / len(decisions) if decisions else None,
            "physical_decisions_max": max(decisions) if decisions else None,
            "rollout_outcomes": outcomes,
            "census": taught["census"],
            "packet_sha256": taught["packet"]["sha256"],
        }
        (directory / "receipt.json").write_text(json.dumps(record, indent=1))
        print(label, round(teacher_seconds, 1), "s", len(decisions), "rollouts", flush=True)
    record["checks"] = {
        "one_packet_across_worker_counts": len({r["packet_sha256"] for r in record["runs"].values()}) == 1
    }
    (directory / "receipt.json").write_text(json.dumps(record, indent=1))
    print(json.dumps(record["checks"]))


def diagnostics(args, directory):
    initial, a48, iterations = template()
    record = {
        "schema": "line-b-engineering-diagnostics-receipt/v1",
        "name": args.name,
        "claim": "non-outcome diagnostics of the teacher at engineering roots; no outcome claim",
        "template": {"path": str(TEMPLATE), "sha256": TEMPLATE_SHA256},
        "rollouts": args.rollouts,
        "teacher_workers": args.workers,
        "backend": args.backends[0],
        "updates": [],
    }
    for update in range(args.updates):
        label = f"u{update:03}"
        entries = iterations[args.iteration + update]["episodes"]
        episodes, games = batch(args.name, update, entries, initial, a48)
        collect = collect_command(initial, episodes, args.collect_workers, directory / f"{label}-collect")
        collected, _ = run(collect, directory, f"{label}-collect")
        options = teacher("reverse-kl", games, args, False)
        command = update_command(
            initial, collected["trajectories"], args.backends[0], options, directory / label
        )
        result, seconds = run(command, directory, label)
        taught = result["line_b"]["teacher"]
        roots = packet_roots(taught["packet"])
        for root in roots:
            root["list"] = learner_list(episodes[root["trajectory_index"]])
        telemetry = taught["telemetry"] or {}
        record["updates"].append(
            {
                "label": label,
                "seconds": round(seconds, 3),
                "teacher_seconds": taught["teacher_seconds"],
                "census": taught["census"],
                "auxiliary_head_l2": telemetry.get("auxiliary_head_l2"),
                "ordinary_head_l2": telemetry.get("ordinary_head_l2"),
                "head_l2_ratio": telemetry.get("head_l2_ratio"),
                "roots": roots,
            }
        )
        (directory / "receipt.json").write_text(json.dumps(record, indent=1))
        print(label, round(seconds, 1), "s", len(roots), "roots", flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", choices=["acceptance", "chain", "throughput", "diagnostics"], required=True)
    parser.add_argument("--name", required=True)
    parser.add_argument("--root", type=Path, default=Path("D:/e-scratch/opus-exit-teacher/receipts"))
    parser.add_argument("--iteration", type=int, default=0)
    parser.add_argument("--updates", type=int, default=4)
    parser.add_argument("--roots", type=int, default=20)
    parser.add_argument("--rollouts", type=int, default=16)
    parser.add_argument("--workers", type=int, default=8)
    parser.add_argument("--worker-counts", type=int, nargs="+", default=[1, 8, 24])
    parser.add_argument("--collect-workers", type=int, default=8)
    parser.add_argument("--replays", type=int, default=2)
    parser.add_argument("--backends", nargs="+", default=["cuda1", "cpu"])
    parser.add_argument("--serial-collect", action="store_true")
    parser.add_argument(
        "--affinity-mask",
        type=lambda text: int(text, 0),
        default=None,
        help="throughput only: pin each update to these logical processors (e.g. 0xffff for the P-cores)",
    )
    args = parser.parse_args()
    directory = args.root / args.name
    if directory.exists():
        raise SystemExit(f"{directory} exists; receipts never overwrite")
    directory.mkdir(parents=True)
    {"acceptance": acceptance, "chain": chain, "throughput": throughput, "diagnostics": diagnostics}[
        args.mode
    ](args, directory)


if __name__ == "__main__":
    main()
