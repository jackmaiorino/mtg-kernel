# Successor player and explicit-registration BO3

This continuation connects the expanded CPU player to the BO3 path at measured source `63de51911386f5fde7e8a40e4e1fc4016c619681` and preserves the completed Ward pilot at source `0f479bad`. The release readback records a clean source tree. The full Multi-Deck BO3 Campaign remains active. These are engineering integration checks, with no candidate promotion or playing-strength conclusion.

## Implementation

- `run_expanded_batch` accepts a pinned `ExpandedModelSourceV1` and explicit supported registered 60/15 lists for each match. Actual lists reach the live game and are included in the result with zone and registration hashes. Labels are metadata. Registrations retain their combined 75 through sideboarding.
- The inference loader reuses the existing bounded checkpoint reader, exact current feature identity, ancestry, ordered parameters and Adam/state validation. Historical revision-2 checkpoints remain unchanged and cannot be relabeled as revision 3.
- The installed player's parameter and embedding hashes are distinct from its ancestral import receipt. Parameter replacement refreshes the cached sideboard embedding table. Old sideboard heads must reject changed weights or embeddings.
- `train_imitation` and `evaluate_imitation` accept an optional explicit `expanded_model_source` with the same import path. A fresh head then binds to actual successor parameters and embeddings. The original frozen-fit commands remain valid.
- Evaluating the existing grouped imitation dataset under new embeddings additionally requires `example_embedding_transfer`, with exact expected behavior-player and embedding-player weight hashes. Original source-match checks, pinned examples, connected groups and split audit remain unchanged. The transfer is recorded in results; it does not turn the inspected development split into a fresh playing-strength holdout.
- Existing static teachers are usable only when both actual registrations exactly match the canonical lists those rows describe. Changed 75s need a correctly bound learned policy, Keep, or a future explicitly list-bound teacher.

## Validation

The focused suite passed 53 tests: nine library checks for checkpoint restoration and actual model identity, twelve CLI/evaluator checks, and 32 registration/match/session checks. One existing real-export library test remains explicitly ignored; the actual checkpoint-to-BO3 check below supplies separate executable evidence.

The executable verification completed under [engineering-006/successor-bo3-001](E:/mtg-kernel-learned-sideboarding-evidence/engineering-006/successor-bo3-001). [Verification](E:/mtg-kernel-learned-sideboarding-evidence/engineering-006/successor-bo3-001/verification.json) and [compiled-source/embedding readback](E:/mtg-kernel-learned-sideboarding-evidence/engineering-006/successor-bo3-001/build-and-embedding-readback.json) record actual execution independently of the source tests.

| Check | Actual outcome |
| --- | --- |
| Fresh revision-3 collection/update | Two episodes and one CPU Adam update |
| Checkpoint reload | Actual parameter/state identity and original import ancestry preserved |
| Changed embeddings | Cached inference/sideboard table matches the checkpoint and differs from the frozen original |
| Explicit registration | `Affinity-engineering-one-Island` changes the actual registered 75; exact lists/hashes retained in results |
| Complete BO3 using successor and keep policies | One match against Elves, two natural games |
| Exact end-to-end replay | Whole-match output byte-identical, SHA `0df98057e64c93aebafc6dbc763ca51e969c71a981a0199b88323c8b2e784c6f` |
| Incompatible predecessors | Historical revision-2 checkpoint and old sideboard head both rejected before BO3 games |
| Frozen inputs | Unchanged |

The fresh checkpoint SHA is `279f3e0ef49dcaf686962f0ad515d8aceecb525ce7373a9fc9bafbbeec632d91`, model-parameter SHA `573831c5ba931911d2b6da417bb87b7b0a5882d2431612b6027a8bb6545e44be`, and successor embedding SHA `7f1b37ab7873c9229f4deb74cb377dcce1b7bf6827c5c1bc48c41ab94f9b93fa`. The original embedding SHA remains `063c25fd053a927aba7a8265ca06b446f7bcf6a3369de249b16762cce15aedd9`. The played BO3 executable SHA is `de43ef664e6dcbf0ea4171e768adb7b8dc27589d67c47fb22255e29fb90c6b31`. One CPU update and one registration check do not establish broader playing competence.

## Fresh successor-bound head and learned BO3

The separately identified head fit and learned-policy check also completed. [Verification](E:/mtg-kernel-learned-sideboarding-evidence/engineering-006/successor-head-001/verification.json) binds the head to the actual successor player and embedding table. The fixed 56-example, 500-action training split was fit once from seed 2026091401 for 100 epochs at learning rate 0.01, with zero value loss and 50,000 policy updates. Root recorded 2.165 seconds for fitting and 4.89 seconds for the four-stage fit/evaluate/evaluate/BO3 sequence. Head SHA is `93aa73834407c4cad77a81cf50d4dc0940e36c0af87d9345aa5bd109154fdbc7`. Its source play checkpoint remains `279f3e0ef49dcaf686962f0ad515d8aceecb525ce7373a9fc9bafbbeec632d91`.

| Read-only imitation diagnostic | Training split | Previously inspected evaluation split |
| --- | ---: | ---: |
| Independent connected groups | 9 | 3 |
| Exact final configuration | 19/56 | 8/18 |
| Exact movement plan | 3/40 | 0/10 |
| Done-only exact | 16/16 | 8/8 |
| Legal completion | 56/56 | 18/18 |

The unchanged grouped examples were evaluated with an explicit behavior-player to successor-embedding transfer receipt, preserving their original pinned source and split audit. The inspected evaluation split remains a development diagnostic. Its limited teacher-plan fidelity did not improve at these reported counts, and no inference about playing strength follows from them. This fit did not replace or mutate the original frozen fit.

One successor-player BO3 with learned versus keep sideboarding completed two natural games. The learned seat made one postboard decision containing seven actions: three cards out, three in, then Done. The actual registered 75s were conserved. The new head remained byte-identical during evaluation and BO3, and hashes verified that old model/head inputs were unchanged. This proves the updated player and freshly bound learned head execute together; it is not an outcome comparison, head ranking, strength result or promotion.

## Review disposition

Independent Codex review found no blocking defect. Root accepted its two material findings: successor head fitting needed the actual updated embeddings, and reusing the prepared grouped examples required an explicit representation-transfer receipt while retaining their original behavior-player provenance. Both changes are implemented and tested. Registration hashes include the label and initial zone partition, so a renamed label is not evidence of a changed deck.

Fresh Fable session `6063b7ec-e34e-4728-b107-e93a4c71a25e` failed HTTP429 with zero source reads and zero substantive tokens. This is an unresolved independent consultation, not endorsement. Root continued the already assigned reversible engineering integration with the Codex source review and focused executable validation; no new paid compute or GPU allocation was made. The [review note](E:/mtg-kernel-learned-sideboarding-evidence/engineering-006/successor-bo3-001/SOURCE-REVIEW.md) records the actual findings.

Broader training, meaningful policy diversity, terminal BO3 sideboard comparisons, human calibration, human play and production/search integration remain campaign work. A single CPU update does not establish expanded-deck competence, and imitation agreement does not establish useful sideboarding or brewing.

The [current production source audit](E:/mtg-kernel-learned-sideboarding-evidence/engineering-006/PRODUCTION-RECONCILIATION.md) confirms main's production scaffold is already inherited. Its actors and Stores still use the older feature/catalog contract. The next engineering boundary is an explicit-registration V3 production actor/trajectory interface with actual behavior identity and successor persistence. Search needs a reviewed dependency slice and V3 adaptation; its final three commits alone do not apply to this branch. No existing native measurement is changed by this plan.
