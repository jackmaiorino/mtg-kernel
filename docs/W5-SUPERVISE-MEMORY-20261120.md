# W5 supervise index memory (2026-11-20, review round 5)

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
- Settlement where it always was, and by boundary as before: the first plan and guard see every record, and a
  boundary they find with every file gone leaves `active` for good (with two records at one boundary, both leave,
  as in the old cache; node1's indexes have no duplicate boundary). The plan is streamed
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
  append gives no duplicate and no gap; its cursor is the end of what it read.

## Review round 4 findings (GPT-6 Astra, head e86223129, DEPLOY WITH CHANGES) and their disposition

Round 5 changed only `supervise_swap.py`, `supervise_swap.sh`, their tests, the rehearsal and this record (node line
commit 65115bb99, main a3ff9cb57):
chains.py (c2a2fbea...), retention.py (b2d6c17a...) and replay_sample.py (15ed4a80...) keep their round 4 hashes,
so the node1 dry cycles and the old-against-new equivalence stand.

| # | Finding | Fixed | Test |
|---|---|---|---|
| 1 | the recovery watch escalated again at once | `watch_swap GID recovery`: no escalation; SUCCESS or DEADLINE | `test_recovery_mode_does_not_escalate`; rehearsal (swap mode ESCALATE, guard-only, recovery mode SUCCESS) |
| 2 | waits opened before T0 were dropped | open waits rebuilt from the whole log (closed by segment, run-exit, stop, end, no-room, run); unknown printed as unknown | `test_unknowns_and_waits_that_began_before_the_stop` |
| 3 | SUCCESS without usable admission | new GUARD id, ok, age under 900 s, a free admission for every live chain, no Traceback | `test_success_means_admission_works`, `test_a_traceback_blocks_success_and_is_shown` |
| 4 | pid checked, then a bare kill; partial stop | signals only through a pidfd, the argv checked after it opens (no pidfd: refused); `stop_between_cycles` ends with STOP REFUSED or STOP INCOMPLETE, and the procedure says what follows each | `test_signals_go_through_a_pidfd_checked_after_it_opens`, `test_no_pidfd_means_no_signal`, `StepFileTests` (Linux) |
| 5 (lead) | a failed count on the last try fell through to the kill | explicit success flag; signals only the pids just found asleep | `test_never_asleep_means_never_signalled`, `test_two_supervisors_refuse_and_nothing_is_signalled` |
| 6 | `check_tails` unlocked | each tail read under its chain's index lock | `test_each_tail_is_read_under_its_index_lock` |
| 7 | step 0 used `$S` before staging | `pids $P` moved into step 1 after staging | |
| 8 | the watch could crash on a half-written file | tolerant reads, unknown printed, retried next round | `test_unreadable_files_are_unknown_not_a_crash` |
| 9 | plan aliases | the doc states the exact spelling | |

Second-family round 4 review (Gemini 4 Carbon, DEPLOY WITH CHANGES), folded in: a supervisor exit logged in
supervise-restarts.log is a failure signal like a Traceback, and the watch says which it saw
(`test_a_supervisor_exit_without_a_traceback_is_a_failure`); a stop decides "nothing left" from fresh scans of the
plan, with one SIGKILL round and one more verified round (`test_a_supervisor_that_ignores_sigterm_is_killed_and_the_rescan_decides`);
waits close on the events chains.py actually logs; a last line is blank only if the index reader skips it (line
break characters, not spaces: `test_a_last_line_of_spaces_is_torn_as_the_reader_sees_it`); the whole `cut_tail`
inspection runs under the lock and refuses a terminated unparseable line there; T0 is a float from one clock read,
recorded with the GUARD id (`test_after_stop_records_t0_and_the_guard_id`). The script supports the two loop shapes
listed (the step file's and node1's start-script loop) and plan paths spelled exactly as in the running argv.

## Review round 3 findings (GPT-6 Astra, head 95343bfd6) and their disposition

The cache code was reported clean; every round 3 finding was in the swap steps.

| # | Finding | Fixed | Test or evidence |
|---|---|---|---|
| 1 | recovery started without a tail check; the cut read the file whole and took no lock | `check_tails` after every stop and inside `start_loop` (it refuses a torn tail); a whole line without its newline is OK; `cut_partial_tail` runs under the chain's `chain.index.lock` through the truncate, rechecks under it, reads backward in 64 KiB blocks and saves the cut bytes to records/ | `TailTests` in `test_w5_supervise_swap.py`: `test_a_torn_tail_on_a_file_larger_than_the_block`, `test_a_whole_line_without_its_newline_is_ok_and_not_cut`, `test_a_writer_that_finishes_its_line_before_the_lock`, `test_the_cut_holds_the_index_lock`, `test_a_terminated_unparseable_line_is_not_cut`; rehearsal |
| 2 | process selection by a regex; paths spliced into the loop text | exact argv from /proc (`supervise_swap.py` `select`): a python running `chains.py supervise --plan PLAN`, its parent only if that is a loop for exactly PLAN; `start_loop` passes paths as arguments; `$PY` quoted | `ProcessIdentityTests` (`test_exactly_the_plan_and_its_loops`, `test_a_new_loop_with_guard_only_and_odd_paths`, `test_real_processes` on Linux: two plans one character apart and a bystander bash); rehearsal bystander survived |
| 3 | the 1286 s deadline assumed remaining admission time | replaced by the rule above, read off the host by `watch_swap` | `WatchTests` (`test_wait_escalate_and_success`, `test_a_traceback_after_the_start_escalates`); rehearsal |
| 4 | guard-only checked only at start; `independent` alone | checked in `cycle` on every guard-only cycle; refuses pairs, a plan not independent, and any filler (F) chain | `GuardOnlyTests.test_every_guard_only_cycle_checks_the_plan` (direct calls and a reloaded pair plan) |
| 5 | read-only cursor taken from a later stat | the cursor is the end of what the stream read | `ReadOnlyCursorTests.test_a_row_appended_while_the_receipts_are_scanned` ([1,2,3]) |

Second-family round 3 review (Gemini 4 Carbon): the old loop must never survive (`stop_between_cycles` needs exactly
one loop and one supervisor, re-reads them on every try and re-verifies each pid's argv just before its signal;
`stop_failing` refuses when nothing runs; `start_loop` refuses a plan that already runs; lists are trimmed;
`test_node1_shapes`, `test_a_recycled_pid_is_never_signalled`); `asleep` reads the state again after 2 s, requires
tick readings and bounds the report age by the loop period (the step file refuses a loop under 10 s; the rehearsal
runs at 12 s); the step files are installed root-owned into the new tree and run from there; a `pids` check of the
real plan and the dry cycle on the staged hashes come before the stop; `terminate` flushes before its size check,
an unwritten record is returned as a copy and `check` refuses a bad record number
(`test_records_read_fresh_and_bad_numbers_refused`). Settlement stays by boundary, as in the old cache.

The node1 dry cycles were not repeated: round 4 changed only the read-only cache's cursor (the dry cycle's own
cache; an end of read instead of a later stat) and the guard-only check at cycle entry, neither of which changes
how long a cycle reads or computes.

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
last line. If it parses, both `index()` and the cache read it and the next append adds its newline (`terminate`):
`check_tails` reports it OK and `cut_partial_tail` leaves it. If it does not parse, both raise on every refresh, so
neither a full nor a guard-only supervisor can start; `start_loop` refuses to start while any index has such a
line. `cut_partial_tail` takes the chain's own index lock (`chain.index.lock`, the lock `retention.index_lock`
takes), holds it through the truncate, looks again under it (a writer that finished its line meanwhile leaves
nothing to cut), reads backward in 64 KiB blocks (never the whole file), saves the cut bytes to
`records/torn-tail-<chain>-<time>.bin`, and truncates to the end of the previous line. A last line that ends in a
newline and does not parse is not a torn append: it refuses. Cutting is safe because every index row is written
from that update's durable receipt (`scan` reads `iterations/NNNNNN/complete.json` and its `update.json`; the row is
only appended after both exist), so the next refresh finds the cut record's receipt again and writes the same
line. `test_cutting_a_torn_last_line_back_is_safe` (cache), the `TailTests` (the cut itself) and the rehearsal
check it.

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
- Observations, not bounds: segment start intervals over the last 500 segments of c1..c3 were 127.3 s at the
  least, 133 to 135 s in the median, 147.9 s at the most; the old supervisor's census precedes its GUARD write by
  its cycle time (median 4.3 s, but up to 300 s in a byte-counter recount cycle). A driver that has used its 4
  admissions waits; it exits 1800 s after its own `guard-wait` event, at the earliest about 509 s plus 1800 s after
  an ordinary census at the observed segment rate. A new verdict at any time before that admits it at its next
  30 s poll (`chains.py` `guard_admit` and the driver loop).
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

