# Public stack screen: NO-ADVANCE

The completed frozen comparison does not demonstrate a benefit from stable explicit stack attributes over its trained controls. All 512 jobs and 4,096 BO3 matches passed verification, covering 9,034 natural evaluation games. There are no missing cases, failed jobs, draws or recovery hash mismatches. The formal controller session10415 ended with exit0 and was reaped; both PCs had no active native evaluator at the final inventory.

| Endpoint | BO3 wins / 1,024 | Win rate | Game-one wins / 1,024 |
| --- | ---: | ---: | ---: |
| g115 | 802 | 78.32% | 783 |
| disabled | 837 | 81.74% | 813 |
| permuted | 848 | 82.81% | 818 |
| structured | 840 | 82.03% | 825 |

The original g115 endpoint is an untouched anchor. Disabled is the matched continued-training control. Permuted retains the feature multiset and instance binding while changing column meanings per decision; it is not a no-information control.

| Structured minus reference | Net BO3 wins | Paired 95% interval, percentage points |
| --- | ---: | ---: |
| g115 | +38 | [+2.34, +5.18] |
| disabled | +3 | [-0.98, +1.56] |
| permuted | -8 | [-2.25, +0.68] |

The primary gate required at least 20 net wins and a strictly positive paired lower bound against each trained control. Both comparisons fail. The breadth gate also fails: structured is nonnegative on 6/8 own decks against g115, 6/8 against disabled, and only 3/8 against permuted. Maximum deck-loss retention and game-one retention pass. Passing secondary checks does not rescue a failed primary comparison. No checkpoint is promoted and no winning arm is selected after inspecting this panel.

## Breadth and seats

| Own deck, 128 BO3 each | g115 | Disabled | Permuted | Structured |
| --- | ---: | ---: | ---: | ---: |
| Affinity | 115 | 114 | 113 | 115 |
| Burn | 103 | 102 | 101 | 96 |
| Elves | 107 | 108 | 109 | 106 |
| Faeries | 97 | 98 | 99 | 98 |
| Rally | 116 | 116 | 116 | 117 |
| Terror | 114 | 112 | 115 | 114 |
| Wildfire | 98 | 100 | 104 | 103 |
| published-44ae71e1e126b63d | 52 | 87 | 91 | 91 |

The published-deck slot contributes +39 of the overall +38 structured-versus-g115 wins. The other seven decks combined contribute -1. Disabled and permuted also improve strongly in that slot, so the anchor comparison cannot isolate a stack-feature benefit. Burn loses 7/128 to g115 and 6/128 to disabled. These are descriptive findings from the complete panel, not new selection gates.

| Physical seat, 512 BO3 each | g115 | Disabled | Permuted | Structured |
| --- | ---: | ---: | ---: | ---: |
| 0 | 407 | 425 | 433 | 426 |
| 1 | 395 | 412 | 415 | 414 |

All 128 ordered-matchup/physical-seat rows are preserved in analysis.json, with eight seed observations per endpoint per row. These cells are too small for reliable individual matchup conclusions. Physical seat is distinct from game-one starting-player assignment. Bootstrap resampling preserves both seats and all endpoints within each seed cluster, stratified by ordered matchup. Intervals are conditional on one training stream and one development opponent; they do not estimate training-seed variability. Per-deck intervals are descriptive, not simultaneous guarantees. KeepSeven and fixed sideboard maps remain limitations.

## Completed work and cost

Training completed 6,000 natural games across three arms in 1,221.60 seconds (20.36 minutes), including verified recovery and audit. Training archives contain 7,846 files, 39,572,584,162 raw bytes and 15,462,472,338 compressed bytes, with zero hash mismatches. GPU0 on the maintainer's remained reserved; no paid compute was launched.

The final panel used desktop 24 CPU workers and the compute host 16, equal job shares, through the supported guarded launcher. Dispatch measured 11.85 seconds staging, 161.11 native group wall time and 43.65 recovery, totaling 216.61 seconds (3.61 minutes). This excludes pre-dispatch guard validation. Parent creation to completion receipt was about 4.17 minutes. Host worker times were 92.26 seconds desktop and 159.08 seconds compute host and overlap; do not sum them. Recovered files: The maintainer 3,843, the compute host 3,844, zero mismatches.

The prelaunch forecast was 666.92 seconds (11.12 minutes). The full queue amortized the timing sample's long-job tail better than the 16x extrapolation. The 1:1 allocation was the fastest measured qualified choice, not a demonstrated production optimum: The maintainer's finished its fixed share sooner. Preserve this completed frozen run. A future run should reuse compatible evidence and qualify host balance using complete-queue throughput, including qualification overhead. No mid-run redistribution or changed scientific inputs occurred.

Allocation qualification itself executed 2,050 BO3 repetitions of 256 unique timing cases and verified 1,792 cross-allocation match hashes. Those executions are engineering costs, not additional scientific observations. Seven timings were preserved when the first controller hit its new-launch budget; only the missing eighth allocation ran in the continuation. Copied recovery fixtures add no native games. The extensive one-time qualification took longer than the final panel and must not become repeated overhead without a material reason.

## Disposition and remaining work

Preserve this NO-ADVANCE result and the current incumbent. Do not extend this stack candidate merely because it beats the untouched anchor. The next low-cost diagnostic is a read-only audit of existing captured training trajectories: quantify how often kicked, flashback, X and mode distinctions were actually present at meaningful decisions, and inspect whether the structured projection changed logits on those decisions. Predeclare that diagnostic before inspecting its derived values. This can distinguish scarce exposure or weak feature use from a broader failure without launching more training or reusing evaluation cells as a holdout. It cannot establish a new win-rate benefit. Any later causal intervention still needs matched controls, frozen gates and fresh evaluation evidence.

Fable review 876765fa-0834-4f03-8db1-6b1f99300885 failed HTTP429 with zero source reads until September22 07:00EDT. No retry or endorsement is claimed. Codex proceeds under the maintainer's explicit research authority with this independent-review gap unresolved. This result is the predeclared gate outcome, not a Fable-reviewed conclusion about the value of stack information in general.

CP7 outcomes were not used for selection. Kimi's separate Escape menu gate remains unadopted. No human-match calibration, independent competent-opponent evidence or online league-strength claim follows from this study. The broad research objective remains active.

Authoritative evidence:

- [Frozen analysis](E:/mtg-postboard-campaign-20260921/public-stack-screen-001/analysis.json)
- [Panel completion](E:/mtg-meta-recovery-20260921/public-stack-panel-001/completion.json)
- [Native dispatch and timing](E:/mtg-meta-recovery-20260921/public-stack-panel-001/full-panel/result.json)
- [Allocation qualification](E:/mtg-meta-recovery-20260921/public-stack-panel-compute-002/qualification.json)
