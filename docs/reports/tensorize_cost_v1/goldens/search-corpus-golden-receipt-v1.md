# Search-corpus golden provenance (T1, FABLE-REVIEW-20260924)

`search-corpus-fingerprints-v1.txt` was regenerated on 2026-09-24 from a base executable and is byte-identical to the committed file.

- Source commit: `d17bd86762d6450cd013981d7c0737a620548533`, a fresh worktree at `D:/mtg-kernel-tensorize-base-d17bd867`. Relative to main `cdc1c5a6`, its `native_flat_tensorizer_v2.rs` changes only a `cfg(feature = "tensorize-cost-profile-v1")` counter module (off in this build) and `pub(super)` visibility on test helpers. `fill` and every encoder are the unmodified base code.
- Test source: the head (`83159103`) `native_flat_tensorizer_v2_cost_tests_v1.rs` with only the timing-breakdown harness removed, because that harness calls post-change internals. The golden, replay and fingerprint code is unchanged. Copy: `search-golden-base-test-source-v1.rs.txt`, SHA-256 `6fa352cc193b7f7d17343d5de15a285ef81a48e01bc882b1ab2e5c8c8ae2c9ac`.
- Replay input: `search-matches-replay-v1.json`, SHA-256 `c94b072f99bc04a6775bddfadc562059b9135f06c43a6e33ca2c16308fd76a36`.
- Executable: `mtg_kernel` lib-test binary built with `cargo test --release --lib --no-run` (LTO off, 16 codegen units), SHA-256 `de23221ceb87d1febe3177a236956ad1f24bfff7a52216d6d283a701d81877ad`.
- Command, run from `D:/mtg-kernel-tensorize-base-d17bd867/mtg-kernel`:

```text
MTG_KERNEL_TENSORIZE_GOLDEN_SEARCH_WRITE=<out> mtg_kernel.exe native_flat_tensorizer_v2::cost_tests_v1::tensorize_cost_search_golden_v1 --ignored --exact --nocapture
```

- Output: rollup `4a51c9db4d3b27a0b0451d8ef243355ad6b2ab4491f108e4eb1ed480f795e2d7`, 1,114 decisions (575/539 by seat), file SHA-256 `708a7b57e49035f5893ae1e5f6ad846439c4e0df3b67e59e98920c2c26ce7229`, identical to the committed golden (`cmp`).

The header field `recorded_at=d17bd867...` names the commit at which the 8 search matches were recorded, not the fingerprint source; both are `d17bd867`. The golden test reads this file and compares the full text, rollup included.
