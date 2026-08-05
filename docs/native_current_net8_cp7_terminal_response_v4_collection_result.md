# Current Net8 CP7 terminal response v4 collection result

Status: collection and merge complete. Training held for exact-pin review.

The fixed panel completed all 256 matched pairs and 512 natural games at base
seed `1930001`. Eight workers ran one 32-pair task each. Total wall time was
567.36 seconds, or 9.46 minutes, for `0.902428` games/s.

Resource use remained inside the declared envelope:

- average CPU: `34.51%`
- maximum process-tree RSS: `12.44 GB`
- minimum available system memory: `71.88 GB`
- maximum system-memory use: `47.6%`
- GPU 1 utilization and memory use: zero

The strict merge bound all eight validated shards to the formal collection
report and produced:

- corpus path: `D:\mtg-kernel-current-net8-cp7-terminal-response-v4\development\collection-base1930001-attempt-01\merged\corpus.jsonl`
- corpus SHA-256: `5b3ac6818c79be9ba0ff6f31e6fef897fa78cc1796b0013ffbcc33365338ed72`
- corpus bytes: `399579668`
- pairs and episodes: `256` and `512`
- decision rows: `21112`
- physical groups: `18440`
- terminal-return counts in loss, draw, win order: `[312, 0, 200]`
- collection manifest SHA-256: `325147a31998c1aa543a0263401a254514255b3d857f8494f5672d44342865be`
- collection report SHA-256: `562d5757f5becaecf1dc7ad12ab3620f9780638356cf46e7a327e6916bc9c512`
- merge report SHA-256: `e4e8cd0036cf252df1e74e1c5a0f6362dadfb8344890ead6102c7a8f48ddded4`

The Rust loader independently reproduced the corpus SHA, panel, decision-row
count, physical-group count, and terminal-return counts. The terminal counts
are recorded only as corpus identity and are excluded from arm selection.
Collection completion is not playing-strength or promotion evidence. CP7 is
part of this training distribution.
