# Pauper Meta Wave 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Register nine revisioned Pauper decks (current mtgtop8 lists) in the kernel pool, implement the 21 cards they need plus one back face and one token, and prove each card against XMage, without changing the active runtime deck set.

**Architecture:** The pool catalog (`data/pauper_pool_v1.json`, registered decks for sideboarding and Bo3) grows from nine to eighteen registrations while the runtime catalog (`data/runtime_decks_v1.json`, the nine decks training and RL sample from) stays byte-identical. Cards append to `data/cards_v1.json` (ids are array indexes, so nothing moves). Card programs follow the existing build.rs tables (`special_for`, `effect_recipe_for`, `trigger_recipe_for`, cost tables) plus a small number of new card-neutral primitives (transform in place, granted equipment ability, mass pump, delve, monarch, adventure). Every wave commit records old and new `KERNEL_CARDDB_HASH`.

**Tech Stack:** Rust 2021 (mtg-kernel crate, build.rs codegen), Python 3 tools under `python/tools` run through `uvx uv@0.11.29 run --no-sync python ...`, XMage Java sources in the Mage fork `C:\Users\Jack\IdeaProjects\mage-cycle4-lead` (pinned 72a08a3b) as the rules oracle.

**Spec:** `docs/superpowers/specs/2026-09-09-pauper-meta-cards-and-sideboarding-design.md` (revision 3, Codex-approved 2026-09-09). Research inputs: `docs/research/pauper_meta_source_lists_2026-09-09.json` and the preserved decklists under `docs/research/pauper_meta_decklists_2026-09-09/`.

## Global Constraints

- Branch `lead/pauper-meta-cards-v1` only; never commit to `lead/cycle4-refresh-manifest-v1` or `main`. Push after every commit.
- Registry identity is append-only: never delete or reorder entries in `data/cards_v1.json`.
- `data/runtime_decks_v1.json` must not change in this wave (its SHA-256 `68e7602f3a4df6217119406973954630800c358a10fca9f28e6cf9f20fd3b851` is pinned by the training store). Verify with `sha256sum data/runtime_decks_v1.json` before every commit that touches `data/`.
- Historical deck registrations (Wildfire, Rally, Affinity, Elves, Spy, Burn, Terror, CawGates, Faeries) and their `.dek` files never change.
- No card-specific pending state (ROADMAP line 175). New primitives are card-neutral `EffectOp`, `CostComponent`, trigger, or definition shapes with their own unit tests.
- Every new card's integration test file header cites the XMage Java file and its git blob SHA from the Mage fork at 72a08a3b (`git -C C:\Users\Jack\IdeaProjects\mage-cycle4-lead rev-parse 72a08a3b:Mage.Sets/src/mage/cards/<letter>/<Class>.java`).
- A card's `engine_capability` may be `full` in the candidate build so manifests generate, but the wave is not certified until Task 14's branch-coverage check passes (spec 5.3 steps 2 and 4).
- No em-dashes in any file, comment, or commit message.
- Python entry point: `uvx uv@0.11.29 run --no-sync python python/tools/<tool>.py` from the worktree root. Rust: `cargo test --locked -p mtg-kernel` from the worktree root (target dir `D:/cargo-target-pauper-meta` via `CARGO_TARGET_DIR`).
- Commit message bodies for registry or deck changes list: old and new `KERNEL_CARDDB_HASH`, pool totals, and each new deck's source SHA-256.

---

## Wave 1 registrations (pinned source 75s)

Registration id, source decklist (mtgtop8 id, preserved file), substitutions (spec 5.1), and new cards. Ids are identifiers without underscores (the pool validator accepts `CawGates`, so CamelCase is the convention).

| id | source | file under docs/research/pauper_meta_decklists_2026-09-09 | substitutions | new cards |
|---|---|---|---|---|
| BurnV2 | 888077 | Burn__888077.txt | none | Kessig Flamebreather (main x4), Smash to Smithereens (side x2) |
| DelverV2 | 887942 | MonoBlueAggro__887942.txt | none | Delver of Secrets (main x4), back face Insectile Aberration |
| AffinityV2 | 887998 | Affinity__887998.txt | Utrom Monitor x3 to Glint Hawk x3; Sewer-veillance Cam x1 to Cryogen Relic x1 (XMage-absent cards; next-most-common sampled cards) | Glint Hawk |
| RallyV2 | 888081 | RedDeckWins__888081.txt | none | Raze (side x3), Smash to Smithereens (side x2) |
| WildfireV2 | 887774 | Jund__887774.txt | none | Gixian Infiltrator (main x2), Ancient Grudge (side x1), Terminate (side x1) |
| ElvesV2 | 887257 | Elves__887257.txt | none | Viridian Longbow (side x2), Webweaver Changeling (side x2) |
| TerrorV2 | 887001 | Terror__887001.txt | none | Artful Dodge (main x1), Azure Fleet Admiral (side x1) |
| SpyV2 | 887010 | Spy__887010.txt | none | Acorn Harvest (side x1), Fang Dragon (side x3), Squirrel Token |
| DimirTerrorV2 | 886841 | DimirControl__886841.txt | none | Abandon Attachments x2, Contaminated Aquifer x4, Gurmag Angler x2, Ice Tunnel x2, Snuff Out x4, Suffocating Fumes x1 (all main), Arms of Hadar (side x2) |

CawGates is not revisioned in this wave: the closest sampled Gates list without XMage-absent cards (885724) adds no card, and the alternative (885394) needs Call Damage Control (XMage-absent).

## Card metadata (from the Mage fork at 72a08a3b)

Each entry is the exact JSON object to append to `data/cards_v1.json` `cards` (the `decks` list is rewritten by the manifest generator; give the initial value shown). `java_file` paths are relative to the Mage fork.

