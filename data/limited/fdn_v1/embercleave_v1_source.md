# Embercleave Partial native source appendix

The frozen pool selects SPG 77, Scryfall `32bee083-859e-407c-99ad-3fb74b1d4233`, Oracle `4d6120d6-fcce-40bc-9fc6-e1f5beb6c728`. After composing the actual Niv-Mizzet prerequisite at ID 461, this checkpoint appends Embercleave as executable `Partial` ID 462. All earlier registry rows and all model, Human, transport and owning permissions remain unchanged. Public Full-only deck resolution continues to reject Embercleave.

Pinned Mage commit: `a5c90fe180021e70e2a644ade00eeab07f857a40`. The local source bytes read for this appendix are:

| Source path | SHA-256 |
| --- | --- |
| `Mage.Sets/src/mage/cards/e/Embercleave.java` | `5a5a59deb5ecfbfa7770b1d0d7ab2c2664da07eb6fe29ebb8f5e387838d56566` |
| `Mage/src/main/java/mage/abilities/dynamicvalue/common/AttackingCreatureCount.java` | `0c43039e78a6126fdd708ba577f397e2639a2f483728346c79804c2f778f73f9` |
| `Mage/src/main/java/mage/abilities/common/EntersBattlefieldAttachToTarget.java` | `e50af4bbb1c04c044c182d9d0acb58dcf41375d41a94e85a67fbcbfe39f2625b` |

Embercleave is a legendary Artifact Equipment costing `{4}{R}{R}` with flash. Its own spell costs one less generic mana per creature currently attacking under its caster's control. Its ETB trigger targets one creature controlled by the captured trigger controller and attaches the original Equipment incarnation. The equipped creature gets +1/+1, double strike and trample. Equip costs `{3}` at sorcery speed.

`DynamicCountDef::ControllerAttackingCreatures` is appended without changing any existing variant order. The shared native cost path counts the live combat attacker roster with current controller, battlefield zone and effective creature type checks. Ordinary departure/control transitions remove creatures from combat, and combat-end removal clears the roster. Generic reduction floors at zero through the existing total-cost path; both red pips remain. The distinct `CreaturesAttackedThisTurn` primitive is unchanged. Frozen disabled `rules_vector_v1` records this read with an opaque current-attacker predicate instead of identifying the two counts.

The registered build recipes reuse the existing equipment definition, Flash keyword, `{3}` equip activation, and exact `AttachSourceToTarget` ETB operation. Native trigger dispatch selects the real Embercleave printing. No new effect, continuation, DTO, journal or serialized enum variant is introduced by registration. All production edits remain in existing source identity inputs (`build.rs`, `card_def.rs`, `engine.rs`, `trigger.rs`, and the existing disabled meaning reader). No new production child file is added.

The existing selected source test `engine::embercleave_v1_tests::current_attacker_cost_count_tracks_native_declaration_departure_control_and_combat_end_both_seats` is preserved unchanged. It uses genuine native attacker declarations and control/zone transitions plus exact serde restore, and covers the reusable count.

The two prepared functions are now selected source tests: `engine::embercleave_v1_tests::genuine_paid_flash_attach_equip_and_restore_both_seats` and `engine::embercleave_v1_tests::genuine_paid_flash_on_opponents_turn_and_targetless_entry_both_seats`. They retain every previous assertion and cover 0/1/4/6 current attackers, actual Mountain payment, combat Flash, mandatory ETB target, +1/+1 and both keywords, source departure/reentry, target departure/reentry/control changes, exact waiting/published restores, paid sorcery-speed equip and grant removal, and Flash on the opponent's turn with no eligible ETB target. An independent negative world has six total mana but only one red, preserving the exact RR requirement. Basic lands/libraries/vanilla creatures are declared setup operands; response mutations are explicitly labelled event/control primitives.

The resulting source registry has 463 definitions and 271 FDN rows. The unchanged frozen pool has 286 printings with 245 Full, 11 Partial and 30 Missing; its 24 token dependencies are unchanged. `FDN_reference_embercleave_partial.dck` is a real forty-card source reference with four Embercleave, four Llanowar Elves and 32 basic lands. The Python metadata case checks the exact frozen identity, ID, recipe fields, real deck size and Full-only refusal.

Rust build/check/test/clippy and native execution were not run. All three Rust source cases remain unexecuted. Formatting, diff checks and lightweight Python/static audits are the available checks. There is no Full promotion, profile/catalog hash claim, model admission, or measured runtime claim. Native qualification remains work for the composed lane.

Local source checks passed: pinned `cargo +1.94.1 fmt --all -- --check`, `git diff --check`, 77 `test_limited_decks_v1.py` cases and six `test_fdn_booster_pool_v1.py` cases. A static comparison against composed prerequisite `89623154f059c280080b71ef1c831d754bece620` confirmed the exact prior 270-row registry prefix, unchanged base catalog and frozen pool/name bytes, contiguous actual Niv461/Embercleave462, and the unchanged generic count test body. These checks do not execute the Rust fixtures.
