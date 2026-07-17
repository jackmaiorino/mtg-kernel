# MTG Kernel Science-Ready Roadmap

Status: active
Roadmap baseline: standalone commit `2f63aec4de0dbed6a5ee9b01933decfe630c07ec` (`Kernel: Prove evaluator failure boundaries`; rewritten from Mage commit `a78e9a2f81b0fdd7eec4c692c837e25656794ebb`)
Repository status: standalone cutover complete; this repository is now the active development line. See `EXTRACTION_PROVENANCE.md` for the exact Mage-to-kernel checkpoint mapping.
Scope: the nine canonical Pauper decks below in best-of-one play. Sideboarding and best-of-three are a separate, deferred gate.

## Decision summary

The kernel is science-ready when it can train and evaluate policies over all nine pinned BO1 decks without unsupported cards or mechanics, reproduce every accepted run from a clean clone on its designated hardware under its published runtime compatibility tuple and versioned hardware record, and publish a complete seat-balanced 9x9 matchup matrix whose primary result uses sampled policy actions. Greedy evaluation remains a required secondary diagnostic, not the promotion result.

These nine decks are the complete Pauper meta pool carried forward from the previous project. A result over Burn/Rally, a convenient subset, or only the newly implemented decks cannot close this roadmap: all nine mainboards must be implemented, trained against, and evaluated in the final full-pool protocol before any concluding science claim.

The current checkpoint has a strong deterministic RL and artifact foundation, but complete-deck coverage is still Burn plus Rally. The deliberately frozen schema-v4 engine/H2 semantic layer still supplies the generic state, hidden-information, cost, relation, continuation, choice, and aggregate combat shapes needed by the remaining decks; synchronized public policy schema v5 now wraps that layer without changing its rules behavior. Policy v5 replaces exponential combat subsets with canonical binary include/exclude scans and adds exact physical-decision grouping across Rust and Python. Metadata-derived basic lands, the reusable Counterspell/Dispel stack-interaction slice, Winding Way's generic resolution-time library partition, Mental Note/Thought Scour's generic mill-then-draw path, Ponder's private reorder/optional-shuffle/draw program, Brainstorm's sequential private hand-to-library program, Preordain's private scry-then-draw program, Cryptic Serpent's data-driven graveyard-spell cost reduction, Deep Analysis's ordered mana/life flashback, and Lorien Revealed's reusable hand-zone typecycling/private search work across the other decks, without making any incomplete deck runnable. The generated pool metadata freezes all nine decks, and the schema-v2 registry at the legacy `cards_v1.json` path declares exact memberships and fail-closed engine capabilities for the 132 deck cards it contains; eighteen Spy cards are still absent, and most registered cards still lack executable behavior. Card-pool expansion therefore precedes claims about general Pauper learning.

Repository cutover is effective now, before the next card-mechanics slice. This separates day-to-day trainer, runner, evaluator, and rules work from the XMage application while retaining XMage only as an explicit oracle. It does not waive the later clean-clone and science-certification gates.

The implementation order after cutover is:

1. Preserve the content-locked, fixed-provenance Burn/Rally replay baseline. The two designated local corpora now pass tracked content-lock and provenance gates and each replay 40/40 with zero divergence.
2. Build reusable card, target, cost, zone, trigger, combat, and token primitives.
3. Terror.
4. Faeries.
5. Elves.
6. Affinity.
7. Wildfire.
8. CawGates.
9. Spy.
10. Run the final sampled and greedy 9x9 matrices, then complete clean-clone reproducibility and science-release certification.

## Science-ready completion definition

All of the following are required:

- **Pinned scope:** the exact nine mainboards below load as 60-card decks and match their canonical full-text source hashes under `utf8_text_crlf_v1`. A changed hash creates a new experimental protocol; it does not silently replace this one.
- **Rules completeness:** every mainboard spell is castable, every mana source works, every reachable card branch is modeled, and no BO1 run can reach `no_effect`, an unimplemented decision, or `UnsupportedMechanic`.
- **Reference evidence:** each rules primitive has unit coverage, each deck has a deterministic public-engine golden, and fixed-provenance XMage traces replay without unexplained divergence.
- **Deterministic environment:** deck construction, shuffling, policy sampling, seat swaps, observations, legal-action identities, checkpoints, and artifacts are versioned and reproducible from explicit seeds when source/protocol inputs, the environment binary, the recorded runtime compatibility tuple, and designated hardware are held fixed. This does not claim cross-platform bit identity for Torch-derived outputs.
- **Execution provenance:** before any science-ready training or evaluation run, publish a versioned hardware record for the designated execution hardware. The existing runtime compatibility tuple is necessary but is not a complete hardware fingerprint: it does not record CPU model/features or replace a release-level record of the accelerator model, driver/runtime, math backend, and deterministic settings.
- **Training integrity:** the already-proven constant-work, append-only training/checkpoint path remains crash-consistent and hash-validated for every deck configuration.
- **Training viability:** before the long full-pool run, benchmark end-to-end throughput and the stable sampler over the observed legal-action-width distribution from all nine decks on the designated hardware, then include the capacity estimate in its versioned hardware record. A faster selector changes the sampler version and requires new deterministic vectors; it cannot silently replace the accepted algorithm.
- **Evaluation integrity:** sampled-policy paired evaluation is primary; deterministic greedy paired evaluation is secondary. Both reject halted, truncated, inconsistent, or provenance-drifting runs.
- **Pool evidence:** every diagonal mirror and every ordered cross-deck cell passes the gates below, producing the complete 9x9 report with seat strata, draws, paired uncertainty, and exact provenance.
- **Reproducible finish:** a clean clone of the independent kernel repository passes CI and, on the designated reference hardware under the published runtime compatibility tuple and versioned hardware record, reproduces the published smoke, golden, and evaluation artifact digests without access to an XMage working tree at runtime.

Sideboard and BO3 support are deliberately excluded from this BO1 definition. They have their own promotion gate later in this document.

## Canonical BO1 decks

The hash covers the complete UTF-8 `.dek` source text, including its sideboard, even though the first science-ready protocol uses only the 60-card mainboard. `utf8_text_crlf_v1` strictly decodes UTF-8, normalizes CRLF, LF, and bare-CR line boundaries, then hashes the complete text encoded with CRLF boundaries without adding or removing a final newline. This keeps the pinned hash portable across Git checkout line endings without excluding any XML content.

