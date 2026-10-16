# Perforating Artist native preparation

This checkpoint adds a reusable definition-owned optional non-target sacrifice
of one nonland permanent. The unpaid program stays bound to its exact printed
path. It does not register a card, reference deck, test module, public consumer,
or model recipe. The inline `effect::perforating_artist_v1` module contains the
printed Raid program, complete native definition predicate, closed public stage
validator, definition-gated trigger dispatch, and separate consumed native row
owner. Existing definitions and the 459-entry registry remain unchanged. The
Artist selector is false while its complete printing is absent.

The input is frozen FDN124, Scryfall printing
`72980409-53f0-43c1-965e-06f22e7bb608`, Oracle
`a3643f66-dfa2-4275-a093-23e162496354`: {1}{B}{R}, a 3/2 Devil with deathtouch.
The frozen pool is `data/limited/fdn_v1/booster_pool_v1.json`. The pinned local
Mage input is commit `a5c90fe180021e70e2a644ade00eeab07f857a40`, file
`Mage.Sets/src/mage/cards/p/PerforatingArtist.java`, whose raw git blob SHA256 is
`fe03a6dff7725aa8a06729ca360c00493d94f7da7dec34e23b59bab10dde9824`.
Its end-step Raid trigger rechecks attacking this turn and lets the affected
opponent sacrifice a nonland permanent or discard a card instead of losing
three life. The native two-seat program offers sacrifice, then discard, then
life loss, using the captured trigger controller through ExecCtx.

The primitive appends an EffectOp, frame, and selection purpose without moving
existing variants. The prompt binds the exact complete payer-controlled
battlefield roster using effective Land type and zone-change counts. It is not
a target: no targetability, hexproof, or shroud filter is applied. Selection is
zero or one object. An empty eligible roster proceeds directly to the unpaid
subtree; decline does the same. Payment uses the existing native sacrifice
batch, including replacement/departure capture, and requires that the selected
battlefield incarnation left. It does not require arrival in the graveyard.
An engine halt or an unchanged original incarnation is a failure.

The root, branch path, payer, trailing frames, exact original roster, candidate
bindings, selected incarnation, and answered frame are validated. The nested
existing discard operator uses the same definition-owned unpaid edge. Old
policy projections and search reference admission explicitly refuse the new
payment tree. The disabled rules-vector meaning remains Opaque. There is no
Full promotion, catalog/profile hash, DTO retag, or consumer permission change.
All production primitive code is inline in already source-hashed effect.rs.

The closed Artist validator checks the exact captured source definition and
controller, printed root, End step and true Raid history. Later control,
suppression, departure and reentry do not replace the captured controller.
Public boundaries are exactly the initial printed root, the waiting sacrifice
or discard prompt, and their answered frames and guards. The existing generic
validators still check complete rosters, selection bindings and answer content.
Consequences execute synchronously, so an empty frame list, false Raid branch,
or exposed unpaid program is not a valid public restore boundary.

A separate move-only row owner is created before actual native removal. It
captures the full original row, exact position, full remaining stack, and, for
resumption, the entire pending continuation. Consumption requires an exact
match. Initial public begin refuses the new payment tree without its native
owner. Both owned begin and resume lend the authentic removed row back only
for the unchanged full public resume validator and reducer. The entire loaned
stack must remain unchanged; the original remaining stack is restored on both
Ok and Err before returning. Spinner and Quandary ownership is unchanged.

The later registration must first compose actual Valkyrie's Call459, then
append Artist460. This checkpoint does not claim live Artist integration or
paid coverage before actual printing registration.

Required registration coverage is genuine paid {1}{B}{R} casting and actual
attack/end-step trigger capture, both seats, no attack, no eligible permanent,
sacrifice, decline and discard, decline and life loss, exact native sacrifice
and discard events, land-creature exclusion, noncreature inclusion,
controller-versus-owner and non-target selection, captured trigger controller,
source departure/reentry, prompt/answer/snapshot restore, and readonly refusal
of forged payer, source, path, roster, candidate incarnation, and continuation.
No genuine Artist paid test can run before actual printing registration. No
existing test body changes in this checkpoint; it adds zero registered cases.

Verification is pinned Rust 1.94.1 formatting, diff checks, and a lightweight
source audit of enum-prefix, registry, existing-test-body, and identity-closure
preservation. Rust compilation and execution were explicitly excluded. There
is no runtime correctness, speed, native qualification, or admission claim.
