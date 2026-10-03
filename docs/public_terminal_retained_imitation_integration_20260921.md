# Real-checkpoint retained imitation: integration result

The real g115 checkpoint now completes two retained-imitation updates with byte-identical checkpoints across one/four CPU workers, fresh-process replay and fresh-process resume. This verifies the training path; it does not establish policy retention across the meta or improved play.

Implementation: `41d37f6a`, with runtime-module availability fixed in `728e28ea`. Command: `public_terminal_teacher_v1`, mode `retained_engineering`. The command permits only one or two updates, 1/4 CPU workers and fixed diagnostic beta 0.5. It has a distinct objective/checkpoint identity and cannot run a substantive campaign. This weight exercises the interface; it has not been selected for an experiment.

## Inputs and result

All 32 previously declared training teacher positions were used. No reserved positions were read. Retention used the first eight complete physical decisions per actor in the previously selected development archive `public-stack-r1-u019-e00`. Selection does not inspect outcomes, scores or chosen actions. These were 16 single-row decisions with menus of 2 through 10 actions, from one postboard Elves versus `published-44ae71e1e126b63d` game. This is deliberately a small integration fixture, not a representative retention set.

All eight recorded g115-opponent rows reproduce their archived logits and value exactly. Parent targets on both actors come from original g115. Teacher tensors cannot overlap retention tensors. Physical grouping, substep order, source identities and input hashes are checked. The archived game predates the corrected trample engine; it supplies saved visible tensors, not new current-engine gameplay.

| Check | Observed result |
| --- | --- |
| Initial optimizer age | 32,400 |
| Final optimizer age | 32,402 |
| Initial retention KL | Exactly zero |
| First retained update versus teacher-only update | Exact full-state equality |
| Second update retention KL | 0.0001376620494 |
| Second update mean teacher CE | 7.393366814 |
| One versus four workers | Both checkpoint files byte-identical |
| Fresh-process replay | Both checkpoints and result byte-identical |
| Resume checkpoint 1 in a new process | Checkpoint 2 byte-identical to uninterrupted execution |

Seven accepted updates were executed across the four verification processes: two each for one worker, four workers and replay, plus one resumed update. Their checkpoints remain engineering outputs, not promoted models. No new games, reserved evaluation or paid compute ran.

The first executable build failed because the reused KL module had been declared only under `cfg(test)`. Runtime availability was corrected; the successful release build took 146.01 seconds using four BelowNormal jobs on E drive. The failed 41.22-second build is preserved. Binary SHA-256: `11f4756a9344eb25fe9ca810e271475dbed2521d9519523c59759d54a582f30d`.

Measured update-and-publication sections took 1.072 seconds for two updates with one worker, 0.990 seconds with four workers, 1.051 seconds for replay, and 0.525 seconds for one resumed update. These short engineering measurements exclude startup/input loading and do not qualify a substantial allocation. Both PCs were checked for ownership before work and after completion; neither had active owned native jobs at completion. Seven existing idle human-play sessions were preserved.

## Rejections and evidence

The command rejects more than two updates, a heldout teacher split, a one-bit archived behavior-logit mismatch, a changed resume objective, corrupted embedding padding and a finite non-padding parameter corruption. All rejection cases publish no output directory.

The original checking helper stopped at its final assertion because corrupting the first embedding element produced `ParameterManifest` before the expected state-hash check. All seven accepted updates and parity assertions had already passed. Their outputs and the helper are preserved. A separate follow-up verified the original parity evidence and changed a non-padding `scorer.0.weight` element; this failed with `retained resume state hash differs` and exit code 1. No successful update was repeated to resolve that harness expectation.

Evidence roots:

- `E:/mtg-meta-recovery-20260921/retained-imitation-integration-001`: original outputs, manifest and explicit `harness-failure.json`; the original harness is not marked complete.
- `E:/mtg-meta-recovery-20260921/retained-imitation-integration-rejections-001/completion.json`: completed follow-up and combined verification, including the remaining corruption check.
- `E:/mtg-meta-recovery-20260921/retained-imitation-tools-001` and `-002`: failed and successful executable builds.

Final state SHA-256: `c0506df44e20e4874fae73ff7f665244dfd109b9cf4930204959011bc27592d1`. Final checkpoint file SHA-256: `41888c18b270fae0892a47944d2588d5df527da11ab8b55974b63b798946250a`. Result SHA-256: `b4a462f479eed94b4b300a02ddb234c7358efeefbfbe968efe35008689e363f1`.

## Next scientific step

Build a development retention set with explicit deck, seat and decision-kind coverage and keep separate fresh validation. Parent agreement is a constraint, not an optimal-action label. The next comparison must distinguish tactical learning from broader policy movement, use the corrected semantic control, preserve terminal rewards and exclude CP7 outcomes. The consumed 24-position evaluation cannot select a retention weight or stopping point.

A new substantive launcher and compatible completed-work qualification are still required. The current two-update engineering command is not that launcher, and repeated use must not bypass qualification. CPU gradients retain the disposable teacher-update overhead; CUDA retained imitation is not implemented. The f32 softmax objective also differs from Hamilton deployment probabilities. No whole-match or human-strength claim follows from these checks.

Fable's independent review remains unavailable under the recorded zero-read HTTP429 until September 22 at 07:00 EDT. Reversible integration continued under Jack's explicit execution assignment, with the objective/design review gap unresolved. The original g115 preview, failed teacher gate and separate Kimi Escape branch remain unchanged.