| Order | Archetype | Canonical source | SHA-256 |
|---:|---|---|---|
| 1 | Wildfire | `Deck - Jund Wildfire.dek` | `cff35798ff724888a9e5a4520dd55e70b0c628a55908697aa116089d8fd980a5` |
| 2 | Rally | `Deck - Mono Red Rally.dek` | `4b5019bd08f9387aeabebdca0d90aaa10dfd75fc75ed3a87c95a2fabf4dba834` |
| 3 | Affinity | `Deck - Grixis Affinity.dek` | `4a41135ac6d14960e75ddce8e9980c0505c0b71a9c08a2e10578a10d2fcf8801` |
| 4 | Elves | `Deck - Elves.dek` | `6b040933c9b3506536e7dc71c94dcaf5f16c7ade43a3d0f7f9b240be6deb0d87` |
| 5 | Spy | `Deck - Spy Combo.dek` | `f08177d5ed133b18312f59649d1155e15b5074ababeaabcdf3f31ded650308ba` |
| 6 | Burn | `Deck - Mono-Red Burn.dek` | `4ebba6b42bb27a0ea55001cee133aada81f0dffd8661b46b012fc5026675aa32` |
| 7 | Terror | `Deck - Mono-Blue Terror.dek` | `8ba22b67b843bc49a421e1c2814c4dd24a04ab2b45131ec7876a8312115a9fda` |
| 8 | CawGates | `Deck - Caw-Gates.dek` | `72c2bbf76a7fd219349a0ad81c44dc6166b4a797a1f66fe9b5a5de79aa6cdc14` |
| 9 | Faeries | `Deck - Mono-Blue Faeries.dek` | `8cb962c4ccee6a5f8c0c70fc27c17d13323d13606c82b9b12b8985aa87e0f344` |

Canonical order is part of the protocol and is reused for manifests, seed derivation, matrix rows, and matrix columns.

## Current checkpoint truth

### What is already implemented

