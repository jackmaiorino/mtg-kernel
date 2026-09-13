# Phase 1 original-time BO3 capture and preparation

`phase1_bo3_trainable_v1` adds explicit original-time V3 tensor capture and read-only gameplay preparation. It does not update parameters, publish a checkpoint, start a training loop, or change the existing BO1 objective. The existing `phase1_bo3_collect_v1` command and its V1 result shape remain unchanged.

The new slice accepts ordinary expanded checkpoints, KeepSevenV2 opening, fixed Play/Draw choices, Keep sideboarding at both seats, and disabled search. Registry-transfer continuation is rejected until an explicit BO3 schedule transition exists. The fixed auxiliary choices are untrained.

## Commands and artifacts

Use a clean compiled executable and absolute paths. The output parent must already exist and be exclusively owned by the run. These are invocation examples, not executed qualification receipts:

```text
phase1_bo3_trainable_v1 collect --request C:/run/collect-request.json --output C:/run/new-capture.json
phase1_bo3_trainable_v1 prepare --request C:/run/prepare-request.json --output C:/run/new-preparation.json
```

| Command | Request | Published artifact |
| --- | --- | --- |
| `collect` | `TrainableBo3RequestV1`: schema `mtg-kernel-trainable-bo3-request/v1`, exact `config`, two `packages`, and `capture_limits` | `TrainableBo3ResultV1`: schema `mtg-kernel-trainable-bo3-result/v1`, original request, unchanged V1 core result, and native capture |
| `prepare` | `Bo3GameplayPreparationRequestV1`: schema `mtg-kernel-bo3-gameplay-preparation/v1`, exact `learner`, ordered `attempts`, and `limits` | Schema `mtg-kernel-bo3-gameplay-preparation-report/v1`, the SHA-pinned preparation request, its exact parsed contents, and the validated report |

Each preparation attempt contains SHA-pinned absolute `request` and `result` paths plus one `learner_seat`. Declare every attempted match in its original order, including incomplete attempts. Preparation limits are `max_input_bytes` and `max_prepared_payload_bytes`. The report preserves complete/incomplete/eligible counts, exclusions, physical-group and substep counts, and exact `weight_bits`. The CLI drops all private prepared tensors without passing them to an optimizer.

Collection verifies the executing binary through the existing complete-package loader, including its clean compiled source identity and actual installed checkpoint/model. Preparation separately verifies recorded producer metadata and pinned artifacts, then loads both actual policies through the strict expanded loader. It does not convert recorded metadata into a current-runtime certificate for the updater. Neither path invents missing tensor outputs for old V1 results.

## Data and weighting contract

Every committed gameplay row has one native witness containing the exact V3 tensors and original raw logit/value bits from its original scoring call. Capture shares the existing selection and pending-commit path; it never re-encodes, re-scores, or draws another action during collection. Auxiliaries have no native witnesses. An uncertain pending selection and its witness are discarded together and reported in diagnostics.

Preparation validates match/config and package identities, reproduces every actor's raw outputs, Hamilton masses, and selected action using the original per-game/per-seat RNG order. It retains only complete physical groups for the designated learner. All games in a naturally completed match receive that match's return. With `N` eligible matches and `G` learner physical groups in a match, each group receives `(1.0_f64 / N / G) as f32`; its bits are preserved. Multiple substeps do not create extra group weight. Incomplete matches produce no groups or targets, and zero eligible matches return `NoUpdate`. This remains conditional-on-completion evidence and can be biased by nonrandom truncation.

Duplicate detection binds seed, chooser, ordered registrations, actual inference parameters/features/registry, sampler, and supported auxiliary behavior. Changing labels, record caps, paths, producer build metadata, or frozen-opponent Adam history cannot make a duplicate fresh. Exact full learner source/model/Adam equality is checked separately.

Tensor forward/sampler replay does not independently prove that an external producer encoded the recorded visible observation or ran legal engine transitions. The trusted pinned original producer and same-build capture-off/core parity remain that boundary. This is engineering preparation, not a full BO3 learner or playing-strength result.

## Bounds and recovery

Collection requests are bounded to 4 MiB; preparation requests to 1 MiB. Native payload and summed canonical native-record JSON are each bounded to 256 MiB, with at most 16 MiB per native record. Preparation accepts at most 32 attempted matches, at most 512 MiB cumulative request/result input, at most 256 MiB retained native payload, 65,536 learner groups, and 100,000 substeps. Declared limits can be smaller.

The CLI serializes directly into one bounded JSON buffer. Collection permits the sum of its two recorded-JSON limits plus a fixed 16 MiB envelope; preparation permits its 1 MiB request bound plus a 4 MiB report envelope. These byte bounds are not total RSS guarantees. Models, parsed input, Rust container overhead and serialization buffers coexist; host reserves and measured worker capacity still apply.

Both commands refuse existing output or `.stage` paths and use `durable_publication_v1`: a new staged file is synced and checked, atomically linked without replacement, reopened and verified. Existing filesystem requirements and Windows directory-entry power-loss limitations apply. There is no incremental match export or mid-game recovery. A killed process before publication can lose its in-memory capture; whole-match replay uses a fresh output path.

## Verification status

Seven source tests cover actual natural two/three-game captures, capture-off/core byte parity, both-seat numerical and sampler replay, unequal match lengths, incomplete exclusion, capture bounds, tampering, and identity binding. A separate structural regrouping assertion checks a complete two-substep physical group using original captured tensors/actions and a natural terminal. It does not claim that the engine emitted that decomposition. The actual-engine fixtures use legal all-basic registrations and explicitly artificial Pass-preferring native weights; no winner, engine action, sampler or production bound is replaced.

Root ran all seven capture/preparation tests successfully in 134.54 seconds of test execution. Three CLI tests also passed, covering command scope, bounded serialization, and public preparation-parser negatives. All five existing collector regressions passed in 132.89 seconds, including the greedy sideboard path, engine-error handling and legacy parity. The first compilation found two mechanical integration errors, an ambiguous helper import and a missing capture-off test field; both were corrected before the successful run. Clean public collect/prepare qualification against a real checkpoint remains pending. No public success is inferred from private fixture tests.

Fresh Fable review `7dca4937-f670-42ca-bb72-9ab677ea0d2e` failed the weekly limit on 2026-09-13 before source reads or feedback. This is unavailability, not endorsement. Independent Codex review identified fixes for core/config binding, semantic duplicate identity and sampler metadata equality; all three were implemented and checked. Root accepted this bounded engineering scope with public integration and learning still pending.
