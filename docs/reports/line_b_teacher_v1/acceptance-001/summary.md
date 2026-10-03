# acceptance-001 (line-b-engineering-acceptance-receipt/v1)

Updates (engineering values):
| update | after state | Adam step | gauge residual | gauge bound | CUDA envelope max | head L2 ratio | mean rescore change | seconds |
|---|---|---|---|---|---|---|---|---|
| cuda1-control-a | 7e14ba8cca8d | 32401 | 1.455e-09 | 0.0001021 | n/a | n/a | n/a | 30.51 |
| cuda1-control-b | 7e14ba8cca8d | 32401 | 1.455e-09 | 0.0001021 | n/a | n/a | n/a | 21.45 |
| cuda1-reverse-kl-a | 6d5b034202e4 | 32401 | -8.445e-10 | 0.0001025 | 3.709e-06 | 0.002957 | 4.463e-07 | 78.52 |
| cuda1-reverse-kl-b | 6d5b034202e4 | 32401 | -8.445e-10 | 0.0001025 | 3.709e-06 | 0.002957 | 4.463e-07 | 70.17 |
| cuda1-forward-kl-a | 83802b8b8e83 | 32401 | -4.225e-10 | 0.0001025 | 3.709e-06 | 0.003824 | 5.533e-07 | 75.84 |
| cuda1-forward-kl-b | 83802b8b8e83 | 32401 | -4.225e-10 | 0.0001025 | 3.709e-06 | 0.003824 | 5.533e-07 | 82.27 |
| cpu-control-a | 68c49454e2b9 | 32401 | 1.863e-09 | 0.0001021 | n/a | n/a | n/a | 4.365 |
| cpu-control-b | 68c49454e2b9 | 32401 | 1.863e-09 | 0.0001021 | n/a | n/a | n/a | 8.81 |
| cpu-reverse-kl-a | b240f95f2359 | 32401 | 1.863e-09 | 0.0001025 | n/a | 0.002957 | 4.463e-07 | 43.91 |
| cpu-reverse-kl-b | b240f95f2359 | 32401 | 1.863e-09 | 0.0001025 | n/a | 0.002957 | 4.463e-07 | 43.78 |
| cpu-forward-kl-a | 55e4fec4a513 | 32401 | 1.863e-09 | 0.0001025 | n/a | 0.003824 | 5.533e-07 | 44.45 |
| cpu-forward-kl-b | 55e4fec4a513 | 32401 | 1.863e-09 | 0.0001025 | n/a | 0.003824 | 5.533e-07 | 40.34 |

Checks:
| check | holds |
|---|---|
| one_frozen_tensor_sha256 | yes |
| before_state_is_g115 | yes |
| one_combined_adam_step | yes |
| gauge_within_bound | yes |
| gauge_anchor_preserved | yes |
| replays_identical: cuda1/control | yes |
| replays_identical: cuda1/reverse-kl | yes |
| replays_identical: cuda1/forward-kl | yes |
| replays_identical: cpu/control | yes |
| replays_identical: cpu/reverse-kl | yes |
| replays_identical: cpu/forward-kl | yes |
| one_packet_per_arm: reverse-kl | yes |
| one_packet_per_arm: forward-kl | yes |
| cuda_envelope_within_bound | yes |
| cpu/reverse-kl_differs_from_control | yes |
| cpu/forward-kl_differs_from_control | yes |
| cuda1/reverse-kl_differs_from_control | yes |
| cuda1/forward-kl_differs_from_control | yes |
| parallel_collection_equals_serial | yes |