- A deterministic engine with a schema-v5 public RL protocol, session, observation, legal-action, and policy-surface boundary, plus a build-time-validated runtime catalog for the exact canonical Burn and Rally mainboards. The engine-facing schema-v4 observation/action identities remain frozen, H2 remains `surface_version: 2`, and its aggregate attacker/blocker semantics are unchanged.
- Policy v5 exposes each eligible attacker, then each eligible blocker for a fixed attacker, in canonical engine/H2 order as exactly two stable actions: exclude (`false`, index 0) and include (`true`, index 1). Intermediate answers live only in chooser-private scan state; the final substep commits one unchanged aggregate H2 combat action atomically, and failed/stale commits preserve the prior state and retry identities.
- Responses, observations, rollouts, and artifacts carry exact grouped provenance through `physical_decision_id`, `substep_index`, and `substep_count`. Policy steps remain monotonic microsteps, physical decisions advance only after a complete group, actor/stage/candidate history is frozen across the group, and terminals cannot interrupt a partial group.
- Sessions, runners, trainers, evaluators, and manifests track independent `max_physical_decisions` and `max_policy_steps` caps plus `physical_decision_count` and `policy_step_count` totals. A multi-substep group is preflighted against the remaining policy-step budget before its first substep, so truncation cannot create a partial combat decision.
- Append-only, hash-linked training artifacts and checkpoints with constant-work update and recovery behavior.
- Failure-boundary and path-safety proofs for training and evaluation publication.
- Deterministic paired head-versus-update-zero evaluation with seat swaps, natural-terminal enforcement, W/D/L and seat strata, paired bootstrap intervals, an exact paired sign test, and immutable artifact validation.
- Greedy V1 and Sampled V3 selector lanes. Current greedy run/game/pair artifacts are schema v3, and sampled run/game/pair artifacts are schema v5, while the sampled selector identity remains Sampled V3: exact binary32-to-Decimal softmax, Hamilton-apportioned `2**64`-unit mass, and one SplitMix64 draw per policy substep. The same-deck mirror head-versus-update-zero lane still shares one actor-local physical-seat stream across pair legs and excludes policy role and game/leg. Greedy remains the secondary lane.
- The sampled-v3 selector contract is stored as an independent release literal, so a future live sampler version cannot silently reinterpret historical sampled-v3 artifacts.
- Source-level card behavior for the complete Burn and Rally mainboards. Chain Lightning now has explicit payment, retarget, target, repeat-copy, counter/fizzle, and copy-departure behavior. Its engine/card semantics and legacy stable identities remain schema v4; the live policy observation, legal-action, and session contracts are schema v5.
- The synchronized Python hard boundary is feature schema `actor-relative-v5-python-4`, feature registry `rust-observation-v5-action-v5-registry-4`, encoding contract `actor-relative-node-graph-12`, model-config schema 5, and model contract `kernel-policy-value-net-8`. Runner artifacts are schema 4. Training is run schema `kernel_rl_train_run/v14`, episode-summary schema `kernel_rl_train_episode_summary/v4`, and summary schema `kernel_rl_train_summary/v3`; checkpoints are `kernel_rl_train_checkpoint/v4` with sidecar v2, update-record v4, and latest-pointer v2. Cross-boundary resume or publication fails closed rather than upgrading an older artifact in place.
- `RUNTIME_DECKS_V1_VALIDATION.md` remains an immutable record of the earlier node-graph-11/net-7 run. Its trainer/runner/evaluator validation must be rerun under node-graph-12/net-8 before new science; old evidence is not relabeled across the encoding boundary.
- Trainer algorithm `terminal_reinforce_value/v3` sums the selected-action log-probabilities across all learner substeps in one physical decision, retains one value prediction, and contributes exactly one REINFORCE/value loss term and one trajectory item per completed learner physical group. Combat scan width therefore cannot multiply the value loss merely by introducing policy microsteps.
- Action-seed schedules are hierarchical v2 contracts throughout: runner policies use `kernel-python-rl-seed-v2`, training uses `kernel-python-rl-trainer-sha256-v2`, and sampled evaluation uses `kernel-python-rl-evaluator-action-sha256-v2`. Each first derives a physical-decision group seed and then a `substep_index` seed; because the physical index advances once per completed group, additional substeps in an earlier scan do not shift any later physical-decision seed.
- Ordered physical-seat deck IDs and resolved deck hashes now travel through reset responses, rollouts, training runs, greedy evaluation, sampled evaluation, manifests, and every episode/game/pair record. Resume and publication reject requested or resolved deck-identity drift before mutation. Session reset and the runner admit all four ordered Burn/Rally pairings; training, greedy evaluation, and sampled evaluation admit only exact mirrors (`Burn`/`Burn` or `Rally`/`Rally`) and reject mixed pairings before artifact mutation pending the separately versioned multi-deck contract.
- The release runner completed one natural-terminal episode for each of the four ordered Burn/Rally pairings and reproduced every run exactly from identical inputs. A bounded `Rally`/`Rally` end-to-end smoke completed one two-episode training update and one two-game pair in each greedy and sampled evaluation lane. [RUNTIME_DECKS_V1_VALIDATION.md](RUNTIME_DECKS_V1_VALIDATION.md) records the exact hashes, commands, runtime tuple, throughput, and limitations. That evidence predates the current node-graph-12/net-8 contract; a fresh bounded runner + train + greedy + sampled smoke under the current boundary is required before science. Even refreshed, this establishes runtime wiring and bounded viability only; it is below the 32-pair mirror gate and is not science evidence.
- Privileged audit artifacts use audit schema 10, model-visible policy artifacts use schema 5, and rollout manifests use schema 8. The audit stream retains the versioned cross-platform diagnostic game-state hash `fnv1a64-serde-json-game-state-envelope-v5` plus the privileged policy-environment binding; the public session wire and model-visible policy stream expose neither `environment_hash` nor `environment_hash_algorithm`, diagnostic state hashes, nor hidden state. Audit and policy readers reject legacy, mixed, missing, unknown, misgrouped, and cross-stream-inconsistent contracts. Internal stack integrity now binds immutable spell/copy and Madness-offer source incarnations, validates staged cast/activation/discard actions before mutation, and routes every spell departure through one provenance-aware path without changing the public observation, action, policy, or session schemas.
- Tracked Phase-0 content locks for `burn_mirror_v6` and `rally_mirror_v2`: all 40 trace paths, raw-byte sizes and SHA-256 digests per corpus, each `manifest.json`, and an aggregate digest are embedded into the replay gate. Designated-corpus replay now fails before parsing a trace on non-`LOCKED` status or any missing, extra, or changed replay input.
- Generated `pauper_pool_v1.json` and `pauper_support_v1.json` metadata that pins all nine normalized 60+15 rosters, exact registry membership, current support blockers, token dependencies, source hashes, and raw pool/registry hashes.
- The support manifest currently classifies 45 unique cards as `full`, zero as `partial`, and 105 as `no_effect`. Rally and Burn are both 60/60 `full` at the mainboard card-behavior layer, and both designated content-locked, fixed-provenance reference corpora replay 40/40 with zero divergence. Phase 1 now includes metadata-derived intrinsic mana for every registered basic land, a shared fail-closed engine-capability/preflight contract, Winding Way's schema-neutral library primitive, Mental Note/Thought Scour's generic mill-then-draw program, Ponder's generic private library-order/optional-shuffle program, Brainstorm's sequential private hand-to-library program, Preordain's three-stage private scry program, a shared data-driven generic-cost reducer, ordered reusable cost components with fixed life payment, Lorien Revealed's reusable typecycling/search path, Deem Inferior's owner-chosen library placement, the symmetric four-card Blast family, and Murmuring Mystic's generic cast-triggered Bird Illusion creation.
- Counterspell and Dispel now compose one generated counter-target effect with separate reusable any-spell and instant-spell stack filters. The target engine rejects a cast unless some complete mandatory target assignment exists, recursively filters dependent target prefixes, excludes the announcing spell itself, and silently retains the original printed mode index when only one modal branch is viable. These behaviors are pinned under schema-v4 observation/action/session identities while preventing targetless dead-end decisions.
- Blue Elemental Blast and Hydroblast now reuse the same modal counter/destroy programs as Pyroblast and Red Elemental Blast through one checked-color/filter-timing code-generation recipe. Blue Elemental Blast filters red spells and permanents while choosing targets; Hydroblast legally targets any spell or permanent and checks for red only at resolution. Focused tests cover real spells, virtual copies, flashback replacement, stale-target fizzles, destroy/no-op behavior, printed mode order, literal stable schema-v4 action identities, and snapshot/restore. The rules reference is pinned to XMage commit `0723fc0c2be922af47b0ef0539f28114cc23b998` and exact source blobs (`BlueElementalBlast.java` `55d1601c2021bd5238b6e739a0d952560dbbfd4f`, `Hydroblast.java` `c256286f9bed6f201dd0af4bf6dc5b6aee3dd7af`, `Pyroblast.java` `538356cc4a861dcace3f1959521cfe0fd29fa07f`, `RedElementalBlast.java` `0f0c804fe49c919e18e8950cc4bd3437fc1177b0`, `CounterTargetEffect.java` `7f49db9876aa44ce8396a6693b2fc6e1f3e2977f`, `DestroyTargetEffect.java` `5271b8fcb5f51cf28444fa8b93a2a664fb1ce287`, and `ColorPredicate.java` `6ec89935618501a6ec24d877309d0f0d36b05eb4`). The kernel currently tests the card definition's static color rather than XMage's dynamic game color, and its shared destroy operation is a zone move without indestructible/regeneration handling; the promoted pool has no supported color-changing effect or qualifying red indestructible/regeneration interaction, but those rules must be generalized before such a consumer is promoted.
- Winding Way now uses the generic resumable effect continuation for its resolution-time Creature/Land choice, publicly reveals an exact top-four snapshot, and partitions it by typed card metadata while preserving revealed identities that move to hand. Each matching/rest group is replacement-evaluated as one batch; whenever two or more cards in a group enter the graveyard, their owner explicitly orders the public objects through the reserved schema-v4 generic card-selection actions, with the forced final card auto-completed. Printed option order, hidden-library privacy, short-library behavior, all six three-card graveyard orders, literal RL action identities, stale-incarnation failure, and snapshot/restore determinism are pinned without a public schema change; Lead the Stampede will reuse the surrounding library-snapshot and identity-transfer substrate while retaining its distinct private-look and subset-choice semantics.
- Mental Note and Thought Scour compose one generated mill-then-draw sequence with independent mill/draw counts and controller/target player references. The exact P0/P1 target order, owner-only pending library identities, public graveyard result, owner-selected graveyard order, short-library behavior, same-resolution draw ordering, countered-spell behavior, action identities, stale-incarnation failure, and snapshot/restore determinism are pinned without a public schema change. This increment models ordinary library-to-graveyard zone-change batches; XMage's separate pre-batch `MILL_CARDS` replacement hook and post-move mill-summary events remain deferred and must be added before a supported card consumes those event classes.
- Ponder composes generic private top-N look/reorder, optional deterministic shuffle, and draw operations in one uninterrupted resolution. All six three-card permutations, short and empty libraries, canonical Boolean action identities, deterministic shuffle state, owner/nonowner knowledge boundaries, stale prefix/incarnation/chooser failure, countering, snapshot/restore on both Boolean outcomes, and the immediate empty-library game-loss checkpoint are pinned without a public schema change. XMage displays the optional choice as Yes/No, while the kernel deliberately retains schema-v4's canonical semantic `false`/`true` order; the outcomes match even though candidate indices are reversed. Reorder and shuffle currently mutate deterministic state directly rather than emitting standalone event-history records; replacement/trigger event hooks remain required before any supported pool card consumes those actions.
- Brainstorm composes three draw attempts with two fresh exact-one private hand choices. The first chosen card commits to the library top before the second prompt, and the second becomes topmost, matching XMage's observable hand/library counts and selected-count semantics while retaining the kernel's action/advance separation. Exact hand snapshots, candidate partitions, object incarnations, chooser ownership, private redaction, conservative nonowner hand-knowledge invalidation, shifted prior library facts, owner-only inserted-prefix knowledge, all six ordered pairs, forced short-hand moves, short/empty-library loss timing, countering, event order, action identities, and snapshot/restore boundaries are pinned without a public schema change. The current draw path emits three individual replaceable draw events but not XMage's preceding aggregate multi-draw replacement event; that hook remains required before a supported pool card consumes it.
- Preordain composes a private scry-two operation with an ordinary draw in one uninterrupted resolution. The current contract is deliberately Preordain/Scry2 final-state semantics only: the first prompt chooses an unordered subset for the bottom, a 2-card bottom group is then ordered shallow-to-deep, and a 2-card retained group is ordered deepest-to-topmost before one atomic library-and-knowledge transition; higher requested counts fail before any prefix binding or reveal. All six semantic outcomes, short/empty libraries, physical duplicate identities, chooser privacy, shifted untouched-tail knowledge, exact prefix/incarnation/partition validation, redundant continuation progress checks, stable action identities, countering, snapshot/restore, and post-resolution empty-draw loss are pinned without a public schema change. XMage AIRL displays STOP before the subset's object candidates; the kernel deliberately retains schema-v4's canonical object-actions-first and semantic Finish-last order, so outcomes and stable object/Finish meanings match without claiming candidate-index parity. Arbitrary higher-count scry requires XMage-order bottom commitment plus typed `SCRY`, `SCRY_TO_BOTTOM`, and `SCRIED` hooks; those hooks are likewise required once a supported replacement or trigger observes them.
- Cryptic Serpent uses the shared cast-offer and final-payment path to reduce only the printed generic component of `{5}{U}{U}` by one for each physical instant or sorcery card in its controller's graveyard, flooring at `{U}{U}`. Zero, partial, exact, and excess reductions; mixed and opposing graveyards; stable cast actions; preflight; snapshot/restore; payment; and real 6/5 resolution are pinned against XMage rules baseline `0723fc0c` and source blob `5febb648d15fbff5604a5bce01f1aaaa4fb75944`. The shared reducer now also consumes Deem Inferior's generic controller-draw counter, while combined generic additional-cost reduction and Tolarian Terror's separate ward mechanic remain deliberately unsupported.
- Deem Inferior uses the same generated cost-reducer path to reduce only `{3}` from `{3}{U}` for each successful card draw by its controller in the current turn, flooring at `{U}` and resetting at untap. It targets any nonland permanent regardless of controller; on resolution that permanent's owner, not its current controller, chooses an exact public placement second from the top or on the bottom of their own library. The generic implementation pins zero-through-saturated reduction, failed and opponent draws, stolen permanents, exact short-library outcomes, hidden-information knowledge, historical target contracts through costs/copies and stale-target fizzles, token cease behavior, LKI-trigger ordering, typed continuation tamper rejection, RL target projection, stable schema-v4 actions, and snapshot/restore. The rules reference is XMage commit `0723fc0c2be922af47b0ef0539f28114cc23b998` with exact source blobs `DeemInferior.java` `dc4a84da7632ca4f70c87770cf863b02c1b943a2`, `CardsDrawnThisTurnDynamicValue.java` `5a28840e1cb338d4d0a4119e5e2425da4f8d047c`, `PutOnTopOrBottomLibraryTargetEffect.java` `76314453b363014af0d6870a0de6b1366c2dc8b4`, `SpellCostReductionSourceEffect.java` `2df91a1fbf784b1931c1c0c5d7d4c4a188c05326`, `TargetNonlandPermanent.java` `b9eac0f230c1f714e37b8176b4bffe39dfbd5c45`, and the `PlayerImpl.java` insertion implementation `1fc883515410e4b6df6255e5be44547f82617784`.
- Deep Analysis composes the generic `AnyPlayer` target with `DrawCards(Target(0), 2)` and an ordered flashback cost `[Mana({1}{U}), PayLife(3)]`; Faithless Looting and Lava Dart now use the same component-slice path without changing their exact mana/sacrifice behavior. Life 2 rejects the flashback, life 3 completes payment then loses to SBA before priority/resolution, and life 4 resolves from 1 life; mana is paid before the replaceable life-loss event, and physical counter/fizzle/resolution departures all exile while virtual copies cease without a card-zone move. Self/opponent draw, two empty-library attempts, stable schema-v4 cast/target IDs, snapshot/restore, and the unchanged XMage candidate label `Flashback {1}{U}` are pinned against rules baseline `0723fc0c`, `DeepAnalysis.java` blob `3b039ba444b5fb88b3a43b1fb32dc2164035a0e6`, `PayLifeCost.java` blob `65ef04d0d86c2c8f037d9fc942a1bf687a431e34`, and the real AIRL trace `game_20260714_011043_0004.txt`. Repeated mana/discard/sacrifice/life components, source-changing components before mana, and fixed life combined with Phyrexian mana deliberately fail closed within one component slice until an atomic solver can prove the full resource shape; cross-slice cost composition remains limited to the currently generated combinations, and pay-life-specific replacement/restriction effects remain outside the current pool.
- Lorien Revealed now casts as `{3}{U}{U}` draw three and exposes reusable hand-zone typecycling: `{1}`, discard this exact card, search privately for zero or one physical land with the Island subtype, publicly reveal a selected card after moving it to hand, then deterministically shuffle. Typecycling has instant timing, uses the stack, leaves paid mana/the discarded source spent if the ability is countered, accepts basic Island and nonbasic Island-subtype lands, preserves every same-name physical candidate, and validates the source incarnation plus the exact full library order/membership/filter/candidate partition at every snapshot boundary. The filter reads current `ObjectStateV4` effective subtypes; because the kernel has no effective card-type field or type-changing operation, definition-derived land type is accepted only on face zero and any other face fails closed. The chooser sees temporary semantic candidates without gaining persistent library-order knowledge; the opponent sees one redacted Finish-capable search envelope regardless of match count, including a Finish-only prompt for zero matches or an empty library, until a selected hand card is revealed. Rules-correct fail-to-find is always available as schema-v4's object-actions-first, Finish-last action. This deliberately differs from the frozen AIRL adapter, which suppresses STOP when any match exists, deduplicates choices by name, and displays main-priority Pass first; no candidate-index parity is claimed. The current kernel emits ordinary mana/tap and zone-change events only: typed `ACTIVATE_ABILITY`, `CYCLE_CARD`, discard/cycled summaries, `SEARCH_LIBRARY`/`LIBRARY_SEARCHED`, and `SHUFFLE_LIBRARY`/`LIBRARY_SHUFFLED` replacement/trigger hooks remain required before a supported consumer observes them. It also has no supported ability-countering spell yet, so the cost-persistence unit removes the activated stack item through the low-level counter boundary. Lorien's spell emits three individual Draw events but still lacks XMage's preceding aggregate multi-draw replacement hook, the same frozen limitation documented for Brainstorm.
- Murmuring Mystic now reuses the generic controller-cast instant/sorcery trigger and `CreateToken` effect to create the appended blue 1/1 Bird Illusion token with flying. Its promoted registry row corrects legacy ingestion-only uppercase subtype spellings to the canonical Human and Wizard variants, so existing Human effects apply without merging the distinct legacy variants used by other unsupported rows. Focused tests pin its real cast and 1/5 Human Wizard resolution, Rally interaction, normal instant/sorcery and flashback casts; creature, opponent, and virtual-copy negatives; a countered spell's already-triggered ability; two-Mystic ordering and snapshot restore; source removal; flying blocks and summoning sickness; and token lethal-damage SBA/cease behavior against XMage baseline `0723fc0c` (`MurmuringMystic.java` blob `219e0325fd5e611f8a7d7a6551b5696f0418d014`, `BirdIllusionToken.java` blob `4ee17731ce3e15d2a56bdb3ade47584b857c8833`, `SpellCastControllerTriggeredAbility.java` blob `ad5dc6439018f4ca77bf6f2ead390774e552fed2`). Existing definition/subtype ids are append-only, Burn/Rally runtime ids and hashes are unchanged, and Terror remains outside the admitted runtime catalog.
- Card-database provenance remains `kernel_carddb/v5` and is now frozen at `0xa06fa9566106f0ea`; it binds every generated gameplay selector, including keywords, targeting contracts, casting variants, activated abilities, secondary modes, final spell/mana programs, flashback/effect recipes, the generic cost reducers, Deem Inferior's owner-library-placement recipe, Lorien's structured Draw3/Islandcycling recipes, each Blast's checked color plus targeting-versus-resolution filter timing, Murmuring Mystic's full-capability promotion and canonical subtype correction, and the appended Bird Illusion token definition. Trigger-to-token and target-history runtime validation remain source-revision-gated and behavior-tested rather than duplicated into the generated-selector hash grammar.
- Interactive activation discards now carry an incarnation-bound cross-slot resume identity, revalidate the complete activation and exact discard obligation before any mutation, and finalize synchronously after payment instead of trusting serialized paid-state markers. Activated stack items freeze discarded/sacrificed/exiled payment objects in schema-v4 `paid_cost_refs`, while a source that changed zones is not stamped with the old incarnation's ability use.
- A deterministic checked-in XMage unit test provides a bounded source-level oracle for Winding Way's Creature branch: `ImpulseDrawAndMillZoneTest#testWindingWayRevealSplitsCorrectlyBetweenHandAndGraveyard` at test commit `3a86580e` (parent rules baseline `0723fc0c`) puts Elvish Mystic and Grizzly Bears in hand and Counterspell and Lightning Bolt in the graveyard. Because Grizzly Bears is outside the frozen Pauper registry, the Rust end-to-end golden uses Quirion Ranger in that second Creature slot and pins the equivalent option order and two-Creature/two-instant partition. This evidence is deliberately a focused Java regression comparison, not a whole-game trace or fixed-provenance v2 replay.
- `xmage_counter_reference_windows_v1.json` pins the raw manifest/trace hashes and six exact Counterspell/Dispel target selections from Caw-Gates, Terror, and Faeries XMage games, plus bounded visible graveyard deltas. The source run did not record its Java commit, so this evidence is explicitly a source-hash-backed micro-reference gate, not fixed-provenance whole-deck parity; the ignored materialization test rehashes and reparses the local source logs when available.

