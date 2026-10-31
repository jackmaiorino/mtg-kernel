# Core slots on reserved hosts

`python/tools/host_slots_v1.py` lets untimed work use the cores that timed work leaves idle. It works alongside the whole-host reservation (`python/tools/host_reservation_v1.py`) and does not replace it. The tool is kept byte-identical with spellbench `python/tools/host_slots_v1.py`; change both together.

The v1 lock at `C:/mtg-node/host-lock/<HOST>.lock` keeps its exact meaning. This tool never writes it, and existing launchers, rated runs and CI pause hooks need no change. Slot records live beside it, in `<HOST>.slots/`.

## Untimed work: claim cores

```
python host_slots_v1.py run --lane LANE --work-id WORK --cores N [--wait] [--wait-timeout S] [--priority below_normal] -- COMMAND...
python host_slots_v1.py submit --log FILE --lane LANE --work-id WORK --cores N -- COMMAND...
```

`run` claims N free cores and pins `COMMAND` and every process it starts to them, at below-normal priority by default. On Windows this uses a job object, so the whole tree also dies if the runner is killed. When the tree ends, the claim is freed.

With `--wait`, the claim queues and starts in arrival order once cores are free. Each poll lists the slots folder first: if the listing fails, or a queue file ahead of the request exists but cannot be read, the true order is unknown and the request waits. Without `--wait`, it exits with code 3 when there is no room. `submit` starts a detached waiting `run` that logs to `FILE`, and returns the claim id immediately.

On Windows, a bare `bash` runs Git Bash (the first on `PATH`, then `C:/Program Files/Git/bin/bash.exe`): Windows would otherwise start WSL's `bash.exe` from System32, which cannot see drive paths such as `D:/`. WSL's bash itself is refused (exit 6) before the request takes a queue turn.

The command sees `HOST_SLOTS_CLAIM` and `HOST_SLOTS_CORES`. On Windows, spellbench's `usable_cpus()` (`spellbench.arena.machine`) honors the pinned cores, so its throughput planning sizes workers to the claim.

## Backfill: small jobs ahead of a waiting head

The queue is first come, first served, so a head request that does not fit blocks smaller requests behind it while cores sit idle. A request that declares a run time limit may start ahead of the head:

```
python host_slots_v1.py run --wait --max-seconds 900 --lane LANE --work-id WORK --cores 4 -- COMMAND...
```

It starts only when all of these hold:

- it fits in the cores free now, without taking any background claim's cores (it never suspends CI work);
- it and the head are both ordinary requests (later ordinary requests overtake a background head, so its protection could not hold), and the head was queued under the current per-host config (requests from launchers older than backfill are never jumped);
- no earlier request behind the head is also eligible, and every queue, claim and delay file reads (while one does not, the true head, its claims or its window are unknown, so nothing backfills);
- no timed reservation is held, and the host has at most 12 live claims (backfills and background claims included);
- it cannot delay the head, or its delay is bounded (below). The live claims, backfills included, could end in any order, so every subset of them is checked: whenever ending that subset would let the head start with no backfills, the head must still start with the remaining backfills running, and an ordinary head must not need any background claim's cores that it would not have needed anyway (it would wait for them to suspend).

**Bounded delay.** A request that could delay the head (some release order would leave the head waiting on backfills) may still start when:

- the head declared its own `--max-seconds H` and cannot start right now (a head that can start now is never jumped, by any backfill);
- every delaying backfill admitted for that head, this one included, fits in one window from the first one's start to the latest deadline, of at most min(15 minutes, H / 10). The head can only be held up while one of them runs, so that window bounds its total delay, however many there are. The window is kept in `delay-<head id>.json` beside the claims, so it does not slide when earlier backfills end, and it is measured on the host's monotonic clock, so a clock change cannot stretch it. It is removed once its head leaves the queue;
- no earlier delaying backfill for that head is overdue, and its window file reads (a file that exists but cannot be read refuses);
- the request comes from a launcher that applies this rule (its ticket records `backfill_policy: 2`). An older launcher's ticket is judged by the zero-delay rule it applies itself, so it never holds back the requests behind it.

Every backfill records `deadline_mono`, which `status` uses for `overdue`. A delaying one also records `delays_head`, `delay_bound_seconds` (the window so far) and `delay_limit_seconds`. `status` shows the bound and the limit for every backfill (0 and none for one that cannot delay its head). The bound relies on backfills keeping their declared limits, which their own runners now enforce (below).

The guarantee covers claims ending. A timed reservation outranks the queue and can block any cores, so no backfill starts while one is held, and none can be proof against one acquired later. A later timed reservation suspends a backfill on its cores exactly as it suspends any other claim; backfill adds no suspension of its own. The no-delay check does not use durations; the bounded-delay rule uses only the declared limits. `--max-seconds` is at most 30 days. A request without `--max-seconds` waits its turn as before.

**Past `--max-seconds`.** Every claim that declares `--max-seconds S` records its deadline (`deadline_mono`, `deadline_at`) and `on_overdue`. Once S has passed since admission, the claim's own runner applies `--on-overdue`:

