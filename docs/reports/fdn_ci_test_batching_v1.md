# FDN fixture CI and snapshot qualification

> October 5 integration status: canonical kernel PR #140 preserves this
> report and its source history. Fixture source `92881658` has passed both full
> Rust matrices and all four Python shards. Its current-main composition needs
> fresh CI and exact-head review/merge. Earlier pending/failure statements below
> refer to their named historical sources. See
> [current fixture validation](fdn_fixture_gameplay_v1_validation.md).

## Current Windows result and arena change

Windows job `111147114674` at `7cbfa1cd` completed with a failure in the
unchanged snapshot timing test: 62.836 microseconds against 40, even with the
timing-child priority change. Later native, Limited and CUDA steps were skipped.
Before that failure, all 24 Witness and 12 London integration cases passed,
including all three natural original-deck games. Their complete receipts match
the prior Linux receipts, and repeated seed123 remains identical. The repaired
private bottom-menu library restore case passed. All four current Python shards
also passed; the overall Rust matrix remains incomplete.

Linux job `111147114731` at the same `7cbfa1cd` source completed successfully.
The complete log shows all 21 selected integration targets and 347 passing
cases with no integration failures or ignores, plus the default, native,
Limited library and host-safe CUDA checks. The isolated snapshot passed at
7.084 microseconds. All three complete original-game receipts equal the
Windows receipts and the prior Linux receipts; private bottom-menu restore
also passed. This qualifies the earlier source on Linux. The Windows failure
above remains, and the arena change below still needs hosted qualification.

The arena now stores its ordered objects in `Arc<Vec<T>>`. Clones share that
storage, and `push`, `get_mut` (including indexing) and `iter_mut` detach it before
mutation. Other game-state fields still clone normally. This avoids cloning
every object and owned buffer when a snapshot is captured, while retaining
independent mutations and stable IDs. The first mutation of shared storage
still clones its objects; this does not claim faster complete turns or search.
Serde's `rc` feature retains the existing `items` array representation without
changing dependency versions or the lockfile.
The arena's mutating methods now require `T: Clone`; the production arena stores
`GameObject`, which already implements that trait.

Two arena regressions cover every mutation entry point, append IDs, legacy JSON
and value-based hashing. The existing snapshot round trip now checks captured
bytes and hash before mutation, after restore, and after a subsequent independent
draw. The entire frozen 80-object workload and timing function remain unchanged,
including 200 warmups, 2,000 iterations and the 40-microsecond assertion.
Pinned formatting and diff checks pass. Hosted compilation, the new regressions,
all original games and the full Rust matrix still need to qualify this change.

The actual changed `ids.rs` module passed all five unit tests in a small
standalone Windows release crate, including both new arena regressions.
Rust 1.94.1, one BelowNormal build worker and the workspace's exact 11 dependency
versions/checksums were used. Compilation took 14.53 seconds; the locked recheck
also passed all five cases with no failures or ignores. Evidence is
`E:/codex-checks/fdn-arena-cow-1cxafdwa/qualification.log`.
This checks arena cloning, IDs, serialization and hashing, not the full engine,
GameState restoration, original games or the frozen snapshot timing gate.

## Earlier CI batching work

The Ubuntu log from CI37038033614 repeatedly compiled the same Limited test
library for separate filters, with roughly eight minutes per compilation.
The Windows Armor job110918929067 remained in the Limited step at19:14UTC,
having entered it at17:25UTC. These are observed compiler costs, not a measured
speedup from this change.

