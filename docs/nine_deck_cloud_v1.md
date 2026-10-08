# Nine-deck campaign on a RunPod lease

`python/tools/nine_deck_cloud_v1.py` runs the nine-deck campaign driver
(`nine_deck_campaign_v1.py`) on one rented CPU Pod per lease, from the
workstation (Windows OpenSSH or Linux `ssh`/`scp`). It generalizes the D3
evaluation controller (`g115_d3_cloud_v1.py`, unchanged) to training. Without
`--execute` it allocates nothing, reads no credential and makes no network
call. Paid compute still needs the maintainer's approval.

```text
python -B python/tools/nine_deck_cloud_v1.py package --spec SPEC.json --output PACKAGE [--host-slots host_slots_v1.py]
python -B python/tools/nine_deck_cloud_v1.py plan    --package PACKAGE --control CONTROL
python -B python/tools/nine_deck_cloud_v1.py execute --package PACKAGE --control CONTROL --ssh-key KEY --execute
```

`CONTROL` holds `lease/` (the `phase1_cloud/prepare_lease.py --spec` output,
which the external guard also reads) and `funding.json`. Every receipt of the
lease is written there.

## Package (offline)

The spec names the workstation campaign used as a template (`lane`, `decks`,
`t1_source`, `a48_source`, `wall_seconds`, `cold_keep_blocks`), the runs, the
Pod placement, storage numbers, the Linux trainer with its engine commit and
tracked tree, the expected Pod memory and, once they exist, the choice and
its verification:

```json
{"schema": "nine-deck-cloud-package-spec/v1", "campaign_root": "nine-deck-linux-a",
 "campaign": "ABS/campaign.json", "runs": ["r1", "r2"],
 "placement": {"cpu_affinity": [0, 1, 2, 3, 4, 5, 6, 7], "workers": 4, "preparation_workers": 4,
               "memory_bytes": 8589934592},
 "storage": {"max_logical_bytes": 161061273600, "reserve_bytes": 64424509440,
             "projected_additional_bytes": 21474836480},
 "lease_block_seconds": 5400,
 "runtime": {"binary": "ABS/native_expanded_training_run_v1", "engine_commit": "40 HEX",
             "tracked_tree_sha256": "64 HEX"},
 "hardware": {"min_memory_bytes": 34359738368, "qualified": "ABS/actual-hardware.json"},
 "choice": {"path": "/workspace/nine-deck-linux-a/qualification/choice.json", "sha256": "SHA256"},
 "choice_verification": {"path": "/workspace/nine-deck-linux-a/qualification/verification.json", "sha256": "SHA256"}}
```

The numbers are syntax illustrations. `lease_block_seconds` is the first
lease's block estimate, measured with the same concurrency (all runs share the
placement CPUs); later leases use 1.25x the longest block measured on runpod.
`hardware.qualified` (optional) is a previous lease's `actual-hardware.json`;
the Pod must then match its CPU model and topology, since libm may choose
code paths by CPU.

Three tar.gz archives (regular files only) and `package.json`, which pins
every member, extract under `/workspace/<campaign root>` on the network volume:

```text
runtime/<binary sha256>/     the trainer and runtime.json (platform linux-x86_64; loader, libc, libm
                             and libgcc_s pinned at /usr/lib/x86_64-linux-gnu in the pinned image)
tooling/<tooling id>/        python/tools: the import closure of the campaign driver, dispatcher, host
                             reservation and lease guard, plus host_slots_v1.py
inputs/objects/<sha256>/     checkpoints and imports byte for byte, rewritten import descriptors, campaign.json
inputs/decks/<id>/           data/runtime_decks_v1.json with data/cards_v1.json and the registered 75s beside it
campaign/                    the campaign roots (state, cold, retained, checkpoints, exposure, hot); accounting root
control/<lease name>/        per-lease staging, launch log and results archive
```

- The tooling closure is derived from imports (lazy ones included) with the
  Pod's search path; anything else must be standard library. `package`
  imports every closure module from a copy holding only the closure.
  `host_slots_v1.py` comes from the tree, else from `--host-slots`, pinned to
  spellbench e7db6861 (sha256 `ae0e5b92...`, mtg-kernel 3d00a457).
- The campaign is rewritten for the Pod: `runpod` placement, cores declared
  to `host_slots_v1.py timed` equal to the placement CPUs, roots under
  `campaign/`, Linux storage (`projected_volume_bytes` key `""`), the
  package's runtime. Inputs keep their bytes; only `play_import` descriptors
  are rewritten, so their initialization and parameter pins name the Pod
  copies. The dispatcher's workload digest binds these paths, so qualify on
  the Pod with this package's inputs.
