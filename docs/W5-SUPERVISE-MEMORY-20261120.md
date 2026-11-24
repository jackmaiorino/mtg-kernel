# W5 supervise index memory (2026-11-20, review round 6)

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

## Review round 5 findings (GPT-6 Astra, DEPLOY WITH CHANGES, 95; Gemini 4 Carbon, DEPLOY WITH CHANGES, 94) and their disposition

Round 6 changed only `supervise_swap.py`, `supervise_swap.sh`, their tests, the rehearsal and this record (node line
commits c33072661 and 9afddf945; the main branch carries the same four files). chains.py (c2a2fbea...),
retention.py (b2d6c17a...) and replay_sample.py (15ed4a80...) keep their round 4 hashes, so the node1 dry cycles
and the old-against-new equivalence stand.

| # | Finding | Fixed | Test |
|---|---|---|---|
| 1, Carbon a | `after_stop` failed when GUARD.json was missing or unreadable; `stop_failing` dead-ended on a plan whose processes had already gone | `after_stop` prints `GID=unknown` with the post-stop time, and the watch accepts it (SUCCESS then needs a GUARD written after T0, any id); `stop_failing` finding nothing prints `verified empty: nothing runs PLAN` and exits 0, so the sequence goes on with `after_stop`, `check_tails`, `start_loop`; it still exits 1 if anything is left after its stop | `test_an_unknown_guard_baseline_needs_a_guard_written_after_t0`, `test_start_loop_refuses_a_running_plan_and_stop_failing_finds_an_idle_one_verified_empty` |
| 2, Carbon c | an exited driver vanished from the watch (and could turn DEADLINE into SUCCESS or an endless WAIT) | a chain whose last driver event is `no-room`, `run-exit`, `stop`, `segment-failed`, `recover-unsupervised` or `w5box-kill` (the names chains.py logs; `w5box-kill` is `request`'s event) shows `exited, needs run (event at unix)` every round in both modes until a later `run` (or `segment`); with a usable GUARD the verdict is RESUME NEEDED (exit 4, the chains named), never plain SUCCESS; in recovery mode a failure while a chain is exited ends the watch with FAILURE, RESUME NEEDED (exit 5) | `test_an_exited_driver_needs_run_and_blocks_plain_success`, `test_a_wait_closed_by_recover_unsupervised_needs_run`, `test_segment_failed_and_w5box_kill_close_a_driver`, `test_recovery_ends_on_a_failure_while_a_driver_has_exited` |
| 3 | one baseline for several starts | `after_stop` runs after every completed stop and prints `T0=<float> GID=<id or unknown> LOG=<bytes> RESTARTS=<bytes>`; each `watch_swap PLAN "$B"` takes the baseline of the stop before its start and counts failures from those offsets (a failure inside `start_loop`, before the first watch round, counts); the rehearsal records a baseline after every stop and watches after every start | `test_a_failure_inside_start_loop_is_seen_through_the_baseline_offsets`, `test_a_full_mode_guard_before_the_failure_gives_recovery_no_instant_success`, `test_after_stop_records_t0_the_guard_id_and_the_log_offsets`; rehearsal |
| 4, Carbon b, e | procedure text did not execute as written (the INCOMPLETE branch had no GID line) | every branch assigns `B=$(bash $S after_stop)` and passes `"$B"` quoted; the replacement resume script (step 6) sets and exports C, ROOT, PY and sets T, P, N, S, P2 at its top, and is shown in full; an exited chain's full old-tree `run` command, in the resume script's shape, run only once a supervisor publishes | (procedure text, checked against the step file's argument order) |
| Carbon d | `live_chains` globbed every folder under chains/ | live chains are the plan's chains without END.json (`watch_swap` takes the plan); a leftover folder does not block SUCCESS | `test_live_chains_are_the_plans_chains_without_end` |
| small | SENT set before the helper ran; the first log read was one read | SENT is 1 only when the helper's `SIGNALLED` list is non-empty, `unknown` when its output cannot be read (STOP UNCERTAIN); LOG.jsonl is streamed line by line from the first read | `test_sent_is_what_the_helper_reports_it_signalled`, `test_the_first_read_streams_lines` |

Documented limits (items 8 and 9, unchanged): `check_tails` and `cut_partial_tail` split the last line on LF only,
so they serve LF-terminated indexes (what `index_locked` writes; node1's are LF only); a CR inside the last line is
not reported. `check_tails` takes each chain's index lock and so waits while a writer holds it (a hung lock holder
stalls the step; it prints nothing until it gets the lock). The rehearsal now also cleans up its processes on an
alarm or a TERM.

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

The deadline rule (read off the host by `supervise_swap.sh watch_swap PLAN "$B"`, which prints every 30 s):
- the baseline `$B`: `after_stop` runs after every completed stop and prints one line, `T0=<float> GID=<GUARD id or
  unknown> LOG=<supervise.log bytes> RESTARTS=<supervise-restarts.log bytes>`, from one clock read; each watch
  takes the baseline of the stop before its start;
- live chains: the plan's chains without END.json; for each, the admitted count against its GUARD ceiling (how
  many more segments it may start) and its driver state;
- success: a GUARD.json written after T0 whose `id` differs from GID (any id when GID is `unknown`, which
  `after_stop` prints when GUARD.json is missing or unreadable; ids are nanosecond times, so the old supervisor's
  last publication can never pass), `ok` true, younger than `GUARD_MAX_AGE` (900 s), a ceiling above the admitted
  count for every live chain, no failure after the baseline's offsets (a Traceback in supervise.log, or a line in
  supervise-restarts.log: the loop logs every supervisor exit, a refusal that prints `stop:` or `refused:` and no
  Traceback included; a failure inside `start_loop`, before the first round, counts), and every live chain running
  or waiting; a failure is printed with any verdict, naming which it was;