Group the six Limited library filters and the three combined-feature filters
into two Cargo invocations. Cargo builds the release library, then its
[configured runner](https://doc.rust-lang.org/cargo/reference/config.html#targetcfgrunner)
launches a fresh test process for each original filter. This retains Cargo's
package working directory and runtime environment, propagates failures, and
preserves the pinned Rust/linker/release settings. Rust jobs use the repository's
pinned Python3.13.14 for the helper.

Rust steps explicitly use Bash on both hosts, so GitHub's `-e -o pipefail`
invocation stops on the first failed command. The default Windows PowerShell
wrapper checks the last native exit code, which cannot establish success of
every earlier Cargo command in a multiline step. Historical Windows step
success therefore requires inspection of all test summaries in its full log.

| Feature selection | Existing filters, unchanged |
| --- | --- |
| limited-fdn-fixtures | limited_session_v1::tests; rl_session::tests; card_def::; koma_spell_protection; threshold_counts_graveyard; native_training_store_run_v2:: |
| limited-fdn-fixtures native-training-store-v2-production | pre_fdn_profile; prior_fdn_batch; fdn_profile_record_round_trips |

The Windows Draw recheck at01c7334b failed the existing snapshot wall-time
test:67.822microseconds per clone against its unchanged40microsecond budget,
with1743 other library tests passing. Job110986481520 ran the timing loop
alongside parallel unit tests. Preserve its80objects,200warmups,2000iterations
and40microsecond assertion. The workspace runner executes every other library
test normally, then the exact timing test in a fresh one-thread process using
the same compiled library. All other workspace executables run unchanged.
The observed failure is retained in the hosted log. PR140's complete Ubuntu
job 111036127712 at 36d54594 executed exactly one isolated timing case and
passed at 4.81 microseconds per call. The default library's other 1705 tests
passed, with 43 existing ignores. PR140's Windows release stage is live;
its exact timing receipt remains pending.

All integration targets, original external games, native CLI checks and
CUDA host-safe checks retain their existing commands.
Documentation CI37059298773 failed before compilation in Ubuntu job111012877488:
Rustup reported recovery of a partially installed1.94.1 toolchain, then a
conflicting `bin/cargo-clippy`. Each Rust/Python job now installs the unchanged
pin, minimal profile, Clippy and rustfmt into its own temporary
[`RUSTUP_HOME`](https://rust-lang.github.io/rustup/environment-variables.html).
This avoids the damaged image installation and retains the existing compiler,
linker and registry cache. The observed bootstrap failure is environmental;
the fresh installation passed on both platforms at PR140 source 36d54594.
The first patch at7dad2e51 was rejected before any job started: `runner.temp`
is unavailable in job-level `env`. Define it in the installation step and
publish `RUSTUP_HOME` through `GITHUB_ENV` for subsequent steps, following
[GitHub's context availability rules](https://docs.github.com/en/actions/reference/workflows-and-actions/contexts#context-availability).
Test selection and build metadata collection are preserved. This is CPU
correctness verification. Ubuntu's full job completed successfully at
22:50:49 UTC October 2: all 21 integration targets, 345 focused cases, all nine
original library filters and host-safe CUDA checks passed. The three production
filters select zero cases on Linux by their Windows-only configuration; their
Windows execution remains required. Formatting/all-feature lint and all four
Python shards passed. The full Windows Rust job is still running.
The existing healthy PR136 through139 jobs keep running on their original
sources. This isolated follow-up does not cancel them or claim either PC.

The complete Ubuntu job111036127712 at36d54594 passed all21 integration
executables/345cases, all nine grouped library filters, default regressions
and host-safe CUDA checks. The isolated snapshot case passed at4.81microseconds.
Windows job111036127744 passed Witness, original London games and pending
restore, default regressions, and the isolated snapshot case at18.858microseconds.
It then failed native feature compilation with
`native_store_rustc_path_not_drive_absolute`: the step's temporary toolchain
path contains mixed separators. The native validator rejects forward slashes.
The workflow now normalizes Windows RUSTUP_HOME and the explicitly pinned
RUSTC path with cygpath before publishing them through GITHUB_ENV. Keep the
validator and compiler/linker pins unchanged. Local path conversion, exact
Rust 1.94.1 invocation, workflow lint and diff checks passed. Hosted native
compilation and complete Windows checks for the correction remain pending.

## Current-main composition

The final stack includes main fe479186, London mulligans, all 21 fixture
integration targets and the actual `current_profile_record_round_trips` filter.
The workspace runner accepts the workflow's `--no-fail-fast` argument and uses
`--include-ignored --exact --test-threads=1` for the unchanged snapshot timing
case. The Windows bootstrap retains normalized `RUSTUP_HOME` without exporting
`RUSTC`: Kiora job 111079733482 demonstrated that this override is correctly
rejected by the existing compile-time build guard. Composed-source complete
Linux/Windows, native capture and CUDA qualification is pending.

## Windows reservation-test synchronization

Draw job111114931527 at b296504e failed the existing WMI chain test:
the final descendant sleeps six seconds, so delayed inspection can observe
the reservation after it has correctly released. The Python shard ran372
tests with one failure and one skip. An eight-second delay before the first
status inspection reproduces the same `free` versus `held` assertion locally.

The test now keeps that descendant alive until an explicit release file is
written after the held/contained/member assertions. A120-second timeout bounds
cleanup if the test never releases it. The release outcome and timestamp
assertions remain unchanged, as does production reservation code. The normal
chain and abandoned-supervisor tests pass locally (2cases,2.777seconds), and
the explicit-release case passes with the same eight-second inspection delay
(1case,9.759seconds). Hosted Python qualification remains pending. Existing
London runtime jobs remain on cd7f451b; this change affects only the Python
test and this report.

## Isolated Windows timing failure

Lifegain-mechanics Windows job111116909367 at50ea8189 failed the isolated
snapshot case at68.294microseconds per clone against the40microsecond budget.
The preceding library run passed2335cases with82existing ignores, but the
default release step failed overall; native, FDN and CUDA steps were skipped.
Isolation alone did not establish the timing requirement on that runner.

The next bounded environment check gives only the short Windows timing
process HIGH_PRIORITY_CLASS. Correctness tests keep normal priority, and the
snapshot implementation,80objects,200warmups,2000iterations,40microsecond
assertion, compiler and linker settings are unchanged. A local child-process
smoke check observes normal0x20 and timing0x80. Python compilation and diff
checks pass. Scheduling contention remains a hypothesis until hosted timing
and the complete matrix pass; this does not reclassify the failed job.

The path classifier now treats both native CI runner scripts as Rust-relevant.
A runner-only change must execute the Rust matrix to qualify its actual test
procedure. Existing library, integration, production and CUDA commands remain
selected. The original London runtime jobs continue on cd7f451b.
