# Search experiment runtime

This task-specific supported path guards the search-sharing-allocation-20261013
screen. Existing `tools/stage4a/launch_stage4a.py` and `stage4a_queue.py` are not
migrated and do not provide this throughput compatibility/final CPU guard.
The lead reviews the manifest before dispatch and owns cross-host allotments,
the 12 aggregate process CPU-hour ledger, and research/result delivery.

`stage_inputs.py --collab REPO --ref COMMIT --root SSD_ROOT` stages the nine
registered cases, ROOTS, bank specs and explicit checkpoint/import leaves. It
pins the collab ref once, verifies all published hashes, and rewrites only source
paths. `inputs/INPUTS.json` is an input index, not a second experiment state.
Current staged root is `D:/e-scratch/search-sharing-allocation-20261013`.

`make_manifest.py --binary EXE --commit SHA --host-slots PATH --out MANIFEST.json`
prepares the prospective qualification queue without launching. Default early,
middle and late roots are decks 0/1/2, all A/B/C, both transition anchors, one
separate qualification seed, 32 fixed work attempts and 128 fresh frozen-tree
worlds per root. Arm order rotates by deck and repetition. The queue preserves
matched group affinity for all arms; this order is a scheduling control.
Common baseline flags are repaired keys, interchangeable-choice merging off and
key memo off. These are the production defaults, despite both optional fixes
being present in the binary. No D arm is generated.

The lead can shorten the initial queue using `--decks 1 --budgets 16000` to bound
the first complete production path. Extend to early/middle/late and both anchors
only after its cost establishes that the qualification and full panel fit. A
32-attempt first probe is a timing probe; the final compatible qualification
should use a representative frozen work limit established by cost-only
calibration. Changed binary, input, machine/storage or workload invalidates the
receipt. To prepare the baseline cost-only path, pass `--calibrate`, a separate
calibration seed namespace, and the desired root subset. It runs only A with
actual transition caps 16k/64k and `PROFILE_EVALUATE=0`. The lead launches that
bounded queue with `--phase qualify`. Then run `calibrate.py --completion
ROOT/calibration/completion.json --out ROOT/CALIBRATION.json`. This reads costs
only and derives full-process CPU targets plus measured output/hash reserves.
The per-root shape is
`{"roots":{"root_id":{"64000":{"native_cpu_seconds":X,"overhead_reserve_seconds":Y}}}}`.
Native formal CPU ceilings include model load once. The calibration asset joins
the pinned inputs in both qualifying and formal queues.

After lead review, serial then increasing eligible parallel counts use the same
manifest and deterministic work limits:

```powershell
python tools/search_experiment/launch.py dispatch --manifest MANIFEST.json --phase qualify --workers 1 --label q-serial
python tools/search_experiment/launch.py dispatch --manifest MANIFEST.json --phase qualify --workers 2 --label q-parallel2
python tools/search_experiment/launch.py qualify --batch ROOT/q-serial/completion.json --batch ROOT/q-parallel2/completion.json --out ROOT/THROUGHPUT.json
```

`dispatch` verifies inputs, binary, storage reserve and allotment, pins the binary
under E:/pinned-binaries/SHA, then enters the existing host_slots `run --wait`
queue with BelowNormal priority. Current reservations remain authoritative.
`--cores-claimed` is separate from `--eligible-workers`: when only one eligible
logical CPU is available, set cores claimed to 1 and compare one versus two real
workers from at least two independent matched groups. Both workers remain on
that single core; no reserved core is used. The receipt pins the claimed core
count. If one worker finishes useful work fastest, it is the measured best
allocation on that available topology; multicore scaling remains unmeasured.
`worker` also checks live containment before spawning native work. Every process
starts suspended, is pinned to its assigned core and has execution throttling
disabled/verified before resuming. A retained handle supplies final user+kernel
CPU even after exit. Selection, inline evaluation, source load, final native
serialization and queue CPU all count. One queue exclusively owns its ledger.
An abrupt supervisor loss keeps the unresolved attempt's entire reserved CPU
allotment until the lead reconciles retained process evidence; retries cannot
forget failed-attempt spend. One compact completion file records progress and
ordered results. STOP, CPU, per-job wall, output cap and 60 GiB reserve checks stop
running jobs and preserve incomplete outputs. Claims suspend if reservations
change. Suspension wall time remains visible and is not a throughput benefit.

