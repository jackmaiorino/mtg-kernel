# FDN first actual life gain each turn

Integration preparation for issue #110, originally claimed in
[comment 6087899788](https://github.com/jackmaiorino/mtg-kernel/issues/110#issuecomment-6087899788).
V62 is accepted on default branch at `8e65be54`. The owned v63 candidate
appends IDs 332-333 and wires its focused gameplay test into CI. Its 40-card
fixture and candidate registry pass 27 deck tests and six frozen booster-target
tests with Python 3.13.14. Candidate metadata covers 134 full target names;
accepted default-branch coverage remains 132. Generated catalog identity,
separate live store profile and native gameplay qualification remain pending.
No passing native check or accepted card support is claimed yet.

| Card | Exact printed behavior | Existing effect |
| --- | --- | --- |
| Vanguard Seraph, {3}{W}, Angel Warrior, 3/3 flying | First life gain each turn: surveil 1 | Controller surveil, count 1 |
| Cat Collector, {2}{W}, Human Citizen, 3/2 | ETB Food; first life gain during each own turn: one white 1/1 Cat | Existing Food and Cat token definitions |

Oracle/printing identities and wording come from the retained Scryfall FDN
pages in the coordinator scratch directory. Primary XMage implementations
are [Vanguard Seraph](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/v/VanguardSeraph.java),
[Cat Collector](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/c/CatCollector.java)
and [the game-scoped positive-gain watcher](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage/src/main/java/mage/abilities/common/GainLifeFirstTimeTriggeredAbility.java).
Those files are clean at the cited local reference commit.

`GameState::life_gain_turn_v1` is an optional engine-only ledger, enabled
when a first-gain definition exists anywhere in the initial pool. It stores
turn, active seat, exact permanent-history boundary index, and omitted-empty
event-time captures. Construction and each real Untap append an anchored
`LifeGainTurnBeganV1` history marker. Every positive committed gain consumes
its player's ordinal, even with no family source on the battlefield; zero,
negative and prevented lifelink damage do not consume it. Extra turns reset
at Untap independently of changes to the round number or active seat.

Immediately after each positive `LifeGain` commit, first-gain abilities
capture existing `PendingTrigger` data with controller, definition ability
index, source incarnation and effect. The optional queue also binds each
capture to its committed gain history index. Collection requires that exact
index in the unprocessed positive-gain suffix of the current event batch;
restoring an already processed capture cannot replay its historical gain.
After resolution, validated
captures join the ordinary waiting-trigger group before SBAs and APNAP
ordering, and drain once. Generic final-battlefield matching skips this
condition. A later ETB cannot see an earlier gain; a later departure,
blink or control change cannot erase or relabel an already captured ability.

Snapshots retain the ledger, atomic-batch captures and private surveil
continuation. Missing/corrupt boundaries or malformed captures reject the
new family explicitly as `InvalidFirstLifeGainHistory`; historical gains
and trigger families retain their wire shapes and construction behavior.
The new history variant is appended after all old enum variants, preserving
their derived Hash discriminants. Absent ledger fields serialize identically
and add no hash input; present ledgers use a manual tagged hash extension.

This batch adds no public first-gain flag. Existing public projections and
frozen flat encoders remain unchanged. Hidden resampling must clone the
ledger/captures exactly; neither hidden family presence nor ledger presence
may change actor-visible keys. Prepared tests compare actor-identical
hidden pools, validate legacy omission/hash behavior, exercise per-seat
ordinals, batches and turn resets, and cover event-time entry/departure,
source control changes, Food costs, Cat identity, pending capture/surveil
restore, stale capture replay after drain, and valid captures followed by
later atomic events. The next-turn regression passes the real End window
through Cleanup and Untap, verifying Cleanup's damage reset before checking
the new ledger anchor. Hosted tests will supply actual results after the prior versions
integrate. Pinned rustfmt and diff checks are the only local Rust checks;
Stage4a's desktop reservation, paid-run authority and frozen evidence remain
untouched.

October 10 integration preparation preserves the recent Standard event/state
fields, appends the new trigger/halt variants after the existing variants,
and maps Vanguard Seraph's flying keyword. The rules-vector record retains
the exact first-gain condition. Its existing vocabulary represents the life
gain and turn-history read, but the global event ordinal and own-turn gate
are explicitly reported as opaque. This does not reduce gameplay support;
it records the extractor's representational limit. Registration, catalog
admission and gameplay qualification still require the generated v63 identity,
separate store profile and native checks. The supported two-core catalog build
is queued behind existing claims, including a malformed S4a build shell reported
to its owner through the shared mailbox and collaboration PR137. No reservation
or other owner's process was changed.
