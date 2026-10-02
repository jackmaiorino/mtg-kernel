# throughput-002-pair-replicate (line-b-engineering-throughput-receipt/v1)

The matched pair repeated in the opposite order (full mask first) in the same window, 06:40:42 to 06:41:53 EDT, same executable, driver, batch and QoS. The 'speedup vs first row' column is relative to the 16-worker full-mask row here.

Placement: priority below_normal, affinity 0xffffff, QoS execution-speed throttling opt-out at spawn, read back per run (windows-owned-child-policy/v1).
Best teach-step rate 88.63 rollouts per second against the cap's 1.48 completed rollouts per reserved host-second (256,000 rollouts in 48 host-hours); the teach step alone, before collection and update time.

| workers | affinity mask | throttling opt-out read back | roots | rollouts | teach step seconds | rollouts per second | speedup vs first row | decisions per rollout (mean) | decisions per rollout (max) | censored rollout fraction | host CPU percent before the run | P-core threads busy during the run (mean percent) | E-cores busy during the run (mean percent) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 16 | 0xffffff | yes | 20 | 1072 | 13.07 | 82.01 | 1 | 100.6 | 1055 | 0 | 1.9 | 58.85 | 33.16 |
| 16 | 0xffff | yes | 20 | 1072 | 13.92 | 76.99 | 0.9388 | 100.6 | 1055 | 0 | 16.6 | 65.35 | 17.67 |
| 24 | 0xffffff | yes | 20 | 1072 | 12.1 | 88.63 | 1.081 | 100.6 | 1055 | 0 | 2.2 | 62.4 | 52.73 |
| 24 | 0xffff | yes | 20 | 1072 | 13.84 | 77.45 | 0.9444 | 100.6 | 1055 | 0 | 6.2 | 66.57 | 27.49 |

| check | holds |
|---|---|
| one_packet_across_worker_counts | yes |
| execution_speed_opt_out_read_back_on_every_run | yes |
| affinity_read_back_as_requested_on_every_run | yes |
| packet_equals_expected | yes |
