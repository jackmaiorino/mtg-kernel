# Learned sideboarding engineering 002, September 12, 2026

Draft evidence snapshot. The completed runs below are immutable engineering results. The separate Affinity/Elves case at match seed `2026091231` remains under repair and is not a completed result. Later fixes and reruns require their own source and output pins.

The rich V6 / flat V3 successor represents explicit Escape object-cost prefixes, chooser-only library-search candidates without hidden positions, and public historical sources. An Initiative transfer can retain Avenging Hunter's live controller while its captured ability belongs to the player taking the Initiative. V3 represents both exact public views. The original V2 feature path remains available. No hidden game state is supplied to the model or reproduced in this note.

## Completed execution

Evidence root: `E:/mtg-kernel-learned-sideboarding-evidence/engineering-002`. Config root: `C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/engineering-002`.

| Completed run directory | Matches / physical games | Actual execution |
| --- | ---: | --- |
| `learned-five-deck-v3-002` | 10 / 23 | Distinct learned policy instances in both seats; five unordered cross-deck pairings in both directions; 26 acting-seat sideboard decisions, 19 with exchanges, 73 cards moved out and 73 moved in |
| `diagnose-affinity-002-v3-001` | 1 / 3 | Affinity mirror, seed `2026091214`, learned versus keep; four acting-seat sideboard decisions, all `Done`; the earlier Map-source case completed |
| `legacy-v2-canary-001` | 1 / 2 | Rally/Burn, seed `2026091251`, original V2 input path; two sideboard decisions and seven exchanges; exact prior match bytes reproduced |

The five-deck batch used seeds `2026091260` through `2026091269`, in this order:

| Match files | Ordered pairings | Physical games |
| --- | --- | --- |
| `match-000000.json`, `match-000001.json` | Affinity/Rally, Rally/Affinity | 2, 2 |
| `match-000002.json`, `match-000003.json` | Elves/Burn, Burn/Elves | 2, 3 |
| `match-000004.json`, `match-000005.json` | Terror/Rally, Rally/Terror | 2, 2 |
| `match-000006.json`, `match-000007.json` | Affinity/Elves, Elves/Affinity | 3, 3 |
| `match-000008.json`, `match-000009.json` | Terror/Burn, Burn/Terror | 2, 2 |

This is five of the ten possible unordered cross-deck pairings, with different engineering seeds in the two directions. It is not a full matchup matrix or a matched-seed strength comparison. Every completion record declares `no_training_performed: true`, `strength_claim: false`, and zero new imitation examples.

The learned head in the five-deck batch and V2 canary is the existing Rally/Burn imitation checkpoint, SHA-256 `914eb1d89bbc10b446b06d0f6de82cb1eea82ba37fb57302ae8bf8d417dfaa71`. The Affinity replay instead uses the earlier two-deck checkpoint, `2151fe0a2b705a5856827ad121b822c3c70081e6f5f8d5515179f5313f514a2a`. These heads were fitted to hand-authored warm-start targets in engineering 001. Completed execution and legal exchanges do not establish useful sideboarding, generalization, a learned value estimate, a trained brewing model, or improved win rate. No GPU training, paid run, promotion, or CP7-based selection occurred in these runs.

## Exact provenance and output identities

All three runs record CPU execution and `gpu_ordinal: null`. Each directory contains `run-start.json`, `config.json`, `inputs.json`, `play-transfer.json`, immutable per-match records, and `completion.json`. Start receipts record the compiled binary, source identities, declared Rust `1.94.1` toolchain, seeds, checkpoint paths, and input hashes. The executable path is reusable; its recorded hash is the identity of that run's executable.

| Run | Recorded clean source commit | Binary SHA-256 |
| --- | --- | --- |
| Five-deck learned batch | `5ca1c733f957c0143fb12d247dd52a0b53b20c85` | `81e9f91710e37173ae8c01bfcd4f1171f24f218fb011568b30f3dd7f27a4a5d2` |
| Affinity replay and V2 canary | `436c62b22cc757a28afbdfeb97b9c0990efac538` | `c87b29edab4533623a0c708969c1702b723508f6af072175e975c9904b007164` |

All runs import the same frozen play weights, SHA-256 `154a985811a0c923108f9770c6a3cc67f482c9b1f85976b30b86fed179644a17`, and immutable embedding table `063c25fd053a927aba7a8265ca06b446f7bcf6a3369de249b16762cce15aedd9`. The real source export is generation 2304 from commit `18f4ca51419d215370d539cef726edd7f8e7ae0b`, trained on Rally/Rally. The explicit registry transfer checks the 162 source entries against the 184-entry destination, allowing deck-membership changes and appended entries only. Appended embedding rows are unchanged exported parameters, not evidence of training exposure.

The five-deck run's V3 feature contract is `a67ef398e332d9d9f83f4c45618b6696a54aa963b4dbcfa5177f9ff5ea3106a7`, encoding digest `303c89c909073ec3476af7332410eabd1369ddcb616819e588cfd61486ed8ef6`. The earlier Affinity replay has encoding digest `6884a1900674b0f6ce125b3bda6bc8bfe02dc5775c3cdbd38f80ed336f7448fe`. They must not be relabeled as one compiled feature identity. Their `play-transfer.json` receipts preserve the original source feature identity and explicit successor semantics.

