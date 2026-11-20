# W5 supervise index memory (2026-11-20)

## Problem

`chains.py supervise --loop` kept every index record of every chain as parsed dicts for the life of the process
(`retention.IndexCache.records`, filled by a whole parse on start and extended every cycle, ended chains included).
On node1 the main chains' supervisor (pid 1568730) held 10.3 GB for 9.28M records and grew about 0.54 GB a day;
a restart on that code needs about 16 GB.

## What the cycle reads

- Every record, every cycle, but only scalar fields: `boundary` (`protected_paths`, the look's protected
  checkpoint), `ledger_usec` (`look` until LOOK.json exists, `first_at_or_past`), `update` (the replay sample's
  watermark, Stage 1a/1b only), and in pair plans `unix_seconds`, `ledger_usec`, `instance` (projection, running
  evidence) and `boundary` (`valid_records`).
- Whole records only for a few: the last record, the last 65 (pair snapshot window), the look hit, the protected
  boundaries, the replay-sample todo past the watermark, and at a pair's finalize `select` and `prune_manifest`.
- `retention.plan` and `guard` read whole records only from `IndexCache.active` (records with a full-state file on
  disk; a few hundred per C chain).

## Design

- `IndexRecords`: per record its line offset and five scalar fields in `array` columns (48 bytes a record). A value
  an array cannot hold exactly (None, an integer past 64 bits, a float in an int field) is kept beside the array; a
  field some record lacks is never served from the columns. A whole record is parsed again from its line on demand.
  On POSIX the indexed file stays open, so a replacement cannot change what is read.
- `IndexView`: what `refresh` now returns instead of a list copy; it reads like that list (len, index, slices,
  iteration, reversed, equality). `retention.fields`, `IndexView.where` and `IndexView.lookup` serve the scalar
  fields; `first_at_or_past`, `ledger_series`, `select`, `chains.valid_records`, `protected_paths`, the pair
  evidence loop and `replay_sample.since` use them when given a view and keep their old code for lists.
- A whole read streams the file (1 MB chunks) and settles records whose full-state files are all absent as they
  pass (the guard settled them in the same cycle before; an absent file is never a plan item or guard count), so
  `active` never holds the index. `settled` is a bitmap (`Boundaries`).
- No record, plan, chain state or decision rule changes; the uncached `index()` path and the driver are untouched.
- Internal differences, none visible in outputs: records are settled at a whole read instead of at that cycle's
  guard; `refresh` returns an `IndexView`, not a `list` (nothing in the cycle serializes the records or checks
  their type); a parsed record is a fresh dict each read, not a shared one (nothing mutates index records); a
  `prune_manifest` or `ledger_at` over a view parses every record (only at a pair's finalize).

## Results

Branches (not pushed): `claude/w5-supervise-index-memory` at e7bf9f37d on main e466fe21f (worktree
`D:/w5-supervise-mem-20261120`), and `claude/w5-supervise-index-memory-eb005f0` at 9cfbd271f on eb005f097, node1's
tree (worktree `D:/w5-supervise-mem-20261120-eb005f0`). The code diff is the same on both.

### Tests (Jack's PC, Python 3.14, 2 host-slot cores)

| Modules | before (main e466fe21f) | after (e7bf9f37d) |
|---|---|---|
| index_cache, tools, replay_sample, byte_counter, chains, scratch, durable, pgrow | 171 run, 0 failed, 29 skipped | 177 run, 0 failed, 29 skipped |

On the node line (9cfbd271f on eb005f097; that commit's index_cache, tools, replay_sample, byte_counter, chains
and scratch modules): 97 run, 0 failed, 14 skipped.

The six new tests (`CompactRecordsTests` in `test_w5_index_cache.py`) check that a view equals `index()`'s list
(index, slices, reversed, iteration); that `first_at_or_past`, `ledger_series`, `project`, `decide`, `select`,
`prune_manifest`, `valid_records`, `protected_paths`, `ledger_at`, `replay_sample.since` and `protected`, and the
pair evidence give the same results on a view and on the list (with None values in the fields); that odd values
(2**70, a float in an int field, True, a string) read back exactly and a missing field falls back to reading
records; `splitlines` parity (CRLF, empty lines, a U+001E separator, an unterminated last line); `Boundaries`
against `set`; and for 200,000 records, 53 bytes a record kept and a whole-read peak of 18.7 MB (a parsed index
would be about 196 MB).

### Old against new on a real index (`tools/w5/tests/supervise_memory_equivalence.py`)

Input: node1 `c4/chain.index.jsonl` (1,053,696 records, sha256 9b52cc4d..., copied read only to
`N:/w5-supervise-mem-20261120/c4`) and its `chain.json`, paths rewritten into the fixture. Fixture: an independent
Stage 0 plan with `c1` live (the real index plus 2,521 updates appended over 19 batches: receipts, checkpoints,
trajectories, segment completions, one driver `index()` append in its own process, one append cut before its
newline, two supervisor restarts) and `c2` ended (same index, END.json); kept checkpoints and recent trajectories
as stand-in files; `r_nom` set so c1's look lands at boundary 1,054,696 in cycle 10. Stubbed: no trainer, no
w5box instance (no ledger, so no chain is live to w5box), `Popen` mocked, `time.time` held to whole seconds plus a
half so files that record the time keep one byte length; wall-clock keys are dropped before comparing. Both tools
trees are copied to one path and the work root is one path, so recorded paths have one length. The driver script
(`pair_run.py`) and the digests are in `N:/w5-supervise-mem-20261120/` (`harness/`, `full-node-line/`, `small/out/`).

Compared per cycle: the report, GUARD.json, SUPERVISE.json, both index files' sha256, LOOK.json, the retention
logs, LOG.jsonl, and a listing of every file under the root with sizes (17,044 files at the end); then the pair
code's reads of the same records (valid records, ledger series, projection, decide running and C-ended,
first_at_or_past at five targets, two `select` specs, `ledger_at`, two PRUNE manifests, the replay sample's
protected set and watermark selection).

