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

| Feature selection | Existing filters, unchanged |
| --- | --- |
| limited-fdn-fixtures | limited_session_v1::tests; rl_session::tests; card_def::; koma_spell_protection; threshold_counts_graveyard; native_training_store_run_v2:: |
| limited-fdn-fixtures native-training-store-v2-production | pre_fdn_profile; prior_fdn_batch; fdn_profile_record_round_trips |

All integration targets, original external games, default workspace tests,
native CLI checks and CUDA host-safe checks retain their existing commands.
Test selection and build metadata collection are preserved. This is CPU
correctness verification; current hosted end-to-end checks remain pending.
The existing healthy PR136 through139 jobs keep running on their original
sources. This isolated follow-up does not cancel them or claim either PC.
