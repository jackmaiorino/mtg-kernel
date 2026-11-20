# W5 supervise index memory (2026-11-20, review round 2)

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
  and guard settle those whose files are all gone (a few hundred per chain afterwards; 273 to 523 on node1).

## Design

- `IndexRecords`: per record its line offset and length and five scalar fields (`update`, `boundary`,
  `ledger_usec`, `unix_seconds`, `instance`) in `array` columns, 56 bytes a record. A value an array cannot hold
  exactly (None, an integer past 64 bits, a float in an int field) or a field the record lacks (`MISSING`) is kept
  beside the array for that record only. A whole record is parsed again from its line on demand.
- `IndexView`: what `refresh` returns instead of a list copy, and what `active` is. Supported: len, truth, integer
  index (negative too), slices (a view), iteration, reversed, `in`, `index`, `count`, == and != against a list or a
  view (never equal to a tuple). List mutation and list arithmetic raise. Each read returns a fresh dict.
  `fields`, `column`, `where`, `lookup` serve the scalars with the call site's semantics (`rec.get` gives None,
  `rec[...]` raises KeyError at the record that lacks the field). Used by `first_at_or_past`, `ledger_series`,
  `select`, `snapshot`, `chains.valid_records`, `ledger_at`, `protected_paths`, the pair evidence loop and
  `replay_sample.since`; lists keep their old code.
- Settlement where it always was: the first plan and guard see every record, as before. The plan is streamed
  (prunable records kept by position, pruned boundaries as a bitmap), so the first cycle never holds the index.
- Append-only, fail closed: every re-read record must have its cached length, one line and all five cached
  fields; each refresh also re-reads the last cached line. A difference raises `IndexChanged` and prints a JSON line
  naming the chain, record, boundary and offset to stderr (supervise.log) before any decision uses the record.
  The old cache ignored an in-place rewrite (its parsed records stood until a whole read); the new one stops.
- Lines are split as `index()` splits them (`splitlines`: LF, CRLF, CR-only and the other line break characters)
  one line at a time, so memory is bounded by the longest line, not the file.
- `snapshot()` takes the cache (no whole read per attempt); the S-container migration streams and writes the same
  bytes `index_locked` writes.
- `supervise --guard-only` (recovery mode, below) and `IndexCache(read_only=True)` (the dry cycle) are new.

## Review round 1 findings (GPT-6 Astra, head 9cfbd271f) and their disposition

| # | Finding | Fixed | Test |
|---|---|---|---|
| 1 | early settlement changes the first plan with duplicate boundaries | yes: no settlement at load | `test_duplicate_boundaries_keep_what_the_first_plan_kept` |
| 2 | handoff before validating startup | yes: dry cycle, python path, loop exit check, guard-only recovery, rehearsal (below) | `test_guard_only_cycle_publishes_the_guard_and_deletes_nothing`, `test_a_read_only_cache_writes_nothing` |
| 3 | same-size rewrite mixes versions | yes: re-read checks, fail closed | `test_a_line_rewritten_in_place_stops_before_any_decision` |
| 4 | full-list allocation in snapshots and migration | yes: both streamed; neither reachable on node1 (no pairs; every row has `state`) | `test_snapshots_read_the_cache_not_the_whole_index`, `test_the_streamed_migration_writes_what_index_writes` |
| 5 | a missing field disables a column; finalization cost | yes: per-record `MISSING`; timings below | `test_odd_values_read_back_exactly_and_a_missing_field_reads_every_record` |
| 6 | CR-only and long lines held whole | yes: line-at-a-time splitter | `test_cr_only_and_long_lines_with_bounded_memory`, `test_lines_are_split_as_index_reads_them` |
| 7 | view not list-equivalent; tuple equality | narrowed and tested | `test_a_view_reads_as_the_index_list` |

## node1 evidence (all read only, as mtg, CPUs 1 and 17, nice 19)

Index scan (`node_index_scan.py`, streamed, 7 s per 2M rows): 9,336,576 rows in c1..c6 (2,073,856; 2,013,440;
2,076,672; 1,053,696; 1,068,032; 1,050,880). Duplicate boundaries 0, non-monotonic 0, boundary equal to position
plus one in every row, update equal to boundary minus one in every row, no row missing any field (`state`
included), no None scalars, no odd types, LF line ends only, longest line 471 bytes, no empty lines.

Plan: `pairs` is empty, stage 0, independent, `r_nom_usec` None. So `cut_snapshots` and `select`/`prune_manifest`
are unreachable, and the S-container migration is unreachable (no row lacks `state`).