- The choice and its verification must already be on the volume under the
  campaign root; they are checked on the Pod before the driver starts.
  Without them a package is not production-ready and `plan` refuses it.

## Plan

`plan` verifies the archives against `package.json` (built by this
controller), then the prepared lease: `lease_guard.validate` (caps, at most
eight hours, margin, funded balance), at most 300 s since preparation, the
lease file equal to the one embedded for the resident guard, the pinned image
digest the runtime libraries belong to, the Pod shape (`cpu3c`, 32 vCPUs,
volume, mount, name), `startup_idle_seconds` >= 900 and `recovery_seconds` >=
600, the placement against the Pod (and the qualified hardware), a fresh
funding snapshot, the external guard file, and that a block of every run
fits: deadline - recovery - 900 s startup >= estimate + 600 s collection.

## Execute: one lease

1. Plan, then read `RUNPOD_API_KEY` from the environment (Windows: the user
   registry). It stays in memory; the worker never receives it.
2. Start `phase1_cloud/external_guard.py CONTROL` detached.
3. Create the Pod (`prepare_lease.execute_create`), wait for its SSH endpoint
   (rate and identity verified), then for the resident guard's `guard.json`:
   this Pod, `provider_ok`, `allow_new_dispatch`, not latched.
4. Record `actual-hardware.json` and check it: the four image libraries by
   path and SHA256, placement CPUs and quota, memory, volume free space for the
   dispatcher's reserve plus projection, and the qualified profile if pinned.
5. Stage: each archive's members already on the volume with matching SHA256
   are skipped; only missing ones are uploaded (a delta archive) and
   extracted; different bytes at a member path are refused, never replaced.
   Then the volume inputs (choice and verification) are verified.
6. Copy the lease into `/run/phase1/<lease>/lease.json` and launch the driver
   (`nine_deck_campaign_v1.py launch`, which takes the host reservation and
   detaches its supervisor) with `RUNPOD_POD_ID`,
   `MTG_LEASE_GUARD_DIR=/run/phase1/<lease>`, `MTG_HOST_LOCK_ROOT` and
   `HOST_SLOTS_ROOT` on Pod-local `/var/lib/mtg-node/host-lock`, and no
   `RUNPOD_*` provider variables; write `worker-started.json`.
7. Every 20 s read the Pod from the provider and the run states, guard and
   supervisor over SSH. Unavailable observations are gaps (as D3) until the
   recovery window. The lease ends when the supervisor exits; a latched guard
   or the recovery window stops the supervisor first.
8. Always, also after any error: recover a tar.gz of the campaign state,
   retained records and sampled trajectories, block-end checkpoints, exposure
   receipts, dispatch logs, guard records and the reservation's events into
   `CONTROL/recovered` (archive and every file SHA256-verified), write
   `release-authorized.json`, DELETE the exact Pod, and confirm absence with
   `verify_pod_absent` (two not-found lookups 30 s apart).

`completion.json` records `complete`, `lease_stopped`, `blocks_completed` (and
`blocks_completed_this_lease`) per run, `run_status`, `stop_reasons`,
`recovered`, `release_confirmed` and any error. The exit code is 0 when the
campaign completed or stopped with `lease_time_exhausted`, outputs were
recovered and release was confirmed.

## Resuming across leases

Campaign state, checkpoints and archives live on the volume. Prepare a fresh
lease and run `execute` again with the same package: staging uploads nothing,
and the driver resumes each run at its next block (a block a lost lease
interrupted reruns from its starting checkpoint). A rebuilt package lands in
new content-addressed tooling and input paths beside the old ones; the roots
stay the same. Each lease recovers what changed since its launch plus every
top-level state record; `--recover-all` takes everything. Outputs stay on the
volume, so a failed or partial recovery loses nothing: the next lease, or
`--recover-all`, brings them back. Size `recovery_seconds` for a lease's
outputs, since the resident guard releases the Pod that long after the driver
finishes.

## Still manual

- The funding snapshot (`CONTROL/funding.json`, at most 30 minutes old) and
  the lease preparation with fresh account and quote observations.
- Creating the network volume in EU-RO-1. The dispatcher needs 60 GiB free
  plus the projection on it, more than `prepare_volume.py`'s 50 GB cap allows.
- The qualification lease: serial then parallel qualification on the Pod with
  this package's inputs, the choice (all three placements inspected, runpod
  eligible) and `check-choice` on the Pod. The inventory must be under 24 h old
  at each dispatch.
- The SSH key pair: the public key in the lease spec, the private key for
  `--ssh-key`.
