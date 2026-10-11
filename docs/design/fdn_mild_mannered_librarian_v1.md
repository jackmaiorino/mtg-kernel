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
timestamp and once_activated indices. Reuse those fields rather than introducing
a second continuous effect, usage counter, source incarnation or history ledger.
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
No TargetSpec, subtype, card ID, serialized field or action layout is added.
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
