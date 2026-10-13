"""Prepare a reviewable ordered queue; never dispatch or inspect quality."""
import argparse
import json
from pathlib import Path
import platform

from launch import CPU_CAP, GIB, SCHEMA, digest, load, publish, require


def prepare(args):
    root = Path(args.root)
    inputs = root / "inputs"
    index = load(inputs / "INPUTS.json")
    panel = load(inputs / "ROOTS.json")
    calibration = load(args.calibration) if args.calibration else None
    seeds = args.seeds.split(",")
    budgets = [int(n) for n in args.budgets.split(",")]
    selected = set(int(n) for n in args.decks.split(","))
    qualification = args.phase == "qualify"
    require(not args.calibrate or qualification, "cost calibration is a bounded qualification phase")
    if not qualification:
        require(calibration, "measurement queue needs cost-only calibration")
        require(args.qualification, "measurement queue needs throughput receipt")
        require(len(seeds) == 3 and len(set(seeds)) == 3 and all("qualification" not in seed and "calibration" not in seed for seed in seeds),
                "measurement needs three fresh repetition seeds")
    jobs = []
    for repetition, seed in enumerate(seeds):
        for budget in budgets:
            for entry in panel:
                if entry["deck"] not in selected:
                    continue
                lineage, deck = entry["lineage"], entry["deck"]
                group = f"d{deck}-{budget}-r{repetition}"
                shift = (deck + repetition) % 3
                arms = "ABC"[shift:] + "ABC"[:shift]
                for arm in ("A" if args.calibrate else arms):
                    env = {"PROFILE_EXPERIMENT": "1", "PROFILE_ARM": arm, "PROFILE_SEED": seed,
                           "PROFILE_CASES": (inputs / f"case-{deck}-{lineage}.jsonl").as_posix(),
                           "PROFILE_POSITIONS": f"{entry['position']}={entry['fraction']}",
                           "PROFILE_INTERCHANGE": "0", "S4A_KEY_MEMO": "0", "BUDGET_KEY_MODE": "repaired",
                           "BUDGET_TOTAL": str(budget if args.calibrate else args.safety_cap), "PROFILE_ANCHOR": str(budget),
                           "BANK_SPEC": (inputs / f"bank-spec-{lineage}.json").as_posix(),
                           "S4A_MODEL": lineage, "PROFILE_EVALUATE": "0" if args.calibrate else "1", "PROFILE_EVAL_WORLDS": "128",
                           "PROFILE_EVAL_CAP": "65536",
                           "OPPONENTS": f"T1={(inputs / 't1-source.json').as_posix()},A48={(inputs / 'a48-source.json').as_posix()}"}
                    if qualification and not args.calibrate:
                        env["PROFILE_WORK_LIMIT"] = str(args.work_limit)
                    elif not qualification:
                        target = calibration["roots"][entry["id"]][str(budget)]
                        env["PROFILE_CPU_SECONDS"] = str(target["native_cpu_seconds"])
                        env["PROFILE_CPU_OVERHEAD_RESERVE_SECONDS"] = str(target["overhead_reserve_seconds"])
                    jobs.append({"id": f"{group}-{arm}", "group": group, "arm": arm, "root": entry["id"],
                                 "seed": seed, "anchor": budget, "env": env, "expected_rows": 1,
                                 "argv": [f"source={(inputs / (lineage + '-source.json')).as_posix()}",
                                          "mode=s4a-search-profile", "games=1", "first_game=0",
                                          "decks=0,1,2,3,4,5,6,7,8"],
                                 "max_cpu_seconds": args.job_cpu_cap, "max_wall_seconds": args.job_wall_cap})
    require(jobs, "empty root selection")
    spec = {"schema": SCHEMA, "host": platform.node().upper(), "root": str(root), "commit": args.commit,
            "binary": {"path": str(Path(args.binary).resolve()), "sha256": digest(args.binary)},
            "host_slots": {"path": str(Path(args.host_slots).resolve()), "sha256": digest(args.host_slots)},
            "host_reservation": {"path": str(Path(args.host_reservation).resolve()), "sha256": digest(args.host_reservation)},
            "inputs": index["assets"], "collab_input_ref": index["collab_ref"],
            "eligible_workers": args.eligible_workers, "cores_claimed": args.cores_claimed, "cpu_cap_seconds": args.cpu_cap,
            "external_cpu_seconds": args.external_cpu, "qualification_cpu_cap_seconds": args.qualification_cpu_cap,
            "output_cap_bytes": 4 * GIB, "projected_output_bytes": 64 * 1024**2,
            "ledger": str(root / "CPU-LEDGER.jsonl"), "cold_binary_root": args.cold_binary_root,
            "storage": {"hot_volume": args.hot_volume, "hot_disk": args.hot_disk, "cold_volume": args.cold_volume},
            "wait_for_cores": True, "wait_timeout_seconds": args.wait_timeout,
            "host_slots_env": ({"HOST_SLOTS_ROOT": args.host_lock_root, "MTG_HOST_LOCK_ROOT": args.host_lock_root}
                               if args.host_lock_root else {}),
            "workload": {"mode": "s4a-search-profile", "experiment": "search-sharing-allocation-20261013",
                         "arms": "ABC", "root_panel_sha256": digest(inputs / "ROOTS.json"),
                         "common_flags": {"key_mode": "repaired", "interchange": False, "key_memo": False},
                         "evaluation": {"enabled": not args.calibrate, "continuation": "frozen-search-tree", "worlds": 128, "cap": 65536},
                         "selection_safety_cap": args.safety_cap,
                         "budget_anchors": budgets, "qualification_work_limit": args.work_limit},
            "seeds": seeds, "jobs": jobs}
    if calibration:
        spec["calibration"] = {"path": str(Path(args.calibration).resolve()), "sha256": digest(args.calibration)}
        spec["inputs"].append(spec["calibration"])
    if args.qualification:
        spec["qualification"] = {"path": str(Path(args.qualification).resolve()), "sha256": digest(args.qualification)}
    publish(args.out, spec)
    return {"path": str(args.out), "sha256": digest(args.out), "jobs": len(jobs), "seeds": seeds,
            "phase": args.phase, "not_launched": True}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default="D:/e-scratch/search-sharing-allocation-20261013" if platform.system() == "Windows" else "/workspace/search-sharing-allocation-20261013")
    parser.add_argument("--binary", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--host-slots", required=True)
    parser.add_argument("--host-reservation", default=str(Path(__file__).resolve().parents[2] / "python/tools/host_reservation_v1.py"))
    parser.add_argument("--host-lock-root", help="on Linux use /var/lib/mtg-node/host-lock for both supported helpers")
    parser.add_argument("--phase", choices=("qualify", "measure"), default="qualify")
    parser.add_argument("--out", required=True)
    parser.add_argument("--eligible-workers", type=int, default=2)
    parser.add_argument("--cores-claimed", type=int, default=2)
    parser.add_argument("--decks", default="0,1,2")
    parser.add_argument("--seeds", default="search-sharing-allocation-20261013/qualification/r1")
    parser.add_argument("--budgets", default="16000,64000")
    parser.add_argument("--work-limit", type=int, default=32)
    parser.add_argument("--job-cpu-cap", type=float, default=300)
    parser.add_argument("--job-wall-cap", type=float, default=1800)
    parser.add_argument("--cpu-cap", type=float, default=CPU_CAP)
    parser.add_argument("--external-cpu", type=float, default=0)
    parser.add_argument("--qualification-cpu-cap", type=float, default=1200)
    parser.add_argument("--wait-timeout", type=float, default=43200)
    parser.add_argument("--calibration")
    parser.add_argument("--qualification")
    parser.add_argument("--calibrate", action="store_true", help="cost-only A at actual baseline transition anchors, no fresh quality evaluation")
    parser.add_argument("--safety-cap", type=int, default=16000000)
    parser.add_argument("--cold-binary-root", default="E:/pinned-binaries" if platform.system() == "Windows" else "/workspace/pinned-binaries")
    parser.add_argument("--hot-volume", default="D" if platform.system() == "Windows" else "/workspace")
    parser.add_argument("--hot-disk", default="Samsung SSD 870 EVO 1TB" if platform.system() == "Windows" else "pod-local-ssd")
    parser.add_argument("--cold-volume", default="E" if platform.system() == "Windows" else "/workspace")
    args = parser.parse_args()
    print(json.dumps(prepare(args)))
