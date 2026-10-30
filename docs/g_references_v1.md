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

`native_expanded_dispatch_v1.py` gains the training family `permuted-units-own-history-v1`. It binds what `permuted-units-v1` binds, with these exceptions:
- A block may start from its fresh initialization or from any checkpoint of that import.
- The import binds by its lineage-independent shape: schema, initializer identity, model, target and parameter layout. Sibling lineages of one recipe therefore share a family.
- Every named opponent must be a checkpoint of the block's own import and feature transfer, and every named assignment must name a listed opponent.
- A named assignment binds as `history` without its id, so the share of history episodes stays bound while the pool grows.

Blocks with an empty pool (blocks 1 and 2) are one family, and blocks with a pool are another. Each is qualified once.

## Driver

```text
python -B python/tools/g_references_campaign_v1.py launch --campaign C.json --runs G1,...,G8
python -B python/tools/g_references_campaign_v1.py status --campaign C.json
```

- `launch` takes the canonical host reservation with `host_slots_v1.py timed --cores` (the union of the lineages' slots) and detaches its supervisor. On Linux the supervisor's stdin, stdout and stderr are `/dev/null`.
- Each lineage has its own pack slot (`placement_by_run`). Before every block, the driver reads the choice and the block family's verification from `choice_pins`.
- Gates before a block starts:
  - Blocks after 2 need `placement_record`: a placement on this host citing three passing records. These are a qualification record from this host with this campaign's runtime, and the records of items 4 and 5 (the Windows evaluator and the Spellbench scorer). The only exception is the qualification tool's own synthetic block, in a scratch campaign that stops there.
  - Blocks after 494 need `continuation`, a pinned D_b "continue" decision.
  - On Jack's PC, the next block must fit the 60 wall hours of claim time (`state/wall/`).
  - The storage allowance must be at most 120 GB.
- Blocks run up to `stop_after_block`.
  - An interrupted block reruns from its starting checkpoint, at most three times.
  - An `invalid` block, or one with non-finite losses, ends the lineage.
  - The baseline's other exposure checks and the block-1 embedding gate are recorded, not a stop.
  - A failed attempt's records (the top-level files and the small per-update records) are copied and hashed before its native tree is pruned.
- Block ends:
  - The last two block ends stay, and the permanent ends stay for good.
  - Older ends are gzip-compressed. They are deleted only once `ancestry_receipt` pins this recipe's interrupted-block regeneration receipt from this host, with `byte_identical` true, meaning equal checkpoint file bytes as well. A rerun in another output root or attempt writes other paths into its checkpoint, so the qualification receipt normally records `byte_identical` false, and ancestry stays compressed.
  - Each step saves its state before removing a file, and publishes its PRUNE entry once (keyed) before it is marked done. A restarted pass finishes a half-done step.
  - Every change is logged in PRUNE with the regeneration recipe.
- At 247, 494 and 741 the driver writes `doses/G<k>-b<block>.json` with the checkpoint, the lineage's play import and the feature transfer.
- On a pod, the guard's `progress.json` activity advances only when a native update completes or a driver step finishes.
- `roots.native` puts trajectories on a separate disk, a pod's container disk. Dispatch records, checkpoints and cold archives stay on the volume.

## Qualification

```text
python -B python/tools/g_references_qualify_v1.py plan   --campaign C.json --spec SPEC.json --out Q
python -B python/tools/g_references_qualify_v1.py launch --campaign C.json --plan Q/plan.json
python -B python/tools/g_references_qualify_v1.py record --campaign C.json --plan Q/plan.json --out Q/QUALIFICATION.json
```

The campaign must stop after block 2 and name `choice_pins`. Inside one reservation, `run` does the following and writes `RESULT.json`. It exits nonzero if any comparison fails. Each finished stage is saved before its scratch payloads are pruned, so a restart resumes the remaining stages.
1. Empty-pool family evidence:
   - launcher trials at 1 worker and at the slot width, then the eight-slot pack, on G1 block 1's payload;
   - the choice, from the spec's inventory (all three placements, at most 24 hours old), with one `check-choice` verification per family.
2. G1's blocks 1 and 2 alone, then all eight lineages at once. G1 is rerun in a scratch campaign as the eighth, and its block ends must be identical.
3. Block 2 rerun with its native process killed at update 81. It must be identical to the straight block: the same state hash, Adam step, and checkpoint content once output paths are set aside, as the launcher compares serial and parallel trials. The regeneration receipt records both identities. It sets `byte_identical` only when the file bytes match too.
4. History family evidence on block 4's schedule (pool {1, 2}) from block 2's end, followed by the choice again over both families.
5. That synthetic block run straight and interrupted. The two runs must be identical and must draw both history checkpoints. The block is discarded afterwards.

`record` adds:
- item 2: bytes per block, and the eight-lineage peak (qualification leftovers included) against the 120 GB cap;
- item 7: every dispatched config carries the runtime profile, and a stripped config is refused;
- the non-natural fraction of blocks 1 and 2;
- `pass`, true when every check passes.

## RunPod

```text
python -B python/tools/g_references_cloud_v1.py package --spec SPEC.json --output PACKAGE
python -B python/tools/g_references_cloud_v1.py plan    --package PACKAGE --control CONTROL --leases LEASES --volume-gb 200 --volume-created-epoch E
python -B python/tools/g_references_cloud_v1.py execute ... --ssh-key KEY --execute
python -B python/tools/g_references_cloud_v1.py ledger  --leases LEASES --volume-gb 200 --volume-created-epoch E
```

The lease cycle, staging, recovery and release are `nine_deck_cloud_v1.py`'s (see `docs/nine_deck_cloud_v1.md`).
- `qualification` mode carries the inventory and launches the qualification tool. The lease completes only when the tool's `RESULT.json` says it passed.
- `production` mode launches the driver. It requires the choice pins and the placement record on the volume.
- A lineage reported complete through an earlier target, qualification's block 2 or D_b, counts as pending work for a lease with a later target.

Trajectories go to `native_root` on the container disk. The dispatcher keeps a 60 GiB reserve on every root's disk, so the lease preparation sets `container_disk_gb` (new in `prepare_lease.py`, default 40). A pack admits its eight packed trials' storage projections together, and a trial is one update, so a qualification spec can set `trial_projected_additional_bytes` (at most the block projection, which is the default) for its trials. A Pod's granted CPUs need not be 0 to n-1 (a 32-vCPU Pod on a 192-CPU host got 5, 9, 11, ...), so the lease binds the package's placements to the granted CPUs by position, writes the bound campaign, qualification spec and inventory beside the staged inputs, launches with them and records the map in `cpu-binding.json`. `execute` refuses a lease that could take the study past 60 USD (compute at the lease's rate ceiling, plus the volume charge over the lease) or past 60 hours of lease time.
