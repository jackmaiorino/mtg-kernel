# chain-001 (line-b-engineering-chain-receipt/v1)

| chain | updates | final state | update seconds | collect seconds |
|---|---|---|---|---|
| control-a | 4 | 3763609da538 | 91.99 | 4.765 |
| control-b | 4 | 3763609da538 | 71.26 | 13.73 |
| reverse-kl-a | 4 | 0cf3207a0973 | 190.8 | 4.081 |
| reverse-kl-b | 4 | 0cf3207a0973 | 208.3 | 4.266 |
| forward-kl-a | 4 | 073ab031f0c9 | 163.4 | 3.951 |
| forward-kl-b | 4 | 073ab031f0c9 | 142.5 | 3.917 |

| check | holds |
|---|---|
| control: parallel against its serial replay, identical at every update | yes |
| control: update 0 trajectory files byte-identical | yes |
| forward-kl: two replays, identical at every update | yes |
| forward-kl: update 0 trajectory files byte-identical | yes |
| reverse-kl: two replays, identical at every update | yes |
| reverse-kl: update 0 trajectory files byte-identical | yes |
| frozen_tensor_sha256_constant | yes |
| gauge_within_bound | yes |