Python: the supervisor (pid 1568730, uid 0) runs `python3`. Root's default PATH (`/etc/login.defs` ENV_SUPATH:
/usr/local/sbin, /usr/local/bin, /usr/sbin, /usr/bin, /sbin, /bin) holds no python3 before /usr/bin; /sbin and /bin
link to /usr; `/usr/bin/python3 -> python3.11` (3.11.2), sha256 9bee109d..., equal to the plan's `python` pin. The
procedure starts the new loop with `/usr/bin/python3.11` explicitly. (Root's environment and `/proc/<pid>/exe`
are not readable as mtg; step 0 has the lead confirm with `readlink /proc/1568730/exe`.)

Dry cycle (`tests/supervise_dry_cycle.py`, the new modules piped in a bundle, nothing written on node1):
`ulimit -v 4194304` (4 GiB), two cycles of the real `chains.cycle` on the real plan and all six indexes.

| | cycle 1 (fresh start) | cycle 2 |
|---|---|---|
| wall time to the GUARD write | 865.5 s | 337.5 s |
| records | 9,343,488 | 9,346,048 (256 unindexed receipts kept in memory) |
| active after the cycle | 273 to 523 per chain | the same |
| would delete | 0 | 2 (c2, c3) |
| peak / steady RSS | 841 MB / 708 MB | 841 MB / 713 MB |

The process got 34% of one CPU (load 18 to 22; user 202 s, system 209 s, 20 min wall for both cycles).
Cycle 2 is also long because the byte counter recounts every sealed segment until it has seen each one twice
(`ByteCounter`, unchanged); from the third cycle it reuses them. The production supervisor is not pinned or niced,
so these are upper bounds. No-write proof: before any W5 module loads, `sys.addaudithook` refuses opens for
write or append, write-flag `os.open`, remove, rename, rmdir, chmod, chown, utime, truncate, link, symlink, mkdir
of a new directory and process start; the run first checks that the hook refuses a write and a remove; the run's
`refused` list was empty; the user was mtg (uid 1000), which cannot write the root-owned chain root; the cycle's
writers were replaced by recorders (only GUARD.json and SUPERVISE.json would have been written; no log line, no
request); `recover_unsupervised`, `release_ended_boxes` and `purge_hold` were skipped (they lock or start
processes).

