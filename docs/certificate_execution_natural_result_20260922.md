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

## Cost correction verified

Corrected source b2ee9703 passed3 controls/0 failures in272.562s. Isolated timing logs added in981b33a8 without altering deterministic results; binarySHA2564bff2c6af10b94e8c0e49bc0b2e136551842073bc42f5ea9989986654d2055dd. Both002 natural replays passed every001 acceptance condition. Artifact comparison certificate-cost-comparison-001.json confirms exact proof/strategy/execution reports and full collected trajectories, excluding only runtime package identities. An initial comparison omitted the top-level behavior_packages_by_seat identity field and failed; inspection found only that expected metadata difference, then verified every record's identity against its declared seat before normalizing both package-identity locations. No game, root or result was replaced.

| Corrected mode | cell25 seconds / peak RSS | cell45 seconds / peak RSS |
| --- | ---: | ---: |
| Tree only |1.887s /85.52MB|1.486s /65.66MB|
| Full audit and three perturbations |18.418s /2889.68MB|1.003s /77.32MB|
| Repeat full audit |20.835s /2837.26MB|1.385s /69.43MB|
| Isolated baseline root audit |3.882s (repeat3.848s)|0.03109s (repeat0.03236s)|

Isolated timing includes all-root tree construction, typed strategy extraction and exhaustive execution verification, not a deployed action-selector latency. Timings are per-root observations, not a distribution or fleet qualification. Cell25 full-mode elapsed improved3.22x and sampled peak RSS fell69.1%; tree-only memory fell98.9%. A live policy may avoid exhaustive diagnostic verification and perturbations, but no such speedup is assumed or measured. E-14's timing request is answered for this diagnostic; deployment latency/headroom still requires its own measurement. Both strategies retain exactly1957/1 natural winning leaves. All native checks are terminal; no model update or strength gate.
