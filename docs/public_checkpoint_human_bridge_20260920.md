# Public-checkpoint human inference gap

Read-only preparation while the independent replication runs. No package is promoted, no live human session is changed, and this bridge is not implemented yet.

`human_match_v2.rs` owns a concrete `FrozenPlayPolicyV1`. Its constructor validates a `CompleteAgentPackageV1`, resets that policy's sampler, reads its embeddings for a fitted sideboard head, and binds completed-game summaries to `package.gameplay.identity.model.weights_sha256`. Replacing only the model field or exporting only the trained legacy weights would either lose the public projection or misidentify the deployed model.

The existing CPU loader `expanded_deck_training_v1::public_features::load_for_evaluation` already verifies config, checkpoint, optimizer bytes, optimizer ages and projection mode. It returns `PublicInputPlayPolicyV1` plus a composite identity including both trained base and public weights. The BO3 public evaluator uses this loader without constructing a GPU device. Reuse this exact loader and policy sampling behavior for human inference.

The bridge needs an explicitly versioned public-checkpoint source and package validation before any game starts. Human journals and game summaries must bind the returned composite model identity. Reuse the fixed-seat projector and request binding unchanged. Keep legacy packages and serialization compatible. Do not attach a fitted sideboard head that is bound to the parent's older embeddings; supported opening and sideboarding behavior must be explicit. A default-off Escape menu filter is not full Escape support and does not resolve the existing human projection gaps.

Acceptance requires: actual trained nonzero public weights retained; changed/missing checkpoint or optimizer rejected before a session starts; same public decisions, seeds and legal bindings select the same actions as the qualified evaluator; replay remains deterministic; both physical human seats preserve hidden information. Then test the existing unsupported-choice cases and natural match completion before describing a usable multi-deck delivery. The old g115 Rally package remains a separate development baseline, with zero verified desktop games.

Implementation follows resolution of the replication. Any new build uses a new package with actual runtime provenance and the supported compute qualification for substantial validation. Fable's recorded quota failure remains an independent-review gap; this preparation is not a completed consultation.
