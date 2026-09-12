# Learned sideboarding fixes, September 12, 2026

The expanded frozen-play BO3 path now completes with Rally, Affinity, Elves, Terror and Burn. The original Map-source, unordered-search and staged-Escape blockers are fixed in explicit rich V6 / flat V3 observations. Real reruns also exposed and fixed Initiative controller snapshots, required goaded attackers, Blood Fountain graveyard targets and Monstrous Emergence's creature/reveal cost. The original V5/V2 feature path remains pinned.

Implementation commit: `cbfa24352a862d89c68539989add2224b2f31331`, following `436c62b2` and `5ca1c733`. Worktree: `E:/mtg-kernel-learned-sideboarding-codex`, branch `codex/learned-sideboarding-integration-v1`. Changes remain local and unmerged. Main/Fable worktrees, original Stores and active science runs were untouched.

## Completed final execution

All rows below use the same clean compiled source and CPU executable. No new training was performed in this fix pass.

| Cohort | Matches | Games | Result |
| --- | ---: | ---: | --- |
| Learned five-deck cross pairings | 10 | 23 | Independent learned sideboard instances in both seats; 26 decisions, 19 with exchanges, 73 cards out and 73 in |
| Static Rally/Affinity/Terror matrix | 9 | 24 | 30 acting-seat teaching examples |
| Static Elves integration | 5 | 13 | 16 acting-seat teaching examples |
| Exact Map, Affinity/Elves and Terror/Affinity regressions | 3 | 8 | All completed, including both previously failing postboard cases |
| Original V2 Rally/Burn canary | 1 | 2 | Original match bytes unchanged |
| Total | 28 | 70 | Every selected final cohort completed |

The independent artifact audit verified all 140 game-seat configurations and 84 sideboard transitions: legal 60/15 sizes, conserved registered 75, and matching initial/selected/next-game hashes. The 46 static examples contain 346 action targets and no invented value labels.

The learned batch covers five unordered cross-deck pairings in both directions, not every possible matchup. The static examples are still fact-checked hand-authored warm starts, not search-ratified labels. Earlier failed runs and one cancelled redundant parallel matrix copy remain explicitly incomplete and are excluded from these totals.

## Verification and reproducibility

- 58 Rust library checks, five CLI checks and 21 Python checks passed. These include original Net8 forward goldens and the old V2 all-thirteen-tensor canary.
- All 18 actual native fixtures match Python V6 bitwise across all 13 tensors, totaling 24,905 scalars. Coverage includes Escape prefixes, temporary searches, Map, Initiative, goad, live/detached Blood Fountain targets, and all three Emergence choice states.
- Exact Affinity/Elves seed `2026091231` repeats bit-identically: match SHA-256 `3036852134e8af9e69bf6a4d76fe905a19e9b122fd6b2c73e52317254a3697e5`.
- Original V2 Rally/Burn seed `2026091251` still produces SHA-256 `09f71de98242cb0fe719b166bc9f6d2136b3c6b85bd1c803c868cb7d3598fb71`.

The final executable SHA-256 is `deb79be71e4a8f495b73cf99c8a01c9dc37e1acf2473d7d40e9cd66a3166813d`. Rust/Cargo are pinned to 1.94.1, LLVM 21.1.8, MSVC linker 14.50.35725.0. Every final start receipt records clean source, CPU execution and null GPU ordinal. The replay receipt and final manifest preserve actual input/output hashes, seeds, limits, historical failures and per-run source identities.

## Use and limits

Use the checked V3 `run_batch` configurations in the config root below. `play_observation_transfer_v3` must contain the current schema/encoding pins from `data/flat_policy_v3/feature_contract_v3.json`; omission intentionally selects the frozen V2 path. The transfer preserves original model weights and input widths while recording changed feature semantics.

The sideboard head remains the existing Rally/Burn imitation checkpoint `914eb1d89bbc10b446b06d0f6de82cb1eea82ba37fb57302ae8bf8d417dfaa71`. Running it on more decks demonstrates executable integration and legal exchanges, not improved win rate, generalization or a useful value model. In particular, Emergence's selected Battlefield/Hand branch and captured damage power remain incomplete in value-state observations. See the [successor design and limitations](sideboarding_observation_successor_v3.md) before any learning or strength experiment.

Brewing remains a primary end goal. This interface represents cards and configurations, but currently exchanges within an existing registered 75. New-registration search, broader training, search-generated targets and formal BO3 comparisons remain future work.

Three fresh independent Fable reviews failed HTTP 429 weekly quota before any reads or tokens. There is no Fable feedback or endorsement. The design note records the actual failures and accepted Codex review points. No GPU run, paid compute, promotion or main-tree merge occurred.

## Evidence

- Configs, `FINAL-MANIFEST.json`, `replay-check-001.json` and the independent transition audit: `C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/engineering-002/`.
- Immutable run outputs and logs: `E:/mtg-kernel-learned-sideboarding-evidence/engineering-002/`.
- Preserved executable: `E:/mtg-kernel-learned-sideboarding-evidence/engineering-002/final-build-001/learned_sideboard_v1.exe`.
- Current continuation handoff: `C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/HANDOFF.md`.
