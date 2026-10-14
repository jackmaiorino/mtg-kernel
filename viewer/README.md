# Decision Studio

A static, fixed-perspective game viewer. It displays captured engine/model outputs; opening a recording performs no inference. The application uses React, TypeScript, Motion, and HTML/SVG. Game state and animation state remain separate so seeking cannot change replay behavior.

## Development

Use Node 24.14.0 and npm 11.18.0. From this directory:

```sh
npm ci
npm run dev
npm test
npm run build
npx playwright install chromium
npm run test:browser
```

Without a materialized demo pack, import a viewer JSON file. Local imports are validated in a browser worker and never uploaded. Imported JSON uses text cards because it does not grant access to companion assets. Curated hosted packs include locally served card art. Space toggles playback; arrow keys step decisions. Seeking pauses and cancels pending motion.

## Recording contract

`mtg-kernel-viewer-recording/v1` contains full before/after snapshots, a fixed seat, stable object identifiers, engine-ordered legal actions, executed selections, optional policy/search diagnostics, and authored bookmarks. `src/recording.ts` is its executable public contract. Unknown fields and versions are rejected, including extra private fields. Preserve the historical fixture in compatibility tests when adding adapters.

Policy masses are exact decimal integer strings with denominator 2^64, copied from the executed Hamilton sampler. Policy favorite, policy sample, and played move are separate. Search visits are exploration counts, not probabilities. Root tables retain their digest, which commits to indices, visits, signed value sums/means, selected action and work counts. The worker verifies this digest before rendering. V4 search integer values use scale 9,000 and root-actor perspective.

Public packs reveal the featured seat's hand and diagnostics only. Opponent menus and diagnostics are omitted; opponent frames show an opaque executed action and observed public transition. Historical actor-only traces retain only the chosen observer's recorded decisions and are labeled `decision-snapshots`. A finished trace file alone does not establish a completed game: publication requires a recorded natural terminal.

## Bounded native capture

Build the feature-gated `viewer_capture_v1` binary with the pinned Rust toolchain, through the existing resource launcher. It loads a pinned `ExpandedModelSourceV1`, uses one V4 checkpoint for both seats, and runs T512 search with a depth cap of 64 and no stability halves. It does not modify checkpoint, primary training, gameplay trace or search outcome schemas. Recording is disabled unless the configuration explicitly sets `record: true`. Operational recorder failures disable diagnostics and report incomplete capture without changing game decisions. Reused output paths remain configuration errors.

Before acquisition, create a read-only three-host inventory and register the owned artifact root under the existing artifact law. Run both launcher modes inside `python/tools/host_slots_v1.py run --lane cinematic-viewer-232 --cores 2 -- ...`:

```sh
python python/tools/viewer_capture_launcher_v1.py qualify --binary ABS_EXE --source ABS_SOURCE --root ABS_CAPTURE_ROOT --inventory ABS_INVENTORY
python python/tools/viewer_capture_launcher_v1.py capture --binary ABS_EXE --source ABS_SOURCE --root ABS_CAPTURE_ROOT --inventory ABS_INVENTORY
```

Qualification compares a fixed-seed recorder off/on case and representative serial/two-worker completed-work throughput. Capture checks compatible pins and allocation, the live claim, free space, and the 90-minute/three-game/2-GiB bounds. It stops after the first complete game. Keep partial/failed attempts. These are illustrative development games, never ratings or strength evidence.

## Export and annotate

```sh
python python/tools/viewer_export_v1.py CAPTURE.jsonl DEMO/game.json --id frozen-selfplay --title "A frozen model, two decks" --bookmarks BOOKMARKS.json --art --cache ABS_CACHE
```

Bookmark objects have `frame`, `title`, `situation`, `alternatives`, and `consequence`. Reference actual frame IDs, explain only observed facts, and identify commentary as authored. Use exactly three for a featured release. Exporter art resolution is cached and occurs before publication; playback calls no card-data API. Captured engine rules text takes precedence over supplemental card metadata.

The exporter also accepts validated historical gameplay traces, with an explicit card ID-to-name mapping. Opponent records are excluded from these actor-only exports. Seeds, library order, RNG state, tensors and private journals are never exported.

## Record and inspect search replay

The viewer also accepts `mtg-kernel-viewer-recording/v2`, which adds optional `mtg-kernel-search-replay/v1` data to a decision's search diagnostics. V1 fixtures and policy-only recordings remain supported. Replay data joins the complete episode/step/physical/substep/actor key, preserves node and action indices, and reconciles every simulated edge and root sum against the recorded table and its original digest. Hypothetical snapshots never become gameplay frames.

Pass `--lane YOUR_LANE --replay-steps 1 15 44` to both guarded qualification and capture commands. The native config accepts `search_replay_steps` with up to three distinct steps; it is empty by default. Use actual featured-seat decisions and verify that all three were recorded before publication. Runtime, input and allocation pins plus replay settings must match qualification. A failed projection or byte-bound overrun makes capture incomplete without changing gameplay. Snapshots are deduplicated in the raw capture; the exporter also supports the initial inline layout. This path uses PUCT root allocation, prior-weighted interior exploration, mean backup and legacy world sampling.

