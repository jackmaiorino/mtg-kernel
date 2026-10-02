# Pauper meta card coverage and sideboarding: design v1

Date: 2026-09-09. Author: LEAD (Fable). Revision 3 (after Codex review rounds 1 and 2, gpt-6-astra; all findings adopted, see the countersignatures under mtg-kernel-gae-lane). Status: DRAFT for Jack's rulings (section 9). Branch: `lead/pauper-meta-cards-v1` (from the main tree head 75406ffe). Nothing here changes a training run, a pre-registered constant, or the main tree.

## 1. Goal

Jack (2026-09-09): "work towards implementing more cards into the kernel and potentially planning out sideboarding. use mtggoldfish or other sources to prioritize pauper meta relevant cards."

Two outcomes:
1. The kernel represents and simulates the cards League opponents actually play, in meta-share order, so that (a) the MTGO visible scorer stops abstaining on unknown names and (b) a later training cycle can include those decks.
2. Sideboarding exists end to end for the registered decks: a ratified plan table behind the existing deterministic policy, an archetype classifier with an explicit Unknown outcome for MTGO where the opponent's deck id is hidden, the roadmap's best-of-three gate, and the reserved slot for a trained sideboard head.

Four claims stay separate throughout: representation (a card has a validated program), classifier quality, post-board rule validity, and trained playing competence. This design delivers the first and the infrastructure for the second and third.

## 2. Evidence

Source: mtgtop8 Pauper metagame, "Last 2 Weeks" window ending 2026-09-09, 779 decks; 120 decklists sampled (8 most recent per archetype, 15 archetypes) through mtgtop8's MTGO export, preserved under `docs/research/pauper_meta_decklists_2026-09-09/` with retrieval metadata, aggregated in `docs/research/pauper_meta_gap_2026-09-09.json` by `python/tools/pauper_meta_gap_v1.py` with shares in `pauper_meta_shares_2026-09-09.json`. Exposure weight = share x unconditional average mainboard copies across the archetype's sampled decks (sideboard copies at half weight). The sampled archetypes total 87 percent of the field; the remaining 13 percent is unmeasured. mtggoldfish and mtgdecks refuse automated fetches; mtgdecks' visible summary names Grixis Madness and Mono Red Madness as its top archetypes, which mtgtop8 files under Burn ("Madness Burn") and Red Deck Wins.

Kernel state: 162 registry definitions (150 pool cards plus 12 tokens), all fully supported; nine pool decks (Wildfire, Rally, Affinity, Elves, Spy, Burn, Terror, CawGates, Faeries) with 60 plus 15 cards each; `pauper_sideboard_policy_v1.json` has zero plans and a keep-registered default.

| archetype (mtgtop8) | share | nearest kernel deck | sampled mainboard copies covered | missing mainboard names (avg >= 1) |
|---|---|---|---|---|
| Burn (Madness Burn) | 12% | Burn | 93% | 1 |
| Mono Blue Aggro (Delver, Faeries) | 12% | Faeries | 90% | 2 |
| Affinity (Grixis) | 10% | Affinity | 84% | 4 |
| Urzatron | 9% | none | 13% | 16 |
| Red Deck Wins | 6% | Rally | 94% | 2 |
| Jund (Wildfire) | 5% | Wildfire | 94% | 1 |
| Elves | 5% | Elves | 90% | 2 |
| Gruul Aggro | 4% | none | 42% | 11 |
| White Weenie | 4% | none | 10% | 11 |
| Ephemerate (Snow Jeskai) | 4% | none | 30% | 16 |
| Dimir Control (Dimir Terror) | 4% | Terror | 70% | 5 |
| Balustrade Spy | 4% | Spy | 100% | 0 |
| Golgari or Jund Garden | 3% | none | 57% | 9 |
| Mono Blue Terror | 3% | Terror | 95% | 1 |
| Gates | 2% | CawGates | 74% | 5 |

Nine archetypes totaling 59 percent of the field are within one to five mainboard names of full sampled coverage; Spy (4 percent) is already complete. Sideboard coverage is reported separately in the research report. These are sampled-mainboard figures, not a claim about the unmeasured field, and each wave's projected coverage is recomputed from the wave's actual registrations, not from the union tables.

