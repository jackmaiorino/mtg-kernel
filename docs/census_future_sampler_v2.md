# Independent future randomness for census samples

`FastActorSessionV1::census_redeterminized_clone_future_v2(seed)` returns an
explicit `CensusFutureSampleV2`, identified as
`census-pinned-hidden-identities-independent-future-shuffle/v2`. Inspect its
session or consume it with `into_session()` and apply the real menu through
`FastActorSessionV1::step`. Record the sampler identity with any resulting
analysis. The seed comes from an external rollout schedule.

The sampler keeps V1's single determinization, known-card/pending-search pins
and source-conflict rejection. It then derives an independent future seed
with SHA-256 over a fixed domain and that external seed. It reads no real RNG,
private state hash or real environment seed. Both legacy RNG and environment
V2 mode are supported; physical-owner shuffle counters are retained. The
closing shuffle now samples known or declined search cards' future positions
instead of reproducing their real positions. Public decisions and cached
executable menus are unchanged.

Existing V1 census callers, default configuration, serialized artifacts and
frozen measurements remain on their original sampler. This opt-in sibling
does not claim V2 learner bindings can represent undisclosed library roots;
use the separate V3/V4 search/collection contract when scoring those roots.
It does not publish a training receipt or launch a campaign.

Affected tests cover both seats, both randomness modes, three library-search
forms, real-RNG independence, visible/menu invariance, physical action replay,
closing-shuffle diversity, shuffle-counter preservation and unchanged source
rejection. Their execution must be observed before qualification is claimed.