```json
{"name": "Kessig Flamebreather", "engine_capability": "full", "mana_cost": "{1}{R}", "mana_value": 2, "colors": ["R"], "types": ["Creature"], "subtypes": ["Human", "Shaman"], "supertypes": [], "power": 1, "toughness": 3, "is_land": false, "produces_mana": [], "decks": ["Deck - Madness Burn V2.dek"], "sideboard_only": false, "complexity": "simple", "mechanics": ["spell_cast_trigger", "burn"], "java_file": "Mage.Sets/src/mage/cards/k/KessigFlamebreather.java"}
{"name": "Smash to Smithereens", "engine_capability": "full", "mana_cost": "{1}{R}", "mana_value": 2, "colors": ["R"], "types": ["Instant"], "subtypes": [], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": [], "decks": ["Deck - Madness Burn V2.dek", "Deck - Red Deck Wins V2.dek"], "sideboard_only": true, "complexity": "simple", "mechanics": ["artifact_hate", "destroy_target", "damage"], "java_file": "Mage.Sets/src/mage/cards/s/SmashToSmithereens.java"}
{"name": "Delver of Secrets", "engine_capability": "full", "mana_cost": "{U}", "mana_value": 1, "colors": ["U"], "types": ["Creature"], "subtypes": ["Human", "Wizard"], "supertypes": [], "power": 1, "toughness": 1, "is_land": false, "produces_mana": [], "decks": ["Deck - Mono-Blue Delver V2.dek"], "sideboard_only": false, "complexity": "moderate", "mechanics": ["transform", "reveal", "upkeep_trigger"], "java_file": "Mage.Sets/src/mage/cards/d/DelverOfSecrets.java"}
{"name": "Glint Hawk", "engine_capability": "full", "mana_cost": "{W}", "mana_value": 1, "colors": ["W"], "types": ["Creature"], "subtypes": ["Bird"], "supertypes": [], "power": 2, "toughness": 2, "is_land": false, "produces_mana": [], "decks": ["Deck - Grixis Affinity V2.dek"], "sideboard_only": false, "complexity": "moderate", "mechanics": ["flying", "etb_trigger", "return_to_hand", "sacrifice_self_cost"], "java_file": "Mage.Sets/src/mage/cards/g/GlintHawk.java"}
{"name": "Raze", "engine_capability": "full", "mana_cost": "{R}", "mana_value": 1, "colors": ["R"], "types": ["Sorcery"], "subtypes": [], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": [], "decks": ["Deck - Red Deck Wins V2.dek"], "sideboard_only": true, "complexity": "simple", "mechanics": ["sacrifice_cost", "destroy_target"], "java_file": "Mage.Sets/src/mage/cards/r/Raze.java"}
{"name": "Gixian Infiltrator", "engine_capability": "full", "mana_cost": "{1}{B}", "mana_value": 2, "colors": ["B"], "types": ["Creature"], "subtypes": ["Phyrexian", "Human"], "supertypes": [], "power": 2, "toughness": 1, "is_land": false, "produces_mana": [], "decks": ["Deck - Jund Wildfire V2.dek"], "sideboard_only": false, "complexity": "simple", "mechanics": ["sacrifice_trigger", "add_counters"], "java_file": "Mage.Sets/src/mage/cards/g/GixianInfiltrator.java"}
{"name": "Ancient Grudge", "engine_capability": "full", "mana_cost": "{1}{R}", "mana_value": 2, "colors": ["R"], "types": ["Instant"], "subtypes": [], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": [], "decks": ["Deck - Jund Wildfire V2.dek"], "sideboard_only": true, "complexity": "simple", "mechanics": ["artifact_hate", "destroy_target", "flashback"], "java_file": "Mage.Sets/src/mage/cards/a/AncientGrudge.java"}
{"name": "Terminate", "engine_capability": "full", "mana_cost": "{B}{R}", "mana_value": 2, "colors": ["B", "R"], "types": ["Instant"], "subtypes": [], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": [], "decks": ["Deck - Jund Wildfire V2.dek"], "sideboard_only": true, "complexity": "simple", "mechanics": ["removal", "destroy_target"], "java_file": "Mage.Sets/src/mage/cards/t/Terminate.java"}
{"name": "Viridian Longbow", "engine_capability": "full", "mana_cost": "{1}", "mana_value": 1, "colors": [], "types": ["Artifact"], "subtypes": ["Equipment"], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": [], "decks": ["Deck - Elves V2.dek"], "sideboard_only": true, "complexity": "moderate", "mechanics": ["equipment", "tap_ability", "damage"], "java_file": "Mage.Sets/src/mage/cards/v/ViridianLongbow.java"}
{"name": "Webweaver Changeling", "engine_capability": "full", "mana_cost": "{3}{G}{G}", "mana_value": 5, "colors": ["G"], "types": ["Creature"], "subtypes": ["Shapeshifter"], "supertypes": [], "power": 3, "toughness": 5, "is_land": false, "produces_mana": [], "decks": ["Deck - Elves V2.dek"], "sideboard_only": true, "complexity": "moderate", "mechanics": ["changeling", "reach", "etb_trigger", "conditional_effect", "graveyard_count", "life_gain"], "java_file": "Mage.Sets/src/mage/cards/w/WebweaverChangeling.java"}
{"name": "Artful Dodge", "engine_capability": "full", "mana_cost": "{U}", "mana_value": 1, "colors": ["U"], "types": ["Sorcery"], "subtypes": [], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": [], "decks": ["Deck - Mono-Blue Terror V2.dek"], "sideboard_only": false, "complexity": "simple", "mechanics": ["keyword_grant", "until_end_of_turn", "flashback"], "java_file": "Mage.Sets/src/mage/cards/a/ArtfulDodge.java"}
{"name": "Azure Fleet Admiral", "engine_capability": "full", "mana_cost": "{3}{U}", "mana_value": 4, "colors": ["U"], "types": ["Creature"], "subtypes": ["Human", "Pirate"], "supertypes": [], "power": 3, "toughness": 3, "is_land": false, "produces_mana": [], "decks": ["Deck - Mono-Blue Terror V2.dek"], "sideboard_only": true, "complexity": "complex", "mechanics": ["monarch", "etb_trigger", "blocking_filter"], "java_file": "Mage.Sets/src/mage/cards/a/AzureFleetAdmiral.java"}
{"name": "Acorn Harvest", "engine_capability": "full", "mana_cost": "{3}{G}", "mana_value": 4, "colors": ["G"], "types": ["Sorcery"], "subtypes": [], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": [], "decks": ["Deck - Spy Combo V2.dek"], "sideboard_only": true, "complexity": "simple", "mechanics": ["token_creation", "flashback", "flashback_cost"], "java_file": "Mage.Sets/src/mage/cards/a/AcornHarvest.java"}
{"name": "Squirrel Token", "engine_capability": "full", "mana_cost": "", "mana_value": 0, "colors": ["G"], "types": ["Creature"], "subtypes": ["Squirrel"], "supertypes": [], "power": 1, "toughness": 1, "is_land": false, "produces_mana": [], "decks": [], "sideboard_only": false, "complexity": "trivial", "mechanics": [], "is_token": true, "java_file": "Mage/src/main/java/mage/game/permanent/token/SquirrelToken.java"}
{"name": "Fang Dragon", "engine_capability": "full", "mana_cost": "{5}{R}{R}", "mana_value": 7, "colors": ["R"], "types": ["Creature"], "subtypes": ["Dragon"], "supertypes": [], "power": 6, "toughness": 3, "is_land": false, "produces_mana": [], "decks": ["Deck - Spy Combo V2.dek"], "sideboard_only": true, "complexity": "complex", "mechanics": ["flying", "adventure", "damage"], "java_file": "Mage.Sets/src/mage/cards/f/FangDragon.java"}
{"name": "Abandon Attachments", "engine_capability": "full", "mana_cost": "{1}{U/R}", "mana_value": 2, "colors": ["U", "R"], "types": ["Instant"], "subtypes": ["Lesson"], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": [], "decks": ["Deck - Dimir Terror V2.dek"], "sideboard_only": false, "complexity": "simple", "mechanics": ["discard", "draw_card", "conditional_effect"], "java_file": "Mage.Sets/src/mage/cards/a/AbandonAttachments.java"}
{"name": "Contaminated Aquifer", "engine_capability": "full", "mana_cost": "", "mana_value": 0, "colors": [], "types": ["Land"], "subtypes": ["Island", "Swamp"], "supertypes": [], "power": null, "toughness": null, "is_land": true, "produces_mana": ["U", "B"], "decks": ["Deck - Dimir Terror V2.dek"], "sideboard_only": false, "complexity": "simple", "mechanics": ["dual_land", "enters_tapped", "mana_ability"], "java_file": "Mage.Sets/src/mage/cards/c/ContaminatedAquifer.java"}
{"name": "Gurmag Angler", "engine_capability": "full", "mana_cost": "{6}{B}", "mana_value": 7, "colors": ["B"], "types": ["Creature"], "subtypes": ["Zombie", "Fish"], "supertypes": [], "power": 5, "toughness": 5, "is_land": false, "produces_mana": [], "decks": ["Deck - Dimir Terror V2.dek"], "sideboard_only": false, "complexity": "complex", "mechanics": ["delve"], "java_file": "Mage.Sets/src/mage/cards/g/GurmagAngler.java"}
{"name": "Ice Tunnel", "engine_capability": "full", "mana_cost": "", "mana_value": 0, "colors": [], "types": ["Land"], "subtypes": ["Island", "Swamp"], "supertypes": ["Snow"], "power": null, "toughness": null, "is_land": true, "produces_mana": ["U", "B"], "decks": ["Deck - Dimir Terror V2.dek"], "sideboard_only": false, "complexity": "simple", "mechanics": ["dual_land", "enters_tapped", "mana_ability"], "java_file": "Mage.Sets/src/mage/cards/i/IceTunnel.java"}
{"name": "Snuff Out", "engine_capability": "full", "mana_cost": "{3}{B}", "mana_value": 4, "colors": ["B"], "types": ["Instant"], "subtypes": [], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": [], "decks": ["Deck - Dimir Terror V2.dek"], "sideboard_only": false, "complexity": "moderate", "mechanics": ["removal", "destroy_target", "alt_cost"], "java_file": "Mage.Sets/src/mage/cards/s/SnuffOut.java"}
{"name": "Suffocating Fumes", "engine_capability": "full", "mana_cost": "{2}{B}", "mana_value": 3, "colors": ["B"], "types": ["Instant"], "subtypes": [], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": [], "decks": ["Deck - Dimir Terror V2.dek"], "sideboard_only": false, "complexity": "simple", "mechanics": ["static_pump", "until_end_of_turn", "cycling"], "java_file": "Mage.Sets/src/mage/cards/s/SuffocatingFumes.java"}
{"name": "Arms of Hadar", "engine_capability": "full", "mana_cost": "{3}{B}", "mana_value": 4, "colors": ["B"], "types": ["Sorcery"], "subtypes": [], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": [], "decks": ["Deck - Dimir Terror V2.dek"], "sideboard_only": true, "complexity": "simple", "mechanics": ["target_player", "static_pump", "until_end_of_turn"], "java_file": "Mage.Sets/src/mage/cards/a/ArmsOfHadar.java"}
```

Rules text used by the tests (verified in the Java at 72a08a3b):
- Kessig Flamebreather: whenever you cast a noncreature spell, it deals 1 damage to each opponent.
- Smash to Smithereens: destroy target artifact; it deals 3 damage to that artifact's controller (last known information if the artifact is gone).
- Delver of Secrets: at the beginning of your upkeep, look at the top card of your library; you may reveal it; if an instant or sorcery card is revealed, transform. Insectile Aberration: 3/2 Human Insect, blue, flying.
- Glint Hawk: flying; when it enters, sacrifice it unless you return an artifact you control to its owner's hand.
- Raze: additional cost sacrifice a land; destroy target land.
- Gixian Infiltrator: whenever you sacrifice another permanent, put a +1/+1 counter on it.
- Ancient Grudge: destroy target artifact; flashback {G}.
- Terminate: destroy target creature; it can't be regenerated (the kernel has no regeneration, so plain destroy).
- Viridian Longbow: equipped creature has "{T}: this creature deals 1 damage to any target"; equip {3}.
- Webweaver Changeling: changeling, reach; when it enters, if there are three or more creature cards in your graveyard, you gain 5 life (intervening if: checked on trigger and on resolution).
- Artful Dodge: target creature can't be blocked this turn; flashback {U}.
- Azure Fleet Admiral: when it enters, you become the monarch; it can't be blocked by creatures the monarch controls.
- Acorn Harvest: create two 1/1 green Squirrel tokens; flashback {1}{G} plus pay 3 life.
- Fang Dragon: 6/3 flying Dragon; adventure Forktail Sweep {1}{R} sorcery: 1 damage to each creature you don't control.
- Abandon Attachments: you may discard a card; if you do, draw two cards. Hybrid {U/R} pip.
- Contaminated Aquifer: Land, Island Swamp, enters tapped. Ice Tunnel: the same with the Snow supertype.
- Gurmag Angler: delve; 5/5.
- Snuff Out: if you control a Swamp, you may pay 4 life rather than pay the mana cost; destroy target nonblack creature.
- Suffocating Fumes: creatures your opponents control get -1/-1 until end of turn; cycling {2}.
- Arms of Hadar: creatures target player controls get -2/-2 until end of turn.

---

### Task 1: Pool catalog and active set are distinct

**Files:**
- Modify: `mtg-kernel/src/sideboard.rs:252-256` (remove the literal nine), `:867` (message)
- Modify: `mtg-kernel/tests/sideboard_v1.rs:121,131,385-388`
- Modify: `python/tools/generate_pauper_manifests.py:38-39,78-114,248-286`
- Modify: `python/tests/test_pauper_pool_manifest.py:22-32,163,215-247,458-461`
- Modify: `oracle/xmage/DeterminizationSampler.java` (add `pauperRegistrationsV2()` after `pauperDefaults()`; `pauperDefaults()` untouched)
- Modify: `C:\Users\Jack\IdeaProjects\mage-cycle4-lead\Mage.Server.Plugins\Mage.Player.AIRL\src\mage\player\ai\rl\DeterminizationSampler.java` (same method, committed on a new Mage branch `lead/pauper-meta-registrations-v1` from 72a08a3b)

**Interfaces:**
- Consumes: nothing new.
- Produces: `generate_pauper_manifests.py` constants `REGISTRATION_SPECS: tuple[DeckSpec, ...]` (all registrations, protocol order) and `RUNTIME_DECK_IDS` (unchanged nine); Java `pauperRegistrationsV2()` whose `paths.put` order equals `REGISTRATION_SPECS`; `JAVA_FACTORY_REGISTRATIONS_METHOD_SHA256` pin; pool document with `decks.len() == len(REGISTRATION_SPECS)`.

- [ ] **Step 1: Write the failing Rust test** in `mtg-kernel/tests/sideboard_v1.rs`, replacing the nine-deck assertions:

