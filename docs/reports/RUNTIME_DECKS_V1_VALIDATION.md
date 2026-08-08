# Runtime Decks V1 Engineering Validation

Date: 2026-07-17

Status: bounded engineering evidence, not a science-release certificate

Historical source binding: this record belongs to feature checkpoint `a86b15e21b2a37e94c4f1f8748e49769bc79e998`, merged unchanged as `9fbc68f26b9d1737f236ed137228e62e495ef545`. Hosted CI independently verified that source. Later commits retain this file as historical evidence; their changed card-database or environment-binary identities do not reproduce the artifact hashes below unless the record is explicitly refreshed. The local artifact roots used below are intentionally outside the repository and are not a published artifact release.

## Scope

This record validates the first frozen runtime-deck catalog and the exact boundaries it enables:

- `Burn` and `Rally` are build-time-validated canonical 60-card mainboards.
- The runner admits all four ordered physical-seat pairings.
- Training, greedy evaluation, and sampled evaluation admit exact mirrors only.
- Mixed-deck training and evaluation remain fail-closed until a separately versioned logical-deck seat-swap and seed contract exists.

The frozen deck hashes are:

| Deck | Runtime deck hash |
|---|---:|
| Burn | `0x5fdb7b92986b6fc1` |
| Rally | `0x0c9f01c2544412bf` |

The checkpoint-bound release environment binary used for every manual run below had SHA-256 `567b0acd81d3141645fd8c5a9a4f257ffa03ad4a0d49a94b8684cb6d543b3c80`.

## Local validation host

This is an engineering-host record, not the still-required designated science-hardware record.

- CPU: Intel Core i7-13700K, 16 cores / 24 logical processors
- Memory: 127.82 GiB
- OS: Microsoft Windows 11 Home, build 26200, x86-64
- Rust: `rustc 1.94.1 (e408947bf 2026-03-25)`, MSVC target, LLVM 21.1.8
- Python: CPython 3.13.14
- Torch: 2.13.0+cpu
- Torch execution: CPU, float32, deterministic algorithms, one intra-op thread, one inter-op thread
- Feature schema: `actor-relative-v5-python-4`
- Feature registry: `rust-observation-v5-action-v5-registry-4`
- Encoding contract: `actor-relative-node-graph-11`
- Model contract: `kernel-policy-value-net-7`
- Feature encoding digest: `ea74553564d370a18679ee5ad62fb359f7ad659b06d42144eb2f53393eee434e`
- Model contract fingerprint: `40e01e8b15ba92cacd922a0d7f12ed409fb35dac5bbd87f4fed153d8c9716110`

## Complete local gates

- `cargo fmt --all -- --check`: pass
- release Clippy over the workspace, all targets, and all features with warnings denied: pass
- release Rust workspace tests over all targets and features: pass; one oracle-material test intentionally ignored
- canonical manifest regeneration check: pass
- Python suite against the real release Rust environment: 279 tests in 958.507 seconds, pass with 5 intentional skips
- `git diff --check`: pass

The Python suite's duration is validation overhead from subprocess, corruption, recovery, and artifact-boundary tests. It is not trainer throughput.

## Four-pair runner rerun

Each row is one uniform-policy episode with base seed 71501 and natural-terminal caps of 5,000 physical decisions and 640,000 policy steps. The complete command set was run into two fresh roots. Both `run.json` and `episodes.jsonl` were byte-identical between roots for every pairing.

