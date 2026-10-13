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
bootstrap intervals are 97.5 percent for the two primary comparisons. The 16k
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