### Card coverage

"Effect-backed" means a card copy has explicit `full` engine capability plus its required spell, permanent, or mana program. Registry rules metadata alone does not qualify. A land mapped to `no_effect` cannot be played, and a spell mapped to `no_effect` cannot be cast.

| Archetype | Main registered | Main effect-backed | Side registered | Side effect-backed | Current limiting fact |
|---|---:|---:|---:|---:|---|
| Wildfire | 60/60 | 7/60 | 15/15 | 3/15 | Its Mountain, Forest, and Swamp copies are usable; spells/nonbasics remain fail-closed |
| Rally | 60/60 | 60/60 | 15/15 | 8/15 | Locked fixed-provenance replay passes 40/40 |
| Affinity | 60/60 | 8/60 | 15/15 | 11/15 | Great Furnace, Swamp, Galvanic Blast, and both blue Blasts work |
| Elves | 60/60 | 17/60 | 15/15 | 0/15 | Snow-Covered Forest and Winding Way work; remaining spells stay fail-closed |
| Spy | 21/60 | 8/60 | 4/15 | 1/15 | Forest, Swamp, and Winding Way work; 39 main and 11 side copies are absent from the registry |
| Burn | 60/60 | 60/60 | 15/15 | 11/15 | Mainboard is the current complete baseline |
| Terror | 60/60 | 54/60 | 15/15 | 5/15 | Mental Note, Thought Scour, Ponder, Brainstorm, Cryptic Serpent, Deem Inferior, Deep Analysis, Lorien Revealed, Murmuring Mystic, and both blue Blasts work; remaining threats and spells stay fail-closed |
| CawGates | 60/60 | 16/60 | 15/15 | 10/15 | Island, Counterspell, Brainstorm, Preordain, Lorien Revealed, and both blue Blasts work; Gates and remaining spells stay fail-closed |
| Faeries | 60/60 | 24/60 | 15/15 | 7/15 | Island, Counterspell, Dispel, and both blue Blasts work; creatures and remaining spells stay fail-closed |

