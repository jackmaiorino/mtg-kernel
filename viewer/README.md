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

Pass `--lane YOUR_LANE --replay-steps 1 15 100` to both guarded qualification and capture commands. The native config accepts `search_replay_steps` with up to three distinct steps; it is empty by default. Use actual featured-seat decisions and verify that all three were recorded before publication. Runtime, input and allocation pins plus replay settings must match qualification. A failed projection or byte-bound overrun makes capture incomplete without changing gameplay.

The search transport has its own event/simulation timeline, play/pause and speeds. Selecting an explored move follows its recorded simulation. Scores retain the search actor and root perspective; search priors are the quantized priors used by search, distinct from Hamilton sampling masses. Leaf estimates clip at ±9,000, natural wins/losses use ±10,000 and draws use zero. Links can add `&searchEvent=N` to an existing game/decision fragment. Search replay is unavailable when it was not recorded.

Both sampled hands are excluded from public hypothetical positions. Interior action descriptions use public references only; captured hidden menus, node keys, RNG details and encoded tensors remain private. Do not reconstruct or invent simulation history from final root statistics. Annotate observed paths and results after inspecting the new capture. The three guided moments remain authored commentary.

## Publish another demo

Place complete exported games and their `assets/` directory under one external demo root. Add `manifest.json` with `games: [{id, file, sha256}]`. Pack it using `python/tools/viewer_demo_pack_v1.py pack ROOT viewer-demo.zip`. Upload the archive as a versioned GitHub release asset and update `demo-release.json` with its tag and SHA-256.

The Viewer workflow verifies the frontend, exporter and browser checks, downloads the pinned archive, checks every recording hash, and publishes the static build on GitHub Pages. Whole recordings and art remain release artifacts rather than source commits. Links use `#game=ID&decision=FRAME_ID`.

Before release, inspect the full recorded game, stack/combat/search moments, 1440/1920/320 layouts, reduced motion and keyboard behavior. Record real seek/frame timings and any misses. Future live streams and branch exploration must feed the same validated recording/playback boundary rather than introducing rules in the renderer.
