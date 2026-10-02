# Complete pilot behavior audit

All4,000 primary training trajectory hashes verified. Four analysis processes completed in36.25seconds. No new games or model updates were generated. Full32cell arm/window/opponent/phase aggregates are in result.json.

**Coverage finding:** 311673 learner substeps contained zero active prevention or cannot-prevent feature exposures. Both players' selected mainboards in all2,000 matched episode schedules exclude Prismatic Strands(card88) and Flaring Pain(card156); these registry IDs were checked in data/cards_v1.json. All384 final state-projection weights and both moment arrays remain exactlyzero. Printed-cost projection weights did change. The pilot therefore does not test learning to use prevention-state information; its NO-ADVANCE finding for this training configuration remains unchanged.

| Arm | Updates | Training wins /500 | Normalized choice entropy | Choice rows with top softmax probability >=99% |
|---|---|---:|---:|---:|
| control | 0-49 | 300 | 0.127 | 66.3% |
| control | 50-99 | 288 | 0.138 | 58.8% |
| control | 100-149 | 282 | 0.150 | 56.5% |
| control | 150-199 | 280 | 0.127 | 60.8% |
| structured | 0-49 | 295 | 0.122 | 68.6% |
| structured | 50-99 | 279 | 0.124 | 67.7% |
| structured | 100-149 | 284 | 0.122 | 67.6% |
| structured | 150-199 | 279 | 0.120 | 70.2% |

There is substantial concentration in the existing policy, but no clear progressive entropy collapse across this continuation. Confidence alone is not a defect: many MTG choices should be nearly deterministic. The exact execution sampler is quantized; these are softmax proxies on unforced substeps, with longer games/macro decisions contributing more observations. Different state distributions across bins confound causal interpretation.

Across all1,000 games per training opponent, control wins488 against frozen g115 and662 against A48; structured wins482 and655 respectively. Neither pattern proves an overly easy pool or strategic cycling. Initial-value terminal-return MSE remains roughly0.78-0.86 across bins, but noisy single-game returns cannot establish critic miscalibration. These are training diagnostics, not independent strength results.

Next causal information needed: unchanged-parent versus endpoint scores on exactly the same public decisions, stratified by deck and pre/postboard state. Measure KL/action disagreement before selecting a retention regularizer. A prevention-specific experiment would first need matched natural exposure to the relevant cards, verified before compute, and a separate control for the cost feature. Do not reinterpret this no-exposure pilot as a failed prevention intervention or immediately launch a combined roster/feature/loss change. Existing full BO3 result remains NO-ADVANCE, no promotion. Independent Fable review remains unavailable under known zero-read quota failure untilSep22 07:00EDT.