- `demote` (the default): it republishes the claim as background work (`background: true`, `overdue: true`). From then on every ordinary claim and timed hold may take its cores; it suspends while they hold them and resumes afterwards, exactly as CI does. Nothing is killed, and a delaying backfill can hold its head up only until its deadline plus one handoff. A shared claim first narrows to its guarantee.
- `kill`: it ends the processes of its own job object, never itself and never another claim (each process is matched to the job by handle before it is ended), then releases the claim. Only a launcher that asks for it explicitly gets it.
- `flag`: it only marks the claim `overdue`. Such a request may still backfill when it cannot delay the head, but never with a bounded delay, because nothing would enforce the bound.

Timed work is never a claim, so none of this touches it. A human-facing claim (the play client) should not declare `--max-seconds`, or should pass `--on-overdue flag`. The #427 window is unchanged: no delaying backfill starts while an earlier one for the same head is overdue. The guarantee covers the head at the time of admission: if that head leaves the queue, the next one may wait for backfills already running.

Deploying: ticket names are chosen under the admission mutex, but older versions choose them before taking it. Replace every deployed copy while no `host_slots_v1.py` process of any version is running, so no older launcher can publish an earlier-named ticket after a backfill is proved.

## Interaction with timed work

Whenever the v1 lock is held and leaves a claim no room, every process in the claim is suspended. It resumes when the room comes back. A lock leaves no room when it has no core declaration, or when its declaration overlaps the claim's cores. A claim that would start under such a lock waits before starting.

Untimed work therefore never runs on a timed run's cores. It may, however, run on other cores of the same machine at the same time, sharing cache, memory bandwidth and the power budget.

## Timed work: give back the cores you do not need

Inside the v1 supervisor, wrap the timed command:

```
host_reservation_v1.py supervise --token T -- python host_slots_v1.py timed --cores 0-7 -- COMMAND...
```

`timed` checks that `MTG_HOST_RESERVATION_TOKEN` holds the lock and declares the cores. It waits for any claim on those cores to suspend; if one does not within 60 s, it refuses with exit 6. It then pins `COMMAND` to those cores and removes the declaration when the command's tree ends. Untimed claims may then use the rest of the host.

Between `acquire` and the `timed` declaration, a lock with no recorded cores reserves the whole host, so every claim pauses for a few seconds even when the declaration will cover other cores. To avoid that, record the cores when acquiring:

```
host_reservation_v1.py acquire --lane L --work-id W --release-condition TEXT --cores 0-7
```

Pass `--cores` only when the work is pinned to those cores, normally by `timed --cores` with the same set; unpinned work under such a lock would share cores with claims. The lock then reserves only those cores from the moment it exists, and again after `timed` ends while the lock is still held. A live `timed` declaration takes precedence while it runs. A held lock with neither recorded cores nor a live declaration still reserves the whole host, which covers every launcher written before this tool.

Declaring is a change to a run's execution. For a rated run, qualify throughput with the same declaration and the same background load before the commitment names it. Launchers whose busy refusal matches process names (for example `java`) also see suspended claim processes.

## Per-host config

The optional file `<HOST>.slots/config.json` controls what claims may use:

```json
{"cores": "23-0", "keep_free": 0, "priority": "below_normal"}
```

- `cores` lists the cores untimed work may use, in order of preference. The default is every logical CPU, highest-numbered first, so claims stay off the low-numbered P cores of hybrid Intel parts, which timed work usually declares.
- `keep_free` is the number of those cores that are never handed out.
- `ci_floor` (default 0) reserves the last that many cores in the preference order for background (CI) claims. Ordinary, shared and backfilled work never takes them; CI claims try them first and may also use any other free core. It is separate from `keep_free`, which no claim takes. A config without `ci_floor` keeps the digest older launchers compute.
- `priority` is the default priority for claims.

On the compute host, its owner's use comes first. Set `keep_free` so the owner always has room, and keep `below_normal`. Use `topology` to see which logical CPUs share a physical core and each core's efficiency class (P cores rank higher).

## Status

`python host_slots_v1.py status` prints:

- the timed reservation (lane, work and declared cores, never the token)
- live claims and their state (`running` or `suspended`)
- the queue
- backfilled claims, the head each one started ahead of, and whether it is past its declared limit
- the free cores

Records whose process has exited are removed as they are read.

Background queue filenames sort after ordinary queue filenames. This keeps
legacy filename-FIFO launchers compatible with the priority-aware CI allocator
and avoids an admission deadlock when both generations share a registry.

## Limits

- On Linux (WSL runners, tests), containment is a process group. A process that leaves the group escapes affinity bookkeeping and suspension.
- Cores above 63 (a second Windows processor group) are not supported.
- During a v1 reclaim the lock is briefly absent, so claims may resume for one poll interval (2 s).
## Per-request core pools

