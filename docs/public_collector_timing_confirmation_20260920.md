# Counterbalanced collector timing confirmation

Complete: nine processes,27 updates,270 natural games. Fixed windows0-2,99-101,197-199; worker orders1/4/8,4/8/1,8/1/4. All324 saved checkpoint, optimizer and episode files match the original completed pilot byte-for-byte. No cargo/rustc processes were present at the before/after checks. No frozen run or model was changed.

| Workers | Nine updates plus three process starts, seconds | Six post-startup updates, seconds | Collection within those six, seconds |
|---|---:|---:|---:|
| 1 | 119.48 | 45.93 | 30.52 |
| 4 | 89.17 | 30.11 | 13.27 |
| 8 | 87.18 | 29.65 | 11.60 |

Eight workers reduced the complete-process sum by27.0% (1.37x speedup) and post-startup update time by35.5% (1.55x). Four workers were close: only2.2% slower in the complete-process sum. One execution per window/worker cell does not establish a precise four-versus-eight ranking or confidence interval. These are three observed windows, not a full200-update campaign. Before/after checks cannot exclude brief or non-build contention.

Parallel collection is now verified in early/middle/late learning and preserves full optimizer continuation. Global placement remains unqualified: neither HaleysPC GPU0 nor Jack GPU0/cloud was timed for this GPU1-specific trainer. No production compute-choice file, substantial training run or rental was issued. Timing result file: E:/mtg-meta-recovery-20260920/public-collector-timing-confirmation-001/result.json. Runnercommit b70edab3, copiednative600b02c5; source and binary pins in manifest.json.

All own native timing processes ended. Total confirmation wall time312.52seconds includes repeated data verification and resource checks. This is engineering evidence, not an improvement in playing strength.