## 3. Constraints that shape the design

- Registry identity is append-only. Card ids are array indexes of `cards_v1.json` (`card_id_by_name` in the generated code), so existing entries are never deleted or reordered; new cards append.
- Deck registrations are frozen. `build.rs` pins each deck's id, pool order, source path, source hash, and materialized hash in `EXPECTED_DECKS`, and refuses registry cards that belong to no pool deck. A refreshed list is therefore a new registration under a revisioned id (for example `Burn_v2`), not an edit of `Burn`; historical registrations stay in the pool so their cards keep legitimate membership and CP7 anchors and recorded results stay attached to them. The registration catalog retains history; a separate active-id selection controls sampling and sideboard policy coverage. Today's loaders assume the two sets are the same (the sideboard policy loader in `sideboard.rs` requires exactly nine pool registrations whose ids equal the policy's ids), so the loaders and the manifest validation are updated to distinguish the catalog from the active set while keeping an explicit historical CP7 lookup. The active set is selected explicitly per registration change, and the catalog, sampler, runtime catalog, policy deck set, and manifests are updated together.
- The pool is bound to XMage: `data/pauper_pool_v1.json` pins the vendored `oracle/xmage/DeterminizationSampler.java` file and its `pauperDefaults()` method SHA, mirrored from the Mage fork's AIRL plugin, with the `.dek` files under `oracle/xmage/decks/Pauper`. Adding a registration edits both copies, commits the Mage fork on a lead branch, and re-pins the manifest.
- Card programs are Rust: metadata comes from `data/cards_v1.json`, simple programs are generated by `build.rs` from mechanics tags, and everything else is hand-written `EffectOp` programs, trigger tables, and target specs. ROADMAP's rule stands: no card-specific pending state. New card-neutral primitives are permitted (section 5.2); reserved identities are unchanged.
- Every wave moves `KERNEL_CARDDB_HASH`, the Flat V2 goldens, and the contract digests. The branch is a card lane; landing on the main tree happens at a campaign boundary under an owner ruling, as for the MTGO deployment branch (design MTGO_LEAGUE_INTEGRATION_RESUMPTION_V1 section 6.1). If the MTGO deployment branch merges the card lane earlier, the resulting deployment identity is requalified (checkpoint compatibility, registry pins, contract digests) before any use.
- XMage is the oracle. Cards absent from the pinned Mage fork (72a08a3b) cannot enter the registry: Giant's Boulder, Utrom Monitor, Leonardo Big Brother, Sewer-veillance Cam, Call Damage Control, Elite Interceptor, Pursue the Past. Each wave that would include one discloses the substitution it makes in the registered 75 (the sampled next-most-common card in that slot), and the MTGO unknown-card policy keeps failing closed on the absent names.
- Every visible face must resolve. MTGO correspondence resolves exact registry names (`card_correspondence.rs`), so a transforming card, an adventure, or a generated token adds its back face, adventure name, or token name to the visible-name projection, with face identity represented explicitly over the underlying card identity. A wave claims to remove an unknown-name abstention only after the scorer input path resolves the name.
- The model's card vocabulary is a fixed 65,537-token table (`CARD_VOCAB_SIZE_V1`), so new registry ids need no architecture change, but their embedding rows are untrained until a training cycle includes the new decks. Representation lands now; competence follows training, which is a science-lane decision (section 9).

## 4. Options

A. Drift refresh only: register current versions of the nine decks under revisioned ids and add the cards that complete them. Cheapest. Its coverage gain is unquantified until the exact registrations and substitutions are selected and recomputed.

B. Staged meta expansion (recommended): option A as wave 1, then new registrations in meta order (Urzatron, Gruul, White Weenie, Ephemerate, Garden), each wave a reviewed, testable increment with its own pinned 75, sampler entry, and manifests. The provisional order follows meta share; the implementation sequence within it is set by mechanic dependencies and recomputed coverage gain at wave planning time.

C. Representation without decks: implement cards with no pool membership. Blocked by the deck-membership rule; relaxing it would let unplayed cards into the registry unvalidated. Rejected.

## 5. Card waves

5.1 Wave contents. The lists below are the candidate cards per wave (sampled mainboard average at or above 0.75 plus sideboard staples at or above 0.75, XMage Java line count in parentheses as a rough size only). They are not the registrations. Before a wave is implemented, its plan pins one exact source 75 per registration (a specific sampled decklist, cited by mtgtop8 id, adjusted only by disclosed substitutions for XMage-absent cards); a materially different variant (for example Dimir Terror versus Mono Blue Terror) is a separate registration, never an average. Three preserved lists (Ephemerate 886994, Terror 887264, Urzatron 888069) hold 61 mainboard cards; they stay unchanged as research inputs and are ineligible as sources unless an additional cut is authorized and disclosed. `python/tools/pauper_meta_source_lists_v1.py` proposes the representative list per archetype (`docs/research/pauper_meta_source_lists_2026-09-09.json`).

W1 Drift refresh, nine revisioned registrations (about 24 to 30 cards):
- Burn_v2 (Madness Burn): Kessig Flamebreather (43), Melded Moxite (48); sideboard Smash to Smithereens (71).
- Faeries_v2 (Mono Blue Delver): Delver of Secrets (90, back face Insectile Aberration), Brinebarrow Intruder (46), Cryoshatter (53); Sewer-veillance Cam substituted.
- Affinity_v2: Glint Hawk (89), Ancient Den (31); Utrom Monitor and Giant's Boulder substituted.
- Rally_v2 (Red Deck Wins): Inventor's Axe (53), Gingerbrute (60); sideboard Raze (39), Tormod's Crypt (39), Smash to Smithereens.
- Wildfire_v2: Gixian Infiltrator (42); sideboard Ancient Grudge (37).
- Elves_v2: Jaspera Sentinel (46), Birchlore Rangers (95).
- Terror_v2 (Mono Blue Terror): Snow-Covered Island (36). If the wave plan also registers the Dimir list, DimirTerror is its own registration: Contaminated Aquifer (40), Snuff Out (51), Abandon Attachments (35), Augur of Bolas (43), Gurmag Angler (37), Ice Tunnel (43), Thorn of the Black Rose (44); sideboard Arms of Hadar (43).
- CawGates_v2: Malevolent Rumble (37), Manor Gate (48), Plains (basic), Cliffgate (48), Armadillo Cloak (59); Pursue the Past substituted.
- Spy_v2: sideboard Nylea's Disciple (41), Fang Dragon (45).

W2 Urzatron: Urza's Tower, Urza's Power Plant, Urza's Mine (37 each), Expedition Map (45), Bramble Wurm (57), Barrels of Blasting Jelly (48), Ancient Stirrings (39), Crop Rotation (40), Unfathomable Truths (37), Maelstrom Colossus (36, cascade), Pinnacle Kill-Ship (51), Bonder's Ornament (79), Boulderbranch Golem (44, prototype), Malevolent Rumble, Conduit Pylons (47), Bojuka Bog (41), Prophetic Prism (41); sideboard Earth Rift (39); Giant's Boulder and Call Damage Control substituted.

W3 Gruul Aggro: Arbor Elf (46), Utopia Sprawl (98), Eldrazi Repurposer (51), Boarding Party (41, cascade), Wild Growth (52), Jewel Thief (47), Mwonvuli Acid-Moss (45), Annoyed Altisaur (44, cascade), Thermokarst (69), Ram Through (97), Structural Distortion (44); sideboard Deglamer (33), Suplex (41).

W4 White Weenie: Plains, Thraben Inspector (38, Clue token), Novice Inspector (38, Clue token), Battle Screech (48, Bird token, flashback), Raffine's Informant (38), Kor Skyfisher (46), Lunarch Veteran (62), Guardians' Pledge (42), Idyllic Grange (64), Spider-Man Web-Slinger (40), Ramosian Rally (51); sideboard Martyr of Sands (62), Standard Bearer (39), Holy Light (43); Leonardo Big Brother and Elite Interceptor substituted.

W5 Ephemerate: Snow-Covered Island, Snow-Covered Plains, Snow-Covered Mountain, Mulldrifter (44, evoke), Ephemerate (36, rebound), Perilous Landscape (64), Skred (73), Archaeomancer (43), Augur of Bolas, Volatile Fjord (43), Glacial Floodplain (43), Bender's Waterskin (35), Ride's End (54), God-Pharaoh's Faithful (48), Sunscape Familiar (54), Azorius Chancery (45), Union of the Third Path (34).

W6 Garden: Khalni Garden (39, Plant token), Defile (45), Tithing Blade (59), Crypt Rats (59), Campfire (90), Bojuka Bog, Witch's Cottage (66), Haunted Mire (40), Nutrient Block (42), Snuff Out, Golgari Rot Farm (45), Cauldron Familiar (54), Pestilence (48); sideboard Drown in Sorrow (35), Rancid Earth (48).

W7 Oracle refresh and deferred cards: after the Mage fork is advanced to a version implementing the TMNT and later sets (an owner ruling: the oracle snapshot and the CP7 baseline are requalified together), add the seven deferred cards as further revisioned registrations.

5.2 Generic primitives the waves need (new card-neutral `EffectOp` shapes, each with its own unit tests, none card-specific):
- Delver of Secrets: private look at the top card, optional public reveal, conditional transformation in place (the existing `TransformSagaSource` exiles and returns the source and is not usable).
- Glint Hawk, Kor Skyfisher: resolution-time non-targeting permanent selection with optional return-or-sacrifice (Hawk) and mandatory return (Skyfisher).
- Utopia Sprawl, Wild Growth: a land aura with parameterized attachment restriction and mana output; Sprawl enchants a Forest, chooses a color as it enters, and adds that color; Wild Growth enchants any land and adds green; in both the triggered mana is delivered immediately to the land's controller.
- Campfire: graveyard-to-library batch move plus shuffle, composing the existing exile-source cost; the commander clause is unreachable in Pauper and is documented as such.
- Battle Screech: flashback with a tap-three-untapped-creatures-of-a-color cost and a white Bird token.
- Crypt Rats: black-only X payment with one simultaneous global damage batch. Pestilence: independent {B} activations, each dealing one simultaneous damage batch, plus its end-step intervening-if sacrifice trigger.
- Ram Through: two independently validated targets, creature-source damage, and excess-damage-to-controller allocation resolved simultaneously.
- Cascade (Maelstrom Colossus, Boarding Party, Annoyed Altisaur), prototype (Boulderbranch Golem), evoke (Mulldrifter), rebound (Ephemerate), Clue and Plant tokens: each is an enumerated mechanic with shared machinery, priced once at wave planning time, not per card.
The wave plan enumerates the new mechanics, tokens, and faces before estimating effort; line counts are not the estimate.

5.3 Per-wave procedure (one plan document per wave, subagent-driven, Codex review before the wave is called done):
1. Pin the source 75s: cite each decklist, disclose substitutions, and register under a revisioned id in the `.dek` directory, the vendored sampler, the Mage fork (lead branch commit), `pauper_pool_v1.json`, the runtime catalog, the sideboard policy deck set, and the manifests, all in one commit with old and new hashes in the body.
2. Registry: append `cards_v1.json` entries derived from the XMage Java source (cost, types, subtypes, power, toughness, mana production, mechanics tags, `java_file`, faces and tokens). The isolated candidate build may declare implemented programs provisionally `full`, because manifest generation and token materialization require it to run the parity checks at all; the wave is neither certified nor promoted until the declared parity checks in step 4 pass.
3. Programs: tag-generated where `build.rs` supports the tags; hand-written programs and triggers composing existing or new generic shapes (5.2) for the rest.
4. Parity: directed CP7 cases that exercise every added card's relevant rules branches. AMENDMENT 2026-09-10 (lead ruling, recorded in the wave 1 ledger): the CP7 shadow harness is hardcoded to the Rally mirror on both sides (XMageRallyAnchorSpike pins one .dek for both seats with no deck flag; the kernel scorer's Reset request has no deck field and hardcodes Rally; the mapper and policy classes carry Rally-specific logic), so shadow-mapped games with new registrations need a harness generalization that is its own plan. Until it lands, a wave's parity evidence is kernel-side: every declared rules branch bound to a named kernel test that exercises it (a `covers:` annotation checked by python/tools/check_wave_branch_coverage_v1.py) plus the standing one-seed rerun, and the wave is labeled "kernel-covered, not XMage-shadow-certified". XMage certification follows the harness generalization (alternate costs, faces, tokens, declined choices, illegal-target cases), including post-board configurations for sideboard-only cards; a record of exercised branches and unresolved discrepancies; unexercised branches do not qualify. Existing regression replays stay green; the standing one-seed bit-identical output-store rerun passes; the ordinary matchup smoke is supplementary.
5. Visible names: MTGO correspondence and projection resolve every new name, face, and token through the scorer input path.
6. Goldens and manifests regenerated (`generate_pauper_manifests.py`, `generate_flat_policy_v1_goldens.py`, `generate_flat_policy_v2_goldens.py`) and the Python manifest tests updated with the new counts; the commit body records old and new `KERNEL_CARDDB_HASH`, pool counts, and deck hashes.

