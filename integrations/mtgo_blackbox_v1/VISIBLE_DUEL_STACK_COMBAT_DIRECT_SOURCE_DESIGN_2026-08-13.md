# Visible duel stack and combat direct-source design, 2026-08-13

## Outcome

Daybreak's reported approval is transport-neutral. The adapter may inspect
client presentation objects and may use backing identities transiently to join
two things that the seated player can see. The first outward value must still
be no more informative than MTGO's UI or rendered Game Log.

This static audit identifies a viable next implementation path for stack and
combat state. It does not admit either path for live scoring or input. Every
new mapping must first pass synthetic adversarial fixtures and then an attended
comparison with the corresponding rendered duel presentation.

## Inspection boundary and pins

The audit decompiled installed assembly metadata and method bodies only. It did
not inspect live duel values, network traffic, process memory contents, account
or game identifiers, hidden cards, or randomness. It sent no input and did not
move or focus the running MTGO window.

The inspected MTGO 3.4.158.4691 files were:

- `DuelScene.dll`, SHA-256
  `72b99e1169f9f9445a510b2dae52f9212fb7300c2483b8bc8e02f5760f11904e`;
- `Card.dll`, SHA-256
  `071338a98d845d5c8db6ebd2f3c847e38ad548f50ba11d2a36973438cdec2ea8`;
- `WotC.MtGO.Client.Model.Reference.dll`, SHA-256
  `f3fef1adfd5b1b6d25a5db577f9a1b184c8b91bb98f19a13428c669266c20dc8`.

## Stack presentation proof

`DuelSceneViewModel.StackZone` is constructed as the shared rendered Stack
zone. `UpdateCardCollections_View` enables the stack panel exactly when that
zone is nonempty. `DuelStackPanel` lays out the zone's
`DuelSceneCardViewModel` children in collection order. MTGO treats
`StackZone.Cards.LastOrDefault()` as the active top stack item when showing
associations. The adapter can therefore export a dense stack position equal to
the rendered collection ordinal, with the last ordinal as the top item. It may
not use a backing card ID or timestamp for outward ordering.

Each rendered stack card exposes its visible `Name`, `IsController`,
`IsAbilityOnTheStack`, `IsActivatedAbility`, `IsTriggeredAbility`, and
`IsReplacementEffect` presentation. `IsController` is already reduced by the
client to whether the controller is the seated player. A private controller
identity may be used only to distinguish an actual opponent from a missing or
malformed controller when the public value is false.

The next stack slice may export:

- the rendered collection ordinal;
- the rendered stack label or visible face name;
- seated-player or opponent controller role;
- spell, activated-ability, or triggered-ability kind;
- a source ordinal joined to an already exported visible object; and
- targets joined to already exported visible objects or the two visible player
  panels.

For an ability, `SourceCardForAbilityOrEffect` resolves the client's
`TriggeringSource` association. The source object is not exportable. It may be
compared by reference only with backing identities of already rendered cards,
then immediately replaced with that card's decision-local visible ordinal.
Failure to resolve exactly one visible source must abstain.

Stack targets must not be inferred from `PendingTargets`. That collection
represents an interaction in progress, not the resolved targets of a stack
item. The rendered association path is the eligible source. When a stack card
is hovered, `ShowAssociations` iterates its associations and visibly marks card
or player targets. `CardAssociation.ActionTarget` drives the rendered targeted
state. A future producer may inspect the association objects transiently only
to reproduce this hover-visible target set. It must reduce every target to an
already rendered card ordinal or relative player panel, reject every unknown
or unresolved target, and discard all association objects and labels before
serialization.

The first stack implementation must abstain on replacement effects, delayed
triggers, special stack items, copies, face-down items, unavailable visible
names, unsupported target kinds, duplicated targets, ambiguous controllers,
or sources outside the represented visible zones. It must not interpret an
empty `PendingTargets` collection as a target-free spell.

## Combat presentation proof

`DuelSceneCardViewModel.VisuallyAttacking` is the presentation state used to
place a card in the attack lane. `VisuallyBlocking` and
`VisualBlockingOrders` drive visible blocker placement and ordering. The
elements of a blocking order contain backing identities, so they may be used
only as transient joins to already rendered attacker ordinals. Raw combat
participants and order objects must never leave the producer.

During declare attackers, `SetUpDeclareAttackersGroupActions` gathers the
seated player's visible creature-row cards. For each card,
`GetDesiredDeclareAttackerPhaseGroupActionsFromCard` selects locally
performable card actions whose rendered names begin with `Attack ` or
`Don't attack`. MTGO groups attack actions by private `AttackVictimId`, and the
card menu displays the corresponding attack-target hover indication. The
private victim ID is not model input. It may only join an action to the visible
player or permanent highlighted by that menu path, after which it is replaced
with a relative player role or visible object ordinal.

The first attack-selection slice should be binary and sequential:

- `ChooseAttackerInclusion { attacker, include: true }` for the exact visible
  `Attack ...` action;
- `ChooseAttackerInclusion { attacker, include: false }` for the exact visible
  `Don't attack` action; and
- a separately visible completion control before advancing the phase.

The producer must return the complete ordered visible choice set for the
current selection moment. It must abstain on exert variants, multiple attack
victims without a qualified visible target join, grouped multi-card actions,
missing completion controls, attack requirements, costs, confirmation modals,
or any action-menu transformation not represented by the semantic schema.

Blocking should remain closed in the first combat tranche. It requires an
exact visible candidate set for each attacker, a sequential inclusion mapping,
and a visible completion action. After declarations, combat observation may be
added only when every `VisuallyBlocking` card's ordered targets resolve exactly
to rendered attacker ordinals and the result agrees with an attended frame.

## Enforced information boundary

Values eligible to leave the producer are limited to visible text, visible
booleans and counts, relative player roles, visible collection order, visible
object ordinals, and the complete visible legal-choice order. The following
must remain transaction-local and must not appear in success output, fixed
errors, receipts, logs, diagnostics, corpus annotations, model features, or
training data:

- client card, player, target, action, association, and combat objects;
- card, player, game, action, victim, target, account, and transport IDs;
- hidden card identities, future draws, randomness, and non-rendered rules
  state;
- backing timestamps and stack order fields;
- raw association type values and association information; and
- private controller, source, target, and blocking-order identities.

A private value may cause only a fixed generic abstention or an exact
reference-equality join between already visible things. It may not select a
model feature value that the UI does not expose. The same restriction applies
to operator output: Jack and Codex must not receive hidden information that the
model is forbidden to receive.

## Implementation order

1. Add synthetic stack fixtures for one spell, one activated ability, one
   triggered ability, visible card and player targets, and every rejection
   listed above.
2. Add the stack presentation getters and narrowly scoped transient association
   joins. Prove booby-trapped hidden properties are never read or serialized.
3. Compare each accepted stack fixture shape with the equivalent rendered MTGO
   hover presentation in an attended no-stakes duel.
4. Add the binary attacker-selection fixture and visible completion control.
5. Keep blockers, model scoring, and input closed until their own exact visible
   qualification succeeds.

The external model bridge remains separately blocked on a kernel-owned public
history importer and a scorer that accepts only the sanitized player-visible
decision contract.
