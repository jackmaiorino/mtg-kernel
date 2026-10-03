# throughput-001 (line-b-engineering-throughput-receipt/v1)

Placement: priority below_normal, affinity all logical processors.
Best teach-step rate 33.76 rollouts per second against the cap's 1.48 completed rollouts per reserved host-second (256,000 rollouts in 48 host-hours); the teach step alone, before collection and update time.

| workers | roots | rollouts | teach step seconds | rollouts per second | speedup vs first row | decisions per rollout (mean) | decisions per rollout (max) | censored rollout fraction | host CPU percent before the run |
|---|---|---|---|---|---|---|---|---|---|
| 1 | 20 | 1072 | 263.6 | 4.067 | 1 | 100.6 | 1055 | 0 | 11.2 |
| 8 | 20 | 1072 | 35.87 | 29.89 | 7.349 | 100.6 | 1055 | 0 | 15.2 |
| 24 | 20 | 1072 | 31.75 | 33.76 | 8.301 | 100.6 | 1055 | 0 | 9.8 |

| check | holds |
|---|---|
| one_packet_across_worker_counts | yes |
