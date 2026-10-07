# Trample defect found by tactical controls

The declared public combat controls found a simulation defect before any model error could be established. The old `assign_attacker_damage_to_blockers` implementation explicitly assumed no trample in the card pool. For a blocked attacker it assigned damage only to blockers, including a departed sole blocker. Avenging Hunter's trample keyword was present in the public model inputs but had no corresponding excess-damage behavior in this path.

The preserved old-engine reproduction is E:/mtg-meta-recovery-20260921/public-combat-fixtures-003/first.log, source c5dd5cc9. A 5/4 Hunter was blocked by a 4/4 Sacred Cat with one marked damage; Bolt killed Cat, but the defender took no combat damage, and the session reached the next draw. The verifier correctly rejected this information boundary. Roots001/002/003 are failed fixture checks, not completed model evaluations. The same old routine was read in the human-bridge and Kimi observation-audit worktrees; neither checkout was modified.

Fix commit2b3bc6bf, on the isolated codex/public-stack-features-v1 branch, adds a trample assignment path: lethal to each remaining blocker, accounting for marked damage and deathtouch, then excess to the defending player. Departed blockers do not receive assignment in this path. The existing nontrample assignment policy is unchanged. This follows [official Comprehensive Rules 702.19b and 702.19d](https://media.wizards.com/2025/downloads/MagicCompRules%2020251114.pdf). The current rules landing page links a document effective September25, after this work's September21 date, so the older official edition was also checked. This fix still uses one deterministic legal allocation; it does not expose all optional damage allocations, add planeswalker/battle combat, or claim complete combat-rules coverage.

Validation completed:

| Check | Result |
| --- | --- |
| Focused trample regressions | 7 passed |
| Broader engine unit suite, including those 7 | 141 passed, zero failures |
| Paired public combat controls | All assertions passed, four public states with two library orders each |
| Actual V4 inputs, logits/value and witnesses under hidden-library reversal | Identical |
| Fresh-process replay of all eight records | Byte-identical |

Focused tests cover single/multiple blockers in both seats, marked damage, removed blockers with a nontrample control, deathtouch, double strike, protection and simultaneous lifelink. Receipts: combat-trample-tests-001 and combat-engine-tests-001 under E:/mtg-meta-recovery-20260921. Full build plus focused test took206.37seconds; the cached broader suite took0.43seconds (141 test bodies0.17seconds). Four BelowNormal Cargo jobs, E target/temp, no paid compute or training.

The corrected fixture binary is public-combat-fixture-tools-004/public_terminal_fixture_v1.exe, SHA256 65a17a5ff8fe885485e0d38a67d96699494ad2d30611a4b10214c6fb4780e384, built from2b3bc6bf in112.68seconds. The three preceding diagnostic builds took113.45,114.59,114.70seconds. Canonical final evidence is public-combat-fixtures-004/result.json and first/result.json. The two final executions took0.966seconds excluding ownership checks; output SHA256 8166e2467115fdbe3f19ca4ddcfe50e7358b98b1ec6814061536cb85ecd6854e. Final inventory preserves seven idle Kimi human sessions and reports no active owned diagnostic workers.

g115 chose Cat by argmax in all four public states. Both physical seats agreed. With opposing life3, both face and Cat targets win; combined descriptive float64-softmax winning mass99.950737%. With opposing life5, only Cat is certified winning; its mass99.959362%. Unresolved alternatives are not forced losses. These constructed controls demonstrate neither natural prevalence nor learned lethal reasoning: the same creature preference succeeds in both life-total variants. No model deficit, promotion or human-strength conclusion follows.

Existing experiment results remain immutable statements about their pinned engine, not measurements of corrected behavior. Do not silently regenerate their binaries or pool corrected outcomes with them. This defect makes real-Magic transfer less certain; its effect on win rates and training is not yet quantified. Next work should qualify a separately identified corrected evaluation/human build and audit naturally occurring trample decisions before selecting another learning intervention. No automatic repeat of the failed entropy, costs or stack-feature screens.

Fable review remains unavailable: session876765fa-0834-4f03-8db1-6b1f99300885 failed HTTP429 with zero source reads, reset September22 07:00EDT. No repeated quota retry or endorsement. Bounded implementation and validation proceeded under the maintainer's research authority with this unresolved independent-review gap. CP7 outcomes remain excluded. The original human/league-competence goal remains active and unproven.