| Tools | lines compared | differing |
|---|---|---|
| eb005f097 against 9cfbd271f (node1's code), full c4 index | 22 (setup, 20 cycles, pair reads) | 0 |
| e466fe21f against e7bf9f37d (main), first 30,208 records | 22 | 0 |

Retention deleted 2,888 files over the run, identically. The main pair was not repeated at full size: the old
side's pair reads peaked at 8.0 GB on this PC, past the 6 GB limit set for this work, and the code diff is the same.

### Memory (Windows working set of the harness process, both chains, 2.11M records)

| | old (eb005f097) | new (9cfbd271f) |
|---|---|---|
| after setup | 36 MB | 36 MB |
| after the first cycle (fresh start) | 3,790 MB | 153 MB |
| steady, cycles 2 to 20 | 3,790 to 3,799 MB (5,588 MB after the cut-newline whole read, until the restart) | 154 to 159 MB |
| peak over the cycles | 6,168 MB (a whole re-read beside the old records) | 666 MB (a 0.5 GB cycle-2 transient both versions show, outside the cache) |
| peak with the pair reads | 7,993 MB | 666 MB |
| kept per record | about 1.78 KB | about 58 bytes |

On node1 (Linux, Python 3.11.2, read-only probe of the same c4 file at nice 19 on CPUs 1 and 17, load 22): the
compact columns are 50.6 MB for 1.05M records (48 bytes a record, no side-table entries), process RSS grew 75.6 MB
(72 bytes a record including allocator residue of the streaming parse), peak RSS 97 MB, 37 s to stream and stat;
a scan of one column over all records takes 0.06 s.

Extrapolation to node1's main supervisor (9.3M records, six chains): 9.3M x 48 bytes = 446 MB of columns, plus the
interpreter (about 20 MB), the streaming residue (tens of MB, fixed) and per-cycle transients (chain manifests, the
byte counter; up to 0.5 GB seen here): about 0.5 to 0.6 GB steady and about 1.1 GB peak, against 10.3 GB now and
about 16 GB for a restart of the old code. Growth: about 48 bytes per new record (node1's three live chains add
about 0.5M records a day: about 25 MB a day, against 0.54 GB a day now). Method: steady figures are the resident
set after `gc.collect()` at the end of each cycle; per-record figures are (resident after - resident before) /
records.

### Speed

Steady cycles took 8 to 29 s on this PC new against 10 to 37 s old; fresh starts 159 to 202 s new against 250 to
284 s old. Reads that parse every record through the view (only `prune_manifest` and `ledger_at` at a pair's
finalize) are slower: the pair reads took 429 s new against 117 s old, once per pair.

## Deployment on node1 (written, not run)

### Can the main chains' tree take a minimal patch?

Yes. node1's `tools-src` is a clean checkout of `eb005f097` (`git rev-parse HEAD`; `chains.py` fee77d3f...,
`retention.py` 446f231d...). `IndexCache`, `index_cache`, `chain_index`, `chain_info`, `protected_paths`, `look`,
`cycle`, `supervise`, `valid_records`, `sample_cycle`, `ledger_at` and `pair_decide` are identical there and on
main, and `replay_sample.py` is byte-identical (c52f2263...). The same diff applies cleanly to `eb005f097`: branch
`claude/w5-supervise-index-memory-eb005f0`. No new tools tree from main is needed (main also changes trainer-side
behaviour this plan never pinned).

