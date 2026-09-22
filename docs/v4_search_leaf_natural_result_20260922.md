# V4 leaf natural-root compatibility, 2026-09-22

Both archived g115 roots passed the fixed replay checks. This is engineering evidence, not a strength gate, M1 repair or candidate promotion. g115 is unchanged. No training or new measurement panel ran.

| Check | cell25 game1 decision242 | cell45 game2 decision113 |
| --- | --- | --- |
| Archived ordinary tensor/logit/value bits | exact | exact |
| Fresh search tensor vs ordinary tensor | bit-identical | bit-identical |
| Whole collection with diagnostic off/on | identical | identical |
| Archived full trajectory | identical except verified runtime package identities | same |
| Fresh-process repeated diagnostic | byte-identical | byte-identical |
| Own-library/opponent-library/RNG perturbations | tensor/output bits identical | tensor/output bits identical |
| Maximum absolute logit difference, deterministic vs ordinary tanh | 0.00000286102294921875 | 0.000003814697265625 |
| Absolute value difference | 0.00000011920928955078125 | 0.000000029802322387695312 |
| Argmax agreement | yes | yes |
| Search value | 0.8831637501716614 | -0.385670006275177 |

Both values were finite and inside the declared [-1,1] output domain. These two activation comparisons are observations, not a global error envelope or proof of unchanged sampling under search. The unchanged trajectories follow because the diagnostic runs after the original sample and never substitutes an action.

Evidence: `E:/mtg-meta-recovery-20260921/v4-leaf-natural-replay-001/completion.json`; all six recorded output hashes reverified. The helper `check-v4-leaf-replay.py` pins the two previous certificate receipts and their source models, constructs packages through the existing collector loader, checks each row's seat package before excluding the two runtime-package identity fields, and retains all other trajectory fields. Source commit `1e7d47876caa42f187e74ddc39d27b519c1ce6c5`; executable SHA256 `c179561535344bda66f1dd639819f2d68b46f5ac0329e64e00128372ce6edfa6`; build receipt `v4-leaf-replay-build-001/completion.json`. g115 checkpoint SHA256 `88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1`.

The seven synthetic controls passed in controls-005; prior fixture failures are retained in the companion controls note. Together these establish the tested plain-V4 fresh-scratch forward seam. They do not establish full S0 search acceptance: V4-compatible redetermination, keys and stepping remain to implement and verify, as specified in `v4_search_state_boundary_design_20260922.md`. Existing V2 and disabled-search guards remain unchanged. No CP7, human or league claim follows.