For formal jobs, generate all nine decks, three fresh selection seeds, both
anchors, `--phase measure --calibration FILE --qualification THROUGHPUT.json`.
Formal queues preserve the 16k/64k anchor labels in `PROFILE_ANCHOR` while all
arms receive the same large safety transition cap (`--safety-cap`, default
16,000,000). Baseline calibration alone retains actual 16k/64k transition caps.
Faster arms can spend their full calibrated CPU allowance. A safety cap that
binds invalidates an equal-CPU comparison.
Before any native spawn, formal dispatch requires receipt parity over search
primary plus independent evaluation hashes, measured serial and parallel work,
exact input/binary/host/storage compatibility, and the fastest measured eligible
worker count. Adjust host CPU caps/external CPU accounting from the parent's
single aggregate ledger. Waiting or spare capacity never grants execution
authority. No provider mutation or rental path exists here.

`analyze.py --input ROWS.jsonl ... --completion ROOT/formal/completion.json
--panel ROOTS.json --seeds exact,seed,strings
--out ANALYSIS.json` pairs the same fresh world seeds for each arm's frozen tree.
Unsupported roots retain the actual ordinary-policy fallback outcomes and are
reported separately. Unknown or rejected pairs remain unresolved and block a
quality advancement. B-A/C-A primary contrasts use only the 64k anchor, averaging
three seeds within each root, then equal root weights. Paired root-cluster
bootstrap intervals are marginal 97.5 percent, Bonferroni-adjusted for nominal
95 percent family coverage across the two primary comparisons. The 16k
rows are secondary diagnostics. Plain fixed-action evaluation cannot satisfy
primary analysis. No cross-arm dedup follows from identical first actions.
Full process CPU comes from hash-bound runtime completion records. The
comparison cost is full native process CPU minus pure fresh evaluation CPU,
retaining source load, construction, C execution preparation, hashing and output.
Each arm and each matched difference must fit the prospectively accepted 5
percent target envelope. Missing costs, a bound safety cap, technical faults or
unknown outcomes prevent quality advancement; no nominal anchor alone proves
equal compute.

This is development-panel evidence, not ordinary-start strength or policy
learning. Final output sealing, recovery cold copy, artifact registration and
tracker delivery belong to the lead's existing manifest and state.

## Linux deployment and detached ownership

The supported Linux worker uses `wait4` user+system rusage for final native CPU;
it never substitutes the last live `/proc` sample. Affinity and nice 10 are set
before native `exec`, and native children remain in the host_slots contained
session. Windows keeps its retained handle and suspended affinity/EcoQoS path.
The two Linux-only accounting tests must pass on the allocated host before
representative qualification. They remain pending when only Windows testing
has been observed. No remote throughput is implied by this implementation.

`dispatch` now detaches by default. Windows uses the existing hidden WMI
`host_reservation_v1.wmi_create` helper to create the supervisor outside the app
job; Linux starts a new session. Both then run the normal host_slots claim. There
is no breakaway fallback. `--foreground` explicitly opts into a caller-dependent
run. `SPAWN-label.json` pins the creation response and method;
`DISPATCH-label.json` records the actual supervisor PID, process creation time,
claim ID, log, final runner exit and verified claim absence. The worker also
checks that the queued manifest hash still matches. Inspect these records and
the existing host_slots status before retrying an unconfirmed creation. A
successful spawn alone is not completion.

