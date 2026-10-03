# FDN Cackling Prowler

Cackling Prowler is a green 4/3 Hyena Rogue for `{3}{G}` with ward `{2}`.
At the beginning of its controller's end step, if a creature died this turn,
put a +1/+1 counter on this creature. Reference: XMage
`a5c90fe180021e70e2a644ade00eeab07f857a40`,
`Mage.Sets/src/mage/cards/c/CacklingProwler.java`.

Track an actual battlefield-to-graveyard creature move after replacements,
using its type before departure. Deaths of opponents' creatures and creature
tokens count; discard, exile, bounce and noncreature deaths do not. The death
stamp binds both the kernel's round number and active player. Clear it at
Untap, serialize it when present and include it in the state hash. The default
Pauper build leaves it absent and retains its old bytes/hashes.

Emit a beginning-end-step marker once at step entry, freezing whether morbid
was true then. Match only the source controller's end step. Recheck the
current death stamp at resolution and bind the counter to the trigger source's
exact battlefield incarnation. A returned physical card cannot inherit the
old trigger's counter. Materialize and authenticate bound source operations
inside conditional programs using the same rules as existing root programs.

Required checks: exact characteristics/payment/ward; no death, own/opponent
creature death, token death, noncreature departure, exile/bounce/discard;
death before Prowler enters; multiple deaths still add one counter; opponent
end step and next-turn reset; late end-step death or entry does not create a
missed trigger; source departure/reentry; save/restore with death history,
queued trigger and ward payment pending. Add focused XMage cases, v44 catalog
history/default compatibility and external natural-terminal replay before
closing the slice. Preserve both original deck files.

This is routine rules engineering. No training or playing-strength claim.

Runtime verification: fourteen focused kernel cases, twelve strict XMage
comparisons, catalog/session/prior-gameplay checks, default compatibility
and warning-denied Clippy pass. Two custom-deck games finish naturally and
replay identically. All 25 release production-boundary checks and combined-feature release
Clippy also pass. Hosted CI remains pending; see the validation report for
exact source and result hashes.
