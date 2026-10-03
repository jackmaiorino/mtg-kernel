# Native human match V2

V2 runs a human BO3 against one verified complete-agent package. It is an inference interface: it does not fit a model, learn from the session, or promote a checkpoint. The V1 executable, configuration schema and existing sessions remain separate.

| Component | V2 behavior |
| --- | --- |
| Gameplay | Loads the pinned gameplay source, model identity and sampler through the complete-package loader; uses the automated paired-policy selection path. |
| Opening | The model keeps seven. The human can use the existing London mulligan and bottoming prompts. |
| Play/draw | Explicit initial chooser; the model uses its package's fixed Play or Draw choice whenever it owns the choice. |
| Sideboarding | Explicit Keep, or a compatible fitted greedy head with the actual gameplay embeddings. Model choices use its permitted history and commit before the human submits a new configuration. |
| Search and learned opening | Unsupported descriptors reject during package loading. |

The human executable and automated collector require packages bound to their respective executable bytes. Preserving identical behavioral components in those two packages is not literal package-byte equality. Loading also checks the compiled clean source/tree, runtime features, registry and toolchain identities. The fitted head's SHA binds the same bounded bytes that are decoded.

## Configuration and service

Run `human_match_v2 --config CONFIG.json` with strict schema `mtg-kernel-human-match-config/v2`. Its fields are:

| Field | Meaning |
| --- | --- |
| `package` | Absolute complete-package path and SHA-256 of its exact file bytes. |
| `registered` | Two executable registrations, each with `label`, `mainboard` and `sideboard`. |
| `human_seat`, `initial_chooser` | Physical seat 0 or 1. |
| `seed` | Explicit BO3 seed; games use the same environment and physical-seat sampling streams as the automated collector. |
| `summary_tags` | The same removal/counterspell card tags used for automated BO3 summaries. |
| `journal_path` | Absolute, fresh server-private journal path. |
| `max_physical_games` | 1 through 16, default 16. |
| `max_physical_decisions`, `max_policy_steps` | Each 1 through 1,000,000; defaults 100,000 and 200,000. |

The service uses the existing JSONL command vocabulary: `current`, `action`, `concede`, `sideboard`, `play_draw`, `mulligan`, `keep` and `bottom`. Each request has a short `request_id`. Mutations use the current game, opening revision or prompt sequence as appropriate. Gameplay `action_index` is the displayed, canonically sorted public index, not the engine action index. Responses use `mtg-kernel-human-match-response/v2` and contain only the fixed human seat's view.

Exact mutation retries return their original response. Reusing an ID for a different command rejects, and invalid/stale commands do not advance model sampling. The session retains up to 8,192 mutation entries within a 64 MiB serialized cache budget, with no eviction; it stops before admitting a mutation beyond that bound. Commands are limited to 128 KiB and responses to 1 MiB. These are serialized-data limits, not total process memory guarantees. The executable runs on a fixed 16 MiB worker stack.

Shared summary accumulation observes actual decisions once. Natural outcomes and explicit concessions have distinct provenance, including concessions during London opening. Initial draws and London redraws remain part of the existing retained-event summary semantics; an opening concession has no gameplay decisions or sampled resource turns. Engine failures and decision caps do not fabricate completed games. A sideboard failure after a completed game retains that result and stops continuation.

Journals retain private package provenance, model selections, raw summaries and sideboard evidence. They must not be served to the human client. Journaling preserves evidence but does not implement restart/resume of a human session. Use a fresh configuration and journal for a new session.

## Browser launcher

`python/tools/native_human_ui_v2/launch.py` accepts `--template`, `--engine` and `--sessions`, plus optional `--human-seat`, `--initial-chooser`, `--seed` and `--card-text`. It verifies the package file and selected executable hashes, creates a fresh session directory, and reuses the existing local browser server. It does not attach to an existing V1 session.

The printed URL means the HTTP server started. The first successful `current` response establishes that the native service loaded and produced a view; URL publication alone does not establish model readiness. Native stopped/error responses must remain visible.

## Offline recorded replay

`human_match_v2 --prepare-replay AUTOMATIC.json --output NEW.json` projects a saved `mtg-kernel-bo3-collection-result/v1` through the same visibility and menu-ordering code. Input and serialized output are bounded at 256 MiB. The output and its `.stage` must be fresh; publication uses the existing durable create-only helper.

Preparation validates the collection schema, strict trajectory/package structure, behavior-selected indices and the typed trajectory SHA-256. It records the exact input-file SHA-256. It creates no engine, model or live action binding and does not independently verify producer/runtime provenance. Existing opt-in private label diagnostics may still write an error diagnostic.

The result schema is `mtg-kernel-recorded-human-replay/v2`, with `live_session: false`, ordered game/decision IDs, actor-relative projected decisions and their `selected_public_index`. It contains both seats' private views, so the combined file is backend-only. A replay coordinator selects only the configured human seat, compares the entire live projected state/menu, substitutes only the live `prompt_seq`, and submits the mapped public index. An offline prompt sequence is never a live authorization.

## Qualification limits

This document describes implementation behavior, not completed public qualification. The source tests use actual engine trajectories with explicit fixture policies; the fitted fixture includes synthetic teaching data. They do not establish raw-score equality, a non-Done model sideboard swap, sideboard benefit or playing strength. Corrected recorded-action replay and the real package/public interface comparison require their own actual results.

Human feedback matches, broader competitive evidence, additional meta coverage, learned opening/search integration and any training from human data remain separate work. Fable's focused V2 review failed with HTTP 429 before source reads. Independent source review is available, but it is not Fable endorsement; bounded authorized engineering continues with that limitation recorded in the Phase 1 review notes.