Expected timeline of the swap (dry-cycle speeds, observations): the new loop starts within about 10 s of the stop;
its first GUARD about 900 s after the stop. Drivers reach their ceilings from about 509 s after the last census and
wait (each live chain idles about 6 to 7 minutes).

The deadline rule (read off the host by `supervise_swap.sh watch_swap`, which prints every 30 s):
- before the stop: each live chain's admitted count against its GUARD ceiling (how many more segments it may start);
- success: a GUARD.json whose `id` differs from the id recorded right after the stop (`after_stop`, which also
  records T0 as a float from the same clock read; ids are nanosecond times, so the old supervisor's last
  publication can never pass), `ok` true, younger than `GUARD_MAX_AGE` (900 s), a ceiling above the admitted count
  for every live chain, and no failure since the watch began: a Traceback in supervise.log, or a line in
  supervise-restarts.log (the loop logs every supervisor exit, a refusal that prints `stop:` or `refused:` and no
  Traceback included); a failure is printed with any verdict, naming which it was;
- a chain's open wait: its latest `guard-wait` in LOG.jsonl not followed by its `segment`, `no-room`, `stop` or
  `end` (logged by its driver, chains.py `drive_locked`) or `run-exit` or `run` (logged by `run`), whenever it began (before the stop included: a driver logs it once); a chain with no such
  event, or a LOG.jsonl that cannot be read, is printed as unknown, never as "not waiting";
