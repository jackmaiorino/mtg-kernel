# Fresh-origin cloud transport

Implement Phase 1's existing local/cloud training interface for the fresh Net8 origins introduced in commit `72a3a28006cfda0d9af1815cd824e701699426ef`. Campaign training, formal measurements and paid allocation remain paused. This is CPU engineering compatibility, not a speed or playing-strength claim.

## Decision

Copy the preserved `phase1/cloud` Python implementation into version-controlled `python/tools/phase1_cloud`. Keep the original tools and their evidence unchanged. Record byte identities of the copied baseline. The new source remains executable as scripts and extends existing imported-policy support.

Relocate a fresh source by copying its descriptor, pinned initialization manifest and raw parameters. Change only the two descriptor paths. The initialization manifest and checkpoints retain their original bytes, including historical Windows provenance. Validate transitive pins, exact model-source mappings, fresh versus imported schema compatibility, complete parameter/optimizer state, and initialization ancestry. Do not erase arbitrary paths or hashes to manufacture parity.

Ordinary training supports fresh Adam-zero sources and trained fresh checkpoints. BO3 transport initially supports its existing ordinary-checkpoint transition with fresh or imported actors; both actors still require trained checkpoints. Transport of arbitrary existing BO3 checkpoint graphs is a separate capability. Existing workers may produce and recover new BO3 graphs after the validated transition.

## Verification and boundaries

Run focused offline tests for malformed descriptors, mismatched origins, path relocation and semantic comparison, plus the preserved cloud suite. Then use clean Linux CPU builds for bounded local engineering checks of actual complete collection, update, checkpoint and continuation. Compare exact full state and trajectories within the same Linux runtime. Do not use cross-platform float equality or synthetic tests as production GPU/cloud qualification. Preserve frozen five-deck gates, CP7 exclusion, local reserves, single-owner native execution and lease/budget behavior.

Fresh registry transfer is being audited separately. This change cannot claim expanded card coverage, two trained independent lineages, useful RunPod acceleration, human readiness or Phase 1 completion.

## Review

Fresh read-only Fable consultation `540ec253-f46c-49b2-bc53-5f8701c1f5f0` started September 14, 2026 at 03:27 UTC. It failed with HTTP 429, weekly usage limit, before any source reads; `read_paths` was empty and the result was an API error. Receipts are in `C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/phase1/reviews/fresh-cloud-001`. This is not endorsement. Paid execution remains paused.

Lead continuation, September 15, 2026 (Jack: continue implementing Phase 1). Codex stopped on September 13 at 23:43 EDT with the focused fresh-origin suite failing (4 failures, 3 errors, receipt `fresh-cloud-checks-001/focused-tests-001`). Two causes were fixed: `validate_inference_identity` hardcoded the feature version labels, which rejected the synthetic build fixture; the BO3 packager already binds those labels to the actual Linux build fields, so the shared module now checks only their shape. The relocation test used Windows `pathlib` against a POSIX remote root. All 13 focused tests and the full 191-test offline suite then passed.

Codex was unavailable at its usage limit (reset September 19, 2026, 20:41 EDT). A labeled stand-in review ran as a Claude workflow: five independent lenses (native wire format, relocation and paths, tamper resistance, cross-module consistency, test adequacy) produced 11 findings; each was judged by three refuters (code trace, executable reproduction, design intent) and survived only with two non-refuting votes. Six survived, five were refuted. Dispositions, all applied before packaging:

- `parity_evidence.training_run` accepted a generated checkpoint's recorded ancestry on its own self-consistent claim. Every generated checkpoint must now equal the qualified initial origin (critical, three of three).
- `reference.py` crashed on a fresh Adam0 initial source (`checkpoint: null`). It now consumes the native inspection proof, from `--initial-inspection` or the payload's `initial_inspection`, and records the pin (critical, three of three).
- `semantic.trajectory_digest` validated fresh identities without an independently derived origin. It accepts an origin map from `fresh_source.fresh_origins`; `reference.py` and `parity_evidence.py` supply it, and a fresh behavior outside the map is rejected (major).
- `inspect_source` accepted producer runtime labels the native loader rejects. It now mirrors `validate_metadata`: four labels of at most 256 characters and `platform_system` in Windows or Linux (minor to major).
- The fresh replay path in `parity_evidence` and the ordinary packager's fresh Adam0 branch had no tests. Tests were added, including one over the frozen Windows fresh run (major).
- Refuted on design grounds and left unchanged: BO3 evaluation origin binding, feature-label binding outside the BO3 packager (the comment was corrected instead), the frozen Adam0 opponent asymmetry, and a bare `assertRaises` style point. One refuted finding was still adopted as a one-line guard: BO3 roster members with a fresh checkpoint must carry an Adam step above zero, matching the "both actors still require trained checkpoints" sentence above.

This stand-in is not Codex or Fable endorsement. Codex re-reviews the committed diff after September 19. Record the Linux engineering results and any further dispositions in `FRESH-CLOUD-RESULTS-001.md` under the Phase 1 handoff directory.
