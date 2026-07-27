# Net8 Action-Ingress Admission Execution Manifest v2

Status: PREDECLARED packaging/evidence retry; no-training diagnostic only.

This manifest authorizes one clean retry of the action-ingress admission
screen predeclared in `ACTION_INGRESS_ADMISSION_V1.md`, whose SHA-256 is
`9317e5504a72acaced0100aea889a36c50539f6ce7f46912170ca2a562fbb88f`.
The invalid execution was recorded at commit
`395913f0c9664372e0e778056a3b90f7ede4e257` in
`ACTION_INGRESS_ADMISSION_EXECUTION_V1_INVALID.md`, whose SHA-256 is
`f299657ab4e4e6ca9906ddc0f06b0eb538c781f53348eaa0d7dd21f5d77f8688`.
Sections 1 through 8 and Section 10 of that manifest are incorporated
unchanged: the parent authority, frozen inputs, structured repair, fixed
digest gates, static and runtime corpora, three model authorities,
interventions, metrics, separate per-model labels, interpretation rules, and
non-claims remain identical.

## 1. Why a retry is required

Execution v1 was frozen at implementation commit
`5d5ed8e856651e56b700915dde1844ea373407ad` and preserved under:

- artifact root:
  `D:\mtg-kernel-action-ingress-admission-v1-20260726`;
- Cargo target:
  `E:\cargo-target-action-ingress-admission-v1`.

The locked CPU release build, Linux and Windows static and packaging
preflights, and the three active Rust controls passed. The official probe
remained ignored during build admission. The build evidence is:

- build receipt SHA-256:
  `0cc9c7d943343b1d8babda4749257221399e3bdefaeb1366815d37bedc4060ba`;
- build receipt payload SHA-256:
  `fe20757fd7f6fbdf795f99d967e6c05e34f526b079ea4c7049fa6f6978981d55`;
- locked release executable SHA-256:
  `6036f3bc0fdfed2cc40931c53a073768a71702f6f839a6713c08b84899d49f82`.

The first fixed model, `raw-common-snapshot`, then exited zero and emitted
one well-formed 1,115-row payload. Before writing an invocation receipt, the
Python admission layer rejected that payload with:

`payload.input_statistics.digest_value_rms aggregation mismatch`

The preserved raw execution evidence is:

- stdout SHA-256:
  `816cf78c980356cd3e1f956962122528d87d76d9fa807dbbdfa0774f7cd2c253`;
- stderr SHA-256:
  `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`;
- envelope payload SHA-256:
  `507a8bdef0fe13e37808074d57e20d446958d8dc78cdfb434f272874cb2bef09`.

The failure is deterministic consumer arithmetic, not a model or payload
integrity failure. The v1 Rust producer implements the manifest's ordered
`f64` accumulation as a positive-zero left fold. On the saved digest row
norms it reports:

- total:
  `37733.52278214774`
  (`0x1.26cb0baa1a06fp+15`);
- value RMS:
  `0.5937322319121602`
  (`0x1.2ffdabcd49a23p-1`);
- mean squared norm:
  `33.84172446829393`
  (`0x1.0ebbda09bc860p+5`).

The official CPython 3.13.14 verifier used built-in `sum`, whose compensated
float accumulation instead reconstructed:

- total:
  `37733.522782147695`
  (`0x1.26cb0baa1a069p+15`);
- value RMS:
  `0.5937322319121598`
  (`0x1.2ffdabcd49a20p-1`);
- mean squared norm:
  `33.8417244682939`
  (`0x1.0ebbda09bc85bp+5`).

An explicit Python positive-zero left fold reproduces all three Rust values
bit-exactly. Direct-block summaries happen to agree under both algorithms.
The digest RMS was merely the first rejected field.

Execution stopped immediately. No invocation receipt or completion receipt
was written, neither imported Store model was invoked, the classifier was not
run, and no v1 functional-effect metric or descriptive label was admitted or
interpreted. The entire v1 screen is `INVALID`.

Before v1 was frozen, one raw ignored-test engineering smoke was mistakenly
executed from a dirty implementation tree. It did not use either official
root, the implementation changed materially afterward, and none of its
payload values were retained or consumed. It is non-authoritative
engineering smoke, not an execution of either manifest.

## 2. Exactly licensed repair

V2 permits only the following scientific-input-independent packaging and
evidence repairs:

1. replace each Python float-valued built-in `sum` reconstruction with an
   explicit ordered `f64` left fold beginning at positive zero and adding
   once per row in corpus decision/action order;
2. retain strict equality for the input-statistic summaries; do not add a
   tolerance, accept either summation algorithm, reorder rows, or use
   compensated summation;
3. extend each first-layer contribution row with the producer's existing
   pre-square-root `direct_contribution_squared_norm` and
   `digest_contribution_squared_norm` accumulators;
4. verify each reported row RMS exactly as
   `sqrt(squared_norm / 64)`, then verify each global contribution RMS from
   the ordered left fold of those pre-square-root norms divided by
   `1,115 * 64`;