| Ordered decks | Terminal | Policy steps | Physical decisions | `run.json` SHA-256 | `episodes.jsonl` SHA-256 |
|---|---|---:|---:|---|---|
| Burn / Burn | p1 win | 183 | 182 | `aa70fc354711976d314ab9e81d49a25dde0c54956c7c1289d2cf6a23d4afb764` | `acff770126d2c613e9e4ccca2c5a9d9676f94a206b7d8a38c3a5a1ae3a1619e1` |
| Burn / Rally | p1 win | 153 | 142 | `3315a05f6b96e1c163caacc07a374d1da9fd57e8a2911406df87fc0f8ca8c09d` | `3f32f3a2189408c7eccf30cc03bf6b6c7c4a88a4380eae56542bf38c886a9a75` |
| Rally / Burn | p0 win | 165 | 148 | `c99002a19bba23a29a7c358e014d09be1777e8a12506c350968993822ab4e455` | `7d18ac2004c59ee96429c6c74edbc89b936c68e5f0f02870087c19e2cc581319` |
| Rally / Rally | p0 win | 249 | 236 | `6344ac96a29ab2d5c6f7f260a758272c1cbabf6c3e73ebb7b15a52931bc1dfba` | `9c5015c47cc82f30868712009fb1c1fad7a6822b25b73cfb80af9a70eaf5de1c` |

## Rally mirror trainer rerun

Configuration: base seed 71501, one update, two episodes, learning rate 0.001, value coefficient 0.5, the same natural-terminal caps, and `Rally` / `Rally`.

Two independent fresh stores completed in 7.234 and 7.279 seconds. They produced the same:

- run digest: `632e3f24f3d0b227248aaa32c1dd115b43324d94e45519afb0765139ff764daa`
- update-one head: `f5c402c2e66a549d1536723c5186569a8386aa442a8ae78d9d53dd8db58b19dc`
- logical state SHA-256: `9a9b2534d5410f621feab25539f5c65fde2a947057a9677ae74377cc9a5f8a26`
- next episode: 2
- optimizer step count: 1

Every authoritative file checked below was byte-identical across the two stores:

| File | SHA-256 |
|---|---|
| `run.json` | `632e3f24f3d0b227248aaa32c1dd115b43324d94e45519afb0765139ff764daa` |
| `updates/update-00000000.json` | `31665f588df1e5d2d2064a9e5d5db0ed265d99e228c9c5e18946cd02c11d34dc` |
| `updates/update-00000001.json` | `b7ca39055a5f6c7514980abfa7f4cad4bd042d9511e8f347cab1b6334c85016a` |
| `checkpoints/update-00000000.json` | `9bb6f11f6ef79cb2280a6b05867cddfc20c1d7a2eb0c8452ced1eeb9ea384b46` |
| `checkpoints/update-00000000.pt` | `342fbfec224cd67e65f5fa1967d2029a5ea465ccbfad59a7f72171640010f03f` |
| `checkpoints/update-00000001.json` | `0b26e1f85bb472aaa8f08ea3cf7ced05725de47faf412f81c128871e13582c36` |
| `checkpoints/update-00000001.pt` | `dd7bd2e25c4c10211a00e7dcd08b90e971c5d2c94c456c363079a3c6e95091f8` |
| `latest.json` | `847499609d6f116185103aa92869a2e0b59847d804bb43594a80f458709ed850` |

This byte identity is scoped to the recorded runtime tuple and host. It is not a cross-platform Torch claim.

## Rally mirror evaluator rerun

Both lanes evaluated the exact update-one candidate above against update zero with one seat-balanced pair, base seed 71501, 1,000 bootstrap replicates, and the same natural-terminal caps. Each lane was repeated into a second fresh root; all three authoritative files were byte-identical.

| Lane | Baseline head | Estimate | Total half-points | `run.json` SHA-256 | `games.jsonl` SHA-256 | `pairs.jsonl` SHA-256 |
|---|---|---:|---:|---|---|---|
| Greedy | `85afbe526214a941e3c1f696e8bc1a63adad87c613f8649ad0c10a0e8b04cd98` | 0.0 | 0 | `ecfe889a8990ba1d4243beebb49d0090e8ff5a5a292da68de884cf16003a8566` | `3c143439d75778f182a0cd825b3466479031d96c328b1b728cb67a132f0e2f99` | `56b4ba75201ac3f049a23ab1bcdbbf78ea8e57faaeff34b066d225929d9c9c0b` |
| Sampled V3 | `85afbe526214a941e3c1f696e8bc1a63adad87c613f8649ad0c10a0e8b04cd98` | 0.5 | 2 | `e9da1b49820cd4b0aaf3f440c7fd45d70c7edac55306e21988d70fa47ccc2a10` | `714d45b5beea2fedf88f4152d42875c12f1e91a87ed3748f20c86ee1b14d6d28` | `fd3232fc4e1e948969d7020cb643bf9fdb0d1aeb946311a7adf91d8adeca05fa` |

