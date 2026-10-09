"""Reconcile retained timing projections and render the audit (no native work)."""
from collections import defaultdict
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def read(name):
    return json.loads((ROOT / name).read_text(encoding="utf-8"))


def add(rows):
    result = defaultdict(float)
    for row in rows:
        for key, value in row.items():
            result[key] += value
    return dict(result)


def main():
    desktop = read("desktop-profile.json")
    haley = read("haley-block-stage-summary.json")
    detailed = read("haley-update-collection-breakdown.json")
    r1 = read("local-r1-stages.json")
    context = read("context.json")
    archive_desktop = sum(a["archive_seconds"] for a in context["surviving_desktop_archive_subset"])
    archive_seconds = {"desktop": archive_desktop,
                       "all": context["archive_summary"]["seconds"],
                       "computehost": context["archive_summary"]["seconds"]-archive_desktop}
    detail = {(b["run"], b["block"]): b for b in detailed["blocks"] if b["complete"]}
    keys = ("input_read_seconds", "behavior_replay_seconds", "learner_update_seconds",
            "checkpoint_io_seconds", "update_elapsed_seconds", "physical_decisions", "policy_substeps")
    blocks = []
    for b in desktop["blocks"]:
        blocks.append({k: b[k] for k in ("run", "block", "host", "updates", "games", "dispatch_seconds",
                       "execution_seconds", "native_logical_bytes", "recovery_bytes", "scheduler", "update")})
    for b in haley["blocks"]:
        number = int(b["block"][1:3])
        if b["run"] == "r1":
            meta = r1["blocks"][f"b{number:02}"]
            assert meta["updates"] == b["updates"]
            u = meta["update_stage_sums"]
        else:
            meta = detail[b["run"], f"b{number:02}"]
            assert meta["iterations"] == b["updates"] and not meta["errors"]
            u = {k: meta["sums"][k] for k in keys}
        blocks.append({"run": b["run"], "block": number, "host": "computehost", "updates": b["updates"],
                       "games": b["completed_games"], "dispatch_seconds": b["report_seconds"],
                       "execution_seconds": b["execution_seconds"], "native_logical_bytes": b["native_logical_bytes"],
                       "recovery_bytes": b["recovery_bytes"], "scheduler": b["scheduler_sums"], "update": u})
    assert {(b["run"], b["block"]) for b in blocks} == {(f"r{i}", j) for i in range(1, 7) for j in range(1, 21)}
    assert len(blocks) == 120 and sum(b["updates"] for b in blocks) == 19440
    assert sum(b["games"] for b in blocks) == 194400
    for b in blocks:
        s, u = b["scheduler"], b["update"]
        s["scheduler_other_seconds"] = s["iteration_wall_seconds"] - sum(s[k] for k in (
            "collection_execution_seconds", "update_execution_seconds", "collection_validation_seconds", "update_validation_seconds"))
        u["update_other_seconds"] = u["update_elapsed_seconds"] - sum(u[k] for k in keys[:4])
    summaries = {}
    for host in ("all", "desktop", "computehost"):
        selected = [b for b in blocks if host == "all" or b["host"] == host]
        s = add([b["scheduler"] for b in selected])
        u = add([b["update"] for b in selected])
        total = sum(b["dispatch_seconds"] for b in selected)
        execution = sum(b["execution_seconds"] for b in selected)
        exclusive = {
            "GAE learner": u["learner_update_seconds"],
            "Post-execution validation/storage": total-execution-archive_seconds[host],
            "Scheduler validation": s["collection_validation_seconds"]+s["update_validation_seconds"],
            "Collection": s["collection_execution_seconds"],
            "Recovery archive/readback": archive_seconds[host],
            "Checkpoint and readback": u["checkpoint_io_seconds"],
            "Input reading/setup": u["input_read_seconds"],
            "Outside iteration timers": execution-s["iteration_wall_seconds"],
            "Behavior replay": u["behavior_replay_seconds"],
        }
        exclusive["Other boundaries"] = total - sum(exclusive.values())
        assert all(v >= 0 for v in exclusive.values())
        assert abs(sum(exclusive.values())-total) < 1e-6
        summaries[host] = {"blocks": len(selected), "updates": sum(b["updates"] for b in selected),
                           "games": sum(b["games"] for b in selected), "dispatch_process_seconds": total,
                           "execution_process_seconds": execution, "scheduler_seconds": s, "update_seconds": u,
                           "exclusive_seconds": exclusive,
                           "exclusive_percent": {k: 100*v/total for k,v in exclusive.items()},
                           "component_2x_speedup": {k: 1/(1-0.5*v/total) for k,v in exclusive.items()},
                           "elimination_ceiling_speedup": {k: 1/(1-v/total) for k,v in exclusive.items()},
                           "native_logical_bytes": sum(b["native_logical_bytes"] for b in selected),
                           "recovery_bytes": sum(b["recovery_bytes"] for b in selected)}
    inputs = [{"path": name, "sha256": hashlib.sha256((ROOT/name).read_bytes()).hexdigest()}
              for name in ("desktop-profile.json", "haley-block-stage-summary.json", "haley-update-collection-breakdown.json", "local-r1-stages.json", "context.json")]
    result = {"schema": "training-throughput-audit-combined/v1", "inputs": inputs,
              "accounting": "Exclusive process-wall attribution; not fleet calendar wall, CPU service time or physical I/O bytes.",
              "summary": summaries, "blocks": sorted(blocks, key=lambda b: (b["run"], b["block"]))}
    (ROOT/"summary.json").write_text(json.dumps(result, indent=2)+"\n", encoding="utf-8", newline="\n")
    render(summaries)
    print(json.dumps(summaries["all"], indent=2))


