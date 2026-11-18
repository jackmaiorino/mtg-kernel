# Integration suites

FDN files live in `fdn/` and compile as `fdn_integration`. Standard files live
in `standard/` and compile as `standard_integration`. Default-feature Pauper and
engine files live in `pauper/` and compile as `pauper_integration`, so the
workspace suite links one test binary instead of seventy. Keep each file's
feature guards and add its `mod` declaration to the corresponding root suite.
CI rejects missing declarations and ungrouped FDN/Standard/Pauper files. A new
file left directly in `tests/` still runs as its own target.

Run all modules with the same fresh-process isolation as CI:

```powershell
python python/tools/run_rust_integration_tests.py fdn_integration --features limited-fdn-fixtures
python python/tools/run_rust_integration_tests.py standard_integration --features standard-magezero-fixtures
python python/tools/run_rust_integration_tests.py pauper_integration --features ""
```

The Eternal tests that play every playable deck (the best-of-three and catalog
replays in `eternal/eternal_bo3_v1.rs` and `eternal/eternal_catalog_v1.rs`, and
the cube and sideboard deck tests) play their games on worker threads through
`in_parallel` in `eternal/eternal_catalog_v1.rs`, and read the results in the
order their loop lists the games. `MTG_ETERNAL_TEST_THREADS=N` sets the number
of threads (1 plays every game in order on the test's own thread). Without it
they use the available parallelism, at most 8, and no more than a self-hosted
CI job's admitted cores (`MTG_CI_JOB_CORES`, or the core list in
`HOST_SLOTS_CORES`).

A suite can also run in shards: `--shard-index I --shard-count N` places whole
modules longest first by their recorded hosted seconds in
`python/tools/rust_integration_module_seconds.json` (a module without a
recorded time weighs the mean), prints the plan of every shard and fails unless
it holds each module exactly once. CI runs `eternal_integration` whole today.

```powershell
python python/tools/run_rust_integration_tests.py eternal_integration --features eternal-fixtures --shard-index 0 --shard-count 2
```

For one former target, use its module as the test filter:

```powershell
cargo test --release --locked -p mtg-kernel --features limited-fdn-fixtures --test fdn_integration fdn_witness_protection_v1::
```

Allocator instrumentation (it installs a global allocator), the native Store
targets, London mulligans, the Limited session, priority and combat targets
that CI selects by name, and the feature-gated `prepared_*` future-fixture
targets remain standalone. Default workspace tests retain their coverage; the feature-gated FDN/Standard suites are skipped when unavailable.
Raid entry fixtures are admitted with their registered cards and run in the
FDN group alongside Condemn, Run Away Together and Vengeful Bloodwitch.

Later candidate Authority, Employment, Frenzied, Cerberus and Wildwood fixtures
are grouped with their executable candidate definitions. The two standalone
`prepared_crystal_barricade*` targets are future qualification fixtures for an
unregistered card whose mixed prevention ordering is still incomplete. Their
source bodies are preserved exactly and all-test typechecks still compile them.
They are outside the registered FDN execution group; no Crystal gameplay or
meaning pass is claimed. After completing the mechanic and real registration,
run both prepared targets and move them into the FDN group before admission.

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
