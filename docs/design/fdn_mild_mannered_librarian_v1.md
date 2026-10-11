# Mild-Mannered Librarian prepared source

Owned base b62fa679a07c49167ef918da4a701e0df23a4b31. The frozen
data/limited/fdn_v1/booster_pool_v1.json and card_names.json include this card
(FDN228). The selected base's registered Limited, Standard and default card
catalogs and generator contain no Librarian definition/program. The new owned
worktree is the only Librarian-named lane observed in the local worktree list.

Pinned Mage a5c90fe180021e70e2a644ade00eeab07f857a40:
Mage.Sets/src/mage/cards/m/MildManneredLibrarian.java is {G}, a 1/1 green Human.
Its exact ability is "{3}{G}: Mild-Mannered Librarian becomes a Werewolf.
Put two +1/+1 counters on it and you draw a card. Activate only once."
ActivateOncePerGameActivatedAbility uses instant timing and one activation.
AddCardSubTypeSourceEffect(Duration.Custom, Werewolf) defaults inAddition=false:
replace creature types, preserve noncreature subtypes, retain this continuous
effect only on the original permanent. DrawCardSourceControllerEffect uses the
ability's captured controller, independently of source survival. All three
primary helper sources are pinned in the same Mage tree.

The existing CreatureUpgradeV1 already carries persistent creature_types with
timestamp and once_activated indices. Reuse those fields for gameplay storage
instead of a second continuous effect, usage counter or source incarnation.
The once limit is definition-owned, selected only for executable Librarian's
sole exact printed program, index zero, on Limited AND NOT Standard builds.
Its paid-activation hook stamps the existing persistent zero index after
successful payment and before priority returns. Menus and pending activation
validation already consult the same activation_allowed hook. This is not a
one-per-turn approximation: cleanup and control changes preserve it, while
ordinary zone-change reset creates a fresh allowance on a new incarnation.
Unaffordable requests do not stamp it. Existing Surge Engine index-two behavior
and absent/default/Standard hook states retain their previous path.

The prepared program is a Sequence of the appended
CreatureEffectV1::LibrarianWerewolfAndCounters through the existing
EffectOp::CreatureUpgrade, then existing DrawCards(Controller,1).
The source instruction requires captured source/owner/controller/Battlefield/ZCC
and a surviving exact Battlefield incarnation. It installs Werewolf as the
persistent replacement creature subtype using the accepted layer timestamp and
adds exactly two counters through event::add_plus_one_counters, preserving that
counter producer's replacement/event authority. It does not require the current
controller or current definition to equal the captured values. The production
definition-owned stack validator supplies historical definition authentication
before this program executes. No cfg(test) root admission bypass or fabricated
printed stand-in is introduced.

A departed or reentered source receives neither new types nor counters.
Its independent draw still goes to the captured ability controller. A surviving
source that changes controller receives the effects while its old controller
draws. Witness Protection before resolution can suppress the printed ability
without countering an already paid activation: the later type timestamp sets
Werewolf; a later Witness type timestamp sets Citizen. Removing Witness does not
erase counters or the already paid once-only mark. The counter instruction is
source-bound and untargeted, using existing TargetSpec::None throughout.
No TargetSpec, subtype, card ID, serialized object field or action layout is added.
The appended CreatureEffect variant preserves predecessor discriminants; absent
upgrades preserve older serialized bytes and hashes by source inspection.

Four registered-operand primitive groups exercise genuine Llanowar Elves/Forest
objects and typed source contracts: surviving upgrade and counter/draw replay,
departure/reentry with independent draw, owner/controller/ZCC forgery refusal
for the source instruction, and unchanged bytes/hash for unrelated activation
hooks. These are typed upgrade primitives, not claimed printed Librarian casts.

Seven future printed groups require true executable Librarian registration.
They cover both seats, exact green/generic payment and unaffordable readonly
requests, sick/tapped activation, no target prompt, consumed limit before
resolution and after cleanup/control changes, exact snapshot replay, departed/
reentered/current-control source semantics, actual Witness aura timing and
layer timestamps, forged restored stack source/controller refusal before
effects, private ordinary draw observations, and real opponent-turn activation.
The Witness response installs registered High Fae Trickster as the actual flash
grant. Control changes are labeled board substrate setup, not a claimed printed
control producer. These fixtures remain unexecuted and deliberately fail lookup
until the parent registers the real card; they do not claim admission.