- the hard exit of a waiting chain: its wait start plus 1800 s; swap mode escalates (exit 2) at the earliest open
  wait plus 1800 s minus the measured guard-only start (723 s) minus 300 s, or at once on a failure; recovery mode
  (after guard-only is started) never escalates and ends with SUCCESS or DEADLINE (the earliest hard exit passed).
- GUARD.json, ADMITTED.json and LOG.jsonl are read tolerantly: a file that does not parse is printed as unknown and
  read again on the next round; a LOG.jsonl line still being written is read once it is complete.

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
| main fd1307457 (round 4) | supervise_swap and the modules above | 204 | 0 | 30 |
| node line e86223129 (round 4) | supervise_swap and that commit's modules above | 124 | 0 | 15 |
| node line e86223129, WSL Ubuntu, Python 3.10.12 | supervise_swap (with the real-process test) | 14 | 0 | 0 |
| round 5 (a3ff9cb57 and 65115bb99, identical test files) | supervise_swap, this PC | 26 | 0 | 5 (Linux only) |
| round 5, node line 65115bb99, WSL Ubuntu, Python 3.10.12 | supervise_swap (real processes, pidfd, the step file) | 26 | 0 | 0 |

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

The old-against-new runs above are of round 3 (ed34735d1, 95343bfd6). Round 4 did not rerun them: its module
changes are the read-only cache's cursor (the dry cycle's cache only), the guard-only check at cycle entry, a
flush before `terminate`'s size check, a copy of an unwritten record and a bound check in `IndexRecords.check`,
none of which changes what a full cycle reads, decides or writes.

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

Branch `claude/w5-supervise-index-memory-eb005f0`, commit 65115bb99 (parent eb005f097). sha256:

    chains.py          c2a2fbeab90a51c50f04d0040e3258af96eda80a485c5d507e8aad3821cde3d6   (round 4, unchanged)
    retention.py       b2d6c17ad91e792645006fe31a3f3f3eb56b87eaa08793cc86e95bff553566cb   (round 4, unchanged)
    replay_sample.py   15ed4a801c696b64468d1e3d71b4e972950c3072591a26a598e46633fbe51b2a   (unchanged)
    supervise_swap.sh  30ab6974ee5cb661aa7aad7ac1adda135bb4b101608539475e076ddde743cc6a   (step file; not a pinned tool)
    supervise_swap.py  47b263da760b25dcfaf46d7d801244bc0b6fae7b039c8efeb31326910d1de59a   (step helpers; not pinned)

