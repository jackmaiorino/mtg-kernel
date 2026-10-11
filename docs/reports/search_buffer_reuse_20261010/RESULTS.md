# Completed search throughput comparison

Frozen baseline `1d2404775a141db4cd90d7579f8f765db37d55df`; candidate `a70a6b9db1d1148f8aab29d7da11b88bb5b1b63d`. Both use Rust 1.94.1, MSVC 14.50.35725.0, ordinary activation math, identical existing cast/target roots and seed 2026100941. This is the authorized two-root engineering comparison at reduced budgets.

All four qualification and four ABBA cases completed successfully. Two workers on admitted cores 16-17 were selected by the lower combined qualification elapsed time. Native checks passed: 38 tensorizer, 21 policy/value and 42 sideboard tests, with 9/1/1 existing ignores, plus strict Clippy, trace-feature compilation and release build.

| Phase | Case | Workers | Queue seconds | Cold-copy seconds |
| --- | --- | ---: | ---: | ---: |
| qualification | baseline-1w | 1 | 109.441000 | 0.040984 |
| qualification | candidate-1w | 1 | 102.914113 | 0.177289 |
| qualification | baseline-2w | 2 | 61.626784 | 0.033481 |
| qualification | candidate-2w | 2 | 59.290506 | 0.032117 |
| ABBA | 1-baseline | 2 | 59.787186 | 0.040532 |
| ABBA | 2-candidate | 2 | 56.434971 | 0.069016 |
| ABBA | 3-candidate | 2 | 61.566586 | 0.040298 |
| ABBA | 4-baseline | 2 | 66.931022 | 0.041645 |

ABBA queue totals: **126.718208s baseline / 118.001557s candidate**, or **1.073869x completed queue throughput**. Adjacent baseline/candidate pair ratios are 1.059400, 1.087132. Startup, input loading and serialization are included; cold-copy time is reported separately. This does not establish a whole-pipeline, all-workload or playing-strength gain.

All eight cases have identical opaque primary store SHA-256 `7c316546ef1e82843151ee0fc52e6255bbbe1a5d9beafd829d3f7a5697b2f202`, identical per-root primary hashes and identical evaluation/selection work counts. The separate read-only acceptance audit verified the sealed and staged binary identities, all 50 plan pins, 15 build/log pins, and each of the 11 cold files plus recovery receipt in every case. D: and E: plan/qualification/result bytes agree.

The [existing state record](STATE.json) binds the completed plan, terminal marker, qualification and ABBA result hashes. Raw receipts remain under `D:/search-buffer-reuse-20261010/q001`; independent cold copies remain under `E:/search-buffer-reuse-20261010/q001`. All failed/superseded attempts remain retained. The measured candidate has identical runtime source to reviewed `c9fa277`; its later changes only publish marker/report evidence.

No additional comparison, reconstruction, training or paid execution was launched for delivery review. Current-head checks and default-branch integration acceptance remain the final delivery steps.