Existing CardPublicV2 can carry the reused upgrade fields and effective subtype/
counter facts. This source does not establish a new public/session/Human
contract or qualify predecessor policy projection. Frozen Human describe_effect
refuses the new CreatureUpgrade leaf. Frozen RulesVector emits Opaque for that
leaf and can retain the ordinary draw atom separately; once-only restriction,
exact persistent type replacement and historical-source lifecycle require
precise successor semantics before full model admission. The extractor source
provenance changes and older identities are not relabeled. No full-vector,
profile or model vocabulary claim is made. Copied/granted cross-definition
ability identity and naturally printed copy producers remain separate consumer
qualification, using the accepted historical-source contracts when composed.

Prepared metadata lives outside registered cards_v1.json. Static pinned
rustfmt, JSON parsing and diff checks are the only checks run in this lane.
No native tests/build/lint, registration, catalog hash derivation, guessed
profile, remote execution or new PR. Parent owns current-head composition,
actual printed qualification, public/Human successor work and serial admission.

Root-owned preparation now composes the original source8a104b56 at4071833b
from411/v79 baseb612 plus compile repair6b2dff91. It remains unregistered and
does not change catalog identity. A separate paid-activation journal authenticates
the reused once mark. Actual engine payment/publication alone emits its full
original root; same-incarnation duplicate payments, removed/duplicated once marks,
missing payment, erased completion and reintroduced completed stack rows refuse
at common read-only action/restore boundaries. Completion is recorded on normal,
resumed or countered resolution. Validation runs before ordinary advancement can
clear a malformed state. No new authority row or object field is emitted for
unrelated abilities; absent/default/Standard serialization and hash paths remain
unchanged by source inspection, with native parity still required. The original
incarnation keeps its once mark after control/ability changes and cleanup; genuine
zone changes reset the gameplay storage. Copied/granted cross-definition abilities
remain outside this selected root's admission. These are trusted engine journal
commitments, not cryptographic proof against coherent replacement of the journal.

Two additional future both-seat groups require actual registration. They restore
missing payment/once marks before and after genuine resolution, and erased
completion/reintroduced original roots afterward. Pass must refuse without any
byte mutation; ordinary advancement may only set the exact failure halt. Existing
four registered-operand and seven actual-printed groups stay intact. All thirteen
groups remain native-unrun. Precise meaning/public/Human successors are still
separate work; no global consumer/model/sampler or supported-gameplay claim follows.

The a125 fabricated-Finished finding is repaired with a separate committed
full-original-item operation and exact ordered completion reference. Genuine
atomic resolution binds the ordinary committed Draw attempt (including an empty
library attempt) and the exact two-counter occurrence for the surviving source;
the source-free case requires a real intervening Battlefield departure. Genuine
counter removal emits a distinct full-item Countered operation. Each paid row
owns either its exact live item with no operation/completion, or exactly one
matching operation followed by one linked completion with no live item.
Reintroduced transient authority, erased/redirected operations and duplicated
completion refuse before actions. Counter operations retain the trusted actual
engine-removal journal assumption, not cryptographic proof against replacing
all journals. The closed printed recipe is atomic and has no player-choice
continuation; the common resumed-completion hook cannot close it without the
same required operation. Old unrelated definitions emit no new operation.

A future genuine both-seat printed fixture deletes the unresolved paid item and
appends a fake Finished record, retaining original payment and once facts, and
requires read-only refusal. Its actual resolved branch retains counters/draw and
rejects erased operations, duplicate completion and redirected draw references.
A future genuine registered-source unit traverses actual activation payment and
the accepted exact-ID counter primitive, verifies no counters/draw, retained once
mark, one completion and ordinary absent-ID second counter behavior. The early
test module declaration is moved unchanged to the production file end. All
fifteen original/new prepared groups remain unrun; only pinned formatting and
diff checks have been performed for this repair.

The follow-up coherent Countered/Finished-pair finding additionally binds counter
closure to its independently committed actual removal. The exact-ID counter
primitive acquires a private owner only from the unique authenticated live row,
removes that row, and emits its full original removal occurrence before the
operation. Countered references that exact occurrence; completion requires one
ordered removal with an identical original item. Live and resolved rows permit
no counter-removal occurrence. Absent-ID, protected and unrelated old counter
paths emit none. The both-seat printed negative now forges the coherent operation
and completion together while preserving original removal journals, and the real
counter primitive fixture rejects erased or coherently rewritten removal rows
read-only. The original independent operation/removal journal is trusted engine
provenance; replacing that entire original authority is not cryptographic snapshot
integrity and remains outside the claim.