### Procedure (as root, `sudo -i`; times Eastern)

The steps are functions of one file, `tools/w5/supervise_swap.sh` (its helpers in `supervise_swap.py` beside it),
which the rehearsal runs as written. Both are staged into the new tree (step 1); neither is a pinned tool. A plan
path given to a step must be spelled exactly as in the running process's argv (no relative form, no symlink
alias): node1's supervisor runs `--plan /srv/work/chains-20261107/oc/plan/oc-node1/CHAINS-PLAN.json`.
Environment for every step:

        C=/srv/work/chains-20261107; ROOT=$C/oc; PY=/usr/bin/python3.11; export C ROOT PY
        P=$ROOT/plan/oc-node1/CHAINS-PLAN.json; N=$C/tools-mem-65115bb99; S=$N/tools/w5/supervise_swap.sh
        P2=$ROOT/plan/oc-node1/CHAINS-PLAN.supervise-65115bb99.json

0. State: `ps -o pid,ppid,rss,etimes,args -p 1568722,1568730` (loop bash and python, RSS about 10.3 GB),
   `readlink /proc/1568730/exe` (/usr/bin/python3.11), `free -g`.

1. Stage the tree. From Jack's PC:

        git -C D:/w5-supervise-mem-20261120-eb005f0 archive 65115bb99 tools/w5/chains.py tools/w5/retention.py tools/w5/replay_sample.py tools/w5/supervise_swap.sh tools/w5/supervise_swap.py | ssh mtg@10.0.0.36 'mkdir /tmp/mem-65115bb99 && tar -x -C /tmp/mem-65115bb99'

   On node1:

        mkdir $N && cp -a $C/tools-src/tools $N/tools && find $N/tools -name __pycache__ -prune -exec rm -r {} +
        install -o root -g root -m 644 /tmp/mem-65115bb99/tools/w5/{chains,retention,replay_sample,supervise_swap}.py $N/tools/w5/
        install -o root -g root -m 755 /tmp/mem-65115bb99/tools/w5/supervise_swap.sh $N/tools/w5/
        sha256sum $N/tools/w5/{chains,retention,replay_sample}.py $N/tools/w5/supervise_swap.{sh,py}   # the pins
        diff -rq -x __pycache__ $C/tools-src/tools $N/tools   # the three changed files and the two step files only
        bash $S pids $P                          # exactly: LOOP_PIDS 1568722 ; PYTHON_PIDS 1568730

   Every later step, and the replacement resume script, runs these root-owned copies (`$S`), never the ones under
   /tmp.

2. The plan copy (prints the three changed keys, then `ok`; it checks every pin of the copy):

        bash $S plan_copy $P $P2 $N/tools/w5

2a. The dry cycle (run on node1 as mtg, read only; results above). It must have been run on modules whose three
    hashes equal the staged tree's (step 1); rerun it if they differ:

        python supervise_dry_cycle.py bundle <eb005f0 worktree>/tools/w5 > dry.py        # on Jack's PC
        ssh mtg@10.0.0.36 'cd /tmp && taskset -c 1,17 bash -c "ulimit -v 4194304; exec /usr/bin/python3.11 -B - --plan /srv/work/chains-20261107/oc/plan/oc-node1/CHAINS-PLAN.json --tools-src /srv/work/chains-20261107/tools-src/tools/w5 --cycles 3 --guard-only-cycles 2"' < dry.py

   Go on only if it exits 0 with `refused` empty and every cycle's `guard.ok` true.

