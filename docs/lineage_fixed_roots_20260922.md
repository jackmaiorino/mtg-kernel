# Fixed-root lineage and signed digest comparison

Read-only plasticity diagnostic on identical11 g115-selected roots at g110/g114/g115 (Adam steps31400/32200/32400). No model update, candidate selection or strength gate. Older models use the same Torch reference implementation and strict parameter shapes; g115 native score parity rechecked. No separate older-checkpoint native parity claim.

| Measure | g110 | g114 | g115 |
| --- | ---: | ---: | ---: |
| Constructed land face-minus-removal | -18.6248 | -18.6053 | -17.4783 |
| Constructed Bolt face-minus-removal | -18.5097 | -18.5805 | -17.3641 |
| Natural Bolt face-minus-selected | -5.5143 | -8.9837 | -5.4475 |
| Scorer parameter L2 | 10.5082 | 10.5346 | 10.5445 |
| State encoder parameter L2 | 15.2601 | 15.2936 | 15.3004 |
| Scorer tanh saturation fraction | .04360 | .05342 | .04906 |
| First state tanh saturation fraction | .04119 | .04261 | .03977 |
| Scorer99%-singular-mass rank | 28 | 28 | 28 |
| State99%-singular-mass rank | 11 | 11 | 11 |

No sampled low-activity units under the previously declared threshold at these checkpoints. Modest norm growth with stable sampled rank and nonmonotonic saturation does not establish plasticity loss. These few g115-selected inputs are not representative older-policy visitation; state rank is sample-limited to11. Natural action preference changes nonmonotonically. No checkpoint selected from this read. Trainability remains unmeasured; no repair lever follows.

Requested finite digest-ablation signs, extracted from already completed natural-hand-channel-001 outputs. Target fixed per root to baseline top-minus-runner-up logit, which is not a tactical-quality label. Digest zeroing effects at steps1/5/7/29/42/43/46/49 respectively: -.0051012, -.0320725, -.0767064, +.0073848, +.0070434, +.0642977, +.0002880, +.3406085. Three negative/five positive. These are actual finite effects, unlike signed gradient-times-input. No new model execution for this supplement.

Evidence E:/mtg-meta-recovery-20260921/lineage-fixed-roots-001.json (checkpoint and input hashes, all layer statistics) and natural-hand-channel-001/signed-digest-effects.json. Source python/tools/lineage_fixed_roots_v1.py. CPU one Torch thread,5.17s including loads, no GPU/native launch. This diagnostic is not a throughput qualification.

E-20260922-04/05 adopted: sibling collab only, optimization-stripped evaluation guards prohibited, semantic stays shelved, milestone order unchanged. Kimi owns further credit audit; Codex retains attribution/plasticity and natural-witness work. Next necessary discriminator is a bounded ephemeral trainability probe on natural captured inputs with matched targets and an explicit non-claim about tactical strength, before choosing a plasticity lever. Historical causal mechanism remains unresolved.