| Run | `completion.json` SHA-256 | Ordered match-index SHA-256 |
| --- | --- | --- |
| `learned-five-deck-v3-002` | `330fd8443a660d1e233b04152661b4d0c8970fb6efb87f1ba8e874a2c50afa55` | `eb6395182690e1ae3ee42d03a47462423400e86b03ede79ad122fb133971906a` |
| `diagnose-affinity-002-v3-001` | `90c53776c11abc156fb24c4a5e3a841e3556adfaae440e073c150b19cbaf3006` | `1d4b9d5b3f3954b78977be47168bd909f88a3bc76185ebec6c82b79f665362de` |
| `legacy-v2-canary-001` | `0658719d75849ce264c328c37232cfbbd801ca035b61646df0da3540afa8c5db` | `ef86ee7839d49fec2ecadb3d861b890e087b6d7f5e0076d1d8b6670ad7be6eff` |

The match-index digest above hashes UTF-8 lines, sorted by match filename, each exactly `filename`, one ASCII space, the file's lowercase SHA-256, and LF. It indexes completed match files only; the final manifest should also enumerate their individual byte sizes and hashes.

The V2 canary's `match-000000.json` is 18,364 bytes with SHA-256 `09f71de98242cb0fe719b166bc9f6d2136b3c6b85bd1c803c868cb7d3598fb71`. This exactly matches engineering 001's `learned-rally-burn-001/match-000001.json` and `learned-rally-burn-replay-001/match-000000.json`.

## Recorded checks

These logs record distinct scopes and partly overlapping tests; their counts must not be added into a unique-test total. They precede the outstanding goad action-choice repair and do not certify that future patch.

| Evidence file under the engineering 002 root | Result |
| --- | --- |
| `focused-successor-002.log` | 19 passed, one explicit fixture emitter ignored; includes actual Initiative stack and pending-effect contexts |
| `compatibility-and-successor-001.log` | 50 passed, two explicitly ignored; includes the legacy V2 all-thirteen-tensor canary |
| `cli-tests-001.log` | Five passed |
| `old-net8-forward-goldens-001.log` | Original Net8 forward golden test passed |
| `python-successor-tests-002.log` | 15 passed |
| `native-v3-fixtures-002.log`, `parity-v3-003.log` | Nine actual fixtures, all 13 tensors bit-identical between native V3 and Python V6; 14,857 scalars |

Parity covers ordinary opening, temporary search, hidden search-card renumbering, Map Explore, Initiative transfer at stack and pending-effect boundaries, and Escape selection prefixes 0 through 2. Completed Escape prefix 3 has no actor decision; a separate test checks its rich state without fabricating an action. The parity receipt SHA-256 is `f7d374e05e8d49de96a67ee85e408fd80a065d3a72a77ae5f97e9cd347becc68`.

## Still open in this snapshot

The exact static-policy Affinity/Elves case at seed `2026091231`, game 1, environment seed `6986822005016227846`, is incomplete. Its original Initiative-controller encoding conflict was repaired, but `affinity-elves-source-002/failure.json` now records `StaleEnvironmentBinding: selected action no longer matches the active policy environment`. The lead's subsequent diagnosis identifies a goad legal-choice bug; the source repair and a fresh completed replay are pending. There is no `completion.json` or completed match in that directory. The successful Affinity/Elves learned cases at different seeds do not close this exact reproduction.

Earlier incomplete runs `run-static-teacher-elves-002-v3-001` and `learned-five-deck-v3-001` retain their failures and already completed per-match records. They are not silently replaced, merged into successful batch totals, or treated as completed batches. A new Rally/Affinity/Terror static matrix was still running as diagnostic coverage when this note was drafted and is excluded from these counts.

Fresh Fable consultation `323e67e6-82c2-42db-b1ed-6df762f9d966` for the observation successor failed HTTP 429 weekly quota with zero reads and tokens. The goad consultation `b86c9969-f091-4455-9102-e2161dd74f79` failed the same way. Receipts are under `C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/fable-observation-successor-review-001` and `fable-goad-legal-choice-review-001`. There is no Fable feedback or endorsement. Authorized engineering continued with that limitation disclosed; these checks are not a scientific promotion decision.

## Final manifest fields to bind

Use one small final manifest containing the final clean source commit and binary hash; actual compiler, Cargo and linker versions; CPU device and null GPU ordinal; config and checkpoint paths plus byte sizes and SHA-256s; original export and registry receipt identities; destination feature contract and encoding digests; seeds and limits for each ordered pairing; every completed output file's size and SHA-256; completion totals; selected test-log and parity hashes; review-failure receipt paths; and the exact disposition of seed `2026091231`. Existing `run-start.json` and `play-transfer.json` supply the run-specific pins. Do not replace their historical commits or encoding digests with the final branch tip.
