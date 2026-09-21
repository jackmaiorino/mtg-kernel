# Retention development data and separate validation reservation

Prepared 256 physical decisions from 100 archived development games, with exactly 16 decisions for each of eight registered decks in each physical seat. All 303 V4 rows reproduce the immutable g115 logits and values exactly in the native runtime. No learner update or validation scoring ran.

## Selection and coverage

Source implementation: `46b3fda5` (`python/tools/prepare_terminal_retention_v1.py`). Native verification: `a065889f`, import correction `f174b8bb` (`campaign/retention_data.rs`, `retention_data_check` command).

The development pool is the same 100 games already inspected in the completed natural-input policy audit. It cannot be relabeled as fresh validation. Eligible groups contain at least one multi-action row. Selection uses deck, physical seat, decision category, preboard/postboard status and stable identity hashes. Within each deck/seat cell it rotates through decision-category/postboard strata and prefers games not yet represented. It does not select on outcomes, chosen actions, candidate scores, policy movement or parent confidence. Every substep of a selected physical decision is retained.

| Decision category | Physical decisions |
| --- | ---: |
| Ability | 28 |
| Attacker | 28 |
| Blocker | 28 |
| Cast | 30 |
| Choice | 26 |
| Discard | 25 |
| Land | 30 |
| Mana | 31 |
| Target | 30 |

The set contains 126 preboard and 130 postboard decisions. Twenty physical decisions have multiple rows, for 303 total rows. Menus range from 2 to 36 actions. Exact duplicate tensor groups and exact teacher-tensor overlap are absent; no exclusions or silent replacement were needed. The eight deck labels are Affinity, Burn, Elves, Faeries, Rally, Terror, Wildfire and `published-44ae71e1e126b63d`. All 100 development games contribute. These correlated decisions are not independent trials of playing strength.

The 5.39 MB data file contains copied visible tensor bits and frozen parent-logit/value bits, with source references. It contains no new reward targets or selected actions. Parent agreement is a constraint, not an optimal-action label. The parent targets come from the completed hash-verified audit and were independently replayed in the native model.

## Validation reservation

Reserved 100 other archived games, using updates 009,029,...189 rather than development updates 019,039,...199. Episode IDs and seeds are disjoint. Schedule metadata shows all eight decks in both seats, with 6 to 16 game exposures per deck/seat cell. File contents totaling 449,038,333 bytes were hashed to freeze identity, but gameplay JSON was not parsed and no scores or outcomes were used. The reservation manifest is pinned before any new candidate exists.

This is a prospective game-level **policy-retention** validation set for the next comparison. It is not a tactical holdout, a strength panel, globally unseen training ancestry or proof of current-engine play. Both pools are archived structured-policy games predating the trample correction. Fresh tactical transfer and later matched whole-match/human evidence remain separate requirements. No validation thresholds or retention weight were selected here.

## Verification and cost

Native checking verifies all 303 parent logits/value rows, V4 model identity, 256 groups, exact deck/seat counts, absence of duplicate IDs/tensor groups, and teacher non-overlap. Fresh-process replay produced identical result bytes. A one-bit parent-logit corruption, duplicate group and heldout teacher split were rejected before output publication. The original g115 optimizer hash remains unchanged.

File-extraction qualification on four archive sizes measured 0.532 seconds with one worker versus 0.658 seconds with four, including decoding and returning copied tensors. Results were identical. The faster local serial extraction completed all 100 files in 3.488 seconds. This is small file preparation, not a training allocation qualification; it does not qualify CPU/GPU/cloud placement for the next learner. Both PCs were checked for competing native work. No paid allocation was made; prior RunPod availability remained a recorded authenticated HTTP403, not a usable allocation.

The native release build took 145.58 seconds with four BelowNormal jobs and E-drive target/temp storage. First native replay took 2.186 seconds, fresh replay 0.518 seconds, and three rejection checks together 0.767 seconds. No active owned native jobs remained afterward; seven idle Kimi human-play sessions were preserved.

Two setup errors were preserved and corrected before successful checks: the first extraction qualification tried to JSON-decode its pinned Python source, and the first native build lacked a `BTreeMap` import (24.38 seconds). Neither produced a training result or changed a measurement gate.

## Artifacts and next action

- `E:/mtg-meta-recovery-20260921/terminal-retention-data-002/train.json`: data SHA-256 `701f816a4f51ce66bc356e69112693151a856a354c429d4c0a9efb8cc8fa5414`.
- `terminal-retention-data-002/validation-reservation.json`: reservation SHA-256 `9e572bdb14d13a2ca5e14984827c05c369e38b63ae0d3fdf25bd0b047bdbdf15`.
- `terminal-retention-data-native-001/completion.json`: native checks complete; result SHA-256 `8c8170f2328d21ba412a827afda2541a3e92c63856d499ba18a5bfb6683e723c`.
- `retention-data-tools-002`: executable SHA-256 `8411d3a3356fb5af9f9f87fb15c3aae7c70be907ba8355de61f4a88848054c6a`.

The subsequent [full-batch integration](public_terminal_retention_full_batch_20260921.md) connects this pinned data and verifies actual updates and resume. Next, define the causal retained-versus-unretained comparison, corrected semantic control, fresh tactical validation and frozen analysis gates before substantive training. Do not tune on the consumed 24-position evaluation or use the old raw-index permutation. A supported substantial launcher still needs compatible completed-work throughput evidence under `C:/Users/Jack/COMPUTE-POLICY.md`; repeated two-update engineering commands must not substitute for it.

Fable's independent objective/design review is still missing under the recorded zero-read HTTP429 until September 22 at 07:00 EDT. Authorized reversible preparation continued with that uncertainty recorded, without repeated quota retries or implied endorsement. CP7 outcomes remain excluded. No candidate promotion, human-preview change or human/league competence claim follows from these artifacts.
