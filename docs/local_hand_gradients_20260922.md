# Frozen input gradients and tanh activation read

Read-only diagnostic of g115 on11 fixed captured roots: two constructed hand controls, eight earlier natural channel roots, one natural Bolt candidate. Protocol collab/lit/20260922-local-attribution-protocol.md posted before measurement. No parameter update, optimizer, candidate or strength gate. M1 unmet.

Loaded all checkpoint parameter bits into the repository's CPU Torch reference model, strict parameter matching. Every logit and value passes the declared native-golden2e-5 absolute plus2e-5 relative tolerance; maximum logit error5.7220459e-6. Differentiated continuous looked-up object embeddings and state inputs, never categorical token IDs. Targets fixed before gradients: face-minus-removal for constructed controls, face-minus-selected for natural Bolt, archived top-minus-runner-up for the eight unlabeled roots.

| Root | Logit contrast | Hand embedding gradient L2 | Digest gradient L2 | Signed hand grad-times-input | Signed digest grad-times-input |
| --- | ---: | ---: | ---: | ---: | ---: |
| Constructed land | -17.4783 | 1.62851 | .42086 | -.01425 | +.02946 |
| Constructed Bolt | -17.3641 | 1.66506 | .40028 | +.13737 | +.07513 |
| Natural Bolt | -5.4475 | .77735 | .15253 | +.16778 | +.10407 |

All11 roots have nonzero hand gradients. Across the eight generic natural roots, hand L2 ranges.00494 to97.88988 and digest L2.00049 to.55197. Hand signed grad-times-input is positive6/8 and digest positive3/8. Full per-root signed and absolute metrics retained. These are NOT finite ablation effects or normalized feature importance. Hand grouping covers all own-hand objects, including objects directly referenced by actions, unlike the earlier single unreferenced-token substitution. Embedding and digest coordinates have different scales and group sizes; no cross-channel norm ratio proves starvation or dominance. A continuous embedding direction need not correspond to any legal card replacement.

No unit in any measured tanh layer has mean absolute activation below1% of its layer mean on this sample. Largest saturation fraction(abs>.99) is scorer.1 at4.906%; first state layer3.977%; first action layer2.762%. State encoder99%-singular-mass rank is11 of at most11 sampled rows, so this cannot diagnose64-dimensional rank collapse. These measurements weaken a simple locally inactive-pathway account on these roots, not the broader plasticity-lock hypothesis. No weight-norm trend or trainability probe completed here. Nonzero local input gradients do not show that the optimizer historically amplified that pathway.

Together with the sampled-credit read, neither absent input sensitivity nor absent sampled advantage is established. The magnitude-gap cause remains unresolved. Do not choose a reset, new representation, reward change or frozen-trunk repair from these probes alone. Next discriminators: frozen-input weight/activation comparison across lineage checkpoints and a bounded ephemeral trainability probe, with existing calibration requirement preserved for any eventual intervention.

Evidence E:/mtg-meta-recovery-20260921/local-hand-gradients-001.json; source python/tools/local_hand_gradients_v1.py. One Torch CPU thread, no GPU/native execution, elapsed7.38s including load. Input/checkpoint hashes retained. Output contains all layer statistics and all11 roots. This is not fleet throughput qualification or a natural tactical certificate.
