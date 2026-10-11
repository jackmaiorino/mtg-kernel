# Exact search policy throughput qualification

PR220 compares baseline `1d2404775a141db4cd90d7579f8f765db37d55df`
(main including PR206) with the candidate pinned in [STATE.json](STATE.json).
The combined change reuses tensor/forward buffers, batches V3/V4 state and
action digests, reuses action-reference working vectors and writes canonical
observation/extension JSON without temporary Value trees. Hash message bytes,
feature contracts, arithmetic, activation selection and sampling stay the same.
Both use Rust 1.94.1, MSVC linker 14.50.35725.0, release defaults and ordinary
activation math. Build receipts bind source, toolchain and the actual binary.
No speedup has been measured yet. Current status is in [STATE.json](STATE.json).

Added parity coverage compares mixed menus against the sealed action goldens,
all common observation fixtures against frozen JSON bytes, every V3/V4
extension member against the original builders, and populated engine/private
contexts across effect-choice variants. Deferred digest tests cover ordering,
duplicate actions, shrinking buffers and recovery after a late error. Separate
read-only source reviews found no serialization, ordering or validation mismatch.
These tests are prepared; their native execution remains pending in the queue.

The earlier buffer-only candidate was superseded before its build or any
measurement started. Its ready markers and watcher records are retained under
`D:/search-buffer-reuse-20261010/superseded-buffer-only`; the queued baseline
build is reused. The fixed two-root workload and comparison budgets are unchanged.

The supported host-slots queue owns the baseline build. A single one-shot
`pipeline.py watch` process waits without holding cores, then submits candidate
tests/build after baseline success and the approved candidate marker. It stops
on failures, retains all attempts and never automatically retries. The marker
templates are concrete snapshots for the parent agent to place in the scratch
root after review. The measurement marker separately binds the reviewed
qualification coordinator, inputs and budgets.

Candidate commands run with `cargo +1.94.1`, release, locked dependencies and
two build jobs: library tests filtered to `native_flat_tensorizer`,
`native_policy_value_net_v1`, and `sideboard_play_policy_v1`, then strict Clippy,
the gameplay-decision-trace feature check, and the regret-census binary build.
The isolated candidate checkout prevents later report commits from changing
the code under test. The shared target is reused only after the baseline
executable has been copied and hashed.

The fixed engineering workload uses the existing cast and target r1 V4 roots,
seed 2026100941, 20,000 transitions per arm, two evaluation worlds and a 65,536
evaluation transition cap. First compare one and two workers for each binary.
Require identical opaque primary-output hashes and work counts. Select the
common worker count with lower combined elapsed time, then run baseline,
candidate, candidate, baseline on the same allocation. ABBA dispatch refuses
missing or incompatible current qualification. These are bounded engineering
checks, not a formal search campaign or playing-strength evaluation.

`qualification.py` invokes the existing `tools/stage4a/stage4a_queue.py` through
the admitted host-slots process. The wrapper supplies the throughput/input,
binary, storage and completion checks that the existing queue does not enforce.
The primary speed denominator is complete queue elapsed time, including startup,
input loading and result serialization. Existing job wall includes its binary
hash and output check. Per-root elapsed sums overlap under parallel workers.
Cold-copy time is reported separately and is not a search speedup.

Hot files stay under `D:/search-buffer-reuse-20261010`, projected at 12 GiB and
capped at 20 GiB, with a 60 GiB volume reserve. Runtime inputs, binaries and
receipts are hash verified. The E: cold tree and independent D: SSD recovery
copies are retained; no scratch pruning is authorized by this helper. Both
artifact roots are registered through the owned collaboration worktree.

The current [inventory](inventory.json) found both PCs reserved and RunPod's
read-only API unavailable with HTTP 403. No paid execution is authorized.
The desktop reservation releases when bank-r1 completes or fails; it gives no
reliable ETA. The queued baseline is the wake mechanism, not a recurring task.
