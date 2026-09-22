# V4 leaf evaluator engineering, 2026-09-22

Initial controls: 2 passed, 0 failed in controls-002; build and checks took 271.909 seconds. All three source hashes match that receipt. Evidence: `E:/mtg-meta-recovery-20260921/v4-search-leaf-controls-002`. The earlier controls-001 compile failure (nonexistent tensorizer constructor) is preserved. This is not S0 acceptance, a strength gate, or M1 progress in win rate.

Fable source followup completed successfully in `v4-search-leaf-source-review-001`, session `774344c2-35a3-497d-b551-136050e1791e`, with actual source reads. Verdict: proceed with bounded engineering; changes required before acceptance. Accepted findings: normalize and verify MXCSR at constructor entry, use the named terminal error, expand fixture coverage across V4 extensions and both seats, exercise hidden-state and call-order controls, and replay pinned g115 with measured deterministic-tanh versus ordinary-tanh differences. Do not assume output equality from tensor equality.

The constructor accepts only the plain frozen V4 policy, not public-input or stack-input wrapper types. Future integration must use the collector's loader and retain that restriction. The evaluator has no playing caller. Existing disabled-search guards remain intact. Neither the initial tests nor the review authorize formal measurement.
