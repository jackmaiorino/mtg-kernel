# Pooled collection: the dispatcher's fixed cost per update

Fixed overhead per update = pooled iteration wall minus in-process wall at the same batch. Lead's target:
as low as possible; above about 0.5 s, cross-host pooling does not pay.

## Profile (Mac proof, 60 games, learner plus three collectors, means over 16 updates)

Source: the lane's `trackA/MAC.md` timing table and the 16 `dispatch.json` receipts on the Mac (read only),
plus the dispatcher code at the box runner head. Pooled 6.73 s against in-process 1.65 s: 5.08 s fixed.

| Part | Seconds | Where it goes |
|---|---|---|
| Play granularity | 0.69 | slowest of three 20-game shards on 4 workers (1.51) against 60 games on 12 workers (0.82); not dispatcher cost |
| Transport, slowest shard | 2.36 | 9 agent connections per shard, 8 outside play, all serial (below) |
| After the shards | 1.81 | merge, staging removal, retention, process start; audits on every 4th update |
| Validation and update | 0.23 | contention on one machine |

Requests per shard on an agent host, before this change (each 0.19 s on the Mac: docker exec 0.05, agent
start 0.14; 0.27 s node2 to node1 over plain ssh, 0.044 s with connection reuse):

1. `remove_shard` before the run: redundant, the tag is new to the invocation (uuid) and the agent refuses an existing one.
2. `pin_status` of every input: the agent hashes every input (checkpoint, opponents, import files).
3. `pin_status` again for each absent pin, under the per-input lock: redundant with one dispatcher thread per host (push itself accepts a racing install of the same bytes).
4. `push` of the new checkpoint (14.8 MB).
5. `put_shard`: the agent hashes every input again.
6. `run_shard`: again, plus the 51 MB collector; then play.
7. `fetch_shard`: 70 to 90 MB per host at 60 games.
8. `remove_shard` of the output.
9. `remove_shard` again for the command file: the same request (it removes both).

So 8 x 0.19 = 1.5 s of request latency plus about 0.8 s of agent hashing and data. A shell host makes 10
connections (its first is a `pin_status` that hashes the 51 MB collector every update). The local host hashes
the collector and every input in process every update and copies its shard.

After the shards: audits ran on updates 0, 4, 8 and 12 only (`audit.every` is 4 in the proof's hosts file;
`audit_slots` already returns nothing on other iterations). Audit updates spent 0.8 to 2.5 s replaying two slots
one after another, each a full 9-request chain. Non-audit updates spent 0.1 to 1.4 s (mean about 0.75) from the
last shard to the receipt (the trainer's merge reads, hashes and copies about 210 MB, then the staging copy is
removed) and about 0.6 s outside it: process start and module load (45 ms measured on x86, more in the Mac VM),
the learner's input closure recomputed per host and again in retention (it rehashes import files each time),
retention, and the trainer's own handling.

## Changes