The registry currently contains 136 definitions: 132 deck cards and four tokens. `pool_decks` now lists all nine sources in canonical order, and the eight already-present Spy cards declare exact Spy membership. Seven of those are Spy mainboard names shared with other decks; fourteen Spy mainboard names and four additional sideboard-only names still need new records.

Chain Lightning's implementation checkpoint is backed by unit tests for unpayable and declined payment, copied-stack identity, retargeting, recursive copies, illegal-target fizzles, copy-aware counters and flashback replacement, target-pool filtering, RL serialization/action semantics, and snapshot/restore determinism. The locked `rally_mirror_v2` corpus contains 40 games, 15 logged payment prompts, six accepted payments, six retarget prompts, and three accepted retargets. Its manifest pins ReferenceRules v2 and Java oracle commit `0723fc0c2be922af47b0ef0539f28114cc23b998`; the runtime provenance gate passes. Replay reaches GameOver in all 40 traces, matches all 40 winners, and reports zero divergence and zero halt. Closing the former residuals required mirroring Java's rendered-cast-candidate equivalence in the replay comparator while preserving exact chosen object identity, filtering attackers killed before blocker declaration, and matching XMage's deterministic generic-mana pool spending order.

The formal Burn and Rally evidence is content-locked by tracked metadata; `CORPUS_CONTENT_LOCKS.md` defines the byte-level algorithm. `corpus_archives_v1.json` additionally pins the exact release-archive bytes and binds each archive to its content-lock aggregate. `python/tools/fetch_corpora.py` provides fail-closed retrieval, safe extraction, and post-download verification for a clean clone.

