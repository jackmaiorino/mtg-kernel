# Limited priority presentation validation

Engineering verification, 2026-09-30. Source commit
`e8f4b3b79c531e717681d44df75a46594c93665e`, stacked on the custom-deck session.
HaleysPC verification checkout commit
`2187126d5f505e40e7c63d6c15f9c270a410872e` contains the same implementation and
tests. Both owned branches were committed. No experiment or GPU run occurred.

Small manifest: seed 123, CPU only, GPU ordinal none. Native compilation and
tests ran on HaleysPC with `CARGO_TARGET_DIR=C:/Users/haley/mtg-kernel-fdn-target`
and two Cargo jobs. Jack's PC remained reserved for the lead's Q6 timing run;
only the approximately two-second standard-library Python tests ran locally.
Rust 1.94.1 (`e408947bf`), Cargo 1.94.1 (`29ea6fb6a`), MSVC linker file version
14.50.35725.0, Python 3.12.10 on HaleysPC and 3.11 locally.

| Check | Result |
| --- | --- |
| `limited_priority_v1` integration | 6 passed: all engine windows, turn/draw ordering, held priority by either caster, two-pass resolution, combat responses, public hashes and pending-choice restoration. |
| `limited_session_v1` integration | 9 passed: strict schema/reset/step behavior, retries, explicit mode identity and deterministic complete binary games in both modes. |
| Existing `rl_session` integration | 27 passed, including all 81 ordered catalog deck pairs. |
| Focused library tests | 4 passed: exact surface and session hash goldens, canonical streaming observation hash, and the new engine-priority session snapshot round trip. |
| Python Limited tests | 15 passed locally and on HaleysPC. |
| Clippy | Selected binary and both Limited integration targets passed with `-D warnings`. |
| Fixture mechanic inventory | 36 distinct names, exactly matching the two fixtures' missing-name union. |

Native commands used `cargo test --locked -j 2 -p mtg-kernel` with the three
integration targets above, and separate `--lib` filters for
`surface_binding_envelopes_are_diagnostic_dispatched_with_exact_goldens`,
`environment_hashes_are_diagnostic_dispatched_with_exact_goldens`,
`streaming_stable_hash_matches_the_canonical_json_bytes` and
`limited_session_v1::tests`. Python used
`-m unittest discover -s python/tests -p 'test_limited*_v1.py'`.

The external Python client ran Forest40 versus Island40. Schema 2 completed
naturally after **1,140** policy/physical decisions; two executions produced
the same entire transcript hash. Schema 1 still completed after **200**
decisions with exactly the earlier milestone-2 transcript hash. Both outcomes
were P0 wins through the land-only draw-to-empty smoke policy.

| Input or output | SHA-256 |
| --- | --- |
| Forest40 input | `713282c0cf8adb4fbc21ef6941d46ccd2b2e1a5ec76433d4d56c63d2b8d19269` |
| Island40 input | `28486a09dbc74267b0b66292145dd8b3beee2f8d7abe056bbec21ebc883c2be2` |
| Schema 1 transcript | `9a57de1f1140436b1934299eea297923652879c806880b6fc4a4a071ed717174` |
| Schema 2 transcript, both executions | `9978bc1f3abc33b5d8fa2b58b1148251c1049a52901239f403b90ae62a1c421e` |
| Native debug binary | `7cd96206affc06f57c5b0b9ae40f259e9e2c52a0d304fa87d5234818f93a48e7` |

Remote smoke output: `C:/Users/haley/fdn-priority-external-smoke-001.json`.
Test logs are beside it as `fdn-priority-tests-001.log`, the three valid
`fdn-priority-unit-*.log` files, `fdn-priority-session-snapshot.log` and
`fdn-priority-clippy.log`. These scratch outputs are not committed.

This establishes priority presentation and default compatibility. It does
not establish complete Limited rules, current arbitrary combat damage
allocation, FDN card support, executable XMage parity or playing strength.
No statistical or promotion decision follows from these correctness checks.
