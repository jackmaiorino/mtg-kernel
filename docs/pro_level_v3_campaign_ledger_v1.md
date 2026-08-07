# Pro-level V3 campaign alpha ledger v1

Frozen before the first V3 formal outcome, 2026-08-04.

The campaign familywise alpha is `0.10`, split by the countersigned V3 fixed
allocation: candidate pool `0.07`, accumulation-chain pool `0.02`, reserve
`0.01`. The candidate pool has four planned attempt slots. Each slot receives
`0.0175`, split equally into an initial gate and mandatory independent
confirmation, `alpha=0.00875` each. The accumulation pool has 200 planned step
attempts and `K=5`, producing the V3 allocations `0.00004` per initial gate,
`0.00004` per confirmation, and `0.0001` per 5-step meta-gate. No gate in this
ledger reuses alpha after launch.

| gate id | pool | alpha | launched | outcome | frozen seed schedule |
|---|---:|---:|---:|---|---|
| `candidate-01-gae-initial` | candidates | 0.00875 | Y | `INCONCLUSIVE-AT-MAX-N`, `n=16384`; closed | `488b64430f2aa806dbaa2689e6bd0d14570f87ed091ca1ac4c553561d05dfa96` |
| `candidate-01-gae-confirm` | candidates | 0.00875 | N | not launched; candidate closed | `b82fa7bd4b4220bcfac60415c097448e7d992846871f1d485865dc3e12f9faaa` |
| `candidate-02-population-g1536-initial` | candidates | 0.00875 | Y | `INCONCLUSIVE-AT-MAX-N`, `n=131072`; closed | `b8609141d42f5c5230dadc95d279ff17d5869de523a8b579e3ec93c10561868c` |
| `candidate-02-population-g1536-confirm` | candidates | 0.00875 | N | not launched; candidate closed | `bd7612fc6a937b55a1f0ed9efec0042200a0effe3d3eaee822a5032de8af0100` |
| `candidate-03-initial/confirm` | candidates | 0.0175 total | N | unassigned | unassigned |
| `candidate-04-initial/confirm` | candidates | 0.0175 total | N | unassigned | unassigned |

Historical and development evaluations predating V3 consumed no slot in this
ledger. They remain selection panels and therefore cannot appear in any formal
seed schedule for a candidate they helped select.

Candidate 01 initial analysis SHA-256:
`fd6940053d9d307621465e39bf792843aaa874b26fd4a4f4abcb4a2979bd1ffb`.
Its unlaunched confirmation allocation remains unused and is not reassigned by
this ledger.

Process deviation for `candidate-01-gae-initial`: formal measurement began
before the concrete design sheet received the per-sheet countersign required
by V3 Erratum 6. The result remains accepted after Fable independently
recomputed all chunk hashes, leg scores, confidence-sequence endpoints,
stopping looks, schedule identities, and component receipts with zero
discrepancies. Candidate 02 and later formal gates require countersign before
launch.

Candidate 02 initial execution manifest SHA-256:
`7446c215961749c7d3d74f217f7baa553701b63046f480a05ee36a06c7c0df4d`.
Its full analysis SHA-256 is
`4b8ecd305f7beb98b86aa2e250e5e301c4e745148167aa91006581c6a55fd6d3`.
Independent reconstruction reproduced the retained analysis byte-for-byte
from all 64 receipts and 128 raw outcomes. The final estimate was
`0.0085296630859375` with confidence-sequence interval
`[0.0033928632690480853, 0.011348276520729561]`.

Process deviation for `candidate-02-population-g1536-initial`: its valid
prelaunch countersign came from an independent OpenAI sidecar reviewer rather
than Fable's usual review lane; Fable acknowledged the clean launch and
requested this ledger note plus independent reanalysis before any confirmation.
The initial was independently reconstructed exactly and was not `SUCCESS`, so
confirmation was not launched.

## Post-freeze development history

These single-shot development panels consume no V3 alpha. Their revealed seed
schedules are excluded from any later formal gate for a candidate they helped
select.

| development lane | outcome | formal consequence |
| --- | --- | --- |
| Current Net8 fixed GAE extension to 16 updates | stopped at `+7/1024` versus the frozen `+8` margin | exact extension closed; no candidate slot consumed |
| Current Net8 population response cycle v1 | stopped: original Pool3 `+6` vs GAE8, `+7` vs parent with P0 `-4`; pure GAE8 `+17` | exact response cycle closed; no candidate slot consumed |
| Bounded history-value search-teacher distillation v1 | stopped on residue 3: search-label NLL improved `1.8577%` versus the matched control and top-1 changed `-0.4208` points | exact teacher and fixed learner closed; no candidate slot consumed |
| Current Net8 CP7 terminal response v4 | Pool3 noninferiority passed at `-8`; CP7 skill-7 gate narrowly failed at terminal net `-1` and win margin `-1` | exact response fine-tune closed; no candidate slot consumed; skill-8 seed `1850001` untouched |

Population response report SHA-256:
`717c80fe855b9903baef19570119992fc43634d5d0809a69c38ded90c64ced6e`.
The next mechanism under design is the regularized continuation retest. It is
development work unless a fixed resulting policy receives a separate V3
candidate sheet, one of the three remaining candidate slots, and a countersign
before formal launch.
