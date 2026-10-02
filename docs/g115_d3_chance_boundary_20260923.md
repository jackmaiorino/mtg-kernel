# D3 future-chance boundary: bounded implementation

Source083588e4. New FutureChanceV3 mode overwrites the disposable clone's future random seed after hidden-card sampling. The seed uses a separate SHA256 domain and the existing simulation seed, already derived from visible root, experiment seed and simulation ordinal. It never mixes in the real game's seed. Legacy and D2 sampling modes retain their original behavior. No live state, policy RNG, game rule, budget, backup or reward changes. The new path is not yet wired into a playing source.

The private GameState helper preserves Legacy/EnvironmentV2 mode and physical-owner shuffle ordinals. The sole production caller is the new search-clone branch. The bound report retains original-state, policy-RNG and tensor checks, including error returns. The same D2 library plan, live-origin regeneration and boundary equality checks apply.

Bounded verification at E:/mtg-g115-lineage-20260923:

| Check | Evidence | Result |
|---|---|---:|
| New future-chance tests | d3-chance-tests-001 | 3/3 passed |
| D2 library/package/evaluation regressions | d3-library-regression-001 | 7/7 passed |
| Existing V4 search regressions | d3-search-regression-001 | 43/43 passed |

New tests cover both seats and RNG modes, all three library forms, known-card shuffle diversity over32simulation seeds, identical samples/reports despite different real seeds, preserved boundary/tensor/key/token, empty and partial choices, physical step parity, unchanged original state and stale-binding rejection. Existing D2's retained-RNG counterexample still passes, confirming that its semantics were not silently replaced. Build/test344.695s; cached regressions8.136s and4.563s. Four BelowNormal Cargo jobs, separate E target, Kimi's concurrent four-job UI tests preserved. All owned test processes exited normally; no GPU, paid compute or new whole-match measurement.

Scope limits for the same queued D3design review: real-seed invariance is tested at equal past shuffle ordinals. This is not a claim of invariance to different public histories or owner counters, nor a resolution of every strategy-fusion/response-model bias. The reviewer must assess that retained-counter convention. Mixed-V3 wrapper, strict versioned source/model binding, actual-g115 acceptance and compatible fleet qualification remain unfinished. These passing tests do not authorize D3 launch or establish strength. The current queue entry remains the only design-review request.