Keyword representation is also not rules completeness. For example, defender is represented but not enforced when declaring attackers, trample still uses a no-trample combat path, and deathtouch, lifelink, protection, hexproof, and ward need real rules behavior.

### Reusable implementation clusters

| Cluster | Required capability | Primary consumers |
|---|---|---|
| Permanent and mana substrate | Generic permanent resolution, colored/colorless/choice mana, tapped entry, chosen colors, indestructible | Every remaining deck |
| Targets and costs | Zone-aware filters, min/max target counts, dependent targets, filtered sacrifices/taps, life and graveyard costs, once-per-turn use | Every remaining deck |
| Library and zones | Search, reveal, choose subsets, order top/bottom, shuffle, scry, mill, cycling, linked exile and return | Terror, Faeries, Elves, Wildfire, CawGates, Spy |
| Stack interaction | General counters, filtered counters, counter-unless-pay, ward, modal spells, spell copy | Rally, Terror, Faeries, CawGates |
| Triggers and continuous effects | Generic ETB/LTB/dies/cast/sacrifice/combat triggers, dynamic counts, temporary effects, type changes | All non-Burn decks |
| Combat | Defender, trample, deathtouch, lifelink, blocker-count restrictions, protection and hexproof | Elves, Wildfire, CawGates, Spy |
| Tokens, counters, attachments | Map, Spawn, Clue, Food, Bird, Treasure, Hero, token copies; stun, toughness and lore counters; Aura/equip/bestow | Affinity, Wildfire, Elves, Terror, CawGates, Spy |
| Persistent object/game state | Chosen color, linked cards, transformed face, Saga chapters, initiative/dungeon progress | CawGates, Elves, Spy |

These should be generic effect and decision primitives, not a growing per-card `Special` enum. Card-specific definitions should compose tested primitives and fail closed when a reachable branch has no representation.

## Dependency-ordered delivery

### Phase 0 - Seal the current baseline

- Keep the generated pool and support manifests current, with explicit `full`, `partial`, and `no_effect` states over all 150 deck cards.
- Preserve the assertion that the authoritative deck set equals these nine canonical full-text hashes; `unresolved: []` must not be possible when a source or registry record was never included.
- Preserve Chain Lightning's implemented copy, retarget, repeat-copy, counter/fizzle, schema-v4 engine/card RL semantics, schema-v5 policy projection/grouping, snapshot, and trace-parser regressions.
- Preserve the content-lock- and provenance-gated, zero-divergence 40/40 replays for `burn_mirror_v6` and `rally_mirror_v2`.
- Preserve all Burn/Rally goldens, replays, RL contracts, and evaluator proofs.

Status: open. The bounded Rally mirror smoke is below the 32-pair mirror gate and does not satisfy this phase's exit condition.

Exit: Burn and Rally pass every BO1 gate below with no conditional halt.

### Phase 1 - General card substrate

- Generate ordinary permanent resolution and mana behavior from card metadata.
- Keep runtime support and generated support reports on the same per-definition capability declaration, and preflight both token-free 60-card decks before any science-facing environment shuffles or constructs state.
- Replace fixed target shapes with zone/filter/cardinality descriptions.
- Generalize costs, library operations, triggers, continuous effects, tokens, counters, and attachments.
- Make every unsupported reachable branch explicit in the support manifest and runtime diagnostics.

The deliberate schema-v4 engine/H2 boundary is complete and remains frozen. It adds a generic resumable effect continuation/choice machine, perspective-safe known-library and known-hand state, typed future choices and cost kinds, persistent object/game state, semantic relations, and aggregate combat decisions. Public policy schema v5 is a strict overlay: it projects those semantics through synchronized Rust/Python features and grouped binary combat scans without reinterpreting the schema-v4 card identities cited below. Schema-v3 and pre-v5 policy artifacts remain frozen historical inputs, and cross-schema resume is rejected. New card work should compose these generic shapes and may not add card-specific `pending_*` state or reinterpret a reserved action identity.

The first post-migration validation slice, Winding Way, is complete: its resolution-time Creature/Land choice composes a reusable public `reveal top N and partition by card type` primitive, exercises the continuation and hand-knowledge boundary, and requires no further public schema change. Mental Note and Thought Scour add generic self/target mill followed by draw, including private pending identities and owner-selected graveyard ordering. Ponder adds generic private top-N ordering, optional deterministic shuffle, and draw with fail-closed continuation validation. Brainstorm adds sequential private hand choices and knowledge-aware top insertion through ordinary zone-change events. Preordain adds Scry2-only private subset selection, independently directed bottom/top ordering, and one atomic knowledge-aware final-state transition. Cryptic Serpent adds a card-name-neutral dynamic counter that reduces only a spell's generic mana component through the same affordability and payment pipeline. Deep Analysis adds target-player draw plus ordered mana/life flashback payment through shared component legality, payment, and sacrifice-scanning paths. Lorien Revealed adds activation-zone definition data, exact-source discard costs, and optional zero-or-one typed whole-library search with public result reveal and deterministic shuffle. The four Blasts now share one generated checked-color/filter-timing implementation while preserving their printed targeting-versus-resolution distinction and schema-v4 modal identities. Generous Ent can reuse the hand activation and Forest-filtered search substrate once its Reach and Food ETB are implemented; Troll of Khazad-dum can reuse Swampcycling after its registry record/card body exists; ordinary Cycling can reuse the hand-zone/discard-source activation substrate with a draw effect. Lead the Stampede follows as a private-look/subset consumer of the broader library substrate.

Exit: foundational unit suites pass and an unsupported card cannot be mistaken for a playable one.

### Phase 2 - Terror

Terror is the lowest-novelty route to the reusable blue core: Island mana, draw and ordered library choices, self/target mill, graveyard-count cost reduction, Counterspell/Dispel, cycling, flashback with life, escape, freeze, ward, and spell-cast token triggers.

Exit: Terror mirror plus Burn, Rally, and Terror cross-deck gates pass.

### Phase 3 - Faeries

Reuse Island, Counterspell, and Dispel. Add flash timing, ninjutsu as a combat-window special action, combat-damage triggers, Faerie-count counters, counter-unless-pay, bounce/untap, loot, backup, hexproof, and temporary power reduction.

