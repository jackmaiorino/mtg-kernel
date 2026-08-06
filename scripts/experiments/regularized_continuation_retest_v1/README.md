# Regularized continuation retest v1 gates

These scripts implement the ordered gates through full-horizon training. Gates
1 through 3 do not interpret gameplay outcomes. Gate 4 is the predeclared
gross-safety terminal comparison and runs only after Gate 3 selects a
coefficient. Full-horizon training runs only after Gate 4 passes.
Run them from the repository root after the native implementation is ready:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/experiments/regularized_continuation_retest_v1/beta-zero-identity.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/experiments/regularized_continuation_retest_v1/throughput-screen.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/experiments/regularized_continuation_retest_v1/coefficient-screen.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/experiments/regularized_continuation_retest_v1/gross-safety.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/experiments/regularized_continuation_retest_v1/full-horizon-training.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/experiments/regularized_continuation_retest_v1/full-horizon-parent-drift.ps1
$drift = (Get-ChildItem 'D:\mtg-kernel-regularized-continuation-retest-v1\development\seed-1941001\full-horizon-parent-drift\attempt-*\parent-drift-manifest.json' | Sort-Object LastWriteTime | Select-Object -Last 1).FullName
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/experiments/regularized_continuation_retest_v1/full-horizon-evaluation.ps1 -ParentDriftManifestPath $drift
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

The gross-safety gate first runs one bounded CPU topology screen on revealed
seed `969999`. It compares one arm with two concurrent arms, requires a
bit-identical repeated control stream and at least 4 GiB free host memory, and
uses concurrent arms only at `1.20x` or greater aggregate speedup. The formal
panel then evaluates the selected update-32 checkpoint and beta-zero update-32
control against promoted(2) generation 384 on 512 fresh seat-swapped pairs at
seed `1942001`. Both terminal streams must complete before either is parsed.
The terminal-order classifier requires selected-minus-control net at least
`-26` overall and `-18` in each selected seat. Failure is the frozen
`GROSS-SAFETY-STOP`; it cannot select another beta.

The full-horizon controller binds the exact selected training executable and
the passed coefficient, gross-safety, and deterministic throughput manifests.
It trains beta `0.1` on seeds `970001` and `970002` concurrently on GPUs 0 and
1, then seed `970003` on GPU 1. Each create-new candidate Store must finish at
generation and Adam step 512 with 32,768 natural episodes and checkpoints 64,
128, 256, 384, and 512. The immutable original beta-zero Stores are recorded as
matched controls. Training completion does not read terminal outcomes and is
not playing-strength evidence.

The full-horizon parent-drift evaluator reuses the frozen terminal-blind
validation corpus at seed `1941001`. It measures mean parent KL and TV for all
three beta `0.1` candidates and matched beta-zero controls at generations 64,
128, 256, 384, and 512. It reports every `R_g` ratio and the predeclared
`R_512 >= 0.75` diagnostic without reading terminal outcomes. Since beta `0.1`
was the only positive screen-eligible coefficient, this diagnostic cannot
authorize an escalation in this campaign.

The full-horizon evaluation first measures 1, 2, and 8 concurrent 64-pair
streams on revealed seed `969999`, verifies a bit-identical repeated stream,
and freezes the fastest resource-safe measured topology. It records the
achieved rate, utilization, and projected wall time before formal evaluation.
This screen chooses only H2H evaluator process concurrency. It does not change
the earlier frozen GPU training topology or its `1.5x` selection rule.
The formal phase runs exactly 21 create-new streams on seed `982001`: nine
512-pair diagnostic streams and twelve 2,048-pair endpoint streams. No formal
terminal stream is parsed until all 21 have completed and their process,
executable, Store, and output hashes validate. The classifier then applies the
frozen collapse-reproduction, V3, late-stability, P1, direct-score, H4, and
nomination rules using terminal W/L/D only.

Both gates bind the base commit, clean candidate commit, original `2/32/16`
topology, revealed preflight seed `969999`, Pool3 generation-384 inputs,
Rust/Cargo/linker/CUDA/driver identity, executable hashes, GPU identity,
resource samples, and create-new evidence attempts. They never parse gameplay
outcomes.

Run the pure harness checks before a release build:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/experiments/regularized_continuation_retest_v1/common-tests.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/experiments/regularized_continuation_retest_v1/coefficient-selector-tests.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/experiments/regularized_continuation_retest_v1/gross-safety-classifier-tests.ps1
python scripts/experiments/regularized_continuation_retest_v1/full-horizon-classifier.py --self-test
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
