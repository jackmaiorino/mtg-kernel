# Entropy telemetry check and F1 evidence scope

No training, game collection, candidate selection or gate was run. The bounded artifact check reads two fixed episodes per six existing control-variance training runs: update0/199, slot0, learner rows only. Each consumed member's SHA-256 matches its archive manifest. The whole 4GB zip shards were not rehashed. Twelve members total108,411,115 uncompressed bytes. Serial1.436s and four-reader1.305s results agree exactly; cache order and the tiny workload prevent treating that difference as fleet throughput qualification. Reader: python/tools/read_control_entropy_snapshot_v1.py; result E:/mtg-meta-recovery-20260921/control-entropy-snapshot-001.json.

| Run | Choices early / late | Mean entropy nats early / late | Mean H/log(menu size) early / late |
| --- | --- | --- | --- |
| r1-a | 179 / 288 | .2298 / .1605 | .1926 / .1172 |
| r1-b | 67 / 176 | .1340 / .2365 | .0918 / .1597 |
| r2-a | 139 / 110 | .2205 / .1878 | .1405 / .1044 |
| r2-b | 57 / 183 | .1697 / .1921 | .0813 / .1293 |
| r3-a | 141 / 292 | .1907 / .1773 | .1206 / .1195 |
| r3-b | 73 / 303 | .0728 / .0920 | .0518 / .0747 |

Metric is float64 softmax entropy from archived f32 logits at visited prefixes, excluding one-action menus. It is not the quantized/clamped execution sampler's entropy or full macro-action entropy. All samples are the published Gates learner: early opponent Affinity, late opponent Wildfire. Opponent and visited-state distributions differ. Three sampled comparisons increase and three decrease; neither these samples nor the between-run win-rate SD diagnoses collapse versus bad luck. No alarm threshold is fitted to these data. Full per-run monitoring still needs all applicable windows with deck/menu/seat coverage and the exact sampling metric labeled. The standard receipt currently contains no policy entropy.

**Outcome correction for the monitor:** entropy beta=.05 remains closed/NO-ADVANCE, but replica1's paired interval is [-2.83,+.49]pp; replica2 is [-6.35,-2.73]pp. Both point estimates are negative, not both intervals strictly below zero. Source public_entropy_result_20260921.md and entropy-joint-analysis-001/REPORT.md. No failed gate is reopened.

**F1 scope:** recounting the original pinned-audit rows in E:/mtg-postboard-campaign-20260920/embedding-exposure-audit-001/audit.json gives192 registry cards,90 unchanged g115 rows,91 distinct original-seven-mainboard cards, and **zero unchanged rows among those91**. This is a recount of the archived audit, not a new checkpoint tensor comparison. Its source report already limits unchanged rows to "no retained embedding update," not proof of no observation or inability to read a fixed vector. The 44/60 Gates-heavy and37/60 Spy untrained-copy counts concern additional lists at the original checkpoint.

Current flat_policy_v2.rs:2278 routes own-hand objects through add_private_card; native_flat_tensorizer_v2.rs:2166 rejects nonzero detailed characteristics on identity-only rows. Public-cost rows are separately looked up from visible tokens (public_cost_features_v1.rs:150 onwards) and provide printed cost structure, not complete rules/keyword semantics. Semantic hand generalization remains a plausible intervention. It is not an information collision between all distinct hand identities, and the downstream network can learn to use a fixed embedding.

Broader exposure was already tried: qualification moved35 formerly unchanged mainboard rows; its completed development pilot gained34/896 G1 wins against continuation control (+3.79pp [1.67,5.92]) but failed canonical retention versus g115 (-2.81pp [-5.36,-.26]). It is NO-ADVANCE and must not be reintroduced as an untested recipe. Before F1 implementation, require a concrete hand-dependent legal-action comparison with known tactical ground truth, then demonstrate what the existing input/policy misses. Preserve unfamiliar-card generalization and whole-match retention as separate questions. No new hand feature or learning objective was chosen from this audit.