- a driver's state from its last driver event in LOG.jsonl, whenever it was logged (before the stop included):
  `guard-wait` waiting (a driver logs it once); `segment` or `run` running; `no-room`, `run-exit`, `stop`,
  `segment-failed`, `recover-unsupervised` or `w5box-kill` exited: printed `exited, needs run` every round, in both
  modes, until a later `run`; with a usable GUARD the verdict is then RESUME NEEDED (exit 4, the chains named),
  never plain SUCCESS; a chain with no such event, or a LOG.jsonl that cannot be read, is printed as unknown;
- the hard exit of a waiting chain: its wait start plus 1800 s; swap mode escalates (exit 2) at the earliest open
  wait plus 1800 s minus the measured guard-only start (723 s) minus 300 s, or at once on a failure; recovery mode
  (after guard-only is started) never escalates and ends with SUCCESS, RESUME NEEDED, DEADLINE (exit 3: the
  earliest hard exit passed) or FAILURE, RESUME NEEDED (exit 5: a failure while a chain's driver has exited).
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
| round 6 (main and node line 9afddf945, identical test files) | supervise_swap, this PC | 36 | 0 | 6 (Linux only) |
| round 6, both lines, WSL Ubuntu, Python 3.10.12 | supervise_swap (real processes, pidfd, the step file) | 36 | 0 | 0 |

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

Branch `claude/w5-supervise-index-memory-eb005f0`, commit 9afddf945 (parent eb005f097). sha256:

    chains.py          c2a2fbeab90a51c50f04d0040e3258af96eda80a485c5d507e8aad3821cde3d6   (round 4, unchanged)
    retention.py       b2d6c17ad91e792645006fe31a3f3f3eb56b87eaa08793cc86e95bff553566cb   (round 4, unchanged)
    replay_sample.py   15ed4a801c696b64468d1e3d71b4e972950c3072591a26a598e46633fbe51b2a   (unchanged)
    supervise_swap.sh  bd268aa4920d0eb62f6e4576a63cf84ee04b2fce1907d8b9ac2d21cf8ddaa9e6   (step file; not a pinned tool)
    supervise_swap.py  a2eb3474e3ec665ef79a1b772f596ac9dd0a9d5f71bd2db8f611f107bdf00130   (step helpers; not pinned)

### Procedure (as root, `sudo -i`; times Eastern)

The steps are functions of one file, `tools/w5/supervise_swap.sh` (its helpers in `supervise_swap.py` beside it),
which the rehearsal runs as written. Both are staged into the new tree (step 1); neither is a pinned tool. A plan
path given to a step must be spelled exactly as in the running process's argv (no relative form, no symlink
alias): node1's supervisor runs `--plan /srv/work/chains-20261107/oc/plan/oc-node1/CHAINS-PLAN.json`.
Environment for every step (one root shell; the resume script in step 6 sets its own):

        C=/srv/work/chains-20261107; ROOT=$C/oc; PY=/usr/bin/python3.11; export C ROOT PY
        P=$ROOT/plan/oc-node1/CHAINS-PLAN.json; N=$C/tools-mem-9afddf945; S=$N/tools/w5/supervise_swap.sh
        P2=$ROOT/plan/oc-node1/CHAINS-PLAN.supervise-9afddf945.json

