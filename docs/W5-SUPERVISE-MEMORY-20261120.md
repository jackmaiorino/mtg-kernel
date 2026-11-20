# W5 supervise index memory (2026-11-20, review round 3)

## Problem

`chains.py supervise --loop` kept every index record of every chain as parsed dicts for the life of the process
(`retention.IndexCache.records`, filled by a whole parse on start and extended every cycle, ended chains included).
On node1 the main chains' supervisor (pid 1568730) held 10.3 GB for 9.28M records and grew about 0.54 GB a day;
a restart on that code needs about 16 GB.

## What the cycle reads

- Every record, every cycle, but only scalar fields: `boundary` (`protected_paths`, the look's protected
  checkpoint), `ledger_usec` (`look` until LOOK.json exists, `first_at_or_past`), `update` (the replay sample's
  watermark, Stage 1a/1b only), and in pair plans `unix_seconds`, `ledger_usec`, `instance` (projection, running
  evidence) and `boundary` (`valid_records`, `ledger_at`, `select`, snapshots).
- Whole records only for a few: the last record, the last 65 (pair snapshot window), the look hit, the protected
  boundaries, the replay-sample todo past the watermark, and at a pair's finalize `select` and `prune_manifest`.
- `retention.plan` and `guard` read whole records only from `IndexCache.active`: every record until the first plan
  and guard settle those whose files are all gone (273 to 523 per chain afterwards on node1).

## Design

- `IndexRecords`: per record its line offset, length and CRC-32 and five scalar fields (`update`, `boundary`,
  `ledger_usec`, `unix_seconds`, `instance`) in `array` columns, 60 bytes a record. A value an array cannot hold
  exactly (None, an integer past 64 bits, a float in an int field) or a field the record lacks (`MISSING`) is kept
  beside the array for that record only; records without `after_state_sha256` are listed. A whole record is parsed
  again from its line on demand.
- `IndexView`: what `refresh` returns instead of a list copy, and what `active` is. Supported: len, truth, integer
  index (negative too), slices (a view), iteration, reversed, `in`, `index`, `count`, == and != against a list or a
  view (never equal to a tuple). List mutation and list arithmetic raise. Each read returns a fresh dict.
  `fields`, `column`, `where`, `lookup` serve the scalars with the call site's semantics (`rec.get` gives None,
  `rec[...]` raises KeyError at the record that lacks the field).
- Settlement where it always was: the first plan and guard see every record, as before. The plan is streamed
  (prunable records kept by position, pruned boundaries as a bitmap), so the first cycle never holds the index; it
  raises KeyError on a candidate without `after_state_sha256`, as the old plan's dict over every candidate did.
- Append-only, fail closed: every re-read record must have its cached length and the CRC-32 of its bytes (taken
  when it was appended), be one line, and carry the five cached fields; each refresh also re-reads the last cached
  line. A difference raises `IndexChanged` and prints a JSON line naming the chain, record, boundary and offset to
  stderr (supervise.log) before any decision uses the record. The old cache ignored an in-place rewrite (its parsed
  records stood until a whole read); the new one stops. A rewrite of a line the cycle never reads again (a settled
  record) is not detected, and changes nothing the cycle decides.
- Lines are split as `index()` splits them (`splitlines`: LF, CRLF, CR-only and the other line break characters)
  one line at a time, so memory is bounded by the longest line, not the file.
- `snapshot()` takes the cache (no whole read per attempt); the S-container migration streams and writes the same
  bytes `index_locked` writes.
- `supervise --guard-only` (recovery mode, below) and `IndexCache(read_only=True)` (the dry cycle) are new. A
  read-only cache keeps receipts past the file's end in memory and forgets them when the file grows, so an outside
  append gives no duplicate and no gap.

## Review round 2 findings (GPT-6 Astra, head 67a2f80a7) and their disposition

