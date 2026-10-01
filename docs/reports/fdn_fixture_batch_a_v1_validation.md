# FDN fixture batch A validation

Engineering verification, 2026-10-01. Source commit
`57971ba0a0e5e921727d5b54a1db8654b59aa195`; HaleysPC verification commit
`a05bc00462f00a0bd5d2c0f8cc1e34930206601a`. Both owned branches are committed.
The six additions are Plains, Healer's Hawk, Fleeting Distraction, Cathar
Commando, Spectral Sailor and Treetop Snarespinner. The original 162 registry
records and their IDs remain unchanged; new IDs are 162 through 167.

Small manifest: CPU only, GPU ordinal none, engineering seed 123. Cargo used
two jobs on HaleysPC and its existing `C:/Users/haley/mtg-kernel-fdn-target`
cache. Jack's PC remained reserved for the lead's Q6 run; only short stdlib
Python checks ran locally. Rust 1.94.1 (`e408947bf`), Cargo 1.94.1
(`29ea6fb6a`), MSVC linker file version 14.50.35725.0; Python 3.12.10 remotely
and 3.11 locally. No experiment, promotion or GPU run occurred.

| Verification | Result |
| --- | --- |
| New `fdn_fixture_batch_a_v1` | 19 passed: casting/payment, flash in opponent windows, flying/reach, combat deathtouch/lifelink, prevention, target loss/blink/control change, repeat activations and pending-state restoration. |
| Existing Limited integrations | 6 priority and 9 session tests passed, including complete binary games and deterministic replay in both modes. |
| Existing catalog/card integrations | 120 passed across `rl_session` and the 13 modified card-test targets. All 81 ordered catalog pairs reset successfully. |
| Core library modules | 239 passed across `card_def`, `engine`, `event`, `effect`, `state` and `surface_v2`. |
| Store decoder and boundaries | 127 decoder tests passed, 3 ignored. Eight distinct boundary checks passed: historical rejection, v32 live mismatch, simulated future mismatch and v33 construct/seal/decode/validate. Machine-local evidence tests skip when their files are absent. |
| Hash bindings | Safe scorer packets, surface bindings and session environment goldens passed. Both core-state environment hashes retain their prior values; policy provenance changes with the v33 database. |
| Python | 16 Limited tests and 8 V2 golden tests passed; both feature-golden generators reproduce their checked-in outputs. |
| Lint | Selected library, Limited binary and three Limited integration targets passed Clippy with `-D warnings`. Modified Rust files were formatted. |

The database is `kernel_carddb/v33`, hash `ff4b98347ca4ef1d`. The store adds
`FdnFixtureBatchA` as a separate complete catalog tuple; both older frozen
profiles keep their literals. Old records remain readable and v32 records are
refused at publication/resume on this v33 build. Original v32 record byte
goldens are still tested, rather than replaced with new hashes. The nine-deck
runtime catalog is unchanged. Repeated per-wave global count/hash assertions
were centralized in the registry tests; their card-specific assertions remain.

The external Python client played two **constructed supported-subset decks**:
WG used 12 Plains, 12 Forest, and four each of Great Furnace, Hawk, Commando
and Snarespinner; UG used 16 Island, 16 Forest and four each of Distraction and
Sailor. A fixed menu policy prefers land, spell, activated ability and mana
actions, then the first offered choice. Two runs reached the same natural
terminal after **919 policy decisions / 915 physical decisions**, with 12
casts and 18 activations each. This is a transport/rules smoke, not a policy
strength estimate or completion of the original fixtures.

| Input/output | SHA-256 |
| --- | --- |
| Registry JSON | `5bd756b4f9d571cf6efa7fe3a99312e7a8d46bc170cb18b162271d3013008492` |
| New formatted rules test | `02337afce8400759ebeac4bb3a45558b45c91b1863060f90c424bdc1a87e5310` |
| Constructed WG input | `a139dddea55a863027df164607e3263aea207328f63ae6c53692ba5319bd8cc8` |
| Constructed UG input | `067b671e8ff436328c5b07bf64d0fa1bd3fb183235d7e582bf339011bc28208b` |
| Both external transcripts | `d1748cf40dae2fe1a8dd0423067abc9062869019dbc13336c4b8ed7f49d35aba` |
| Verified debug binary | `3b7ddc66ba0d58757f873e5df1c878b931f6b59094e7db9450c919ce95f041b4` |

Commands used `cargo test --locked -p mtg-kernel -j 2` with the integration
targets and library filters above. Logs and the smoke driver/report are under
`C:/Users/haley/fdn-batch-a-*`. The verified binary is preserved under
`C:/Users/haley/fdn-pinned-binaries/3b7ddc66ba0d58757f873e5df1c878b931f6b59094e7db9450c919ce95f041b4/`.
The broad library attempt was stopped during the expensive checkpoint-evaluator
test; it is not a full-suite pass. Hosted CI remains the full-suite check.

Coverage now equals **13/286** reference names, with **273 missing**. Both
original fixtures have **19/40** supported mainboard copies and their union
still has **30 missing names**. They continue to be rejected by deck resolution.
Mulligans, current arbitrary damage allocation, planeswalkers, the remaining
card batches and executable XMage comparisons remain outstanding. These
checks establish engineering behavior, not full FDN support or playing strength.
