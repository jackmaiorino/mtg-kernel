# Evaluation allocation: separate full-output recovery from native scaling

The completed entropy panel's old guard multiplied a small recovery sample by64. A transient74.212-second sample therefore outweighed faster native execution with16 workers and selected8 workers. The completed panel and all v1 scientific inputs remain unchanged.

New code: python/tools/evaluation_throughput_v2.py and qualify_evaluation_recovery_v2.py. The guard verifies repeated native timing with exact replay outputs, measures full-output export/download/extraction/hash verification separately, and adds that recovery cost once. Each worker allocation needs at least two timing observations. Use the median native/staging time and the slower of two complete recovery observations. Reference recovery may serve a new prepared plan only with the same binary and compatible placement, job/match/game limits, and a current sampled-output estimate within the measured volume. New target work still requires its own input-bound timing and deterministic comparisons. This is an estimate, not a worst-case guarantee about future game lengths; monitor actual production stages.

Qualification E:/mtg-meta-recovery-20260921/evaluation-recovery-qualification-001 executed288 engineering BO3,48 unique already-measured cases repeated across1/8/16 workers twice, reversing the parallel order in the second repetition. All240 replay comparisons matched byte-for-byte. It exported the unchanged completed full-panel corpus twice,3412 files each, with zero mismatches. No new training or scientific outcome measurement was performed.

| Haley workers | Median execution of48 cases | Revised full-panel estimate |
|---|---:|---:|
| 1 | 63.489s | 4094.960s |
| 8 | 15.879s | 1047.794s |
| 16 | 13.965s | 925.650s |

Full-output recovery took19.994 and21.647seconds. Network copy took0.563/0.589seconds; local file verification took14.581/15.958seconds. Full archive content was99,352,690 uncompressed bytes, with71,311,831 bytes of match records. The target sample extrapolated to68,743,680 match bytes. The main repeatable recovery cost was local small-file verification, not transfer bandwidth. This does not prove the cause of the earlier74-second outlier.

The first prototype bound calibration too tightly to the already-completed target. Source ac548a85 and its original receipts are preserved, with source copies under source-at-qualification. Revision28c45119 allows compatible new prepared plans. Qualification root002 revalidated every input/report/archive with the revised guard, using zero additional native games. It selected16 workers. Checks rejected a slower forced selection, missing timing repetition, wrong recovery volume/source, absent calibration and changed unmeasured workload; a distinct plan with identical measured workload was accepted. Original root001 choices reference the old source digest and are not launch authority for revised code.

The supported new full-panel entry point is evaluation_throughput_v2.dispatch_qualified. It checks persisted qualification, fresh owners, newly available hosts, output volume and the plan's projection ceiling before dispatch. Historical frozen runners retain v1 semantics. Future runner integrations must call this entry point; arbitrary raw executable launches are not globally prevented by instruction files.

Current qualification excluded Jack because Kimi's native tests were live, and RunPod because authenticated inventory was403 with no new paid authority. The bounded qualification CLI presently measures the idle-Haley case and rejects if Jack becomes eligible, requiring local/storage and combined allocations to be added before launch. The generic guard accepts other placements only with their own full measured evidence. This is not a permanent reservation of Jack or a claim of global hardware optimality.

Remaining limitation: native sample execution still includes process/model startup, so scaling it by64 is conservative for larger batches. No second full panel was rerun to advertise a speedup. Fine-grained deterministic shards and startup separation remain follow-up work. Fable's known zero-read429 remains a review gap through September22 07:00EDT, not endorsement. This bounded correction proceeded under Jack's useful-compute and continuing research authority.
