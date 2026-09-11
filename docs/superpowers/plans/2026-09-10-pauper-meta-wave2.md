# Pauper Meta Wave 2 Implementation Plan (Urzatron)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Revision 4** (2026-09-10, final before execution): re-review round 2 confirmed all 16 earlier items and found five Task 6 refinements, all engine-verified and all adopted here in design note B and Task 6 only. Two more exhaustive `SpellCastRouteV4` matches join the route-aware list (engine.rs 1577-1649 and 1894-1911, with the `def.cascade` versus `source.v4.cascade_grant` pitfall stated), a kicked cascade still pays kicker under CR 702.85, `ShufflePurposeV2::CascadeBottomOrder` gains its own per-player ordinal, and two citations are corrected. Task 6 goes from 10 to 12 hours.

**Revision 3** (2026-09-10, controller ruling): revision 2's new `CastMethodV4::Cascade` is withdrawn. A new variant would force an edit to `flat_policy_v2.rs` (its `cast_method_id` match is exhaustive), and the frozen encoder is never edited by a card wave, nor may a card wave change the model input without a separate owner ruling. Cascade's free cast now reuses `CastMethodV4::Normal` on a dedicated `SpellCastRouteV4::CascadeExile`, exactly as wave 1's adventure-from-exile cast does; the route and the `cascade_grant` marker are unchanged from revision 2, the model-input migration is deleted, and Task 6 drops from 12 to 10 hours. Design note B carries the spike evidence.

**Revision 2** (2026-09-10): panel review round 1 adopted in full (one Critical, nine Important, two Minor). The Critical was cascade's cast route, which revision 1 routed through `CastMethodV4::Plotted`.

**Goal:** Register Urzatron (9 percent of the sampled Pauper field, no kernel deck today) as the nineteenth pool registration from the pinned mtgtop8 list 888074, implement the 19 cards it needs plus three shared mechanics (cascade, prototype, station) and the Tron conditional mana rule, and gate the wave with the kernel-side branch-coverage parity check, without changing the active runtime deck set.

**Architecture:** The pool catalog (`data/pauper_pool_v1.json`) grows from eighteen to nineteen registrations while the runtime catalog (`data/runtime_decks_v1.json`, SHA-256 `68e7602f3a4df6217119406973954630800c358a10fca9f28e6cf9f20fd3b851`) stays byte-identical. Cards append to `data/cards_v1.json` (ids are array indexes: 184 definitions today, 203 after this wave, of which 190 are non-token). Card programs follow the existing build.rs tables (`special_for`, `effect_recipe_for`, `trigger_recipe_for`, `activated_ability_recipes_for`, `mana_ability_def_for`, `additional_mana_abilities_for`) plus four new card-neutral mechanic families: conditional multi-mana land yield (Tron), cascade, prototype, and station. Every wave commit records old and new `KERNEL_CARDDB_HASH` (wave 1 finished at `0xde59_c501_e943_f3fd`).

**Tech Stack:** Rust 2021 (mtg-kernel crate, build.rs codegen), Python 3 tools under `python/tools` run through `uvx uv@0.11.29 run --no-sync python ...`, XMage Java sources in the Mage fork `C:\Users\Jack\IdeaProjects\mage-cycle4-lead` (pinned `72a08a3b`) as the rules oracle.

**Spec:** `docs/superpowers/specs/2026-09-09-pauper-meta-cards-and-sideboarding-design.md` (revision 3; rulings 1, 2, 5, 6, 7 RULED, ruling 3 superseded, ruling 4 deferred; section 5.3 step 4 carries the 2026-09-10 amendment that a wave's parity evidence is kernel-side until the shadow harness is generalized). Research inputs: `docs/research/pauper_meta_source_lists_2026-09-09.json`, `docs/research/pauper_meta_gap_after_w1_2026-09.json`, and the preserved decklists under `docs/research/pauper_meta_decklists_2026-09-09/`. Predecessor: `docs/superpowers/plans/2026-09-09-pauper-meta-wave1.md` and its ledger `.superpowers/sdd/2026-09-09-pauper-meta-wave1/progress.md`.

## Global Constraints

- Branch `lead/pauper-meta-cards-v1` only; never commit to `lead/cycle4-refresh-manifest-v1` or `main`. Push after every commit.
- Registry identity is append-only: never delete or reorder entries in `data/cards_v1.json`. The same rule binds `card_def::Subtype` (its `stable_id()` is `self as u16` and its doc says "existing discriminants are append-only"), so new subtypes are appended at the end of the enum, after `Fish`, never inserted alphabetically.
- `data/runtime_decks_v1.json` must not change in this wave (SHA-256 `68e7602f3a4df6217119406973954630800c358a10fca9f28e6cf9f20fd3b851`, pinned by the training store). Verify with `sha256sum data/runtime_decks_v1.json` before every commit that touches `data/`.
- Historical deck registrations (the nine originals and the nine V2 registrations) and their `.dek` files never change.
- No card-specific pending state (ROADMAP line 175). New primitives are card-neutral `EffectOp`, `CostComponent`, `DynamicValueDef`, trigger, or definition shapes with their own unit tests.
- Every new card's integration test file header cites the XMage Java file and its git blob SHA from the Mage fork at `72a08a3b` (this plan's "Card metadata" section lists all of them; re-derive with `git -C C:\Users\Jack\IdeaProjects\mage-cycle4-lead rev-parse 72a08a3b:Mage.Sets/src/mage/cards/<letter>/<Class>.java`).
- A card's `engine_capability` may be `full` in the candidate build so manifests generate, but the wave is not certified until Task 12's branch-coverage check passes (spec 5.3 steps 2 and 4).
- No em-dashes in any file, comment, or commit message.
- Python entry point: `uvx uv@0.11.29 run --no-sync python python/tools/<tool>.py` from the worktree root.
- Commit message bodies for registry or deck changes list: old and new `KERNEL_CARDDB_HASH`, pool totals, and each new deck's source SHA-256.
- Every task runs `cargo test --locked -p mtg-kernel --no-run` (compile all test targets) before committing, in addition to its focused tests.
- Cargo runs in the foreground, one command at a time, with `CARGO_TARGET_DIR=E:/cargo-target-pauper-meta`, `TEMP=E:/tmp/lead`, `TMP=E:/tmp/lead`. The canonical prefix for every cargo line in this plan is:
  `CARGO_TARGET_DIR=E:/cargo-target-pauper-meta TEMP=E:/tmp/lead TMP=E:/tmp/lead cargo ...`
- No full library sweeps except the single one at Task 11 (identity finalisation). Every other task runs filtered runs by test name (`--test <file> <name>`, `--lib <module>::<name>`) plus the Python golden suites after any re-pin.
- Every new decision shape needs an `rl.rs` candidate arm (`legal_action_candidates_v1`, around lines 2180 to 2390), acceptance in BOTH `rl_session.rs` validators (`flat_validate_origin_decision_v1` at 1615 and `flat_validate_semantic_policy_pair_v1` at 2380), and a test through `flat_build_action_cache_v2` (3211), before the card reaches a registered deck. This is the wave 1 lesson from commits `30d2c20d` and `8cdda534`: a decision shape that only the engine understands aborts the live flat-encoded session as soon as its card is in a registered deck.
- `mtg-kernel/src/flat_policy_v1.rs` and `mtg-kernel/src/flat_policy_v2.rs` are never modified by a card wave (the frozen encoder); no model-input schema change (enum mapping versions, `python/mtg_kernel_rl/features.py`, flat goldens) lands without a separate owner ruling naming that change. Regenerating `data/flat_policy_v1/*` and `data/flat_policy_v2/*` after a card DB re-pin is not such a change: those generators rewrite data whose content moved because the registry grew, and the encoder source is untouched.
- Every new persisted field (`ObjectStateV4`, `Counters`, `PendingOptionalCost` and friends) carries `#[serde(default)]` and a pre-wave fixture test proving a snapshot written before this wave still deserializes.
- sha256 sidecars are computed over canonical LF bytes (`git show HEAD:<path> | sha256sum`), never over a Windows working copy (wave 1 harness-lane defect, fixed in `558535bb`).
- Each card task writes `covers:` annotations for every declared rules branch and adds the card's branch list to `data/wave_branch_manifests/pauper_meta_w2.json` as it lands, so the parity task (Task 12) only runs the checker and the one-seed rerun.
- State-hash literal re-pins are allowed only with the cause recorded per literal (the wave 1 directive: name the exact mechanism, for instance "`Counters` gained `charge`, which is hashed into `state_hash`", not a generic "card DB moved").
- Batch same-shape cards into one task where they share a primitive. This plan already does so; do not split a task's cards across commits unless a review requires it.

---

## Wave 2 registration (pinned source 75)

One new registration. Urzatron has no historical kernel registration, so this is a new deck id, not a revision: `Urzatron`, file `Deck - Urzatron.dek`. Ids are identifiers without underscores (the pool validator accepts `CawGates`, so CamelCase is the convention).

| id | source | file under docs/research/pauper_meta_decklists_2026-09-09 | substitutions | new cards |
|---|---|---|---|---|
| Urzatron | mtgtop8 888074 | `Urzatron__888074.txt` | Giant's Boulder x4 to Malevolent Rumble x4 (main); Call Damage Control x2 to Blue Elemental Blast x2 (side). Both are XMage-absent at `72a08a3b` (spec section 3). | 19 (listed below) |

### The pinned 75 after substitution

Mainboard (60): 4 Urza's Tower, 4 Urza's Power Plant, 4 Urza's Mine, 2 Conduit Pylons, 1 Bojuka Bog, 3 Forest, 4 Expedition Map, 4 Ancient Stirrings, 3 Crop Rotation, 2 Bonder's Ornament, 4 Barrels of Blasting Jelly, 4 Malevolent Rumble (substituted), 2 Unfathomable Truths, 4 Bramble Wurm, 3 Generous Ent, 4 Boulderbranch Golem, 4 Maelstrom Colossus, 4 Pinnacle Kill-Ship.

Sideboard (15): 4 Breath Weapon, 4 Relic of Progenitus, 2 Blue Elemental Blast (substituted), 2 Earth Rift, 2 Tangle, 1 Rooftop Percher.

Already in the registry and reused unchanged: Forest, Generous Ent, Breath Weapon, Relic of Progenitus, Blue Elemental Blast. The manifest generator rewrites their `decks` membership; do not hand-edit it.

### Substitution disclosure (spec section 3: "the sampled next-most-common card in that slot")

- **Giant's Boulder x4 to Malevolent Rumble x4.** Giant's Boulder is absent from the Mage fork at `72a08a3b` and appears at 4 copies in all eight sampled Urzatron lists, so no in-list replacement exists. Ranking the archetype's sampled mainboard names that are *not* already in the pinned 75 (`docs/research/pauper_meta_gap_after_w1_2026-09.json`, `archetypes.Urzatron.cards`, unconditional average copies across the eight sampled decks): Malevolent Rumble 1.50, Nyxborn Hydra 1.38, Prophetic Prism 0.75, Plunder the Trollshaws 0.50, Visionary's Dance 0.50, Warped Tusker 0.38. Malevolent Rumble is the next-most-common, is present at 4 copies in three of the eight lists (887158, 888069, 888076), and keeps the deck's green shell (it is a `{1}{G}` sorcery and the deck already runs Ancient Stirrings and Crop Rotation on 3 Forest plus 2 Conduit Pylons). Runner-up disclosure: Nyxborn Hydra is already registered and would have cost zero new cards, but frequency governs the choice, not implementation cost. Malevolent Rumble also creates the Eldrazi Spawn Token, which is already in the registry (Writhing Chrysalis), so it adds no token.
- **Call Damage Control x2 to Blue Elemental Blast x2.** Call Damage Control is absent from the fork. Ranking sampled Urzatron sideboard names not already in the pinned sideboard, by total copies across the eight sampled sideboards: Blue Elemental Blast 11 (887158 x4, 887775 x4, 888076 x3), Hydroblast 8 (887775 x1, 887930 x4, 888069 x3), Ancient Grudge 5, Scour from Existence 5, Red Elemental Blast 4, Fiery Cannonade 4. Blue Elemental Blast is the next-most-common, is already registered (`sideboard_only: true`, in eight decks), and fills the same anti-red slot the sampled Urzatron sideboards use alongside Breath Weapon and Hydroblast.
- Both substituted names stay on the MTGO unknown-card fail-closed list; nothing in this wave claims those two names resolve.

### Not registered in this wave

Prophetic Prism is in the spec's W2 candidate list (section 5.1) but is not in the pinned 888074 list, and `build.rs` panics on any non-token registry card whose `decks` list is empty (`c.decks.is_empty() && !c.is_token`, build.rs:1892), so a card that belongs to no pool deck cannot be registered (spec section 3). It is therefore deferred to a wave whose pinned list contains it (887775 plays 4, 888069 plays 2). Record this in the wave record; it is a deliberate consequence of the pin-one-exact-75 rule, not an omission.

---

## Card metadata (from the Mage fork at 72a08a3b)

Each entry is the exact JSON object to append to `data/cards_v1.json` `cards` (the `decks` list is rewritten by the manifest generator; give the initial value shown). `java_file` paths are relative to the Mage fork. Append in task order (Task 2 first, Task 9 last).