Gap tolerance: a driver needs an admission per segment (about 138 s at node1's 1.85 updates a second); the last
verdict admits 4 more, so a driver is refused about 9 minutes after the last old GUARD (or at 15 minutes, the
900 s staleness), then waits up to 1800 s before it exits. The new supervisor must publish within about 39
minutes of the last old GUARD; the measured upper bound for the first cycle is 14.4 minutes.

## Tests and equivalence

Unit tests (this PC, Python 3.14, 2 host-slot cores; modules index_cache, tools, replay_sample, byte_counter,
chains, scratch, durable, pgrow):

| Code | run | failed | skipped |
|---|---|---|---|
| main e466fe21f (before) | 171 | 0 | 29 |
| round 1, e7bf9f37d | 177 | 0 | 29 |
| round 2, 8a13f4527 | 184 | 0 | 29 |
| node line round 2, 67a2f80a7 (that commit's index_cache, tools, replay_sample, byte_counter, chains, scratch) | 104 | 0 | 14 |

Old against new on a real index (`tests/supervise_memory_equivalence.py`, run by
`N:/w5-supervise-mem-20261120/harness/pair_run_capped.py` in WSL Ubuntu, Python 3.10.12, each side under
RLIMIT_AS 6.0 GB). Input: node1 `c4/chain.index.jsonl` (1,053,696 rows, sha256 9b52cc4d...) for the live chain c1
plus 2,521 appended updates over 19 batches (receipts, checkpoints, trajectories, segment completions, a driver
`index()` append in its own process, an append cut before its newline, two supervisor restarts); the ended chain
c2 holds the first 300,000 rows of the same index (so the old side stays under the cap). Stubbed: no trainer, no
w5box instance, `Popen` mocked, `time.time` held at whole seconds plus a half; wall-clock keys dropped. Compared
per cycle: report, GUARD.json, SUPERVISE.json, both index hashes, LOOK.json, retention logs, LOG.jsonl and a
listing of every file with its size; then the pair code's reads (valid records, ledger series, projection, decide
twice, five first_at_or_past, two select, ledger_at, two PRUNE manifests, the replay sample's protected set and
selection).

| Tools | lines | differing |
|---|---|---|
| eb005f097 against 67a2f80a7 (node1's line) | 22 | 0 |
| e466fe21f against 8a13f4527 (main) | 22 | 0 |

The old sides peaked at 5.75 GB (node line) and 5.76 GB (main), under the cap. (Round 1's full-size run on this
PC reached 8.0 GB, past the 6 GB limit; these reruns were capped instead.)

## Memory and time

| (WSL, 1.35M records) | old eb005f097 | new 67a2f80a7 |
|---|---|---|
| after the first cycle | 2,751 MB | 130 MB |
| steady cycles | 2,751 to 4,774 MB | 132 to 217 MB |
| peak (cycles, pair reads) | 5,240 MB, 5,749 MB | 683 MB, 717 MB |
| fresh-start cycle | 50 to 69 s | 79 to 82 s |
| steady cycle | 1.7 s and up | 1.4 s and up |

The new fresh start is slower than the old (it parses the records it no longer keeps, in the first plan) and
far smaller; steady cycles are as fast. node1's dry cycle (above) is the production-shaped number: 841 MB peak,
708 MB steady for 9.34M records, 865 s to GUARD at 34% of one CPU.

Worst cases at 2M records (one chain, every checkpoint gone; WSL, `worst_case_timing.py`): whole read 13.7 s,
first plan 84.2 s, first guard 0.6 s, steady refresh and plan 0.01 s each, an unreached look scan 0.09 s,
protected paths 0.08 s, ledger_at 0.09 s, valid_records 0.27 s, ledger_series 0.34 s; finalization: select with a
far replay start 22.5 s, prune_manifest 38.9 s; peak RSS 665 MB. Extrapolated linearly to node1's 9.3M (6.2M in
the three live chains, which the first plan reads twice; the ended ones are read by the guard): about 64 s
reading, 260 s first plans, 60 s first guards, about 6.5 minutes on a free CPU, within the 900 s staleness and far
within the 39-minute driver tolerance; the dry cycle's 865 s at a third of a CPU is the measured bound. Finalization
does not occur on node1 (no pairs); on a pair plan it runs once per pair, before that cycle's GUARD write: about 3
minutes at 9.3M records.

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

Branch `claude/w5-supervise-index-memory-eb005f0`, commit 67a2f80a7 (parent eb005f097). sha256:

    chains.py         5d1ebd52018d2dbe9dab18f209ea410f4779214c0e13690121f7d560e3eaa651
    retention.py      5c351dbf37d36e0b241425b4882f34195eb8c50b0c5d3115d9a0d11eee24f5b7
    replay_sample.py  15ed4a801c696b64468d1e3d71b4e972950c3072591a26a598e46633fbe51b2a

### Procedure (as root, `sudo -i`; times Eastern). Steps marked RUN were run as written (on node1 read only, or in
the rehearsal); the others are for the lead.

0. State (expect: loop bash 1568722, python 1568730 RSS about 10.3 GB, GUARD.json younger than 70 s, the python
   path /usr/bin/python3.11).

        C=/srv/work/chains-20261107; P=$C/oc/plan/oc-node1/CHAINS-PLAN.json; PY=/usr/bin/python3.11
        ps -o pid,ppid,rss,etimes,args -p 1568722,1568730; free -g; readlink /proc/1568730/exe
        $PY -c "import json,time; g=json.load(open('$C/oc/GUARD.json')); print(round(time.time()-g['unix']), g['ok'], g['used_bytes'], g['ceilings'])"

1. RUN (rehearsal) Stage the tree. From Jack's PC:

        git -C D:/w5-supervise-mem-20261120-eb005f0 archive 67a2f80a7 tools/w5/chains.py tools/w5/retention.py tools/w5/replay_sample.py | ssh mtg@10.0.0.36 'mkdir /tmp/mem-67a2f80a7 && tar -x -C /tmp/mem-67a2f80a7'

   On node1:

        N=$C/tools-mem-67a2f80a7
        mkdir $N && cp -a $C/tools-src/tools $N/tools && find $N/tools -name __pycache__ -prune -exec rm -r {} +
        install -m 644 /tmp/mem-67a2f80a7/tools/w5/{chains,retention,replay_sample}.py $N/tools/w5/
        sha256sum $N/tools/w5/{chains,retention,replay_sample}.py      # the three pins above
        diff -rq -x __pycache__ $C/tools-src/tools $N/tools             # exactly those three files