For a recording repair, retain the previous acquisition receipt and use `--previous-acquisition ABS_RECEIPT --max-games REMAINING --seed-base SEED`. The launcher accounts for previous games, wall time and raw bytes, preserves the checkpoint, and verifies requested development replay coverage before dispatch. `--qualification-steps 16` can include a concrete failing decision within a bounded correctness case. Keep all earlier captures and qualification evidence.

The search transport has its own event/simulation timeline, play/pause and speeds. Selecting a root move inspects its recorded initial evaluation without changing the timeline; selecting an action in the simulation path seeks that recorded event. Scores retain the search actor and root perspective; search priors are the quantized priors used by search, distinct from Hamilton sampling masses. Leaf estimates clip at ±9,000, natural wins/losses use ±10,000 and draws use zero. Links can add `&searchEvent=N` to an existing game/decision fragment. Search replay is unavailable when it was not recorded.

Search replay v2 adds `leafNode` identities for the growing graph. V4 allocates one sequential node per recorded expansion; export validates the allocation against parent/action connections, complete topology and native node visits. The graph retains discovered branches, shared positions and sampled outcomes, highlights the current path, and marks evaluated frontier positions. Evaluation-only coverage, terminal and cutoff states have no persistent leaf node. Counts update at backup. Select a position to inspect its public snapshot and recorded estimate, or use zoom/pan, fit, path focus and the complete-graph shortcut. Keyboard arrows traverse graph positions and Enter selects them. Existing replay v1 imports show only the topology their recorded paths establish and label unavailable leaf identities. Graph depth counts engine decisions, including targets and costs, rather than complete turns.

All legal root choices stay connected to the current position from the first event. Position cards identify decision states and their actors; action blocks name moves; evaluation cards retain one-step evaluations without a stored child. Status distinguishes untried choices, evaluated but unexpanded choices, and branches with recorded child positions. Counts and previews use only the current replay prefix; seeking backward removes later evidence. Move indices remain the original engine indices. Older traces with a recorded expansion but no child identity label that information unavailable.

The graph now uses readable position and action cards at every explored level. Scroll/drag to follow the tree, or fit the overview and focus the current path. Actions show the acting seat, original action index and prefix-only visits. Select an action to compare its native child positions without seeking. Unevaluated future evidence stays hidden; multiple evaluated frontiers under one action collapse into an inspectable group, and a position that gains a recorded continuation unfolds automatically. Grouping never merges engine identities or implies a probability. A single action can lead to several sampled positions, even with identical public boards.

Opponent cards show the response actually selected by a hypothetical search simulation. A frontier with no selected response says “Evaluated before a response.” The current Burn/Rally traces stop on their opponent frontiers and contain no selected opponent actions; the 96 d15 frontiers under Pass are evaluated situations, not 96 opponent choices. Sampled hands remain excluded, so a pass cannot establish that a particular spell was absent or available. Unknown action names remain unavailable. These are simulated responses, not moves taken in the actual game.

Both sampled hands are excluded from public hypothetical positions. Interior action descriptions use public references only; captured hidden menus, node keys, RNG details and encoded tensors remain private. Do not reconstruct or invent simulation history from final root statistics. Annotate observed paths and results after inspecting the new capture. The three guided moments remain authored commentary.

## Publish another demo

Recording v4 adds `mtg-kernel-uct-search-replay/v1` for fresh persistent UCT captures. Build `viewer_spy_capture_v1` with the optional `viewer-recording-v1` feature. Its pinned config contains absolute-path/SHA-256 `spec`, `focal`, and `opponent` inputs, development source `id`, scheduled `search_start`, fresh `output` and `primary` paths, `record`, and `max_steps`. The native binary generates the opening from the frozen policies and executes the supported R guide. No saved action stream or combo script is accepted. Search settings remain FPU1.5/depth32/128000 initial, 64000 refresh, 512000 total transitions. The first three searches retain all attempted simulations, including rejected or limited attempts.

Run the guarded `viewer_spy_capture_launcher_v1.py qualify` and then `capture` modes inside `host_slots_v1.py`, using lane `spy-full-search-trace-239`, two admitted cores, `--binary`, `--template`, `--root`, and current `--inventory`. Qualification checks off/on/failed-recorder byte-identical primary outputs, the actual final core hash against replay, and serial/two-worker completed short-case throughput including trace serialization. Capture refuses incompatible evidence and enforces three games, 90 minutes, 2 GiB raw output, 8 GiB working storage and a 60 GiB free reserve. It stops at the first complete game containing a natural winning combo rollout. Retain interrupted attempts and original inputs.

The UCT adapter verifies the original native tree hash and reconciles every node's visits/wins with natural backups. Canonical choices join their actual live action indices to the complete rollout decision key. Both hypothetical hands are removed before registering metadata. Public v4 records omit private tree keys and carry a separate digest for their allowlisted node tables. The worker verifies these bindings before rendering. Policy distributions remain unavailable when this path did not record them.