| # | Finding | Fixed | Test or evidence |
|---|---|---|---|
| 1 | a same-length rewrite with the five fields kept (a checkpoint path) passes | yes: CRC-32 per line | `test_a_checkpoint_path_rewritten_in_place_stops_the_plan` (boundary 64, not the last line, and boundary 66, the last) |
| 2 | the recovery's stop waits for a SUPERVISE.json that never changes | yes: `stop_failing`, bounded, SIGKILL after 20 s | rehearsal runs `supervise_swap.sh stop_failing` on a loop that fails before every GUARD write |
| 3 | 865 s is no bound; the real deadline | yes: time budget below, nice-0 dry cycle, escalation deadline | node1 evidence below |
| 4 | guard-only is not process-free; non-independent plans | documented; refuses pairs and filler plans | `test_guard_only_refuses_plans_with_pairs_or_fillers`, `test_guard_only_cycle_publishes_the_guard_and_deletes_nothing` |
| 5 | read-only cache: [1,2,2] | yes: reconcile on growth | `test_a_read_only_cache_reconciles_an_outside_append` |
| 6 | dry cycle claims | narrowed (below and in its docstring) | |
| 7 | missing `after_state_sha256` planned deletions | yes: KeyError as before | `test_a_record_without_its_after_state_hash_raises_as_before` |
| also | step 3 timing and torn tails | `asleep` before the kill; `check_tails`; cutting a torn last line is safe | `test_cutting_a_torn_last_line_back_is_safe`, rehearsal |

Earlier findings (round 1) stay fixed with their tests: duplicate boundaries
(`test_duplicate_boundaries_keep_what_the_first_plan_kept`), snapshots and migration streamed, per-record missing
fields, CR-only and long lines, the narrowed view.

## Cutting a torn last line

A supervisor killed inside an append (`IndexCache._refresh` or `_whole` writing new rows) can leave an unterminated
last line. If it parses, both `index()` and the cache read it and the next append adds its newline: leave it. If it
does not parse, both raise on every refresh, so neither supervisor can start. Cutting the file back to the previous
newline is safe: every index row is written from that update's durable receipt (`scan` reads
`iterations/NNNNNN/complete.json` and its `update.json`; the row is only appended after both exist), so the next
refresh finds the cut record's receipt again and writes the same line. `test_cutting_a_torn_last_line_back_is_safe`
cuts a torn line and checks that the file then ends byte for byte as it did before the tear; the rehearsal does the
same with `supervise_swap.sh cut_partial_tail`.

## node1 evidence (read only, as mtg, CPUs 1 and 17)

Index scan: 9,336,576 rows in c1..c6; no duplicate or non-monotonic boundary, boundary equal to position plus one in
every row, no row missing any field (`state` and `after_state_sha256` included), no None scalars, LF only, longest
line 471 bytes.

Plan: no pairs, stage 0, independent, `r_nom_usec` None: snapshots, selection and pair resumes are unreachable, and
`--guard-only` accepts it.

Python: root's default PATH holds no python3 before /usr/bin; `/usr/bin/python3 -> python3.11` (3.11.2, sha256
9bee109d..., the plan's `python` pin). The new loop names `/usr/bin/python3.11`; step 0 has the lead confirm the
running one with `readlink /proc/1568730/exe` (not readable as mtg).

### Time budget (from the code and node1's own records)

- A driver asks for admission at every segment start (`guard_admit`): it is refused when GUARD.json is older than
  `GUARD_MAX_AGE` = 900 s or its admission count has reached the verdict's ceiling, which supervise sets to the
  count at its census plus `GUARD_SEGMENTS` = 4 (node1 now: c1 8156 admitted, ceiling 8160). A refused driver polls
  every `GUARD_POLL` = 30 s and exits with EXIT_ROOM after `GUARD_WAIT` = 1800 s of waiting.
- Segment start intervals over the last 500 segments of c1..c3: minimum 127.3 s, median 133 to 135 s, maximum
  147.9 s. So after the last old GUARD (written seconds after its census) a driver can start at most 4 more segments;
  the earliest refusal is about 4 x 127.3 = 509 s after the census, and the first exit about 509 + 1800 = 2309 s
  (38.5 minutes) after the last old GUARD, if no new verdict raises the ceiling first. A new verdict at any time
  before that admits the waiting driver at its next poll.
- An exit is clean: `drive` returns at a segment boundary (the last segment complete and committed); the chain
  stays stopped until `chains.py run --plan $P --chain cN` restarts it (the old tree and plan, as
  `resume-node1.sh` does; not that script itself after the swap, whose supervise line would start the old loop).
  Its cost is the idle box time until then.
