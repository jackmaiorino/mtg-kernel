# Pinned Linux release build

The workflow builds the combined trainer at `c5719e1cf81769f4f30f9e91ae1079d4ee3cbfcf` on a standard public-repository GitHub runner. It exports `native_expanded_training_run_v1` and `checkpoint_container_v1` with SHA-256s and a source/toolchain manifest. It does not execute training or qualify throughput.

`build.sh` installs hash-pinned Rust 1.94.1 components in a pinned Debian 12 Python image. It verifies glibc 2.36, GCC 12, GNU ld 2.40 and the checkout's thin-LTO/one-codegen-unit release profile. Both binaries build sequentially with two Cargo jobs and a clean environment. The current hosted guard accepts only the exact combined source above.

Before downloading the image, `hosted.py` requires a fresh root on a volume with at least 65.5 GiB free. Its private Docker daemon keeps image, container, client and temporary state on that volume. The guard samples free space, allocated owned bytes and available host RAM every five seconds. It stops its own work at 5 GiB growth, below 60.5 GiB free, below 1 GiB host RAM or after 6,600 seconds. The container receives four CPUs and 12 GiB RAM with no swap. These are admission limits and enforced ceilings, not a claim that the build has demonstrated this footprint.

The workflow preserves capacity evidence, `guard.json`, logs, binaries and build manifests for seven days. Download and hash-verify those artifacts into the required durable storage before using a binary in native qualification. Retain failed-attempt receipts and logs. Compiler caches are disposable and are excluded from the artifact upload.

Local checks: `bash -n tools/pinned-release-build/build.sh` and `python3 -B -m unittest discover -s tools/pinned-release-build -p 'test_*.py'`. A passing build still needs workload-specific correctness and throughput checks on the intended execution host.