For actual-game lifecycle markers, run the same native binary with `bind_actions: {path, sha256}` pointing to the original primary JSONL, and a fresh `output` path. This mode replays the recorded action stream without loading a model, sampling or searching. It computes native legacy history keys at real guide choices, including the previous tree's key before a refresh, and requires the recorded final core hash and natural terminal to match. Pin this metadata executable separately from the original capture executable. Export with `--execution-bindings BINDINGS --primary ORIGINAL_PRIMARY`. The adapter verifies primary identity, full episode/step/physical/substep/actor keys, original live moves, canonical selected edges, completed-rollout support and refresh evidence before exposing node IDs. Raw native keys remain private.

Optional v4 `executionBinding` provenance and per-frame `searchLifecycle` distinguish the scheduled initial search, a refresh, tree reuse and policy fallback. A missing position, insufficient support or depth limit explains a refresh; remaining transition budget is recorded separately. In the viewer, blue marks the actual guide route and green follows a simulated search path. A continuation opens the completed source tree at its exact recorded node. Discovery mode retains its original prefix, and locating the actual game does not change the real decision or search timeline. Forced moves and opponent steps between guide choices are labeled as having no separate tree node. Legacy recordings without these bindings do not infer them from matching public boards.

An actual-route link adds `searchView=actual` to the fragment. Search discovery links continue to use `searchEvent`; switching the graph view preserves the real decision and selected event.

UCT payloads can use `storage: object-pool/v1`: every snapshot still contains complete zones and counts, with zone entries referencing shared immutable card states in `objects`. This lossless storage keeps full traces within the 64MiB import bound. The worker validates and resolves references before rendering, rejecting unused or broken pool entries and more than one million decoded object references per search. Historical inline snapshots remain accepted. Seeking requires no rules replay.

The growing graph shows focal non-forced choice nodes. Opponent, forced, and later policy moves occur between nodes; the rollout inspector retains every applied move. Natural wins, draws, losses, rejected samples and truncated attempts remain distinct. Proposed nodes with no natural backup are shown without visits. Combo shortcuts require ordered self-target, self-mill resolution, Giant reanimation choice and resolution, followed by a natural focal win. That establishes an observed winning rollout in the sampled world, not representative playing strength or proof of perfect lookahead. Sparse rollout snapshots are identified explicitly; playback animates no unsupported transitions.

The full-trace view is a single-screen console on displays at least 1100×680: the transport sits above the panels, and its event slider is laid over an outcome strip with one column per attempt (natural wins, combo wins, stopped attempts and the running win count). The first-move list ranks root moves by final tries and shows live wins/tries. The tree sizes nodes by backed-up visits and tints nodes with recorded wins. Hovering a node names the move that reaches it; clicking picks the nearest node, highlights its route from the root, opens a details card beside it (choices from the root, tries, wins, first attempt, with a button to watch that attempt) and shows its board below the tree. The attempt panel walks through tree selection, rollout (with ordered combo stages), result and backup. Smaller screens stack the panels and keep the transport sticky.

Recording v3 also supports verified reconstructions of retained real-game actions. Build the opt-in `viewer_replay_v1` binary with `viewer-recording-v1` and run it under the supported host-slot launcher with a pinned configuration containing `spec`, `bank`, `witness` (each absolute path and SHA-256), source `id`, new `output`, both checkpoint hashes, original source commit and original runtime hash. It checks the saved anchor, full decision keys, legal actions, canonical root order and natural terminal. It performs no inference, sampling or search. Export its output using `viewer_export_v1.py` as usual.

The Spy example reconstructs the selected r1-o4-t0-s0 qualification game, plain-policy opening plus R guide, rather than acquiring another self-play game. Recorded `decisionSource` entries bind a search-controlled move to its source root, native tree digest and retained artifact hash. Only two root tables were saved; descendant statistics, policy masses and individual simulations remain unavailable. This historical persistent UCT variant uses FPU1.5 and selects the greatest empirical rollout win mean among edges with at least eight backups. It is distinct from the V4 T512 examples. Root wins are represented exactly in units of 1,000,000 and displayed as wins/completed rollouts. Its table digest is generated during export; the original tree digest remains a separate provenance identity. The chosen qualification anecdote does not establish representative behavior or learned combo ability. Legacy v1/v2 fixtures remain supported.

Place complete exported games and their `assets/` directory under one external demo root. Add `manifest.json` with `games: [{id, file, sha256}]`. Pack it using `python/tools/viewer_demo_pack_v1.py pack ROOT viewer-demo.zip`. Upload the archive as a versioned GitHub release asset and update `demo-release.json` with its tag and SHA-256.

The Viewer workflow verifies the frontend, exporter and browser checks, downloads the pinned archive, checks every recording hash, and publishes the static build on GitHub Pages. Whole recordings and art remain release artifacts rather than source commits. Links use `#game=ID&decision=FRAME_ID`.

Before release, inspect the full recorded game, stack/combat/search moments, 1440/1920/320 layouts, reduced motion and keyboard behavior. Record real seek/frame timings and any misses. Future live streams and branch exploration must feed the same validated recording/playback boundary rather than introducing rules in the renderer.
