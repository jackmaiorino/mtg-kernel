# Public training runs identically on both PCs

The state-only trainer now supports explicit execution-device placement. The same copied executable and unchanged scientific configuration produced byte-identical checkpoints, optimizer states and trajectories on Jack's RTX3050, Jack's RTX4070Super and HaleysPC's RTX4060. This removes the previous GPU1-only implementation restriction for bounded engineering and makes remote training technically usable. It does not yet select a production allocation or establish playing strength.

Native source `7f092a86`, runner `56994793`. Evidence is `E:/mtg-meta-recovery-20260920/public-device-placement-001`. Executable SHA256: `94a80b79979773f9e975c61e0efe232b37485bba2481e07b1c5214f91f22d318`. The release build passed in147.33seconds with four BelowNormal jobs, Rust/Cargo1.94.1 and installed MSVC14.50.35725.0.

`execution_gpu_ordinal` is an optional command field, separate from the scientific config. Omission preserves the old assignment. Update receipts and completion now record both the actual requested device ordinal and collector count. The configuration/default test passed, and the complete new GPU1 run matches the previous native executable's saved learning outputs. No model/config pins, terminal rewards, batch ordering, loss, sampler or frozen measurements changed.

The bounded workload is the previously qualified four updates of two natural games with state-only learning, unchanged g115/A48 and fixed seeds. Six full runs executed48 games over8 distinct scheduled games. All96 checkpoint/optimizer/trajectory comparisons match the original reference. Each process had a180second cap and stopped on any parity failure. The learner and collectors remained on the same host. This qualification contains no new win-rate evaluation.

| Machine / GPU | Collectors | Whole process, seconds | Last three updates, seconds |
|---|---:|---:|---:|
| Jack / RTX3050 | 1 | 31.50 | 10.67 |
| Jack / RTX3050 | 2 | 27.41 | 8.02 |
| Jack / RTX4070Super | 1 | 30.38 | 10.59 |
| Jack / RTX4070Super | 2 | 31.40 | 8.88 |
| Haley / RTX4060 | 1 | 42.48 | 9.15 |
| Haley / RTX4060 | 2 | 46.70 | 8.43 |

The larger GPU is not automatically faster on this small job. Initial updates took17.30-37.53seconds, including startup/JIT and the first game's work. Comparing only total time would incorrectly hide the later parallel benefit in two rows. There is one ordered run per cell, not randomized repetitions; the two-game batches permit at most two useful collectors. These numbers establish compatibility and suggest usable additional capacity after startup, but do not qualify the next larger workload. Total native process time was209.88seconds, partly concurrent across hosts.

Fresh inventory confirmed Jack's16-core/24-thread i7-13700K and both GPUs, and Haley's8-core/16-thread Ryzen73700X with RTX4060. No MTG native worker was present before staging. Normal desktop graphics contexts were preserved. A live remote snapshot tied native PID31944 to the RTX4060 UUID, rather than inferring use from a request file. RunPod's authenticated read-only inventory still returned HTTP403; no pod, credits use or availability claim was made.

Haley had no CUDA compiler runtime. A separate `C:/mtg-node/public-device-placement-001` directory contains pinned model inputs, the two CUDA12.8 NVRTC DLLs and headers, the executable and the runner. All1779 input files were verified there. CUDA environment changes were limited to the qualification process. Temporary D/E drive mappings preserved original absolute model paths and were removed afterward. Existing `C:/mtg-node` experiment roots and normal desktop processes were not changed.

The88.24MB input archive transferred in1.58seconds. The88.99MB results archive took3.92seconds to generate and1.84seconds to download. All61 recovered files were independently rehashed locally with zero mismatches; the32 remote scientific outputs were additionally compared to the local reference. `haley-final-state.json` confirms no remaining trainer or temporary drive mapping. Both local and remote native sessions are terminal. The staged runtime remains available for the next qualification.

The legacy production dispatcher still launches the config's default device. Its guard now explicitly rejects GPU-override benchmark evidence and contradictory completion-device metadata, preventing a fast GPU0 timing from silently authorizing a GPU1 launch. All10 guard tests pass. This is an honest remaining migration requirement, not universal launcher enforcement: a device-aware production dispatcher and remote recovery mapping still need to consume the selected placement before substantial work. Raw/other legacy entry points remain outside this guard.

Next: finish that dispatcher binding and compare representative full batches on the actual next matched prevention/control workload, including larger collector counts and simultaneous independent streams when useful. Include setup/recovery costs and honor current resource reservations. Then freeze the strength comparison with actual prevention exposure in both arms, canonical retention and complete terminal BO3 panels. Do not convert this compatibility test into a win-rate claim or rerun the failed cost-feature experiment.

Fable's known zero-read HTTP429 review gap lasts until September22 07:00EDT. These reversible implementation and bounded qualification steps proceed under Jack's execution assignment; no completed independent review is claimed. The human/league objective remains active and unproven.
