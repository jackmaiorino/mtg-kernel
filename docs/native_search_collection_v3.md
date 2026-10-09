# Native search collection V3

`native_search_collection_v3::collect_native_search_population_v3` is the
explicit native population collection route for search algorithm V3. It
reuses the existing deterministic async scheduler, episode/actor seed
derivation, eight-slot weighted selection and native physical-group checks.
The caller supplies an `AsyncRolloutConfigV2`, base seed, a
`NativeSearchPopulationV3`, and a `NativeSearchBatchScorerV3`.

Construct the population from eight validated V3 search authorities and
eight integer weights. This constructor rejects V1 search authorities,
checkpoint occupants, an empty weight total and overflow. It does not
upgrade a frozen manifest or change an existing population in place.

Both learner and opponent decisions use the existing V4 scoring/action
producer on V3 session storage. Learner V2 cannot bind Ent forestcycling or
basic-land fetch roots either, so retaining a V2 learner would leave the
same collection failure. The fresh scorer receives a V4 packet identity,
validated model rows and extensions. The common row layouts are reused
storage types; no V2 scorer contract is asserted. Arena identifiers,
hidden cards/positions, RNG state and private diagnostics are absent from
the scorer view and collection rows.

The search opponent maps its canonical menu back to live physical action
indices. Each chosen command is consumed by the real session. A precise
`HiddenReferenceConflict` rejection from the fresh sampler invokes the
fixed `hidden-reference-conflict-canonical-visible-first-no-retry/v1`
collection rule: choose the minimum validated handle-free V4 action-row
key, then map that index to the live menu. The rejected seed is not retried;
no hidden source is pinned and no search backup or outcome is invented.
Other sampler, key, binding, authority and engine errors fail collection.
The receipt records the rejection tag and counts each fallback separately.

`NativeSearchTrajectoryReceiptV3` binds a fresh digest domain, the V4 action
contract, episode/seed/deck/seat metadata, the selected search authority
digest, every accepted decision/menu commitment, the fixed rejection rule
and the natural terminal. The old group/count/provenance validator is
reused internally with zero placeholder commitments; its resulting digest
is discarded and never claimed as V1/V2 action provenance. Foreign
episodes, stale groups, invalid indices and non-natural terminals reject.

The result contains ordered learner selections/logits and opaque fresh
receipts. This is an opt-in collection consumer, not an automatic migration
of the frozen V2 trainer or Store. Old scored families reject V3 search at
admission; the old V2 observer cannot unwrap a fresh receipt. Existing
V1/V2 serializers, identities, goldens, default constructors and measurements
retain their meanings. Substantial execution still requires the qualified
launcher and separately authorized campaign scope.

Regression checks cover both seats and Ent/Hawk/basic fetch roots through
the learner encoder and population opponent dispatch; a real Lembas
shuffle beneath its ETB trigger; precise reference rejection and stale
binding failure; eight-slot selection; menu/authority/rejection receipt
binding; and one completed native episode replay with identical fresh
receipts. These are engineering checks, not strength measurements.
