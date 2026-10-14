# Banishing Light prepared source

Unregistered source atop fff6d12fbc90098ae5c896e65ff970ad77783503 for issue #110. No ID, catalog hash, profile or Full admission is assigned. Pinned Mage a5c90fe180021e70e2a644ade00eeab07f857a40, Mage.Sets/src/mage/cards/b/BanishingLight.java, establishes {2}{W}, white Enchantment with one ETB ability: exile target nonland permanent an opponent controls until this source leaves. Mage/src/main/java/mage/abilities/effects/common/ExileUntilSourceLeavesEffect.java explicitly does nothing if the original source has already left before resolution.

The existing ordinary permanent spell builder supplies battlefield entry. BANISHING_LIGHT_TRIGGERS supplies only the ETB, reusing ExileTargetLinkedToSource(Target0). OpponentNonlandPermanent supplies relative controller/type filtering and the established full target legality/incarnation recheck. Adding Banishing Light to battlefield_exile_until_source_leaves selects the existing duration semantics: captured source generation must remain for exile; exact exiled incarnation is linked; the zone commit returns it immediately to the battlefield under its owner when that historical source incarnation leaves. This differs from Journey to Nowhere's separate leave trigger and leave-before-enter exile behavior. Neither card's prior behavior is redefined.

A source changing controller does not change or cancel its duration. Source departure/reentry cannot make an old trigger recognize a new source incarnation. An exiled object departing/reentering exile cannot be returned by the old link. Expiration is independent of the source's currently active printed abilities. Existing zone-event replacements, token state-based actions, source contracts, link records and return entry handling remain authoritative. No new enum variant, serialized state field, action shape, target ordinal or model tensor is added.

tests/fdn_banishing_light_v1.rs prepares actual both-seat printed casts and target announcements, creature/artifact/enchantment menus, own/land/foreign read-only refusal, no legal target completion, pending-target/announced/link exact restores, actual Broken Wings before and after exile, source/recipient controller-owner distinctions, exact stale exile and source generation behavior, and per-trigger invalid target resolution. Typed keyword grants cover changed hexproof and protection at recheck without forging a printed spell root. Source reentry is a labeled physical zone primitive rather than a fake sorcery recast over a nonempty stack. A known Partial Ajani is a nonland planeswalker recipient primitive with real loyalty entry/return, not admitted Ajani gameplay.

There is no selected Limited printed producer that legally removes a plain Enchantment's abilities. Witness Protection targets only creatures, so this fixture does not pretend it can enchant Banishing Light. A labeled serialized face-down characteristic primitive suppresses the source's printed abilities after a true printed exile while retaining its exact battlefield incarnation; expiration and restore are checked. This is primitive characteristic coverage, not a claim of a newly implemented face-down producer.

The existing public exile/link observations, target actions, Human projection and state snapshots express this operation without a new action contract. No model runtime or profile is admitted. Frozen RulesMeaning v1's ExileTargetLinkedToSource atom represents the battlefield-to-exile move and omits the until-leaves source duration (its existing comment describes a separate return trigger). It must not be presented as a full representation of Banishing Light's duration. A future effect/source companion must encode captured source generation, no exile after source departure, exact exile link and immediate owner return without rewriting frozen vectors or weights. The target-only RulesMeaning successor alone does not fill that effect gap. Broader Battle type support is absent and the selected FDN pool has no Battle card; planeswalker combat destination remains a separate full-pool dependency.

Pinned Rust 1.94.1 formatting, static recipe/diff checks and an owned source commit are the authorized checks. Native builds and fixtures remain unrun. Future printed fixtures require actual Banishing Light registration and use production definition-owned roots throughout. Serial integration owns registration, actual catalog identity derivation, affected gameplay/restore checks and consumer/model qualification. Historical profiles and artifacts are untouched.
# Composition after the seven-card candidate

Reviewed unregistered Banishing source `54cf12101` is composed locally onto
seven-card candidate `d096333b7` on a separate owned branch. Eight shared-file
conflicts preserve Librarian/Raid/extra-turn/Ashroot ownership alongside
Banishing entry/removal/linked-duration hooks. The ordinary engine driver
retains the seven-card validators inside the linked-Aura operation driver;
counter handling acquires each applicable owner before the one actual removal.
Historical Ashroot and linked-Aura sampler refusals are cumulative.

Shared observation builders retain every existing permission and add a false
Banishing permission for every old caller. The selected V26 constructor alone
sets it true. The existing five storage flags and Authority/extra-turn/combat
tuple remain intact, with Banishing appended to that tuple; observation
capabilities use one tuple to stay within the argument limit. All separate
V20/V21/V23/V25/V27 modes remain present. Banishing is still unregistered and
this composition is uncompiled/unrun. PR235 and its frozen Native77 source
remain unchanged; no new printing or catalog identity is admitted here.

## Staged genuine registration

The actual Banishing metadata appends at prospective id418 after the unchanged
226 FDN records, with a legal40-card reference mainboard. Its genuinely printed
integration and owning Human tests are enabled but unexecuted. Prospective214
Full/1 Partial/71 Missing is not accepted coverage. Frozen286 printing names
and24 external token dependencies are byte-exact. The current v80 identity is
deliberately stale for this419-definition candidate; derive a successor from
actual generated output before qualification. No native/gameplay, model,
sampler, experiment or full-pool acceptance is claimed. PR235 and its separate
418-definition guarded source remain unchanged.
