# Visible duel later-state metadata audit, 2026-08-13

## Outcome

The installed MTGO 3.4.158.4691 presentation layer contains direct UI-facing
state for ordinary battlefield ownership and combat presentation. The existing
producer's per-player `BattlefieldCards` traversal is correctly scoped to the
cards MTGO displays on that player's battlefield. It must remain the exported
ownership source. The adapter does not need to export a game-player, controller,
owner, protector, or card identifier.

Initiative, dungeon progress, stack details, and prompt-specific actions still
need a live visible corpus before they can be added to the producer. Metadata
names alone do not prove exact visible semantics.

## Inspection boundary and pins

This audit read installed assembly metadata and method bodies only. It did not
read live duel objects or values, inspect network traffic, send input, move the
MTGO window, or expose client identifiers.

The inspected 3.4.158.4691 files were:

- `DuelScene.dll`, SHA-256
  `72b99e1169f9f9445a510b2dae52f9212fb7300c2483b8bc8e02f5760f11904e`;
- `Card.dll`, SHA-256
  `071338a98d845d5c8db6ebd2f3c847e38ad548f50ba11d2a36973438cdec2ea8`;
- `WotC.MtGO.Client.Model.Reference.dll`, SHA-256
  `f3fef1adfd5b1b6d25a5db577f9a1b184c8b91bb98f19a13428c669266c20dc8`.

## Battlefield ownership proof

`DuelSceneViewModel.InitializePlayerInfo` supplies the combined rendered
battlefield zone collection to each `PlayerViewModel`. Each player model then
routes every candidate through its private `MultiplayerBattlefieldAddCard`
method. That method calls `BelongsOnBattlefield` before adding a card to the
public `BattlefieldCards` collection.

`BelongsOnBattlefield` compares the displayed card's root attachment controller
with the exact player model. For a Battle, it uses the displayed protector
instead. Therefore `PlayerViewModel.BattlefieldCards` is the client-created
display collection for that player's battlefield, including attached-card and
Battle layout behavior. The existing producer reads only this final collection.

The backing comparison is MTGO's internal construction of the visible
collection. It is not an adapter join and none of its identities cross the
producer boundary.

## Later-state candidates

- Combat: `DuelSceneCardViewModel.VisuallyAttacking` and
  `VisuallyBlocking` are presentation properties. They are stronger candidates
  than inherited backing flags. `VisualBlockingOrders` may provide visible
  ordering, but any element identity must remain a producer-private join to a
  displayed card ordinal.
- Stack: `DuelSceneViewModel.StackZone` is the rendered stack collection.
  `DuelSceneCardViewModel.IsAbilityOnTheStack` and visible ability or effect
  presentation fields are candidates for reconstructing stack entries.
- Dungeon progress: `CardViewModel.CurrentDungeonRoom` drives the rendered
  current-room state. It may export only the room state that the UI shows.
- Monarch: `IGameCard.IsMonarch` affects card presentation, but the backing
  value is not itself an eligible export. A public presentation counterpart or
  exact rendered-frame qualification is required.
- Initiative: `CardFrameID` contains `CLBInitiativeEmblem`, suggesting that the
  visible emblem can identify the current holder. No exact player association
  has yet been qualified. The current producer must continue to abstain after
  the untouched opening until that mapping is proven.

## Governing information boundary

Direct process attachment and exact client-object joins are allowed under the
reported permission. The first outward value must still contain only facts
available to the seated player through the MTGO UI. Raw client objects, hidden
cards, future draws, RNG state, internal action state, account and game IDs,
controller and owner objects, transport details, and other non-rendered values
must be destroyed before serialization, scoring, logging, diagnostics, operator
display, or training.

Private identities may exist only for the duration of one transaction to join
two player-visible things, such as a displayed card and its displayed action.
The exported join uses only a decision-local visible ordinal or selected visible
action index.

## Next qualification

Keep the currently loaded v1.5 producer unchanged until it is observed in one
no-stakes live duel. The next producer version should add later-state getters
only after exact comparison against composed UI frames, with adversarial
fixtures proving hidden values cannot enter success output, errors, or receipts.
This audit grants no semantic evidence, model, input, event-entry, or spending
authority.
