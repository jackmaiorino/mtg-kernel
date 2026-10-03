# Natural combat recorder replay: engineering PASS

Fixed consumed parent case cell00-r1-s1-g115 completed three physical games and 373 recorded decisions. Complete collection objects were identical with the recorder off/on. Two recorder-on executions were byte-identical (SHA-256 38e26239924e350a802d920900f3888dd68d3ee8b2f839195132b2e675458c7c). Three roots, representing two physical combat decisions, were prepared and committed; 365 simulated transitions, no audit budget exhaustion. This is recorder correctness on one path, not a playing-strength or M1 pass.

| Root, game 2 | Physical decision | Chosen action | Bounds for action 0 / action 1 |
| --- | --- | --- | --- |
| Step 182 | 175, substep 0/2 | 1 | [-1,+1] / [-1,+1] |
| Step 183 | 175, substep 1/2 | 1 | [-1,+1] / [-1,+1] |
| Step 207 | 192, substep 0/1 | 1 | [-1,-1] / [-1,-1] |

No strict action dominance and no proven natural blunder were found. The first two roots remain unresolved; both actions at the third root have equal forced-loss bounds. Do not convert unresolved alternatives to losses, count two flattened substeps as independent examples, or train on this output. Tree-node reason counts: 243 opponent-choice, 4 own-choice, 4 natural-terminal, 123 unsupported-action, 12 node-limit, 55 information-boundary, 59 combat-window-finished. These are correlated tree nodes, not a prevalence sample. Root status counts also include 135 non-combat menus and 16 nonempty-opponent-hand abstentions.

Guarded executable build: source 5e49f1a3, 155.154 seconds, binary SHA-256 0e52406315f9efc062f436af8fba0c0ab50ef7e5c9471dc1bf1aede12080a516. Native elapsed seconds including subprocess startup: package .3455, off .8486, on .8574, repeat .8712. These single-case timings are not throughput qualification or a search-speed claim. E:/mtg-meta-recovery-20260921/combat-recorder-build-001 contains the build/toolchain receipts; combat-recorder-natural-002 contains complete outputs and checks. No GPU training, substantial census, new candidate or paid allocation.

First preparation attempt combat-recorder-natural-001 failed before any game: the launcher passed its toolchain report instead of the compiled rust-toolchain.toml pin. The native check rejected it with `runtime toolchain/registry pin differs from compiled bytes`. Version check-combat-natural-replay-v2.py supplies the required TOML; no source, binary, input model, seed, or guard changed. Both launchers and failure evidence are preserved. The selected case and checks were declared before execution in collab/lit/20260921-combat-recorder-correctness.md.

Monitor guidance arrived during completion. Synthetic-template teaching and prevention remain closed. Next priority is the requested normalization audit, not a new teacher or expanded combat run. All original retained128 outcomes remain consumed and unchanged. Fable independent review remains unavailable under HTTP429; no ADVANCE is claimed.