Argument order (as the step file reads them): `stop_between_cycles PLAN`, `stop_failing PLAN`, `after_stop` (no
arguments; prints the baseline line), `check_tails`, `cut_partial_tail INDEX`, `start_loop W5 PLAN [--guard-only]`,
`watch_swap PLAN "BASELINE" [swap|recovery] [EVERY] [ROUNDS]`. The baseline is always passed quoted (`"$B"`): it is
one line with four fields.

0. State: `ps -o pid,ppid,rss,etimes,args -p 1568722,1568730` (loop bash and python, RSS about 10.3 GB),
   `readlink /proc/1568730/exe` (/usr/bin/python3.11), `free -g`.

1. Stage the tree. From Jack's PC:

        git -C D:/w5-supervise-mem-20261120-eb005f0 archive 9afddf945 tools/w5/chains.py tools/w5/retention.py tools/w5/replay_sample.py tools/w5/supervise_swap.sh tools/w5/supervise_swap.py | ssh mtg@10.0.0.36 'mkdir /tmp/mem-9afddf945 && tar -x -C /tmp/mem-9afddf945'

   On node1:

        mkdir $N && cp -a $C/tools-src/tools $N/tools && find $N/tools -name __pycache__ -prune -exec rm -r {} +
        install -o root -g root -m 644 /tmp/mem-9afddf945/tools/w5/{chains,retention,replay_sample,supervise_swap}.py $N/tools/w5/
        install -o root -g root -m 755 /tmp/mem-9afddf945/tools/w5/supervise_swap.sh $N/tools/w5/
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
        bash $S watch_swap $P "$(bash $S after_stop)" swap 30 1   # a look before the stop (its baseline is now):
                                                 # one line, admitted, ceiling and driver state per live chain
        bash $S stop_between_cycles $P           # needs exactly one loop and one supervisor (read again each try);
                                                 # waits for a cycle's end; asleep (state S before and after 2 s, the
                                                 # same CPU ticks, report 0 to 55 s old) on the very pids it then
                                                 # signals, each through a pidfd with its argv checked after the pidfd
                                                 # opens; "nothing left" is a fresh scan of $P (what is still found
                                                 # after 20 s gets SIGKILL, then one more round)
        B=$(bash $S after_stop); echo "$B"       # the baseline of this stop: record the line
        bash $S check_tails                      # every index's last line parses (each read under its index lock)
        # only for an index check_tails names `torn`:  bash $S cut_partial_tail <that index>; bash $S check_tails
        bash $S start_loop $N/tools/w5 $P2       # refuses if $P2 already runs or a tail is torn; prints LOOP_PID
        bash $S watch_swap $P2 "$B" swap         # every 30 s until SUCCESS (0), RESUME NEEDED (4) or ESCALATE (2)

   If `stop_between_cycles` fails, its last line says which case it is. `STOP REFUSED: nothing signalled, nothing
   changed`: the old supervisor still runs; investigate, and retry the step or abandon the swap. `STOP INCOMPLETE:
   signals were sent` or `STOP UNCERTAIN: the signal helper's result could not be read`: do exactly

        bash $S pids $P                          # what still runs (for the record)
        bash $S stop_failing $P                  # rescans $P and finishes the stop (pidfd, argv checked), or prints
                                                 # "verified empty: nothing runs $P" (exit 0); exit 1: something is
                                                 # left: stop here
        B=$(bash $S after_stop); echo "$B"       # the baseline of this stop
        bash $S check_tails                      # cut_partial_tail a torn index, then check_tails again
        bash $S start_loop $N/tools/w5 $P2
        bash $S watch_swap $P2 "$B" swap

   The loop logs each exit with its real code (`rc=$?` first; the current loop's `echo "$(date -Is) ... $?"`
   always logs 0, because `$(date)` runs before `$?` is expanded).