2. RUN (rehearsal) The supervisor's plan copy (prints the three changed keys, then `ok`):

        P2=$C/oc/plan/oc-node1/CHAINS-PLAN.supervise-67a2f80a7.json
        $PY -B - "$P" "$P2" "$N/tools/w5" <<'EOF'
        import json, sys
        sys.path.insert(0, sys.argv[3]); import chains
        plan = json.load(open(sys.argv[1]))
        new = chains.tools_shas()
        changed = sorted(k for k in new if plan['tools'].get(k) != new[k])
        assert changed == ['chains.py', 'replay_sample.py', 'retention.py'] and set(new) == set(plan['tools']), changed
        print(changed)
        plan['tools'] = new
        with open(sys.argv[2], 'x') as f:
            f.write(json.dumps(plan, indent=1) + '\n')
        chains.load_plan(sys.argv[2])
        print('ok')
        EOF

2a. RUN (node1, read only, results above) The dry cycle, as mtg, before the swap:

        python supervise_dry_cycle.py bundle <eb005f0 worktree>/tools/w5 > dry.py        # on Jack's PC
        ssh mtg@10.0.0.36 'cd /tmp && taskset -c 1,17 nice -n 19 bash -c "ulimit -v 4194304; exec /usr/bin/python3.11 -B - --plan /srv/work/chains-20261107/oc/plan/oc-node1/CHAINS-PLAN.json --tools-src /srv/work/chains-20261107/tools-src/tools/w5 --cycles 2"' < dry.py

   Go on only if it exits 0 with `refused` empty and `guard.ok` true. Rerun it if the code changes.

3. RUN (rehearsal) Swap between cycles. The old loop must be gone before the new one starts (a surviving loop would
   restart the old code, which then holds SUPERVISE.lock):

        SUP=$C/oc/plan/oc-node1/SUPERVISE.json; t0=$(stat -c %Y $SUP)
        while [ "$(stat -c %Y $SUP)" = "$t0" ]; do sleep 1; done; sleep 2   # a cycle just ended
        kill 1568722; kill 1568730
        for p in 1568722 1568730; do while kill -0 $p 2>/dev/null; do sleep 1; done; done
        ps -o pid,args -p 1568722,1568730 | tail -n +2 | wc -l              # 0, else stop here
        pgrep -af "chains.py supervise --plan $P" | grep -v pgrep           # nothing
        date -Is
        setsid nohup bash -c "while true; do $PY $N/tools/w5/chains.py supervise --plan $P2 --loop 60 >> $C/records/supervise.log 2>&1; rc=\$?; echo \"\$(date -Is) supervise exited \$rc\" >> $C/records/supervise-restarts.log; sleep 30; done" > /dev/null 2>&1 < /dev/null &
        sleep 5; pgrep -af "supervise --plan $P2" | grep -v pgrep          # one bash loop, one python

   (`rc=$?` first: the current loop's `echo "$(date -Is) ... $?"` always logs 0, because `$(date)` runs before
   `$?` is expanded; the rehearsal shows it.)

4. Check at 3 minutes: `ps -o pid,rss,pcpu,etimes,args -p $(pgrep -d, -f "supervise --plan $P2")` (busy, RSS under
   1 GB), `tail -c 4000 $C/records/supervise.log | grep -c Traceback` (0), no new line in
   `supervise-restarts.log`, the three drivers 3867287..9 alive.

5. Check at 10 and 20 minutes: a report line after the swap with `"ok": true`, `used_bytes` within a few GB of
   step 0, `recovered`, `kills`, `oom_failures` empty; GUARD.json younger than 70 s with ceilings for c1..c3; RSS
   steady (0.7 to 0.9 GB) across two readings; index files growing. The first report may take up to about 15
   minutes (dry cycle bound); escalate to the recovery below if none by 25 minutes.

6. Record: copy `resume-node1.sh` to `resume-node1-mem.sh` with line 9 using `$PY`, `$N`, `$P2`, `rc=$?` and the
   pgrep check on `$P2`; the chain `run` lines keep `$T` and `$P`.

### Forward recovery: the new supervisor raises (RUN in the rehearsal)

If the new loop logs a Traceback on its first cycles (restart lines every 30 s, GUARD.json ageing), stop it as in
step 3 (its own pids) and start the same tree in guard-only mode:

        setsid nohup bash -c "while true; do $PY $N/tools/w5/chains.py supervise --plan $P2 --loop 60 --guard-only >> $C/records/supervise.log 2>&1; rc=\$?; echo \"\$(date -Is) supervise exited \$rc\" >> $C/records/supervise-restarts.log; sleep 30; done" > /dev/null 2>&1 < /dev/null &

