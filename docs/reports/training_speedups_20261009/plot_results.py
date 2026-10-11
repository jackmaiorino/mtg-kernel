"""Plot completed receipt timings. Requires Matplotlib; performs no workload."""
import argparse
import json
from pathlib import Path
from statistics import mean

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt


ROOT = Path(__file__).resolve().parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--preview", type=Path, help="Optional PNG preview destination")
args = parser.parse_args()
result = json.loads((ROOT / "desktop/formal-v4-analysis.json").read_bytes())
assert result["complete"] and result["full_fingerprint_parity"]
cases = result["cases"]
plt.rcParams.update({"font.family": "DejaVu Sans", "font.size": 10,
                     "svg.fonttype": "none", "svg.hashsalt": "training-v4"})
fig, axes = plt.subplots(1, 2, figsize=(13.4, 6.8), gridspec_kw={"width_ratios": [1.6, 1]})
fig.subplots_adjust(left=.11, right=.98, top=.78, bottom=.30, wspace=.30)
fig.suptitle("Exact CPU training: all four matched ABBA cases", x=.11, ha="left", y=.98, fontsize=19, weight="bold")
fig.text(.11, .88, "162 updates / 1,620 games per case  |  Identical full output fingerprints  |  CPU only", fontsize=11)
phases = [("dispatch_seconds", "Dispatch", "#255f85"),
          ("inspect_seconds", "Inspect", "#4a9b8e"),
          ("transport_seconds", "Recovery copy", "#99bf73"),
          ("retain_seconds", "Retain", "#d9c773"),
          ("late_copy_seconds", "Late metadata", "#d87949")]
left = [0.] * 4
for key, label, color in phases:
    values = [c[key] / 60 for c in cases]
    axes[0].barh(range(4), values, left=left, height=.60, color=color, label=label)
    left = [x + y for x, y in zip(left, values)]
for i, total in enumerate(left):
    axes[0].text(total + .6, i, f"{total:.2f}", va="center", fontsize=10)
axes[0].set_yticks(range(4), ["A1 baseline", "B1 candidate", "B2 candidate", "A2 baseline"])
axes[0].invert_yaxis()
axes[0].set_xlim(0, 59)
axes[0].set_xlabel("Completed phases (minutes)")
axes[0].set_title("Recovery latency dominates the first baseline", loc="left", fontsize=12, pad=14)
axes[0].legend(loc="upper left", bbox_to_anchor=(0, -.21), ncol=3, frameon=False, fontsize=9)

groups = [[c for c in cases if c["variant"] == name] for name in ("baseline", "candidate")]
learner = [("input_read_seconds", "Input read", "#7698ae"),
           ("behavior_replay_seconds", "Replay", "#adb2b5"),
           ("learner_update_seconds", "Arithmetic", "#735995"),
           ("checkpoint_io_seconds", "Checkpoint I/O", "#4a9b8e")]
left = [0., 0.]
for key, label, color in learner:
    values = [mean(c["learner_nested_seconds"][key] for c in group) for group in groups]
    axes[1].barh(range(2), values, left=left, height=.48, color=color, label=label)
    left = [x + y for x, y in zip(left, values)]
totals = [mean(c["learner_nested_seconds"]["update_elapsed_seconds"] for c in group) for group in groups]
axes[1].barh(range(2), [total - used for total, used in zip(totals, left)], left=left, height=.48, color="#eeeeee", label="Other")
for i, total in enumerate(totals):
    axes[1].text(total + 4, i, f"{total:.1f}", va="center", fontsize=10)
axes[1].set_yticks(range(2), ["Baseline", "Candidate"])
axes[1].invert_yaxis()
axes[1].set_xlim(0, 385)
axes[1].set_xlabel("Mean learner update wall (seconds)")
axes[1].set_title("Learner arithmetic: 218.2 to 74.3 seconds", loc="left", fontsize=12, pad=14)
axes[1].legend(loc="upper left", bbox_to_anchor=(0, -.21), ncol=2, frameon=False, fontsize=9)
for ax in axes:
    ax.spines[["top", "right", "left"]].set_visible(False)
    ax.tick_params(axis="y", length=0)
    ax.set_axisbelow(True)
    ax.grid(axis="x", alpha=.15)
fig.text(.11, .045, "Observed completed-phase ratio: 2.49x, strongly affected by recovery variability. Dispatch: 1.35x.\nAll cases retained; learner timings are nested inside dispatch and must not be added to the left chart.", fontsize=10, color="#40464b")
svg = ROOT / "formal-v4-timing.svg"
fig.savefig(svg, metadata={"Date": None})
svg.write_text("\n".join(line.rstrip() for line in svg.read_text(encoding="utf-8").splitlines()) + "\n", encoding="utf-8", newline="\n")
if args.preview:
    fig.savefig(args.preview, dpi=150)