4. While `watch_swap` waits: `bash $S pids $P2` (one loop, one python), RSS under 1 GB, the three drivers
   3867287..9 alive. Every round shows each live chain's driver: running, waiting since, or `exited, needs run`.

5. On SUCCESS: a report line in supervise.log with `"ok": true`, `used_bytes` within a few GB of step 0,
   `recovered`, `kills`, `oom_failures` empty; RSS steady (0.7 to 0.9 GB) across two readings; index files growing.
   On RESUME NEEDED: the new supervisor publishes and the named chains' drivers have exited; start each with the
   exited-chain command below, then `bash $S watch_swap $P2 "$B" swap` again until SUCCESS. On ESCALATE (a
   failure, or the escalation time of the deadline rule reached): the recovery below.

   Exited chain (old tree and plan, as `resume-node1.sh` starts it; replace `cN` by the chain the watch names, for
   example c2). Run it only once a supervisor is publishing (the watch shows RESUME NEEDED, or SUCCESS before the
   driver exited); a driver started without a usable GUARD only waits and exits again 1800 s later:

        cd /srv/work/chains-20261107 && setsid nohup python3 /srv/work/chains-20261107/tools-src/tools/w5/chains.py run --plan /srv/work/chains-20261107/oc/plan/oc-node1/CHAINS-PLAN.json --chain cN >> records/run-cN.log 2>&1 < /dev/null &

6. Record: write `$C/resume-node1-mem.sh` with exactly this text (it replaces `resume-node1.sh`, which must not be
   run after the swap: its supervise line would start the old loop). It runs from a fresh root shell
   (`bash /srv/work/chains-20261107/resume-node1-mem.sh`):

        #!/bin/bash
        # chains-20261107 node1 after the supervise memory swap: (re)start supervise (new tree, its plan copy) and the
        # 6 chains (old tree and plan) after a stop (run as root). Chains resume at their last persisted segment end;
        # an unfinished scratch segment is discarded and rerun (deterministic).
        set -u
        C=/srv/work/chains-20261107; ROOT=$C/oc; PY=/usr/bin/python3.11; export C ROOT PY
        T=$C/tools-src/tools/w5; P=$ROOT/plan/oc-node1/CHAINS-PLAN.json
        N=$C/tools-mem-9afddf945; S=$N/tools/w5/supervise_swap.sh
        P2=$ROOT/plan/oc-node1/CHAINS-PLAN.supervise-9afddf945.json
        cd $C || exit 1
        [ -e $ROOT/STOP ] && { echo "remove oc/STOP first"; exit 1; }
        [ "$(bash $S pids $P)" = "LOOP_PIDS ; PYTHON_PIDS" ] || { echo "the old supervisor runs: $(bash $S pids $P)"; exit 1; }
        B=$(bash $S after_stop) || exit 1
        echo "baseline: $B"
        bash $S start_loop $N/tools/w5 $P2 || { echo "supervise not started"; exit 1; }
        for i in 1 2 3 4 5 6; do
          setsid nohup python3 $T/chains.py run --plan $P --chain c$i >> records/run-c$i.log 2>&1 < /dev/null &
        done
        echo "resumed $(date -Is); watch: bash $S watch_swap $P2 \"$B\" swap"

   `start_loop` refuses a plan that already runs and a torn index tail (`check_tails` first). As in
   `resume-node1.sh`, the chains start at once and wait for the new supervisor's first GUARD (about 900 s on a
   fresh start, inside their 1800 s wait); ended chains (c4..c6) return at once (`ended-already`). The printed
   watch line runs from a root shell with the step 3 environment set.

