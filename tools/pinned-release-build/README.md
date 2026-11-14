# Pinned Linux release build

The workflow builds the combined trainer at `f933c8b8dfc60945eff683b5125d694231109417` on a standard public-repository GitHub runner. It exports `native_expanded_training_run_v1` and `checkpoint_container_v1` with SHA-256s and a source/toolchain manifest. It does not execute training or qualify throughput.

The source is the w128 backport (branch `claude/w128-backport-c5719e1cf-20261114`, PR #694 into `release/w128-c5719e1cf`): the production trainer source `c5719e1cf` plus the V4 engine-fault fix (#673), the net-8w128 learner, movement reader and widening rows (#678, #681, #682) and the wide-state I/O fix. The w128 forks must play the same game as their control chains, which run `c5719e1cf`; main's later rules changes are not in it. The guard also accepts `411feed25` (the backport before the I/O fix) for the fix's equality check only.

`build.sh` installs hash-pinned Rust 1.94.1 components in a pinned Debian 12 Python image. It verifies glibc 2.36, GCC 12, GNU ld 2.40 and the checkout's thin-LTO/one-codegen-unit release profile. Both binaries build sequentially with two Cargo jobs and a clean environment. The current hosted guard accepts only the exact sources above.

Before downloading the image, `hosted.py` requires a fresh root on a volume with at least 81.5 GiB free. Its private Docker daemon keeps image, container, client and temporary state on that volume. The guard samples free space, allocated owned bytes and available host RAM every five seconds. It budgets 5 GiB for the build plus a separate 16 GiB swap file, and stops its own work at 21 GiB total growth, below 60.5 GiB free, below 1 GiB host RAM or after 6,600 seconds. The container receives four CPUs, 12 GiB RAM and at most 16 GiB swap. Cleanup deactivates only the owned swap after stopping private Docker and verifies that existing swap entries remain. These are admission limits and enforced ceilings, not a claim that the build has demonstrated this footprint.

The first hosted attempt passed capacity admission but its compiler was killed under the 12 GiB no-swap limit. Its failed artifacts remain associated with run42536599525 and are retained in the qualification archive. The swap-backed retry preserves the exact source, toolchain and release profile. Docker state is captured before removal to improve the memory-failure diagnosis.

The workflow preserves capacity evidence, `guard.json`, logs, binaries and build manifests for seven days. Download and hash-verify those artifacts into the required durable storage before using a binary in native qualification. Retain failed-attempt receipts and logs. Compiler caches are disposable and are excluded from the artifact upload.

Local checks: `bash -n tools/pinned-release-build/build.sh` and `python3 -B -m unittest discover -s tools/pinned-release-build -p 'test_*.py'`. A passing build still needs workload-specific correctness and throughput checks on the intended execution host.
