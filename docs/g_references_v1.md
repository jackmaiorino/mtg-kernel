# G(i) fresh-init reference lineages

Tooling for eight self-play lineages, G1 to G8, each trained from its own fresh initialization on the nine-deck recipe (collab `LANES/g-references-20261028/DESIGN.md` v1.2 and `FROZEN.md`).

| File | Role |
|---|---|
| `python/tools/g_references_v1.py` | Block configs: the nine-deck baseline with four fields changed (fresh start, own-history `fixed` slot, `g-references-v1/*` namespaces, runtime profile); fresh-initialization requests, descriptors and the distinct-starts check |
| `python/tools/g_references_namespaces_v1.py` | The study's namespaces against every registered one (code only) |
| `python/tools/g_references_campaign_v1.py` | Campaign driver (Windows or a Linux pod): chained 162-update blocks through the supported launcher, own history, stop block, checkpoint retirement, dose records |
| `python/tools/g_references_qualify_v1.py` | Qualification on one host: launcher trials and pack, the choice, blocks 1 and 2 alone and eight at once, the interrupted-block receipt, the synthetic history block, bytes |
| `python/tools/g_references_cloud_v1.py` | RunPod packages and leases, derived from `nine_deck_cloud_v1.py`, plus the study's lease ledger and caps |

## Recipe

Everything not listed is `nine_deck_baseline_v1.py`'s own code: units, opponent kinds by repeat, starting player, the SHA-sorted order packed five per update, frozen method fields and episode limits.

- **Start.** Block 1's `initial_source` is the lineage's fresh-initialization descriptor (`play_import`) with `checkpoint: null`; block b starts from block b-1's checkpoint.
- **History.** The pool for block b is the lineage's permanent block ends at or below b-2: 1, 2, 4, 8, every 16th block, and the doses 247, 494 and 741. A unit with repeat 8 or 9 plays pool index `u mod n`, where u is the first 8 bytes (little-endian) of SHA256 of `["g-references-v1/history", run, block, o, t, j]` in compact JSON. An empty pool (blocks 1 and 2) plays `initial`. Block 741 lists 52 history opponents; the engine's 32-opponent cap applies per 10-episode preparation.
- **Runtime rules** (the collab lane's AMENDMENT-1.md). Every one of a block's 1,620 episodes carries `runtime_rules: "resolution-boundary-v1"`; the baseline sets none. The trainer leaves the profile out of state hashes, so the block config is the record. The driver refuses a config with any episode outside the profile and pins every config it dispatches. No mainboard contains a Delve card, which the profile refuses.
- **Seeds.** `g-references-v1/order`, `/train` and `/history` per unit; `/init` gives each lineage's `base_seed`; `/panel/mirror` and `/panel/cross` belong to the evaluator.

## Launcher family

`native_expanded_dispatch_v1.py` gains the training family `permuted-units-own-history-v1`. It binds what `permuted-units-v1` binds, except that a block may start from a fresh initialization or any checkpoint of the same import and feature transfer, every named opponent must share that import and transfer, and a `fixed` assignment counts like `initial`. One qualification of block 1's first update then covers every block, whatever its history pool.

## Driver

```text
python -B python/tools/g_references_campaign_v1.py launch --campaign C.json --runs G1,...,G8
python -B python/tools/g_references_campaign_v1.py status --campaign C.json
```

- `launch` takes the canonical host reservation with `host_slots_v1.py timed --cores` (the union of the lineages' slots) and detaches its supervisor. The supervisor's stdin, stdout and stderr are `/dev/null` on Linux.
- Each lineage has its own pack slot (`placement_by_run`). The choice and its verification are read from `choice_pins` before every block, so a refreshed inventory applies from the next block on.
- Blocks run up to `stop_after_block`. An interrupted block reruns from its starting checkpoint, at most three times. An `invalid` block, or one with non-finite losses, ends the lineage. The baseline's other exposure checks and the block-1 embedding gate are recorded, not a stop.
- Block ends stay while they are one of the last two, and for good at the permanent ends. Older ends are gzip-compressed, and deleted only once `ancestry_receipt` pins a byte-identical regeneration receipt. Every change is logged in PRUNE with the regeneration recipe.
- At 247, 494 and 741 the driver writes `doses/G<k>-b<block>.json` with the checkpoint, the lineage's play import and the feature transfer.
- On a pod the guard's `progress.json` activity advances only when a native update completes or a driver step finishes.
- `roots.native` puts trajectories on a separate disk (a pod's container disk); the dispatch records, checkpoints and cold archives stay on the volume.

## Qualification

```text
python -B python/tools/g_references_qualify_v1.py plan   --campaign C.json --spec SPEC.json --out Q
python -B python/tools/g_references_qualify_v1.py launch --campaign C.json --plan Q/plan.json
python -B python/tools/g_references_qualify_v1.py record --campaign C.json --plan Q/plan.json --out Q/QUALIFICATION.json
```

The campaign must stop after block 2 and name `choice_pins`. Inside one reservation, `run` does the following:
1. Launcher trials at 1 worker and at the slot width, then the eight-slot pack, all on G1 block 1's payload.
2. The choice from the spec's inventory (all three placements, at most 24 hours old), verified with `check-choice`.
3. G1's blocks 1 and 2 alone, then all eight lineages at once. G1 is rerun in a scratch campaign as the eighth, and its block ends must match.
4. Block 2 rerun with its native process killed at update 81. It must equal the straight block in checkpoint bytes and state hash; this is the regeneration receipt.
5. A synthetic block, discarded afterwards: block 4's schedule (pool {1, 2}) from block 2's end, run straight and interrupted.

`record` adds bytes per block and the eight-lineage peak against the storage cap, plus the non-natural fraction of blocks 1 and 2.

## RunPod

```text
python -B python/tools/g_references_cloud_v1.py package --spec SPEC.json --output PACKAGE
python -B python/tools/g_references_cloud_v1.py plan    --package PACKAGE --control CONTROL --leases LEASES --volume-gb 200 --volume-created-epoch E
python -B python/tools/g_references_cloud_v1.py execute ... --ssh-key KEY --execute
python -B python/tools/g_references_cloud_v1.py ledger  --leases LEASES --volume-gb 200 --volume-created-epoch E
```

The lease cycle, staging, recovery and release are `nine_deck_cloud_v1.py`'s (see `docs/nine_deck_cloud_v1.md`). In `qualification` mode the package carries the inventory and launches the qualification tool. In `production` mode it launches the driver and requires the choice pins on the volume.

Trajectories go to `native_root` on the container disk. The dispatcher keeps a 60 GiB reserve on every root's disk, so the lease preparation sets `container_disk_gb` (new in `prepare_lease.py`, default 40). `execute` refuses a lease that could take the study past 60 USD (compute at the lease's rate ceiling plus the volume charge) or past 60 hours of lease time.
