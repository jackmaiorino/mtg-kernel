# Gates color counterfactual

Question: does changing only Gate extra-color choices improve the two exposed g115 Gates/Affinity matches whose traces showed off-palette choices? This is causal debugging, not an adoption or strength experiment. The broader-exposure pilot remains NO-ADVANCE and g115 is unchanged.

The separate `gate_color_diagnostic_v1` loads both exact V4 model identities and consumes the base selection once on every decision. Only the designated physical seat's Sea Gate or Citadel Gate color menu can be changed, and only when exactly one legal color appears among its own registered mainboard's printed cost pips. Other decisions, ambiguous menus, resets and the other seat preserve the base policy. Printed costs omit ability/alternate costs and sideboard needs; this rule is not claimed optimal. Own registration is legitimate information supplied to the diagnostic, not an added model feature. Stable references are only used to verify one source across a menu, never to rank choices.

The output explicitly names the intervention, base models, raw and applied actions, visible-state hashes and seed resets. It is not a Hamilton-policy training trajectory. Keep7, play, Keep sideboards, terminal outcomes and no search are fixed. A fresh root preserves all old evidence.

Qualification: actual live engine Gate bindings, both seats, hidden identities/order/allocation changes, menu permutation, Strands exclusion, unique-color ambiguity, base draw/reset preservation, plus complete baseline trace equality with the existing collector. Six executions cover the two physical seats in each arm and exact baseline/intervention replays. One match has a 60-second cap, total 240 worker seconds; first baseline times six must pass before further dispatch. All executions and natural games must complete. There is no integer improvement gate or statistical claim from this diagnostic.

Fable's September 19 fresh review failed HTTP429 with zero source reads, reset September 22 07:00 EDT. No retry or endorsement. Proceeding under the maintainer's bounded local authorization retains that independent-review gap. No paid compute, new training, CP7 selection or existing frozen-binary change.

## Completed September 20

Implementation and frozen runner commit `a6bd6de60d269d74c625b14a6378be332e4ee3ad`. Four tests passed, including actual g115 state `8139016c...43159f2`, Adam 32400, on both Gate types/both seats under changed hidden hand identities, library order and arena allocation. These are bounded fixtures, not a universal invariance proof. The test log is `E:/mtg-postboard-campaign-20260920/gate-color-engineering-001/tests.log`. The clean release build took 4m31s with rustc/cargo 1.94.1, explicit MSVC linker 14.43.34808.0, at most four BelowNormal jobs. Build and verification receipts are in that engineering root.

Execution root `E:/mtg-postboard-campaign-20260920/gate-color-counterfactual-001` is COMPLETE. Four unique BO3s, eight natural games, 1,520 gameplay decisions; including exact baseline and intervention replays, six executions/twelve games. All six succeeded with no timeout. First-baseline projection 10.575 worker seconds versus 240 cap; actual all six 6.116 seconds. Executable SHA-256 `6d87f174b3db726d52f77e92aad0e183be80c36b97e013338bd2e0f3633b04b2`.

Both baselines match all 698 original gameplay records, including complete visible observation/menu hashes and selected actions, as well as starts, environment seeds and winners. Baseline replay SHA `c94f39424d3472e876e68f59f015fc8ff8bce217392b59460699cf5e922d60ee`; intervention replay SHA `51821824b8edb27e6b510353df7ebab656fb6e7f5ddf349b794e8ad8d82cd859`. Recount verifies all pinned inputs and outputs and reconstructs the match winner from each game's winner.

| Gates physical seat | Baseline game score | Intervention game score | Changed color decisions |
|---|---:|---:|---:|
| 0 | 0-2 | 0-2 | 7 |
| 1 | 0-2 | 0-2 | 3 |

The intervention did not reverse these selected losses. It did change the trajectory: seat-0 game one grew from 307 to 427 decisions, with cast selections expanding to include Sacred Cat, Journey to Nowhere and Brainstorm. More actions/casts are not a strength metric. Seat-1 game one retained the same cast-selection counts and 136 decisions. No broad color-futility conclusion follows from two exposed matches against the familiar A48 training opponent. Keep g115 untouched and do not launch another broad training run from this result.

The native encoder path is now traced beyond the Python reference: `flat_policy_v4.rs::encode_current_flat_scoring_decision_owned_v4` delegates to the common V2 builder; `flat_policy_v2.rs::register_objects` uses `add_private_card` then `add_stable` for hand cards. These populate the card token and actor-relative location fields with default public characteristics. `native_flat_tensorizer_v4.rs::fill` uses `native_flat_tensorizer_v2.rs::fill_native_flat_decision_tensors_v4`, whose object feature path adds no printed mana-cost field or registry cost lookup. Tokens and legal-action availability can still teach or indirectly reveal costs; this is not proof that the model lacks all cost information, nor a causal explanation for its losses. No feature contract was changed.

Next: inspect remaining spell-use/resource failures using the preserved actor-visible traces before choosing a learning intervention. Gate-color repair alone was insufficient here; do not simply stack hand-coded patches or reinterpret the earlier NO-ADVANCE pilot as successful. Keep broader coverage, retention, independent opposition and actual human feedback as the research requirements. Fable review remains unavailable and no endorsement is implied.