`--guard-only` does what keeps chains admitted: closes dead w5box supervisors' instances and records OOM ends,
refreshes every index (appending rows), records looks, kills ended chains, releases ended chains' boxes, and
publishes GUARD.json and SUPERVISE.json (`"guard_only": true`). It does not make pair decisions, cut snapshots,
select endpoints, stage the replay sample, run rolling retention or purge the hold folder (nothing is deleted),
or start fillers. Used bytes then only grow; the guard refuses admission at the cap. It does not help if the
fault is in reading the indexes or in the guard itself; then stop the loop: the chains keep running for about 9
minutes, wait up to 30 more, and exit cleanly for a later `resume-node1.sh`.

Going back to the old code is not safe on node1: its fresh start parses every record (about 16 GB for 9.3M).

### Rehearsal transcript (WSL Ubuntu, Python 3.10.12, `tests/supervise_swap_rehearsal.py`, scaled: loop 5 s, pause 3 s)

| t (s) | step | result |
|---|---|---|
| 1.7 | start loop (old) | loop pid 13582, supervisor [13584], /usr/bin/python3 |
| 8.7 | driver started | pid 14667 (old tree) |
| 34.7 | check: old supervisor running | guard age 3.8 s, ok True; driver segments 0 -> 13, index rows 0 -> 42; tracebacks so far 0; restarts 0 |
| 34.8 | stage tree | 5d1ebd52018d chains.py; 5c351dbf37d3 retention.py; 15ed4a801c69 replay_sample.py; diff lists 3 files |
| 35.0 | plan copy | ['chains.py', 'replay_sample.py', 'retention.py'] ok, exit 0 |
| 35.3 | dry cycle | exit 0, refused [], guard ok True, writes recorded ['GUARD.json', 'SUPERVISE.json'] |
| 36.7 | stop loop (old) | killed [13582, 13584], still alive [] |
| 36.7 | gap begins | guard age 0.7 s, driver alive True |
| 37.7 | start loop (new) | loop pid 18526, supervisor [18527], /usr/bin/python3 |
| 53.8 | check: new supervisor, 3-minute check (scaled) | guard age 1.7 s, ok True; driver segments 14 -> 22, index rows 54 -> 82; tracebacks so far 0; restarts 0 |
| 83.8 | check: new supervisor, 10-minute check (scaled) | guard age 1.5 s, ok True; driver segments 22 -> 36, index rows 82 -> 140; tracebacks so far 0; restarts 0 |
| 88.0 | stop loop (new) | killed [18526, 18527], still alive [] |
| 88.2 | plan copy | ['chains.py', 'replay_sample.py', 'retention.py'] ok, exit 0 |
| 89.2 | start loop (faulty) | loop pid 25117, supervisor [25126], /usr/bin/python3 |
| 109.2 | check: faulty supervisor (raises every cycle) | guard age 21.8 s, ok True; driver segments 39 -> 42, index rows 152 -> 168; tracebacks so far 7; restarts 3 |
| 110.2 | stop faulty loop | alive [] |
| 111.3 | start loop (guard-only) | loop pid 27696, supervisor [27697], /usr/bin/python3 --guard-only |
| 137.3 | check: guard-only recovery on the faulty tree | guard age 1.6 s, ok True; driver segments 42 -> 48, index rows 168 -> 185; tracebacks so far 7; restarts 3 |
| 137.4 | guard-only deletions | deleted 0, added 111 |
| 141.4 | stop loop (guard-only) | killed [27696, 27697], still alive [] |
| 142.4 | start loop (new again) | loop pid 31408, supervisor [31409], /usr/bin/python3 |
| 162.4 | check: back on the new tree | guard age 0.7 s, ok True; driver segments 50 -> 60, index rows 196 -> 235; tracebacks so far 7; restarts 3 |
| 167.4 | stop loop (new again) | killed [31408, 31409], still alive [] |
| 167.4 | done | driver segments 62 |

The old and new trees were node1's (eb005f097 and 67a2f80a7's three files); one live chain (stand-in trainer, 4-update segments) and one never-started chain. The faulty tree is the new tree with `retention.plan` made to raise. Each of its exits was logged as `supervise exited 0` (the `$?` problem in step 3). Full transcript: `N:/w5-supervise-mem-20261120/swap-rehearsal.transcript.jsonl`.

### Not verified

- Root's `readlink /proc/1568730/exe` (not readable as mtg; step 0 has the lead check it).
- Timing at normal priority on node1 (only nice 19 on two busy CPUs was measured: an upper bound).
- The rehearsal ran on WSL with a stand-in trainer, no w5box instance and no cgroups; node1's drivers run under
  w5box. The swap does not touch w5box, but its interaction was not rehearsed.
- The other node1 supervisors (forks2, wave2, wave2-cont, batch-ladder) keep the old cache and grow with their
  chains; the main-line commit covers them when they move.
