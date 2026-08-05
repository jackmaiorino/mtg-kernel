# Current Net8 CP7 terminal response v4 throughput result

Status: topology selected. Formal collection has not started.

The bounded revealed-seed screen compared the default eight-worker topology
with the single authorized alternative. Both runs completed all 64 games,
passed exact revealed-output identity, stayed above `16 GiB` available system
memory, stayed below `90%` system-memory use, and used zero GPU 1 memory.

| Attempt | Workers | Pairs per task | Games/s | Average CPU | Maximum RSS | Result |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| 01 | 8 | 4 | 0.440188 | 33.39% | 11.24 GB | selected |
| 02 | 16 | 2 | 0.323490 | 36.05% | 22.30 GB | not selected |

The `0.60` games/s threshold triggered one alternative. It was not a stop
threshold. Attempt 01 is selected because it is faster and resource-safe. No
third topology is authorized. At its measured rate, 512 games project to
1,163 seconds, or 19.4 minutes. The formal tasks use larger pair batches to
amortize JVM startup, so this projection is conservative but not guaranteed.

This selection used throughput, resources, and revealed identity only. It is
not playing-strength evidence and did not inspect the fresh formal panel.