### Forward recovery: the new supervisor fails before publishing (rehearsed)

        bash $S stop_failing $P2                 # bounded: loop, then supervisor, SIGKILL after 20 s, verified;
                                                 # "verified empty" (exit 0) if it had already gone; exit 1: stop here
        B=$(bash $S after_stop); echo "$B"       # the baseline of this stop: a GUARD the failed loop wrote is old now
        bash $S check_tails                      # always, after any stop: a killed supervisor may have torn a line
        # only for an index check_tails names `torn`:  bash $S cut_partial_tail <that index>; bash $S check_tails
        bash $S start_loop $N/tools/w5 $P2 --guard-only   # refuses while a tail is torn
        bash $S watch_swap $P2 "$B" recovery     # never escalates: SUCCESS (0), RESUME NEEDED (4), DEADLINE (3) once
                                                 # the earliest open guard wait + 1800 s has passed, or FAILURE,
                                                 # RESUME NEEDED (5): a failure while a driver has exited

The recovery watch takes the baseline of the `stop_failing` before guard-only's start, so guard-only's first GUARD
counts as success and a GUARD the failed loop wrote before its failure does not; it never escalates, so following
the recovery text does not stop guard-only before it publishes. On RESUME NEEDED start the named chains with the
exited-chain command (step 5) and watch again until SUCCESS. On DEADLINE run `bash $S watch_swap $P2 "$B" recovery`
again a minute later (the driver logs `no-room` at its next 30 s poll): the chain whose wait ran out shows `exited, needs run` and the watch ends with RESUME NEEDED once guard-only
publishes. On FAILURE, RESUME NEEDED guard-only fails too: see the end of the next paragraph.

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
| 2.8 | `start_loop` | exit 0: started: loop 17183 LOOP_PIDS 17183 ; PYTHON_PIDS 17185 / LOOP_PID 17183 |
| 16.8 | driver started | pid 20849; tree old; old_loop 17183 |
| 41.8 | check: old supervisor running | GUARD age 4.9 s; segments 0 -> 8, rows contiguous True; tracebacks 0 |
| 41.9 | tree staged | diff: chains.py, replay_sample.py, retention.py only |
| 42.0 | `plan_copy` | exit 0: ['chains.py', 'replay_sample.py', 'retention.py'] / ok |
| 42.3 | dry cycle | exit 0, refused [], modes ['full', 'full', 'guard-only'] |
| 42.4 | `after_stop` | exit 0: T0=1795214987.777028 GID=1795214982324137314 LOG=1158 RESTARTS=0 |
| 42.6 | `watch_swap` | exit 1: WAIT; drivers c1 waiting since 1795214978.595, c2 unknown (no driver event for this chain) |
| 54.2 | `stop_between_cycles` | exit 0: signal 15: SIGNALLED 17185 ; REFUSED / stopped: /tmp/w5-swap-rehearsal/chains-rehearsal/oc/plan/oc-rehearsal/CHAINS-PLAN.json (next: after_stop, check_tails, start_loop) |
| 54.2 | `after_stop` | exit 0: T0=1795214999.605816 GID=1795214994350733232 LOG=1461 RESTARTS=0 |
| 54.2 | gap begins | guard_age_seconds 5.3; driver {'admitted': 8, 'index_rows': 32, 'boundaries_contiguous': True, 'segments': 8}; driver_alive True |
| 54.3 | `check_tails` | exit 1: torn /tmp/w5-swap-rehearsal/chains-rehearsal/oc/chains/c1/chain.index.jsonl |
| 54.3 | torn tail injected | check_tails_exit 1 |
| 54.5 | `start_loop` | exit 1: torn /tmp/w5-swap-rehearsal/chains-rehearsal/oc/chains/c1/chain.index.jsonl / an index's last line does not parse: cut_partial_tail it first |
| 54.5 | `pids` | exit 0: LOOP_PIDS ; PYTHON_PIDS |
| 54.5 | start_loop refuses a torn tail | exit 1; loops ['LOOP_PIDS ; PYTHON_PIDS'] |
| 54.6 | `cut_partial_tail` | exit 0: cut /tmp/w5-swap-rehearsal/chains-rehearsal/oc/chains/c1/chain.index.jsonl from 14935 to 14878 bytes; the 57 cut bytes are in /tmp/w5-swap-rehearsal/chains-rehearsal/records/torn-tail-c1-20261120T174959.bin |
| 54.7 | `check_tails` | exit 0: ok /tmp/w5-swap-rehearsal/chains-rehearsal/oc/chains/c1/chain.index.jsonl |
| 54.7 | tail cut back | same_bytes_as_before True; saved [True] |
| 57.1 | `start_loop` | exit 0: started: loop 29387 LOOP_PIDS 29387 ; PYTHON_PIDS 29389 / LOOP_PID 29387 |
| 57.1 | new loop: first GUARD | seconds 0.3 |
| 57.3 | watch_swap "$B" swap after the new start | exit 0, SUCCESS; failure []; resume needed []; drivers {'c1': 'waiting since 1795214978.595', 'c2': 'unknown (no driver event for this chain)'}; GUARD new since stop True |
| 72.3 | check: new supervisor, 3-minute check (scaled) | GUARD age 5.3 s; segments 8 -> 13, rows contiguous True; tracebacks 0 |
| 102.3 | check: new supervisor, 10-minute check (scaled) | GUARD age 11.2 s; segments 13 -> 14, rows contiguous True; tracebacks 0 |
| 107.7 | `stop_between_cycles` | exit 0: signal 15: SIGNALLED 29389 ; REFUSED / stopped: /tmp/w5-swap-rehearsal/chains-rehearsal/oc/plan/oc-rehearsal/CHAINS-PLAN.supervise-new.json (next: after_stop, check_tails, start_loop) |
| 107.8 | `plan_copy` | exit 0: ['chains.py', 'replay_sample.py', 'retention.py'] / ok |
| 107.8 | `after_stop` | exit 0: T0=1795215053.221722 GID=1795215048514889769 LOG=2990 RESTARTS=0 |
| 107.9 | `check_tails` | exit 0: ok /tmp/w5-swap-rehearsal/chains-rehearsal/oc/chains/c1/chain.index.jsonl |
| 110.2 | `start_loop` | exit 0: started: loop 8859 LOOP_PIDS 8859 ; PYTHON_PIDS 8861 / LOOP_PID 8859 |
| 130.2 | check: faulty supervisor (raises before every GUARD write) | GUARD age 27.1 s; segments 17 -> 18, rows contiguous True; tracebacks 2 |
| 130.2 | faulty loop never published | guard_unchanged True |
| 130.2 | watch_swap "$B" swap on the faulty loop | exit 2, ESCALATE; failure ['Traceback in supervise.log', "supervisor exits: ['2026-11-20T17:50:59-05:00 supervise exited 1']"]; resume needed []; drivers {'c1': 'waiting since 1795215059.042', 'c2': 'unknown (no driver event for this chain)'}; GUARD new since stop False |
| 130.4 | `stop_failing` | exit 0: signal 15: SIGNALLED 12775 ; REFUSED / stopped: /tmp/w5-swap-rehearsal/chains-rehearsal/oc/plan/oc-rehearsal/CHAINS-PLAN.supervise-faulty.json (next: after_stop, check_tails, start_loop) |
| 130.4 | `after_stop` | exit 0: T0=1795215075.829688 GID=1795215048514889769 LOG=4860 RESTARTS=90 |
| 130.5 | `check_tails` | exit 0: ok /tmp/w5-swap-rehearsal/chains-rehearsal/oc/chains/c1/chain.index.jsonl |
| 132.7 | `start_loop` | exit 0: started: loop 13669 LOOP_PIDS 13669 ; PYTHON_PIDS 13671 / LOOP_PID 13669 |
| 132.7 | guard-only: first GUARD | seconds 0.3 |
| 132.9 | watch_swap "$B" recovery after guard-only | exit 0, SUCCESS; failure []; resume needed []; drivers {'c1': 'waiting since 1795215059.042', 'c2': 'unknown (no driver event for this chain)'}; GUARD new since stop True |
| 158.0 | check: guard-only recovery on the faulty tree | GUARD age 3.1 s; segments 18 -> 22, rows contiguous True; tracebacks 2 |
| 158.0 | guard-only deleted nothing | deleted 0; added 88 |
| 171.3 | `stop_between_cycles` | exit 0: signal 15: SIGNALLED 13671 ; REFUSED / stopped: /tmp/w5-swap-rehearsal/chains-rehearsal/oc/plan/oc-rehearsal/CHAINS-PLAN.supervise-faulty.json (next: after_stop, check_tails, start_loop) |
| 171.3 | `after_stop` | exit 0: T0=1795215116.709190 GID=1795215112265983420 LOG=6051 RESTARTS=90 |
| 171.4 | `check_tails` | exit 0: ok /tmp/w5-swap-rehearsal/chains-rehearsal/oc/chains/c1/chain.index.jsonl |
| 173.5 | `start_loop` | exit 0: started: loop 23301 LOOP_PIDS 23301 ; PYTHON_PIDS 23303 / LOOP_PID 23301 |
| 173.7 | watch_swap "$B" swap back on the new tree | exit 0, SUCCESS; failure []; resume needed []; drivers {'c1': 'waiting since 1795215097.268', 'c2': 'unknown (no driver event for this chain)'}; GUARD new since stop True |
| 193.8 | check: back on the new tree | GUARD age 10.1 s; segments 22 -> 27, rows contiguous True; tracebacks 2 |
| 200.0 | `stop_between_cycles` | exit 0: signal 15: SIGNALLED 23303 ; REFUSED / stopped: /tmp/w5-swap-rehearsal/chains-rehearsal/oc/plan/oc-rehearsal/CHAINS-PLAN.supervise-new.json (next: after_stop, check_tails, start_loop) |
| 200.0 | done | driver {'admitted': 27, 'index_rows': 108, 'boundaries_contiguous': True, 'segments': 27}; guard_loop 13669; new_loop 29387; bystander_alive True |

