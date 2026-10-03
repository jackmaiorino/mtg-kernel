# throughput-002 (line-b-engineering-throughput-receipt/v1)

Claim CLAUDE #614 on Jack's PC, window 06:36 to 06:42 EDT on 2026-09-28 (start #628, release #630), announced in #616 after the launcher's yield (#615). Executable 1823e77a (change 9, pinned), driver 42774382, CPU update backend, seed namespace throughput-001, so the batch, roots and packet equal throughput-001's (b0a6b848, checked in every run). Placement per director ruling CLAUDE #611: BelowNormal, the execution-speed throttling opt-out applied at spawn and read back per run (windows_owned_child_policy_v1), explicit affinity per row; each count of 16 or more workers runs again right after under the full mask (the ruling's matched pair). The per-CPU columns cover each whole update run (load, teach step, update, writes), so they include other processes' load; with mask 0xffff the E-core load is not this process. Engineering timing, not an outcome.

Sequence disclosure (CODEX #619, found in cloud-eval's runner at 2a0476dc): throughput-002's children started unsuspended and received the affinity and the opt-out within milliseconds, so the mechanism did not ensure that the readback preceded the work. The teach step starts only after the binary reads its inputs and replays the behavior (0.7 to 0.9 s into each run, by the receipts' input_read_seconds and behavior_replay_seconds), so the readback preceded it in practice, and a late opt-out could only have slowed a run. Per ruling #611 item 2 the completed receipt keeps its recorded configuration. The driver now starts each child suspended and resumes it only after the readback (`spawn_placed`; python/tests/test_line_b_receipts_placement_v1.py: a startup marker stays absent until the resume, and a failed identity readback or a mask outside the host kills the child before it runs).

Placement: priority below_normal, affinity 0xffff, QoS execution-speed throttling opt-out at spawn, read back per run (windows-owned-child-policy/v1).
Best teach-step rate 96.2 rollouts per second against the cap's 1.48 completed rollouts per reserved host-second (256,000 rollouts in 48 host-hours); the teach step alone, before collection and update time.

| workers | affinity mask | throttling opt-out read back | roots | rollouts | teach step seconds | rollouts per second | speedup vs first row | decisions per rollout (mean) | decisions per rollout (max) | censored rollout fraction | host CPU percent before the run | P-core threads busy during the run (mean percent) | E-cores busy during the run (mean percent) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | 0xffff | yes | 20 | 1072 | 112.8 | 9.503 | 1 | 100.6 | 1055 | 0 | 30.4 | 10.65 | 6.488 |
| 8 | 0xffff | yes | 20 | 1072 | 17.98 | 59.62 | 6.273 | 100.6 | 1055 | 0 | 2.2 | 40.05 | 14.19 |
| 16 | 0xffff | yes | 20 | 1072 | 13.8 | 77.69 | 8.176 | 100.6 | 1055 | 0 | 3.5 | 61.66 | 6.281 |
| 16 | 0xffffff | yes | 20 | 1072 | 13.38 | 80.15 | 8.434 | 100.6 | 1055 | 0 | 4.4 | 55.05 | 36.42 |
| 24 | 0xffff | yes | 20 | 1072 | 13.91 | 77.05 | 8.108 | 100.6 | 1055 | 0 | 1.5 | 64.62 | 11.18 |
| 24 | 0xffffff | yes | 20 | 1072 | 11.14 | 96.2 | 10.12 | 100.6 | 1055 | 0 | 5.1 | 56.39 | 45.68 |

| check | holds |
|---|---|
| one_packet_across_worker_counts | yes |
| execution_speed_opt_out_read_back_on_every_run | yes |
| affinity_read_back_as_requested_on_every_run | yes |
| packet_equals_expected | yes |