5. update synthetic packaging fixtures and add cross-platform regressions
   that distinguish ordered addition from compensated addition and reject
   independently altered row norms, row RMS values, or global RMS values.

The two new row fields expose already-computed evidence. They do not change
the contribution calculation, action tensors, forward pass, outputs,
interventions, metric values, or interpretation.

Float exponentiation is not an authorized reconstruction of Rust
multiplication. No repaired verifier path may use `x ** 2` for these
aggregates.

Other than the mandatory V2 manifest/package/schema/marker/environment/test
identities, fresh-root bindings, evidence fields, and packaging tests
enumerated here, no other source or protocol change is licensed. In
particular, V2 must not change:

- any frozen input, Store, model parameter, tensor feature, action-reference
  value, corpus row, transform, or gate;
- any model read, timeout, execution order, intervention, scalar metric,
  sign rule, scientific/descriptive `RAW-*` or `IMPORTED-*` label, non-claim,
  or training prohibition;
- the Rust producer's scientific arithmetic or the v1 payload fields other
  than adding the two exact contribution evidence fields.

The preserved v1 stdout may be used only as arithmetic-regression and exact
unchanged-scientific-field projection evidence. It cannot substitute for a
v2 invocation and must not be copied into the v2 artifact root.

## 3. Source and artifact isolation

- Parent result commit:
  `ebd13031ea14526547849272ffbd7526fa2087fd`
- Failed v1 implementation commit:
  `5d5ed8e856651e56b700915dde1844ea373407ad`
- Branch:
  `codex/observation-diagnostics-v1`
- Worktree:
  `C:\Users\Jack\IdeaProjects\mtg-kernel-observation-diagnostics-codex`
- V2 artifact root:
  `D:\mtg-kernel-action-ingress-admission-v2-20260727`
- V2 dedicated Cargo target:
  `E:\cargo-target-action-ingress-admission-v2-20260727`
- GPU use:
  forbidden
- Training, optimizer steps, checkpoint publication, Store mutation, and
  model-parameter mutation:
  forbidden

The v1 artifact root and Cargo target are immutable failure evidence and must
not be deleted, overwritten, reused, or interpreted as a scientific result.
The v2 artifact root and target must be absent before the retry.

Their frozen pre-v2 tree snapshots are:

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

The tree-transcript identity is
`sha256-canonical-json-sorted-relative-tree-rows-v1`: walk without following
links or reparse points; encode each descendant directory as
`{"kind":"directory","path":<root-relative POSIX path>}` and each regular
file as
`{"byte_count":<integer>,"kind":"file","path":<root-relative POSIX path>,"sha256":<lowercase SHA-256>}`;
sort all rows by `(path, kind)`; serialize the array as UTF-8 JSON with keys
sorted, separators `,` and `:`, Unicode unescaped, and non-finite numbers
forbidden; hash those bytes with SHA-256. The root itself is not a row.

V2 uses the distinct Python package
`scripts/action_ingress_admission_v2`, v2 receipt/envelope/payload schemas,
marker and environment names, and distinct Rust tests. The exact version
authorities are:

- evidence label:
  `ACTION-INGRESS-ADMISSION-V2-DIAGNOSTIC-NON-EVIDENCE`;
- build receipt schema:
  `mtg-kernel-action-ingress-admission-build-receipt/v2`;
- invocation receipt schema:
  `mtg-kernel-action-ingress-admission-invocation-receipt/v2`;
- completion receipt schema:
  `mtg-kernel-action-ingress-admission-completion-receipt/v2`;
- classification schema:
  `mtg-kernel-action-ingress-admission-classification/v2`;
- classification receipt schema:
  `mtg-kernel-action-ingress-admission-classification-receipt/v2`;
- probe envelope schema:
  `mtg-kernel-action-ingress-admission-envelope/v2`;
- probe payload schema:
  `mtg-kernel-action-ingress-admission-payload/v2`;
- marker:
  `ACTION_INGRESS_ADMISSION_V2_JSON=`;
- environment names:
  `ACTION_INGRESS_V2_MODEL_ROLE` and
  `ACTION_INGRESS_V2_STORE_ROOT`;
- active Rust tests:
  - `native_checkpoint_inference_v1::checkpoint_reliance_probe_v1::action_ingress_admission_v2::slot69_repair_and_fixed_digest_gates_are_exact_and_fail_closed_v2`;
  - `native_checkpoint_inference_v1::checkpoint_reliance_probe_v1::action_ingress_admission_v2::digest_zero_stress_and_non_digest_ingress_controls_are_exact_v2`;
  - `native_checkpoint_inference_v1::checkpoint_reliance_probe_v1::action_ingress_admission_v2::supplemental_player_target_cross_runtime_tensorization_is_bit_exact_v2`;
- ignored official Rust test:
  `native_checkpoint_inference_v1::checkpoint_reliance_probe_v1::action_ingress_admission_v2::official_action_ingress_admission_probe_v2`.

The v1 package, Rust module, schemas, marker, and test identity remain
unchanged at their frozen commit.

The official packaging runtime is exactly:

- Windows Python:
  `D:\mtg-kernel-clean-venv-019f63a2\Scripts\python.exe`;
- Python version:
  `3.13.14 (main, Jun 23 2026, 15:19:27) [MSC v.1944 64 bit (AMD64)]`;
- Rust channel:
  `1.94.1`;
- GPU visibility:
  empty;
- model device:
  `cpu`.

The build must resolve and hash the actual pinned `cargo` and `rustc`
executables, reject ambient build-affecting overrides and Cargo configuration
files under the existing v1 environment policy, and record their verbose
version output.

The final v2 implementation commit, this manifest's SHA-256, implementation
source hashes, frozen input hashes, exact commands, runtime tuple, executable
SHA-256, output hashes, and exact inventories must be captured before
interpretation.

Relative to invalid-result commit
`395913f0c9664372e0e778056a3b90f7ede4e257`, the complete allowed source
diff is exactly:

- add `ACTION_INGRESS_ADMISSION_V2.md`;
- add
  `mtg-kernel/src/native_checkpoint_inference_v1/checkpoint_reliance_probe_v1/action_ingress_admission_v2.rs`;
- add the twelve named Python files under
  `scripts/action_ingress_admission_v2`:
  `__init__.py`, `build_probe.py`, `classify_results.py`, `contract.py`,
  `run_classifier.py`, `run_probe.py`, `test_build_probe.py`,
  `test_classify_results.py`, `test_contract.py`, `test_fixtures.py`,
  `test_run_classifier.py`, and `test_run_probe.py`;
- add only the V2 module declaration to
  `mtg-kernel/src/native_checkpoint_inference_v1/checkpoint_reliance_probe_v1.rs`.

The build preflight must mechanically require that exact name/status set and
that the parent-module diff is exactly the single V2 module declaration. It
must also bind every added source hash. The v1 Rust module and Python package
must remain byte-identical to implementation commit
`5d5ed8e856651e56b700915dde1844ea373407ad`.

For `raw-common-snapshot`, both the preserved V1 payload and the fresh V2
payload must project away the top-level:

- top-level `schema`, `label`, and `test_identity`;

The V2 projection must additionally remove only the new per-row
`direct_contribution_squared_norm` and
`digest_contribution_squared_norm` evidence fields. Serialize each projected
object as compact UTF-8 canonical JSON with sorted keys under the contract
serializer. The preserved V1 projection is exactly `400290` bytes with
SHA-256
`23419136ee00d84f1cfb0b13110333705eaa2ebdd0a130f2d0dd069b89aac9af`.
The V2 projection must have the same byte count and digest and must equal the
V1 projection byte-for-byte before the V2 raw invocation can be admitted.
This equality is an invariance/evidence gate only; no preserved V1 functional
metric or label may be interpreted.

## 4. Frozen retry sequence

1. Commit this manifest by itself as the predeclared authority.
2. Implement, review, test, and commit exactly the licensed repairs and V2
   evidence surfaces.
3. Require a clean worktree at that implementation commit, revalidate both
   frozen v1 tree snapshots, and require both V2 roots to be absent and not
   reparse points immediately before exclusive creation.
4. From that commit, build one new locked, release,
   `--no-default-features` Windows lib-test executable in the v2 target.
5. Re-run the unchanged Linux and Windows static controls, the versioned V2
   packaging tests at exactly `29/29` per operating system, and the three V2
   active Rust controls; the V2 official probe remains ignored during build
   admission. The static suite remains exactly `7/7` per operating system.
6. Require the release executable to be a non-reparse regular file strictly
   under the V2 Cargo target and every receipt/output path to be a
   non-reparse descendant of the V2 artifact root.
7. Execute the unchanged raw snapshot and two imported Store reads
   sequentially with the unchanged timeout of exactly 120 seconds per model.
8. Classify only a complete v2 receipt containing three `VALID` invocations
   and exactly five run files per model.
9. Preserve stdout, stderr, payloads, envelopes, receipts, classifier
   outputs, Store snapshots, and exact file inventories.
10. Interpret only the three separately qualified, predeclared per-model
   labels. Produce no pooled or global label.
11. Revalidate both frozen v1 tree snapshots after build, execution, and
   classification.

Creation of either V2 root consumes this one retry. Any partial output or
failure makes both V2 roots immutable `INVALID` evidence; they must not be
deleted, repaired in place, resumed, or reused. Another attempt would require
a separately predeclared V3 manifest, source surface, executable, and fresh
roots.

Any source/input drift, dirty worktree, stale output, malformed record,
non-finite value, timeout, panic, nonzero exit, model digest mismatch,
corpus mismatch, cross-model parameter mutation, invariant failure,
arithmetic reconstruction mismatch, or partial result is `INVALID`. No
partial metric may be interpreted.

## 5. Unchanged authority and non-claims

All scientific protocol, precedence, interpretation, and non-claim text in
the v1 manifest remains authoritative. Passing V2 licenses drafting, but not
executing, the separately predeclared three-arm micro-rung. This retry does
not license training, model promotion, a production encoding change, a
game-strength claim, or a pro-level-play claim.
