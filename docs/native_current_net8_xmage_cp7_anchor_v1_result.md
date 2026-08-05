# Current Net8 XMage CP7 anchor v1 result

Status: COMPLETE, RETAIN GAE8.

## Result

The fresh common-root benchmark completed all 64 planned pairs with no
exclusions or task failures. Each model played 128 natural games against XMage
CP7 skill 7.

| Arm | Wins | Draws | Losses | Win rate |
| --- | ---: | ---: | ---: | ---: |
| GAE8 | 48 | 0 | 80 | 37.5% |
| GAE16 | 48 | 0 | 80 | 37.5% |

GAE16 was better on 1 matched leg, GAE8 was better on 1, and 126 tied. The
GAE16 net was 0 against the required +4. Its seat nets were -1 as candidate P0
and +1 as candidate P1, both inside the safety floor but with no aggregate
gain. The frozen ranking gate failed and GAE8 remains the current external
anchor candidate.

By candidate seat:

| Arm | P0 W-L | P1 W-L |
| --- | ---: | ---: |
| GAE8 | 29-35 | 19-45 |
| GAE16 | 28-36 | 20-44 |

## Bridge and throughput verification

The exact GAE8 and GAE16 raw train states loaded through two-file packages
whose payload, optimizer, model, and semantic state hashes were checked by the
Rust scorer. The Java bridge preserved the original G384 source lineage while
requiring the loaded model identity and the truthful
`environment-randomization-v2` trajectory contract.

The revealed one-pair bridge check completed twice. Each model's full outcome
stream was byte-identical across repeats. The four-pair topology screen then
produced byte-identical outcome streams at one, two, and four workers with no
failures:

| Workers | Wall seconds | Games/s | Projected 256-game wall |
| ---: | ---: | ---: | ---: |
| 1 | 235.32 | 0.0680 | 62.8 min |
| 2 | 126.81 | 0.1262 | 33.8 min |
| 4 | 73.45 | 0.2178 | 19.6 min |

Four workers were selected. The formal run completed in 989.00 seconds at
0.2588 games/s. Average host CPU was 25.43 percent, peak process-tree RSS was
9.34 GiB, and GPU 1 remained at 0 percent utilization with 9 MiB used. This is
a CPU-bound XMage workload.

## Evidence

- Formal manifest:
  `D:\mtg-kernel-current-net8-xmage-cp7-anchor-v1\formal-base1820001-attempt-01\manifest.json`
  SHA-256 `6fe17497dc95c52696c5baecf321327255afa290a451b2afd4d5efefe245d3a5`.
- Formal report:
  `D:\mtg-kernel-current-net8-xmage-cp7-anchor-v1\formal-base1820001-attempt-01\report.json`
  SHA-256 `d1401336f7288a4e358dccc6a3f41d095da118fe54d5390eadc539aeefc7bfc9`.
- Formal state:
  `D:\mtg-kernel-current-net8-xmage-cp7-anchor-v1\formal-base1820001-attempt-01\state.json`
  SHA-256 `fd1236ef2f86cfbfca8c901ed15c10cfddaf5d63bcd6786f2485b14731e9adca`.
- Kernel commit `c8b81ddaeebff68d35194d42755e6d7933f298bb`.
- Mage commit `dc6921b25e3b19d6287775ca888a83b174358d5c`.
- Scorer SHA-256
  `db8daeda796ef49d8c50af5bc743ba46f6e273f4d65fb4547466ded0f7211018`.

## Interpretation

The fixed GAE16 extension did not improve external play. This corroborates its
earlier development stop and closes further investment in that exact extension.

The new baseline also changes project prioritization. A 37.5 percent win rate
against CP7 skill 7 shows that the current Net8 policy has substantial room to
improve even in the narrow Rally mirror. The next high-value lane is direct
terminal learning from on-policy XMage CP7 games, using the exported Net8
decision tensors and natural terminal rewards, followed by another fresh
common-root CP7 gate. This targets the observed deficit directly and avoids
using search scores or imitation labels as rewards.

## Nonclaims

- CP7 skill 7 is not a professional player.
- This result is not professional-level evidence.
- It does not measure cross-deck play, hidden-information beliefs,
  sideboarding, or human match strength.
- It does not consume V3 alpha or a candidate slot.
- Terminal win, draw, or loss is the only playing-strength outcome.
