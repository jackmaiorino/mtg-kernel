# Issue 454 current-actor terminal probe: native results

Native release runs of `current_actor_probe_v1` (see `docs/design/actor454_terminal_probe_v1.md`) at source `8093d9d49bd594f7fddf1186b43f6f5d03dc1313` on the maintainer's Windows desktop, BelowNormal, via the supported host-slot launcher. Full receipts, logs and binaries stay outside the repository; this file keeps the compact numbers and SHA-256 pins.

| | Eternal | Limited (FDN) |
| --- | --- | --- |
| Cohort / runtime | historical60 / `EternalActorV1/native-Full/environment-v2` | original_limited / `FullPoolV1/reveal-minima-v3/Foundations-combat/London/environment-v2` |
| Feature | `eternal-fixtures` | `limited-fdn-fixtures` |
| Native unit tests (issue454 + ordinal audit + regressions) | 15 + 1 + 5 passed | 7 + 1 + 1 passed |
| 4-game samples w=1/2/4 primary SHA identical | yes | yes |
| Sample native seconds w=1/2/4 | 24.2 / 14.9 / 11.5 | 2473.6 / 2023.8 / 1783.2 |
| Selected allocation | desktop-w4 | desktop-w4 |
| Games planned / attempted / completed | 60 / 60 / 60 | 60 / 60 / 56 |
| Terminals | natural:natural_game_over 60 | None:None 3, halted:fail_closed 1, natural:natural_game_over 56 |
| Stopped games (encoder or engine) | 0 | 4 |
| Decisions (sum) | 10210 | 59590 |
| Decision kinds and seats | AttackerInclusion/P0 140, AttackerInclusion/P1 170, BlockerInclusion/P0 70, BlockerInclusion/P1 30, Surface/P0 4920, Surface/P1 4880 | AttackerInclusion/P0 929, AttackerInclusion/P1 802, BlockerInclusion/P0 240, BlockerInclusion/P1 140, Surface/P0 28832, Surface/P1 28647 |
| Ordinal-neutral collisions (total / authenticated / unexplained) | 0 / 0 / 0 | 0 / 0 / 0 |
| Missing reported pair/kinds | 0 | 0 |
| Terminal panel accepted | yes | NO |
| 60-game native seconds | 112.2 (w=4) | 15654.1 (w=4) |
| Primary SHA-256 | `694ab358f83f2b7eb5ccada7ac2e83c7f39586c74d221d9f7b051358e8a40f9d` | `c024ed5714adce1a61097d54bd1d5d89ca1344530c4a30202674dd24bd698e19` |
| Binary SHA-256 | `232fccd0a746f99cd1b22aadbf290f52245646ee77bc1acaef2d8a3108391c2b` | `90e3538c1667297eb60cf71dd395051998258fb18ed6bde6654070e8d420e94f` |
| Manifest SHA-256 | `2a813309bacef09edf0b595a924cbb74c73b2944491fecbf2a0f5116f1955d7d` | `185021240c8ada58d9f9a8bcac160f896d1e5cc05631e5cde367bbb485f69ede` |

## Limited stopped games

| Game | Decks | Seed / start | Decisions | Stage | Decision kind (legal actions) | Detail |
| --- | --- | --- | ---: | --- | --- | --- |
| 23 | FDN_top_15472_WB vs FDN_top_24789_BGpW | 1592590359 / p1 | 428 | encoding | Surface (1) | `current_mixed_policy_v1:Projection("opponent hand binding requires a public name action")` |
| 34 | FDN_top_24145_RG vs FDN_top_20065_BR | 1592590370 / p0 | 610 | encoding | Surface (3) | `current_mixed_policy_v1:UnsupportedOperand("arena_id")` |
| 39 | FDN_top_28740_RG vs FDN_top_12042_WRpG | 1592590375 / p1 | 762 | encoding | BlockerInclusion (2) | `header/action arity differs` |
| 56 | FDN_top_09703_UB vs FDN_top_06330_WB | 1592590392 / p0 | 1254 | engine | none (engine halt) | `fail_closed:ravenous_v51_required:Conqueror gain and its actual downstream captures changed` |

## Notes

- The Eternal unit set includes the paid Doomsday fixture `issue454_paid_doomsday_full_selection_range_and_native_apply_both_seats` and the reported-mainboard printed-operand extraction; neither runs in any CI filter.
- The Eternal panel is accepted. The Limited panel is not: four of 60 games stopped (three at the encoder, one at an engine fail-closed guard), so #454 stays open for those stops. Every reported deck pair and decision kind was still reached and no action collision occurred.
- Serially, Limited costs about 0.55 s per decision on this composition (Eternal about 0.03 s), so the first Limited w=1 sample hit the launcher's 1800 s sample bound and was rerun with a 7200 s bound; the panel ran with a 21600 s bound. The failed receipt is retained outside the repository.
- This is a uniform legal-action diagnostic over the current numeric actor input (six explicit action flags, no digest), not a strength measure. It does not exercise the rebuilt production big-network tensorizer, which is not on main; the issue's rerun with that tensorizer before any Eternal production remains open. Action kinds per pool are in the JSON companion file.
