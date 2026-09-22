# V4 key and consume controls

2026-09-22. Engineering only, no strength gate or model update. Source base ca454b34 plus pinned additive files. `E:/mtg-meta-recovery-20260921/v4-key-step-controls-001` passed 17/17 selected release tests, zero failures, 0.09s test execution, 275.323s full invocation. All four source SHA256s matched after completion. Four BelowNormal Cargo jobs, ownership-checked helper `check-v4-key-step.py`; no GPU calls or formal games.

Tests cover visible/depth/action-order/substep key sensitivity, transport-field invariance, hidden-state key invariance, V4 library-choice key acceptance, direct-consume parity within each sampled world, equal root authority across clones, invalid index/episode/stale token rejection without mutation, and legacy rejection, alongside existing leaf/clone controls. Synthetic relabeling step failure remains reproduced. No sampled playing route is accepted.

Source review `v4-key-step-source-review-001` accepted implementation, requested three additional controls. Added both-seat key/consume parity, both hidden-trigger key/token equality fixtures, and Natural-terminal consume from the original Hunter fixture. Expanded check `v4-key-step-controls-002` is RUNNING; no pass claim. Later trailing V7-only history sensitivity and second-node consume remain open coverage. Future core must abort and discard on StepFailed or HaltedSimulation. See v4_key_step_review_disposition_20260922.md for actual reviewer feedback and disposition.

Expanded controls002 completed: 17 passed, 1 failed, 0.09s tests, 273.053s invocation, all four source hashes verified. Failure was the Natural-terminal assertion in a changed Hunter fixture: unlike the original witness, the test added cards to P1's library. Restored P0-only library additions for that fixture. This failed engineering check is retained, not a formal gate or discarded replication.

The next check includes the separate whole-object sampler repair, root clone integration and historical-source stepping regression. It is recorded as v4-sampler-controls-001, not a rerun of a strength gate. Key/consume acceptance remains pending these expanded checks.