Exit: Faeries mirror and all cross-deck cells against accepted decks pass.

### Phase 4 - Elves

Add mana creatures, dynamic Elf counts, filtered tap/return costs, creature/land selection modes, Forestcycling, Food, changeling, bestow, and initiative/dungeon state. This also completes five exact Spy mainboard cards and Nyxborn Hydra for Wildfire.

Exit: Elves mirror and all accepted cross-deck cells pass.

### Phase 5 - Affinity

Implement artifact lands and bridges, affinity cost reduction, sacrifice-and-draw costs, ETB/LTB triggers, graveyard hate and recursion, investigate, damage sweeps, equipment, Job select/Hero attachment, subtype grants, and stun counters.

Exit: Affinity mirror and all accepted cross-deck cells pass.

### Phase 6 - Wildfire

Reuse eleven Affinity cards plus the Elves bestow work. Add typed-basic fetching, land destruction with indestructible handling, Map and Spawn tokens, colorless-cast and Eldrazi-sacrifice triggers, scry, graveyard-to-library triggers, and creature/land recursion.

Exit: Wildfire mirror and all accepted cross-deck cells pass.

### Phase 7 - CawGates

Reuse the blue core, then add Gate chosen-color and count state, linked exile, lifelink and dies triggers, Saga chapters and transformation, embalm, multi-mode/multi-target spells, color-based prevention, same-name search, and protection from monocolored.

Exit: CawGates mirror and all accepted cross-deck cells pass.

### Phase 8 - Spy

- Add Spy and its eighteen missing unique card records to the generated registry.
- First implement defender mana, filtered tap/sacrifice costs, Land Grant, basic search, cycling, and Omen casting.
- Then prove the core no-land line: Balustrade Spy mills the library, Dread Return sacrifices three creatures, and Lotleth Giant resolves the graveyard-count kill.
- Finish linked hand exile, defender combat restrictions, Wall of Roots counters, three-blocker restriction, and remaining interaction.

Exit: both the land-hit and landless-combo branches have goldens and replay evidence; Spy mirror and all cross-deck cells pass.

## Promotion gates

Every phase must pass gates in this order. A later gate cannot waive an earlier one.

### 1. Registry gate

- Parse the pinned `.dek` file to exactly 60 mainboard and 15 sideboard cards.
- Resolve every in-scope mainboard card and required token.
- Match the deck hash and card-database hash recorded in the run manifest.
- Report zero `no_effect` or partial mainboard definitions.

### 2. Unit gate

- Test every new effect, cost, target filter, trigger, continuous effect, combat rule, and state field in positive and negative cases.
- Cover payment selection, target loss/fizzle, indestructible, zone-change identity, trigger order, illegal action rejection, and serialization/hash sensitivity.
- Treat checked-in XMage Java card implementations as the card-semantics oracle.

### 3. Golden gate

- Add a deterministic `<deck>_goldfish.rs` driven only through the public engine after setup.
- Assert the exact decision sequence, targets/costs, events, mana, zones, life, battlefield, and terminal result.
- Cover every new nontrivial mechanic in at least one golden or reference replay.

### 4. Replay gate

- Capture fixed-provenance XMage traces for the deck and new mechanic branches.
- Match candidate sets, chosen actions, targets, modes, costs, and checkpointed public state.
- Allow zero unexplained divergence, unexpected suppression, halt, or uncastable reference action.

### 5. Mirror gate

- Run at least 32 fixed paired seeds with the deck in both seats.
- Admit only natural win/draw terminals; any halt, truncation, illegal action, or unsupported branch fails the gate.
- Rerunning from identical inputs under the same recorded runtime compatibility tuple and on the same designated hardware must reproduce decision, state, terminal, and artifact digests exactly.

### 6. Cross-deck gate

- Before phase promotion, run at least 16 paired seeds against every previously accepted deck under the separately versioned multi-deck seed contract described below. The current policy-schema-v5 same-deck mirror schedule with the Sampled V3 selector is not that contract.
- Require natural terminals, exact rerun determinism under the same recorded runtime compatibility tuple and on the same designated hardware, zero unsupported cards/mechanics, and explicit seat-stratified results.
- After Spy, replace these smoke-sized gates with the final matrix protocol below.

## Evaluation protocol

### Primary: sampled policy

Science claims use actions sampled from the policy logits, with all sampling performed by an explicit, versioned deterministic RNG. The manifest records policy snapshot hashes, deck hashes, environment and policy seed derivation versions, pair count, temperature, and any logit transformation.

#### Current policy-schema-v5 seed schedule and Sampled V3 selector contract

Sampled V2 remains a frozen legacy identity: it selected with `torch.multinomial`, whose seeded categorical stream is not stable across supported Torch releases. The Sampled V3 selector does not reinterpret that schema. It converts finite CPU binary32 logits exactly to Decimal, subtracts the maximum at 256-digit precision, evaluates exponentials at 80-digit round-half-even precision (with deltas strictly below -128 assigned zero mass), apportions exactly `2**64` units by largest remainder with legal-index tie-breaking, and selects by inverse CDF using the first SplitMix64-v1 output seeded by the action seed. Exact vectors cover the softmax weights, RNG outputs, CDF boundaries, translation invariance, and independence from process-global Python/Torch RNG state and Decimal context. This portability guarantee starts from identical supplied binary32 logits and action seeds; it does not claim that Torch produces bit-identical logits across platforms, builds, or hardware.

Policy-schema-v5 sampled artifacts retain that Sampled V3 selector but use hierarchical evaluator action-seed derivation `kernel-python-rl-evaluator-action-sha256-v2`. Each actor-local physical decision first derives a group seed from base seed, pair index, physical seat, and local physical-decision index; each policy substep then derives its own seed from that group seed and `substep_index`. The physical-P0 and physical-P1 group streams are each shared across both pair legs, and derivation still excludes candidate/baseline role and game/leg. Because the local physical-decision index advances once per completed group rather than once per policy substep, widening an earlier combat scan does not perturb any later physical-decision group seed. Trainer action seeds use the analogous `kernel-python-rl-trainer-sha256-v2` physical-group/substep hierarchy. These remain same-deck mirror contracts for one fixed identical deck in both seats; they do not define a multi-deck shuffle or logical-deck action schedule.

#### Future multi-deck seed contract

Cross-deck evaluation requires a new, separately versioned schema and seed-derivation contract. That protocol may key shuffle/determinization and action streams by logical deck role so that each deck carries the corresponding common-random-number streams when the decks swap physical seats. The exact mapping must be explicit in manifests and tested for seat-swap, row-order, and rerun invariance. It must not be retrofitted into the current same-deck mirror v5 artifact/seed contract or described as behavior the Sampled V3 selector already provides.

