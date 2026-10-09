# Prevention-state learning qualification

The isolated prevention projection learns from natural terminal-reward games and preserves deterministic collection and continuation. This is engineering evidence, not improved playing strength. Keep g115 as the reference; no endpoint is promoted.

Source `1b757e49` adds explicit `projection_mode: state_only`. CUDA receives zero cost-feature rows, and checkpoint loading, resume and post-update checks reject nonzero cost weights or moments. The control also disables state inputs. Omitted/default `all` retains the previous serialized configuration and hash. Frozen binaries and measurements are unchanged.

Evidence roots:

- `E:/mtg-postboard-campaign-20260920/public-state-only-engineering-001`
- `E:/mtg-meta-recovery-20260920/public-state-only-tools-001`

Both arms start from untouched g115, full Adam step32400 and public step0. Each runs four updates of two games from the existing exposed engineering schedule, including Caw-Gates/Strands and canonical cases, both learner seats. A48, seeds, V4, terminal rewards, GAE1/.9, learning rate.0001 and value coefficient.5 remain fixed. There are16 distinct arm-games and40 executed games including replay, all naturally terminal. No CP7 outcomes or winning-case selection are involved.

| Check | Observed result |
|---|---|
| Active prevention exposure, each arm | 86 learner substeps in3/8 games |
| Treatment state weights / first moments / second moments | 248/384 nonzero in each array |
| Treatment cost weights and both moments | All zero |
| Control public weights and both moments | All zero |
| Serial versus two collectors, both arms | All saved outputs byte-identical |
| Fresh process resume, one to two collectors | All saved outputs byte-identical |
| Exact checkpoint/optimizer/trajectory comparisons | 48 |
| First collected batch, treatment versus control | Identical except declared config/input identity |
| Actual V4 active-prevention hidden-state invariance | Pass, both physical seats |
| New checkpoint CPU behavior replay | 256 sampled rows exactly reproduce saved float bits |
| Old checkpoint compatibility | 13 legacy replay output files byte-identical |
| Nonzero cost weight in state-only checkpoint | Actual inference loader rejects before producing output |

The CPU/CUDA forward envelope is checked by every real training update. The hidden-state test changes the opponent hand and both library orders, verifies legal selection, deterministic replay, nonzero state contribution and exact disabled-input control, without stepping the session. Two additional tests cover configuration serialization and forbidden cost parameters/moments. The CUDA release build passed with four BelowNormal jobs in156.99seconds, Rust/Cargo1.94.1, installed MSVC14.50.35725.0. Binary hashes and commands are in `build-completion.json`.

Exposure by feature is W10, U0, B21, R37, G46, cannot-prevent0 per arm; colors can overlap within a substep. Treatment nonzero weight counts by feature are56,0,64,64,64,0. Neither blue prevention nor cannot-prevent learning was exercised. This does not qualify all six flags under natural learning.

The saved-decision audit also establishes an actual learned contribution. Immediately before the fourth update the base models are identical. Across256 sampled rows, the learned projection changes logits or value on28/28 active rows and0/228 inactive rows. Quantized probabilities change on4 active rows, maximum total variation0.00001166, with no top-action changes. The two source archives repeat states, so these are not independent samples, evidence of better play, or a formal effect size.

| Four-update process | Serial seconds | Two collectors seconds |
|---|---:|---:|
| Control | 33.41 | 31.86 |
| Treatment | 31.07 | 28.71 |

Total native training process time including prefix/restart was168.01seconds. This tiny two-game batch supports at most two useful concurrent collectors and shows a modest local reduction. It is not representative qualification for a larger campaign, not a cross-host comparison, and no production compute choice was issued. The initial update took17.39seconds, passing the frozen180second projected four-update timing cap. Every child also stayed within the180second process and600second total bounds.

Two test-construction mistakes were corrected without changing product validation or repeating training. The first hidden-state test tried a noncanonical zeroed public-card description; the validator correctly rejected it, and the test now requires that rejection. The first corrupted-checkpoint audit request supplied only one model and failed the audit's two-model minimum before loading. A preserved second request retains two models and reaches the intended masked-cost rejection. The first request is not counted as proof of checkpoint rejection. See `saved-replay-verification.json` and `masked-cost-rejected-002.execution.json`.

Next: qualify actual eligible placement, including the GPU0-only compute host host, using explicit execution-device handling rather than silently altering a scientific config. Then prepare a matched control/state-only pilot with actual prevention coverage in both arms and canonical retention, fresh seeds, fixed analysis gates and complete terminal BO3 panels. Do not repeat the prior combined cost-feature pilot or widen its failed gates. Broader opponents, learned openings/sideboards, human interface coverage and games against the maintainer's remain unresolved.

Independent Fable review remains unavailable after the known zero-read HTTP429 until September22 07:00EDT. This reversible engineering proceeds under the maintainer's execution assignment with that review gap, not an endorsement. No paid compute, formal strength evaluation or human/league-competence claim follows from this qualification.
