# Net8 Action-Ingress Admission Execution v1

Status: `INVALID`; packaging failure; no scientific result.

## Outcome

The v1 admission screen did not produce a completion receipt or any
classifiable model result. Its locked build passed, but the verifier rejected
the first model's payload before writing an invocation receipt:

`payload.input_statistics.digest_value_rms aggregation mismatch`

The raw probe exited zero and its payload was structurally intact. The
failure was caused by a version-dependent Python consumer accumulator, not by
model execution: CPython 3.13.14 built-in `sum` compensated the 1,115
positive digest row norms, while the manifest and Rust producer require an
ordered positive-zero `f64` left fold.

No v1 functional-effect metric or descriptive label is admitted or
interpreted. The two imported models were not invoked, no classifier was
run, and there is no v1 per-model or global label.

## Frozen authority

- Manifest:
  `ACTION_INGRESS_ADMISSION_V1.md`
- Manifest SHA-256:
  `9317e5504a72acaced0100aea889a36c50539f6ce7f46912170ca2a562fbb88f`
- Implementation commit:
  `5d5ed8e856651e56b700915dde1844ea373407ad`
- Branch:
  `codex/observation-diagnostics-v1`
- Artifact root:
  `D:\mtg-kernel-action-ingress-admission-v1-20260726`
- Cargo target:
  `E:\cargo-target-action-ingress-admission-v1`

The build ran from a clean worktree at the implementation commit. Linux and
Windows static and packaging preflights passed, the three active Rust
controls passed, and the official probe remained ignored during build
admission.

- Build receipt SHA-256:
  `0cc9c7d943343b1d8babda4749257221399e3bdefaeb1366815d37bedc4060ba`
- Build receipt payload SHA-256:
  `fe20757fd7f6fbdf795f99d967e6c05e34f526b079ea4c7049fa6f6978981d55`
- Locked release executable SHA-256:
  `6036f3bc0fdfed2cc40931c53a073768a71702f6f839a6713c08b84899d49f82`
- Cargo exit/wall time:
  `0` / `402036 ms`
- Build start/completion:
  `2026-07-27T05:09:13.409925+00:00` /
  `2026-07-27T05:16:00.2077899+00:00`

## Preserved partial execution

Only these two run files exist:

- `runs/01-raw-common-snapshot/probe.stdout.log`
  - bytes:
    `401005`
  - SHA-256:
    `816cf78c980356cd3e1f956962122528d87d76d9fa807dbbdfa0774f7cd2c253`
  - envelope payload SHA-256:
    `507a8bdef0fe13e37808074d57e20d446958d8dc78cdfb434f272874cb2bef09`
- `runs/01-raw-common-snapshot/probe.stderr.log`
  - bytes:
    `0`
  - SHA-256:
    `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`

There is no `probe-envelope.json`, `probe-payload.json`,
`invocation-receipt.json`, `completion-receipt.json`, or classification
directory. There are no run directories for either imported model.

The frozen root snapshots are:

- v1 artifact root:
  - aggregate SHA-256:
    `5488ffd74443833a28c44d63ebea0be27a770684caf78f0be41f57d48a248bc6`;
  - directories/files/bytes:
    `3` / `25` / `584131`;
- v1 Cargo target:
  - aggregate SHA-256:
    `0c1680d0b4c72f4dd7e4b8b739f30fba3f0acd732e949d9fa90d08aef8312aa0`;
  - directories/files/bytes:
    `118` / `560` / `191861359`.

Both roots are immutable invalid-execution evidence. They must not be
deleted, repaired in place, resumed, or reused.

## Failure reconstruction

For the saved digest row norms:

| Summary | Rust / ordered left fold | CPython 3.13 built-in `sum` |
|---|---:|---:|
| Total | `37733.52278214774` (`0x1.26cb0baa1a06fp+15`) | `37733.522782147695` (`0x1.26cb0baa1a069p+15`) |
| Value RMS | `0.5937322319121602` (`0x1.2ffdabcd49a23p-1`) | `0.5937322319121598` (`0x1.2ffdabcd49a20p-1`) |
| Mean squared norm | `33.84172446829393` (`0x1.0ebbda09bc860p+5`) | `33.8417244682939` (`0x1.0ebbda09bc85bp+5`) |

An explicit Python positive-zero left fold reproduces the Rust values
bit-exactly. The direct summaries happen to agree under both algorithms.
The digest RMS was only the first failing field; accepting it with a
tolerance would not repair the declared arithmetic contract.

## Pre-formal deviation

Before the implementation was frozen, a subagent mistakenly executed one raw
ignored-test engineering smoke from a dirty source tree. It did not use
either official v1 root, the implementation changed materially afterward,
and none of its payload values were retained or consumed. It is not an
execution of the manifest and supplies no scientific evidence.

## Route

Do not run the v1 classifier or either remaining v1 model read. A retry
requires a separately predeclared V2 manifest, distinct evidence package and
Rust test identity, fresh executable, and roots that are absent at launch:

- `D:\mtg-kernel-action-ingress-admission-v2-20260727`;
- `E:\cargo-target-action-ingress-admission-v2-20260727`.

V1 raw output may be used only as arithmetic-regression and scientific-field
projection evidence. It cannot substitute for a V2 model invocation.