3. Swap:

        bash $S pids $P                          # exactly: LOOP_PIDS 1568722 ; PYTHON_PIDS 1568730; else stop here
        bash $S watch_swap $(bash $S guard_id) swap 30 1   # one line: admitted, ceiling, open wait per live chain
        bash $S stop_between_cycles $P           # needs exactly one loop and one supervisor (read again each try);
                                                 # waits for a cycle's end; asleep (state S before and after 2 s, the
                                                 # same CPU ticks, report 0 to 55 s old) on the very pids it then
                                                 # signals, each through a pidfd with its argv checked after the pidfd
                                                 # opens; "nothing left" is a fresh scan of $P (what is still found
                                                 # after 20 s gets SIGKILL, then one more round)
        bash $S after_stop                       # T0=<float> GID=<id>: one clock read and the GUARD id after the
                                                 # stop; record both (success needs a GUARD id other than GID)
        GID=<the GID it printed>
        bash $S check_tails                      # every index's last line parses (each read under its index lock)
        # only for an index check_tails names `torn`:  bash $S cut_partial_tail <that index>; bash $S check_tails
        bash $S start_loop $N/tools/w5 $P2       # refuses if $P2 already runs or a tail is torn; prints LOOP_PID
        bash $S watch_swap $GID swap             # every 30 s until SUCCESS (exit 0) or ESCALATE (exit 2)

   If `stop_between_cycles` fails, its last line says which case it is. `STOP REFUSED: nothing signalled, nothing
   changed`: the old supervisor still runs; investigate, and retry the step or abandon the swap. `STOP INCOMPLETE:
   signals were sent`: do exactly

        bash $S stop_failing $P                  # rescans $P, finishes the stop (pidfd, argv checked), verifies
        bash $S after_stop                       # record T0 and GID as above
        bash $S check_tails                      # cut_partial_tail a torn index, then check_tails again
        bash $S start_loop $N/tools/w5 $P2
        bash $S watch_swap $GID swap

   The loop logs each exit with its real code (`rc=$?` first; the current loop's `echo "$(date -Is) ... $?"`
   always logs 0, because `$(date)` runs before `$?` is expanded).

4. While `watch_swap` waits: `bash $S pids $P2` (one loop, one python), RSS under 1 GB, the three drivers
   3867287..9 alive.

5. On SUCCESS: a report line in supervise.log with `"ok": true`, `used_bytes` within a few GB of step 0,
   `recovered`, `kills`, `oom_failures` empty; RSS steady (0.7 to 0.9 GB) across two readings; index files growing.
   On ESCALATE (a Traceback, or the escalation time of the deadline rule reached): the recovery below.

6. Record: copy `resume-node1.sh` to `resume-node1-mem.sh`, its supervise line replaced by
   `bash $S start_loop $N/tools/w5 $P2` (with `$S` the root-owned copy in the new tree, which refuses a plan that
   already runs) and its pgrep check removed; the chain `run` lines keep the old tree and `$P`. Do not run the old
   `resume-node1.sh` after the swap.

### Forward recovery: the new supervisor fails before publishing (rehearsed)

        bash $S stop_failing $P2                 # bounded: loop, then supervisor, SIGKILL after 20 s, verified
        bash $S check_tails                      # always, after any stop: a killed supervisor may have torn a line
        # only for an index check_tails names `torn`:  bash $S cut_partial_tail <that index>; bash $S check_tails
        bash $S start_loop $N/tools/w5 $P2 --guard-only   # refuses while a tail is torn
        bash $S watch_swap $GID recovery         # no escalation: SUCCESS (0), or DEADLINE (3) once the earliest open
                                                 # guard wait + 1800 s has passed (a driver has exited)

The recovery watch keeps `$GID` (the id recorded after the stop), so guard-only's first GUARD counts as success,
and it never escalates again: following the recovery text does not stop guard-only before it publishes. On
DEADLINE the exited chains are restarted with `chains.py run --plan $P --chain cN` once a supervisor publishes.