## 6. Sideboarding

6.1 Plan table. Author `pauper_sideboard_policy_v1` plans for every ordered pair of active registered decks including mirrors (nine decks: 81 pairs), with deliberate behavior for every post-board physical game index (`plan_for_v1` keys on self deck, opponent deck, and game index; a game-two entry does not cover game three, and draws can extend a match). Class templates (aggro, control, combo, artifact, graveyard) with explicit precedence and per-matchup overrides may reduce authoring effort, but the generated concrete table is the audit and ratification unit. Each plan applies to its exact registered 75 and is validated for availability, conservation, and 60 plus 15 sizes by the existing `SideboardPlanV1` rules. Play or draw is intentionally ignored in this version. Jack ratifies the table before any Bo3 measurement or League use, and again after every registration change.

6.2 MTGO archetype classifier. A deterministic classifier over sanitized opponent-visible facts from the declared completed games (card names only) with outcomes: a registered deck id, or Unknown. Its constants are ratified before measurement or League use: per-deck signatures and weights, minimum evidence count, absolute acceptance threshold, runner-up margin, tie handling, contradictory-evidence rules, deduplication, and observation window. It is evaluated offline on frozen visible prefixes from known pool decks and from outside-pool decks before it is trusted; a winning margin alone does not detect an opponent outside the pool. Every pool change re-ratifies the constants because it changes the competing scores.

