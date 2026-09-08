# Visible duel view-model metadata audit, 2026-08-12

## Outcome

A direct player-visible source is technically plausible and likely easier to
maintain than full-screen OCR. The installed MTGO 3.4.158.4691 client exposes a
public WPF duel presentation layer in `DuelScene.dll`. Public metadata names
include battlefield, hand, prompt-box, phase, option-button, player, zone,
stack, and card view-model surfaces.

This does not make the live view model an eligible unrestricted input. The
same public presentation objects also expose backing-model references and
state that may not be rendered. A production broker must use an exact property
allowlist and discard every other value before any model, diagnostic, log,
artifact, or training consumer can observe it.

## Inspection boundary

This audit used only ordinary installed-file metadata from the active ClickOnce
client directory and the .NET metadata tables in `DuelScene.dll`. It did not load or
execute MTGO code, inspect method bodies, attach to the running process, read
process memory, enumerate live objects, intercept network traffic, inspect
protocol or message assemblies, send input, focus or move an MTGO window, or
emit any game state.

The inspected file had SHA-256
`72b99e1169f9f9445a510b2dae52f9212fb7300c2483b8bc8e02f5760f11904e` in
the active 3.4.158.4691 installation. The individual assembly is not
Authenticode-signed. Its hash is a drift marker, not a production trust root;
a future broker must bind it through the verified ClickOnce deployment and
signed MTGO executable rather than treating the DLL as independently signed.

## Candidate visible surfaces

Public type metadata named 177 duel, view, control, prompt, phase, action,
zone, hand, battlefield, or stack-related types. The most relevant candidates
were:

- `DuelSceneViewModel`
- `DuelBattlefieldViewModel`
- `DuelSceneCardViewModel`
- `PlayerViewModel`
- `ZoneViewModel`
- `PromptBoxViewModel`
- `PhaseControllerViewModel`
- `OptionButton`

Examples of properties with an apparent direct UI counterpart include current
phase, active player, player name, health, chess clock, mana-pool display
items, public zone counts, battlefield cards, prompt text, prompt active state,
standard prompt buttons, button name, enabled state, visible state, card tapped
state, visible counters, visible attack or block state, and visible card
actions.

Every such property remains untrusted until a reviewed UI corpus demonstrates
exact agreement with what the seated player can see. Property names alone are
not evidence of visibility.

## Non-exportable backing properties

The public metadata also named backing or potentially non-rendered properties,
including:

- `Game`
- `GameId`
- `GamePlayer`
- `GameCard`
- `OtherFaceGameCardDefinition`
- `ModelZone`
- `Owner`
- `Controller`
- timestamps and corrected timestamps
- raw action objects and command objects
- auto-target, pending-target, and internal interaction state
- replay and server-waiting state
- window settings and render geometry

These values must never be serialized, hashed into model features, logged, or
returned. Under the later transport-neutral permission, the sealed producer may
transiently traverse an exact audited backing route only to join a visible UI
control or seated-player-visible card to its corresponding action object. The
object and every internal-only field remain inside the producer and are
discarded before its first outward value. An unrestricted backing traversal is
still forbidden.

## Smallest safe broker

The next implementation should be a Daybreak-reviewed in-process exporter or
an equivalent isolated broker that receives the exact current duel window and
returns only a versioned visible projection. It should:

1. bind the exact client version, signed MTGO executable, ClickOnce deployment,
   hashed presentation assemblies, visible duel window, seated account, event,
   match, and game;
2. enumerate only compile-time allowlisted UI-facing properties and exact
   private join routes whose sources are already player-visible;
3. reject any unexpected type, property, collection element, or nullability
   shape rather than reflecting recursively;
4. map players to seated-player or opponent roles and objects to
   decision-local visible ordinals;
5. convert visible card definitions to visible card names, retain action
   objects only long enough for a sealed selected-action call, and discard all
   client identifiers;
6. produce `MtgoPlayerVisibleDuelDecisionInputV1` or an explicit abstention as
   its first exported value;
7. corroborate the UI-facing property set against composed pixels during
   qualification and around each live transaction until the exporter itself
   is formally reviewed by Daybreak;
8. retain no raw objects or values outside the live transaction and expose no
   debug or error formatter that can print them.

Production admission still requires the complete reviewed-frame corpus,
adversarial hidden-field fixtures, exact action-family coverage, reproducible
broker and producer binaries, and an explicit ratification commitment. This
audit grants no live evidence, model-scoring, input, event-entry, or spending
authority.

## Frozen candidate surface

The first version-pinned candidate surface is now encoded in
`src/visible_duel_viewmodel_surface.rs`. For MTGO 3.4.158.4691 it binds the
signed executable, ClickOnce manifests, `DuelScene.dll`, and `Card.dll`; lists
48 UI-facing property candidates with their required visibility context; and
lists 28 forbidden backing properties. Its deterministic commitment is
`19f25384a934c7673b2c1e22ad8dfc7bc2cdb35b2d714dd012de954bbe22a1fe`.

The candidate set includes visible phase, player-panel totals and highlights,
public or explicitly open zones, inherited card presentation values, rendered
counters, prompt text and controls, and visible action-menu labels. Context is
part of eligibility. For example, zone cards are eligible only for the seated
player's hand, a public zone, or a zone the UI explicitly reveals. A face-down
card cannot export a name merely because the presentation object carries one.

The frozen public list remains a metadata audit artifact. A later reviewed
producer adds a separate 15-property private join allowlist for visible-source
action binding. Neither list demonstrates a
complete duel projection, attest a live producer, or allow any property value
to reach the model. Every candidate still requires exact UI-corpus
qualification, and every authority flag remains false.

## Narrow broker protocol

`src/visible_duel_viewmodel_broker_protocol.rs` defines the data boundary for
a future audited producer. A request contains only the pinned candidate
surface, broker and producer binary digests, client identity, a nonce, and an
admitted before-frame commitment. A response must bind those exact values, a
strictly newer after-frame, and an unchanged commitment to the qualified
visible projection regions.

The only data-bearing success variant is
`MtgoPlayerVisibleDuelDecisionInputV1`. Failure is one fixed abstention enum.
Unknown response fields, free-form diagnostics, raw source values, internal
identifiers, crossed identities, stale frames, changed visible regions, and
authority claims reject. The protocol wrapper remains structurally untrusted
and cannot score the model or send input until a separately audited live
producer and capture owner attest the transaction.

## Accessibility comparison

The existing Windows UI Automation probe is a useful corroborating source, not
a complete duel-state source. It reads `CurrentName` only for controls that are
owned by the admitted MTGO process, reported on-screen, and fully contained in
the visible client. Its stronger diagnostic brackets the query with two stable
composed-pixel captures and verifies the matched regions are unchanged.

That path is intentionally limited to predeclared exact visible text. It does
not export arbitrary UI Automation values, discover semantics from labels, or
construct a complete battlefield and legal-action set. The direct presentation
exporter is therefore the preferred completeness path, while composed pixels
and the narrow UI Automation probe remain independent qualification and drift
checks.
