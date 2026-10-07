# Custom-deck session v1: engineering verification

Completed 2026-09-30 EDT for issue 110, milestone 2.

| Check | Result |
| --- | --- |
| New Rust integration tests | 7 passed, 0 failed |
| Existing Pauper session integration tests | 27 passed, 0 failed |
| Limited Python importer and adapter tests | 14 passed, 0 failed, on the primary desktop and the compute host |
| Clippy, new binary and integration target, warnings denied | Passed |
| Native subprocess replay | Complete 40-card game twice, byte-identical transcripts |
| Actual Python-to-Rust adapter replay | Natural terminal twice, identical transcript SHA-256 |

The external adapter's pass-only Forest/Island smoke reached natural game-over
at 200 policy steps and 200 physical decisions. Its outcome was P0 win through
the existing game-over path. These land-only fixtures verify plumbing and
replay, not FDN parity, a win-rate estimate or playing strength.

Small verification manifest:

- Implementation commit: `e53b1a840213d778bb01de013dcb98f26be9ea72` (`codex/fdn-custom-session-v1`).
- Remote source projection committed at `a5a0d6ca3b70b6100e1fd82134af28ce8d75844d`
  (`codex/fdn-custom-session-verify-v1`); its changed implementation/test files
  are the copied implementation, with documentation left local.
- Host: The compute host. No active build/training/evaluation processes at preparation.
- Rust/Cargo: rustc `1.94.1 (e408947bf 2026-03-25)`;
  cargo `1.94.1 (29ea6fb6a 2026-03-24)`, locked dependencies, two build jobs.
- MSVC linker: installed BuildTools `14.50.35725.0`,
  `VC/Tools/MSVC/14.50.35717/bin/Hostx64/x64/link.exe`.
- Python: local 3.11; remote 3.12.
- Seed: 123 for the external adapter smoke, episode 0. Native integration
  replay uses the same seed with episode 7.
- Forest fixture SHA-256: `713282c0cf8adb4fbc21ef6941d46ccd2b2e1a5ec76433d4d56c63d2b8d19269`.
- Island fixture SHA-256: `28486a09dbc74267b0b66292145dd8b3beee2f8d7abe056bbec21ebc883c2be2`.
- External transcript SHA-256:
  `9a57de1f1140436b1934299eea297923652879c806880b6fc4a4a071ed717174`.
- GPU ordinal: none. This is bounded correctness verification, no formal run.
- Dedicated remote checkout: `C:/Users/hostuser/mtg-kernel-fdn-custom-session-codex`;
  target directory: `C:/Users/hostuser/mtg-kernel-fdn-target`.

Commands on the compute host, with the dedicated target directory configured:

```text
cargo clippy --locked -j 2 -p mtg-kernel --bin kernel_limited_env --test limited_session_v1 -- -D warnings
cargo test --locked -j 2 -p mtg-kernel --test limited_session_v1 --test rl_session
python -m unittest discover -s python/tests -p 'test_limited*_v1.py' -v
```

The external smoke used `LimitedClientV1` against that target's actual
`debug/kernel_limited_env.exe`, with both fixture decks. The two resulting
summaries and transcripts matched exactly. Source was formatted with Rustfmt
before the final Clippy and Rust checks. The primary desktop's lead-owned Q6 timing
process remained active; no local Rust build or GPU work was launched.
