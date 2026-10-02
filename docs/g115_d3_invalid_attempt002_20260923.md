# D3 attempt002: technically invalid, no strength verdict

September 23, 2026. The frozen local attempt stopped after 3,469.044 seconds. It is **invalid**, neither ADVANCE nor NO-ADVANCE. The pinned collector withheld all strength statistics. No sample expansion, normalization change, discarded failure or formal rerun has been made. D4 cannot be selected from this attempt.

| Execution disposition | Jobs |
|---|---:|
| Assigned | 2,048 |
| Completed native stores | 812 |
| Primary wall-time timeout | 1 |
| Other active jobs interrupted by the stop | 23 |
| Never started | 1,212 |

The primary failure is `c18-r3-s1-search`: Elves mirror, candidate seat 1, seed `6867782919926683558`. The supported launcher enforced its unchanged 1,800-second per-match bound at 1,800.064 seconds, with 1,288.188 sampled CPU seconds and 105,725,952 peak sampled resident bytes. This is a launcher timeout, not a reported typed search/library error or a natural loss. Its output contains only `start.json`; the log is empty. The failing game number, decision/root and engine step were not persisted and are unknown. Source `public_evaluation.rs` publishes match/search records after the whole native call returns, so killing a live call leaves those diagnostics absent. `c18-r4-s1-search` was interrupted at 1,784.917 seconds. All 24 incomplete records and all 1,212 unstarted conditions remain in the collector's retained panel accounting.

There is a second independent validity failure: **43 baseline semantic mismatches among 418 completed baseline comparisons**. All 418 raw hashes differ, as expected from the two predeclared representation changes, but only 375 become equal after exactly that normalization. The other 606 baseline conditions have no completed store. The 43 discrepancies break down as follows:

| Differing top-level fields | Records |
|---|---:|
| `decision_count` | 16 |
| `decision_count`, `sideboard_decisions` | 19 |
| `decision_count`, `sideboard_decisions`, `v3_forced_actions` | 6 |
| `sideboard_decisions` | 1 |
| `decision_count`, `v3_forced_actions` | 1 |

These are not the permitted terminal-audit or V3 build-provenance differences. For example, `c16-r5-s1-baseline` has 214 archived decisions versus 177 current decisions. Sideboard-input evidence also changes in other records; the discrepancy cannot be dismissed as a renamed counter. No wider normalization is admitted.

The archive's start receipt pins native source `5c429a3f408ce390878d5a63d80285da7ace3836`; this attempt pins `cd41885e0ac05586d89bd4b2b7fb1284248689ef`. Their live engine diff includes the intervening trample correction `2b3bc6bf`, as well as D2's equivalent pending-choice projection extraction. The trample correction is a concrete candidate explanation for trajectory drift, especially in the affected Elves conditions, but attribution of all 43 records is **not yet established**. Its existing result note explicitly warns against treating corrected behavior as byte-identical to old-engine results. Neither restoring an engine defect nor excluding affected matchups is authorized by this report.

The R3/R4 distinction needs a reviewed disposition: a workload exceeding a correctly enforced time limit is not yet established as an environmental interruption, and baseline drift independently prevents declaring a clean environmental restart. Read-only source and retained-artifact diagnosis continues. Before any further formal attempt, resolve the baseline behavior discrepancy and qualify a feasible, reviewed execution bound without changing S128/T1024/depth8, models, seeds, rewards or the statistical rule. Preserve the entire attempt and the earlier zero-result setup void. Any corrected formal attempt still needs all 2,048 jobs in fresh roots and the required review; this report grants no launch authority.

Evidence:

- Frozen manifest: `D:/g115-d3-formal-binding-cd418-20260923-attempt002/launch-final-local-001.json`, SHA-256 `8d2b9e5dfb1af0a449b8fc1205b10e1c3daad16f9619e57e33be52e52b1b2eeb`.
- Terminal shard receipt: `attempt002/local-shard/completion.json`, SHA-256 `03c66f18556283097706fec4a9d7c695f6059f6706a1046f4615fffb31de9830`. Handle33870 is terminal, exit1. Its stale progress file is not live-process evidence.
- Pinned collection: `E:/mtg-g115-lineage-20260923/d3-formal-result-attempt002/analysis.json`, SHA-256 `50f562871c589ad59e64d439b1fbc90659ad3de5a19a3b22442a17e55256eee8`; `complete:false`, `formal_verdict:null`, no `statistics` field. Collector handle12503 completed successfully as an invalidity collection, not a valid measurement.
- Recovery mapping: `E:/mtg-g115-lineage-20260923/d3-recovery-attempt002.json`. All original native stores, requests and execution receipts remain at their original paths.
- Cost accounting: `docs/g115_d3_compute_costs_20260923.md`, commit `a481fe2b`. This formal attempt incurred no cloud compute charge. All previous D3 cloud attempts remain included in the approximately $4.81-$5.01 quoted-window estimate, not an invoice; every owned Pod was released.

The predeclared n=1,024 per arm, 512 seed clusters and 80% MDE=3.0 pp are planning quantities. They cannot be applied to this incomplete invalid subset. No win counts, effect estimate, confidence interval, closure or ExIt selection is reported. D1/D2 remain delivered; D3/D4 remain unfinished.