- Connection reuse, no code: the hosts file's `ssh` list carries `-o ControlMaster=auto -o ControlPath=<dir,
  mode 700>/%C -o ControlPersist=120`; the dispatcher passes it to every call unchanged (tested). Never add `-v`
  there (the background master would keep the call's stderr open). A dead master fails its calls like any
  connection failure and the host fails over; the next call starts a new master (tested with a fake mux ssh).
- Requests per steady update on an agent host: 9 to 6 (`pin_status` of the new checkpoint only, `push`,
  `put_shard`, `run_shard`, `fetch_shard`, one `remove_shard` off the critical path). A command's own pins
  confirmed on an agent host in an earlier collection are not asked again: the agent hashes them at `put_shard`
  and `run_shard` and refuses the store if one vanished, and then the dispatcher asks again, pushes and stores
  once more (no failover). Import and nested files are asked every collection (the agent lets a missing one
  through to the loader). Shell and local hosts are asked once per collection. A host that fails a shard or an
  audit replay loses its knowledge; pruned paths are forgotten.
- No pre-run removal; a failed run removes its output and command file before the shard moves on.
- Cleanup of a fetched shard runs in the background and overlaps the next steps; the receipt waits for it.
- Audit replays on different hosts run concurrently (same host choice; one host's replays stay serial). Any
  error in a replay fails the collection before the merge, and every planned slot must end in a comparison.
- Learner side: input hashes are kept per file signature (device, inode, size, modification and change times),
  so the closure and the local host's pin checks rehash only changed files. A local shard is renamed into staging.
- Retention: storage markers and owned removals run per host in parallel; its events now reach the receipt.
- The receipt gains `timing` (since module load on the first collection, plan, shards, audit, merge, staging
  removal, cleanup wait, prune) and `calls` (count and seconds per host and request) for the measured acceptance.
- Unchanged: the agent and its seven operations, one request per connection, every trajectory byte and the merge.

A streamed fetch (extract while receiving, no temporary file) was tried and dropped: on the bench it was slower
at 480 games (two Python copies through a pipe against one write to the page cache).

## Bench (tools/w5/tests/pooled_dispatch_bench.py, WSL, one core)

Real agents behind a fake ssh with a fixed latency per connection, three hosts, 3.5 MB per game written by
a collector that plays nothing, a Python merge that reads, hashes and copies. One machine is one path namespace,
so nothing is pushed (a real update adds one push per host, before and after alike). Steady updates, mean wall:

| Case | Box runner head | This change | Connections per update |
|---|---|---|---|
| 60 games, 0.044 s per connection | 1.33 s | 1.00 s | 21 to 15 |
| 480 games, 0.044 s | 4.23 s | 4.20 s | 21 to 15 (data and merge dominate) |
| 60 games, 0.19 s, audits every update | 6.20 s | 2.82 s | 35 to 23 |

How to measure a real run: each `dispatch.json` has `timing` and `calls`; fixed overhead is the trainer's
iteration wall minus the in-process wall at the same batch.

## Resident dispatcher

Linux learners only (the peer check uses SO_PEERCRED; elsewhere the client always collects in its own
process). Start one per run, beside the trainer, with the run's hosts file:

    MTG_POOLED_COLLECT_CONFIG=/srv/work/RUN/hosts.json pooled_collect.py serve /srv/work/RUN/.resident [IDLE_SECONDS]

and give the trainer `MTG_POOLED_COLLECT_RESIDENT=/srv/work/RUN/.resident` (the `--collect-external` program
stays `pooled_collect.py`). Start the server inside the trainer's own claim or box, as a child of the learner's
launch with its environment: the merge and local collectors run as the server's children. The server serves a
request only when the client's working directory, CPU set, cgroup, niceness and the variables listed in
`RESIDENT_CONTEXT_*` (PATH, library and thread settings, `MTG_*`, ...) equal its own, and it needs absolute
`merge_binary` and `state_path`. The socket directory is the account's, mode 700 (created so, refused otherwise,
checked by the client too, with the server's uid on Linux); one server per directory; it stops on SIGTERM, on `<dir>/STOP`, or after
IDLE_SECONDS (default 1800) idle, answering every request it accepted first.

The per-update program then runs only a small client at the top of the file (no other imports): it sends the
command with the hash of its hosts file, of the dispatcher source and its process context; the server answers
ready only when all match its own, the client commits, and the server runs the same collect (retention included,
so a retention error is exit 1 as one-shot) and returns the one-shot exit code and stderr. What the server keeps:
no interpreter start or module load per update, pin knowledge on agent hosts across updates (only the new
checkpoint and import files are asked for), and the learner's input hashes. Python still compiles the whole file
on each start of the per-update program (about 8 ms on x86); a separate client module would remove that.

Fallback, not fail closed, when no server takes the request: until the client commits nothing has started, and a
one-shot collection gives the same bytes, so a missing server costs time instead of the attempt.
`MTG_POOLED_COLLECT_RESIDENT_ONLY=1` fails instead, for measurements that must use the server. After the commit
the client never falls back: a lost server is exit 1 and the attempt fails closed (the server may have staged or
merged; the merge refuses an existing `collect/` anyway). A client killed after its commit leaves the collection
to finish; the next request waits for it.

Measured (WSL, two local hosts, 9 games, start-up dominated): the per-update program took 0.196 s one-shot and
0.113 s handing off to the server (median of 9 updates each), so about 0.08 s per update from process start and
module load alone.