```json
{"name": "Urza's Tower", "engine_capability": "full", "mana_cost": "", "mana_value": 0, "colors": [], "types": ["Land"], "subtypes": ["Urza's", "Tower"], "supertypes": [], "power": null, "toughness": null, "is_land": true, "produces_mana": ["C"], "decks": ["Deck - Urzatron.dek"], "sideboard_only": false, "complexity": "moderate", "mechanics": ["mana_ability", "colorless_mana", "conditional_mana"], "java_file": "Mage.Sets/src/mage/cards/u/UrzasTower.java"}
{"name": "Urza's Power Plant", "engine_capability": "full", "mana_cost": "", "mana_value": 0, "colors": [], "types": ["Land"], "subtypes": ["Urza's", "Power-Plant"], "supertypes": [], "power": null, "toughness": null, "is_land": true, "produces_mana": ["C"], "decks": ["Deck - Urzatron.dek"], "sideboard_only": false, "complexity": "moderate", "mechanics": ["mana_ability", "colorless_mana", "conditional_mana"], "java_file": "Mage.Sets/src/mage/cards/u/UrzasPowerPlant.java"}
{"name": "Urza's Mine", "engine_capability": "full", "mana_cost": "", "mana_value": 0, "colors": [], "types": ["Land"], "subtypes": ["Urza's", "Mine"], "supertypes": [], "power": null, "toughness": null, "is_land": true, "produces_mana": ["C"], "decks": ["Deck - Urzatron.dek"], "sideboard_only": false, "complexity": "moderate", "mechanics": ["mana_ability", "colorless_mana", "conditional_mana"], "java_file": "Mage.Sets/src/mage/cards/u/UrzasMine.java"}
{"name": "Bojuka Bog", "engine_capability": "full", "mana_cost": "", "mana_value": 0, "colors": [], "types": ["Land"], "subtypes": [], "supertypes": [], "power": null, "toughness": null, "is_land": true, "produces_mana": ["B"], "decks": ["Deck - Urzatron.dek"], "sideboard_only": false, "complexity": "moderate", "mechanics": ["mana_ability", "enters_tapped", "etb_trigger", "target_player", "graveyard_hate"], "java_file": "Mage.Sets/src/mage/cards/b/BojukaBog.java"}
{"name": "Conduit Pylons", "engine_capability": "full", "mana_cost": "", "mana_value": 0, "colors": [], "types": ["Land"], "subtypes": ["Desert"], "supertypes": [], "power": null, "toughness": null, "is_land": true, "produces_mana": ["C", "W", "U", "B", "R", "G"], "decks": ["Deck - Urzatron.dek"], "sideboard_only": false, "complexity": "moderate", "mechanics": ["mana_ability", "etb_trigger", "surveil", "tap_ability", "activated_ability"], "java_file": "Mage.Sets/src/mage/cards/c/ConduitPylons.java"}
{"name": "Expedition Map", "engine_capability": "full", "mana_cost": "{1}", "mana_value": 1, "colors": [], "types": ["Artifact"], "subtypes": [], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": [], "decks": ["Deck - Urzatron.dek"], "sideboard_only": false, "complexity": "moderate", "mechanics": ["activated_ability", "tap_ability", "sacrifice_cost", "fetch_effect"], "java_file": "Mage.Sets/src/mage/cards/e/ExpeditionMap.java"}
{"name": "Bonder's Ornament", "engine_capability": "full", "mana_cost": "{3}", "mana_value": 3, "colors": [], "types": ["Artifact"], "subtypes": [], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": ["W", "U", "B", "R", "G"], "decks": ["Deck - Urzatron.dek"], "sideboard_only": false, "complexity": "moderate", "mechanics": ["mana_ability", "any_color_mana", "activated_ability", "tap_ability", "draw_card"], "java_file": "Mage.Sets/src/mage/cards/b/BondersOrnament.java"}
{"name": "Barrels of Blasting Jelly", "engine_capability": "full", "mana_cost": "{1}", "mana_value": 1, "colors": [], "types": ["Artifact"], "subtypes": [], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": ["W", "U", "B", "R", "G"], "decks": ["Deck - Urzatron.dek"], "sideboard_only": false, "complexity": "moderate", "mechanics": ["mana_ability", "any_color_mana", "activated_ability", "tap_ability", "sacrifice_cost", "damage"], "java_file": "Mage.Sets/src/mage/cards/b/BarrelsOfBlastingJelly.java"}
{"name": "Ancient Stirrings", "engine_capability": "full", "mana_cost": "{G}", "mana_value": 1, "colors": ["G"], "types": ["Sorcery"], "subtypes": [], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": [], "decks": ["Deck - Urzatron.dek"], "sideboard_only": false, "complexity": "moderate", "mechanics": ["card_selection", "reveal"], "java_file": "Mage.Sets/src/mage/cards/a/AncientStirrings.java"}
{"name": "Malevolent Rumble", "engine_capability": "full", "mana_cost": "{1}{G}", "mana_value": 2, "colors": ["G"], "types": ["Sorcery"], "subtypes": [], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": [], "decks": ["Deck - Urzatron.dek"], "sideboard_only": false, "complexity": "moderate", "mechanics": ["card_selection", "reveal", "mill", "token_creation"], "java_file": "Mage.Sets/src/mage/cards/m/MalevolentRumble.java"}
{"name": "Crop Rotation", "engine_capability": "full", "mana_cost": "{G}", "mana_value": 1, "colors": ["G"], "types": ["Instant"], "subtypes": [], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": [], "decks": ["Deck - Urzatron.dek"], "sideboard_only": false, "complexity": "moderate", "mechanics": ["sacrifice_cost", "fetch_effect"], "java_file": "Mage.Sets/src/mage/cards/c/CropRotation.java"}
{"name": "Bramble Wurm", "engine_capability": "full", "mana_cost": "{6}{G}", "mana_value": 7, "colors": ["G"], "types": ["Creature"], "subtypes": ["Wurm"], "supertypes": [], "power": 7, "toughness": 6, "is_land": false, "produces_mana": [], "decks": ["Deck - Urzatron.dek"], "sideboard_only": false, "complexity": "moderate", "mechanics": ["reach", "trample", "etb_trigger", "life_gain", "graveyard_activated_ability", "exile_self_cost"], "java_file": "Mage.Sets/src/mage/cards/b/BrambleWurm.java"}
{"name": "Unfathomable Truths", "engine_capability": "full", "mana_cost": "{4}{U}", "mana_value": 5, "colors": [], "types": ["Instant"], "subtypes": [], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": [], "decks": ["Deck - Urzatron.dek"], "sideboard_only": false, "complexity": "simple", "mechanics": ["devoid", "draw_card", "token_creation"], "java_file": "Mage.Sets/src/mage/cards/u/UnfathomableTruths.java"}
{"name": "Maelstrom Colossus", "engine_capability": "full", "mana_cost": "{8}", "mana_value": 8, "colors": [], "types": ["Artifact", "Creature"], "subtypes": ["Golem"], "supertypes": [], "power": 7, "toughness": 7, "is_land": false, "produces_mana": [], "decks": ["Deck - Urzatron.dek"], "sideboard_only": false, "complexity": "complex", "mechanics": ["cascade"], "java_file": "Mage.Sets/src/mage/cards/m/MaelstromColossus.java"}
{"name": "Boulderbranch Golem", "engine_capability": "full", "mana_cost": "{7}", "mana_value": 7, "colors": [], "types": ["Artifact", "Creature"], "subtypes": ["Golem"], "supertypes": [], "power": 6, "toughness": 5, "is_land": false, "produces_mana": [], "decks": ["Deck - Urzatron.dek"], "sideboard_only": false, "complexity": "complex", "mechanics": ["prototype", "etb_trigger", "life_gain"], "java_file": "Mage.Sets/src/mage/cards/b/BoulderbranchGolem.java"}
{"name": "Pinnacle Kill-Ship", "engine_capability": "full", "mana_cost": "{7}", "mana_value": 7, "colors": [], "types": ["Artifact"], "subtypes": ["Spacecraft"], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": [], "decks": ["Deck - Urzatron.dek"], "sideboard_only": false, "complexity": "complex", "mechanics": ["station", "etb_trigger", "damage", "flying"], "java_file": "Mage.Sets/src/mage/cards/p/PinnacleKillShip.java"}
{"name": "Earth Rift", "engine_capability": "full", "mana_cost": "{3}{R}", "mana_value": 4, "colors": ["R"], "types": ["Sorcery"], "subtypes": [], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": [], "decks": ["Deck - Urzatron.dek"], "sideboard_only": true, "complexity": "simple", "mechanics": ["destroy_target", "land_destruction", "flashback"], "java_file": "Mage.Sets/src/mage/cards/e/EarthRift.java"}
{"name": "Rooftop Percher", "engine_capability": "full", "mana_cost": "{5}", "mana_value": 5, "colors": [], "types": ["Creature"], "subtypes": ["Shapeshifter"], "supertypes": [], "power": 3, "toughness": 3, "is_land": false, "produces_mana": [], "decks": ["Deck - Urzatron.dek"], "sideboard_only": true, "complexity": "moderate", "mechanics": ["changeling", "flying", "etb_trigger", "up_to_two_targets", "graveyard_target", "exile", "life_gain"], "java_file": "Mage.Sets/src/mage/cards/r/RooftopPercher.java"}
{"name": "Tangle", "engine_capability": "full", "mana_cost": "{1}{G}", "mana_value": 2, "colors": ["G"], "types": ["Instant"], "subtypes": [], "supertypes": [], "power": null, "toughness": null, "is_land": false, "produces_mana": [], "decks": ["Deck - Urzatron.dek"], "sideboard_only": true, "complexity": "moderate", "mechanics": ["damage_prevention", "skip_untap", "fog"], "java_file": "Mage.Sets/src/mage/cards/t/Tangle.java"}
```

Rules text used by the tests, verified in the Java at `72a08a3b` (blob SHAs in the same order as the JSON above):

| card | blob SHA |
|---|---|
| Urza's Tower | `33660e89813db8b1a2d5938c2d4fddee78c8431e` |
| Urza's Power Plant | `1ed40cbcc9c0d70dbba0fe7c8242d50247cb35ad` |
| Urza's Mine | `f4b4470351a99866a29f4bfb47122caeb798e1e2` |
| Bojuka Bog | `681976c4d054c43804426a99ffe9bde19c9bc82d` |
| Conduit Pylons | `abedb87ee8d4f313bfda191fa88b9ff7088507fe` |
| Expedition Map | `dc7a64ba4e8f8473eec8d38d77f53626fb662884` |
| Bonder's Ornament | `8e36188ee81e41f2c15ef02290234261229d77fe` |
| Barrels of Blasting Jelly | `9fdd9763dbbb2bfeab2ddc4f81b82540a3a64c16` |
| Ancient Stirrings | `f9db9656d00ec52b73d2e8188a27fa2391ed3a33` |
| Malevolent Rumble | `31831ce9ba48413aaaaaf713a977ad29d684fdb6` |
| Crop Rotation | `a5dbe4537f05e6677c6c8069c2f8d2532b08df36` |
| Bramble Wurm | `0342c37f7ecc5f71b91fc8f28a5b4ccb37e879e2` |
| Unfathomable Truths | `8f39e0aa4e247b51784a84c6d28f3378e0891618` |
| Maelstrom Colossus | `e1264ffc89857abb8ff65cd526fbd75177504992` |
| Boulderbranch Golem | `0cc82e907a734b45d88aa795743a4e1f013538c1` |
| Pinnacle Kill-Ship | `319a63286f86797a1a06eb00b43802c02c9d35f7` |
| Earth Rift | `1b15ddeec7eb953c662b45d642fbbd41fc0f377f` |
| Rooftop Percher | `94ebb64507530048bae92f50b74611694f6b0d61` |
| Tangle | `ded22eb382a7d0645fbb46cc458ce9b528ea98e0` |

Shared ability sources (cite these in the mechanic tasks): `Mage/src/main/java/mage/abilities/keyword/CascadeAbility.java` blob `45a4ee8634164bde1da98f4f5accfec6d0c89169`; `PrototypeAbility.java` blob `643b93b0997460ccb661074399ea483142351105`; `StationAbility.java` blob `e918dc283d2531599b9b33673a9fd2c3227219e6`; `StationLevelAbility.java` blob `a021b34bd793bed65bdd84e3be11b7b9b78ae109`; `Mage/src/main/java/mage/abilities/dynamicvalue/common/UrzaTerrainValue.java` blob `57fa2b3f0ce2f8c8e60d68e2dfe9561e2578d71d`.

