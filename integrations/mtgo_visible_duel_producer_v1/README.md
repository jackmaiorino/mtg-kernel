# MTGO visible duel producer v1

This isolated .NET Framework 4.7.2 assembly is the in-process root seam for a
direct player-visible MTGO projection. It locates exactly one visible
`Shiny.Play.Duel.DuelScene` through the WPF visual tree, requires its public
`DataContext` to be the exact duel view-model type, and checks that all 89
compile-time allowlisted public getters exist on the pinned presentation
assemblies.

Version 1.23 invokes only exact allowlisted getters for the player-visible game
chrome, two player panels, mana display, prompt, visible standard buttons,
public zones, battlefield and stack cards, attachments, and visible counters.
It checks exact declaring types, bounded collections, bounded visible strings,
and basic two-seat consistency. It also performs thirty-five exact private joins
from visible enabled prompt controls and the seated player's visible cards to
the corresponding action objects. Those joins read only visible action labels,
action type, locally-performable classification, cast, activated, and mana
classification, visible mode labels, and the displayed mana color. Private
alternate-menu, group, action-choice, mode-choice, submenu, and attack-hover
values are used only as one-way guards. A non-default value forces a generic
abstention until the corresponding visible menu shape is modeled exactly; the
private value itself is never exported. Two exact private combat fields join
the already rendered blocker order back to already visible battlefield cards.
Their values are discarded before serialization. The turn
number is parsed only from the rendered `GameTurnText`, accepting the pinned
English-client forms `Turn N` and `Turn N: active player`; other locales or
formats abstain. The ordinary local-priority prompt is admitted only when the
prompt box is active and its sole visible enabled standard control is the
default `OK` action with the exact `OK` action flag. Other default prompts,
including numeric input, mana-payment controls, and target completion,
abstain. Raw action objects,
internal action identifiers, targets, timestamps, flags, and card or player
backing objects are never exported.

It never enumerates either library. It enumerates the seated player's hand,
but not the opponent's hidden hand. It never reads or exports the retained
name of a face-down exiled card for either player. Getter objects
remain local and are discarded. The producer can serialize a bounded
`MtgoPlayerVisibleDuelDecisionInputV1` slice containing only sanitized
player-visible values, or return one of the fixed abstentions. The slice is
explicit about which player-relative rendered exile panel contains each
visible exiled object, so later positions cannot collapse the two UI panels.
The slice supports noncombat priority decisions during upkeep, draw, Main 1,
Main 2, and the end step on any bounded turn. The stack may be empty or contain
only face-up, non-copy spells controlled by the seated player whose rendered
association set is empty.
It maps visible life, mana, hand and library counts, battlefield, graveyards,
exile, attachments, the seated player's visible hand, and that narrow rendered
stack slice in client collection order. The last collection item is the
rendered top of stack. Opponent-controlled spells, targeted items, abilities,
copies, face-down items, combat, revealed-zone windows, Initiative, choice
prompts, and modal choices remain closed. Raw association objects never leave
the producer. Its supported actions are source-bound cast, play-land,
activated-ability, and mana-ability actions plus the one visible priority Pass.

Version 1.21 also emits a separate coordinate-free attacker-selection result
for the narrow direct-opponent declare-attackers case. It contains the current
visible public state, the battlefield-order attacker candidates, each
candidate's visible selected state, and the fact that one enabled visible
`Done` control is present. Private action objects, attack-victim identifiers,
and player backing objects are used only inside the producer to require an
exact visible `Attack <opponent name>` or `Don't attack` join. They are never
serialized. The result carries no executable action binding. The Rust adapter
turns it into the checkpoint's sequential include/exclude deliberation without
touching the client. Exert, grouped attacks, alternate victims, modals,
nonempty stack, and ambiguous or incomplete controls still abstain.

Version 1.23 retains the blocker-selection result for the
narrow case with exactly one visibly attacking opposing creature, an initially
empty block lane, simple visible `Block` actions, and one enabled visible
`Done` control. The result contains visible battlefield ordinals and rendered
combat state only. A separate sealed monotonic dispatcher may add only blockers
chosen from that exact visible candidate order. It rebuilds the complete
sanitized state before every step, requires each prior blocker to appear in the
rendered assignment before another action, and presses `Done` only after the
exact desired visible set is present. It refuses preexisting or unexpected
assignments because removing a blocker has not been qualified. Client objects
remain transaction-local and are discarded after resolving the one already
visible `Block` or `Done` control. No target, action object, identifier, input
primitive, or hidden state is serialized. The ordinary selected-index
dispatcher still rejects this result.

For multiple visible attackers, version 1.22 emits two separate staged results.
The first presents only the rendered current assignments, unassigned visible
blockers, visible attacker order, and `Done`. After one blocker is chosen and
MTGO opens its target prompt, the second presents only the one visibly targeting
blocker and opponent attackers currently marked targetable by the client. The
producer verifies the presence of the internal target-set shape but never reads
or exports its candidate list, targets, IDs, or objects. Existing assignments
come only from rendered blocker order. Any targetable player, nonattacker,
ambiguous targeting card, incomplete control, or inconsistent presentation
abstains.

