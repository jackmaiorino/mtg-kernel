# MTGO visible card-action menu audit, 2026-08-13

## Result

Direct parsing remains practical under the seated-player-visible-only boundary,
but the raw card action collection is not itself the final visible legal-action
set. The pinned MTGO client constructs the visible card menu from
`DuelSceneCardViewModel.Actions`, then changes its presentation and selection
behavior using alternate-menu, grouping, mode-choice, submenu, action-choice,
pile, attack-hover, drag-cast, and auto-mana rules.

Producer version 1.15 therefore supports only the simple one-action-per-visible-
label subset. It reads the transformation fields transiently as one-way guards.
If any guarded field is non-default, the producer emits only the fixed generic
`projection_incomplete` abstention. None of the private field values, underlying
objects, or identifiers is serialized, logged, exposed to the model, or exposed
to the operator.

## Evidence scope

This was static inspection of the installed managed presentation assemblies. It
did not inspect a live duel object, capture pixels, access network traffic, send
input, or move or focus the MTGO window.

Pinned installed files:

- `MTGO.exe` version `3.4.158.4691`, SHA-256
  `bb9c1a189674cd7333b1d997259109576cafe78767f0f11badaad2203c388e92`
- `DuelScene.dll` version `3.4.158.4691`, SHA-256
  `72b99e1169f9f9445a510b2dae52f9212fb7300c2483b8bc8e02f5760f11904e`
- `WotC.MtGO.Client.Model.Reference.dll` version `3.4.158.4691`, SHA-256
  `f3fef1adfd5b1b6d25a5db577f9a1b184c8b91bb98f19a13428c669266c20dc8`

The inspected path was `Shiny.Play.Duel.Card_View.ProcessMouseUp` and the
`WotC.MtGO.Client.Model.Play.ICardAction` interface.

## Confirmed presentation behavior

- The card view obtains candidate actions from the exact visible card view
  model's `Actions` collection.
- A normal left click excludes actions whose alternate-menu flag is set. A
  right click can present alternate actions.
- Group labels and action-choice labels create headings or reordered entries.
- A mode-choice mapping groups multiple backing actions into one visible mode
  chooser.
- Submenu entries attach to a parent menu entry rather than appearing as an
  independent top-level choice.
- Attack-victim metadata adds visible hover behavior.
- Pile context can synthesize a grouped action when the same local action is
  available across a pile.
- Drag-cast and the user's auto-mana setting can choose a backing action without
  first showing the full context menu.
- `CanBePerformedLocally` participates in pile behavior. It is not a general
  proof that an action is or is not visible in the context menu.

These findings mean action order and cardinality must be reconstructed from the
presentation algorithm, not inferred from raw collection order alone.

## Version 1.14 closure

Version 1.13 added six exact private visible-source-bound join getters:

- `ActionChoices`
- `AltMenuAction`
- `AttackVictimId`
- `GroupName`
- `IsSubmenuItem`
- `ModeChoiceMapping`

Version 1.14 adds eleven more one-way guard getters for the player-visible modal
that follows action selection:

- `Targets`
- `HasXTarget`
- `InSideboard`
- `ConfirmModeString`
- `ConfirmBeforeTargetingOwnCard`
- `IsFakeAction`
- `ModeMinChoices`
- `ModeMaxChoices`
- `XIsAMinimum`
- `XDeterminedByTargetWithGreatestCMC`
- `XTargetDivisor`

For the currently supported simple noncombat main-phase slice, all guards must have inert
defaults and the target collection must be empty. Static inspection of
`PromptBoxViewModel.Execute` confirms nonempty targets enter the visible target
selection interaction and a non-null confirmation string opens a visible
confirmation dialog. The other values describe X, sideboard, fake-action, and
mode-choice behavior that the current semantic slice cannot continue safely.

All menu-shape fields must be empty, false, or
`-1` as appropriate. The preexisting `ModeOptions` collection must also be
empty at semantic mapping time. Synthetic tests independently set every new
field to a non-default value and require the fixed generic abstention. The
private values themselves are never exported.

This does not authorize or claim support for grouped actions, modal choices,
targets, piles, attacks, blocks, drag-cast, automatic mana selection, or later
game states. It narrows the supported noncombat slice by preventing an incomplete
legal-action list from reaching the model.

## Next implementation boundary

General League and Challenge play requires a deterministic visible-menu
normalizer that reproduces both left-click and right-click choices, flattens
group and submenu structure into stable player-visible semantic actions, binds
each semantic action to exactly one transient executable client action or
visible modal transition, and confirms the resulting postcondition before a
second input. The normalizer must export only visible labels and semantic game
meaning. Raw flags, IDs, backing objects, client settings, and private metadata
must remain inside the adapter.
