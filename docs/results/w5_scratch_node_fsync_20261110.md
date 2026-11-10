# W5 scratch persistence on the nodes

mtg-kernel#601 (T7), the milestone 3 decision from milestone 1's handoff
([`w5_scratch_work_20261108.md`](w5_scratch_work_20261108.md)). On Nov 10 the bench's 256-update persistence
profile ran on node1 and node2, two samples each, with its work directory on the filesystem of each node's W5 root.

**Result.** Both nodes fsync in about 0.18 ms per call: node1 0.176 to 0.177 ms, node2 0.181 to 0.183 ms. The
development VM took 0.27 to 0.39 ms in milestones 1 and 2. Persisting a 256-update segment's records (1,539
members, 2,568 fsyncs) took 0.62 to 0.66 s, about 72% of it in fsyncs. That is about 2.5 ms of persistence per
update.

**Decision (recommended).** Milestone 3's grouped durability boundary could remove at most the fsync share: about
0.46 s per 256-update segment, or 1.8 ms per update. Publication fan-out is therefore not material on these nodes,
and milestone 3 is not worth building. The maintainer's disposition is recorded on #601.

| Node | Sample | fsyncs | fsync time | Per fsync | `persist` wall | fsync share |
|---|---:|---:|---:|---:|---:|---:|
| node1 | 0 | 2,568 | 454.5 ms | 0.177 ms | 632.5 ms | 71.8% |
| node1 | 1 | 2,568 | 451.6 ms | 0.176 ms | 622.6 ms | 72.5% |
| node2 | 0 | 2,568 | 468.9 ms | 0.183 ms | 663.1 ms | 70.7% |
| node2 | 1 | 2,568 | 464.2 ms | 0.181 ms | 642.9 ms | 72.2% |

The rest of `persist` is small too: per segment, copying took 21 to 24 ms, gzip 11 to 12 ms and removing the
scratch directory afterwards 9 to 13 ms. Every sample on both nodes persisted the fixture with the same logical digest (`e32b5ca8…`).

## Conditions

- Nodes: AMD Ryzen 9 9950X (32 CPUs), 30 GiB, Debian 12, Linux 6.12.95, Python 3.11.2. The work directory was on
  ext4 on `/dev/nvme0n1p2`, the same device as the node's W5 root; scratch was tmpfs (`/dev/shm`).
- Production load: unlike the handoff's idle condition, both nodes were running six production chains each, at a
  load average of 12 to 14. The bench ran at `nice -n 19` and `ionice -c2 -n7`, on 8 of the 32 CPUs. The nodes'
  10-second I/O pressure stayed at 0.00 and their load average did not change. The times include whatever
  contention the chains caused, so an idle node would be no slower.
- Source: main `abc8b4730587966c385b9776fa84fd914186fafb`, with `tools/w5` and `tools/ex1` copied by `git archive`
  instead of a clone on the nodes. The receipts' commit fields are therefore null. Their `chains.py`, `retention.py`
  and bench hashes equal main's at that commit, whose `tools/w5` is the tree merged in #616.
- Each run took about 2 s. The temporary directories were removed afterwards, and nothing else on the nodes was
  touched.

Receipts (verbatim): `w5_scratch_node_fsync_20261110/t7-node1-fsync.json` (sha256 `70bcffb8de80…`) and
`t7-node2-fsync.json` (sha256 `4a5df93881e4…`).

## Not covered

- The real checkpoint size. The fixture's checkpoint is 64 KB; a real one adds one larger copy and one fsync per
  segment, which is bandwidth, not fan-out.
- An idle node, per the handoff's condition (see production load above).
- Durable updates/hour for milestone 2: the code-only comparison in #616's handoff, which stays optional.

## Reproduce

On a node, from a copy of main's `tools/w5` and `tools/ex1`, with an empty directory on the W5 root's filesystem:

```bash
nice -n 19 ionice -c2 -n7 python3 tools/w5/tests/scratch_bench.py --out t7-node1-fsync.json --sizes 256 \
  --rounds 2 --no-counts --scratch-dir /dev/shm --work-dir WORK --max-wall-seconds 600 --quiet
```
