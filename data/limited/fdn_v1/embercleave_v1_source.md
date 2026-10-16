# Embercleave unregistered native source appendix

The frozen pool selects SPG 77, Scryfall `32bee083-859e-407c-99ad-3fb74b1d4233`, Oracle `4d6120d6-fcce-40bc-9fc6-e1f5beb6c728`. This checkpoint allocates no kernel ID and changes neither card registry nor deck/mapping/admission metadata. The real Niv prerequisite must be composed before any append registration. All model and owning permissions remain unchanged.

Pinned Mage commit: `a5c90fe180021e70e2a644ade00eeab07f857a40`. The local source bytes read for this appendix are:

| Source path | SHA-256 |
| --- | --- |
| `Mage.Sets/src/mage/cards/e/Embercleave.java` | `5a5a59deb5ecfbfa7770b1d0d7ab2c2664da07eb6fe29ebb8f5e387838d56566` |
| `Mage/src/main/java/mage/abilities/dynamicvalue/common/AttackingCreatureCount.java` | `0c43039e78a6126fdd708ba577f397e2639a2f483728346c79804c2f778f73f9` |
| `Mage/src/main/java/mage/abilities/common/EntersBattlefieldAttachToTarget.java` | `e50af4bbb1c04c044c182d9d0acb58dcf41375d41a94e85a67fbcbfe39f2625b` |

Embercleave is a legendary Artifact Equipment costing `{4}{R}{R}` with flash. Its own spell costs one less generic mana per creature currently attacking under its caster's control. Its ETB trigger targets one creature controlled by the captured trigger controller and attaches the original Equipment incarnation. The equipped creature gets +1/+1, double strike and trample. Equip costs `{3}` at sorcery speed.

`DynamicCountDef::ControllerAttackingCreatures` is appended without changing any existing variant order. The shared native cost path counts the live combat attacker roster with current controller, battlefield zone and effective creature type checks. Ordinary departure/control transitions remove creatures from combat, and combat-end removal clears the roster. Generic reduction floors at zero through the existing total-cost path; both red pips remain. The distinct `CreaturesAttackedThisTurn` primitive is unchanged. Frozen disabled `rules_vector_v1` records this read with an opaque current-attacker predicate instead of identifying the two counts.

Deferred build recipes reuse the existing equipment definition, Flash keyword, `{3}` equip activation, and exact `AttachSourceToTarget` ETB operation. Native trigger dispatch requires a real catalog row named Embercleave, which is absent in this checkpoint. No new effect, continuation, DTO, journal or serialized enum variant is introduced. All production edits are in existing source identity inputs (`build.rs`, `card_def.rs`, `engine.rs`, `trigger.rs`, and the existing disabled meaning reader). No new production child file is added.

One new selected source test is `engine::embercleave_v1_tests::current_attacker_cost_count_tracks_native_declaration_departure_control_and_combat_end_both_seats`. It uses genuine native attacker declarations and control/zone transitions plus exact serde restore, and covers only the reusable count. Existing test bodies are preserved. This source case has not been executed.

Two functions in `src/embercleave_v1_tests.rs` prepare genuine paid both-seat fixtures for the later real printing. They intentionally have no `#[test]` entrypoints and fail lookup if invoked before registration. They cover 0/1/4/6 current attackers, actual Mountain payment, combat Flash, mandatory ETB target, +1/+1 and both keywords, source departure, target departure/reentry/control changes, exact waiting/published restores, paid sorcery-speed equip and grant removal, and Flash on the opponent's turn with no eligible ETB target. Basic lands/libraries/vanilla creatures are declared setup operands; response mutations are explicitly labelled event/control primitives.

This is source preparation. Rust build/check/test/clippy and native execution were not run. Formatting, diff checks and lightweight static audits are the available checks. There is no Full promotion, profile/catalog hash claim, model admission, or measured runtime claim. Real registration, enabled paid entrypoints and qualification remain work for the composed native lane.
