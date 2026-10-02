# Complete the existing FDN CI filters with fewer builds

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
The observed failure is retained in the hosted log; isolated success remains
unproven until CI executes it.

All integration targets, original external games, native CLI checks and
CUDA host-safe checks retain their existing commands.
Documentation CI37059298773 failed before compilation in Ubuntu job111012877488:
Rustup reported recovery of a partially installed1.94.1 toolchain, then a
conflicting `bin/cargo-clippy`. Each Rust/Python job now installs the unchanged
pin, minimal profile, Clippy and rustfmt into its own temporary
[`RUSTUP_HOME`](https://rust-lang.github.io/rustup/environment-variables.html).
This avoids the damaged image installation and retains the existing compiler,
linker and registry cache. The observed bootstrap failure is environmental;
the fresh installation still needs hosted execution on both platforms.
The first patch at7dad2e51 was rejected before any job started: `runner.temp`
is unavailable in job-level `env`. Define it in the installation step and
publish `RUSTUP_HOME` through `GITHUB_ENV` for subsequent steps, following
[GitHub's context availability rules](https://docs.github.com/en/actions/reference/workflows-and-actions/contexts#context-availability).
Test selection and build metadata collection are preserved. This is CPU
correctness verification; current hosted end-to-end checks remain pending.
The existing healthy PR136 through139 jobs keep running on their original
sources. This isolated follow-up does not cancel them or claim either PC.