```rust
#[test]
fn checked_in_pool_and_policy_cover_the_same_registration_set() {
    let decks = mtg_kernel::sideboard::checked_in_pauper_registered_decks_v1().expect("pool");
    let policy = mtg_kernel::sideboard::DeterministicSideboardPolicyV1::checked_in_pauper_v1().expect("policy");
    let mut pool_ids: Vec<&str> = decks.iter().map(|deck| deck.deck_id()).collect();
    pool_ids.sort_unstable();
    let mut policy_ids: Vec<&str> = policy.deck_ids().iter().map(String::as_str).collect();
    policy_ids.sort_unstable();
    assert_eq!(pool_ids, policy_ids);
    assert!(pool_ids.len() >= 9, "the historical nine never leave the pool");
    for historical in ["Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "Terror", "CawGates", "Faeries"] {
        assert!(pool_ids.contains(&historical), "{historical} missing");
    }
}
```
(Use the real accessor names: `grep -n "pub fn checked_in_pauper" mtg-kernel/src/sideboard.rs` and `grep -n "pub fn deck_id\|pub fn deck_ids" mtg-kernel/src/sideboard.rs`; if the deck-list accessor is named differently, use that name and do not add a new one.)

- [ ] **Step 2: Run it**: `cargo test --locked -p mtg-kernel --test sideboard_v1 checked_in_pool_and_policy_cover_the_same_registration_set`. Expected: PASS already (nine equals nine). Then change `sideboard.rs:252` so the count gate becomes `if document.decks.is_empty()` with a new error variant `SideboardErrorV1::EmptyPool` (add to the enum near `PoolDeckCount`, keep `PoolDeckCount` for compatibility, Display text "Pauper pool must contain at least one deck") and delete the `assert_eq!(decks.len(), 9)` and nine-name array at `tests/sideboard_v1.rs:121,131` and the `covers_nine_decks` test at `:385-388` (its body is replaced by Step 1's test).

- [ ] **Step 3: Run the sideboard suite**: `cargo test --locked -p mtg-kernel --test sideboard_v1` and `cargo test --locked -p mtg-kernel --lib sideboard`. Expected: PASS.

- [ ] **Step 4: Python generator: split registrations from the runtime set.** In `generate_pauper_manifests.py`:
  - Rename `DECK_SPECS` to `REGISTRATION_SPECS` (keep `DECK_SPECS = REGISTRATION_SPECS` as an alias for one release so callers do not break); every loop that iterates `DECK_SPECS` for the pool document and support manifest now iterates `REGISTRATION_SPECS`.
  - Add `JAVA_FACTORY_REGISTRATIONS_METHOD = "DeterminizationSampler.pauperRegistrationsV2"` and `JAVA_FACTORY_REGISTRATIONS_METHOD_SHA256 = ""` (filled in Step 6).
  - In `_validate_java_factory` (lines 248-286): keep the existing check that `pauperDefaults()`'s `paths.put` list equals the specs filtered to `RUNTIME_DECK_IDS` (the active set), and add a second extraction for `pauperRegistrationsV2()` (same signature-to-`return loadArchetypes(paths);` slicing) whose `paths.put` list must equal `REGISTRATION_SPECS` in order and whose SHA-256 must equal `JAVA_FACTORY_REGISTRATIONS_METHOD_SHA256`.
  - Write both method SHAs and the method names into the pool document `source` block (`java_factory_registrations_method`, `java_factory_registrations_method_sha256`); `build.rs` does not read these keys (it reads only `runtime_decks_v1.json`), and `sideboard.rs` deserializes `PauperPoolDocumentV1` with `serde(deny_unknown_fields)`? Check with `grep -n "deny_unknown_fields" -B2 -A8 mtg-kernel/src/sideboard.rs | grep -A8 PauperPoolDocument`; if the source block is typed, add the two optional fields as `Option<String>` with `#[serde(default)]`.

- [ ] **Step 5: Java method.** Append to the vendored `oracle/xmage/DeterminizationSampler.java` directly after `pauperDefaults()`:

```java
    /**
     * Every registered Pauper deck (historical nine plus revisioned
     * registrations). Sampling for training keeps using pauperDefaults();
     * this roster exists for the kernel pool manifest and sideboard policy.
     */
    public static DeterminizationSampler pauperRegistrationsV2() {
        String base = "Mage.Server.Plugins/Mage.Player.AIRL/src/mage/player/ai/decks/Pauper";
        java.util.LinkedHashMap<String, String> paths = new java.util.LinkedHashMap<>();
        paths.put("Wildfire", base + "/Deck - Jund Wildfire.dek");
        paths.put("Rally", base + "/Deck - Mono Red Rally.dek");
        paths.put("Affinity", base + "/Deck - Grixis Affinity.dek");
        paths.put("Elves", base + "/Deck - Elves.dek");
        paths.put("SpyCombo", base + "/Deck - Spy Combo.dek");
        paths.put("Burn", base + "/Deck - Mono-Red Burn.dek");
        paths.put("Terror", base + "/Deck - Mono-Blue Terror.dek");
        paths.put("CawGates", base + "/Deck - Caw-Gates.dek");
        paths.put("Faeries", base + "/Deck - Mono-Blue Faeries.dek");
        return loadArchetypes(paths);
    }
```
(Copy the exact `paths` declaration style from `pauperDefaults()` at line 163 so the regex in the generator matches; new registrations are appended to this method in Task 12.) Apply the identical edit to the Mage fork file, commit it there on branch `lead/pauper-meta-registrations-v1`, and push.

- [ ] **Step 6: Re-pin and regenerate.** Compute the file SHA and both method SHAs with the generator's own normalization by running it in check mode and copying the "expected ... got ..." values it prints into `JAVA_FACTORY_FILE_SHA256`, `JAVA_FACTORY_METHOD_SHA256` (unchanged, `pauperDefaults` did not move) and `JAVA_FACTORY_REGISTRATIONS_METHOD_SHA256`. Then run:
```
uvx uv@0.11.29 run --no-sync python python/tools/generate_pauper_manifests.py --write
sha256sum data/runtime_decks_v1.json
```
Expected: `runtime_decks_v1.json` still hashes to `68e7602f...b851`; `pauper_pool_v1.json` `source` carries the new keys; nothing else changes.

- [ ] **Step 7: Python tests.** In `test_pauper_pool_manifest.py` replace the hardcoded nine-tuple `EXPECTED_SPECS` with an import of `REGISTRATION_SPECS`, keep `RUNTIME_DECK_IDS` assertions literal (the active nine), change the `deck_count` expectation to `len(REGISTRATION_SPECS)`, and add:
```python
def test_registrations_are_a_superset_of_the_runtime_set(self):
    registered = [spec.deck_id for spec in manifests.REGISTRATION_SPECS]
    self.assertEqual(registered[:9], list(manifests.RUNTIME_DECK_IDS))
    self.assertEqual(self.pool["source"]["java_factory_registrations_method"], "DeterminizationSampler.pauperRegistrationsV2")
```
Run: `uvx uv@0.11.29 run --no-sync python -m unittest python.tests.test_pauper_pool_manifest -v`. Expected: PASS.

- [ ] **Step 8: Full check and commit.**
```
cargo test --locked -p mtg-kernel
uvx uv@0.11.29 run --no-sync python -m unittest discover -s python/tests -v
git add -A && git commit -m "pool: separate deck registrations from the active runtime set; pauperRegistrationsV2 roster pinned"
git push
```

---

### Task 2: Card DB identity re-pin procedure

**Files:**
- Create: `python/tools/repin_card_db_identity_v1.py`
- Test: `python/tests/test_repin_card_db_identity_v1.py`

**Interfaces:**
- Produces: `repin_card_db_identity_v1.py --check` (exit 1 and list every stale pin) and `--write` (rewrite the pins). Pin sites (verified 2026-09-09; the tool greps for the old literal and fails if a site is missing):
  - `mtg-kernel/src/card_def.rs` test literal `0x64c8_2a26_1e07_8f1a`
  - `mtg-kernel/src/native_training_store_run_v2.rs` `FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1` (handled by Task 3, the tool only reports it)
  - `mtg-kernel/src/runtime_decks.rs`, `mtg-kernel/src/kernel_native_search_calibration_runner_v1.rs`, `mtg-kernel/src/native_full_episode_trajectory_v2.rs`, `mtg-kernel/src/native_full_episode_trajectory_v2_goldens.rs`
  - `mtg-kernel/tests/caw_gates_completion_v1.rs`, `caw_gates_future_v1.rs`, `faeries_future_v1.rs`, `final_pool_completion_v1.rs`, `wildfire_utility_future_v1.rs`
  - `python/tests/test_flat_policy_v2_goldens.py`, `python/tools/generate_environment_randomization_v2_reset_physical_trajectory_goldens_v1.py`, `python/tools/generate_native_full_episode_trajectory_v2_goldens.py`

- [ ] **Step 1: Write the failing test** `python/tests/test_repin_card_db_identity_v1.py`:
```python
import subprocess, sys, unittest, pathlib
ROOT = pathlib.Path(__file__).resolve().parents[2]

class RepinTool(unittest.TestCase):
    def test_check_mode_passes_on_a_consistent_tree(self):
        result = subprocess.run([sys.executable, "python/tools/repin_card_db_identity_v1.py", "--check"], cwd=ROOT, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("card_db_hash", result.stdout)
```
Run: `uvx uv@0.11.29 run --no-sync python -m unittest python.tests.test_repin_card_db_identity_v1`. Expected: FAIL (tool missing).

- [ ] **Step 2: Write the tool.** It computes the live hash exactly as `build.rs` does: read `build.rs`'s canonicalization (function around line 6716, `let hash = fnv1a64(canon.as_bytes())`; the canon string is built by the function that precedes it) and reimplement it in Python, or, simpler and exact, parse the hash out of the generated code: run `cargo build --locked -p mtg-kernel` and read `pub const KERNEL_CARDDB_HASH: u64 = 0x...;` from `$CARGO_TARGET_DIR/debug/build/mtg-kernel-*/out/*.rs` (glob for the line). Use the generated-code route. Then for every site in the list above, find the previous hash literal in both spellings (`0x64c8_2a26_1e07_8f1a` and `64c82a261e078f1a`) and replace with the new one in the same spelling. `--check` prints `card_db_hash <new>` and every stale site, exit 1 if any. `--write` rewrites and prints the sites changed. The training-store constant `FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1` is reported but never rewritten (Task 3 owns it).

- [ ] **Step 3: Run the test.** Expected: PASS (no cards added yet, so the tree is consistent).

- [ ] **Step 4: Commit.**
```
git add python/tools/repin_card_db_identity_v1.py python/tests/test_repin_card_db_identity_v1.py
git commit -m "tools: card DB identity re-pin checker for card waves"
git push
```

---

### Task 3: Training-store catalog profile for the card lane

**Files:**
- Modify: `mtg-kernel/src/native_training_store_run_v2.rs:163-164, 2292-2320` (classifier) and every `match` over `NativeRunCatalogProfileV1` (the compiler lists them)
- Test: the module's existing classifier tests plus one new test

**Interfaces:**
- Produces: `NativeRunCatalogProfileV1::PauperMetaW1` selected when `card_db_hash_u64_hex == FROZEN_CARD_DB_HASH_U64_HEX_PAUPER_META_W1 && runtime_catalog_sha256 == FROZEN_RUNTIME_CATALOG_SHA256_CURRENT_V1` (the runtime catalog is unchanged in this wave).

Note: this is the schema migration the spec's ruling 6 covers. It lands on the card lane only; Jack rules before any store written under this profile is used.

- [ ] **Step 1: Write the failing test** next to the existing classifier tests (`grep -n "fn classify_catalog_profile" mtg-kernel/src/native_training_store_run_v2.rs` and the `#[cfg(test)]` block that names `Historical`/`Current`):
```rust
#[test]
fn classify_catalog_profile_accepts_the_pauper_meta_w1_tuple() {
    let environment = sample_environment_v2_with(
        FROZEN_CARD_DB_HASH_U64_HEX_PAUPER_META_W1,
        FROZEN_RUNTIME_CATALOG_SHA256_CURRENT_V1,
    );
    assert_eq!(
        classify_catalog_profile_v1(&environment).unwrap(),
        NativeRunCatalogProfileV1::PauperMetaW1
    );
    let hybrid = sample_environment_v2_with(
        FROZEN_CARD_DB_HASH_U64_HEX_PAUPER_META_W1,
        FROZEN_RUNTIME_CATALOG_SHA256_V2,
    );
    assert!(classify_catalog_profile_v1(&hybrid).is_err(), "hybrid tuples stay rejected");
}
```
(`sample_environment_v2_with` is whatever helper the existing tests use to build an environment with given hash strings; reuse its name.)

- [ ] **Step 2: Run it.** Expected: compile FAIL (constant and variant missing).

- [ ] **Step 3: Implement.** Add `const FROZEN_CARD_DB_HASH_U64_HEX_PAUPER_META_W1: &str = "<filled by Task 13 step 5>";` initialised to the current live hash `64c82a261e078f1a` for now so the crate builds, add the enum variant, extend `classify_catalog_profile_v1` to a three-way exclusive match (`(true,false,false)`, `(false,true,false)`, `(false,false,true)`, everything else `InvalidLiteral`), and update every exhaustive `match` the compiler reports so `PauperMetaW1` behaves like `Current` (same field encodings) except in the profile label string, which becomes `"pauper_meta_w1"`.

- [ ] **Step 4: Run** `cargo test --locked -p mtg-kernel --lib native_training_store_run_v2`. Expected: PASS. (The new test passes trivially while the constant equals the current hash; Task 13 finalizes it.)

- [ ] **Step 5: Commit.**
```
git add mtg-kernel/src/native_training_store_run_v2.rs
git commit -m "store: third catalog profile PauperMetaW1 for the card lane (schema migration, ruling 6 pending)"
git push
```

---

### Task 4: Deck file writer and the nine V2 `.dek` files

**Files:**
- Create: `python/tools/write_dek_from_mtgo_list_v1.py`
- Create: nine files under `oracle/xmage/decks/Pauper/` named `Deck - Madness Burn V2.dek`, `Deck - Mono-Blue Delver V2.dek`, `Deck - Grixis Affinity V2.dek`, `Deck - Red Deck Wins V2.dek`, `Deck - Jund Wildfire V2.dek`, `Deck - Elves V2.dek`, `Deck - Mono-Blue Terror V2.dek`, `Deck - Spy Combo V2.dek`, `Deck - Dimir Terror V2.dek`
- Copy: the same nine files into the Mage fork `Mage.Server.Plugins/Mage.Player.AIRL/src/mage/player/ai/decks/Pauper/` (commit on `lead/pauper-meta-registrations-v1`)
- Test: `python/tests/test_write_dek_from_mtgo_list_v1.py`

**Interfaces:**
- Produces: `write_dek_from_mtgo_list_v1.py <list.txt> <out.dek> [--substitute "Old Name=New Name"]...` writing the XMage `.dek` XML (`<Cards CatID="0" Quantity="N" Sideboard="true|false" Name="..." Annotation="0"/>`, header identical to the existing files, CRLF line endings like the existing files: check with `file "oracle/xmage/decks/Pauper/Deck - Elves.dek"`). Substitutions replace the name and keep the quantity; a substitution whose target is not in `data/cards_v1.json` or whose source is absent from the list is an error.

- [ ] **Step 1: Failing test:**
```python
import subprocess, sys, unittest, pathlib, tempfile, xml.etree.ElementTree as ET
ROOT = pathlib.Path(__file__).resolve().parents[2]
LIST = ROOT / "docs/research/pauper_meta_decklists_2026-09-09/Affinity__887998.txt"

class WriteDek(unittest.TestCase):
    def test_substitutions_and_counts(self):
        out = pathlib.Path(tempfile.mkdtemp()) / "t.dek"
        subprocess.run([sys.executable, "python/tools/write_dek_from_mtgo_list_v1.py", str(LIST), str(out),
                        "--substitute", "Utrom Monitor=Glint Hawk", "--substitute", "Sewer-veillance Cam=Cryogen Relic"],
                       cwd=ROOT, check=True)
        rows = ET.parse(out).getroot().findall("Cards")
        main = sum(int(r.get("Quantity")) for r in rows if r.get("Sideboard") == "false")
        side = sum(int(r.get("Quantity")) for r in rows if r.get("Sideboard") == "true")
        names = {r.get("Name") for r in rows}
        self.assertEqual((main, side), (60, 15))
        self.assertIn("Glint Hawk", names); self.assertNotIn("Utrom Monitor", names)
```
Run it. Expected: FAIL (tool missing).

- [ ] **Step 2: Implement the tool** (parse "N Name" lines, blank line or "Sideboard" starts the sideboard, strip " // " suffixes, apply substitutions, emit XML with the two-line header copied verbatim from `Deck - Elves.dek`, one `Cards` row per list line in list order, closing `</Deck>`).

- [ ] **Step 3: Run the test.** Expected: PASS.

- [ ] **Step 4: Write the nine files** with these exact commands (one per registration; only AffinityV2 has substitutions):
```
P=python/tools/write_dek_from_mtgo_list_v1.py; D=docs/research/pauper_meta_decklists_2026-09-09; O="oracle/xmage/decks/Pauper"
uvx uv@0.11.29 run --no-sync python $P $D/Burn__888077.txt "$O/Deck - Madness Burn V2.dek"
uvx uv@0.11.29 run --no-sync python $P $D/MonoBlueAggro__887942.txt "$O/Deck - Mono-Blue Delver V2.dek"
uvx uv@0.11.29 run --no-sync python $P $D/Affinity__887998.txt "$O/Deck - Grixis Affinity V2.dek" --substitute "Utrom Monitor=Glint Hawk" --substitute "Sewer-veillance Cam=Cryogen Relic"
uvx uv@0.11.29 run --no-sync python $P $D/RedDeckWins__888081.txt "$O/Deck - Red Deck Wins V2.dek"
uvx uv@0.11.29 run --no-sync python $P $D/Jund__887774.txt "$O/Deck - Jund Wildfire V2.dek"
uvx uv@0.11.29 run --no-sync python $P $D/Elves__887257.txt "$O/Deck - Elves V2.dek"
uvx uv@0.11.29 run --no-sync python $P $D/Terror__887001.txt "$O/Deck - Mono-Blue Terror V2.dek"
uvx uv@0.11.29 run --no-sync python $P $D/Spy__887010.txt "$O/Deck - Spy Combo V2.dek"
uvx uv@0.11.29 run --no-sync python $P $D/DimirControl__886841.txt "$O/Deck - Dimir Terror V2.dek"
```
(Glint Hawk is not yet in the registry, so the tool's registry check must accept names listed in this plan's metadata table; implement the check as "in `cards_v1.json` or in `--allow-pending Name`" and pass `--allow-pending "Glint Hawk"`.) Copy the nine files to the Mage fork deck directory and verify each loads there: `cd C:\Users\Jack\IdeaProjects\mage-cycle4-lead && mvn -q -pl Mage.Server.Plugins/Mage.Player.AIRL -Dtest=DeterminizationSamplerTest test` if such a test exists (`grep -rl "pauperDefaults" Mage.Server.Plugins/Mage.Player.AIRL/src/test`), otherwise write a five-line JUnit test there that calls `DeterminizationSampler.pauperRegistrationsV2()` and asserts each archetype has 60 mainboard cards; the XMage importer resolves rows by `Name`, so `CatID="0"` is acceptable only if that test passes.

- [ ] **Step 5: Commit** (kernel repo and Mage fork separately, both pushed):
```
git add python/tools/write_dek_from_mtgo_list_v1.py python/tests/test_write_dek_from_mtgo_list_v1.py "oracle/xmage/decks/Pauper/"*V2.dek
git commit -m "decks: nine V2 Pauper registrations from pinned mtgtop8 lists (not yet registered)"
git push
```

---

### Task 5: Spells on existing machinery (Terminate, Ancient Grudge, Artful Dodge, Abandon Attachments, Acorn Harvest, Squirrel Token)

**Files:**
- Modify: `data/cards_v1.json` (append the six entries from the metadata table, in this order: Terminate, Ancient Grudge, Artful Dodge, Abandon Attachments, Acorn Harvest, Squirrel Token)
- Modify: `mtg-kernel/build.rs` `special_for` (2864+), `effect_recipe_for` (2979+), the `Special` enum (2330+), `flashback_for` (3360+), the token table (`grep -n "Map Token" mtg-kernel/build.rs` for the token creation recipe precedent), `keywords_for` (3138+)
- Modify: `mtg-kernel/src/card_def.rs` `Keywords` (add `CANT_BE_BLOCKED` bit next to `ISLANDWALK`) and `TargetSpec` (no additions: `Creature`, `ArtifactPermanent` exist)
- Modify: `mtg-kernel/src/engine.rs` blocker legality (`grep -n "ISLANDWALK" mtg-kernel/src/engine.rs` shows the evasion check to extend with `CANT_BE_BLOCKED`)
- Create: `mtg-kernel/tests/pauper_meta_w1_spells.rs`

**Interfaces:**
- Consumes: `Special::DestroyNonlegendaryCreature` recipe shape (`target=NonlegendaryCreature;spell=DestroyObject(Target0);mana=None`), `EffectOp::GrantKeywordTargetUntilEndOfTurn`, `EffectOp::MayPayCostThen`, `EffectOp::CreateToken`, `CostComponent::PayLife(u8)`, `FlashbackDef` cost arrays.
- Produces: `Special::DestroyCreature` (Terminate: `target=Creature;spell=DestroyObject(Target0);mana=None`), `Special::DestroyArtifact` (Ancient Grudge: `target=ArtifactPermanent;spell=DestroyObject(Target0);mana=None`), `Special::GrantCantBeBlockedUntilEndOfTurn` (Artful Dodge), `Special::MayDiscardThenDraw { draw: 2 }` (Abandon Attachments: `MayPayCostThen { cost: DiscardCards(1), then: DrawCards(Controller, 2) }`), `Special::CreateTokens { token: "Squirrel Token", count: 2 }` (Acorn Harvest).

- [ ] **Step 1: Registry entries.** Append the six JSON objects. Run `cargo build --locked -p mtg-kernel`. Expected: build.rs panics "card ... has empty deck coverage"? No: each has a non-empty `decks` list. Expected: build succeeds with the cards as `Special::None` (unplayable spells resolve to `spell=None`).

- [ ] **Step 2: Failing tests** in `mtg-kernel/tests/pauper_meta_w1_spells.rs`. Header cites the five Java files and blob SHAs (`git -C C:\Users\Jack\IdeaProjects\mage-cycle4-lead rev-parse 72a08a3b:Mage.Sets/src/mage/cards/t/Terminate.java` and the others). Copy the `card_id`, `card_name`, `put_object` helpers from `tests/deep_analysis.rs:24-60` verbatim, plus a `ready_main1(p0_library: &[&str], p1_library: &[&str]) -> GameState` that mirrors `ready_deep` without the Deep Analysis object (Main1, P0 active with priority, both libraries as given). Tests:

```rust
#[test]
fn terminate_destroys_target_creature() {
    let mut state = ready_main1(&["Island"; 8], &["Island"; 8]);
    let terminate = put_object(&mut state, PlayerId::P0, "Terminate", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Swamp", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
    let victim = put_object(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
    let offer = engine::advance_until_decision(&mut state);
    assert!(matches!(offer, Decision::CastSpellOrPass { ref castable_spells, .. } if castable_spells.contains(&terminate)));
    engine::step(&mut state, Action::CastSpell(terminate)).unwrap();
    match engine::advance_until_decision(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => assert!(legal_targets.contains(&Target::Object(victim))),
        other => panic!("{other:?}"),
    }
    engine::step(&mut state, Action::ChooseTarget(Target::Object(victim))).unwrap();
    pass_until_stack_empty(&mut state);
    assert_eq!(state.objects[victim].zone, Zone::Graveyard);
    assert_eq!(state.objects[terminate].zone, Zone::Graveyard);
}

#[test]
fn terminate_has_no_legal_target_without_a_creature() { /* only lands on both sides: Terminate is not castable (or ChooseTargets offers none); assert whichever the engine's convention is by checking Cast Down's existing test in tests/pauper_interaction_creatures_v1.rs and mirroring it */ }

#[test]
fn ancient_grudge_destroys_an_artifact_and_flashes_back_for_g() {
    // cast from hand with {1}{R} on Mistvault Bridge? No: pay with Mountain + Forest; destroy P1's Myr Enforcer (artifact creature).
    // then from graveyard: flashback offered with exactly one untapped Forest, resolves, card exiled.
}

#[test]
fn artful_dodge_makes_the_target_unblockable_this_turn_and_flashes_back() {
    // P0 attacks with a 2/2 into P1's untapped 3/3 after Artful Dodge; DeclareBlockers legal set for P1 must exclude the 3/3 (empty); next turn the keyword is gone.
}

#[test]
fn abandon_attachments_draws_two_only_if_a_card_is_discarded() {
    // hand: Abandon Attachments + Island; pay {1}{U/R} with Island + Mountain; decision offers pay-discard or decline; decline: no draw; accept: Island discarded, two draws.
}

#[test]
fn acorn_harvest_creates_two_squirrels_and_flashback_costs_three_life() {
    // resolve from hand: two objects named "Squirrel Token" on P0 battlefield, is_token true, 1/1 green.
    // from graveyard with {1}{G} available: flashback offered; life 20 -> 17; exiled afterwards; at life 3 or less the flashback is not offered (PayLife legality mirrors Deep Analysis's test `flashback_is_not_offered_below_three_life` in tests/deep_analysis.rs).
}
```
Fill each body in the same style as the first test; the comments above are the assertions to encode, not placeholders to leave. `pass_until_stack_empty` is `pass_until_stack_len(state, 0)` from `deep_analysis.rs:126-140`, copied.

- [ ] **Step 3: Run**: `cargo test --locked -p mtg-kernel --test pauper_meta_w1_spells`. Expected: FAIL (spells not castable, `Special::None`).

- [ ] **Step 4: Implement in build.rs**: add the five `Special` variants with doc comments, map the names in `special_for`, add recipes in `effect_recipe_for` (Terminate `target=Creature;spell=DestroyObject(Target0);mana=None`; Ancient Grudge `target=ArtifactPermanent;spell=DestroyObject(Target0);mana=None`; Artful Dodge `target=Creature;spell=GrantKeywordTargetUntilEndOfTurn(Target0,CANT_BE_BLOCKED);mana=None`; Abandon Attachments `target=None;spell=MayPayCostThen(DiscardCards(1),DrawCards(Controller,2));mana=None`; Acorn Harvest `target=None;spell=CreateToken(Squirrel Token,2);mana=None`), the canonical tokens in the `Special` to string table (line 2660 region), and the codegen branches (line 4533+, follow the `DestroyNonlegendaryCreature` branch at 6162 and the `DrawThenCreateToken` branch for the token). Flashback: `"Ancient Grudge" => {G}`, `"Artful Dodge" => {U}`, `"Acorn Harvest" => {1}{G} then CostComponent::PayLife(3)` (Deep Analysis precedent at `flashback_for`). Hybrid pip: `parse_cost("{1}{U/R}")` must produce a hybrid pip; check `grep -n "Hybrid" mtg-kernel/build.rs mtg-kernel/src/mana.rs`; the registry already has `phyrexian_mana`, and if no hybrid pip exists add `Pip::Hybrid(ManaColor, ManaColor)` to `card_def.rs` and payment in `mana.rs` (either color satisfies it) with a unit test in `mana.rs` (`hybrid_pip_accepts_either_color`). Keyword: add `CANT_BE_BLOCKED` and make the blocker legality check treat it like an unconditional evasion (no legal blockers). Token: add `("Squirrel Token", ("Acorn Harvest",))` to `TOKEN_DEPENDENCIES` in `generate_pauper_manifests.py`.

- [ ] **Step 5: Run** the new test file, then `cargo test --locked -p mtg-kernel`. Expected: PASS except pinned identity tests (card DB hash moved). Run `uvx uv@0.11.29 run --no-sync python python/tools/repin_card_db_identity_v1.py --write`, then `cargo test --locked -p mtg-kernel` again. Expected: PASS. (Goldens under `data/flat_policy_*` are regenerated once in Task 13, so if their tests fail here, mark them `#[ignore]`-free but run the golden generators now as well: `uvx uv@0.11.29 run --no-sync python python/tools/generate_flat_policy_v1_goldens.py` and `..._v2_goldens.py`, then rerun.)

- [ ] **Step 6: Commit** with the hash line:
```
git add -A
git commit -m "cards: Terminate, Ancient Grudge, Artful Dodge, Abandon Attachments, Acorn Harvest, Squirrel Token

KERNEL_CARDDB_HASH 0x64c82a261e078f1a -> 0x<new>"
git push
```

---

### Task 6: Mass pump and controller damage (Suffocating Fumes, Arms of Hadar, Smash to Smithereens, Raze)

**Files:**
- Modify: `data/cards_v1.json` (append Suffocating Fumes, Arms of Hadar, Smash to Smithereens, Raze)
- Modify: `mtg-kernel/src/effect.rs` (new ops), `mtg-kernel/src/card_def.rs` (`TargetSpec::LandPermanent`, `PermanentFilter::Land`), `mtg-kernel/build.rs` (specials, recipes, `additional_cost_for`, cycling recipe following Lorien Revealed at 3638)
- Create: `mtg-kernel/tests/pauper_meta_w1_black_red.rs`

**Interfaces:**
- Produces: `EffectOp::PumpAllUntilEndOfTurn { filter: PermanentFilterDef, controller: PumpControllerScope, power: i16, toughness: i16 }` with `PumpControllerScope::{Opponents, TargetPlayer(u8)}`; `EffectOp::DealDamageToControllerOfTarget { target: u8, amount: i32 }` using last-known controller when the target has left the battlefield; `Special::PumpOpponentsCreatures { power, toughness }`, `Special::PumpTargetPlayersCreatures { power, toughness }`, `Special::SmashToSmithereens`, `Special::DestroyLand`.

- [ ] **Step 1: Failing tests** (same helpers as Task 5):
  - `suffocating_fumes_gives_opponents_creatures_minus_one_until_end_of_turn`: P1 has a 1/1 (Squirrel Token via `put_object` with `is_token` state) and a 3/3; P0 has a 2/2. After resolution the 1/1 died (graveyard, tokens cease to exist: check the engine's token cleanup convention in `tests/spy_combo_core.rs`), the 3/3 is 2/2 (assert the effective toughness through the engine's characteristic accessor used in `tests/elves_tribal.rs`), P0's 2/2 is unchanged; after `engine` advances to P0's next turn the 3/3 is back to 3/3.
  - `suffocating_fumes_cycles_for_two`: from hand with two untapped lands, the cycling activation is offered (`Decision::ActivateAbilityOrPass` or the engine's cycling decision shape used in `tests/lorien_revealed.rs`), resolves to one draw and the card in the graveyard.
  - `arms_of_hadar_shrinks_only_the_targeted_players_creatures`: targets P1; P1's 2/2 dies; P0's 2/2 is unchanged; targeting P0 instead shrinks P0's.
  - `smash_to_smithereens_destroys_the_artifact_and_burns_its_controller`: P1 controls Myr Enforcer; after resolution it is in the graveyard and P1 life is 17; when the artifact is sacrificed in response (P1 activates Nihil Spellbomb's sacrifice ability, an existing card, holding priority) the spell still deals 3 to P1 (last known controller) and P1 life is 17.
  - `raze_requires_sacrificing_a_land_and_destroys_the_target_land`: with one Mountain only, Raze cannot be cast (the mana must come from the sacrificed land? No: cost payment order in XMage sacrifices at cost time after mana; assert the engine offers Raze only when P0 controls at least one land in addition to mana sources, mirroring how `Fanatical Offering`'s sacrifice additional cost is tested in `tests/affinity_wildfire_value.rs`), and after resolution both the sacrificed land and the target land are in graveyards.

- [ ] **Step 2: Run**: expected FAIL.

- [ ] **Step 3: Implement** the two ops in `effect.rs` (resolution: iterate the battlefield, apply the until-end-of-turn modifier through the same mechanism `PumpTargetUntilEndOfTurnDynamic` uses; controller damage reads `state.objects[target].controller` from the last-known snapshot the engine keeps for targets that left the battlefield, see how `DealDamage` handles a vanished creature target), the recipes in build.rs (`Suffocating Fumes` `target=None;spell=PumpAllUntilEndOfTurn(Creature,Opponents,-1,-1);mana=None`; `Arms of Hadar` `target=AnyPlayer;spell=PumpAllUntilEndOfTurn(Creature,TargetPlayer0,-2,-2);mana=None`; `Smash to Smithereens` `target=ArtifactPermanent;spell=Sequence[DestroyObject(Target0),DealDamageToControllerOfTarget(0,3)];mana=None`; `Raze` `target=LandPermanent;spell=DestroyObject(Target0);mana=None` with `additional_cost_for("Raze") => Some(&[CostComponent::SacrificeControlled { count: 1, filter: PermanentFilter::Land }])`), the cycling activated-ability recipe for Suffocating Fumes (copy Lorien Revealed's recipe with `Cost {generic: 2}`), and `TargetSpec::LandPermanent` legality in the targeting code (`grep -n "TargetSpec::ArtifactPermanent =>" mtg-kernel/src/engine.rs` shows the match to extend).

- [ ] **Step 4: Run** the file, then the full crate after `repin_card_db_identity_v1.py --write`. Expected: PASS.

- [ ] **Step 5: Commit** (hash line in the body as in Task 5): `git commit -m "cards: Suffocating Fumes, Arms of Hadar, Smash to Smithereens, Raze; mass pump and controller-damage ops"` and push.

---

### Task 7: Conditional alternative cost (Snuff Out) and typed duals (Contaminated Aquifer, Ice Tunnel)

**Files:**
- Modify: `data/cards_v1.json` (append Snuff Out, Contaminated Aquifer, Ice Tunnel)
- Modify: `mtg-kernel/src/card_def.rs` (`AltCostDef { components: &[CostComponent], condition: AltCostCondition }`, `AltCostCondition::{Always, ControlsPermanentWithSubtype(Subtype)}`), `mtg-kernel/build.rs` `alt_cost_for` (3318) and its codegen, `TargetSpec::NonblackCreature`
- Create: `mtg-kernel/tests/pauper_meta_w1_snuff_out_and_duals.rs`

**Interfaces:**
- Consumes: Fireblast's alt cost path (`alt_cost_for` and the engine's alternative-cost offer).
- Produces: `alt_cost_for` returns `AltCostDef` instead of a bare component slice; existing Fireblast and Land Grant entries become `AltCostDef { components: ..., condition: AltCostCondition::Always }`.

- [ ] **Step 1: Failing tests**:
  - `snuff_out_offers_the_life_payment_only_with_a_swamp`: P0 has Island only and Snuff Out in hand at 20 life: not castable; add a Swamp: castable through the alternative cost with no mana spent, life 16, target nonblack creature destroyed; a black creature (Gixian Infiltrator once Task 8 lands, otherwise Gurmag Angler after Task 10; for now use a black creature already in the registry: `grep -n '"colors": \["B"\]' data/cards_v1.json | head`) is not a legal target.
  - `snuff_out_still_casts_for_mana_without_a_swamp` with four lands.
  - `contaminated_aquifer_enters_tapped_and_taps_for_u_or_b`: after playing it, `tapped == true`; next turn the mana ability offers U and B (mirror `tests/pauper_simple_draw_lands_v1.rs`'s Idyllic Beachfront test).
  - `ice_tunnel_is_snow`: `CARD_DEFS[card_id("Ice Tunnel")].supertypes` contains `Supertype::Snow` (check the supertype accessor name used by Snow-Covered Forest tests: `grep -rn "Snow" mtg-kernel/tests | head`).

- [ ] **Step 2: Run**: FAIL.

- [ ] **Step 3: Implement**: the two lands are tag-generated (no code beyond the registry). Snuff Out: `special_for => Special::DestroyNonblackCreature`, recipe `target=NonblackCreature;spell=DestroyObject(Target0);mana=None`, `alt_cost_for("Snuff Out") => "Some(AltCostDef { components: &[CostComponent::PayLife(4)], condition: AltCostCondition::ControlsPermanentWithSubtype(Subtype::Swamp) })"`; extend the engine's alternative-cost offer to evaluate the condition against the caster's battlefield at offer time and at payment time.

- [ ] **Step 4: Run** file and crate (with re-pin). Expected: PASS.

- [ ] **Step 5: Commit**: `git commit -m "cards: Snuff Out (conditional alternative cost), Contaminated Aquifer, Ice Tunnel"` and push.

---

### Task 8: Triggered creatures (Kessig Flamebreather, Gixian Infiltrator, Webweaver Changeling, Glint Hawk)

**Files:**
- Modify: `data/cards_v1.json` (append the four)
- Modify: `mtg-kernel/build.rs` `trigger_recipe_for` (4433+), `keywords_for` (3138+), `changeling_for` (4425)
- Modify: `mtg-kernel/src/trigger.rs` (new trigger tables and effects), `mtg-kernel/src/effect.rs` (`EffectCond::ControllerGraveyardCreatureCardsAtLeast(u8)`)
- Create: `mtg-kernel/tests/pauper_meta_w1_creatures.rs`

**Interfaces:**
- Consumes: Guttersnipe (`cast_instant_or_sorcery:damage_opponent:2`), Writhing Chrysalis (sacrifice trigger precedent), Sagu Wildling (`etb:gain_life:3`), Masked Vandal (changeling), `EffectOp::MayPayCostThen`, `CostComponent::ReturnControlledPermanentToOwnersHand(PermanentFilterDef::Artifact)`.
- Produces: trigger recipes `cast_noncreature:damage_opponent:1` (Kessig), `sacrifice_another_controlled_permanent:plus_one_counter_on_source` (Gixian), `etb_if_graveyard_creature_cards_at_least_3:gain_life:5` (Webweaver, intervening if), `etb:unless_return_controlled_artifact_to_hand:sacrifice_source` (Glint Hawk).

- [ ] **Step 1: Failing tests**:
  - `kessig_flamebreather_pings_each_opponent_on_noncreature_casts_only`: cast Lightning Bolt at P0's own face? No: cast Ponder (noncreature, in registry) → P1 life 19; cast a creature (Faerie Miscreant) → no trigger; cast an artifact (Nihil Spellbomb) → P1 life 18.
  - `gixian_infiltrator_grows_when_another_permanent_is_sacrificed`: P0 sacrifices a Blood token by activating it (existing Blood Fountain flow in `tests/artifact_control.rs`) → Infiltrator has one +1/+1 counter; Infiltrator itself being sacrificed does not trigger.
  - `webweaver_changeling_gains_five_only_with_three_creature_cards_in_graveyard`: two creature cards in graveyard → no life gain; three → life 25; the intervening-if is rechecked on resolution (exile one creature card from the graveyard with Nihil Spellbomb's activation in response, count drops to 2, no gain).
  - `webweaver_changeling_has_every_creature_type`: is an Elf for Elvish Mystic's count (`PumpTargetByControlledSubtypeCount`-style query used by an existing Elves test in `tests/elves_tribal.rs`).
  - `glint_hawk_is_sacrificed_unless_an_artifact_is_returned`: with no artifact, the ETB resolves by sacrificing the Hawk; with Ichor Wellspring on the battlefield the decision offers returning it; accept → Wellspring in hand, Hawk stays; decline → Hawk sacrificed.

- [ ] **Step 2: Run**: FAIL.

- [ ] **Step 3: Implement** the four trigger tables in `trigger.rs` (copy the `GUTTERSNIPE_TRIGGERS` static and effect function shape; the noncreature filter reuses the predicate Black Mage's Rod's `noncreature_spell_damage_to_each_opponent` uses), the recipe strings in build.rs, `keywords_for` (`"Glint Hawk" => FLYING`, `"Webweaver Changeling" => REACH`), `changeling_for` (`name == "Masked Vandal" || name == "Webweaver Changeling"`), the new `EffectCond`, and the Hawk ETB as `MayPayCostThen { cost: ReturnControlledPermanentToOwnersHand(Artifact), then: Sequence[] , otherwise: SacrificeSource }` (if `MayPayCostThen` has no `otherwise` arm, add one as `Option<Box<EffectOp>>` defaulting to none; every existing constructor passes `None`).

- [ ] **Step 4: Run** file and crate with re-pin. Expected: PASS.

- [ ] **Step 5: Commit**: `git commit -m "cards: Kessig Flamebreather, Gixian Infiltrator, Webweaver Changeling, Glint Hawk"` and push.

---

### Task 9: Delver of Secrets (transform in place) and visible face lookup

**Files:**
- Modify: `data/cards_v1.json` (append Delver of Secrets)
- Modify: `mtg-kernel/build.rs` `transform_face_for` (3261 region, `TransformFaceDef` for Insectile Aberration), `trigger_recipe_for` (`"Delver of Secrets" => "upkeep_controller:look_top_may_reveal_instant_or_sorcery:transform_source_in_place"`)
- Modify: `mtg-kernel/src/effect.rs` (`EffectOp::TransformSourceInPlace`, `EffectOp::LookAtTopMayRevealThen { predicate: CardTypePredicate, then: Box<EffectOp> }`), `mtg-kernel/src/event.rs` (a `Transformed { object, face_index }` committed event that flips `ObjectStateV4::face_index` without a zone change), `mtg-kernel/src/trigger.rs` (upkeep trigger table)
- Modify: `mtg-kernel/src/card_def.rs`: add `pub fn card_id_by_visible_name(name: &str) -> Option<(u16, u8)>` returning `(card id, face index)` for front names (face 0), transform back faces (face 1), and adventure names (face 2 once Task 11 lands); tokens resolve by their registry name already.
- Create: `mtg-kernel/tests/pauper_meta_w1_delver.rs`

**Interfaces:**
- Consumes: `TransformFaceDef` (card_def.rs:901), `ObjectStateV4::face_index`, the saga transform precedent (`effect.rs:10673`, which must stay unchanged).
- Produces: `card_id_by_visible_name` for the MTGO correspondence layer (spec section 3, visible faces).

- [ ] **Step 1: Failing tests**:
  - `delver_transforms_when_the_revealed_top_card_is_an_instant_or_sorcery`: P0 library top is Ponder; advance from P0's untap into upkeep; the trigger resolves with a reveal decision (may reveal); accept → object name reads "Insectile Aberration", power 3, toughness 2, `Keywords::FLYING`, still the same `ObjectId`, `zone_change_count` unchanged; decline reveal → no transform.
  - `delver_does_not_transform_on_a_land_or_creature`: top is Island → unchanged.
  - `delver_trigger_is_controllers_upkeep_only`: on P1's upkeep nothing triggers.
  - `visible_name_resolves_faces`: `card_id_by_visible_name("Insectile Aberration") == Some((card_id("Delver of Secrets"), 1))`, `card_id_by_visible_name("Delver of Secrets") == Some((id, 0))`, `card_id_by_visible_name("Nonexistent") == None`.

- [ ] **Step 2: Run**: FAIL.

- [ ] **Step 3: Implement.** `transform_face_for("Delver of Secrets") => Some(TransformFaceDef { name: "Insectile Aberration", types: &[CardType::Creature], subtypes: &[Subtype::Human, Subtype::Insect], colors: &[ManaColor::U], power: Some(3), toughness: Some(2), keywords: Keywords::FLYING })` (add `Subtype::Insect` if absent). The upkeep trigger: a new trigger kind `BeginningOfUpkeep { controller_only: true }` if none exists (`grep -n "Upkeep" mtg-kernel/src/trigger.rs`), effect `LookAtTopMayRevealThen { predicate: InstantOrSorcery, then: TransformSourceInPlace }`. Characteristics after transform: every accessor that reads name, types, P/T, colors, keywords must consult `face_index` (find them with `grep -n "transform_face" mtg-kernel/src/*.rs`; the saga path already switches on it for Vector Glider, so the flip must reuse that switch). `card_id_by_visible_name` is generated in build.rs next to `card_id_by_name` (6495) from the front names plus `transform_face` names.

- [ ] **Step 4: Run** file and crate with re-pin. Expected: PASS.

- [ ] **Step 5: Commit**: `git commit -m "cards: Delver of Secrets with in-place transform; card_id_by_visible_name for faces"` and push.

---

### Task 10: Delve (Gurmag Angler) and equipment-granted ability (Viridian Longbow)

**Files:**
- Modify: `data/cards_v1.json` (append Gurmag Angler, Viridian Longbow)
- Modify: `mtg-kernel/src/card_def.rs` (`CardDef::delve: bool`; `EquipmentDef::granted_activated_ability: Option<GrantedActivatedAbilityDef>` with `GrantedActivatedAbilityDef { cost: &[CostComponent], target_spec: TargetSpec, effect: fn() -> EffectOp }`), `mtg-kernel/src/mana.rs` (delve payment plan: exile `k` cards from the caster's graveyard to reduce generic by `k`), `mtg-kernel/src/engine.rs` (cast flow offers a `ChooseDelveCount` decision or folds delve into the existing payment-plan enumeration; choose the fold if the payment planner already enumerates plans, since a separate decision changes the action surface), `mtg-kernel/build.rs` (`delve_for(name) -> bool`, `equipment_for("Viridian Longbow")`, `equip cost {3}`)
- Create: `mtg-kernel/tests/pauper_meta_w1_delve_and_longbow.rs`

**Interfaces:**
- Consumes: `CostComponent::ExileOtherCardsFromOwnGraveyard(u8)` (Sleep of the Dead escape), `EquipmentDef` (card_def.rs:923), Black Mage's Rod and Hunter's Blowgun tests in `tests/artifact_equipment_completion.rs`.
- Produces: delve as a generic cast-time reduction; equipment that grants a tap ability to the equipped creature.

- [ ] **Step 1: Failing tests**:
  - `gurmag_angler_costs_one_black_with_six_cards_in_graveyard`: P0 has one Swamp untapped and six cards in graveyard: castable; after casting, the graveyard has zero cards and the six are in exile; with only two cards in graveyard and one Swamp: not castable; with two in graveyard and five lands: castable, exiling two and tapping five (or tapping all seven lands with no exile if the planner prefers mana: assert both plans are legal by checking the legal action candidates include both).
  - `delve_never_exiles_more_than_generic`: seven cards in graveyard, seven lands: at most six are exiled.
  - `viridian_longbow_equips_for_three_and_grants_a_tap_ping`: equip onto a 1/1 Elf; the creature's activated abilities include a tap ability targeting any target; activate at P1 face: P1 life 19, creature tapped; unequipped creatures have no such ability; summoning-sick creatures cannot activate (tap cost).

- [ ] **Step 2: Run**: FAIL.

- [ ] **Step 3: Implement** per the interfaces; the mana planner's delve branch enumerates `k` from `0..=min(graveyard_len, generic)` and emits payment plans in the engine's existing plan encoding so `legal_action_candidates_v1` stays schema-stable (check `mtg-kernel/src/rl.rs` for how payment plans surface; if plans are opaque there, delve counts are part of the plan identity, not a new action kind). Longbow: `EquipmentDef { power_delta: 0, toughness_delta: 0, add_subtype: None, controller_turn_keywords: Keywords::NONE, other_turn_keywords: Keywords::NONE, noncreature_spell_damage_to_each_opponent: 0, job_select: false, granted_activated_ability: Some(GrantedActivatedAbilityDef { cost: &[CostComponent::Tap], target_spec: TargetSpec::AnyTarget, effect: longbow_ping }) }` where `longbow_ping` returns `DealDamage { target: TargetRef::Target(0), amount: 1 }`; every existing `EquipmentDef` literal gains `granted_activated_ability: None`.

- [ ] **Step 4: Run** file and crate with re-pin. Expected: PASS.

- [ ] **Step 5: Commit**: `git commit -m "cards: Gurmag Angler (delve), Viridian Longbow (granted tap ability)"` and push.

---

### Task 11: Adventure (Fang Dragon) and monarch (Azure Fleet Admiral)

**Files:**
- Modify: `data/cards_v1.json` (append Fang Dragon, Azure Fleet Admiral)
- Modify: `mtg-kernel/src/card_def.rs` (`AdventureDef { name: &'static str, cost: Cost, types: &'static [CardType], target_spec: TargetSpec, effect: fn() -> EffectOp }`, `CardDef::adventure: Option<AdventureDef>`), `mtg-kernel/src/state.rs` (`ObjectStateV4::on_adventure: bool`; `GameState::monarch: Option<PlayerId>`), `mtg-kernel/src/engine.rs` (cast the adventure from hand as a sorcery-speed spell with the adventure's characteristics; on resolution exile the card with `on_adventure = true`; cast permission from exile for the creature face while `on_adventure`; monarch end-step draw trigger for the monarch's controller; combat damage to the monarch by a creature makes that creature's controller the monarch; blocker legality: a creature with `cant_be_blocked_by_monarchs_creatures` is unblockable by creatures the monarch controls), `mtg-kernel/src/trigger.rs` (`etb:become_monarch`), `mtg-kernel/build.rs` (`adventure_for`, trigger recipe, static flag), `mtg-kernel/src/rl.rs` observation (monarch holder and on-adventure flag must be observable: add them to the feature block only if the Flat V2 encoder has a reserved slot; otherwise leave observation unchanged and record that in the commit body, since the encoder file is under the overlay digest hazard)
- Create: `mtg-kernel/tests/pauper_meta_w1_adventure_and_monarch.rs`

**Interfaces:**
- Consumes: `OmenDef` (build.rs:4330-4340) as the alternative-characteristics precedent; initiative (`state.rs:333-348`, `Avenging Hunter` in trigger.rs 958-1060) as the global-designation precedent.
- Produces: `card_id_by_visible_name("Forktail Sweep") == Some((card_id("Fang Dragon"), 2))`.

- [ ] **Step 1: Failing tests**:
  - `forktail_sweep_is_cast_from_hand_then_the_dragon_is_cast_from_exile`: with {1}{R} the adventure is castable at sorcery speed; resolution deals 1 to each creature P0 does not control (P1's 1/1 dies, P0's 1/1 lives); the card is in exile with `on_adventure`; with {5}{R}{R} available on a later turn, Fang Dragon is castable from exile and enters as a 6/3 flier; a card exiled by other means (Nihil Spellbomb? no: use `put_object(..., Zone::Exile)`) is not castable.
  - `fang_dragon_can_be_cast_directly_from_hand`.
  - `azure_fleet_admiral_makes_its_controller_the_monarch_who_draws_at_end_step`: after ETB `state.monarch == Some(P0)`; at P0's end step P0 draws one; at P1's end step nobody draws.
  - `combat_damage_to_the_monarch_moves_the_crown`: P1 attacks P0 with a 2/2 unblocked → `state.monarch == Some(P1)`.
  - `admiral_cannot_be_blocked_by_the_monarchs_creatures`: P1 is the monarch with an untapped 3/3; P0 attacks with the Admiral; P1's legal blockers for it are empty; once P0 is the monarch again, P1's 3/3 may block it.
  - `visible_name_resolves_adventure_face`.

- [ ] **Step 2: Run**: FAIL.

- [ ] **Step 3: Implement** per the interfaces. Monarch triggers are engine-owned like initiative (bind them the way `InitiativeTriggerBindingV1` binds the undercity trigger, with a `MonarchTriggerBindingV1` in `state.rs`). The observation change, if any, must not touch `flat_policy_v2.rs` (overlay digest hazard, spec section 3); if monarch cannot be observed without it, leave it unobserved in this wave and note it in the commit body.

- [ ] **Step 4: Run** file and crate with re-pin. Expected: PASS.

- [ ] **Step 5: Commit**: `git commit -m "cards: Fang Dragon (adventure), Azure Fleet Admiral (monarch)"` and push.

---

### Task 12: Register the nine V2 decks

**Files:**
- Modify: `python/tools/generate_pauper_manifests.py` (`REGISTRATION_SPECS`: append nine `DeckSpec` rows in this order after Faeries: `("BurnV2","BurnV2","Deck - Madness Burn V2.dek",sha)`, `("DelverV2","DelverV2","Deck - Mono-Blue Delver V2.dek",sha)`, `("AffinityV2","AffinityV2","Deck - Grixis Affinity V2.dek",sha)`, `("RallyV2","RallyV2","Deck - Red Deck Wins V2.dek",sha)`, `("WildfireV2","WildfireV2","Deck - Jund Wildfire V2.dek",sha)`, `("ElvesV2","ElvesV2","Deck - Elves V2.dek",sha)`, `("TerrorV2","TerrorV2","Deck - Mono-Blue Terror V2.dek",sha)`, `("SpyV2","SpyV2","Deck - Spy Combo V2.dek",sha)`, `("DimirTerrorV2","DimirTerrorV2","Deck - Dimir Terror V2.dek",sha)`; `EXPECTED_MAINBOARD_SUPPORT` gains nine `{"full": 60, "partial": 0, "no_effect": 0}` entries; `RUNTIME_DECK_IDS` unchanged)
- Modify: `oracle/xmage/DeterminizationSampler.java` `pauperRegistrationsV2()` (nine `paths.put` lines appended in the same order, keys equal to ids) and the Mage fork copy (commit and push on `lead/pauper-meta-registrations-v1`)
- Modify: `data/pauper_sideboard_policy_v1.json` `deck_ids` (append the nine ids; `plans` stays empty)
- Modify: `python/tests/test_pauper_pool_manifest.py` totals (`deck_count` 18, unique and copy totals: take them from the generator's check-mode output)
- Regenerate: `data/pauper_pool_v1.json`, `data/pauper_support_v1.json`, `data/cards_v1.json` `decks` membership (the generator rewrites it), `data/flat_policy_v1/*`, `data/flat_policy_v2/*`

- [ ] **Step 1: Source SHAs.** `for f in oracle/xmage/decks/Pauper/*V2.dek; do uvx uv@0.11.29 run --no-sync python -c "import sys,hashlib; b=open(sys.argv[1],'rb').read().decode('utf-8').replace('\r\n','\n').replace('\r','\n').replace('\n','\r\n').encode(); print(hashlib.sha256(b).hexdigest(), sys.argv[1])" "$f"; done` (this is `canonical_source_sha256` from the generator, lines 176-193). Paste into the nine `DeckSpec` rows.

- [ ] **Step 2: Failing check.** `uvx uv@0.11.29 run --no-sync python python/tools/generate_pauper_manifests.py --check`. Expected: FAIL on the Java registrations method SHA (the roster changed) and on totals.

- [ ] **Step 3: Java roster and re-pin.** Append the nine `paths.put` lines to `pauperRegistrationsV2()` in both copies; run check mode, copy the new `JAVA_FACTORY_FILE_SHA256` and `JAVA_FACTORY_REGISTRATIONS_METHOD_SHA256` values it reports (`JAVA_FACTORY_METHOD_SHA256` for `pauperDefaults` must not change; if it did, the edit touched the wrong method).

- [ ] **Step 4: Write manifests and goldens.**
```
uvx uv@0.11.29 run --no-sync python python/tools/generate_pauper_manifests.py --write
sha256sum data/runtime_decks_v1.json      # must still be 68e7602f...b851
uvx uv@0.11.29 run --no-sync python python/tools/generate_flat_policy_v1_goldens.py
uvx uv@0.11.29 run --no-sync python python/tools/generate_flat_policy_v2_goldens.py
uvx uv@0.11.29 run --no-sync python python/tools/repin_card_db_identity_v1.py --check
```
Expected: pool totals `deck_count: 18`; `cards_v1.json` `decks` lists now include the V2 files for every shared card; the repin check passes (no new cards in this task).

- [ ] **Step 5: Tests.** Update `test_pauper_pool_manifest.py` totals from the generator output; run the Python suite and `cargo test --locked -p mtg-kernel` (the sideboard pool test from Task 1 now sees eighteen ids and the policy JSON must list them). Expected: PASS. Also run `cargo test --locked -p mtg-kernel --test bo3_session_v1` and start one Bo3 session with `p0 = "DimirTerrorV2"` against `"Burn"` in a new test `bo3_session_accepts_a_v2_registration` (copy the shape of the existing session test) to prove registrations are usable for match play.

- [ ] **Step 6: Commit** with the body listing each deck's source SHA, pool totals before and after, and the unchanged runtime catalog SHA:
```
git add -A
git commit -m "pool: register nine V2 Pauper decks (18 registrations, runtime set unchanged)"
git push
```

---

### Task 13: Identity finalisation and wave record

**Files:**
- Modify: `mtg-kernel/src/native_training_store_run_v2.rs` (`FROZEN_CARD_DB_HASH_U64_HEX_PAUPER_META_W1` set to the final live hash)
- Modify: `python/tests/test_pauper_pool_manifest.py`, `mtg-kernel/src/card_def.rs` test literal (via the repin tool)
- Create: `docs/research/pauper_meta_wave1_record_2026-09.md`
- Modify: `ROADMAP.md` (one paragraph under the card coverage section: registrations versus active set, wave 1 contents, certification status)

- [ ] **Step 1:** `uvx uv@0.11.29 run --no-sync python python/tools/repin_card_db_identity_v1.py --write`; set the training-store constant to the same hex; run `cargo test --locked -p mtg-kernel` and the Python suite. Expected: PASS everywhere, zero ignored tests newly added.

- [ ] **Step 2:** Recompute coverage with the wave's actual registrations: `uvx uv@0.11.29 run --no-sync python python/tools/pauper_meta_gap_v1.py docs/research/pauper_meta_decklists_2026-09-09 "$(cat docs/research/pauper_meta_shares_2026-09-09.json)" docs/research/pauper_meta_gap_after_w1_2026-09.json` and put the per-archetype coverage table (before and after) into the record file, with the list of every card added, every new primitive, the substitutions, and the old and new `KERNEL_CARDDB_HASH`.

- [ ] **Step 3: Commit**: `git commit -m "docs: Pauper meta wave 1 record; card DB identity finalised"` and push.

---

### Task 14: Parity: directed branch coverage against XMage

**Files:**
- Create: `data/wave_branch_manifests/pauper_meta_w1.json` (per card: the list of rules branches from the metadata section, as strings: e.g. `"Delver of Secrets": ["upkeep_trigger", "reveal_instant_or_sorcery_transforms", "reveal_declined", "non_instant_top_no_transform"]`; every card in the table gets its list; sideboard-only cards are marked `"post_board": true`)
- Create: `python/tools/check_wave_branch_coverage_v1.py` (reads a mapper trace directory and the manifest, prints exercised and unexercised branches per card, exit 1 if any unexercised)
- Mage fork: run the shadow-mapped harness (`RallyCp7KernelShadowMapper` on the anchor-spike pin; `grep -rn "pauperDefaults\|loadArchetypes" Mage.Server.Plugins/Mage.Player.AIRL/src/main/java/mage/player/ai/rl/*Shadow*` shows how it picks decks) with each V2 registration versus one historical deck, mapper trace on, at least 40 games per registration, sideboard-only cards forced into the mainboard for a second set of 20 games per registration (post-board configuration).
- Create: `docs/research/pauper_meta_wave1_parity_2026-09.md` (exercised branches, unexercised branches, every kernel-versus-XMage terminal disagreement with its trace path, the one-seed bit-identical rerun result)

**Interfaces:**
- Consumes: mapper trace format (see `docs/CP7_60_PERCENT_PROGRESS.md` rows dated 2026-09-06 for where traces land under `cp7-evidence/diagnostics/`), `RallyCp7KernelShadowMapper`'s branch-recording lines (if the mapper does not record per-card ability activations, add a `card_branch` trace line to the mapper in the Mage fork on `lead/pauper-meta-registrations-v1`: card name, ability index, decision outcome).
- Produces: the certification verdict per card. A card with an unexercised branch is not certified; the record lists it and the wave is not called done (spec 5.3 step 4).

- [ ] **Step 1:** Write the manifest and the checker with a unit test (`python/tests/test_check_wave_branch_coverage_v1.py`) on a synthetic trace directory: two cards, one fully exercised, one not; expect exit 1 and the unexercised branch named.
- [ ] **Step 2:** Run the shadow-mapped games (lead runs this: E-cores, low priority, mailbox announcement per the CPU rules).
- [ ] **Step 3:** Run the checker over the traces; fix any kernel divergence found (each fix is its own commit with a regression test in the card's test file); rerun until every branch is exercised or the record explains why a branch is unreachable in the pool (for example Campfire's commander clause in later waves).
- [ ] **Step 4:** One-seed bit-identical rerun of the standing small run manifest (the command recorded in `docs/CP7_60_PERCENT_PROGRESS.md` for the shard of record) against the branch build; record the aggregate identity.
- [ ] **Step 5: Commit** the manifest, checker, test, and record: `git commit -m "parity: wave 1 branch coverage manifest, checker, and XMage shadow record"` and push. Then the lead posts the wave summary to the mailbox and requests Codex review of the whole wave (`codex exec review --commit <range>` on a new thread) before calling wave 1 done.

---

## Self-review notes

- Spec coverage: section 3 catalog versus active set (Task 1), append-only ids (Task 2 tool, Task 5 onward), Mage-fork sampler (Tasks 1, 4, 12), primitives 5.2 for this wave's cards (Tasks 6 to 11), per-wave procedure 5.3 steps 1 (Task 4, 12), 2 (Tasks 5 to 11), 3 (same), 4 (Task 14), 5 (Task 9's `card_id_by_visible_name`; the MTGO correspondence crate lives on the deployment branch and consumes it in a later plan), 6 (Tasks 12, 13); ruling 6 schema migration (Task 3) flagged for Jack.
- Not in this plan: sideboard plan table (spec 6.1), classifier and controller (6.2, 6.3), Bo3 gate (6.4): separate plans after Jack's rulings 3, 4, and 7.
- Type names introduced here and reused across tasks: `Special::DestroyCreature`, `Special::DestroyArtifact`, `Special::DestroyLand`, `Special::DestroyNonblackCreature`, `EffectOp::PumpAllUntilEndOfTurn`, `EffectOp::DealDamageToControllerOfTarget`, `EffectOp::TransformSourceInPlace`, `EffectOp::LookAtTopMayRevealThen`, `EffectCond::ControllerGraveyardCreatureCardsAtLeast`, `AltCostDef`, `AltCostCondition`, `GrantedActivatedAbilityDef`, `AdventureDef`, `Keywords::CANT_BE_BLOCKED`, `TargetSpec::LandPermanent`, `TargetSpec::NonblackCreature`, `PermanentFilter::Land`, `card_id_by_visible_name`, `NativeRunCatalogProfileV1::PauperMetaW1`, `REGISTRATION_SPECS`, `pauperRegistrationsV2`.