- The old supervisor's own cycles (9,696 consecutive report lines over 176 hours in supervise.log): median 4.3 s,
  p99 10.6 s; every 6 hours 260 to 300 s (the byte counter's full recount, `FULL_RECOUNT_SECONDS`), and once 797 s.
  A fresh supervisor's first cycle includes that full recount.

Dry cycles on node1 (`tests/supervise_dry_cycle.py`, the round 3 modules, as mtg, normal priority, CPUs 1 and 17,
`ulimit -v` 4 GiB; MemAvailable 7.3 GB at the start; 9,382,144 records):

| cycle | mode | time to the GUARD write | peak / steady RSS |
|---|---|---|---|
| 1 (fresh start) | full | 891.6 s | 903 MB / 763 MB |
| 2 | full | 344.8 s (the byte counter's second full count) | 903 MB / 767 MB |
| 3 | full | 10.9 s | 903 MB / 767 MB |
| 4 (fresh start) | guard-only | 722.9 s | 908 MB / 848 MB |
| 5 | guard-only | 345.7 s | 908 MB / 847 MB |

The process got 32% of a CPU (user 362 s, system 402 s; CPUs 1 and 17 are shared with pinned training work), so
these times are long; the production supervisor is not pinned. Cycle 2 kept 256 unindexed rows of c1 and c3 in
memory and cycle 3 had none: the supervisor had appended them meanwhile and the read-only cache read them from the
file (the reconciliation of round 2's finding 5, on real data). Earlier, at nice 19 (round 2 code): 865.5 s and
337.5 s for cycles 1 and 2.

What the dry cycle leaves out, bounded from the old supervisor's own log: the real index appends, index-lock waits,
retention deletions (at most 24 files a cycle), the hold purge (0 in every report) and the real GUARD and
SUPERVISE writes all happen inside the old supervisor's steady cycles, whose median is 4.3 s and p99 10.6 s. The
first real cycle should take the dry cycle's time plus seconds (plus the deletions that accumulate during the swap,
about one per live chain per minute). The real supervisor also stops pruning while it is down, which the dry cycle
does not see; that changes what it deletes, not how long its reads take.

Expected timeline of the swap (dry-cycle speeds): T0 the last old GUARD; the new loop starts within about 10 s;
its first GUARD about 900 s after T0. Drivers reach their ceilings from about 509 s after T0 and wait (each live
chain idles about 6 to 7 minutes); the first would exit at 2309 s.

Escalation deadline (step 5): the first driver exit (2309 s) minus the measured guard-only time (723 s, the dry
fresh-start guard-only cycle) minus 300 s is 1286 s (21.4 minutes) after the last old GUARD. If no new GUARD has
been written by then, run the recovery. The expected first GUARD at dry-cycle speed is about 900 s after T0, so the
margin to the deadline is about 6 minutes; a supervisor that needs longer than the dry cycle goes to guard-only.

### Dry cycle: what it covers

`tests/supervise_dry_cycle.py` runs the real `chains.cycle` of the new modules on the real plan and indexes and
writes nothing. Its audit hook refuses opening a path for writing or appending, `os.open` with a write, append,
create or truncate flag, remove, rename and replace, rmdir, chmod, chown, utime, truncate by path, link, symlink,
mkdir of a new directory, process creation and signals; the run first checks that a write and a remove are refused,
and its `refused` list was empty. The hook does not see writes through descriptors the process inherited (stdout,
stderr) or `flock` on read-only descriptors. It does not exercise the real lock paths (the indexes are read without
the index lock, through `IndexCache(read_only=True)`), and it skips `recover_unsupervised`, `release_ended_boxes`
and `purge_hold`; the cycle's writers are recorders. It loads the plan without its pin checks; step 2 checks the
pins on the supervisor's plan copy. It ran as mtg, which cannot write the root-owned chain root.

## Tests and equivalence

Unit tests (this PC, Python 3.14, 2 host-slot cores):

| Code | modules | run | failed | skipped |
|---|---|---|---|---|
| main e466fe21f (before) | index_cache, tools, replay_sample, byte_counter, chains, scratch, durable, pgrow | 171 | 0 | 29 |
| main ed34735d1 (round 3) | the same | 189 | 0 | 29 |
| node line 95343bfd6 (round 3) | that commit's index_cache, tools, replay_sample, byte_counter, chains, scratch | 109 | 0 | 14 |

New in round 3 (`CompactRecordsTests`): `test_a_checkpoint_path_rewritten_in_place_stops_the_plan`,
`test_cutting_a_torn_last_line_back_is_safe`, `test_a_read_only_cache_reconciles_an_outside_append`,
`test_a_record_without_its_after_state_hash_raises_as_before`, `test_guard_only_refuses_plans_with_pairs_or_fillers`.

Old against new on a real index (`tests/supervise_memory_equivalence.py`, run by
`N:/w5-supervise-mem-20261120/harness/pair_run_capped.py` in WSL Ubuntu, Python 3.10.12, each side under
RLIMIT_AS 6.0 GB): the live chain c1 is node1's c4 index (1,053,696 rows, sha256 9b52cc4d...) plus 2,521 updates
appended over 19 batches (receipts, checkpoints, trajectories, segment completions, a driver `index()` append in
its own process, an append cut before its newline, two supervisor restarts); the ended chain c2 holds the first
300,000 rows (so the old side stays under the cap). Stubbed: no trainer, no w5box instance, `Popen` mocked,
`time.time` held at whole seconds plus a half; wall-clock keys dropped. Compared per cycle: report, GUARD.json,
SUPERVISE.json, both index hashes, LOOK.json, retention logs, LOG.jsonl and every file with its size; then the pair
code's reads.

| Tools | lines | differing |
|---|---|---|
| eb005f097 against 95343bfd6 (node1's line) | 22 | 0 |
| e466fe21f against ed34735d1 (main) | 22 | 0 |

The old sides peaked at 5.76 GB, under the cap. (Round 1's full-size run on this PC reached 8.0 GB, past the 6 GB
limit; every run since has been capped.)

## Memory and time

| | round 2 (67a2f80a7) | round 3 (95343bfd6) |
|---|---|---|
| kept per record (200,000-record test, tracemalloc) | 69 bytes | 74 bytes |
| whole read of 2M records (WSL) | 14.6 s | 17.7 s |
| first plan of 2M records, every file gone (WSL) | 102 s | 106 s |
| peak RSS at 2M records (WSL) | 665 MB | 673 MB |

The CRC-32 adds 4 bytes a record (37 MB at 9.3M records) and about 1.6 microseconds a record to a whole read (about
15 s at 9.3M). Other 2M-record timings (round 3, WSL): steady refresh and plan 0.01 s each, an unreached look scan
0.15 s, protected paths 0.11 s, ledger_at 0.10 s, valid_records 0.31 s, ledger_series 0.41 s; finalization (pairs
only): select with a far replay start 31 s, prune_manifest 59 s. On node1 the dry cycle peaked at 903 to 908 MB
for 9.38M records.

## Deployment on node1 (written, not run)

### Can the main chains' tree take a minimal patch?

Yes. node1's `tools-src` is a clean checkout of `eb005f097`; the cycle code there matches main and
`replay_sample.py` is byte-identical. Branch `claude/w5-supervise-index-memory-eb005f0` carries the same diff.

### Why a plan copy and not `make_plan`

`load_plan` compares `plan['tools']` to the running tree's hashes, so the supervisor needs a plan naming the
patched files. `make_plan` on the live root would rewrite `chain.json` of running chains without `DRIVE.lock`, write
new arm files and overwrite `w5box-C.json` and `CHAINS-PLAN.json`, which the running drivers, `run` processes and
`resume-node1.sh` use with the old tree. The supervisor gets its own plan copy differing only in `tools` (three
hashes); nothing the drivers use changes. Recipe identity has no tool pin (`recipe_identity`), so no manifest or
recorded identity changes.

### Pins

Branch `claude/w5-supervise-index-memory-eb005f0`, commit 95343bfd6 (parent eb005f097). sha256:

    chains.py         57b3d1c48356caebf31ff526146ef50ea2e5d4d00548e17f86ad8eb871ea330f
    retention.py      00c70172bd2045725db116474022c81dfadcd21a3f8cc4f66436b6a1b97cb089
    replay_sample.py  15ed4a801c696b64468d1e3d71b4e972950c3072591a26a598e46633fbe51b2a
    supervise_swap.sh 8736007d1537e351826862bf6b1c643550b1347c0553d7ad0730b384f0a7780d   (the step file; not a pinned tool)

### Procedure (as root, `sudo -i`; times Eastern)

The steps are functions of one file, `tools/w5/supervise_swap.sh`, which the rehearsal runs as written. Staged on
node1 as `$S` (step 1). Environment for every step:

        C=/srv/work/chains-20261107; ROOT=$C/oc; PY=/usr/bin/python3.11; export C ROOT PY
        P=$ROOT/plan/oc-node1/CHAINS-PLAN.json; N=$C/tools-mem-95343bfd6; S=/tmp/mem-95343bfd6/tools/w5/supervise_swap.sh
        P2=$ROOT/plan/oc-node1/CHAINS-PLAN.supervise-95343bfd6.json

0. State: `ps -o pid,ppid,rss,etimes,args -p 1568722,1568730` (loop bash and python, RSS about 10.3 GB),
   `readlink /proc/1568730/exe` (/usr/bin/python3.11), `free -g`, and GUARD.json's age and ceilings:

        $PY -c "import json,time; g=json.load(open('$ROOT/GUARD.json')); print(round(time.time()-g['unix']), g['ok'], g['used_bytes'], g['ceilings'])"

1. Stage the tree. From Jack's PC:

        git -C D:/w5-supervise-mem-20261120-eb005f0 archive 95343bfd6 tools/w5/chains.py tools/w5/retention.py tools/w5/replay_sample.py tools/w5/supervise_swap.sh | ssh mtg@10.0.0.36 'mkdir /tmp/mem-95343bfd6 && tar -x -C /tmp/mem-95343bfd6'

   On node1:

        mkdir $N && cp -a $C/tools-src/tools $N/tools && find $N/tools -name __pycache__ -prune -exec rm -r {} +
        install -m 644 /tmp/mem-95343bfd6/tools/w5/{chains,retention,replay_sample}.py $N/tools/w5/
        sha256sum $N/tools/w5/{chains,retention,replay_sample}.py $S     # the pins above
        diff -rq -x __pycache__ $C/tools-src/tools $N/tools              # exactly the three files

2. The plan copy (prints the three changed keys, then `ok`; it checks every pin of the copy):

        bash $S plan_copy $P $P2 $N/tools/w5

2a. The dry cycle (run on node1 as mtg, read only; results above). Rerun it if the code changes:

        python supervise_dry_cycle.py bundle <eb005f0 worktree>/tools/w5 > dry.py        # on Jack's PC
        ssh mtg@10.0.0.36 'cd /tmp && taskset -c 1,17 bash -c "ulimit -v 4194304; exec /usr/bin/python3.11 -B - --plan /srv/work/chains-20261107/oc/plan/oc-node1/CHAINS-PLAN.json --tools-src /srv/work/chains-20261107/tools-src/tools/w5 --cycles 3 --guard-only-cycles 2"' < dry.py

   Go on only if it exits 0 with `refused` empty and every cycle's `guard.ok` true.

3. Swap. Note the time of the old supervisor's last GUARD (`unix` in GUARD.json): the escalation deadline counts
   from it.

        bash $S stop_between_cycles $P      # waits for a cycle's end, checks the supervisor is asleep (state S,
                                            # no CPU in 2 s, report < 10 s old), stops loop then supervisor,
                                            # SIGKILL after 20 s, verifies nothing of $P is left; exit 0 or stop here
        bash $S check_tails                 # each index ends in a newline and its last line parses
        # only if check_tails names an index whose last line does not parse:
        #   bash $S cut_partial_tail <that index>; bash $S check_tails
        $PY -c "import json; print(json.load(open('$ROOT/GUARD.json'))['unix'])"   # T0, the last old GUARD
        bash $S start_loop $N/tools/w5 $P2  # prints LOOP_PID and the supervisor's pid

   The loop logs each exit with its real code (`rc=$?` first; the current loop's `echo "$(date -Is) ... $?"`
   always logs 0, because `$(date)` runs before `$?` is expanded).

4. Check at 3 minutes: `bash $S pids $P2` (one loop, one python), RSS under 1 GB, `tail -c 4000
   $C/records/supervise.log | grep -c Traceback` (0), no new line in `supervise-restarts.log`, the three drivers
   3867287..9 alive.

5. A report line after the swap with `"ok": true`, `used_bytes` within a few GB of step 0, `recovered`, `kills`,
   `oom_failures` empty; GUARD.json newer than T0 with ceilings for c1..c3; RSS steady (0.7 to 0.9 GB) across two
   readings; index files growing. Deadline: if GUARD.json is not newer than T0 by T0 + 1286 s
   (21.4 minutes), or the log shows a Traceback in every cycle, run the recovery.

6. Record: copy `resume-node1.sh` to `resume-node1-mem.sh`, its supervise line replaced by
   `bash $S start_loop $N/tools/w5 $P2` and its pgrep check by `bash $S pids $P2`; the chain `run` lines keep the
   old tree and `$P`. Do not run the old `resume-node1.sh` after the swap.

### Forward recovery: the new supervisor fails before publishing (rehearsed)

        bash $S stop_failing $P2                        # bounded: loop, then supervisor, SIGKILL after 20 s, verified
        bash $S start_loop $N/tools/w5 $P2 --guard-only

`--guard-only` (refused for a plan with pairs or filler boxes) refreshes every index (appending rows), records
looks, and publishes GUARD.json and SUPERVISE.json (`"guard_only": true`). It still runs the cycle's safety steps,
which act on processes and files: `w5box.py recover` for a chain whose w5box supervisor died (it stops a running
instance), OOM ends (END.json, and for a box with no driver left the unmount and removal of its unpersisted scratch
segment), kill requests for ended chains, and the release of ended chains' boxes. It makes no pair decision,
snapshot or selection, stages no replay sample, runs no rolling retention, purges no hold folder and starts no
filler. Used bytes only grow without retention; the guard refuses admission at the cap. On node1 the headroom is
about 224 GB (cap 560 GB, used 294 GB, reserved 42 GB) and the old supervisor's retention removed 26.6 GB in the
last 24 hours (1.1 GB an hour): roughly 8 days of guard-only before the cap. It does not help a fault in reading the
indexes or in the guard; then `stop_failing` and leave the chains to exit cleanly (about 38 minutes after T0), to
restart them with `chains.py run` once a supervisor runs.

Going back to the old code is not safe on node1: its fresh start parses every record (about 16 GB for 9.3M).

### Rehearsal (WSL Ubuntu, Python 3.10.12, `tests/supervise_swap_rehearsal.py`, scaled: loop 5 s, pause 3 s)

| t (s) | step | result |
|---|---|---|
| 3.0 | `start_loop` | exit 0: started: loop 16635 LOOP_PIDS 16635 ; PYTHON_PIDS 16637 / LOOP_PID 16635 |
| 10.0 | driver started | pid 18668 (old tree) |
| 35.0 | check: old supervisor running | GUARD age 3.9 s, ok True; segments 0 -> 13, rows contiguous True; tracebacks 0; last restart lines [] |
| 35.0 | stage tree | diff lists 3 files |
| 35.2 | `plan_copy` | exit 0: ['chains.py', 'replay_sample.py', 'retention.py'] / ok |
| 35.5 | dry cycle | exit 0, refused [], cycles [('full', 0.0), ('full', 0.0), ('guard-only', 0.0)] |
| 40.5 | `stop_between_cycles` | exit 0: stop_between_cycles: loops [ 16635 ] supervisors [ 16637] / asleep: pid 16637 state S cpu_ticks 14 -> 14 report_age 4s / stopped: /tmp/w5-swap-rehearsal/chains-rehearsal/oc/plan/oc-rehearsal/CHAINS-PLAN.json |
| 40.5 | gap begins | GUARD age 4.3 s, driver alive True |
| 40.5 | `check_tails` | exit 1: BAD no final newline: /tmp/w5-swap-rehearsal/chains-rehearsal/oc/chains/c1/chain.index.jsonl |
| 40.5 | torn tail injected | check_tails exit 1 |
| 40.6 | `cut_partial_tail` | exit 0: cut /tmp/w5-swap-rehearsal/chains-rehearsal/oc/chains/c1/chain.index.jsonl from 23325 to 23268 bytes |
| 40.6 | `check_tails` | exit 0: ok /tmp/w5-swap-rehearsal/chains-rehearsal/oc/chains/c1/chain.index.jsonl |
| 40.6 | tail cut back | same bytes as before the tear: True |
| 42.6 | `start_loop` | exit 0: started: loop 24395 LOOP_PIDS 24395 ; PYTHON_PIDS 24397 / LOOP_PID 24395 |
| 42.6 | new loop: first GUARD | 0.1 s after the loop started |
| 57.6 | check: new supervisor, 3-minute check (scaled) | GUARD age 1.8 s, ok True; segments 16 -> 24, rows contiguous True; tracebacks 0; last restart lines [] |
| 87.7 | check: new supervisor, 10-minute check (scaled) | GUARD age 1.7 s, ok True; segments 24 -> 38, rows contiguous True; tracebacks 0; last restart lines [] |
| 96.7 | `stop_between_cycles` | exit 0: stop_between_cycles: loops [ 24395 ] supervisors [ 24397] / asleep: pid 24397 state S cpu_ticks 32 -> 32 report_age 5s / stopped: /tmp/w5-swap-rehearsal/chains-rehearsal/oc/plan/oc-rehearsal/CHAINS-PLAN.supervise-new.json |
| 96.8 | `plan_copy` | exit 0: ['chains.py', 'replay_sample.py', 'retention.py'] / ok |
| 98.9 | `start_loop` | exit 0: started: loop 2830 LOOP_PIDS 2830 ; PYTHON_PIDS 2832 / LOOP_PID 2830 |
| 118.9 | check: faulty supervisor (raises before every GUARD write) | GUARD age 27.9 s, ok True; segments 43 -> 44, rows contiguous True; tracebacks 2; last restart lines ['supervise exited 1', 'supervise exited 1'] |
| 118.9 | faulty loop never published | GUARD unchanged: True |
| 118.9 | `stop_failing` | exit 0: stop_failing: loops [ 2830 ] supervisors [ 5957] / stopped: /tmp/w5-swap-rehearsal/chains-rehearsal/oc/plan/oc-rehearsal/CHAINS-PLAN.supervise-faulty.json |
| 120.9 | `start_loop` | exit 0: started: loop 6873 LOOP_PIDS 6873 ; PYTHON_PIDS 6875 / LOOP_PID 6873 |
| 120.9 | guard-only: first GUARD | 0.1 s after the loop started |
| 146.0 | check: guard-only recovery on the faulty tree | GUARD age 1.8 s, ok True; segments 44 -> 52, rows contiguous True; tracebacks 2; last restart lines ['supervise exited 1', 'supervise exited 1'] |
| 146.0 | guard-only deletions | deleted 0, added 160 |
| 155.1 | `stop_between_cycles` | exit 0: stop_between_cycles: loops [ 6873 ] supervisors [ 6875] / asleep: pid 6875 state S cpu_ticks 23 -> 23 report_age 5s / stopped: /tmp/w5-swap-rehearsal/chains-rehearsal/oc/plan/oc-rehearsal/CHAINS-PLAN.supervise-faulty.json |
| 157.1 | `start_loop` | exit 0: started: loop 14295 LOOP_PIDS 14295 ; PYTHON_PIDS 14297 / LOOP_PID 14295 |
| 177.1 | check: back on the new tree | GUARD age 1.8 s, ok True; segments 57 -> 67, rows contiguous True; tracebacks 2; last restart lines ['supervise exited 1', 'supervise exited 1'] |
| 186.1 | `stop_between_cycles` | exit 0: stop_between_cycles: loops [ 14295 ] supervisors [ 14297] / asleep: pid 14297 state S cpu_ticks 25 -> 25 report_age 5s / stopped: /tmp/w5-swap-rehearsal/chains-rehearsal/oc/plan/oc-rehearsal/CHAINS-PLAN.supervise-new.json |
| 186.1 | done | segments 71, index rows 272, contiguous True |

The old tree is node1's (eb005f097), the new one 95343bfd6's; one live chain (stand-in trainer, 4-update segments) and one never-started chain. The faulty tree is the new tree with `retention.plan` made to sleep 6 s and raise, so it fails before every GUARD write with its supervisor alive most of the time (`stop_failing` killed a running one, pid 5957). Its exits were logged with their real code 1. Full transcript: `N:/w5-supervise-mem-20261120/swap-rehearsal-r3.transcript.jsonl`.

### Not verified

- Root's `readlink /proc/1568730/exe` (not readable as mtg; step 0).
- The dry cycle at normal priority shares CPUs 1 and 17 with pinned work; the real supervisor is not pinned.
- The rehearsal ran on WSL with a stand-in trainer, no w5box instance and no cgroups; node1's drivers run under
  w5box. The swap does not touch w5box, but its interaction was not rehearsed.
- The other node1 supervisors (forks2, wave2, wave2-cont, batch-ladder) keep the old cache and grow with their
  chains; the main-line commit covers them when they move.
