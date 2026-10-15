# Flamewake Phoenix bounded source candidate

Issue #110, source prerequisite only. This candidate starts at
`eb05b03b241a09cbc2c29e79470de018741ddf83` and appends the actual printing as
Partial after the unaccepted Fake and Ravenous candidates. Its provisional
combined ID 444 is derived from 192 base plus 252 existing FDN records. The
previous 252 rows are unchanged. The new synthetic reference has 40 cards and
Full deck preflight refuses its Phoenix. No accepted coverage, frozen profile
literal, catalog hash, model or sampler admission changes are claimed.

Pinned Mage `a5c90fe180021e70e2a644ade00eeab07f857a40` defines a red
{1}{R}{R} 2/2 Phoenix with flying, haste and attacks each combat if able. Its
graveyard ability triggers at the beginning of its controller's combat if
that controller controls a creature with power at least 4. Resolution rechecks
that same controller's current effective creature power before offering {R}.
Paying returns the original graveyard incarnation untapped. The exact source
paths, Git blobs and SHA-256s are in the adjacent provenance JSON.

The appended trigger condition composes the existing beginning-combat event,
graveyard home zone and effective-power query. Its program is
`Conditional(ferocious, MayPayManaThen(R, Conditional(SourceStillInTriggerZone,
MoveObject(ThisSource, Battlefield), empty)), empty)`. The existing combat
static supplies the attack requirement, and ordinary keywords supply flying
and haste. No printing aliases, gained death ability or new death system are
introduced. A source that leaves and reenters the graveyard cannot substitute
its later incarnation; the optional payment remains legal and can be spent
even when the original return fails.

The shared optional-payment validator now recognizes its original root form
and the single true ferocious branch. It checks the definition-owned root,
path, payer, exact ordered cost, consequence and empty remainder at both the
live prompt and accepted answer. The actual interpreter evaluates ferocious
once before the prompt. Validation does not add a later threshold gate after
resolution has begun. The automatic payment solver currently uses floating
mana and repeatable tap sources; it offers no sacrifice selection during this
payment. No late sacrifice-payment producer is claimed.

Seven genuine paid test groups cover both seats. Three real Mountains pay the
Phoenix cast and an opponent's real paid Lightning Bolt kills it. Tests restore
before combat, on the published trigger, at the payment prompt, after accepting
and after the actual payment/return. They check haste, mandatory attacks,
flying blockers, tapped/defender restrictions, the actual opposing turn,
opposing-only and below-threshold ferocious, departure of the sole four-power creature before resolution, decline,
unpayable {R}, a graveyard round trip, actual engine counter removal and
read-only refusal of changed cost/consequence/path/payer. Supporting permanent
entries and response departures are labeled existing primitive operands,
not paid-printing claims for other producers.

Native compilation and execution are pending with the assigned lead. The
test filter is `--lib flamewake_phoenix_v1` under the pure FDN feature set.
Only pinned Rust 1.94.1 formatting, 61 Python deck-tool tests, enum-prefix/static
checks and a diff check have executed. Rust tests are written, not passed.

Historical authority remains the existing generic trigger/continuation
contract. The new branch check does not create a separate historical receipt
proving that a restored choice was originally reached through a real combat
occurrence or that its ferocious predicate held then. Complete printing-owned
trigger/payment custody and refusal of internally consistent fabricated
histories remain a precise follow-on boundary. No such history is constructed
as a positive test owner. Phoenix appends a subtype without changing frozen
creature-choice/changeling lists; extending those lists needs successor
qualification. Full public meaning, backend/Human, models, fair sampling and
native cross-family qualification remain pending. This candidate is not Full
and does not establish issue #110 completion.
