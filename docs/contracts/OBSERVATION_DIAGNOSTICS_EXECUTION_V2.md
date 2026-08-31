# Net8 Observation Diagnostics Execution Manifest v2

Status: PREDECLARED packaging retry; diagnostic only.

This manifest authorizes one clean retry of the observation diagnostics
predeclared in `OBSERVATION_DIAGNOSTICS_EXECUTION_V1.md`, whose SHA-256 is
`8b8e364cf122397e940c0bf76e6c186fd4ed188a20e65faaa908531cbbc2c575`.
Sections 2 through 6 of that manifest are incorporated unchanged: the feature
audit, six Store/generation pairs, fixed 256-decision Rally corpus,
interventions, metrics, integrity controls, five-of-six interpretation rules,
and non-claims are identical.

## 1. Why a retry is required

Execution v1 was frozen at implementation commit
`75f8a375a10e66c1fb06eab4d3c0f8a5d59b6f51` and preserved under:

- artifact root:
  `D:\mtg-kernel-observation-diagnostics-v1-20260726`;
- Cargo target:
  `E:\cargo-target-observation-diagnostics-v1`.

Its locked release build and all artifact-free controls passed. The first
fixed Store probe then exited zero in 79.876 seconds and emitted one valid JSON
marker and one timing marker, but the Python admission layer rejected the
stdout before parsing or interpretation. Windows libtest writes

`test <exact-test-name> ... OBS_RELIANCE_JSON=<envelope>`

on one line; the v1 parser required the marker at column zero. The preserved
invocation receipt is `INVALID` with payload SHA-256
`29c178715e35208508ca99c93568cb3e2bbd42d6a479a5ca20d754c9fbee39f6`.
No v1 Diagnostic B metric was admitted or interpreted.

The preserved v1 file hashes are:

- build receipt:
  `e6d40aa9573cd73681e3a583905fb04bc8cc9a5cb17cbd95025a8fbd0f6e42d2`;
- invalid invocation receipt:
  `4b6aad5b183263d5be7adad72dbdead983c22539c55f8cc7889bbee985039062`;
- captured stdout:
  `7f207246c1ea11476fdd3861bd914fd8db188f4c3fb1e09a91e5bed00cfc9140`.

The only scientific-input-independent repair licensed here is to require and
strip that exact frozen libtest prefix, while continuing to require exactly
one marker, the immediately following timing line, the following `ok` status,
an exit code of zero, and all existing raw-payload/hash checks. Tests must
reject a bare marker or any other prefix.

## 2. Source and artifact isolation

- Parent scientific result commit:
  `a6259b2d82474a407af98752dbbf802361f0076d`
- Failed v1 implementation commit:
  `75f8a375a10e66c1fb06eab4d3c0f8a5d59b6f51`
- Branch:
  `codex/observation-diagnostics-v1`
- Worktree:
  `C:\Users\Jack\IdeaProjects\mtg-kernel-observation-diagnostics-codex`
- Artifact root:
  `D:\mtg-kernel-observation-diagnostics-v2-20260727`
- Dedicated Cargo target:
  `E:\cargo-target-observation-diagnostics-v2`
- GPU use: forbidden.

The v1 artifact root and target are immutable failure evidence and must not be
deleted, overwritten, reused, or interpreted as a scientific result. The v2
artifact root and target must be absent before the retry.

The final v2 implementation commit, this manifest's SHA-256, executable
SHA-256, input hashes, exact commands, exit codes, wall times, and output
hashes must be captured before interpretation. A dirty worktree, changed input
after preflight, missing or malformed marker, nonzero exit, panic, non-finite
metric, timeout, or any mismatch is an execution failure rather than a
scientific result.

## 3. Frozen retry sequence

1. Commit this manifest and the exact-prefix parser repair.
2. From that clean commit, build a new locked, release,
   `--no-default-features` Windows lib-test executable in the v2 target.
3. Re-run all five artifact-free Rust controls and all seven static-audit
   tests; the external Store test remains ignored during build admission.
4. Execute all six unchanged fixed pairs sequentially with the unchanged
   120-second per-pair cap.
5. Classify only a complete v2 receipt containing six `VALID` invocations and
   the exact 30-file output inventory.
6. Preserve a canonical classifier execution receipt and interpret only the
   four predeclared metrics independently in their two predeclared scopes.

The already-captured v1 stdout is regression input for parser testing only. It
does not substitute for any v2 invocation and cannot be copied into the v2
artifact root.

## 4. Unchanged authority and non-claims

All scientific protocol, precedence, interpretation, and non-claim text in
v1 Sections 2 through 6 remains authoritative. This retry does not license
training, model promotion, a game-strength claim, a representation-bottleneck
verdict, or a pro-level-play claim.