`run`, `submit` and `adopt` accept `--core-pool 8-15 --pool-reason R` to restrict one request to those configured
logical CPUs. R is one of `timing`, `isolation`, `determinism`, `play-client` or `hardware`, and is recorded on
the ticket and claim and shown by `status`. A pool is a hard restriction: the request waits while its pool lacks
room even if CPUs outside the pool are free, so a pool without a reason is refused (exit 2) before any ticket is
written. Without a reason, drop the pool and the scheduler places the job. The reservation, ordinary and background claims, queue order and the shared mutex still
govern admission, and the host configuration is unchanged.

## Shared claims: lend what you do not use

```
python host_slots_v1.py run --shared --cores 8 --min-cores 1 --lane LANE --work-id WORK -- cargo build --release -j 8
```

A shared claim guarantees `--min-cores` cores (default 1; it queues for those, so the queue and backfill see an
M-core request) and runs on up to `--cores`. The cores beyond its guarantee are lent:

- Any later claim may take a lent core. The shared claim narrows its job's affinity off that core and only then
  republishes its cores; the later claim starts its command after that (the same handshake as with background
  claims, at most one or two polls).
- While any request is queued, a shared claim gives back every lent core, so it lends only while nobody waits. It
  also gives back lent cores that timed work declares, and enough of them to keep the occupied cores within the
  configured cores minus `keep_free`.
- While nothing is queued, it grows (every 10 s at most) onto cores no claim of any class runs on, within
  `keep_free` and its own pool. It never takes a background claim's cores.
- Timed work that overlaps its guaranteed cores suspends it exactly like any claim.

Use it for release builds, clippy, test compiles and other bursty work. Never use it for timing, isolation or
deterministic studies: their cores must stay exclusive, so they stay ordinary claims (or `timed`). The command's
`HOST_SLOTS_CORES` names only the guarantee, since the rest can be taken back at any time. Tools size their
parallelism once at start, so pass the job count (`cargo build -j N`).

Shared claims are off on a host until its `config.json` says `"lending": true` (kept out of the config digest).
Without it, `--shared` exits 2 before any ticket. Set it only after an inventory shows that every copy of this tool
that can queue on that host is c543e1b4 (mtg-kernel e2657b8c, #427) or later: older backfill engines, the #407
one included, do not honor the lending marker below. As a runtime guard, lenders also give back everything and
stop growing while an older backfill engine shows itself (a queued request with a limit but no
`backfill_policy`, or a backfill claim without `deadline_mono`). That guard cannot see an older copy before it
queues, so the config gate is what keeps such copies away.

Records: `shared`, `guaranteed`, `max_cores`, and `cores` (the current affinity). Older launchers read only
`cores`, so they treat a shared claim's whole current affinity as held and never land on a lent core. While a
shared claim lends any core it keeps `delay-lending-<claim id>.json` in the slots folder. The file is deliberately
not a JSON object: every backfill engine at or after c543e1b4 refuses to backfill while a delay file cannot be read,
so none of them proves a backfill while a lent core could still be given back. The marker is created before any lent
core is published and removed only once the read-back record lends nothing. This version skips the markers when
reading, and it (and `status`) removes any whose claim is gone.

## Utilization watcher

```
python host_slots_v1.py watch [--interval 30] [--once] [--duration S] [-- NOTIFY_CMD...]
python host_slots_v1.py watch-report [--since ISO] [--until ISO]
```

`watch` samples per-CPU busy time (Windows `NtQuerySystemInformation`, Linux `/proc/stat`) and the slot records,
without the admission mutex and without deleting anything, and never changes a claim. It writes to
`<root>/<HOST>.util/`:

- `util-YYYYMMDD.jsonl`, one line per sample: per-CPU busy fraction and holder kind (timed, held, guaranteed,
  lent, background, unclaimed; first match wins), the queue with each ticket's cause, and the integrals
  `capacity_cs`, `idle_cs` by kind, `eligible_idle_cs` (idle capacity queued tickets were allowed to use, taken in
  queue order without double counting and attributed to their causes; an upper bound, since a request is
  indivisible) and `coincident_idle_cs`; plus a `claim_end` line with each claim's busy and held core-seconds;
- `nudges.jsonl`, one line per new flag, and `watch-state.json`, whose current flags `status` shows as `watch`.

Flags: `low_use`, a running ordinary claim whose held cores average under 30% busy for 10 minutes (an 8-core build
with one busy CPU is 12.5%); `queued_idle`, a ticket queued 60 s while a core is unclaimed beyond `keep_free`, with
its cause (`pool`, `head_of_line`, `no_max_seconds`, `timed`, `keep_free`, `too_few_free`). With a notify command
(for example collab `tools/fleet_note.py`), each new flag runs it with `--to LANE --title T --body B --sender
"host_slots watch"`, at most 6 per hour.

Runners also append `admit`, `start` and `end` events to `events-YYYYMMDD.jsonl` (lane, cores, queued seconds,
`took_lent`, and `old_rules_room`: whether the rules before shared claims had room on the same snapshot). A failed
append never blocks a claim, so event counts are lower bounds. `watch-report` sums both logs: observed hours and
coverage, queued hours, mean utilization, eligible idle core-hours per queued hour and by cause, shared share of
claimed CPU time, lent transfers, handoff time and censored admissions.