6.3 Controller contract. The classifier lives in an `MtgoCompetitiveNativeSideboardScorerV1` implementation that provides both the whole-target and the sequential scoring interfaces and the completed-match-history consumer; each history import replaces the previous state. Unknown bypasses the registered-opponent lookup and preserves the exact current configuration (today's placeholder behavior). A known classification derives an absolute target 60 plus 15 from the verified registered 75 and the ratified plan, then reconciles the current configuration to that target (the kernel default rebuilds the registered configuration, which differs from the current one after game-two boarding, so the reconciliation is explicit). Receipts carry the classifier version, per-deck scores, abstention reason, and selected plan id.

6.4 Bo3 gate (ROADMAP "Deferred sideboard and BO3 gate"). ROADMAP requires accepted sampled-primary BO1 evidence before BO3 work; a changed pool does not inherit that acceptance. The wave plan either identifies the accepted BO1 evidence that applies to the registrations under test or obtains an explicit sequencing exception from Jack. Before measurement: a frozen one-page design, integer win gates, matched seeds, permuted controls, the paired-bootstrap unit (matches for match-win claims), and the standing small-run manifest. Then post-board goldens and reference replays for graveyard hate, color hosers, artifact hate, sweepers, and alternate win lines, then the sampled-primary and greedy-secondary post-board matrices with BO1's failure rules. Measurement only: the table is frozen first.

6.5 Trained sideboard head. Reserved behind the same slot. Its training would be Bo3 self-play over the registered 75s with terminal match win-loss as the only reward; whether and when to spend a training cycle on it is a science-lane decision outside this design.

## 7. Work breakdown

| # | Item | Depends on | Size |
|---|---|---|---|
| 0 | Research report, decklists, and this design on the card lane; Codex review | none | done |
| 1 | W1 plan: pin nine source 75s, enumerate primitives (Delver, Hawk), cost | ruling 1 | half a day |
| 2 | W1 implementation (revisioned registrations, about 24 to 30 cards, primitives, directed CP7 cases) | 1 | 3 to 4 days |
| 3 | Sideboard plan table for the nine W1 registrations (6.1) | 2 | 1 day plus ratification |
| 4 | Classifier and controller (6.2, 6.3) on the deployment branch, offline evaluation | 3 | 1 to 2 days |
| 5 | W2 Urzatron (cascade, prototype) | 2, ruling 2 | 2 to 3 days |
| 6 | W3 Gruul (Sprawl, Ram Through), W4 White Weenie (tokens, flashback) | 5 | 2 to 3 days each |
| 7 | W5 Ephemerate (evoke, rebound), W6 Garden (Crypt Rats, Pestilence, Campfire) | 6 | 2 to 3 days each |
| 8 | Bo3 goldens and matrices (6.4) | 3, BO1 evidence or exception | measurement window |
| 9 | W7 oracle refresh and deferred cards | ruling 6 | after the upgrade |
| 10 | Main-tree landing of the card lane | ruling 5 | one merge |

## 8. Risks

- Each wave changes the pool identity and the card DB hash; a landing mid-campaign would break contract digests, so the lane stays unmerged until the boundary, and a deployment-branch merge is requalified.
- Hand-written programs and new primitives can diverge from XMage; the directed CP7 cases in 5.3 step 4 exist to catch that before a wave is called done.
- The sampled window is two weeks and covers 87 percent of the field; shares move. The ranking is regenerated from the preserved decklists, so a re-run before each wave is cheap.
- Historical registrations stay in the pool, so the registry grows by the union of old and new lists; training over the active set only is a selection, not a deletion.

## 9. Owner rulings required

1. Approve W1 as revisioned registrations of the nine decks (historical registrations retained, active set switched explicitly), including the sampler re-pin in the vendored copy and the Mage fork. RULED 2026-09-10 (Jack, in session: "1 and 2 approved" to the lead's list that bundled card rulings 1, 2, 5, 6): approved.
2. Approve pool expansion with new registrations in the stated provisional order (W2 to W6), understanding that the trained checkpoint gains competence on them only through a later training cycle over the expanded active set, which is a separate science-lane decision. RULED 2026-09-10 (Jack, in session: "1 and 2 approved" to the lead's list that bundled card rulings 1, 2, 5, 6): approved.
3. Ratify the sideboard plan table (6.1) as pre-registered constants when it is authored, and again after each registration change. RULED 2026-09-10 (Jack, in session, evening: "ok i accept all your recommendations"): superseded by the search-generated tables (deck-model design section 4); no separate ratification.
4. Ratify the classifier constants (6.2) as a disclosed non-model component, and confirm the Unknown-preserves-current rule (6.3). RULED 2026-09-10 (Jack, in session, evening: "ok i accept all your recommendations"): deferred until the deck model v1 exists (with deck-model ruling 12).
5. Confirm the landing rule: the card lane merges to the main tree only at a campaign boundary under a ruling; a deployment-branch merge requalifies the deployment identity. RULED 2026-09-10 (Jack, in session: "1 and 2 approved" to the lead's list that bundled card rulings 1, 2, 5, 6): approved.
6. Any schema migration required by any wave receives an owner ruling before landing or deployment; W7 additionally requires the oracle and toolchain upgrade of the Mage fork with a CP7 baseline requalification. RULED 2026-09-10 (Jack, in session: "1 and 2 approved" to the lead's list that bundled card rulings 1, 2, 5, 6): approved.
7. Rule on the Bo3 sequencing: which accepted BO1 evidence applies to the revisioned registrations, or grant a sequencing exception (6.4). RULED 2026-09-09 (Jack, in session: "i feel like its fine to work now"): exception granted. Sideboard plan authoring, the classifier, and Bo3 measurement proceed before the sampled-primary 9x9 BO1 matrix is accepted, as deployment work; nothing measured under this exception is reported as a "Pauper match" or BO3 promotion result, and the ROADMAP BO1 wording rule stands for science claims.