The old tree is node1's (eb005f097), the new one 9afddf945's step file and modules, run as written and in the
procedure's order: every stop is followed by `after_stop`, whose line is the baseline of the next start, and every
`watch_swap PLAN "$B"` runs after its `start_loop`. One live chain (stand-in trainer, 4-update segments) and one
never-started chain (driver state unknown); a bystander bash whose command line holds `supervise --plan <the
plan>` ran throughout and was never signalled. Every signal went through a pidfd. c1 reached its ceiling in each gap
and was shown `waiting since` until the new GUARD admitted it. The faulty tree is the new tree with
`retention.plan` made to sleep 6 s and raise, so it fails before every GUARD write: the swap-mode watch (baseline
of the new loop's stop) reported ESCALATE and named both failure signals; `stop_failing` stopped it, its
`after_stop` gave the recovery baseline (LOG and RESTARTS offsets past the faulty loop's failures), guard-only was
started, and the recovery-mode watch reported SUCCESS on guard-only's first GUARD. Full transcript:
`N:/w5-supervise-mem-20261120/swap-rehearsal-r6.transcript.jsonl`.

### Not verified

- Root's `readlink /proc/1568730/exe` (not readable as mtg; step 0).
- The dry cycle at normal priority shares CPUs 1 and 17 with pinned work; the real supervisor is not pinned.
- The rehearsal ran on WSL with a stand-in trainer, no w5box instance and no cgroups; node1's drivers run under
  w5box. The swap does not touch w5box, but its interaction was not rehearsed.
- The other node1 supervisors (forks2, wave2, wave2-cont, batch-ladder) keep the old cache and grow with their
  chains; the main-line commit covers them when they move.
