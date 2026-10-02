# FDN counter-creature validation

Local source `109bd020` implements Gnarlid Colony and Mossborn Hydra.
Matching committed HaleysPC source is
`2bba84150a2fcebd0d810591b6218547f90d2b37`.
See [rules contract](../design/fdn_counter_creatures_v1.md).

| Check | Result |
| --- | --- |
| New rules | All 16 passed: kicker/payment, pre-SBA counters, continuous trample, landfall ownership/batching/incarnation, wide counters/damage, exact lethal totals and restore. |
| Gameplay regressions | All 133 existing checks passed across fixture A/B, targeted spells, combat cards, legend rule, combat, priority and custom sessions. |
| Pending decisions | Five custom-session restore checks passed, including paid kicker with identical response, action identity and next environment binding. |
| Catalog | All 46 definition and 132 record checks passed. Three existing tests remain ignored. Older literals remain readable; v38 is live. |
| Compatibility | All 28 default state checks, exact environment-hash golden and frozen v32 CardDB check passed. Native publication refuses wide counters/damage before modifying destination buffers. Frozen flat source bytes are unchanged. |
| Build/lint | Feature and default workspace all-target Clippy passed with warnings denied. External binary built. |
| Python | All 41 focused importer, client, flat-V2 golden and Pauper-manifest checks passed. |
| External replay | Two identical seed-123/episode-7 games ended naturally at 381 policy steps and 367 physical decisions. Each cast five Colonies and two Hydras, and paid kicker twice. |
| Production boundaries | Release checks running from clean committed remote source, strict source guard enabled. Passing is not yet claimed. |
| XMage | Five matching scenarios committed at Mage `d05387387e8`, based on `a5c90fe1800`, and dispatched on HaleysPC. Execution is pending. |
| Hosted CI | Pending for this new branch. |

The external deck has 20 Forest, eight Llanowar Elves, eight Gnarlid Colony
and four Mossborn Hydra. Its natural outcome is `p1_win`. This is a
constructed transport/replay check. The original fixture milestone and
playing strength are not established by this result.

Limited v38 is `39c83779971ee2c4`, with 187 full definitions and one partial
reference planeswalker. Original fixture coverage is UG 29/40 and WG 32/40;
13 distinct fixture names remain missing. The full goal also requires
remaining mechanics, mulligans, original-deck natural games, executed
XMage comparisons and passing CI.

Small manifest: CPU correctness checks on HaleysPC, two Cargo build jobs,
GPU ordinal none; Rust/Cargo 1.94.1, MSVC linker 14.50.35725.0, remote Python
3.12.10, local Python 3.11. Jack's PC reservations remain respected. The
XMage check uses Maven 3.9.9, Oracle Java 23.0.2, one reactor worker, two
active processors and bounded heaps. No formal measurement, training or
paid compute was launched.

| Input/output | SHA-256 |
| --- | --- |
| Base registry | `af9d725658d238a6a1b0c99b52c12ff6823bdcaf8147a77556ee2e3b0b67d6bc` |
| FDN extension | `199d26b237f02bb15bef82ad51c343398823d0cf1a16d005e1e97945f862bfdc` |
| New integration tests | `53f5b63a52f8477b13ff2ddf2261c1e222503035a3e4d1ef992a7b37234f4969` |
| External input deck | `906e2af56fe8f787b822ff6294dced8cb8c51e8edfa6916e55d4f32d72a353c4` |
| Tested binary | `d2888059cd1e8aa873bfa60c1f88038c94a1d3da5ca17046450a6f87961921af` |
| External transcript | `95c5dec964ece00d57968a0013ba00bf22c03ad44b7d565114fe3da8e8b88d7d` |

Logs stay outside Git under `C:/Users/haley/`:
`fdn-counter-tests-009.log/.exit`, `fdn-counter-external-001.py/.json/.log/.exit`,
`fdn-counter-production-010.log/.exit`, and `fdn-mage-counter-001.log/.exit`.
Earlier failures are retained. Runs 001/002 caught attempted changes to
frozen flat sources; those files were restored before passing run 003.
Runs 004-007 caught old assertions using the narrower counter type.
Run 008 passed gameplay but found a stale shared catalog fixture. Run 009
passed after that fixture was updated and wide damage was implemented.