Version 1.21 retains the sealed attacker-step dispatcher for that result. The
adapter commits the exact source selection, candidate count, and desired
attacker bit set. On every call the producer completely rebuilds the current
visible selection, requires the same visible candidate ordinals in the same
order and the exact visible postcondition expected from the prior step, then
submits at most the first still-required toggle. It submits `Done` only after
all candidates visibly match the committed set. A missing, stale,
contradictory, reordered, or changed selection rejects without input. Plan
commitments and source-selection hashes are globally single-use within the
loaded producer. The only
outward response is the existing fixed submitted or rejected receipt.

Version 1.22 adds a sealed dispatcher for the staged multi-attacker blocker
results. A checked first-stage choice can submit exactly one matching visible
`Block` action or `Done`. After a blocker action, only a fresh target-stage
observation for the same visible blocker is accepted. The selected attacker is
then passed through MTGO's own `LeftClickDuringSelectTargets` path using the
unique loaded visible `Card_View` already bound to that visible target. The
adapter never reads or exports the target-set candidate list. It accepts no
later blocker action until a fresh outward observation contains the selected
blocker under the selected attacker. Between calls the ledger retains only
visible ordinals, turn, hashes, and one-use commitments. Client objects are
transaction-local and discarded before the call returns.

The observation method writes that bounded result to an exact broker-created
local memory channel and returns only a fixed transport status code. A second
offline-qualified method accepts only the SHA-256 of that exact serialized
visible decision plus one selected visible action index. It rebuilds the same
sanitized decision, requires an exact hash match, resolves the same current
private action binding, and calls the public `IGame.ExecuteAction` method once.
The same exact decision rejects every later index within the loaded producer,
and the decision is marked consumed before the client call. Only a fixed
submitted or rejected receipt leaves the producer. The client action object
never leaves.

The synthetic WPF fixture verifies all intended chrome, zone, card, and private
visible-action join getters, then passes the serialized result through the
strict Rust producer-result validator. It booby-traps either-library
enumeration, opponent hidden-hand enumeration, closed revealed-zone
enumeration, opponent card-action collection inspection, and face-down name
reads. It also verifies that no hidden fixture value enters the output. The
fixture validates the original untouched opening, upkeep, draw, and end-step
priority, a two-spell rendered stack and its fail-closed variants, plus a Turn
4 Main 2 state with changed life, mana, hand and library counts and populated
battlefield, graveyard, exile, and stack zones. The native
offline broker test
observes the synthetic decision, dispatches its Pass index, and proves neither
that dispatch nor another index for the same decision can be replayed.
The fixture also validates one selected and one unselected direct-opponent
attacker, rejects exert, alternate-victim, and missing-`Done` variants, and
proves the legacy selected-index dispatcher cannot execute the new result.
It separately validates the complete single-attacker blocker execution, proves that
rendered attacking and blocking state wins over contradictory backing fixture
flags, proves the ordinary dispatcher performs zero actions for the blocker
result, and proves stale or replayed steps cannot add an action. It also validates two attackers and two blockers, an existing rendered
assignment, and the intermediate visible target prompt. The fixture proves the
backing target-set candidates never enter the serialized result.
The fixture also runs the full blocker action, target click, visible assignment,
and `Done` sequence, and proves replay and out-of-stage attempts cannot add a
client action.

Version 1.5 was loaded into a clean MTGO 3.4.158.4691 process and invoked
while no duel was open. It returned only the fixed
`duel_surface_unavailable` abstention. That lobby-only result does not attest a
data-bearing getter, a complete visible projection, model scoring, live input,
event entry, or spending. In particular, the synthetic slice has not yet
confirmed its priority-pass control against a real duel. Initiative-bearing,
combat, general stack, revealed, and modal states remain unsupported. The
narrow stack mapping is synthetic-only. The slice is
therefore not live-authorized. The
producer now also reduces the rendered per-player Shields zone and each
rendered card's public frame style to an Initiative-holder candidate. A
non-null or ambiguous holder still forces abstention until an exact live UI
corpus qualifies the per-player placement. No shield card, client object, or
internal identifier is exported. Visible poison, energy, experience, or
radiation counter badges are detected at the player-panel layer and force an
abstention because the current decision schema does not represent their
amounts. A visible Companion panel likewise forces abstention because the
current schema does not represent that public card. Visible pile and card
selectors, trigger panels, wish flow, storm display, and temporary-zone
windows also force abstention until their public contents and choices are
represented. Commander and Planechase duel layouts, plus visible dungeon,
Ring-temptation, and speed presentation, also force the same generic
abstention. Target-bearing, X-bearing, sideboard, fake, confirmation, and
mode-count action state likewise forces abstention until its subsequent
player-visible modal is represented. Version 1.23 has not been loaded into
MTGO. Version 1.17 remains loaded in the current broker process; no attempt was
made to replace its locked assembly. A separate deterministic Release build of
the v1.22 source succeeded outside the client with zero warnings and zero
errors. It has not been live-qualified.
The live broker build explicitly rejects the dispatch command until it is joined to
the attended competitive authorization and confirmed-postcondition chain.
The next tranche must confirm the exact client pass control, add later special
visible game state, and validate the producer result inside the live broker
transaction. The broker's `LIVE_QUALIFICATION_2026-08-13.md` records the
lobby-only load and identity pins.

The v1 replay key is deliberately conservative: a byte-identical visible
decision later in the loaded producer also rejects. A production broker must
replace that limitation with a private observation-generation token, without
exposing the token to the model or operator, before live dispatch is enabled.