For the proposed CPU3c 32-vCPU/64-GB Pod, allocate at least 100 GB local disk,
measure actual free bytes, and retain 60 GiB after the 16 GiB build scratch plus
4 GiB run cap. The resident lease guard and independent local guardian must be
armed by the lead's lease controller before substantial work. Recover pinned
Linux binaries and all final/failed evidence to the registered cold/recovery
locations before releasing an ephemeral Pod. These commands prepare/verify the
runtime only; they are not permission to create a Pod.

On the Pod, use the pinned Rust 1.94.1 toolchain and an observed linker, Python
3.12 with psutil, the reviewed source archive, and the 83 MB explicit input archive.
Run `relocate_inputs.py` once after transfer; it verifies the explicit assets,
preserves the transport index and changes source specification paths only.
The lead records actual CPU topology, cgroup quota/memory and storage profile;
the existing `g115_d3_cloud_host_v1.observe/profile` can supply these fields when
`/workspace` is its own mount. If it is not, inspect the containing mount instead
of claiming that legacy helper passed. Configure allocator cores from the actual
allowed CPU set and limit claimed cores by observed effective quota:

```bash
export HOST_SLOTS_ROOT=/var/lib/mtg-node/host-lock
export MTG_HOST_LOCK_ROOT=/var/lib/mtg-node/host-lock
python3 tools/search_experiment/configure_slots.py --out /workspace/search-sharing-allocation-20261013/HOST_PROFILE.json
python3 tools/search_experiment/relocate_inputs.py --inputs /workspace/search-sharing-allocation-20261013/inputs
python3 -m unittest discover -s tools/search_experiment/tests -v
python3 python/tools/host_slots_v1.py status
```

Prepare the first small cost-only root queue, then have the lead review it:

```bash
python3 tools/search_experiment/make_manifest.py --root /workspace/search-sharing-allocation-20261013 --binary /workspace/pinned-runtime/regret_census_v1 --commit REVIEWED_SHA --host-slots python/tools/host_slots_v1.py --host-lock-root /var/lib/mtg-node/host-lock --resource-profile /workspace/search-sharing-allocation-20261013/HOST_PROFILE.json --hot-volume /workspace --hot-disk RunPod-CPU3c-local-100GB --cold-volume /workspace --cold-binary-root /workspace/pinned-binaries --cores-claimed 2 --eligible-workers 2 --decks 1 --budgets 16000 --calibrate --seeds search-sharing-allocation-20261013/calibration/cloud-r1 --out /workspace/search-sharing-allocation-20261013/CALIBRATION-MANIFEST.json
python3 tools/search_experiment/launch.py check --manifest /workspace/search-sharing-allocation-20261013/CALIBRATION-MANIFEST.json --phase qualify --workers 1 --label cal-cloud-probe
```

After review, the same guarded `dispatch` command starts its detached claim and
returns a persisted receipt. Complete cost-only calibration first, then qualify
one and increasing useful worker counts with fixed work limits and the same
calibration asset. Do not carry a Windows receipt across host, binary, source
path or storage changes. Generate full formal queues only when the measured
projection fits the lead's remaining CPU/dollar/lease/output ceilings. The
runtime carries no API key and has no provider creation or spending path.
For the approved leased Pod, add `--lease-state STATE --pod-id ID --lease-name
NAME --lease-deadline EPOCH` when preparing each queue. The launcher consumes
fresh resident `allow_new_dispatch`, its STOP files and the recovery deadline
before dispatch and during execution. Its detached supervisor takes over the
single progress writer only after the previous controller's work ends, observes
actual native CPU deltas and completed outputs, and writes the resident guard's
existing progress record. Idle review/preservation updates keep observations
current without inventing productive activity or setting `finished`. The next
controller takes over that writer; the existing idle and lease bounds remain.

New matched groups are assigned to currently available cores, then keep that
affinity for A/B/C. When workers do not exceed claimed cores, only one job runs
per core. Measured oversubscription is bounded and balanced when workers exceed
cores. Dispatch refuses concurrency greater than independent matched groups;
completion and throughput receipts report actual peak live workers. A requested
32-worker limit is not evidence that 32 workers were active.