def render(summaries):
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    plt.rcParams.update({"font.family": "DejaVu Sans", "font.size": 11})
    fig, (ax, bx) = plt.subplots(1, 2, figsize=(15, 6.8), gridspec_kw={"width_ratios": [1.5, 1]})
    fig.patch.set_facecolor("#f7f8fa")
    for a in (ax, bx):
        a.set_facecolor("#f7f8fa")
        a.spines[["top", "right"]].set_visible(False)
    labels = list(summaries["all"]["exclusive_percent"])
    vals = list(summaries["all"]["exclusive_percent"].values())
    colors = ["#285a8e", "#df8b30", "#6e66a6", "#2f887d"] + ["#9aa6b5"]*6
    bars = ax.barh(labels, vals, color=colors)
    ax.invert_yaxis()
    for bar, v in zip(bars, vals):
        ax.text(v+0.4, bar.get_y()+bar.get_height()/2, f"{v:.1f}%", va="center")
    ax.set_xlim(0, max(vals)+6)
    ax.set_xlabel("Share of summed guarded dispatch time")
    ax.set_title("Current nine-deck CPU training\n120 blocks · 19,440 updates · 194,400 games", loc="left", weight="bold")
    names = labels[:4]
    values = [summaries["all"]["component_2x_speedup"][n] for n in names]
    bx.barh(names, values, color=colors[:4])
    bx.invert_yaxis()
    for i, v in enumerate(values):
        bx.text(v+0.006, i, f"{v:.3f}x", va="center")
    bx.set_xlim(1, max(values)+0.055)
    bx.set_xlabel("Whole-dispatch speedup")
    bx.set_title("If one stage became 2x faster\nOther stages unchanged", loc="left", weight="bold")
    fig.suptitle("Training speed is now limited by learning and artifact processing", fontsize=18, weight="bold", x=0.02, ha="left")
    fig.text(0.02, 0.035, "Measured runtime c69255d3, October 7–8. Bars reconcile to dispatch time; concurrent process seconds are not campaign elapsed time.\nForward/backward/Adam are combined. Archive time is separated from other post-execution work. Projections are not achieved speedups.", fontsize=10, color="#444e5b")
    fig.tight_layout(rect=(0, 0.10, 1, 0.91), w_pad=3)
    fig.savefig(ROOT/"throughput-audit.png", dpi=170, facecolor=fig.get_facecolor())
    plt.close(fig)


if __name__ == "__main__":
    main()
