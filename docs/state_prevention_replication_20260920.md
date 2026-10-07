# Independent prevention replication

Both arms restart from untouched g115, with 2,000 fresh training seeds and 436 fresh evaluation seeds disjoint from the discovery pilot. The 200 updates of ten games, schedule composition, terminal rewards, state-only treatment, zero cost projection and final-endpoint analysis remain unchanged. The frozen 2,616-match BO3 panel retains its integer-win, paired-bootstrap and canonical-retention gates. This replication does not establish human or league strength.

Pilot: `E:/mtg-postboard-campaign-20260920/state-prevention-pilot-002`.

## Completed allocation qualification

`E:/mtg-meta-recovery-20260920/prevention-replication-storage-005/qualification.json` records nine completed allocations across C/D SSDs, E HDD, one/four/ten collectors and simultaneous desktop/compute host execution. There were 580 natural game executions including fresh-process continuation replays, representing 60 unique arm-games. All 576 cross-allocation file comparisons and 48 restart comparisons were exact. Each arm's fixed prefix contained seven prevention-exposed games. Treatment state parameters and both moments changed; control and cost projections stayed zero. No outcome-based seed selection occurred.

| Measured allocation | Projected full training and recovery |
| --- | ---: |
| C SSD, sequential arms, ten collectors | 35.8 min |
| D SSD, sequential arms, ten collectors | 33.4 min |
| E HDD, sequential arms, ten collectors | 33.7 min |
| The maintainer D SSD plus compute host, one collector each | 39.9 min |
| The maintainer D SSD plus compute host, ten collectors each | 28.4 min |

These are projections from three initial full batches, not repeated-run confidence intervals or actual full-run durations. Startup/update variation is material, and the close D/E estimates do not establish a storage speed difference. The primary desktop GPU failed five current idle checks and remained active at the final readback, so this qualification used GPU1 and the compute host GPU0. RunPod's saved read-only inventory returned HTTP403; no paid allocation was made. Earlier attempts 002-004 ended at preflight before native execution and remain preserved.

The selected allocation uses SSD native stores and two compressed archive shards on E, with fsync and full member hash readback. Six negative checks rejected missing storage evidence, slower placement, changed binary, mismatched disk, missing archives and omitted archive cost. The production runner also rejects optimized Python, which would disable assertions in dependencies. These checks cover the new replication path, not every legacy/raw executable entry point.

## Execution and follow-up

`python/tools/run_prevention_replication_v1.py` launched both 200-update arms on September 20 at approximately 23:08 EDT. The launch manifest pins the qualification, guards, binary and runner. Initial native workers were the maintainer PID67360 and the compute host PID2452. Full training and evaluation are not complete at this note's creation. Native receipts retain their actual paths; final checkpoint and optimizer copies on E require exact hash agreement.

After successful training audit, prepare and qualify `python/tools/state_prevention_evaluation_v2.py`. It orders the unchanged independent jobs longest first and uses two-match timing batches to reduce startup distortion. It still requires exact replay and the existing allocation guard before the complete panel. Analyze only the complete 90-job, 2,616-match panel with the unchanged paired analysis. Never pool timing repetitions or inspect outcome prefixes for selection.

Kimi's observation audit and default-off Escape gate at f74d2784 remain queued after replication. They do not change this experiment. Fable's independent review remains unavailable following the recorded zero-source-read HTTP429 until September 22 07:00 EDT. No retry or endorsement is claimed; bounded continuation proceeds under the maintainer's execution assignment with that review gap explicit.