### Why not re-plan with `make_plan`

`load_plan` compares `plan['tools']` to the running tree's hashes, so the supervisor needs a plan whose `tools`
names the patched files. `make_plan` on the live root would also rewrite `chain.json` of running chains without
`DRIVE.lock` (the drivers append segments to it), write new arm files and overwrite `w5box-C.json` and
`CHAINS-PLAN.json`, which the running `run` processes and `resume-node1.sh` use with the old tree (`load_arm` checks
the old tree against the arm file pins). So the procedure gives the supervisor its own plan copy that differs from
`CHAINS-PLAN.json` only in `tools` (three hashes), and leaves everything the drivers use untouched.

What changes: a new tree `tools-mem-9cfbd271f` (the old tree's `tools/` with three files replaced), a new file
`CHAINS-PLAN.supervise-9cfbd271f.json`, and the supervise loop's command line. What does not change: `CHAINS-PLAN.json`,
arm files, `w5box-C.json`, every `chain.json` (recipe identity has no tool pin: `recipe_identity` covers arm,
recipe, base, parent, trainer sha and segment keys), the drivers and their tree, the index files and their format,
LAUNCHED.json and every record the chains have written.

### Pins

Branch `claude/w5-supervise-index-memory-eb005f0`, commit 9cfbd271f8f036cbfa15a104ae8ff02138193141 (parent eb005f097). Expected
sha256 of the three files:

    chains.py         0fb6ae79ab1fd4843a8983cb6de6e19d77b6f1fc5192dca1497c94818f645b79
    retention.py      544a953e49de2baacd96eac87b5972df04bf9d05ace31cefb8196b94f8610490
    replay_sample.py  15ed4a801c696b64468d1e3d71b4e972950c3072591a26a598e46633fbe51b2a

### Procedure (as root, `sudo -i`; times Eastern)

0. Read the state (expect: loop bash 1568722, python 1568730 RSS about 10.3 GB, GUARD.json younger than 70 s).

        C=/srv/work/chains-20261107; P=$C/oc/plan/oc-node1/CHAINS-PLAN.json
        ps -o pid,ppid,rss,etimes,args -p 1568722,1568730; free -g
        python3 -c "import json,time; g=json.load(open('$C/oc/GUARD.json')); print(round(time.time()-g['unix']), g['ok'], g['used_bytes'], g['ceilings'])"

1. Stage the tree (no running process reads it). From Jack's PC (committed blobs, LF line ends):

        git -C D:/w5-supervise-mem-20261120-eb005f0 archive 9cfbd271f tools/w5/chains.py tools/w5/retention.py tools/w5/replay_sample.py | ssh mtg@10.0.0.36 'mkdir /tmp/mem-9cfbd271f && tar -x -C /tmp/mem-9cfbd271f'

   On node1:

        N=$C/tools-mem-9cfbd271f
        mkdir $N && cp -a $C/tools-src/tools $N/tools && find $N/tools -name __pycache__ -prune -exec rm -r {} +
        install -m 644 /tmp/mem-9cfbd271f/tools/w5/{chains,retention,replay_sample}.py $N/tools/w5/
        sha256sum $N/tools/w5/{chains,retention,replay_sample}.py      # must print the three pins above
        diff -rq -x __pycache__ $C/tools-src/tools $N/tools             # must list exactly those three files

2. Write the supervisor's plan copy and check it (prints the three changed keys, then `ok`):

        P2=$C/oc/plan/oc-node1/CHAINS-PLAN.supervise-9cfbd271f.json
        /usr/bin/python3.11 -B - "$P" "$P2" "$N/tools/w5" <<'EOF'
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
        chains.load_plan(sys.argv[2])  # every pin of the copy checks against the new tree
        print('ok')
        EOF

3. Swap the supervisor between cycles (no chain stops; GUARD.json stays valid for 900 s and a driver that finds it
   stale waits up to 1800 s before exiting):

        SUP=$C/oc/plan/oc-node1/SUPERVISE.json; t0=$(stat -c %Y $SUP)
        while [ "$(stat -c %Y $SUP)" = "$t0" ]; do sleep 1; done; sleep 2   # a cycle just ended; 60 s of sleep follow
        kill 1568722 && kill 1568730                                       # the restart loop first, then the supervisor
        while kill -0 1568730 2>/dev/null; do sleep 1; done; date -Is
        setsid nohup bash -c "while true; do python3 $N/tools/w5/chains.py supervise --plan $P2 --loop 60 >> $C/records/supervise.log 2>&1; echo \"\$(date -Is) supervise exited \$?\" >> $C/records/supervise-restarts.log; sleep 30; done" > /dev/null 2>&1 < /dev/null &
        sleep 5; pgrep -af "chains.py supervise --plan $P2"                 # one bash loop and one python3

   The new supervisor first streams the six indexes (9.3M records) and stats each record's checkpoint path once
   (the old code's first guard did the same stats). Measured on node1 for c4 at nice 19 under load 22: 37 s and
   96 MB for 1.05M records, so expect its first report within about 5 minutes.

4. Check at 3 minutes:

        ps -o pid,ppid,rss,pcpu,etimes,args -p $(pgrep -d, -f "supervise --plan $P2")   # the loop and one python3
        tail -c 4000 $C/records/supervise.log | grep -c Traceback                          # 0
        ls $C/records/supervise-restarts.log 2>/dev/null                    # absent
        ps -o pid,etimes,args -p 3867287,3867288,3867289                     # the three drivers still run

   RSS should be under 1 GB and the process busy; a first report line may not be there yet.

5. Check at 10 minutes: at least two new report lines after the swap time, each with `"ok": true`, `used_bytes`
   within a few GB of step 0, `recovered`, `kills`, `oom_failures` empty, retention entries for c1, c2, c3 as before
   (0 or 1 checkpoint removed a cycle); GUARD.json younger than 70 s with ceilings for c1..c3; RSS stable (expect
   0.5 to 0.8 GB, under 1.5 GB) across two readings 5 minutes apart; the three index files still growing.

        tail -n 3 $C/records/supervise.log | cut -c1-400
        python3 -c "import json,time; g=json.load(open('$C/oc/GUARD.json')); print(round(time.time()-g['unix']), g['ok'], g['used_bytes'], g['ceilings'])"
        ps -o rss= -p $(pgrep -f "chains.py supervise --plan $P2" | tail -1); wc -c $C/oc/chains/c{1,2,3}/chain.index.jsonl

6. Record the change: copy `resume-node1.sh` to `resume-node1-mem.sh` with line 9 using `$N` and `$P2` and the
   `pgrep` check on `$P2`; the chain `run` lines keep the old tree `$T` and `$P`. Note the new tree in the chain
   records (tree path, commit, three pins, plan copy sha256).

### Way back

- If the new supervisor fails (a Traceback each cycle, a restart line, a report that differs from the pattern
  above): kill its loop and python as in step 3. The chains keep running; admissions stop 900 s after the last
  verdict, a waiting driver exits after 1800 s more at a segment boundary, and `resume-node1.sh` restarts them later.
- Going back to the old code is not a safe option on node1 now: its fresh start parses every record (about 1.7 KB
  a record, about 16 GB for 9.3M), more than node1 has free even after the old supervisor's 10.3 GB is released
  (24 of 29 GiB used at 10:16 with it running). Fix forward, or restart the old code only after other work on node1
  has been stopped to free that memory.

### Assumptions not verified

- The lead's sudo covers `kill` of root processes and writing in `oc/plan/oc-node1/` (root-owned).
- `python3` in the loop is `/usr/bin/python3.11` (verified 3.11.2 as mtg; root's PATH not checked).
- A non-interactive bash loop dies on SIGTERM while waiting on its child (default disposition), so it does not
  restart the old supervisor; step 3 checks with `kill -0` only for the python process, so confirm with `ps`.
- The fresh-start time at normal priority on node1 under the training load (measured only at nice 19, one chain).
- Nothing else on node1 reads `CHAINS-PLAN.json`'s `tools` field and expects the supervisor to match it.
- The other node1 supervisors (forks2, wave2, wave2-cont, batch-ladder, trees `abc8b4730` and `1917bea8`) keep the
  same cache and grow with their chains (0.04 to 0.7 GB now); the main-line commit covers them when they move.
