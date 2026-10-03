# Expanded-deck training and sideboard data, September 12, 2026

All three assigned engineering items are implemented and checked in the isolated
sideboarding worktree. Code commit: `a70f0985d58a3e26d448104ffc5922df4fa6c4a9`,
branch `codex/learned-sideboarding-integration-v1`. No main-tree merge, GPU run,
paid compute or change to the active native campaign occurred.

| Item | Completed result |
| --- | --- |
| Value observations | Chosen Battlefield/Hand branch and visible refreshed paid-creature power reach value-state inputs with explicit revision-2 feature pins |
| Native training connection | Explicit 60/15 registrations and pre/postboard configurations flow through actor-visible rollouts, immutable trajectories, native CPU backward/Adam and reloadable successor checkpoints |
| Broader sideboard data | All 25 ordered matchups across Rally, Affinity, Elves, Terror and Burn; 74 examples and 622 action targets; grouped split of 56 training and 18 imitation holdout examples |

## End-to-end verification

Two naturally completed Affinity/Elves games, one preboard with learner P0 and
one postboard with learner P1, produced 136 learner physical decisions and
143 policy substeps. The existing `terminal_reinforce_value/v3` loss completed
one native CPU Adam update. The checkpoint was reread through the real
successor loader, preserving its complete model and optimizer state.

A repeat of the initial game produced identical trajectory bytes:
`3e0920470c8ada4c0537f9f9ba0971696b3ddcdff7c7c9da5a6edd276052e01c`.
A resumed rollout used the updated checkpoint: its first input was identical
to the original, both logits and value changed, and its behavior-state hash
matched the saved update. Independent comparison confirmed changed model
parameters and nonzero Adam moments. Four checks rejected stale behavior state,
changed terminal deck metadata, sampled actions and feature identity before
creating an update output directory.

The two additional teacher batches completed 11 matches / 25 games and added
28 examples. With the two earlier completed static batches, the dataset covers
25 match groups, 22 teacher plans and 12 connected groups. Independent audit
confirmed all 25 requested matchup cells, every output hash, all 74 distinct
pinned input paths, and zero shared match, teacher-plan or duplicate-example
groups between training and holdout. Training contains 56 examples / 500 actions;
holdout contains 18 / 122.

Verification passed 24 distinct Rust tests, including the explicit fixture
emitter, and 40 Python tests with seven subtests. Rust and Python match exactly
on 24 real fixtures across all 13 tensors and 32,845 scalar values. Frozen V2
tensor and legacy loss/backward/Adam goldens pass. Old V3 revision-1 import pins
reject explicitly. Shared-source provenance hashes were refreshed while all
V2 golden cases remain unchanged.

## Scope and remaining work

This is working CPU training infrastructure and engineering evidence. It is
not the optimized native GPU/parallel Store pipeline, a playing-strength result,
or proof that the model understands numeric power represented by hash features.
It does not establish general Markov completeness or sustained throughput.

The sideboard head remains the original Rally/Burn imitation checkpoint; it
was not refit here. Targets remain hand-authored warm starts, with no value
labels. Of 74 examples, 24 teach only `Done`, all at game 3. Holdout contains
Affinity/Elves both ways, Terror/Elves both ways, and Terror/Terror; it provides
no held-out Burn/Rally evidence. Game-3 data occurs in 15 of 25 matchup cells.
Check a prior head's teacher exposure, or use a fresh head, before reporting
imitation holdout performance. These development groups are not BO3 win-rate gates.

Next learning work is broader sideboard fitting/qualification and an integrated
multi-deck training design with independent review. Production GPU/search/Store
integration remains prospective. Arbitrary supported registration labels and
configuration identities provide a brewing foundation; proposing and learning
new registrations remains unimplemented.

Fresh read-only Fable session `435736f5-f380-444f-a02c-985f7f04819c` failed HTTP
429 weekly quota with zero reads/tokens. No endorsement is claimed. Supplementary
Codex review findings and fixes are in [the training workflow](expanded_deck_training_v1.md).

## Artifacts and use

- [Training workflow](expanded_deck_training_v1.md), [observation revision](sideboard_chosen_creature_observation_v2.md), [dataset utility](sideboard_dataset_preparation_v1.md).
- Manifest: `C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/engineering-003/FINAL-MANIFEST.json`.
- Collect/update configs and replay verifier: the manifest directory's `expanded-training/`.
- Outputs: `E:/mtg-kernel-learned-sideboarding-evidence/engineering-003/`, including `expanded-collect-001`, `expanded-update-001`, `roundtrip-verification-001`, and `dataset-development-002`.
- Preserved executables: `build-a70f0985/` in the output root. Rust/Cargo 1.94.1, LLVM 21.1.8, MSVC linker 14.50.35725.0; CPU only, GPU ordinal null.

Use current pins from `data/flat_policy_v3/feature_contract_v3.json`. Collect
fresh games after every update; stale behavior-state data is rejected. Use fresh
output directories and preserve completed evidence. No task-owned build, rollout
or update remains running after completion.
