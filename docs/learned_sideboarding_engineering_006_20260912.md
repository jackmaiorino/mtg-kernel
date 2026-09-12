# Successor player and explicit-registration BO3

This continuation connects the expanded CPU player to the BO3 path and preserves the completed Ward pilot at source `0f479bad`. The full Multi-Deck BO3 Campaign remains active. These are engineering integration checks, with no candidate promotion or playing-strength conclusion.

## Implementation

- `run_expanded_batch` accepts a pinned `ExpandedModelSourceV1` and explicit supported registered 60/15 lists for each match. Actual lists reach the live game and are included in the result with zone and registration hashes. Labels are metadata. Registrations retain their combined 75 through sideboarding.
- The inference loader reuses the existing bounded checkpoint reader, exact current feature identity, ancestry, ordered parameters and Adam/state validation. Historical revision-2 checkpoints remain unchanged and cannot be relabeled as revision 3.
- The installed player's parameter and embedding hashes are distinct from its ancestral import receipt. Parameter replacement refreshes the cached sideboard embedding table. Old sideboard heads must reject changed weights or embeddings.
- `train_imitation` and `evaluate_imitation` accept an optional explicit `expanded_model_source` with the same import path. A fresh head then binds to actual successor parameters and embeddings. The original frozen-fit commands remain valid.
- Evaluating the existing grouped imitation dataset under new embeddings additionally requires `example_embedding_transfer`, with exact expected behavior-player and embedding-player weight hashes. Original source-match checks, pinned examples, connected groups and split audit remain unchanged. The transfer is recorded in results; it does not turn the inspected development split into a fresh playing-strength holdout.
- Existing static teachers are usable only when both actual registrations exactly match the canonical lists those rows describe. Changed 75s need a correctly bound learned policy, Keep, or a future explicitly list-bound teacher.

## Validation

The focused suite passed 53 tests: nine library checks for checkpoint restoration and actual model identity, twelve CLI/evaluator checks, and 32 registration/match/session checks. One existing real-export library test remains explicitly ignored; the actual checkpoint-to-BO3 check below supplies separate executable evidence.

The next executable verification uses a fresh revision-3 CPU collection and update, then exact checkpoint reload, a changed noncatalog 75 through a complete BO3 and one exact replay. It also checks stale checkpoint/head rejection before games. Results belong under [engineering-006/successor-bo3-001](E:/mtg-kernel-learned-sideboarding-evidence/engineering-006/successor-bo3-001). No completion is inferred from the source tests.

## Review disposition

Independent Codex review found no blocking defect. Root accepted its two material findings: successor head fitting needed the actual updated embeddings, and reusing the prepared grouped examples required an explicit representation-transfer receipt while retaining their original behavior-player provenance. Both changes are implemented and tested. Registration hashes include the label and initial zone partition, so a renamed label is not evidence of a changed deck.

Fresh Fable session `6063b7ec-e34e-4728-b107-e93a4c71a25e` failed HTTP429 with zero source reads and zero substantive tokens. This is an unresolved independent consultation, not endorsement. Root continued the already assigned reversible engineering integration with the Codex source review and focused executable validation; no new paid compute or GPU allocation was made. The [review note](E:/mtg-kernel-learned-sideboarding-evidence/engineering-006/successor-bo3-001/SOURCE-REVIEW.md) records the actual findings.

Broader training, meaningful policy diversity, terminal BO3 sideboard comparisons, human calibration, human play and production/search integration remain campaign work. A single CPU update does not establish expanded-deck competence, and imitation agreement does not establish useful sideboarding or brewing.
