# Latest: complete, NO-ADVANCE

Training and the full 4,096-BO3 panel are complete. See public_stack_screen_result_20260921.md for final results, costs and limits. The notes below are chronological snapshots, not current process status.

# Stack screen progress, September21

The frozen design and analyzer were prepared in `E:/mtg-postboard-campaign-20260921/public-stack-screen-001/manifest.json` before training. Four-arm resampling checks verify deterministic draws, preservation of anti-correlated seat pairs, shared endpoint sampling and exact breadth-guard boundaries. The manifest pins the design, analyzer, configs, native binaries, disjoint evaluation seed inputs and512 eight-match endpoint jobs. No formal evaluation has run.

Evaluation splitting passed in `public-stack-chunks-joint-001/result.json`:578 bounded BO3 executions repeating96 engineering cases. All384 within-host split comparisons and96 original cross-host match comparisons are byte-identical. Both physical seats, all three input modes and eight opponents are represented; only six own-deck/seat combinations and small engineering checkpoints were used. This establishes split/replay behavior, not final-endpoint performance qualification.

| Host | 96 split matches, 1 worker | 96 split matches, 8 workers |
| --- | ---: | ---: |
| The maintainer |29.05seconds |6.16seconds |
| The compute host |50.63seconds |12.69seconds |

These are native dispatch wall times; each result also records staging and verified recovery. The maintainer's initial preflight stopped before any native execution because Kimi's Cargo build was active. The compute host's idle machine ran first; the maintainer ran after the build ended. All qualification handles ended successfully and post-case owner checks were clean.

That owner discovery corrected the earlier training timing choice. Kimi's build began13:23:54UTC during original b-w10. The old compute root now contains a pinned revocation and retains every output. `compute_throughput_v3` rejects a revoked choice before launch; the screen's attempted old-choice check created no training launch. `public-stack-compute-002/result.json` replaces only the affected90-game case, verifies108 identical saved learning files, and retains the five prior cases with subsequent clean preflight observations. New post-run checks are also clean. Corrected selected b-w10 projects2,383.13seconds (39.72minutes), including scaled recovery; the former37.06-minute estimate is withdrawn. This is a short-sample forecast, not a completed full-run time.

Full training launched September21 about09:42EDT through `run_public_stack_screen_v1.py` and the supported storage/allocation guard. It runs structured on the compute host GPU0 and queues permuted then disabled on the maintainer GPU1, ten synchronous collectors each. All three start from g115 and run200 updates x10 natural games. The maintainer GPU0 remains reserved. Owned native processes and their requested GPU UUIDs were observed on both hosts; the parent controller is session71632. Exact paths and continuation instructions are in the current RESEARCH-STATE.md handoff. No paid compute, outcome-prefix analysis or formal evaluation has started.

Independent Fable review remains the explicitly recorded zero-read429 gap untilSeptember22 07:00EDT. A screen pass permits consideration of independent replication only. Training completion, a development win-rate improvement, human competitiveness and league readiness are separate claims; none is asserted here.

## Audited training completion, 10:02 EDT

`training-audit.json` now passes:6,000 natural games,200 updates for each arm, imported Adam age32600 and stack-projection age200, with final endpoint/checkpoint links verified. Session71632 ended with exit0; all owned trainer processes ended. End-to-end elapsed time including recovery and audit was1,221.60seconds (20.36minutes). Native times were716.39seconds structured on the compute host,417.04seconds permuted and413.60seconds disabled queued on the maintainer. These overlap across hosts and must not be summed as elapsed time.

Canonical archives cover7,846 files,39,572,584,162 uncompressed bytes and15,462,472,338 compressed bytes, with zero readback mismatches. Archive construction and verification took225.76seconds. Recovery accounting across hosts can overlap native work on the other host; it is not an additional disjoint wall-time phase. The39.72-minute prelaunch forecast was conservative for this realized workload, not a missed target.

The final-checkpoint panel is prepared at `E:/mtg-meta-recovery-20260921/public-stack-panel-001/plan.json`. Its bounded compute qualification is running under session16221 in `public-stack-panel-compute-001`. The full4,096-match panel has not started and no playing-strength result exists yet. Keep the frozen analyzer and final endpoints unchanged.
