# Burn choices linked to actual archived training credit

Read-only extension of archived_gae_credit_audit_20260922.md. No new experiment or training. Same first r1/a continuation batch, same ten episodes; no outcome-based selection. g115 unchanged, M1 unmet.

Decoded ChooseTarget from action-feature one-hot index6 (195 features/action), source references from role0 (25 features/reference), card token minus1, own-hand group0, opponent player target index46 and card target index44. Definitions are in flat_policy_v1.rs and native_flat_tensorizer_v2.rs:6662/6788. Checked the decoded source and both player-target flags against every semantic action in the independently engine-captured natural Bolt root before applying to the training archive. Every training member hash reverified. Joining by episode slot and physical_decision_id connects each substep to the previously reconstructed group advantage.

Found16 burn target substeps in16 physical groups:5 face choices,11 card choices,9 positive normalized advantages. Six roots also have another burn card in hand. Their normalized advantages are two positive and four negative, all nonzero. Thus this batch provides sampled training signal at relevant burn-with-hand-burn decisions; it is not evidence of absent updates to these choices.

| Episode slot / step | Source | Chosen target | Normalized advantage |
| --- | --- | --- | ---: |
| 2 / 140 | Galvanic Blast | Opponent | -.00022 |
| 3 / 2 | Lightning Bolt | Opponent | -.25533 |
| 3 / 6 | Lightning Bolt | Opponent | -1.12871 |
| 3 / 44 | Lightning Bolt | Opponent | +.12668 |
| 8 / 4 | Lightning Bolt | Card | +1.03600 |
| 9 / 113 | Lava Dart | Card | -.03471 |

No root has a certified tactical label, and none is claimed identical to the constructed or naturally captured candidate. The signs cannot be interpreted as correct/incorrect credit. Nonzero advantage is a loss input, not proof that a particular hand-token parameter moved: gradient routing, saturation and optimizer state still matter. This is current continuation coverage, not g115 historical exposure. Mechanism C remains unresolved, with the narrower absence-of-signal claim unsupported for these six sampled choices. No justification to change lambda, reward, features or optimizer follows.

Evidence E:/mtg-meta-recovery-20260921/burn-credit-link-001.json retains all16 rows, raw/normalized advantages, terminal coefficients and the value-plus-roundoff remainder. Source python/tools/link_burn_credit_v1.py. The remainder is an algebraic decomposition with recorded values fixed, not a causal attribution. No native engine, training or GPU process launched. Next priority is actual grouped-input attribution and activation/plasticity measurement on the captured tensors, not more unlabeled credit counts.