`--guard-only` (refused, at every cycle, for a plan with pairs, a plan that is not independent, or any filler
chain) refreshes every index (appending rows), records looks, and publishes GUARD.json and SUPERVISE.json
(`"guard_only": true`). It still runs the cycle's safety steps, which act on processes and files: `w5box.py recover`
for a chain whose w5box supervisor died (it stops a running instance), OOM ends (END.json, and for a box with no
driver left the unmount and removal of its unpersisted scratch segment), kill requests for ended chains, and the
release of ended chains' boxes. It makes no pair decision, snapshot or selection, stages no replay sample, runs no
rolling retention, purges no hold folder and starts no filler. Used bytes only grow without retention; the guard
refuses admission at the cap. On node1 the headroom is about 224 GB (cap 560 GB, used 294 GB, reserved 42 GB) and
the old supervisor's retention removed 26.6 GB in the last 24 hours (1.1 GB an hour): roughly 8 days of
guard-only before the cap. It does not help a fault in reading the indexes or in the guard; then `stop_failing`,
`check_tails`, and leave the chains to exit cleanly (1800 s after each one's `guard-wait`), to restart them with
`chains.py run` once a supervisor runs.

Going back to the old code is not safe on node1: its fresh start parses every record (about 16 GB for 9.3M).

### Rehearsal (WSL Ubuntu, Python 3.10.12, `tests/supervise_swap_rehearsal.py`, scaled: loop 12 s, pause 3 s)

| t (s) | step | result |
|---|---|---|
| 2.8 | `start_loop` | exit 0: started: loop 30455 LOOP_PIDS 30455 ; PYTHON_PIDS 30612 / LOOP_PID 30455 |
| 16.9 | driver started | pid 459 (old tree) |
| 41.9 | check: old supervisor running | GUARD age 4.9 s; segments 0 -> 8, rows contiguous True; tracebacks 0 |
| 42.1 | `plan_copy` | exit 0: ['chains.py', 'replay_sample.py', 'retention.py'] / ok |
| 42.4 | dry cycle | exit 0, refused [], modes ['full', 'full', 'guard-only'] |
| 42.7 | `watch_swap` | exit 1: WAIT; failure []; first driver exit in 1790.8 s; escalate in 767.8 |
| 54.1 | `stop_between_cycles` | exit 0: signal 15: SIGNALLED 30612 ; REFUSED / stopped: /tmp/w5-swap-rehearsal/chains-rehearsal/oc/plan/oc-rehearsal/CHAINS-PLAN.json (run check_tails before any start) |
| 54.2 | `after_stop` | exit 0: T0=1795213300.191469 GID=1795213295004416982 |
| 54.2 | gap begins | GUARD age 5.2 s, driver alive True |
| 54.3 | `check_tails` | exit 1: torn /tmp/w5-swap-rehearsal/chains-rehearsal/oc/chains/c1/chain.index.jsonl |
| 54.3 | torn tail injected | check_tails exit 1 |
| 54.4 | `start_loop` | exit 1: torn /tmp/w5-swap-rehearsal/chains-rehearsal/oc/chains/c1/chain.index.jsonl / an index's last line does not parse: cut_partial_tail it first |
| 54.5 | `pids` | exit 0: LOOP_PIDS ; PYTHON_PIDS |
| 54.5 | start_loop refuses a torn tail | exit 1, LOOP_PIDS ; PYTHON_PIDS |
| 54.6 | `cut_partial_tail` | exit 0: cut /tmp/w5-swap-rehearsal/chains-rehearsal/oc/chains/c1/chain.index.jsonl from 14931 to 14874 bytes; the 57 cut bytes are in /tmp/w5-swap-rehearsal/chains-rehearsal/records/torn-tail-c1-20261120T172140.bin |
| 54.7 | `check_tails` | exit 0: ok /tmp/w5-swap-rehearsal/chains-rehearsal/oc/chains/c1/chain.index.jsonl |
| 54.7 | tail cut back | same bytes as before the tear True, cut bytes saved [True] |
| 57.0 | `start_loop` | exit 0: started: loop 7556 LOOP_PIDS 7556 ; PYTHON_PIDS 7558 / LOOP_PID 7556 |
| 57.0 | watch_swap during the swap | exit 0, SUCCESS; failure []; first driver exit in 1777.6 s; escalate in 754.6 |
| 72.0 | check: new supervisor, 3-minute check (scaled) | GUARD age 4.9 s; segments 8 -> 13, rows contiguous True; tracebacks 0 |
| 102.0 | check: new supervisor, 10-minute check (scaled) | GUARD age 10.8 s; segments 13 -> 14, rows contiguous True; tracebacks 0 |
| 108.4 | `stop_between_cycles` | exit 0: signal 15: SIGNALLED 7558 ; REFUSED / stopped: /tmp/w5-swap-rehearsal/chains-rehearsal/oc/plan/oc-rehearsal/CHAINS-PLAN.supervise-new.json (run check_tails before any start) |
| 108.6 | `plan_copy` | exit 0: ['chains.py', 'replay_sample.py', 'retention.py'] / ok |
| 108.6 | `after_stop` | exit 0: T0=1795213354.665660 GID=1795213349208818874 |
| 110.9 | `start_loop` | exit 0: started: loop 17488 LOOP_PIDS 17488 ; PYTHON_PIDS 17490 / LOOP_PID 17488 |
| 130.9 | check: faulty supervisor (raises before every GUARD write) | GUARD age 27.7 s; segments 17 -> 18, rows contiguous True; tracebacks 2 |
| 130.9 | faulty loop never published | GUARD unchanged True |
| 130.9 | watch_swap on the faulty loop | exit 2, ESCALATE; failure ['Traceback in supervise.log', "supervisor exits: ['2026-11-20T17:22:41-05:00 supervise exited 1']"]; first driver exit in 1798.4 s; escalate in 775.4 |
| 131.1 | `stop_failing` | exit 0: signal 15: SIGNALLED 20935 ; REFUSED / stopped: /tmp/w5-swap-rehearsal/chains-rehearsal/oc/plan/oc-rehearsal/CHAINS-PLAN.supervise-faulty.json (run check_tails before any start) |
| 131.2 | `check_tails` | exit 0: ok /tmp/w5-swap-rehearsal/chains-rehearsal/oc/chains/c1/chain.index.jsonl |
| 133.5 | `start_loop` | exit 0: started: loop 22347 LOOP_PIDS 22347 ; PYTHON_PIDS 22349 / LOOP_PID 22347 |
| 133.5 | watch_swap recovery mode after guard-only | exit 0, SUCCESS; failure []; first driver exit in 1781.8 s; escalate in None |
| 158.5 | check: guard-only recovery on the faulty tree | GUARD age 2.9 s; segments 18 -> 22, rows contiguous True; tracebacks 2 |
| 158.5 | guard-only deletions | deleted 0, added 88 |
| 172.9 | `stop_between_cycles` | exit 0: signal 15: SIGNALLED 22349 ; REFUSED / stopped: /tmp/w5-swap-rehearsal/chains-rehearsal/oc/plan/oc-rehearsal/CHAINS-PLAN.supervise-faulty.json (run check_tails before any start) |
| 175.1 | `start_loop` | exit 0: started: loop 30500 LOOP_PIDS 30500 ; PYTHON_PIDS 30502 / LOOP_PID 30500 |
| 195.2 | check: back on the new tree | GUARD age 9.9 s; segments 22 -> 28, rows contiguous True; tracebacks 2 |
| 202.8 | `stop_between_cycles` | exit 0: signal 15: SIGNALLED 30502 ; REFUSED / stopped: /tmp/w5-swap-rehearsal/chains-rehearsal/oc/plan/oc-rehearsal/CHAINS-PLAN.supervise-new.json (run check_tails before any start) |
| 202.8 | done | segments 28, index rows 112, contiguous True; bystander bash alive True |

The old tree is node1's (eb005f097), the new one 65115bb99's step file and modules, run as written; one live chain
(stand-in trainer, 4-update segments) and one never-started chain; a bystander bash whose command line holds
`supervise --plan <the plan>` ran throughout and was never signalled. Every signal went through a pidfd. The faulty
tree is the new tree with `retention.plan` made to sleep 6 s and raise, so it fails before every GUARD write: the
swap-mode watch reported ESCALATE and named both failure signals (the Traceback and the loop's `supervise exited
1` line); `stop_failing` stopped it, `check_tails` passed, guard-only was started, and the recovery-mode watch
(same GUARD id) reported SUCCESS on guard-only's first GUARD instead of escalating again. Full transcript:
`N:/w5-supervise-mem-20261120/swap-rehearsal-r5.transcript.jsonl`.

### Not verified

- Root's `readlink /proc/1568730/exe` (not readable as mtg; step 0).
- The dry cycle at normal priority shares CPUs 1 and 17 with pinned work; the real supervisor is not pinned.
- The rehearsal ran on WSL with a stand-in trainer, no w5box instance and no cgroups; node1's drivers run under
  w5box. The swap does not touch w5box, but its interaction was not rehearsed.
- The other node1 supervisors (forks2, wave2, wave2-cont, batch-ladder) keep the old cache and grow with their
  chains; the main-line commit covers them when they move.
