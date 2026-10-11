# Searslicer Goblin prepared source

Base b62fa679a07c49167ef918da4a701e0df23a4b31 (PR226). Searslicer stays
unregistered. No card ID, catalog/profile hash, model admission or coverage
change is assigned. Gorehorn, Strongbox, Gutless and all other owned roots
remain separate and unchanged.

Pinned primary source: Mage a5c90fe180021e70e2a644ade00eeab07f857a40,
Mage.Sets/src/mage/cards/s/SearslicerGoblin.java, constructor:
{1}{R}, 2/1 red Goblin Warrior. Raid: at the beginning of your end step,
if you attacked this turn, create a 1/1 red Goblin creature token.
Mage uses BeginningOfEndStepTriggeredAbility(YOU, CreateTokenEffect(
new GoblinToken()), false, RaidCondition.instance) and PlayerAttackedWatcher.
Mage/src/main/java/mage/game/permanent/token/GoblinToken.java's zero-argument
constructor creates exactly 1/1 red Goblin with no haste.

The appended trigger condition accepts only the controller's actual beginning
end-step event and the existing player_attacked_this_turn_v1 fact. It uses the
accepted successful nonempty declaration ledger, bound to round and active
seat, without another Raid primitive or state field. Declines, rejected
declarations, tokens entering attacking and another seat's history do not
enable it. The existing Conditional/ControllerAttackedThisTurn rechecks the
captured controller at resolution. Surviving attackers, drained events, source
departure/reentry and current source control cannot substitute that controller.
The existing CreateToken leaf supplies ownership/controller, untapped entry,
summoning sickness, no haste, real entry events and ordinary SBAs.

The generator's definition-owned trigger recipe includes the event, capture,
recheck and exact token semantics. The separate prepared JSON is not read as
a catalog registration. No generated database bytes are guessed or changed.

RulesMeaningSearslicerV1 selects the actual executable definition and its sole
printed trigger, checks the event/home zone/face/intervening shape, exact
untargeted conditional program and token definition, and binds schema, catalog
and producer source. Wrong roots, forged selectors, foreign definitions and
default/Standard catalogs refuse. It records the precise declaration and
captured-controller semantics. Frozen RulesVectorV1 gains only the exhaustive
new-condition branch with Opaque for the missing declaration predicate; existing
condition paths and atom/feature meanings remain unchanged by source inspection.
Extractor source provenance changes with the exhaustive branch; frozen archived
identities are not overwritten or relabeled. Byte parity still needs execution.
A global
vector vocabulary, full model/profile successor and source-qualified parity
remain separate work. This narrow companion does not declare the card admitted.

Existing ObservationV9 already exports the only additional public state fact,
successful declaration history for each seat. Existing HumanRaidV9 represents
ordinary Cast/Pass and distinct-source trigger ordering without a new action
or changed label; there is no Searslicer target or optional decision. Future
tests consume the genuine V9 session and safe projector, check flags, safe
handles, immediate retry and exact pending-snapshot replay with fresh selectors.
They do not claim that the frozen V9 adapter has the owning-rollback generation
protection provided by later separately versioned consumers. Same-source
ChooseTriggerOrderNext remains explicitly refused by predecessor Human labels.
Any admission requiring that larger menu or owning-rollback transport must
select a separate successor instead of relabeling frozen V9.

Two registered-operand primitive groups use actual declarations, the typed
condition/effect and Goblin Token to check both seats, failed/empty declarations,
captured-controller token facts, exact serialization replay, source departure
and stale histories. These are primitives, not printed Searslicer gameplay.
Seven integration groups require future true registration and exercise actual
casting, natural combat/end-step events, control-change setup, Witness
Protection before/after capture, source departure/reentry, stale restored
histories, token entry triggering Skyknight Squire, real cleanup/player flip,
V9 privacy and unchanged V8 history omission. Control-change setup is explicitly
a board primitive; it does not claim a printed control spell was cast.
The Human fixture uses a typed postcombat setup; natural traversal is covered
by integration fixtures. No fixture bypasses definition-owned root validation.

Only pinned rustfmt and diff checks run in this lane. Native typecheck, tests,
strict lint, serial registration/catalog derivation, successor consumer/model
qualification and current-head integration remain parent-owned and unrun.