The greedy run took 20.581 seconds and the sampled run took 8.092 seconds. These two-game results prove wiring and reproducibility only; they are not policy-quality estimates. The durations are not a selector-performance comparison because each lane followed different trajectories and only one pair was observed.

## Single-run end-to-end trainer timing

One fresh 20-episode update was timed for each exact mirror. The timing wraps the complete CLI call and therefore includes process startup, Rust environment steps, Python feature construction, CPU Torch inference, one optimizer update, checkpoint serialization, and artifact publication. It uses one environment process and one Torch CPU thread; it is not a parallel-capacity benchmark.

These are single observed rates from fresh CLI invocations, including fixed startup and update-publication costs. No repeated steady-state throughput estimate or uncertainty interval is claimed.

| Deck mirror | Wall time | Episodes/s | Policy steps | Policy steps/s | Physical decisions | Physical decisions/s |
|---|---:|---:|---:|---:|---:|---:|
| Burn | 23.401 s | 0.8547 | 3,009 | 128.6 | 2,965 | 126.7 |
| Rally | 44.342 s | 0.4510 | 5,347 | 120.6 | 4,758 | 107.3 |

These measurements establish no end-to-end speedup over XMage. Engine-only microbenchmarks are much faster under inference-free random-policy workloads, but they do not predict trainer throughput. The observed gap shows that overhead outside raw simulation is material; this benchmark does not attribute it among JSONL IPC, Python feature construction, Torch inference, optimization, and artifact persistence. A fair XMage comparison requires the same workload, policies, model, hardware, concurrency, warmup, and measurement boundary. The designated-hardware all-deck capacity gate remains open.

## Reproduction command shape

After building the release environment and activating the locked Python environment, the material commands were:

```text
mtg-kernel-rl run --env-bin ENV --out-dir FRESH --episodes 1 --base-seed 71501 --max-physical-decisions 5000 --max-policy-steps 640000 --p0 uniform --p1 uniform --deck-ids P0_DECK P1_DECK

mtg-kernel-rl train --env-bin ENV --out-dir FRESH --base-seed 71501 --until-update 1 --batch-episodes 2 --learning-rate 0.001 --value-coef 0.5 --max-physical-decisions 5000 --max-policy-steps 640000 --deck-ids Rally Rally

mtg-kernel-rl evaluate --training-store STORE --expected-candidate-head f5c402c2e66a549d1536723c5186569a8386aa442a8ae78d9d53dd8db58b19dc --env-bin ENV --out-dir FRESH --pairs 1 --base-seed 71501 --bootstrap-replicates 1000 --max-physical-decisions 5000 --max-policy-steps 640000 --timeout-ms 10000 --deck-ids Rally Rally

mtg-kernel-rl evaluate-sampled --training-store STORE --expected-candidate-head f5c402c2e66a549d1536723c5186569a8386aa442a8ae78d9d53dd8db58b19dc --env-bin ENV --out-dir FRESH --pairs 1 --base-seed 71501 --bootstrap-replicates 1000 --max-physical-decisions 5000 --max-policy-steps 640000 --timeout-ms 10000 --deck-ids Rally Rally
```

Every rerun must use a new output root. The trainer and evaluators fail closed on nonempty output roots, runtime-tuple drift, binary-hash drift, deck-identity drift, and artifact-chain drift.

## Explicit limitations

- One runner episode per ordered pairing is not the 32-pair mirror gate or 16-pair cross-deck gate.
- One two-episode training update is not a learning or strength result.
- One evaluator pair is not a statistical comparison.
- Local external artifacts are not a published clean-clone reproduction bundle.
- Exact Torch/checkpoint bytes are only claimed for the repeated recorded host/runtime tuple.
- Seven canonical decks and the full sampled-primary 9x9 protocol remain incomplete.
- A designated science-hardware record and clean-clone artifact reproduction remain open.
