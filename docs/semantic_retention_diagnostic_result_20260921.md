# Removing retention does not resolve the semantic-control fit failure

The fixed training-only ablation completed: the zero-beta semantic learner still fits **16/32** labels after 32 updates, exactly the count from the beta-0.5 control. Both choose self on every training position. Removing retention alone does not resolve the failure under this budget and initial optimizer state. This does not establish an architectural impossibility or an optimization plateau.

The design was frozen before the new endpoint in `docs/semantic_retention_diagnostic_plan_20260921.md`. Same g115 parent, all optimizer moments, initial Adam age 32,400, actor-relative semantic labels, 32 full-batch updates and learning rate 0.0001. One new zero-beta learner ran. The completed beta-0.5 endpoint and its verified training scores were reused after exact shared-code state checks. Neither the consumed 48-position tactical panel nor the 100-game retention panel was read by this diagnostic.

| Training measure | Original g115 | Semantic beta 0 | Semantic beta 0.5 |
|---|---:|---:|---:|
| Own semantic targets fitted | 0/32 | 16/32 | 16/32 |
| Face-family control targets | 0/16 | 16/16 | 16/16 |
| Creature-family control targets | 0/16 | 0/16 | 0/16 |
| Mean raw-logit cross-entropy | 23.450706 | 0.638734 | 0.639715 |
| Argmax choice across 32 inputs | Object, all 32 | Self, all 32 | Self, all 32 |

The control labels deliberately mean self in the face family and opponent in the creature family. They are imitation labels, not actual gameplay rewards. Every seat/family contains eight cases, and all paired-seat counts agree. The final zero-beta model's mean opponent probability on the failed creature family is 0.461352, versus 0.461540 with retention. Its positive self-minus-opponent logit margins range from 0.092119 to 0.258581 in that family. There is no successful fit hidden by a seat/index discrepancy.

The retained and unretained semantic training curves are also close:

| Before update | Beta 0 CE | Beta 0.5 CE |
|---|---:|---:|
| 1 | 23.450705 | 23.450705 |
| 4 | 7.716992 | 7.717609 |
| 8 | 0.882860 | 0.883647 |
| 16 | 0.675712 | 0.675734 |
| 24 | 0.663082 | 0.662459 |
| 28 | 0.654211 | 0.653844 |
| 32 | 0.642226 | 0.642832 |

The first table scores **after** update 32; the training-loop losses above score **before** each listed update. Loss is still declining at the endpoint, so a plateau or futility conclusion would be unwarranted. Cross-entropy uses raw logits, whereas reported action probabilities use the production Hamilton sampler and its transport behavior; those quantities are not interchangeable.

## Verification and execution

Source **e8ea75b0** adds an explicit `SemanticDiagnostic` mode to the existing shared training implementation. The diagnostic uses its own checkpoint schema `terminal-semantic-retention-diagnostic/v1`, loss identity `terminal-semantic-ce-retention-ablation/v1`, and qualification schema `semantic-retention-diagnostic-compute/v1`. Its artifacts cannot satisfy the earlier final-evaluation checkpoint contract. The ordinary retained campaign's numerical path and contracts remain unchanged.

Engineering evidence at `E:/mtg-meta-recovery-20260921/semantic-retention-diagnostic-compute-001/engineering/completion.json` verifies:

- Diagnostic beta-0.5 state after two updates exactly matches the original semantic control's saved parameters, both optimizer moments, age, gauge and state hash.
- The zero-beta first update exactly matches the same original first update, as expected when parent KL starts at zero.
- One-worker continuous and four-worker resumed two-update checkpoints are byte-identical, with identical final training scores.
- Native requests reject missing substantial-throughput qualification, wrong-arm resume, diagnostic arms through the ordinary mode, and an unnecessary 32-update retained-baseline rerun.
- The same two-update zero-beta checkpoints and training scores are exact on both PCs with one and four native workers.

The full new run completed all 32 updates and recovered all 32 checkpoint files with zero mismatches. Final Adam age is 32,432, initial state hash `8139016ca561961714f25e22a9d6f7fc888548fc332e45b6bce402dfc43159f2`, final state hash **74491c251397e9463b67a97522524fdc416040080615db1293eadf1d9a2a2404**. The analyzer independently recomputes target cross-entropy and argmax from the saved raw logits. Its source was hashed before the new endpoint was produced.

Supported launcher: `E:/mtg-meta-recovery-20260921/semantic-retention-diagnostic.py`. The substantial path requires fresh, matching completed-work qualification and revalidates frozen baseline and dependency pins. Both launcher and native diagnostic guard the 32-update launch. Bounded one/two-update engineering remains possible; these are not OS-level restrictions.

| Placement | Forecast complete turnaround |
|---|---:|
| Jack, one native worker | 16.28 s |
| **Jack, four native workers** | **15.59 s** |
| HaleysPC, one native worker | 63.44 s |
| HaleysPC, four native workers | 62.39 s |

Actual selected run: 2.831 seconds staging, 7.300 seconds execution, 1.631 seconds recovery, **11.761 seconds total**, excluding separate ownership checks. Remote preparation cost 2.925 seconds. Native build cost 150.992 seconds, with four BelowNormal Cargo jobs and E-drive target/temp storage. Local execution/checkpoints used the D SSD with recovered evidence on E. Both hosts' CPU, storage, memory, GPU and competing-process inventories were checked. There is one new sequential learner with four deterministic local backward partitions; a two-host gradient split is not implemented, and the matched baseline was already complete. No duplicate learner was launched to create occupancy. The new diagnostic has no qualified CUDA execution path, so no GPU speedup is claimed. RunPod inventory returned HTTP403 on September 21 at 20:21:59 UTC; no allocation or spending occurred.

## Disposition and next causal question

The original retained comparison remains **no-advance**, with the semantic-control comparison inconclusive. This diagnostic does not modify its gate, training budget, endpoint or validation result. No diagnostic model is eligible for deployment, promotion or the human preview.

The result removes retention as the sole explanation for inadequate fit at 32 updates. It leaves the amount of optimization and the network's state-conditioned player-target discrimination unresolved. Since the training loss is still declining, the next useful bounded question is whether a fixed longer **training-only** budget fits these same labels without changing the optimizer or architecture. Predeclare that budget and comparison separately, keep evaluation panels closed, and measure training adequacy before designing any new strength experiment. Do not label this run a plateau or automatically start a broad campaign.

Independent Fable review remains unavailable under the known zero-source-read HTTP429 until September 22 at 07:00 EDT; it was not retried or counted as endorsement. The residual review gap remains explicit under Jack's research authority. CP7 information is excluded. Human/league competence, current-engine match strength and generalization beyond these fixtures remain unproven.

Evidence roots: `E:/mtg-meta-recovery-20260921/semantic-retention-diagnostic-001` and `semantic-retention-diagnostic-compute-001`. Key receipts are `plan.json`, `analysis-before-launch.json`, `completion.json`, `analysis.json`, `choice.json`, `engineering/completion.json` and `after-launch.json`. The training binary SHA is `1bed6d1f498e593d293daac2a760ca356f2a13e81f71a31bec47625d491a25ce`. Frozen prior outputs and the seven idle human sessions remain preserved. The heartbeat remains paused.
