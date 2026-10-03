# Fixed trace pair: exact replay, descriptive divergence

Both predeclared examples reproduced the original root, trajectory digest, terminal and all non-record row fields. Loss61 has50 gameplay records and winnerp0; win0 has22 and winnerp1. Two owner-checked CPU replays completed in2.490seconds. Evidence: E:/mtg-meta-recovery-20260921/conditional-continuation-traces-001/{completion.json,descriptive-analysis.json}. The400-row formal result is unchanged.

They first diverge at decision116, p1 choosing Galvanic Blast's target. Actor-visible inputs and behavior masses are identical at that decision:

| Trace | Selected target | Recorded probability | Behavior argmax? |
| --- | --- | ---: | --- |
| Loss61 | Burning-Tree Emissary | 75.7578% | Yes |
| Win0 | Samurai Token | 2.7061% | No |

The first differing action in this pair is therefore not a p1 deviation from argmax in the losing trace. It does not follow that greedy play is worse, that the low-probability target is optimal, or that this first divergence caused the terminal difference. Both seat RNG streams differ. No counterfactual action experiment was run.

Full recorded non-argmax choices, with ties treated as argmax:

| Trace | Decision | Seat | Sampled choice | Probability | Argmax choice |
| --- | ---: | --- | --- | ---: | --- |
| Loss61 | 130 | p0 | Activate Mountain for red | 0.1150% | Pass |
| Loss61 | 136 | p0 | Do not block Clockwork Percussionist with Samurai Token | 15.8174% | Block |
| Loss61 | 157 | p1 | Keep Chain Lightning copy's target | 16.1320% | Change target |
| Win0 | 116 | p1 | Galvanic Blast targets Samurai Token | 2.7061% | Target Burning-Tree Emissary |

At157 the observed life totals are p0=2, p1=5; after the copy resolves they become2,2, and the trace later ends with a Samurai attack and p1 loss. That sequence suggests a search requirement worth checking, not a certified counterfactual: choices can span a copy-retarget flag, a later target menu, and subsequent responses. A search policy must execute an entire validated strategy, not override one target and assume the model supplies the rest. The two-spell certificate at113 had already been abandoned in both traces.

No training or sampler change is proposed from this pair. E-12's teaching redirect is accepted; its subsequent correction of zero-risk/causal-regret/prevalence wording was read. A certificate gives an engine-relative available winning strategy. It does not by itself validate a deployed search policy or establish population whole-match gain.

Fable's fresh results review upheld the diagnostic ADVANCE and allowed this rescoping. Additional artifact-only checks directly verified archived remainder equality and equality for three engineering rows across different batch compositions; receipts archived-remainder-check.json and batch-composition-check.json. These support implementation isolation, not iid proof. The optional review suggestion to inspect all12 loss traces remains available but has not run. The next consequential choice is a bounded search contract using the current model as baseline/fallback, with no hidden-state leakage or guard weakening; source/literature preparation is in collab/lit/20260922-search-redirect-preparation.md. M1 remains unmet.
