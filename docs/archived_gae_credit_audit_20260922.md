# Archived credit audit: first complete batch reconstructed

Read-only mechanism-C diagnostic, not a gate or a model update. No repair selected. M1 remains unmet.

Reconstructed replica1/arm a/update0 of the completed control-variance continuation from its pinned request, ten archived episodes and saved receipt. Member SHA256s verified against archive manifest; whole zip not rehashed. This is the first continuation batch from g115, not a census of the historical training that produced g115. Source algorithm: expanded_deck_training_v1/public_features.rs:594 and bootstrapped_advantage_v1.rs:45. Learner physical groups use first-substep values, terminal reward only on the final learner group, then whole-batch normalization. Reconstruction uses ordered f32 arithmetic.

All five saved advantage-statistic values match bit-for-bit, with639 learner groups and668 learner substeps matching the receipt. Ten episodes cover Gates, Burn, Affinity, Rally, Elves and Terror. This checks the reconstructed targets against the actual batch receipt rather than interpreting evaluation logs as training targets.

| Quantity | Result |
| --- | ---: |
| gamma / lambda | 1 / f32(.9) |
| Raw advantage mean | -.0002541569 |
| Raw advantage SD | .2548775971 |
| Groups with at least one nonforced choice | 636 |
| Choice groups with direct terminal coefficient below .01 | 314/636 |
| Median direct terminal coefficient | .01077525 |
| Choice groups whose sign changed under normalization | 0/636 |

The direct terminal coefficient in the unnormalized GAE recurrence, holding recorded values fixed, is (gamma*lambda)^k for k later learner groups. At lambda.9 it falls below.01 after44 groups. This coefficient is NOT total credit: intermediate bootstrapped values may already encode the eventual outcome. Batch normalization further couples examples. No counterfactual unchosen-action return or parameter-gradient attribution is reconstructed here. Receipt-summary agreement is a strong implementation check, not independent proof of every individual target bit.

This result establishes attenuated direct terminal contribution in a real complete batch. It does not establish that credit never reached a hand-dependent fork, or that changing lambda/reward would improve play. No certified hand-fork labels exist in this batch. Mechanism C remains unresolved; A attribution and B plasticity remain pending. The next analysis should connect actual tensor/action identities to this reconstruction and quantify learned-value contribution before proposing an intervention.

Reader serial.504s/four-reader.487s, exact agreement over40,457,614 uncompressed bytes. This small artifact check is not training throughput qualification. Evidence E:/mtg-meta-recovery-20260921/archived-gae-batch-001.json; source python/tools/audit_archived_gae_batch_v1.py. No native child or GPU process launched.
