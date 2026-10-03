# Fresh V3 model initialization

The explicit `trainer-seeded-v1` producer creates a complete Net8 parameter payload from a declared base seed. The native loader installs all 33 tensors before creating Adam at step zero. This is a separate origin from an imported Store model. It does not fabricate export, run or checkpoint ancestry.

Generate with `python -m mtg_kernel_rl.phase1_fresh_initialization_v1 --request ABS_REQUEST.json --output FRESH_ABS_DIR`, using the repository's Python package. The strict `mtg-kernel-fresh-initialization-request/v1` request binds `lineage_id`, `base_seed` and `target`. The target contains a pinned registry file, the compiled card database hash, V3 contract and encoding digests, and source/descriptor hashes. The producer verifies the actual source registry and V3 files; the native loader owns verification of the compiled target.

Outputs are `initialization.json` and `parameters.f32le`. The manifest records the actual generator commit, source-file and runtime pins, original Python generator configuration, base/derived seeds and complete tensor hashes. It has no timestamp or output directory, allowing identical requests to be compared exactly across fresh processes using the same source and runtime. The original fixed-seed common snapshot remains unchanged.

Use this source descriptor through the existing `ExpandedModelSourceV1.play_import` pin:

```json
{
  "schema": "mtg-kernel-fresh-initialization-source/v1",
  "initialization": {"path": "ABS/initialization.json", "sha256": "ACTUAL_SHA256"},
  "parameters": {"path": "ABS/parameters.f32le", "sha256": "ACTUAL_SHA256"}
}
```

Set `checkpoint` to null only for explicit initialization. Later continuations pin a full fresh-origin checkpoint, preserving all parameters, Adam moments, step count and the sampled scorer-bias anchor. The source descriptor pins actual input files; historical producer paths in the manifest are portable provenance and are not opened or rewritten by a consumer.

The read-only `phase1_fresh_initialization_v1 ABS_CONFIG.json` command loads this source and exports an inspection of all parameters and Adam0. Its request schema is `phase1-fresh-initialization-inspection-request/v1`, with fields `source` and `output_directory`. The latter must be fresh and absolute. Inspection does not collect a game, perform an update or produce a training checkpoint.

Fresh identity is `mtg-kernel-fresh-play-initialization/v1`. Its ordinary inference receipt is `mtg-kernel-expanded-deck-inference/v2`, checkpoint is `mtg-kernel-expanded-deck-fresh-checkpoint/v1`, and trajectories involving either a fresh learner or a fresh opponent use `mtg-kernel-expanded-deck-trajectory/v3`. Imported identities and their original schemas remain unchanged. A trained fresh ordinary checkpoint may enter the explicit BO3 objective-transition path; complete-agent and sideboard fitting paths use actual installed model identities.

Current limits:

- A distinct initialization seed establishes a separate starting model, not independently learned strategies or playing strength.
- Exact regeneration is qualified against a pinned runtime. Rust does not reproduce Torch's initialization RNG.
- Fresh-origin registry transfer and cloud packaging are not supported by this change. Their existing imported-only readers remain strict.
- Existing sideboard heads remain bound to their actual gameplay weights and embeddings. They cannot be relabeled to fit a fresh model.
- GPU training, campaign execution, acceleration and human strength require separate qualification.
