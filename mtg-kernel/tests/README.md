# Integration suites

FDN files live in `fdn/` and compile as `fdn_integration`. Standard files live
in `standard/` and compile as `standard_integration`. Keep each file's feature
guards and add its `mod` declaration to the corresponding root suite. CI rejects
missing declarations and ungrouped FDN/Standard files.

Run all modules with the same fresh-process isolation as CI:

```powershell
python python/tools/run_rust_integration_tests.py fdn_integration --features limited-fdn-fixtures
python python/tools/run_rust_integration_tests.py standard_integration --features standard-magezero-fixtures
```

For one former target, use its module as the test filter:

```powershell
cargo test --release --locked -p mtg-kernel --features limited-fdn-fixtures --test fdn_integration fdn_witness_protection_v1::
```

Allocator instrumentation, London mulligans, Limited CLI and the other
integration targets remain standalone. Default workspace tests retain their
coverage; the feature-gated FDN/Standard suites are skipped when unavailable.

The optional `python/tools/check_rust_integration_inventory.py` compares compiled
test names with a pre-consolidation checkout. It normalizes only the original
source-module prefix, includes ignored test names, and refuses omissions or
additions. Use identical toolchains and features for both checkouts.

Hosted CI caches `target/` and registry sources separately for each Rust feature
group, quality checks and the Python environment. Compatible keys include the
runner image, architecture, compiler, release settings, manifests and lockfile.
Each commit can refresh the cache. Cargo checks freshness after restore,
including the build script's Git/source provenance inputs. A cache hit does not
skip any checks. Self-hosted runners keep their existing persistent directories.
