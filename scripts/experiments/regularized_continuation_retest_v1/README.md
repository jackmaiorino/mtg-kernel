# Regularized continuation retest v1 preflight

These scripts are preflight only. They do not interpret gameplay outcomes.
Run them from the repository root after the native implementation is ready:

```powershell
pwsh -File scripts/experiments/regularized_continuation_retest_v1/beta-zero-identity.ps1
pwsh -File scripts/experiments/regularized_continuation_retest_v1/throughput-screen.ps1
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

Both gates bind the base commit, clean candidate commit, original `2/32/16`
topology, revealed preflight seed `969999`, Pool3 generation-384 inputs,
Rust/Cargo/linker/CUDA/driver identity, executable hashes, GPU identity,
resource samples, and create-new evidence attempts. They never parse gameplay
outcomes.

Each child process receives exactly one physical GPU UUID through
`CUDA_VISIBLE_DEVICES` and therefore uses CUDA-local ordinal 0. This removes
any ambiguity between CUDA and `nvidia-smi` ordinal ordering. Every run parent
also carries an experiment-side policy-anchor authority record. A resume must
match its exact beta, promoted(2) checkpoint, sidecar, state, and Pool3 hashes
before the native process can start.
