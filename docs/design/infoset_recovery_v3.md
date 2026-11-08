# Bounded information-set recovery v3

Issue #579 records persistent `R2:export` faults in the frozen W1 4k-k1 soak:
opposing Fiery Temper rollback spans collide at the final synthetic transcript
position, and an old Damage reference is untracked. Evidence is collab commit
`ec83ac0b50f4ccdae301b06cb9a4702db93319d1`, under
`LANES/lead/w1/composition/tools/soak/results-20261108/`. The first matching
desktop records are `refusals-33728.jsonl` (step 55, turn 4) and
`refusals-51436.jsonl` (step 64, turn 5) in the frozen 4k-k1 refusal log.

Consumers explicitly call `infoset_v1::export_v3`. The state and payload use
`x_kernel_infoset_v3`; v3 inherits v2 event-time visibility and its conservative
face-down and snapshot restrictions. V1 and v2 remain defined, and the existing
bridge still selects v1. No frozen entrant, result, or research instrument is
changed. Wiring a successor entrant and qualifying it are separate work.

The rollback repair orders only a group of tied, currently public objects whose
final synthetic spans have distinct public owner seats. It uses seat order.
It never orders by native IDs, allocation order, names, zone-list positions,
or private reference ownership. Equal-owner ambiguities still refuse. Paid-cost
and discarded references remain intact and undergo the existing rewrite checks.

The Damage repair preserves source, target, and amount. Current linkable object
handles stay linked; historically observed identities stay named. An endpoint
without observed identity becomes the existing per-reference unnamed tombstone,
with canonical definition and epoch, only inside a bounded public scope:

- Both declared decklists use cards from the frozen runtime catalog whose file
  SHA-256 is `68e7602f3a4df6217119406973954630800c358a10fca9f28e6cf9f20fd3b851`.
- The public stack, cast/activation announcements, effect continuation, and
  pending trigger phase have finished.

The guard does not inspect whether a hidden physical copy occurs in any private
ledger. Catalog drift and other public pools retain the old refusal for this
special repair. Ordinary exports without an unobserved Damage reference retain
their existing pool support. Completing a public operation can therefore recover
export without clearing or replacing its old history.

The consumer audit distinguishes immediate processing from future reads. Trigger
matching, life-link-like eternal hooks, and combat marker creation consume the
newly committed event slice. The unfinished-operation guard excludes that work.
Life-loss/raid history queries consume player and amount; game-summary damage
totals also consume owner, which tombstones preserve. Persistent identity custody
in Drake, Crystal, Gorehorn, Goblin Negotiation, Greatwurm, Niv, and Lathril is
outside the declared roster. The roster's generated Blood/Food/Treasure and
creature tokens have no such custody mechanism; its Chain Lightning copies repeat
the same immediate damage program. Token generation and copying do not introduce
any of those off-roster printed mechanisms. A larger pool needs a new audit and
versioned scope, not a check of secret runtime ledger contents.

Regression coverage uses both viewer seats, reversed native allocation, retained
paid-cost references, private library knowledge, known and unknown damage
identities, and later export after refusal on the same continuing state. These
are bounded correctness checks, not a replay or requalification of either soak.
