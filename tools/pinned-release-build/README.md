# Pinned Linux release build

The workflow builds the combined trainer at `a30c3769f1ef3a7b569ab52814688141969d877f` on a standard public-repository GitHub runner. It exports `native_expanded_training_run_v1` and `checkpoint_container_v1` with SHA-256s and a source/toolchain manifest. It does not execute training or qualify throughput.

The source is the w128 backport (branch `claude/w128-backport-c5719e1cf-20261114`, PR #694 into `release/w128-c5719e1cf`): the production trainer source `c5719e1cf` plus the V4 engine-fault fix (#673), the net-8w128 learner, movement reader and widening rows (#678, #681, #682), the wide-state I/O fix and the checkpoint-write change (#696). The w128 forks must play the same game as their control chains, which run `c5719e1cf`; main's later rules changes are not in it. The guard also accepts `f933c8b8d` (the backport before the checkpoint-write change, the a266eb4d pin) for that change's equality check.

The guard also accepts the London keep-offset backport (PR #700 into `release/mull-c5719e1cf`): `c5719e1cf` plus only an opt-in pregame keep offset for London mulligans. It is built by explicit dispatch (`source_commit`); the default source above is unchanged.

Four later sources of the same release line are accepted the same way, each built by explicit dispatch: the per-count keep offset and terminal pregame advantage change (PR #707) at its first head `587b5aca2`, at its head after test-only lint fixes `6faa42219` and at its merge commit `ce3df74f8` (the same tree as that head), and the merge commit `b6516e6f5` of the pregame announcement step scale change (PR #729). `hosted.py` names each one beside its commit id.

The guard also accepts these exact `source_commit` values for the signed experiment.

- `a561d7a3a4ea1762cf0f2ec9d8f8c2ac244e3acc` is the `main` merge of PR #991 with the game-test adapter for `regret_census_v1 s4a-budget`.
- `2479c637d674b786bf6eb5104520baeb84d500e6` is the old-catalog base (3ca7b1ad4 plus PRs #988 and #991 by cherry-pick; draft PR #997, never merged): the game test's build source after main's catalog change.
- `46f7ecfec16dd0483f14865ff5a1af37fe6c3420` is the `release/mull-c5719e1cf` trainer that produced the frozen checkpoint for the parity gate.

`hosted.py` holds the full exact allow-list, including this branch's `PACK3_SOURCES`.

`build.sh` installs hash-pinned Rust 1.94.1 components in a pinned Debian 12 Python image. It verifies glibc 2.36, GCC 12, GNU ld 2.40 and the checkout's thin-LTO/one-codegen-unit release profile. The selected binaries build sequentially with two Cargo jobs and a clean environment.

The optional dispatch input `bins` sets `PINNED_BINS` and accepts `native_expanded_training_run_v1`, `checkpoint_container_v1` and `regret_census_v1`. Separate names with spaces or commas. Omit the input or use an empty string to build the trainer followed by the checkpoint container. The guard refuses unknown or repeated names and input made entirely of separators. `BUILD-MANIFEST.json` lists the selection in `build.binaries`, and `guard.json` adds `binaries` when the ordered selection differs from the default pair.

Before downloading the image, `hosted.py` requires a fresh root on a volume with at least 81.5 GiB free. Its private Docker daemon keeps image, container, client and temporary state on that volume. The guard samples free space, allocated owned bytes and available host RAM every five seconds. It budgets 5 GiB for the build plus a separate 16 GiB swap file, and stops its own work at 21 GiB total growth, below 60.5 GiB free, below 1 GiB host RAM or after 6,600 seconds. The container receives four CPUs, 12 GiB RAM and at most 16 GiB swap. Cleanup deactivates only the owned swap after stopping private Docker and verifies that existing swap entries remain. These are admission limits and enforced ceilings, not a claim that the build has demonstrated this footprint.

The first hosted attempt passed capacity admission but its compiler was killed under the 12 GiB no-swap limit. Its failed artifacts remain associated with run42536599525 and are retained in the qualification archive. The swap-backed retry preserves the exact source, toolchain and release profile. Docker state is captured before removal to improve the memory-failure diagnosis.

The workflow preserves capacity evidence, `guard.json`, logs, binaries and build manifests for seven days. Download and hash-verify those artifacts into the required durable storage before using a binary in native qualification. Retain failed-attempt receipts and logs. Compiler caches are disposable and are excluded from the artifact upload.

A dispatch may name a target-flag variant (`target_cpu`: `x86-64-v3-no-fma`, `x86-64-v3`, `znver5`, `znver3`; default `default`, the portable x86-64 build, unchanged). `build.sh` then compiles every crate built for the target, dependencies included, with the named `-C target-cpu` (and `-C target-feature=-fma` for the no-FMA variant) through an explicit `--target`. The flags do not reach the precompiled standard library, build scripts or proc macros.

A variant bundle is not a portable pin and says so: its `BUILD-MANIFEST.json` has the schema `fleet-combined-trainer-build-target-variant/v1` and names the variant and flags (`build.target_cpu`, `build.target_rustflags`, `build.explicit_target`), its upload artifact is `pinned-release-<run id>-<target_cpu>`, and `guard.json` records `target_cpu`. The portable build's manifest, artifact name and binaries are unchanged; its receipts record a new `build_helper_sha256` because `build.sh` changed.

A variant binary runs only on CPUs with those features and is qualified like any other pin: first-updates equality against chain records on the execution host, then throughput. It does not meet the build-flag contract of `model_guided_search_contract_digests_v1.rs` (target rustflags, and the `fma` and `avx` target features), so never use one where `ModelGuidedSearchAuthorityV1` is validated.

Four prototype variant builds on `release/mull-c5719e1cf` were built and qualified on a Zen 5 node from a temporary guard branch carrying an earlier version of this change (using the previous `fleet-combined-trainer-build/v1` manifest schema with `build.target_rustflags`):
- Run `44201511056` (`x86-64-v3-no-fma`, `-C target-cpu=x86-64-v3 -C target-feature=-fma`): source `77908fee6f2f647cf0cfa7a95ceaeefc4dd3e429` (`--settle-vector-tanh`).
- Run `44201513959` (`znver5`, `-C target-cpu=znver5`): source `77908fee6f2f647cf0cfa7a95ceaeefc4dd3e429`.
- Run `44201516871` (`znver3`, `-C target-cpu=znver3`): source `77908fee6f2f647cf0cfa7a95ceaeefc4dd3e429`.
- Run `44262085652` (`znver5`, `-C target-cpu=znver5`): source `675e2c038ecdcd94a2901080ef8ad16098c57ddb` (PR #592 backport on the mulligan release line).

Neither `77908fee6f2f647cf0cfa7a95ceaeefc4dd3e429` nor `675e2c038ecdcd94a2901080ef8ad16098c57ddb` is in `hosted.py`'s `SOURCES` list on `main`. With this change merged, `main` can build variants only of the listed sources; rebuilding either qualified source requires adding it to `SOURCES` in a separate change, and the qualification measurements above apply only to those two release-line sources.

Local checks: `bash -n tools/pinned-release-build/build.sh` and `python3 -B -m unittest discover -s tools/pinned-release-build -p 'test_*.py'`. A passing build still needs workload-specific correctness and throughput checks on the intended execution host.