- Urza's Tower: Land, Urza's Tower. `{T}`: Add `{C}`. If you control an Urza's Mine and an Urza's Power-Plant, add `{C}{C}{C}` instead. (`UrzaTerrainValue.TOWER`, value 3.)
- Urza's Power Plant: Land, Urza's Power-Plant. `{T}`: Add `{C}`. If you control an Urza's Mine and an Urza's Tower, add `{C}{C}` instead. (value 2)
- Urza's Mine: Land, Urza's Mine. `{T}`: Add `{C}`. If you control an Urza's Power-Plant and an Urza's Tower, add `{C}{C}` instead. (value 2)
- `UrzaTerrainValue.calculate` returns 1 unless the controller controls at least one permanent of each of the two *other* pieces; each piece filter is "controlled permanent with the piece subtype AND the Urza's subtype". The source itself is not counted for its own piece, so a second copy of the same piece does not assemble Tron.
- Bojuka Bog: enters tapped; when it enters, exile all cards from target player's graveyard; `{T}`: add `{B}`.
- Conduit Pylons: Land, Desert. When it enters, surveil 1. `{T}`: Add `{C}`. `{1}`, `{T}`: Add one mana of any color.
- Expedition Map: `{2}`, `{T}`, Sacrifice this: search your library for a land card, reveal it, put it into your hand, then shuffle.
- Bonder's Ornament: `{T}`: Add one mana of any color. `{4}`, `{T}`: Each player who controls a permanent named Bonder's Ornament draws a card. (In a two-player game with one owner of the name this draws exactly one card for that player; the opponent draws only if they also control one. XMage iterates `getPlayersInRange` and checks each player's own battlefield by name.)
- Barrels of Blasting Jelly: `{1}`: Add one mana of any color. Activate only once each turn. `{5}`, `{T}`, Sacrifice this artifact: it deals 5 damage to target creature. (The mana ability has no tap symbol.)
- Ancient Stirrings: Sorcery `{G}`. Look at the top five cards of your library. You may reveal a colorless card from among them and put it into your hand. Put the rest on the bottom of your library in any order. (`LookLibraryAndPickControllerEffect(5, 1, colorless, HAND, BOTTOM_ANY)`; "colorless" is the card's color, so lands and generic-cost artifacts qualify.)
- Malevolent Rumble: Sorcery `{1}{G}`. Reveal the top four cards of your library. You may put a permanent card from among them into your hand. Put the rest into your graveyard. Create a 0/1 colorless Eldrazi Spawn token with "Sacrifice this creature: Add `{C}`."
- Crop Rotation: Instant `{G}`. Additional cost: sacrifice a land. Search your library for a land card and put it onto the battlefield (untapped), then shuffle.
- Bramble Wurm: `{6}{G}` 7/6 Wurm with reach and trample. When it enters, you gain 5 life. `{2}{G}`, Exile this card from your graveyard: you gain 5 life.
- Unfathomable Truths: Instant `{4}{U}` with devoid (colorless). Draw three cards and create a 0/1 colorless Eldrazi Spawn token.
- Maelstrom Colossus: `{8}` 7/7 Artifact Creature Golem with cascade.
- Boulderbranch Golem: `{7}` 6/5 Artifact Creature Golem. Prototype `{3}{G}` 3/3. When it enters, you gain life equal to its power (6 when cast normally, 3 when prototyped; `SourcePermanentPowerValue.NOT_NEGATIVE`).
- Pinnacle Kill-Ship: `{7}` Artifact Spacecraft. When it enters, it deals 10 damage to up to one target creature. Station (tap another untapped creature you control: put charge counters equal to its power on this; sorcery speed only). STATION 7+: flying, 7/7 (it becomes a creature while it has 7 or more charge counters).
- Earth Rift: Sorcery `{3}{R}`. Destroy target land. Flashback `{5}{R}{R}`.
- Rooftop Percher: `{5}` 3/3 Shapeshifter with changeling and flying. When it enters, exile up to two target cards from graveyards. You gain 3 life. (The life gain is a second effect of the same trigger and happens even with zero targets chosen.)
- Tangle: Instant `{1}{G}`. Prevent all combat damage that would be dealt this turn. Each attacking creature does not untap during its controller's next untap step. (The skip applies to attacking creatures at resolution, whoever controls them.)

---

## New mechanics and primitives (priced once here, per spec 5.2)

**A. Tron conditional mana yield (Task 2).** Today `mana::ManaSource` assumes one tap yields exactly one mana and `PaymentPlan::taps` is `Vec<(ObjectId, ManaColor)>`, so a land that adds two or three mana has no representation on the automatic payment path. The alternative of giving the Tron lands a `ManaAbilityDef` would exclude them from `is_automatic_payment_mana_source` (which requires `mana_ability_def.is_none()`), forcing the agent to float mana explicitly before every spell for twelve of the deck's lands, which changes the whole deck's action surface. The chosen design instead teaches the planner about per-tap yield:
- `card_def::DynamicValueDef::AmountIfControllerControlsEach { required: [SubtypeConjunctionDef; 2], amount_when_met: u8, amount_otherwise: u8 }` where `SubtypeConjunctionDef { first: Subtype, second: Subtype }` (a single permanent must carry both). Fixed-size arrays keep the enum `Copy`, `Serialize`, and `Deserialize`. Evaluated by the existing `engine::evaluate_dynamic_value` (engine.rs 2994).
- `CardDef::conditional_tap_yield: Option<DynamicValueDef>`, generated by a new `build.rs` `conditional_tap_yield_for(name)`. It does not set `mana_ability_def`, so the lands remain automatic payment sources.
- `mana::ManaSource` gains `yield_per_tap: u8` (1 for every existing source). `mana::solve_pips` and `mana::pay_generic` credit `yield_per_tap` per tap: one mana pays the current pip or generic unit, the remaining `yield_per_tap - 1` are added to `pool_remaining` for later pips/generic and recorded in a new `PaymentPlan::surplus: [u8; 6]`. Backtracking undoes both.
- `engine::pay_plan` (engine.rs 14112) adds each tap's full yield to the pool, then adds `surplus`, then subtracts the consumed amounts, so unspent Tron mana floats exactly as the rules require.
- The explicit activation path uses a new `EffectOp::AddManaDynamic { player, color, amount: DynamicValueDef }` so a hand-activated Urza's Tower adds the same 1 or 3.
- No new decision shape and no new action kind: the land is still `Action::ActivateManaAbility(id)` with one color, and payment plans are opaque to `rl.rs` (wave 1 Task 10 established this for delve).

**B. Cascade (Task 6).** `CardDef::cascade: bool`. The trigger reuses the existing `cast_self` trigger kind (precedent: `"Weather the Storm" => "cast_self:storm_copies:frozen_turn_cast_count"`), so cascade resolves above the spell that cast it. Its program is one card-neutral op, `EffectOp::CascadeFromSource`, with no parameters: the comparison value is the resolving source's own mana value, read at resolution. The cast route below was settled by a read-only engine spike on 2026-09-10 (review round 1 Critical); every claim here is cited by file and line.
1. Exile cards from the top of the controller's library one at a time (a separate zone change per card, matching `CascadeEffect`'s sequential `moveCards`) until a nonland card with mana value strictly less than the source's is exiled, or the library is empty.
2. If such a card was exiled, offer `Decision::ChooseEffectBoolean` with a new `effect::EffectBooleanChoicePurpose::CascadeFreeCast`. That fine-grained purpose maps down to `rl::BooleanChoicePurposeV4::OptionalEffect` (rl.rs 606-610), which is the value actually projected (`flat_policy_v2::boolean_purpose_id`, flat_policy_v2.rs 1338-1342, mirrored by `python/mtg_kernel_rl/features.py:117`), so the decision needs no new projected enum value: `core_surface_action_candidates_v1`'s `ChooseEffectBoolean` arm emits exactly `[false, true]` for every purpose unconditionally, which rl.rs 6979-6986 documents for Delver of Secrets.
3. On accept, cast that card from exile for no mana as an ordinary `CastMethodV4::Normal` cast on a dedicated new route, `SpellCastRouteV4::CascadeExile`. No cast method is added, so the frozen encoder is untouched (Global Constraints) and a cascaded spell encodes exactly like any other normal cast.
   *Spike result 1, why not `CastMethodV4::Plotted` (revision 1's route).* Plotted is closed to any card without a Plot ability at four independent points: `begin_cast_ex`'s `debug_assert!(matches!(forced_cast_method, None | Some(CastMethodV4::Madness)))` (engine.rs 13188-13191) forbids forcing Plotted at all; `is_plotted` (engine.rs 13205-13209) requires `def.plot_cost.is_some()` and `plotted_turn < state.turn`; the route construction `SpellCastRouteV4::Plotted { plotted_turn: plotted_turn.expect(...) }` (engine.rs 13233-13236) panics on `None`; and both `validate_spell_source_contract_fields` (engine.rs 1774-1780) and the pending-cast validator (engine.rs 6825-6836) re-check `def.plot_cost.is_some()` plus the marker on every departure from the stack.
   *Spike result 2, why not `CastMethodV4::Alternative` (revision 3's preferred candidate).* Alternative reads truthfully in English but is definition-owned in this engine, and reusing it would break an invariant three validators rely on and misreport a player choice: (a) a staged Alternative is structurally illegal, because the pending-cast validator rejects it outright with "pending cast stamped its selected cast method before finalization" (engine.rs 6798-6801); Alternative arises only at finalization from `pending.cast_mode == Some(CastMode::Alternative)` (`finalized_cast_method`, engine.rs 6042-6060). (b) Reaching it therefore means staging `cast_mode = Some(CastMode::Alternative)` at engine.rs 13260-13263, and `CastMode` is itself projected to the model (`cast_mode_id`, flat_policy_v2.rs 1236-1241, `Alternative => 2`) and is the value `rl.rs`'s `ChooseCastMode` candidate arm (2203) exists to produce, so the model would be told the caster chose an alternative cost that was never offered. (c) Three sites assert the definition-owned cost that a cascaded card does not have: `validate_spell_source_contract_fields` (engine.rs 1762, `Alternative if def.alt_cost.is_some()`, rejecting at 1775-1779 otherwise), `remaining_cast_payment_is_payable` (engine.rs 5989, its `Alternative` arm at 6009-6013, `def.alt_cost.is_some_and(...)`), and the payment match (engine.rs 13546-13560), whose first line is `def.alt_cost.expect("validated alternative cast has a definition-owned cost")`, a panic for every cascaded card in this deck (none of the ten castable cascade hits in the pinned 75 has an `alt_cost`).
   *Spike result 3, why `CastMethodV4::Normal` is the precedent-matching answer.* `SpellCastRouteV4::AdventureExile` (state.rs 513-520) is documented as "Always `CastMethodV4::Normal`, deliberately distinct from `ExilePermission`: this permission is derived from the card's own incarnation-local flag rather than a separately granted/expiring `engine::PlayPermission`". Wave 1 built exactly this shape for Fang Dragon's creature face cast from exile, and the pending-cast validator's Normal arm already carries its clause `has_adventure_exile_permission = pending.origin_zone == Zone::Exile && source.v4.on_adventure` (engine.rs 6767-6786). Cascade is the same shape plus a mana waiver, so it adds one sibling clause rather than a new method. `forced_cast_method` stays `None`, so the debug_assert at 13188-13191 is untouched, and `validate_spell_source_contract_fields` needs no change at all because its `CastMethodV4::Normal => {}` arm (engine.rs 1761) passes unconditionally.
   *Chosen design.* Append `SpellCastRouteV4::CascadeExile` (state.rs 501-521), modeled verbatim on `AdventureExile`, and `ObjectStateV4::cascade_grant: bool` (`#[serde(default)]`, unobserved, like wave 1's `on_adventure`), set when the cascade effect exiles the candidate and restamped across the Exile to Stack move exactly where `is_plotted` and `is_adventure_exile` restamp theirs (engine.rs 13286-13300). Neither `SpellCastRouteV4` nor `ObjectStateV4::cascade_grant` appears in `rl.rs` or the flat modules, so both are free model-side.
   *The seven engine sites that become route-aware, all in engine.rs.* (a) 13224-13245, `begin_cast_ex`'s `cast_route` match: add `CastMethodV4::Normal if origin_zone == Zone::Exile && is_cascade_exile => SpellCastRouteV4::CascadeExile` immediately before the `ExilePermission` arm, mirroring the `is_adventure_exile` arm at 13240-13242, with the `is_cascade_exile` binding beside `is_adventure_exile` at 13209-13215. (b) 13260-13263, the cast-mode staging: stage `Some(CastMode::Normal)` unconditionally on the cascade route, so a cascaded card that happens to carry an `alt_cost` does not open a `ChooseCastMode` decision for a free cast. (c) 5998-6008, `remaining_cast_payment_is_payable`'s Normal arm (the function is at engine.rs 5989): on the cascade route the printed mana cost is waived, so an unkicked cast is payable without consulting `effective_normal_cast_cost`, and a kicked one is payable exactly when `def.kicker_cost` alone is payable. The `def.additional_cost` check that follows the match (engine.rs 6036-6039) still applies, which is rules-correct: "without paying its mana cost" waives the mana cost only. (d) 13501-13543, the payment match's Normal arm, which today binds `let kicked = pending.kicked == Some(true)` and `let normal_cost = effective_normal_cast_cost(...)`, then either `mana::can_pay_combined(&[&normal_cost, &kicker_cost], ...)` when kicked or `mana::can_pay(&normal_cost, ...)` when not (13524-13541). On the cascade route it skips `normal_cost` entirely: kicked pays `mana::can_pay(&kicker_cost, x_value, ...)` alone, unkicked pays nothing, and `was_kicked = kicked` is unchanged. CR 702.85 waives only the printed mana cost, so a kicked cascade still pays kicker. The `def.delve` branch at 13509-13523 is skipped on the cascade route as well: delve reduces the generic part of a cost that is no longer being paid (no delve card is cascade-reachable in the pinned 75, but the branch must not run). (e) 6767-6786, the pending-cast validator's Normal arm: add `has_cascade_exile_permission = pending.origin_zone == Zone::Exile && source.v4.cascade_grant` beside `has_adventure_exile_permission`. (f) 1577-1649, `validate_spell_source_contract_fields`'s `route_supports_method` match over `SpellCastRouteV4`, which is exhaustive and therefore forces a `CascadeExile` arm: `origin.origin_zone == Zone::Exile && source.owner == item.controller && cast_method == CastMethodV4::Normal && source.v4.cascade_grant`. Pitfall, and the reason this arm is called out rather than left to imitation: the neighbouring `AdventureExile` arm (1590-1595) ends in `def.adventure.is_some()`, but `def` here is the *cast* spell's own definition, which for a cascaded card is the cheap card, not the cascading one. Gating on `def.cascade` would reject every real cascade (and accept a hand-cast Maelstrom Colossus on a forged route); the evidence is the incarnation flag `source.v4.cascade_grant`, never a definition field. (g) 1894-1911, `storm_source_contract_is_structurally_valid`'s `route_is_valid` match, also exhaustive: `CascadeExile` joins the `GraveyardFlashback | Plotted { .. } | Madness | GraveyardEscape | AdventureExile => false` arm, since that helper is Weather the Storm specific and no cascaded spell is a storm source. Nothing outside engine.rs and state.rs changes, and no `CastMethodV4` match gains an arm.
4. Put every card still exiled this way on the bottom of the library in a random order with `environment_randomization_v2::shuffle_slice_in_place_v2` (environment_randomization_v2.rs:238) under a new `ShufflePurposeV2::CascadeBottomOrder` variant (the enum, its `as_str` and `parse`, and the `ENVIRONMENT_RANDOMIZATION_PURPOSES_V2` table at environment_randomization_v2.rs 151-171). The whole-library `state.rs:2066 shuffle_library` is the wrong shape, and revision 1's `grep -n "fn shuffle"` pointer matched nothing in engine.rs or effect.rs.
   *Ordinal source.* A purpose alone does not make two cascades in one game differ: the existing per-player counter `next_live_shuffle_ordinal` (environment_randomization_v2.rs:262) is hardwired to `InGameLibraryShuffle`, so deriving cascade's seed from it would either reuse one derived seed for every cascade trigger in a game or perturb the library-shuffle stream. `GameEnvironmentRandomizationV2` therefore gains its own `next_cascade_bottom_ordinal: [u64; 2]` with `#[serde(default)]` (the struct is `#[serde(deny_unknown_fields)]`, so a defaulted new field is the backward-compatible shape: old snapshots omit it and read `[0, 0]`), plus a getter and a `set_cascade_bottom_ordinal` commit that mirror `next_live_shuffle_ordinal`/`set_live_shuffle_ordinal` exactly, including the preflight that rejects `u64::MAX` before derivation rather than arithmetic at commit time. Each cascade bottoming preflights, derives its seed from (pair environment seed, purpose, owner, ordinal), then commits the successor.
The free spell's own targeting flows through the ordinary cast decisions. No card-specific pending state: the pending cascade carries the exiled-card list and the source binding in the generic effect continuation, the same way `LookAtTopMayRevealThen` does (wave 1 Task 9).

**C. Prototype (Task 7).** `CardDef::prototype: Option<PrototypeDef { cost: Cost, power: i16, toughness: i16, colors: &'static [ManaColor] }>`. The prototype cast is offered as `CastMode::Alternative` (no new `CastMode` value, so the `rl.rs` `ChooseCastMode` arm and the flat encoding are unchanged) with `CardDef::prototype` as the carrier that distinguishes it from Fireblast's alt cost, exactly as wave 1 distinguished an Adventure from an Omen under one `CastMethodV4::Omen` tag. The cost checks at engine.rs 1763 and 6009 currently require `def.alt_cost.is_some()` for `Alternative`; both gain `|| def.prototype.is_some()`, and the prototype branch pays `PrototypeDef::cost` at sorcery speed only. The resolving permanent carries `ObjectStateV4::prototyped: bool` (`#[serde(default)]`), set when the spell's `cast_method` is `Alternative` and the definition has a prototype, at the same seam where the battlefield incarnation is built (`v4.reset_for_zone_change` clears v4, so the flag is applied after it). `engine::effective_power`/`effective_toughness` use the prototype P/T as the base when the flag is set; `v4.effective_color_mask` is materialized from `PrototypeDef::colors`; mana-value and mana-cost readers use the prototype cost while the object is a spell or a prototyped permanent. Rejected alternative: modeling prototype as a `transform_face`. It would reuse `types_for_face`/`power_for_face`, but wave 1's blanket "no triggers while `face_index != 0`" gate would silence Boulderbranch Golem's ETB, face indexes 1 and 2 already mean "transform back face" and "adventure name" in `card_id_by_visible_name`, and a prototyped permanent keeps its printed name, so a face would add a name the projection must then suppress.

**D. Station (Task 8).** `CardDef::station: Option<StationDef { level: u8, power: i16, toughness: i16, keywords: Keywords }>` plus the activation itself:
- `Counters` gains `charge: i16` (`#[serde(default)]`, appended last). Every pinned `state_hash`/`diagnostic_state_hash` literal moves as a result; each re-pin records that cause.
- The station activation is one `ActivatedAbilityRecipe` with a new `AbilityCostRecipe::TapAnotherUntappedControlledCreature` (decision shape: the existing `Decision::ChooseCostTargets` with `CostKind::TapPermanents`, Heap Gate's precedent) and `sorcery_speed_only: true`. The tapped creature's power at cost-payment time is bound into the effect, which puts that many charge counters on the source (`EffectOp::PutChargeCountersOnSource { amount: DynamicValueDef::BoundCostTargetPower }`).
- The level static is read through the engine's existing effective-characteristic seams: `engine::object_has_type` (engine.rs 10188) already models a type change for an attached bestow aura, so the station arm goes next to it (a permanent with `station` and `counters.charge >= level` also has `CardType::Creature`); `effective_power`/`effective_toughness` set the station base P/T; `has_effective_keyword` grants the station keywords.
- Required audit: `CardType::Creature` appears 33 times in engine.rs and 24 times in effect.rs. Every site that asks "is this battlefield object a creature" must go through `object_has_type`, not `def.has_type`. The task enumerates them and fixes the ones that concern battlefield permanents; the bestow precedent bounds the work, since bestow already required that discipline.
- Fallback if the audit overruns (lead decision, not the implementer's): register Pinnacle Kill-Ship with `engine_capability: "partial"`, implement station counters and the ETB damage only, change `EXPECTED_MAINBOARD_SUPPORT["Urzatron"]` to `{"full": 56, "partial": 4, "no_effect": 0}`, and record the STATION 7+ level in the wave 2 branch manifest's `unreachable` map with the reason. Do not take this path silently.

**E. Smaller card-neutral shapes (assorted tasks).**
- `effect::LibraryCardFilter::AnyLand` (Expedition Map, Crop Rotation).
- `EffectOp::SearchLibraryToBattlefieldUntapped { player, filter }` beside the existing tapped form (Crop Rotation).
- `EffectOp::Surveil { player, count }` (Conduit Pylons), modeled on the existing scry path.
- `Special::LookTopSelectFilteredToHandRest { look, max, filter, rest }` with `LookSelectFilter::{Colorless, PermanentCard}` and `LookRest::{BottomAny, Graveyard}` (Ancient Stirrings, Malevolent Rumble). Appended beside `LookTopSelectByTypeToHandBottomRest`, which stays untouched so Lead the Stampede's program identity does not move.
- `ManaAbilityCostDef::None` with `max_activations_per_turn` (Barrels of Blasting Jelly's tapless once-per-turn mana ability).
- `EffectOp::EachPlayerControllingDefinitionDrawsCard { card_def: u16 }` (Bonder's Ornament; the "same name" contract is a definition id, as in `LibraryCardFilter::CardDefinition`).
- `CostComponent::ExileSourceFromGraveyard` and an `ActivatedAbilityDef` with `activation_zone: Zone::Graveyard` (Bramble Wurm; the zone field already exists and carries `Hand` for cycling).
- `DynamicValueDef::SourcePermanentPower` (Boulderbranch Golem's life gain).
- `EffectOp::InstallCombatDamagePreventionThisTurn` beside the existing `InstallDamagePreventionFromColor` (Tangle), plus reuse of `Special::TapAndSkipNextUntap`'s skip-untap machinery applied to every attacking creature.
- New subtypes appended after `Subtype::Fish`: `Urzas`, `Tower`, `PowerPlant`, `Mine`, `Desert`, `Spacecraft`, `Wurm`, `Golem`, with `build.rs::subtype_variant` mappings for the JSON strings `"Urza's"`, `"Tower"`, `"Power-Plant"`, `"Mine"`, `"Desert"`, `"Spacecraft"`, `"Wurm"`, `"Golem"`. Only `Wurm` and `Golem` are creature types; adding them to `Subtype::CREATURE_TYPES` changes the effective subtype set of every changeling object, so the changeling tests are re-run and the state-hash consequence recorded.

**Tokens and faces:** this wave adds no token and no new face. The Eldrazi Spawn Token already exists (Writhing Chrysalis); `TOKEN_DEPENDENCIES` in `generate_pauper_manifests.py` gains Malevolent Rumble and Unfathomable Truths as producers of it. `card_id_by_visible_name` needs no new entry, so the MTGO visible-name projection for this wave is exactly the 19 front names (spec 5.3 step 5).

**Catalog profile decision:** wave 2 does not add a fourth `NativeRunCatalogProfileV1` variant. `FROZEN_CARD_DB_HASH_U64_HEX_PAUPER_META_W1` is documented (and implemented in `repin_card_db_identity_v1.py`'s `TASK3_REWRITABLE_IDENTIFIERS_V1`) as tracking the card lane's live hash forward across waves, and no site renders its label string, so the repin tool moves it to the wave 2 hash as it does for any other pin site. Task 11 updates the variant's doc comment to say it denotes the lane's live identity rather than wave 1 specifically. The alternative, a `PauperMetaW2` variant with W1 frozen at `de59c501e943f3fd`, costs one enum variant, one constant, a four-way classifier, and a repin-tool change, and buys nothing while no store has been written under the wave 1 identity; if a store is ever written on this lane before the wave 2 identity lands, revisit before re-pinning.

---

### Task 1: Pinned 75 and the Urzatron `.dek`

**Files:**
- Create: `oracle/xmage/decks/Pauper/Deck - Urzatron.dek`
- Copy: the same file into the Mage fork `Mage.Server.Plugins/Mage.Player.AIRL/src/mage/player/ai/decks/Pauper/` (commit on the existing branch `lead/pauper-meta-registrations-v1`)
- Modify (test only): `python/tests/test_write_dek_from_mtgo_list_v1.py` (one new case)

**Interfaces:**
- Consumes: `python/tools/write_dek_from_mtgo_list_v1.py <list> <out> [--substitute "Old=New"] [--allow-pending Name]` (wave 1 Task 4).
- Produces: a 60 plus 15 `.dek` whose names are all either in `data/cards_v1.json` or in this plan's metadata table.

- [ ] **Step 1: Failing test.** Add to `python/tests/test_write_dek_from_mtgo_list_v1.py` a case that writes `Urzatron__888074.txt` with both substitutions and asserts `(main, side) == (60, 15)`, that `"Giant's Boulder"` and `"Call Damage Control"` are absent, that `"Malevolent Rumble"` has quantity 4 in the mainboard and `"Blue Elemental Blast"` quantity 2 in the sideboard. Run:
```
uvx uv@0.11.29 run --no-sync python -m unittest python.tests.test_write_dek_from_mtgo_list_v1 -v
```
Expected: FAIL only on Malevolent Rumble unless `--allow-pending` is passed for it. The tool's registry check applies to a `--substitute` target's new name, not to names copied through from the source list (see the tool's docstring and `apply_substitutions()`), so the other 18 new cards are written verbatim; the `--allow-pending` flags for them in Step 2 are belt and braces and are kept so the command does not depend on that asymmetry.

- [ ] **Step 2: Write the deck file.**
```
P=python/tools/write_dek_from_mtgo_list_v1.py
D=docs/research/pauper_meta_decklists_2026-09-09
O="oracle/xmage/decks/Pauper"
uvx uv@0.11.29 run --no-sync python $P $D/Urzatron__888074.txt "$O/Deck - Urzatron.dek" \
  --substitute "Giant's Boulder=Malevolent Rumble" \
  --substitute "Call Damage Control=Blue Elemental Blast" \
  --allow-pending "Urza's Tower" --allow-pending "Urza's Power Plant" --allow-pending "Urza's Mine" \
  --allow-pending "Conduit Pylons" --allow-pending "Bojuka Bog" --allow-pending "Expedition Map" \
  --allow-pending "Ancient Stirrings" --allow-pending "Crop Rotation" --allow-pending "Bonder's Ornament" \
  --allow-pending "Barrels of Blasting Jelly" --allow-pending "Malevolent Rumble" \
  --allow-pending "Unfathomable Truths" --allow-pending "Bramble Wurm" --allow-pending "Boulderbranch Golem" \
  --allow-pending "Maelstrom Colossus" --allow-pending "Pinnacle Kill-Ship" --allow-pending "Earth Rift" \
  --allow-pending "Tangle" --allow-pending "Rooftop Percher"
```
(The substitution targets are registry-checked; Malevolent Rumble is pending, hence its `--allow-pending`.) Verify the totals by hand as well: `grep -c "Sideboard=\"false\"" "$O/Deck - Urzatron.dek"` and the quantity sums.

- [ ] **Step 3: Run the test.** Expected: PASS.

- [ ] **Step 4: Mage fork copy.** Copy the file into the fork's Pauper deck directory, extend the fork's existing `pauperRegistrationsV2` JUnit (wave 1 Task 4, stubbed `CardLookup`) with an `Urzatron` case asserting 60 mainboard and 15 sideboard rows, and run that test alone. Commit on `lead/pauper-meta-registrations-v1` and push. If the fork's pre-commit hook fails at the stale vendored `kernel/` subtree pin (the wave 1 Task 12 ruling), commit with `--no-verify` and document the failing step in the commit body; that subtree belongs to the campaign pairing, not this branch.

- [ ] **Step 5: Commit.**
```
git add "oracle/xmage/decks/Pauper/Deck - Urzatron.dek" python/tests/test_write_dek_from_mtgo_list_v1.py
git commit -m "decks: Urzatron registration from pinned mtgtop8 888074 (not yet registered)"
git push
```

**Effort: 2 hours.**

---

### Task 2: Tron lands and conditional mana yield in the planner

**Files:**
- Modify: `data/cards_v1.json` (append Urza's Tower, Urza's Power Plant, Urza's Mine)
- Modify: `mtg-kernel/src/card_def.rs` (`Subtype` appends; `SubtypeConjunctionDef`; `DynamicValueDef::AmountIfControllerControlsEach`; `CardDef::conditional_tap_yield`)
- Modify: `mtg-kernel/src/mana.rs` (`ManaSource::yield_per_tap`, `PaymentPlan::surplus`, `solve_pips`, `pay_generic`)
- Modify: `mtg-kernel/src/engine.rs` (`gather_sources` at mana.rs 331 calls into the evaluator; `pay_plan` at 14112; `evaluate_dynamic_value` at 2994)
- Modify: `mtg-kernel/src/effect.rs` (`EffectOp::AddManaDynamic`)
- Modify: `mtg-kernel/build.rs` (`subtype_variant`, `conditional_tap_yield_for`, the mana-ability codegen at 4946 to 4960 and 6820 to 6950, the canon string at 7135)
- Create: `mtg-kernel/tests/pauper_meta_w2_tron.rs`
- Create: `data/wave_branch_manifests/pauper_meta_w2.json` (this task creates the wave manifest; Tasks 3 to 9 append to it)

**Interfaces:**
- Consumes: `ManaAbilityAmountDef::Dynamic`, `engine::evaluate_dynamic_value`, `engine::has_effective_subtype`.
- Produces: `CardDef::conditional_tap_yield`, `ManaSource::yield_per_tap`, `PaymentPlan::surplus`, `EffectOp::AddManaDynamic`, `DynamicValueDef::AmountIfControllerControlsEach`.

- [ ] **Step 1: Unit tests in `mana.rs`** next to the existing solver tests (`src(id, choices)` helper at 536):
```rust
#[test]
fn a_multi_yield_source_pays_several_pips_with_one_tap() {
    let mut tower = src(0, &[ManaColor::C]);
    tower.yield_per_tap = 3;
    let cost = Cost { pips: &[], generic: 3, x_count: 0 };
    let plan = solve(&cost, 0, [0; 6], &[tower]).expect("payable");
    assert_eq!(plan.taps.len(), 1);
    assert_eq!(plan.surplus, [0; 6]);
}

#[test]
fn unspent_multi_yield_mana_is_recorded_as_surplus() {
    let mut tower = src(0, &[ManaColor::C]);
    tower.yield_per_tap = 3;
    let cost = Cost { pips: &[], generic: 1, x_count: 0 };
    let plan = solve(&cost, 0, [0; 6], &[tower]).expect("payable");
    assert_eq!(plan.taps.len(), 1);
    assert_eq!(plan.surplus[ManaColor::C.pool_index()], 2);
}
```
Run: `cargo test --locked -p mtg-kernel --lib mana::tests`. Expected: compile FAIL.

- [ ] **Step 2: Implement the planner change.** Add `yield_per_tap: u8` to `ManaSource` (every existing construction site sets 1) and `surplus: [u8; 6]` to `PaymentPlan`. In `solve_pips`, when a source is tapped for color `c`, credit `yield_per_tap - 1` into `pool_remaining[c]` and `plan.surplus[c]`, undoing both on backtrack. In `pay_generic`, subtract `min(needed, yield)` from `needed` and credit the remainder to `pool_remaining`/`surplus`. In `engine::pay_plan`, add each tap's full yield to the pool, add `surplus`, then subtract the consumed amounts, so the arithmetic never goes negative. Re-run Step 1's tests. Expected: PASS, and every pre-existing `mana::tests` case still passes (`assert_eq!(plan.taps, ...)` shapes are unchanged for single-yield sources).

- [ ] **Step 3: Failing card tests** in `mtg-kernel/tests/pauper_meta_w2_tron.rs`. Header cites the three Java blobs and `UrzaTerrainValue` blob `57fa2b3f0ce2f8c8e60d68e2dfe9561e2578d71d`. Copy `card_id`, `card_name`, `put_object`, `ready_main1`, `pass_until_stack_len` from `tests/pauper_meta_w1_snuff_out_and_duals.rs:21-90`. Tests, each with its `covers:` line:
  - `urzas_tower_adds_one_colorless_without_the_other_two_pieces`: only a Tower on the battlefield; activating its mana ability adds exactly 1 `{C}`.
  - `assembled_tron_makes_tower_add_three_and_mine_and_power_plant_add_two`: all three pieces untapped; the Tower's activation adds 3, the Mine's 2, the Power Plant's 2; total available 7.
  - `a_second_copy_of_the_same_piece_does_not_assemble_tron`: two Towers and a Mine; each Tower still adds 1 (the source's own piece is never the missing partner, and the missing Power Plant gates both).
  - `automatic_payment_taps_one_tower_for_a_three_generic_spell`: with assembled Tron, `Myr Enforcer`-sized generic costs are paid by fewer taps; assert the spell is castable with the three Tron lands alone and that only the expected number of lands are tapped afterwards.
  - `surplus_tron_mana_floats_after_a_cheaper_spell`: assembled Tron, cast a `{1}` artifact paying with the Tower; afterwards the controller's `mana_pool[C]` is 2.
  - `losing_a_piece_drops_the_yield_back_to_one`: assembled Tron, destroy the Mine (engine `DestroyObject` through a registered removal spell or `put_object` plus a direct move, whichever the existing tests use), then the Tower adds 1.

- [ ] **Step 4: Implement the cards.** Append the three registry entries. Append `Urzas`, `Tower`, `PowerPlant`, `Mine` to `Subtype` after `Fish` (not alphabetically) and map the JSON strings in `build.rs::subtype_variant`; none of the four is a creature type. Add `conditional_tap_yield_for`:
```rust
"Urza's Tower" => "Some(DynamicValueDef::AmountIfControllerControlsEach { required: [SubtypeConjunctionDef { first: Subtype::Urzas, second: Subtype::Mine }, SubtypeConjunctionDef { first: Subtype::Urzas, second: Subtype::PowerPlant }], amount_when_met: 3, amount_otherwise: 1 })",
```
and the analogous entries for the Mine (partners Tower and PowerPlant, `amount_when_met: 2`) and the Power Plant (partners Mine and Tower, `amount_when_met: 2`). Generate the `mana_ability` program for these three as `EffectOp::AddManaDynamic { player: PlayerRef::Controller, color: ManaColor::C, amount: <the same `DynamicValueDef` literal as that card's `conditional_tap_yield_for` entry> }`. `mana::gather_sources` sets `yield_per_tap` from `def.conditional_tap_yield` through `engine::evaluate_dynamic_value` (clamped to `1..=8`), leaving 1 when the field is `None`.

- [ ] **Step 5: Run** `cargo test --locked -p mtg-kernel --test pauper_meta_w2_tron`, then `--lib mana::`, `--lib engine::` filtered to the touched modules, then:
```
uvx uv@0.11.29 run --no-sync python python/tools/repin_card_db_identity_v1.py --write
uvx uv@0.11.29 run --no-sync python python/tools/generate_flat_policy_v1_goldens.py
uvx uv@0.11.29 run --no-sync python python/tools/generate_flat_policy_v2_goldens.py
uvx uv@0.11.29 run --no-sync python -m unittest python.tests.test_flat_policy_v1_goldens python.tests.test_flat_policy_v2_goldens -v
cargo test --locked -p mtg-kernel --no-run
```
Expected: PASS. Any pinned state-hash literal that moves is re-pinned with its cause recorded in the commit body (this task changes no `GameState` field, so a moved state hash means the card DB hash, which is cause A).

- [ ] **Step 6: Active-nine invariance.** The planner refactor must not move any existing deck's payments (controller ruling). Add `payment_plans_for_the_active_nine_are_unchanged_by_the_yield_refactor` to `mtg-kernel/tests/pauper_meta_w2_tron.rs`: for each of the nine active runtime deck ids, build a deterministic opening state from `runtime_decks_v1.json` (reuse the deck-seeded environment constructor `tests/rl_contract.rs` uses), and for every castable card in hand assert that `mana::can_pay` returns a `PaymentPlan` whose `taps` and `pool_used` equal the values recorded in a checked-in fixture, with `surplus == [0; 6]` everywhere. Generate the fixture from the pre-refactor build during Step 1, before `mana.rs` is touched, and commit it with the test, so the assertion is a real before-and-after comparison rather than a self-confirming snapshot.

- [ ] **Step 7: Manifest membership note.** `data/cards_v1.json` now lists `Deck - Urzatron.dek`, which is not yet in `REGISTRATION_SPECS`, so `python/tests/test_pauper_pool_manifest.py`'s roster-membership test is expected red until Task 10 (the wave 1 ruling for its Tasks 5 to 11). Record the expected red in the task report; do not "fix" it.

- [ ] **Step 8: Wave manifest and commit.** Add this task's cards to `data/wave_branch_manifests/pauper_meta_w2.json` (one entry per card: `post_board` plus the exact branch names this task's `covers:` annotations use), then run
```
uvx uv@0.11.29 run --no-sync python python/tools/check_wave_branch_coverage_v1.py data/wave_branch_manifests/pauper_meta_w2.json
```
Expected: exit 0 over the cards landed so far. Then commit:
```
git add -A
git commit -m "cards: Urza's Tower, Urza's Power Plant, Urza's Mine; conditional multi-mana yield in the payment planner

KERNEL_CARDDB_HASH 0xde59c501e943f3fd -> 0x<new>"
git push
```

**Effort: 7 hours** (6 plus 1 for the active-nine invariance fixture and test).

---

### Task 3: Utility lands and mana artifacts (Bojuka Bog, Conduit Pylons, Expedition Map, Bonder's Ornament, Barrels of Blasting Jelly)

**Files:**
- Modify: `data/cards_v1.json` (append the five, in this order)
- Modify: `mtg-kernel/src/card_def.rs` (`ManaAbilityCostDef::None`; `Subtype::Desert` append)
- Modify: `mtg-kernel/src/effect.rs` (`LibraryCardFilter::AnyLand`, `EffectOp::Surveil`, `EffectOp::EachPlayerControllingDefinitionDrawsCard`)
- Modify: `mtg-kernel/build.rs` (`subtype_variant`, `primary_mana_ability_colors` override for Conduit Pylons and Barrels, `additional_mana_abilities_for`, `mana_ability_def_for`, `activated_ability_recipes_for` at 3596, `trigger_recipe_for` at 4683, `enters_battlefield_tapped`)
- Modify: `mtg-kernel/src/engine.rs` (offer a mana ability whose `ManaAbilityCostDef` is `None` under its per-turn cap; surveil resolution beside the scry path)
- Modify: `mtg-kernel/src/rl.rs` and `mtg-kernel/src/rl_session.rs` if and only if the surveil choice is a decision shape the generic `ChooseEffectBoolean`/`ChooseEffectTargets` vocabulary does not already cover
- Create: `mtg-kernel/tests/pauper_meta_w2_lands_and_rocks.rs`
- Modify: `data/wave_branch_manifests/pauper_meta_w2.json` (this task's cards and their branch lists)

**Interfaces:**
- Consumes: `AdditionalManaAbilityDef` (Heap Gate), `EffectOp::ExilePlayersGraveyard` (Nihil Spellbomb), `EffectOp::SearchLibraryToHand`, `ActivatedAbilityRecipe` (Nihil Spellbomb's tap-and-sacrifice shape), `Special::ScryThenDraw`'s scry machinery, `trigger_recipe_for`'s targeted ETB form (`"Harrier Strix" => "etb:target_any_permanent:tap"`).
- Produces: `LibraryCardFilter::AnyLand`, `EffectOp::Surveil { player, count }`, `EffectOp::EachPlayerControllingDefinitionDrawsCard { card_def }`, `ManaAbilityCostDef::None`, trigger recipes `"Bojuka Bog" => "etb:target_player:exile_graveyard"` and `"Conduit Pylons" => "etb:surveil:1"`.

- [ ] **Step 1: Failing tests** in `mtg-kernel/tests/pauper_meta_w2_lands_and_rocks.rs` (header cites the five blobs), each with its `covers:` line:
  - `bojuka_bog_enters_tapped_and_exiles_a_targeted_graveyard`: P1 has three cards in the graveyard; playing the Bog enters tapped, the ETB asks for a target player, choosing P1 empties P1's graveyard into exile; choosing P0 empties P0's instead.
  - `bojuka_bog_taps_for_black_the_turn_after`.
  - `conduit_pylons_surveils_one_on_entry`: the ETB offers keep-on-top or graveyard; both branches asserted (library top unchanged versus card in graveyard).
  - `conduit_pylons_taps_for_colorless_free_and_any_color_for_one`: the free ability yields `{C}`; the paid ability is offered only with one other mana source available and yields the chosen color.
  - `expedition_map_fetches_any_land_to_hand`: with `{2}` available, `{2}`, `{T}`, sacrifice finds a Forest (a nonbasic land is equally legal: assert Bojuka Bog is in the candidate set), the Map is in the graveyard, the library is shuffled.
  - `bonders_ornament_taps_for_any_color_and_draws_for_four`: activation with `{4}` available draws exactly one card for its controller; with the opponent also controlling a Bonder's Ornament, both draw; with only the opponent controlling one and the controller's own gone, assert against the Java's per-player battlefield check.
  - `barrels_of_blasting_jelly_adds_any_color_once_per_turn`: the `{1}` ability needs no tap, is offered twice in one turn only until it has been used once, and resets next turn.
  - `barrels_of_blasting_jelly_deals_five_to_a_creature`: `{5}`, `{T}`, sacrifice kills a 5-toughness creature and puts the Barrels in the graveyard.

- [ ] **Step 2: Run**: `cargo test --locked -p mtg-kernel --test pauper_meta_w2_lands_and_rocks`. Expected: FAIL.

- [ ] **Step 3: Implement.** Append the five registry entries plus `Subtype::Desert`. Conduit Pylons: `primary_mana_ability_colors` override to `vec!["C"]` and an `additional_mana_abilities_for` entry copied verbatim from Heap Gate's. Barrels: `primary_mana_ability_colors` override to `vec![]` (so it is not an automatic payment source) plus an `additional_mana_abilities_for` entry with `mana_cost: Cost { pips: &[], generic: 1, x_count: 0 }`, `ability: ManaAbilityDef { cost: ManaAbilityCostDef::None, amount: ManaAbilityAmountDef::Fixed(1), controller_damage: 0, max_activations_per_turn: Some(1) }` (the per-turn cap uses Wall of Roots' existing enforcement). Expedition Map and Barrels' damage ability and Bonder's Ornament's draw ability are `ActivatedAbilityRecipe` rows beside Nihil Spellbomb's. Bojuka Bog is `enters_tapped` plus the targeted ETB trigger; Conduit Pylons is the surveil ETB.

- [ ] **Step 4: RL surface.** Run `cargo test --locked -p mtg-kernel --test rl_session` and `--test rl_contract`. If surveil introduces a decision the generic vocabulary does not cover, add the `rl.rs` candidate arm plus both `rl_session.rs` validator arms and a `flat_build_action_cache_v2` test in the same commit (Global Constraints). The tapless mana ability must appear in `legal_action_candidates_v1` as an `ActivateManaAbilityChoice` candidate: assert that explicitly in a new `rl_session` test.

- [ ] **Step 5: Run** the focused file, the touched module filters, the repin, the two golden generators and their Python tests, and `--no-run`. Expected: PASS.

- [ ] **Step 6: Wave manifest and commit.** Add this task's cards to `data/wave_branch_manifests/pauper_meta_w2.json` (one entry per card: `post_board` plus the exact branch names this task's `covers:` annotations use), then run
```
uvx uv@0.11.29 run --no-sync python python/tools/check_wave_branch_coverage_v1.py data/wave_branch_manifests/pauper_meta_w2.json
```
Expected: exit 0 over the cards landed so far. Then commit with the hash line: `git commit -m "cards: Bojuka Bog, Conduit Pylons, Expedition Map, Bonder's Ornament, Barrels of Blasting Jelly"` and push.

**Effort: 5 hours.**

---

### Task 4: Green library manipulation (Ancient Stirrings, Malevolent Rumble, Crop Rotation)

**Files:**
- Modify: `data/cards_v1.json` (append the three)
- Modify: `mtg-kernel/build.rs` (`Special::LookTopSelectFilteredToHandRest` variant, its canonical token, `special_for`, `effect_recipe_for`, `additional_cost_for`, the codegen branch beside `RevealTopAndPartitionByType` at 5432)
- Modify: `mtg-kernel/src/effect.rs` (`LookSelectFilter`, `LookRest`, the resumable selection program, `EffectOp::SearchLibraryToBattlefieldUntapped`)
- Modify: `python/tools/generate_pauper_manifests.py` (`TOKEN_DEPENDENCIES`: Malevolent Rumble as an Eldrazi Spawn Token producer)
- Create: `mtg-kernel/tests/pauper_meta_w2_green_selection.rs`
- Modify: `data/wave_branch_manifests/pauper_meta_w2.json` (this task's cards and their branch lists)

**Interfaces:**
- Consumes: `Special::LookTopSelectByTypeToHandBottomRest` (Lead the Stampede) and `Special::WindingWay`'s resumable selection interpreter; `CostComponent::SacrificeControlled { filter: PermanentFilter::Land }` (Raze, wave 1 Task 6); `EffectOp::SearchLibraryToBattlefieldTapped`; `EffectOp::CreateToken`.
- Produces: `Special::LookTopSelectFilteredToHandRest { look, max, filter, rest }`, `LibraryCardFilter::AnyLand` (from Task 3), `EffectOp::SearchLibraryToBattlefieldUntapped`.

- [ ] **Step 1: Failing tests** (header cites the three blobs), each with its `covers:` line:
  - `ancient_stirrings_takes_a_colorless_card_from_the_top_five`: library top five contain Urza's Tower (colorless land), Myr Enforcer (colorless artifact creature), and three green spells; the decision offers exactly the two colorless candidates plus declining; selecting the Tower puts it in hand and the other four on the bottom in the chosen order.
  - `ancient_stirrings_may_decline_and_bottoms_all_five`: no colorless card among the top five; the effect completes with no hand change and five cards on the bottom.
  - `malevolent_rumble_takes_a_permanent_card_and_mills_the_rest`: top four are a Forest, a Bramble Wurm, Ponder, Brainstorm; selecting the Wurm puts it in hand, the other three go to the graveyard, and one Eldrazi Spawn Token is created.
  - `malevolent_rumble_creates_the_spawn_even_when_nothing_is_taken`.
  - `crop_rotation_sacrifices_a_land_and_fetches_one_untapped`: with two lands, casting requires the sacrifice choice, the fetched land enters untapped and can be tapped for mana the same turn; with no land besides the mana source, assert the engine's independent cost-check behavior (mirror the one-land Raze sub-case in `tests/pauper_meta_w1_black_red.rs`).

- [ ] **Step 2: Run**: FAIL.

- [ ] **Step 3: Implement.** Append the three registry entries. Add the new `Special` variant with its canonical token `look_top_select_filtered_to_hand_rest:{look}:{max}:{filter}:{rest}` and recipes:
  - Ancient Stirrings: `target=None;spell=LookTopSelectFilteredToHandRest(5,1,Colorless,BottomAny);mana=None`
  - Malevolent Rumble: `target=None;spell=Sequence[LookTopSelectFilteredToHandRest(4,1,PermanentCard,Graveyard),CreateToken(Eldrazi Spawn Token,1)];mana=None`
  - Crop Rotation: `target=None;spell=SearchLibraryToBattlefieldUntapped(Controller,AnyLand);mana=None` with `additional_cost_for("Crop Rotation") => Some(&[CostComponent::SacrificeControlled { count: 1, filter: PermanentFilter::Land }])`
  `LookTopSelectByTypeToHandBottomRest` is not modified, so Lead the Stampede's generated program and canonical token stay byte-identical.

- [ ] **Step 4: RL surface.** The selection uses `Decision::ChooseEffectTargets` plus `Action::FinishEffectSelection`, both already covered; assert that by adding a `rl_session` test that drives Ancient Stirrings' selection through `flat_build_action_cache_v2`, including the decline path.

- [ ] **Step 5: Run** the focused file, module filters, repin, golden generators and their Python tests, `--no-run`. Expected: PASS.

- [ ] **Step 6: Wave manifest and commit.** Add this task's cards to `data/wave_branch_manifests/pauper_meta_w2.json` (one entry per card: `post_board` plus the exact branch names this task's `covers:` annotations use), then run
```
uvx uv@0.11.29 run --no-sync python python/tools/check_wave_branch_coverage_v1.py data/wave_branch_manifests/pauper_meta_w2.json
```
Expected: exit 0 over the cards landed so far. Then commit: `git commit -m "cards: Ancient Stirrings, Malevolent Rumble, Crop Rotation; filtered look-and-take primitive"` and push.

**Effort: 5 hours.**

---

### Task 5: Bramble Wurm (graveyard-activated ability) and Unfathomable Truths

**Files:**
- Modify: `data/cards_v1.json` (append the two)
- Modify: `mtg-kernel/src/card_def.rs` (`CostComponent::ExileSourceFromGraveyard`)
- Modify: `mtg-kernel/src/engine.rs` (offer `activation_zone: Zone::Graveyard` abilities in `available_activatable_abilities`, beside the existing `Zone::Hand` cycling case)
- Modify: `mtg-kernel/build.rs` (`activated_ability_recipes_for`, `keywords_for`, `trigger_recipe_for`, `effect_recipe_for`)
- Create: `mtg-kernel/tests/pauper_meta_w2_wurm_and_truths.rs`
- Modify: `data/wave_branch_manifests/pauper_meta_w2.json` (this task's cards and their branch lists)

**Interfaces:**
- Consumes: `ActivatedAbilityDef::activation_zone` (Lorien Revealed's hand-zone cycling), `AbilityEffectRecipe` gain-life shape, `EffectOp::DrawCards`, `EffectOp::CreateToken`.
- Produces: `CostComponent::ExileSourceFromGraveyard`, trigger recipe `"Bramble Wurm" => "etb:gain_life:5"` (reuses the existing `etb:gain_life:N` family with a new amount).

- [ ] **Step 1: Failing tests**, each with its `covers:` line:
  - `bramble_wurm_gains_five_on_entry_and_has_reach_and_trample`.
  - `bramble_wurm_graveyard_ability_exiles_itself_for_five_life`: the Wurm in the graveyard with `{2}{G}` available: the ability is offered, resolving gains 5 life and the card is in exile; with the Wurm on the battlefield the graveyard ability is not offered; with only `{1}{G}` it is not offered.
  - `unfathomable_truths_draws_three_and_makes_a_spawn`: resolution draws three and creates one Eldrazi Spawn Token that can be sacrificed for `{C}`.
  - `unfathomable_truths_is_colorless`: `CARD_DEFS[card_id("Unfathomable Truths")].colors` is empty (devoid) while its mana cost still has the `{U}` pip.

- [ ] **Step 2: Run**: FAIL.

- [ ] **Step 3: Implement.** Append the entries and `Subtype::Wurm` (a creature type: add it to `Subtype::CREATURE_TYPES` and re-run the changeling tests in `tests/pauper_meta_w1_creatures.rs` and `tests/elves_tribal.rs`). Bramble Wurm's graveyard ability is one `ActivatedAbilityRecipe` with `activation_zone: "Graveyard"`, costs `[Mana {2}{G}, ExileSourceFromGraveyard]`, effect gain 5. Keywords: reach and trample by name in `keywords_for`.

- [ ] **Step 4: RL surface.** A graveyard-zone activation appears in `Decision::CastSpellOrPass`'s `activatable_abilities` as an ordinary `(source, ability_index)` pair, so the existing `rl.rs` arm covers it; prove it with a `rl_session` test that reaches the graveyard activation through `flat_build_action_cache_v2` (this is exactly the class of gap that bit wave 1 with Glint Hawk).

- [ ] **Step 5: Run** the focused file, module filters, repin, goldens, `--no-run`. Expected: PASS.

- [ ] **Step 6: Wave manifest and commit.** Add this task's cards to `data/wave_branch_manifests/pauper_meta_w2.json` (one entry per card: `post_board` plus the exact branch names this task's `covers:` annotations use), then run
```
uvx uv@0.11.29 run --no-sync python python/tools/check_wave_branch_coverage_v1.py data/wave_branch_manifests/pauper_meta_w2.json
```
Expected: exit 0 over the cards landed so far. Then commit: `git commit -m "cards: Bramble Wurm (graveyard-activated ability), Unfathomable Truths"` and push.

**Effort: 3 hours.**

---

### Task 6: Cascade and Maelstrom Colossus

The cast route is settled (design note B, spikes of 2026-09-10). Implement it as written there: an ordinary `CastMethodV4::Normal` cast on the new `SpellCastRouteV4::CascadeExile`. Do not add a `CastMethodV4` variant and do not touch `flat_policy_v1.rs`, `flat_policy_v2.rs`, or `python/mtg_kernel_rl/features.py` (Global Constraints: the encoder is frozen for a card wave). Do not substitute `CastMethodV4::Plotted` or `CastMethodV4::Alternative`, which design note B shows the engine rejects or mis-reports.

**Files:**
- Modify: `data/cards_v1.json` (append Maelstrom Colossus)
- Modify: `mtg-kernel/src/card_def.rs` (`CardDef::cascade: bool`)
- Modify: `mtg-kernel/src/state.rs` (`SpellCastRouteV4::CascadeExile`, `ObjectStateV4::cascade_grant: bool` with `#[serde(default)]`; `CastMethodV4` is not touched)
- Modify: `mtg-kernel/src/effect.rs` (`EffectOp::CascadeFromSource`, the resumable continuation frames it needs, `EffectBooleanChoicePurpose::CascadeFreeCast`)
- Modify: `mtg-kernel/src/trigger.rs` (a `cast_self:cascade` trigger table beside Weather the Storm's)
- Modify: `mtg-kernel/src/engine.rs` (the seven route-aware sites design note B lists, plus the cascade restamp)
- Modify: `mtg-kernel/src/environment_randomization_v2.rs` (`ShufflePurposeV2::CascadeBottomOrder` and its purpose-string table; `GameEnvironmentRandomizationV2::next_cascade_bottom_ordinal` with its getter and commit pair)
- Modify: `mtg-kernel/src/rl.rs`, `mtg-kernel/src/rl_session.rs` (the `CascadeFreeCast` arm of the `EffectBooleanChoicePurpose` to `BooleanChoicePurposeV4` match inside `pending_effect_semantic_v4`, rl.rs 5632-5656, and both `rl_session.rs` validators)
- Modify: `mtg-kernel/build.rs` (`cascade_for`, `trigger_recipe_for`)
- Create: `mtg-kernel/tests/pauper_meta_w2_cascade.rs`
- Modify: `data/wave_branch_manifests/pauper_meta_w2.json` (this task's cards and their branch lists)
- Never modified by this task: `mtg-kernel/src/flat_policy_v1.rs`, `mtg-kernel/src/flat_policy_v2.rs`, `python/mtg_kernel_rl/features.py`

**Interfaces:**
- Consumes: `trigger_recipe_for`'s `cast_self` kind (Weather the Storm), `Decision::ChooseEffectBoolean` and its existing `BooleanChoicePurposeV4::OptionalEffect` projection, `SpellCastRouteV4::AdventureExile` and its `on_adventure` flag as the structural model for an incarnation-flag exile route (state.rs 513-520, engine.rs 13209-13215, 13240-13242, 13293-13300, 6767-6786), `environment_randomization_v2::shuffle_slice_in_place_v2`.
- Produces: `CardDef::cascade`, `SpellCastRouteV4::CascadeExile`, `ObjectStateV4::cascade_grant`, `EffectOp::CascadeFromSource`, `EffectBooleanChoicePurpose::CascadeFreeCast`, `ShufflePurposeV2::CascadeBottomOrder`.

- [ ] **Step 1: Failing tests** (header cites `MaelstromColossus.java` blob `e1264ffc89857abb8ff65cd526fbd75177504992` and `CascadeAbility.java` blob `45a4ee8634164bde1da98f4f5accfec6d0c89169`), each with its `covers:` line:
  - `cascade_exiles_until_a_cheaper_nonland_and_may_cast_it_free`: library top is Forest, Urza's Mine, Bramble Wurm (mana value 7, less than 8); casting the Colossus triggers cascade, the two lands and the Wurm are exiled in order, the boolean offer appears, accepting puts the Wurm on the stack with no mana paid, and after it resolves the two lands are on the bottom of the library.
  - `cascade_declined_bottoms_every_exiled_card`: declining leaves the Wurm and the two lands on the bottom in some order and the Wurm is not on the battlefield.
  - `cascade_skips_lands_and_equal_or_greater_mana_values`: top cards are a land and a second Maelstrom Colossus (mana value 8, not less than 8) before the Wurm: only the Wurm is castable.
  - `cascade_resolves_the_free_spell_before_the_cascading_spell`: the Wurm is on the battlefield while the Colossus is still on the stack (assert the stack contents at the moment the free spell resolves).
  - `cascade_with_an_empty_library_does_nothing_and_does_not_halt`.
  - `cascade_that_exhausts_the_library_without_a_match_bottoms_everything_and_does_not_halt`: a library of three lands only, so the exile loop runs to exhaustion with no qualifying card; no boolean offer appears, all three cards return to the bottom, the library is the same size as before, and the engine is not halted. (The Oracle's loop leaves `cardToCast` null and calls `controller.getLibrary().reset()`; this is a distinct path from the empty-library case.)
  - `cascade_free_cast_targets_are_chosen_normally`: use a cheaper targeted spell (Lightning Bolt, already registered, in the library) and assert the target decision appears for the free cast.
  - `cascade_still_pays_an_additional_cost`: a cascaded card with a non-mana additional cost is offered only when that cost is payable and pays it on resolution, because "without paying its mana cost" waives the mana cost only. (Crop Rotation's sacrifice-a-land additional cost, landed in Task 4, is the in-deck case.)
  - `cascade_still_pays_kicker_when_the_free_spell_is_kicked`: cascade into Goblin Bushwhacker (the registry's only card with a `kicker_cost`, `{R}`, mana value 1, so cascade off an 8-drop reaches it). With no red source the free cast resolves unkicked and no mana is spent; with an untapped Mountain the `ChooseKicker` decision is offered, paying it spends exactly `{R}` (the Mountain is tapped, the Bushwhacker's printed `{R}` is not paid a second time), and the kicked effect happens. CR 702.85 waives the printed mana cost only. The library entry for the Bushwhacker is built by the test helper, so the card needs no deck membership; a test-only definition is unnecessary because a real kicker card is registered.
  - `two_cascades_in_one_game_derive_different_bottom_orders`: the same player casts two cascading spells in one game over an identically ordered set of bottomed cards; the two resulting bottom orders come from different derived seeds (assert the two orders are produced from distinct ordinals through the randomization transaction, not merely that the permutations differ, since two draws can coincide). This is the test that catches a build reusing one seed per game.
  - `cascade_bottoming_order_is_deterministic_for_a_fixed_seed`: two runs from the same seed produce the same bottom order.
  - `cascade_free_cast_encodes_as_the_normal_cast_method_and_leaves_the_encoder_frozen`: the cascaded spell's stack item carries `CastMethodV4::Normal` and `SpellCastRouteV4::CascadeExile`; its flat encoding's cast-method id equals the id the same card gets on an ordinary hand cast (a differential assertion, so the test never hardcodes an encoder id); `mtg_kernel::flat_policy_v2::FLAT_POLICY_ENUM_MAPPING_VERSION_V2 == 1` and `mtg_kernel::flat_policy_v1::FLAT_POLICY_ENUM_MAPPING_VERSION_V1 == 1`, proving this wave added no model-input enum value.
  - `cascade_cast_route_survives_a_snapshot_round_trip` and `pre_wave_snapshot_without_cascade_grant_still_loads` (the Global Constraints fixture test for the new `ObjectStateV4` field).

- [ ] **Step 2: Run**: FAIL.

- [ ] **Step 3: Route, marker, and the seven route-aware arms.** Append `SpellCastRouteV4::CascadeExile` and `ObjectStateV4::cascade_grant` in `state.rs`, then make the seven engine.rs sites design note B lists route-aware: the `cast_route` match and its `is_cascade_exile` binding (13209-13215, 13224-13245), the cast-mode staging (13260-13263), the restamp across the Exile to Stack move (13286-13300), `remaining_cast_payment_is_payable`'s Normal arm (the function at 5989, its arm at 5998-6008), the payment match's Normal arm (13501-13543, including the kicker and delve branches), the pending-cast validator's Normal arm (6767-6786), and the two exhaustive `SpellCastRouteV4` matches the compiler will point at: `route_supports_method` inside `validate_spell_source_contract_fields` (1577-1649) and `route_is_valid` inside `storm_source_contract_is_structurally_valid` (1894-1911). **Pitfall:** the `AdventureExile` arm at 1590-1595 ends in `def.adventure.is_some()`, and `def` there is the *cast* spell's definition. The `CascadeExile` arm must gate on `source.v4.cascade_grant`, never on `def.cascade`: the cascading spell carries `cascade`, the cascaded card does not, so a `def.cascade` gate would reject every real cascade and accept a forged route on a hand-cast Colossus. `forced_cast_method` stays `None` and no `CastMethodV4` match gains an arm; confirm that in the task report by showing `git diff --stat` contains neither flat module. Build with `cargo test --locked -p mtg-kernel --no-run` before writing the effect.

- [ ] **Step 4: Implement the effect** per design note B: the `cast_self:cascade` trigger, `EffectOp::CascadeFromSource`'s exile loop, the `CascadeFreeCast` boolean, the cast through the ordinary cast entry point on the cascade route, and the bottoming through `shuffle_slice_in_place_v2` under the new `ShufflePurposeV2::CascadeBottomOrder`, seeded from the new per-player `next_cascade_bottom_ordinal` with the preflight and commit pair that mirrors `next_live_shuffle_ordinal`/`set_live_shuffle_ordinal` (environment_randomization_v2.rs 262 onward). Do not derive cascade's seed from the live-shuffle ordinal: it is hardwired to `InGameLibraryShuffle`, so sharing it would either repeat one seed across a game's cascades or perturb the library-shuffle stream.

- [ ] **Step 5: RL surface.** The mandatory site is the exhaustive `EffectBooleanChoicePurpose` to `BooleanChoicePurposeV4` match inside `pending_effect_semantic_v4` (rl.rs 5632-5656): add `CascadeFreeCast => BooleanChoicePurposeV4::OptionalEffect` beside `LookAtTopMayRevealThen`. `rl.rs`'s `Decision::ChooseEffectBoolean` candidate arm (2388) is purpose-agnostic and needs no change; do not edit it. Then extend both `rl_session.rs` validators and add a test that drives the accept and decline branches through `flat_build_action_cache_v2`, plus one that encodes the cascaded spell's own cast action through the same path. Without this, the first live AffinityV2-style abort repeats for Urzatron.

- [ ] **Step 6: Run** `--test pauper_meta_w2_cascade`, `--test rl_session`, `--test rl_contract`, the touched module filters, the repin, the flat golden generators and their Python tests (data regeneration after the card DB re-pin, not an encoder change), `--no-run`. Expected: PASS.

- [ ] **Step 7: Wave manifest and commit.** Add this task's cards to `data/wave_branch_manifests/pauper_meta_w2.json` (one entry per card: `post_board` plus the exact branch names this task's `covers:` annotations use), then run
```
uvx uv@0.11.29 run --no-sync python python/tools/check_wave_branch_coverage_v1.py data/wave_branch_manifests/pauper_meta_w2.json
```
Expected: exit 0 over the cards landed so far. Then commit: `git commit -m "mechanic: cascade (shared machinery) with Maelstrom Colossus"` and push. Record in the body: the reused cast method and the new route, the seven route-aware sites, the kicker treatment under CR 702.85, the evidence that neither flat module nor `features.py` was touched, and the shuffle purpose and ordinal used for the random bottoming.

**Effort: 12 hours** (revision 4: 10 plus the two omitted exhaustive route matches, the kicker branch in both the payability and payment arms with its test, and the per-player cascade ordinal with its preflight, commit, and two-cascade test).

---

### Task 7: Prototype and Boulderbranch Golem

**Files:**
- Modify: `data/cards_v1.json` (append Boulderbranch Golem)
- Modify: `mtg-kernel/src/card_def.rs` (`PrototypeDef`, `CardDef::prototype`, `DynamicValueDef::SourcePermanentPower`)
- Modify: `mtg-kernel/src/state.rs` (`ObjectStateV4::prototyped: bool` with `#[serde(default)]`)
- Modify: `mtg-kernel/src/engine.rs` (alternative-cost offer at 1763 and 6009, the sorcery-speed gate, the battlefield incarnation seam that applies the flag, `effective_power`/`effective_toughness`/`object_color_mask` materialization, mana-value readers)
- Modify: `mtg-kernel/build.rs` (`prototype_for`, `trigger_recipe_for` for the power-scaled life gain)
- Create: `mtg-kernel/tests/pauper_meta_w2_prototype.rs`
- Modify: `data/wave_branch_manifests/pauper_meta_w2.json` (this task's cards and their branch lists)

**Interfaces:**
- Consumes: `CastMode::Alternative` and `CastMethodV4::Alternative`, `Decision::ChooseCastMode`, `ObjectStateV4::reset_for_zone_change`, `engine::sorcery_speed_timing_ok`.
- Produces: `CardDef::prototype`, `ObjectStateV4::prototyped`, `DynamicValueDef::SourcePermanentPower`.

- [ ] **Step 1: Failing tests** (header cites `BoulderbranchGolem.java` blob `0cc82e907a734b45d88aa795743a4e1f013538c1` and `PrototypeAbility.java` blob `643b93b0997460ccb661074399ea483142351105`), each with its `covers:` line:
  - `prototype_cast_mode_is_offered_only_when_both_costs_are_payable`: with `{3}{G}` exactly, only the prototype cast is legal and no `ChooseCastMode` decision appears; with seven mana and a Forest, the decision appears with both options.
  - `prototyped_golem_is_a_three_three_green_artifact_creature`: the permanent is 3/3, not the printed 6/5; `object_color_mask` includes green; it is still an Artifact Creature Golem; its mana value reads 4.
  - `full_cost_golem_is_a_six_five_colorless_artifact_creature`.
  - `prototype_etb_gains_life_equal_to_current_power`: prototyped, life 20 to 23; full cost, life 20 to 26; with a `-1/-1` counter on the prototyped Golem at the moment the trigger resolves, 2 life (the value is sampled at resolution).
  - `prototype_is_sorcery_speed_only`: not castable during the opponent's turn or with the stack non-empty.
  - `prototyped_state_survives_a_snapshot_round_trip`: serialize and deserialize the `GameState` and assert the permanent is still 3/3 green.
  - `pre_wave_snapshot_without_the_prototyped_field_still_loads` (the Global Constraints fixture test: a JSON fixture with the wave 1 `ObjectStateV4` shape deserializes with `prototyped == false`).

- [ ] **Step 2: Run**: FAIL.

- [ ] **Step 3: Extend `evaluate_dynamic_value` with a source.** `DynamicValueDef::SourcePermanentPower` names a specific object, but `engine::evaluate_dynamic_value` (engine.rs 2994-3022, an exhaustive match with no wildcard) takes only `(state, value, controller)`. Add a `source: ObjectId` parameter and update its five call sites: engine.rs 5350 (`ManaAbilityAmountDef::Dynamic`, where the source is the activated permanent) and effect.rs 9840, 9953, 10732, 10734 (where it is `ctx.source`). Task 2's `AmountIfControllerControlsEach` ignores the new parameter; only the prototype arm reads it. Run the touched module filters before continuing.

- [ ] **Step 4: Implement** per design note C above. Enumerate, in the task report, every consumer of `CastMethodV4::Alternative` and `def.alt_cost` that must now also admit `def.prototype` (`grep -n "alt_cost" mtg-kernel/src/engine.rs`), the wave 1 lesson being that the Omen-tag reuse shipped with an incomplete consumer audit and a Critical review finding.

- [ ] **Step 5: RL surface.** `ChooseCastMode` already has an `rl.rs` arm (2203) and validator coverage; add a `rl_session` test that encodes both cast modes for the Golem through `flat_build_action_cache_v2` and asserts the two candidates are distinguishable.

- [ ] **Step 6: Run** `--test pauper_meta_w2_prototype`, `--test rl_session`, module filters, repin, goldens, `--no-run`. Expected: PASS.

- [ ] **Step 7: Wave manifest and commit.** Add this task's cards to `data/wave_branch_manifests/pauper_meta_w2.json` (one entry per card: `post_board` plus the exact branch names this task's `covers:` annotations use), then run
```
uvx uv@0.11.29 run --no-sync python python/tools/check_wave_branch_coverage_v1.py data/wave_branch_manifests/pauper_meta_w2.json
```
Expected: exit 0 over the cards landed so far. Then commit: `git commit -m "mechanic: prototype (shared machinery) with Boulderbranch Golem"` and push.

**Effort: 6 hours.**

---

### Task 8: Station and Pinnacle Kill-Ship

**Files:**
- Modify: `data/cards_v1.json` (append Pinnacle Kill-Ship)
- Modify: `mtg-kernel/src/card_def.rs` (`StationDef`, `CardDef::station`, `Subtype::Spacecraft`)
- Modify: `mtg-kernel/src/state.rs` (`Counters::charge: i16` with `#[serde(default)]`)
- Modify: `mtg-kernel/src/engine.rs` (`object_has_type` station arm at 10188, `effective_power`/`effective_toughness`/`has_effective_keyword` arms, the cost-target binding for the tapped creature's power, the `CardType::Creature` call-site audit)
- Modify: `mtg-kernel/src/effect.rs` (`EffectOp::PutChargeCountersOnSource`)
- Modify: `mtg-kernel/build.rs` (`station_for`, `activated_ability_recipes_for` with the new `AbilityCostRecipe`, `trigger_recipe_for` for the ETB damage)
- Create: `mtg-kernel/tests/pauper_meta_w2_station.rs`
- Modify: `data/wave_branch_manifests/pauper_meta_w2.json` (this task's cards and their branch lists)

**Interfaces:**
- Consumes: `Decision::ChooseCostTargets` with `CostKind::TapPermanents` (Heap Gate), `sorcery_speed_only` (Experimental Synthesizer), the bestow type-change precedent in `object_has_type`.
- Produces: `CardDef::station`, `Counters::charge`, `EffectOp::PutChargeCountersOnSource`, `AbilityCostRecipe::TapAnotherUntappedControlledCreature`, `DynamicValueDef::BoundCostTargetPower`.

- [ ] **Step 1: Failing tests** (header cites `PinnacleKillShip.java` blob `319a63286f86797a1a06eb00b43802c02c9d35f7`, `StationAbility.java` blob `e918dc283d2531599b9b33673a9fd2c3227219e6`, `StationLevelAbility.java` blob `a021b34bd793bed65bdd84e3be11b7b9b78ae109`), each with its `covers:` line:
  - `kill_ship_etb_deals_ten_to_up_to_one_creature`: with a creature on the battlefield the target may be chosen or declined; with no creature the trigger resolves with no target and does not halt.
  - `station_taps_another_untapped_creature_for_its_power_in_charge_counters`: tapping a 3/3 puts three charge counters on the Ship; the tapped creature is tapped; the Ship itself cannot pay its own cost; a summoning-sick creature may still be tapped for a cost that is not its own tap symbol only if the engine's existing `TapPermanents` rule allows it (mirror `heap_gate_reserves_both_tap_costs_and_treasure_is_exact_one_shot_mana` in `tests/caw_gates_future_v1.rs:429`).
  - `station_with_a_zero_power_creature_adds_no_counters`: tapping a 0/1 Eldrazi Spawn token (reachable in this very deck through Malevolent Rumble and Unfathomable Truths) is a legal activation that adds zero charge counters, because `StationAbilityEffect` gates on `power > 0 && permanent.addCounters(...)`; the tapped token is still tapped afterwards.
  - `station_is_sorcery_speed_only`.
  - `station_below_the_level_leaves_it_a_noncreature_artifact`: at six counters the Ship is not a creature, cannot attack, and is not a legal target for a creature-only removal spell.
  - `station_at_seven_makes_it_a_seven_seven_flier`: at seven counters it is a creature with flying, power 7, toughness 7, can attack, and is a legal creature target; removing a counter is not possible in this pool, so the reverse direction is exercised by building the state directly at six and at seven.
  - `station_counters_survive_a_snapshot_round_trip` and `pre_wave_snapshot_without_charge_counters_still_loads`.

- [ ] **Step 2: Run**: FAIL.

- [ ] **Step 3: Type-change audit before implementing.** Produce the list:
```
grep -n "CardType::Creature" mtg-kernel/src/engine.rs mtg-kernel/src/effect.rs mtg-kernel/src/trigger.rs mtg-kernel/src/rl.rs
```
For every site, classify it as "definition-only (card in a non-battlefield zone, filter over card types)" or "battlefield object" and route the second class through `engine::object_has_type`. Put the classification table in the task report. If more than about fifteen sites need routing, stop and report before continuing: the fallback in design note D is a lead decision.

- [ ] **Step 4: Implement** per design note D.

- [ ] **Step 5: RL surface.** The station activation surfaces as `activatable_abilities` plus `ChooseCostTargets`, both already covered; add a `rl_session` test that pays the station cost through `flat_build_action_cache_v2`. Also assert that the Ship's creature-ness at 7 counters is visible to `rl.rs`'s creature-dependent candidate logic (the two `CardType::Creature` sites in `rl.rs`).

- [ ] **Step 6: Run** `--test pauper_meta_w2_station`, `--test rl_session`, `--test burn_combat` and `--test elves_tribal` (combat and creature-count regressions), module filters, repin, goldens, `--no-run`. Expected: PASS. Every moved state-hash literal records `Counters` gaining `charge` as its cause.

- [ ] **Step 7: Wave manifest and commit.** Add this task's cards to `data/wave_branch_manifests/pauper_meta_w2.json` (one entry per card: `post_board` plus the exact branch names this task's `covers:` annotations use), then run
```
uvx uv@0.11.29 run --no-sync python python/tools/check_wave_branch_coverage_v1.py data/wave_branch_manifests/pauper_meta_w2.json
```
Expected: exit 0 over the cards landed so far. Then commit: `git commit -m "mechanic: station (shared machinery) with Pinnacle Kill-Ship"` and push, with the type-audit summary in the body.

**Effort: 7 hours.**

---

### Task 9: Sideboard cards (Earth Rift, Rooftop Percher, Tangle)

**Files:**
- Modify: `data/cards_v1.json` (append the three)
- Modify: `mtg-kernel/build.rs` (`special_for`, `effect_recipe_for`, `flashback_for`, `keywords_for`, `changeling_for`, `trigger_recipe_for`)
- Modify: `mtg-kernel/src/effect.rs` (`EffectOp::InstallCombatDamagePreventionThisTurn`, the attacking-creature skip-untap batch)
- Modify: `mtg-kernel/src/engine.rs` (combat damage prevention consult in `deal_combat_damage` at 10675)
- Create: `mtg-kernel/tests/pauper_meta_w2_sideboard.rs`
- Modify: `data/wave_branch_manifests/pauper_meta_w2.json` (this task's cards and their branch lists)

**Interfaces:**
- Consumes: `Special::DestroyLand` (Raze, wave 1), `FlashbackDef`, `Keywords::FLYING`, `changeling_for` (Webweaver Changeling), `TargetSpec::UpToTwoCardsInGraveyards` and `AbilityEffectRecipe::MoveAllTargetsToExile` (Faerie Macabre), `EffectOp::InstallDamagePreventionFromColor` (Prismatic Strands), `Special::TapAndSkipNextUntap`'s skip-untap state.
- Produces: `EffectOp::InstallCombatDamagePreventionThisTurn`, trigger recipe `"Rooftop Percher" => "etb:up_to_two_target_cards_in_graveyards:exile:gain_life:3"`.

- [ ] **Step 1: Failing tests**, each with its `covers:` line:
  - `earth_rift_destroys_a_land_and_flashes_back_for_five_r_r`: from hand with `{3}{R}`, then from the graveyard with `{5}{R}{R}`, exiled afterwards.
  - `rooftop_percher_exiles_up_to_two_graveyard_cards_and_gains_three`: two targets chosen from both graveyards; one target; zero targets (life still 20 to 23 and no halt).
  - `rooftop_percher_is_every_creature_type_and_flies`: changeling assertion in the style of `tests/pauper_meta_w1_creatures.rs`'s Webweaver test, plus a flying blocker assertion.
  - `tangle_prevents_all_combat_damage_this_turn`: two attackers and one blocker; after combat, no life or damage changed on any of them, including the blocker's damage to the attacker.
  - `tangle_keeps_attacking_creatures_tapped_through_the_next_untap_step`: the attackers are still tapped after their controller's next untap step, and untap normally the step after; a creature that was not attacking untaps normally.
  - `tangle_does_not_prevent_noncombat_damage`: a Lightning Bolt in the same turn still deals 3.

- [ ] **Step 2: Run**: FAIL.

- [ ] **Step 3: Implement.** Append the three entries and `Subtype::Golem` if not already added by Task 6 or 7 (whichever lands first adds it; the later task asserts it is present rather than adding it twice). Earth Rift reuses `Special::DestroyLand` with `flashback_for("Earth Rift") => {5}{R}{R}`. Rooftop Percher's ETB is a targeted trigger with the up-to-two graveyard target spec and a second gain-life effect that runs regardless of target count. Tangle's first effect installs a combat-damage-only prevention shield for the turn beside the existing color-based shield; the second applies the existing skip-untap marker to every attacking creature at resolution.

- [ ] **Step 4: RL surface.** The up-to-two graveyard targeting on a trigger must appear in `legal_action_candidates_v1` with a finishable selection: add a `rl_session` test covering zero, one, and two selections through `flat_build_action_cache_v2`.

- [ ] **Step 5: Run** the focused file, `--test burn_combat`, `--test rl_session`, module filters, repin, goldens, `--no-run`. Expected: PASS.

- [ ] **Step 6: Wave manifest and commit.** Add this task's cards to `data/wave_branch_manifests/pauper_meta_w2.json` (one entry per card: `post_board` plus the exact branch names this task's `covers:` annotations use), then run
```
uvx uv@0.11.29 run --no-sync python python/tools/check_wave_branch_coverage_v1.py data/wave_branch_manifests/pauper_meta_w2.json
```
Expected: exit 0 over the cards landed so far. Then commit: `git commit -m "cards: Earth Rift, Rooftop Percher, Tangle"` and push.

**Effort: 5 hours.**

---

### Task 10: Register Urzatron (nineteenth registration)

**Files:**
- Modify: `python/tools/generate_pauper_manifests.py` (`REGISTRATION_SPECS` gains `DeckSpec("Urzatron", "Urzatron", "Deck - Urzatron.dek", <sha>)` after `DimirTerrorV2`; `EXPECTED_MAINBOARD_SUPPORT` gains `"Urzatron": {"full": 60, "partial": 0, "no_effect": 0}`; the registry baseline at line 549 moves from `171` to `190` non-token cards; `TOKEN_DEPENDENCIES` gains Malevolent Rumble and Unfathomable Truths as Eldrazi Spawn Token producers; `RUNTIME_DECK_IDS` unchanged)
- Modify: `oracle/xmage/DeterminizationSampler.java` `pauperRegistrationsV2()` (one `paths.put("Urzatron", base + "/Deck - Urzatron.dek");` appended last) and the Mage fork copy (commit and push on `lead/pauper-meta-registrations-v1`)
- Modify: `data/pauper_sideboard_policy_v1.json` `deck_ids` (append `Urzatron`; `plans` stays empty)
- Modify: `python/tests/test_pauper_pool_manifest.py` (totals: `deck_count` 19, unique and copy totals from the generator's check-mode output)
- Regenerate: `data/pauper_pool_v1.json`, `data/pauper_support_v1.json`, `data/cards_v1.json` `decks` membership, `data/flat_policy_v1/*`, `data/flat_policy_v2/*`

- [ ] **Step 1: Source SHA.**
```
uvx uv@0.11.29 run --no-sync python -c "import sys,hashlib; b=open(sys.argv[1],'rb').read().decode('utf-8').replace('\r\n','\n').replace('\r','\n').replace('\n','\r\n').encode(); print(hashlib.sha256(b).hexdigest())" "oracle/xmage/decks/Pauper/Deck - Urzatron.dek"
```
Paste into the `DeckSpec` row.

- [ ] **Step 2: Failing check.** `uvx uv@0.11.29 run --no-sync python python/tools/generate_pauper_manifests.py --check`. Expected: FAIL on the Java registrations method SHA and on the totals.

- [ ] **Step 3: Java roster and re-pin.** Append the `paths.put` line in both copies, run check mode, and copy the new `JAVA_FACTORY_FILE_SHA256` and `JAVA_FACTORY_REGISTRATIONS_METHOD_SHA256` it reports. `JAVA_FACTORY_METHOD_SHA256` (`pauperDefaults`) must not change; if it did, the edit touched the wrong method.

- [ ] **Step 4: Write manifests and goldens.**
```
uvx uv@0.11.29 run --no-sync python python/tools/generate_pauper_manifests.py --write
sha256sum data/runtime_decks_v1.json      # must still be 68e7602f...b851
uvx uv@0.11.29 run --no-sync python python/tools/generate_flat_policy_v1_goldens.py
uvx uv@0.11.29 run --no-sync python python/tools/generate_flat_policy_v2_goldens.py
uvx uv@0.11.29 run --no-sync python python/tools/repin_card_db_identity_v1.py --check
```
Expected: `deck_count: 19`; `cards_v1.json` `decks` membership includes `Deck - Urzatron.dek` for every shared card. The repin check is expected to FAIL here rather than pass: a card's `decks` list feeds the `KERNEL_CARDDB_HASH` canon string (build.rs:7303), so rewriting membership for every shared card moves the hash again (wave 1 precedent d7ca0972). Run `repin_card_db_identity_v1.py --write`, then the two golden generators again, then `--check`, and record the old and new hash for the commit body.

- [ ] **Step 5: Tests.** Update `test_pauper_pool_manifest.py` totals from the generator output, run the Python manifest and golden suites, then `cargo test --locked -p mtg-kernel --test sideboard_v1 --test bo3_session_v1`, adding `bo3_session_accepts_the_urzatron_registration` in the shape of wave 1's `bo3_session_accepts_a_v2_registration`. Also run `--test rl_session` and `--test rl_contract`: this is the first moment a live flat-encoded session can reach the wave 2 cards, so any missing validator arm surfaces here rather than in production.

- [ ] **Step 6: Commit** with the body listing the deck source SHA, pool totals before and after, and the unchanged runtime catalog SHA:
```
git add -A
git commit -m "pool: register Urzatron (19 registrations, runtime set unchanged)

KERNEL_CARDDB_HASH 0x<pre-registration> -> 0x<post-registration>
pool registrations 18 -> 19; runtime catalog sha256 68e7602f...b851 unchanged
Deck - Urzatron.dek sha256 <source sha>"
git push
```

**Effort: 3 hours.**

---

### Task 11: Identity finalisation and wave record

**Files:**
- Modify: `mtg-kernel/src/native_training_store_run_v2.rs` (`FROZEN_CARD_DB_HASH_U64_HEX_PAUPER_META_W1` final value via the repin tool; the variant doc comment now says the profile tracks the card lane's live identity across waves)
- Modify: `python/tests/test_pauper_pool_manifest.py`, `mtg-kernel/src/card_def.rs` test literal (via the repin tool)
- Create: `docs/research/pauper_meta_wave2_record_2026-09.md`
- Modify: `ROADMAP.md` (one paragraph under the card coverage section: wave 2 contents, 19 registrations, certification status)

- [ ] **Step 1: Finalise the pins.**
```
uvx uv@0.11.29 run --no-sync python python/tools/repin_card_db_identity_v1.py --write
uvx uv@0.11.29 run --no-sync python python/tools/repin_card_db_identity_v1.py --check
```
Then the single permitted full sweep:
```
cargo test --locked -p mtg-kernel
uvx uv@0.11.29 run --no-sync python -m unittest discover -s python/tests -v
```
Expected: PASS except the known design canary. Every residual failure is classified in the task report as pin-consequence (with the moved literal and its cause) or a real defect; real defects are fixed with a regression test in the owning card's test file.

- [ ] **Step 2: Recompute coverage.**
```
uvx uv@0.11.29 run --no-sync python python/tools/pauper_meta_gap_v1.py docs/research/pauper_meta_decklists_2026-09-09 "$(cat docs/research/pauper_meta_shares_2026-09-09.json)" docs/research/pauper_meta_gap_after_w2_2026-09.json
```
Write `docs/research/pauper_meta_wave2_record_2026-09.md` in the shape of `pauper_meta_wave1_record_2026-09.md`: the pinned 75 with both substitutions and their frequency arithmetic, the per-archetype coverage table before and after (Urzatron specifically, which starts at 13 percent sampled mainboard coverage), the 19 cards added, the four new mechanic families and every smaller primitive, the Prophetic Prism deferral and its reason, old and new `KERNEL_CARDDB_HASH` (`0xde59c501e943f3fd` to the new value), pool totals 18 to 19, the unchanged runtime catalog SHA, and the certification label "kernel-covered, not XMage-shadow-certified" with a pointer to Task 14b of the wave 1 ledger.

- [ ] **Step 3: Commit**: `git commit -m "docs: Pauper meta wave 2 record; card DB identity finalised"` and push.

**Effort: 4 hours.**

---

### Task 12: Parity gate (branch manifest, checker, one-seed rerun)

**Files:**
- Modify: `data/wave_branch_manifests/pauper_meta_w2.json` (already assembled: Task 2 created it and Tasks 3 to 9 appended to it; this task only completes `unreachable` reasons and verifies)
- Create: `docs/research/pauper_meta_wave2_parity_2026-09.md` and its `.json` companion
- Modify: nothing in `mtg-kernel/src`

**Interfaces:**
- Consumes: `python/tools/check_wave_branch_coverage_v1.py` (unchanged), the `covers:` annotations written by Tasks 2 to 9, `mtg-kernel/examples/rollout_record.rs`.
- Produces: the wave 2 coverage verdict and the one-seed rerun comparison.

- [ ] **Step 1: Verify the manifest.** The manifest was built incrementally by Tasks 2 to 9, each of which ran the checker over the cards landed so far, so this step audits rather than authors: confirm all 19 cards are present with `post_board` set (true for Earth Rift, Rooftop Percher, Tangle) and that every branch a card task declared in its report appears; a branch that is unreachable in this pool belongs in the entry's `unreachable` map with its reason rather than dropped. Then run the whole-wave check:
```
uvx uv@0.11.29 run --no-sync python python/tools/check_wave_branch_coverage_v1.py data/wave_branch_manifests/pauper_meta_w2.json --json docs/research/pauper_meta_wave2_parity_2026-09.json
```
Expected: exit 0, 19 of 19 cards kernel-covered, zero unexercised branches. An annotation naming a card or branch the manifest does not declare fails the checker; fix the annotation, never the checker.

- [ ] **Step 2: Branch build for the rerun.**
```
CARGO_TARGET_DIR=E:/cargo-target-pauper-meta TEMP=E:/tmp/lead TMP=E:/tmp/lead cargo run --locked -p mtg-kernel --release --example rollout_record -- --matchup burn_mirror --games 4 --seed 5151 --out E:/pauper-meta-parity/w2_branch_seed5151
```

- [ ] **Step 3: Baseline (controller ruling: reuse before rebuild).** Compare first against the retained wave 1 artifact `E:/pauper-meta-parity/base_seed5151` (the 75406ffe run, already on disk, no build). Only if that comparison differs outside the excluded fields, build and compare against the wave 1 tip `1ebb3b6b`: check it out read-only in a separate worktree and run the same command with `CARGO_TARGET_DIR=E:/cargo-target-pauper-meta-w1base` and `--out E:/pauper-meta-parity/w1base_seed5151`. Record in the parity record which baseline was used and why.

- [ ] **Step 4: Compare.** Walk every header, decision, and terminal record in `audit_episodes.jsonl`, `policy_episodes.jsonl`, and `manifest.json`, comparing every field except the documented exclusions: `diagnostic_state_hash` and `environment_hash` (cause: `Counters` gained `charge` and `ObjectStateV4` gained `prototyped`, both hashed into the state), `card_db_hash`, `observation_projection_hash`, `observation.visible_projection_hash` (cause: the registry grew by 19 definitions), `git`, `variable_metadata`, `cli_args`. Everything else, including every `legal_actions` entry's full `semantic` object and `stable_id`, must match exactly. Any action or outcome difference is a divergence reported in the record, not fixed here.

- [ ] **Step 5: Write the record** `docs/research/pauper_meta_wave2_parity_2026-09.md` in the shape of the wave 1 parity record: the exact commands, both output directories, the exclusion list with per-field causes, the comparison result, the coverage verdict, and the explicit statement that the wave is kernel-covered and NOT XMage-shadow-certified (Task 14b of the wave 1 ledger is still unstarted, so the shadow harness still cannot play a non-Rally registration).

- [ ] **Step 6: Commit**: `git commit -m "parity: wave 2 branch coverage manifest and one-seed rerun record"` and push. Then post the wave summary to the mailbox and request the Codex review of the whole wave.

**Effort: 4 hours.**

---

### Task 13: Wave 1 deferred minors and wave 2 polish

Wave 1's final review recorded that "all ledger-deferred minors carry to wave 2". This task closes them; each is a one-line or one-paragraph fix with no behavior change, and the whole task is one commit.

**Files and items:**
- `mtg-kernel/src/sideboard.rs:213`: doc comment still says "nine-deck pool".
- `python/tools/generate_pauper_manifests.py` module docstring: still names `pauperDefaults()` and nine files as the roster source of truth.
- `python/tools/repin_card_db_identity_v1.py`: `_classify_protected_line` checks the allow-list before the protected list by substring (latent); add the `find_occurrences` test for a stale lane-profile value.
- `python/tools/check_wave_branch_coverage_v1.py`: the contiguity diagnostic reports false-positive `test_without_fn` warnings on pre-existing files with multi-line `#[ignore = "..."]` attributes.
- Nine stale "ruling 6 pending" comments across four files (`grep -rn "ruling 6 pending" mtg-kernel/src python/tools`): design ruling 6 is RULED as of 2026-09-10.
- `mtg-kernel/src/engine.rs` `push_paid_activation`: duplicated printed-versus-granted index boundary.
- `mtg-kernel/examples/walk_diff.rs`: re-derives `return_permanent_payable` from engine state instead of the decision, plus a redundant branch rendering the same text.
- `mtg-kernel/build.rs` `supported_omen`: lacks the adventure and bestow exclusions its siblings have (unreachable today, and Task 7 adds prototype to the same family, so fix it there or here, once).
- `mtg-kernel/tests/pauper_meta_w1_spells.rs:143`: comment misattributes the `castable_spells` exclusion convention.
- `python/tools/write_dek_from_mtgo_list_v1.py`: `escape_attr` entity escaping untested.
- `python/tests/test_repin_card_db_identity_v1.py`: the release-layout test calls `parse_live_hash` directly rather than through the env-var wiring (wave 1 ledger line 41, still unfixed).

- [ ] **Step 1:** Fix each item, adding a test where the item is a tool behavior (the repin tool's `find_occurrences`, the checker's contiguity diagnostic, `escape_attr`).
- [ ] **Step 2:** `cargo test --locked -p mtg-kernel --no-run` plus the Python tool tests.
- [ ] **Step 3: Commit**: `git commit -m "polish: close the wave 1 deferred minors"` and push.

**Effort: 3 hours.**

---

## Effort summary

| task | subject | hours |
|---|---|---|
| 1 | Pinned 75 and the Urzatron `.dek` | 2 |
| 2 | Tron lands and conditional mana yield | 7 |
| 3 | Utility lands and mana artifacts | 5 |
| 4 | Green library manipulation | 5 |
| 5 | Bramble Wurm and Unfathomable Truths | 3 |
| 6 | Cascade and Maelstrom Colossus | 12 |
| 7 | Prototype and Boulderbranch Golem | 6 |
| 8 | Station and Pinnacle Kill-Ship | 7 |
| 9 | Sideboard cards | 5 |
| 10 | Register Urzatron | 3 |
| 11 | Identity finalisation and wave record | 4 |
| 12 | Parity gate | 4 |
| 13 | Deferred minors and polish | 3 |
| | **total** | **66** |

Sixty-six implementer-hours, excluding review workflows and fix rounds, which added roughly one hour per task in wave 1. The spec's work-breakdown row 5 estimated "2 to 3 days" for W2; that estimate predates three findings: Pinnacle Kill-Ship's station mechanic needs a type-changing seam (design note D), the Tron lands need a payment-planner change because the solver models one mana per tap (design note A), and cascade needs a dedicated exile route with an incarnation marker plus seven route-aware cost and validation arms and its own randomization ordinal, because no existing free-cast route admits a card without a Plot ability and the frozen encoder forbids a new cast method (design note B). At wave 1's dispatch-and-review cadence, 66 hours is about four calendar days.

## Self-review notes

- Spec coverage: section 3's append-only registry and subtype ids (Global Constraints), deck-membership rule (the Prophetic Prism deferral), XMage-absent substitution disclosure (the registration section), visible faces (this wave adds none, stated explicitly); section 5.1's W2 card list (all of it except Prophetic Prism, with the reason); section 5.2's "cascade and prototype are enumerated mechanics with shared machinery priced once at planning time" (design notes B and C, plus station in D, which section 5.2 did not anticipate); section 5.3 step 1 (Tasks 1 and 10), step 2 (Tasks 2 to 9 registry appends), step 3 (the same tasks' programs), step 4 as amended 2026-09-10 (Task 12: `covers:` annotations, the branch manifest, the checker, the one-seed rerun, and the "kernel-covered, not XMage-shadow-certified" label), step 5 (no new faces or tokens, stated in the mechanics section), step 6 (Tasks 10 and 11). Ruling 6's schema-migration surface in this wave is three new persisted fields (`ObjectStateV4::prototyped`, `ObjectStateV4::cascade_grant`, `Counters::charge`), each with `#[serde(default)]` and a pre-wave fixture test, plus one new `SpellCastRouteV4` variant. There is no model-input change: revision 3 removed it, the frozen encoder is untouched, and Task 6 carries a test asserting both enum-mapping versions are still 1. No new catalog profile.
- Not in this plan: the sideboard plan table and classifier (superseded or deferred by the 2026-09-10 rulings), Bo3 measurement, the shadow-harness generalization (wave 1 ledger Task 14b), the deferred XMage-absent cards (spec W7), and Prophetic Prism.
- Type names introduced here and reused across tasks: `DynamicValueDef::AmountIfControllerControlsEach`, `DynamicValueDef::SourcePermanentPower`, `DynamicValueDef::BoundCostTargetPower`, `SubtypeConjunctionDef`, `CardDef::conditional_tap_yield`, `CardDef::cascade`, `CardDef::prototype`, `PrototypeDef`, `CardDef::station`, `StationDef`, `ManaSource::yield_per_tap`, `PaymentPlan::surplus`, `ManaAbilityCostDef::None`, `EffectOp::AddManaDynamic`, `EffectOp::CascadeFromSource`, `EffectOp::Surveil`, `EffectOp::SearchLibraryToBattlefieldUntapped`, `EffectOp::EachPlayerControllingDefinitionDrawsCard`, `EffectOp::PutChargeCountersOnSource`, `EffectOp::InstallCombatDamagePreventionThisTurn`, `EffectBooleanChoicePurpose::CascadeFreeCast`, `SpellCastRouteV4::CascadeExile`, `ObjectStateV4::cascade_grant`, `ShufflePurposeV2::CascadeBottomOrder`, `LibraryCardFilter::AnyLand`, `Special::LookTopSelectFilteredToHandRest`, `LookSelectFilter`, `LookRest`, `CostComponent::ExileSourceFromGraveyard`, `AbilityCostRecipe::TapAnotherUntappedControlledCreature`, `ObjectStateV4::prototyped`, `Counters::charge`, `Subtype::{Urzas, Tower, PowerPlant, Mine, Desert, Spacecraft, Wurm, Golem}`.
- Cross-task ordering hazards: `Subtype::Golem` is needed by both Task 6 and Task 7 and `Subtype::Wurm` by Task 5; whichever lands first appends it and the later task asserts its presence. Tasks 2 to 9 each leave `python/tests/test_pauper_pool_manifest.py`'s roster-membership test red until Task 10, by the wave 1 ruling. Tasks 6, 7 and 8 each move every pinned state hash (`ObjectStateV4::cascade_grant`, `ObjectStateV4::prototyped`, `Counters::charge`), so each records its own cause and Task 11 re-verifies rather than re-explaining. Task 7's `evaluate_dynamic_value` signature change touches the call site Task 2 introduced, so Task 7 runs after Task 2, as ordered.
- Known risk the plan does not remove: station's type-changing audit is the one item that could exceed its estimate. Its fallback (a `partial` capability for Pinnacle Kill-Ship plus a declared unreachable branch) is written down so the decision is made in the open, by the lead, with the support totals changed explicitly rather than silently.
- Revision 4 changes, for a reviewer comparing against revision 3 (re-review round 2, five findings, all adopted in design note B and Task 6 only): the route-aware list grows from five sites to seven with the two exhaustive `SpellCastRouteV4` matches at engine.rs 1577-1649 and 1894-1911, and the plan states the `def.cascade` versus `source.v4.cascade_grant` pitfall explicitly; the payment and payability arms now say how a kicked cascade still pays kicker under CR 702.85, with a Goblin Bushwhacker test; `ShufflePurposeV2::CascadeBottomOrder` gains a named per-player ordinal (`next_cascade_bottom_ordinal` with a preflight and commit pair) and a two-cascade test; `cast_cost_is_payable` is corrected to `remaining_cast_payment_is_payable` (engine.rs 5989); Task 6 Step 5 is repointed from the purpose-agnostic rl.rs 2388 to the mandatory exhaustive arm in `pending_effect_semantic_v4` (rl.rs 5632-5656); Task 6 is re-estimated 10 to 12 hours.
- Revision 3 changes, for a reviewer comparing against revision 2 (controller ruling): no new `CastMethodV4` variant, so no edit to `flat_policy_v1.rs`, `flat_policy_v2.rs`, `features.py`, or the flat goldens for cascade; the free cast is an ordinary `CastMethodV4::Normal` cast on the new `SpellCastRouteV4::CascadeExile` with the unchanged `cascade_grant` marker, made free at five route-aware engine.rs sites; design note B carries the spike evidence for rejecting both `Plotted` and the preferred `Alternative`; Task 6's model-input migration step is deleted and the task is re-estimated 12 to 10 hours; Task 6 gains an encoder-invariance test and an additional-cost test; the Global Constraints gain the frozen-encoder rule.
- Revision 2 changes, for a reviewer comparing against revision 1: design note B rewritten around the spike (route, marker, named shuffle target; its cast-method choice was superseded by revision 3); Task 6 restructured into eight steps at 12 hours with an exhausted-library test; Task 2 gains the active-nine `PaymentPlan` invariance fixture and test (7 hours); Task 7 gains the `evaluate_dynamic_value` source-parameter step with its five call sites and renumbers; Task 8 gains the zero-power station tap test; Tasks 2 to 9 now write `data/wave_branch_manifests/pauper_meta_w2.json` as they land (Task 2 creates it) and run the checker, so Task 12 only verifies; Task 12's baseline reuses the retained `base_seed5151` artifact and builds `1ebb3b6b` only on a difference outside the excluded fields; Task 10 expects the membership rewrite to move the card DB hash again and carries the hash line in its commit body; Task 1's dek-writer expectation corrected; the `EXPECTED_DECKS` prose corrected to build.rs:1892's empty-`decks` panic; Task 13 gains the `parse_live_hash` ledger minor.
