# Regularized continuation retest v1 gates

These scripts implement the first three ordered gates. They do not interpret gameplay outcomes.
Run them from the repository root after the native implementation is ready:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/experiments/regularized_continuation_retest_v1/beta-zero-identity.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/experiments/regularized_continuation_retest_v1/throughput-screen.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/experiments/regularized_continuation_retest_v1/coefficient-screen.ps1
```

The identity gate builds the exact original macro commit and the clean candidate
descendant. It compares an uninterrupted original 64-update Store with a
candidate beta-zero Store produced by two operating-system processes: updates
0 through 32, close, reopen, then 32 through 64. Every Store file hash is bound
in the manifest and the complete trees must be bit-identical.

The throughput gate measures one GPU-1 lane against concurrent GPU-0 and GPU-1
lanes in an exclusive window. It selects both GPUs only when aggregate speedup
is at least 1.5x, resource ceilings pass, and cross-device Stores are identical.
Cross-device nonidentity legitimately selects GPU 1 only. Same-device
nonidentity is always `FAIL-INVESTIGATE`. The script refuses to start unless
the latest identity attempt passed on the same candidate commit.

The coefficient gate refuses to start unless both preceding manifests pass on
the same clean candidate commit. It completes all five 32-update beta arms
before running one fixed, parent-generated, terminal-blind validation corpus.
The selector reads only KL, TV, action-distribution, group-log-ratio,
parameter-distance, identity, completeness, and finiteness fields. It rejects
any report containing a terminal outcome property. The smallest eligible beta
is selected; no eligible beta records the predeclared stop without trying a
new coefficient.

Both gates bind the base commit, clean candidate commit, original `2/32/16`
topology, revealed preflight seed `969999`, Pool3 generation-384 inputs,
Rust/Cargo/linker/CUDA/driver identity, executable hashes, GPU identity,
resource samples, and create-new evidence attempts. They never parse gameplay
outcomes.

Run the pure harness checks before a release build:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/experiments/regularized_continuation_retest_v1/common-tests.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/experiments/regularized_continuation_retest_v1/coefficient-selector-tests.ps1
```

Each child process receives exactly one physical GPU UUID through
`CUDA_VISIBLE_DEVICES` and therefore uses CUDA-local ordinal 0. This removes
any ambiguity between CUDA and `nvidia-smi` ordinal ordering. Every run parent
also carries an experiment-side policy-anchor authority record. A resume must
match its exact beta, promoted(2) checkpoint, sidecar, state, and Pool3 hashes
before the native process can start.

Parallel lanes use an atomic child-completion record bound to the process ID,
launch contract, executable hash, and native log hash. The parent requires an
empty child stderr stream and the native test-success marker. This is the
completion authority because Windows did not reliably surface `ExitCode` from
the redirected hidden process object on the first throughput attempt.