The primary report includes candidate-centric W/D/L, score, candidate-as-P0 and candidate-as-P1 strata, favorable/tied/unfavorable pairs, a deterministic paired-bootstrap 95% interval for draw-inclusive score, and the exact two-sided paired sign test. Halted, truncated, provenance-drifting, or inconsistent terminal rows invalidate the complete cell rather than becoming losses or draws.

### Secondary: greedy policy

Run the existing deterministic argmax policy as a separate artifact over the same snapshots and ordered matchup plan. Greedy results diagnose policy sharpness, tie behavior, and sampling sensitivity. They cannot substitute for the sampled result or independently promote a checkpoint.

### Comparators

- **Integrity anchor:** every candidate is evaluated against the frozen update-zero policy for continuity with the current evaluator.
- **Promotion comparator:** every candidate is also evaluated against the last accepted checkpoint under identical sampled and greedy protocols.
- **Pool matrix:** the accepted checkpoint supplies the policy used for every deck in the final 9x9 environment matrix.

The promotion decision must name which comparator supports it. An update-zero gain alone is not evidence of improvement over the accepted checkpoint.

## Final ordered 9x9 matrix

Rows are the evaluated deck and columns are the opponent. Every cell is seat-balanced and independently materialized in this canonical order, including diagonal mirrors and reciprocal off-diagonal cells. Reciprocal cells must agree after candidate/opponent perspective inversion when backed by the same paired games.

Abbreviations: `Wld` Wildfire, `Rly` Rally, `Aff` Affinity, `Elf` Elves, `Spy` Spy, `Brn` Burn, `Ter` Terror, `Caw` CawGates, `Fae` Faeries.

| Row \ Opponent | Wld | Rly | Aff | Elf | Spy | Brn | Ter | Caw | Fae |
|---|---|---|---|---|---|---|---|---|---|
| **Wld** | Wld/Wld | Wld/Rly | Wld/Aff | Wld/Elf | Wld/Spy | Wld/Brn | Wld/Ter | Wld/Caw | Wld/Fae |
| **Rly** | Rly/Wld | Rly/Rly | Rly/Aff | Rly/Elf | Rly/Spy | Rly/Brn | Rly/Ter | Rly/Caw | Rly/Fae |
| **Aff** | Aff/Wld | Aff/Rly | Aff/Aff | Aff/Elf | Aff/Spy | Aff/Brn | Aff/Ter | Aff/Caw | Aff/Fae |
| **Elf** | Elf/Wld | Elf/Rly | Elf/Aff | Elf/Elf | Elf/Spy | Elf/Brn | Elf/Ter | Elf/Caw | Elf/Fae |
| **Spy** | Spy/Wld | Spy/Rly | Spy/Aff | Spy/Elf | Spy/Spy | Spy/Brn | Spy/Ter | Spy/Caw | Spy/Fae |
| **Brn** | Brn/Wld | Brn/Rly | Brn/Aff | Brn/Elf | Brn/Spy | Brn/Brn | Brn/Ter | Brn/Caw | Brn/Fae |
| **Ter** | Ter/Wld | Ter/Rly | Ter/Aff | Ter/Elf | Ter/Spy | Ter/Brn | Ter/Ter | Ter/Caw | Ter/Fae |
| **Caw** | Caw/Wld | Caw/Rly | Caw/Aff | Caw/Elf | Caw/Spy | Caw/Brn | Caw/Ter | Caw/Caw | Caw/Fae |
| **Fae** | Fae/Wld | Fae/Rly | Fae/Aff | Fae/Elf | Fae/Spy | Fae/Brn | Fae/Ter | Fae/Caw | Fae/Fae |

Initial final-run minimums are 128 paired seeds per cell for sampled-primary evaluation and 32 paired seeds per cell for greedy-secondary evaluation. Counts are protocol fields and may be raised before launch, but may not be reduced after the run begins. The matrix is accepted only if all 81 cells validate and the aggregate contains no halt or truncation.

## Deferred sideboard and BO3 gate

BO1 completion does not imply match-play support. BO3 work starts only after the sampled-primary BO1 matrix is accepted.

BO3 promotion requires:

- all 15 sideboard cards for every pinned deck to be fully supported;
- a versioned, deterministic sideboarding policy and legal 60/15 exchange validation;
- game-to-game match state, play/draw choice, and best-of-three termination;
- post-board goldens and reference replays for graveyard hate, color hosers, artifact/enchantment hate, sweepers, and alternate win lines;
- sampled-primary and greedy-secondary post-board mirror/cross-deck matrices with the same failure rules as BO1.

Until this gate passes, reports must say "canonical-mainboard BO1," not "Pauper match" or "BO3."

## Standalone cutover, CI, and clean-clone finish

The repository cutover is complete now: the path-filtered history and exact source/standalone checkpoint mapping are recorded in `EXTRACTION_PROVENANCE.md`, and new work continues in this repository. Cutover is an ownership and dependency boundary, not the final science-ready certificate.

The remaining repository-hardening work may proceed alongside card coverage:

1. The generated card database, token definitions, canonical deck manifests, and source hashes needed at build/runtime are vendored. The formal replay corpora have tracked release-archive byte locks and a retrieval command that verifies both archive and corpus content locks before use. XMage Java sources remain provenance and optional oracle inputs, not runtime dependencies.
2. Rust, Python, schema, seed derivation, and artifact formats are pinned, and `uv.lock` pins cross-platform Python dependency resolution. The lock does not promise cross-platform bit-identical Torch outputs or artifact bytes. Any deliberate protocol change must produce a new version rather than silently reinterpreting existing artifacts.
3. Independent Linux and Windows CI jobs run formatting/lint checks, `cargo test --locked`, the Python test suite, deck/hash validation, all unit/golden tests, and bounded deterministic environment smoke coverage. They do not compare Torch-derived, checkpoint, or evaluation artifact bytes across operating systems.

Final clean-clone and science-release certification remains after the 9x9 BO1 matrix passes:

4. From a brand-new clone with no parent XMage checkout, on the designated reference hardware under the published runtime compatibility tuple, build the environment, run one training update, recover/validate its store, run sampled and greedy paired evaluation, and reproduce the checked-in expected artifact digests for that reference runtime and hardware.
5. Verify generated outputs, caches, model artifacts, databases, and `target/` remain untracked and the clean-clone workflow ends with a clean worktree.
6. Tag the first release only after CI and clean-clone artifacts record the independent repository commit, original Mage checkpoint, nine deck hashes, card-database hash, environment binary hash, runtime compatibility tuple, versioned hardware record, and protocol versions.

The project is finished for science-ready BO1 only when that release artifact and the accepted sampled-primary 9x9 matrix can be independently reproduced on the designated hardware under the published runtime compatibility tuple and versioned hardware record.
