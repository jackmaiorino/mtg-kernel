# M4 D1 and D2 result

## D1

D1 head is `de1d02c528ba83bbe5838df91464d3d72ead22f7`. The index has 545 rows, comprising 540 cube cards and five basics. Its SHA-256 is `fb2f6fa80f612fb100be983b1c2a4947f8c3add61aa2d81b187d8e49ea526b63`.

The SHA-256 of `mtg-kernel/tests/data/cube_draft_build_v1/dense_fields_and_scales_v1.json` is `36a79a1919f4f15f4774409b8b0acbdaea5903187a5d4475d604530592deb5da`. It contains the 56 mapped dense column indices and 978 indexed-row divisors in vocabulary order. The regenerated table adds the hybrid tokens `cost:hybrid:GW` and `cost:hybrid:RW`. D0 revision 9 pins the 56 mapped fields for the provisional table. Both new tokens are scaled in the full feature vector.

The focused x86-64 Linux release harness compiled the final D1 module and integration tests against the Cube engine and registry from the regular release build. It passed six integration tests and 15 unit tests. The fetch test was ignored until the table has fetch columns. The D1 unit tests also passed in D2's standard Cargo release test executable.

## D2

The parameter-layout SHA-256 is `7d2c6692d675436dd17c85d060b11022b8d88eb4ac9a0e943e8dbe8eba642c60` for 166,410 parameters on the 545 by 978 table with 56 dense fields.

The SHA-256 of `mtg-kernel/tests/data/cube_draft_build_v1/model_forward_goldens_v1.txt` is `27b684623d783dd1529f87b38640123c4c3a7c3824b51a036e9e5695f11c1c21`. The test emitted 885 lines from the bound card rows and fixed pick and build inputs. The capture used x86-64 Ubuntu under WSL, glibc 2.35, and `rustc 1.94.1 (e408947bf 2026-03-25)` on source commit `a9b18742e1441d0cac74bea7c08e20029c3fea97`.

```powershell
wsl.exe -e bash -lc 'cd /mnt/n/cube-20261122/d2 && export GIT_DIR=/mnt/n/cube-20261122/repo/.git/worktrees/d2 GIT_WORK_TREE=/mnt/n/cube-20261122/d2 CARGO_TARGET_DIR=/mnt/n/cube-20261122/d1/target && mkdir -p target && CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 RUST_TEST_THREADS=2 nice cargo test --release --locked -j 4 -p mtg-kernel --features eternal-fixtures --lib cube_draft_build_v1::model::tests::forward_matches_the_recorded_golden_bits -- --exact --nocapture --test-threads=2 >target/m4-forward-capture.log 2>&1'
```

The missing-file capture printed the expected record. The committed fixture stores the bytes between the two capture markers with LF endings. The same release test executable then passed the bit comparison and all 18 D2 model tests, including thread-count determinism. It also passed all 15 D1 unit tests for 33 passing tests in total.

## Second-host check

The arm64 Linux check is pending. Run this from the root of the completed D2 checkout on that host.

```bash
CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 cargo test --release --locked -j 4 -p mtg-kernel --features eternal-fixtures --lib cube_draft_build_v1::model::tests::forward_matches_the_recorded_golden_bits -- --exact --test-threads=2
```

## D0.4 question

D0.4 carries forward a sentence saying Kappa Cannoneer stays fenced under A3am1. The pinned `fence_v3.json` lists 15 cards and records Kappa Cannoneer's release under A3am2. Should that sentence be updated before the ordered pool is signed?
