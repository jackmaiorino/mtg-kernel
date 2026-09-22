# Natural certificate execution result, 2026-09-22

Engineering acceptance passed on the two fixed consumed roots using sourceef4ead24, executableSHA2568b0c4031d2b308a7d2b24eac2b926726af1e38d331a6942e22adba724ae70362. This is an executed local winning strategy under the engine's legal-menu contract, not a playing endpoint or strength result. g115 unchanged; M1 unmet.

| Root | Root action | Winning leaves | Opponent nodes | Own nodes including root | Execution transitions |
| --- | ---: | ---: | ---: | ---: | ---: |
| cell25 game1/d242 |0|1957|1237|1957|5150|
| cell45 game2/d113 |0|1|0|3|3|

Both action0 choices match the semantic face target and earlier independently extracted leaf counts. All leaves executed to a natural actor win, with no unknown leaf accepted. Original-session hash and inherited headroom are unchanged. Collection off/tree-only/on matched, original archived prefixes matched excluding runtime package ID, and on/replay files were byte-identical. Full audit and strategy digest matched across own-library reversal, opponent-library reversal and RNG alteration; the session hook checked all four knowledge vectors and sufficient library length before accepting permutations. These are three tested perturbations, not universal hidden-state noninterference. Binding checks establish path self-consistency, not independent hidden-information safety.

Evidence E:/mtg-meta-recovery-20260921/certificate-{burn,hand}-natural-001, completion.json and execution-check.json. Build123.025s. Source-review follow-up certificate-execution-source-review-001 completed successfully in session9ca79c04-c82b-40db-82f2-86860c0fed4c; nonvacuity and perturbation-applicability checks adopted before reading acceptance.

## Cost finding and correction

| End-to-end recorder mode | cell25 seconds | cell25 sampled peak RSS | cell45 seconds | cell45 sampled peak RSS |
| --- | ---: | ---: | ---: | ---: |
| Off |1.190|80.60MB|0.989|76.53MB|
| Tree only |9.205|7796.99MB|1.460|165.93MB|
| Execute plus three perturbations |59.371|9360.60MB|2.850|184.27MB|
| Exact execution replay |61.307|9349.74MB|1.518|187.73MB|

Memory is sampled process RSS every20ms, decimal MB; not an exact allocator maximum. Older tree-only receipts were1.911s cell25 and0.858s cell45, with no retained peak-memory measurement and no matched timing qualification. Nevertheless the multi-GB binding retention and large cell25 slowdown are material. All-node binding capture was being performed even in tree-only mode. The correction captures bindings only for lower=1 nodes when execution is explicitly requested. This preserves the existing wire proof, winning strategy and all rejection conditions. Original results remain retained. Corrected-source controls are running in certificate-controls-002; corrected binary/native replay and exact report comparison are still pending. Do not treat the optimization as verified.

No whole-match gate, model update, objective change, CP7 outcome or paid compute. A supported playing integration needs its own reviewed policy/fallback contract, coverage and measurement design; these selected archive roots do not supply a population effect forecast.
