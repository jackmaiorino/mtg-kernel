# Library-search contract repair

Issue #187's correction identifies two failures: V2 cannot bind undisclosed
library-search offers, and the census can relabel an object retained as a
frozen trigger source. Pending-selection pins alone repair only the Hawk case.

`KernelNativeSearchAuthorityV1::current_v3` opts into algorithm
`deterministic-redeterminized-is-mcts-integer-ucb-v4-future-chance/v3`.
`KernelNativeSearchOpponentV1::select_action` creates a disposable V3 session
view and uses the existing V4 visible key, fresh action tokens and
`FutureChanceV3` whole-object sampler. Every root action maps back to the
original executable candidate, including when the root's V2 cache reports
`HiddenActionReference`. Selected indices and root statistics use that live
menu. Search never mutates the authoritative session.

The v1 constructor, identity, seed formula and sampling branch retain their
existing behavior. New authorities use a distinct algorithm and key identity;
they reconstruct and validate those identities together. This does not revise
past measurements or install a new population instrument automatically.

The census samples exactly once with its existing draws and pending-selection
pins, then rejects a sample when a relabeled hidden object is retained as a
stack or pending-trigger source. Rejection does not pin hidden sources, retry
seeds or convert failed simulations to outcomes. It also catches historical
sources whose zone-change generation advanced when they entered the library.
The census's inherited future-RNG limitation remains.

## Explicit native population migration

The native async rollout's full-episode receipt binds frozen Flat Action V2
commitments for learner and opponent decisions. Its trajectory V2 envelope
delegates to unchanged V1 decision rows. Passing V4 bytes under
`flat_action_v2_commitment` would mislabel provenance.

The legacy native async runner therefore rejects search v3 at admission with
`UnsupportedSearchTrajectoryContract`, before workers or games start. The
supported caller is the public `select_action` API followed by the returned
physical index in `FastActorSessionV1::step`; both V2 and V3 sessions work.
The separate [native search collection V3](native_search_collection_v3.md)
consumer supplies a fresh V4 scorer and receipt contract for both learner and
opponent library-search decisions throughout native collection and replay.
Callers explicitly opt in; existing V2 trainer and Store identities remain frozen.

Engineering checks cover Ent forestcycling, Hawk, basic-land fetches, both
seats, V2 cache failure, V3 inputs, physical index mapping, repeatability,
authoritative-state preservation, hidden-source census rejection and native
admission. No training, campaign sweep or strength claim is part of this repair.
