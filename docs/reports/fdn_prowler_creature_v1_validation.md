# FDN Prowler implementation and verification

Hosted CI at `b68eb5b42d58976122ae479c9969fc4f39f5d07f` passed all eight checks: Rust on
Ubuntu and Windows, all four Python shards, formatting/lint and path detection.
Run: https://github.com/jackmaiorino/mtg-kernel/actions/runs/37004227577. Earlier pending entries below record the original
verification sequence. The subsequent status/report update changes no tested
code, workflow, fixture or catalog bytes.

Cackling Prowler is implemented in the opt-in FDN catalog: green 4/3 Hyena
Rogue for `{3}{G}`, ward `{2}`, and an intervening morbid trigger at the
beginning of its controller's end step. A qualifying death adds one +1/+1
counter to the source's exact battlefield incarnation.

Creature death history records actual battlefield-to-graveyard moves with
the type before departure. Opposing creatures and tokens count. Discard,
bounce, exile and noncreature deaths do not. The stamp includes the active
player because the engine's round number can span both players' turns.
Untap clears the stamp; absent history preserves prior default bytes/hashes.

The end-step marker freezes the condition at entry, and the conditional
effect rechecks it at resolution. Bound-source operations inside a
conditional program are materialized and authenticated recursively. Late
deaths/entries cannot create a missed trigger; departed/reentered sources
cannot inherit an old counter.

| Check | Evidence |
| --- | --- |
| Core compile | `fdn-prowler-checks-001` phase 1 passed in 29.85 seconds. |
| Gameplay | All 14 cases passed in checks-001 phase 2 and checks-003 phase 1. Casting, death/timing boundaries, ward, source incarnation, death-history restore, queued trigger restore and pending ward restore. |
| Generated identity | FDN v44 `cc304ceaf678738f`, 200 definitions. Default Pauper v32 and all earlier frozen profile literals remain unchanged in source. |
| Python | All 20 deck/client cases passed on Python 3.11. |
| XMage | All 12 strict-choice cases passed in `fdn-mage-prowler-001`; [Mage PR #9](https://github.com/jackmaiorino/mage/pull/9) records reference boundaries and hashes. |
| Catalog | Checks-004 phases 2 and 3 passed: 46 definition checks, 138 native-record checks and 3 existing ignores. Earlier compile failures exposed a missing consumer match arm and overly narrow historical-fixture test configuration; both are corrected. |
| Sessions and regressions | Checks-004 passed: 66 public Limited session checks, 7 private custom-session checks and 220 prior gameplay cases across fourteen integration suites. |
| Default compatibility and lint | All 68 default session checks and the exact v32 golden passed. Limited all-targets and default workspace all-targets Clippy passed with warnings denied. |
| Production mutation boundaries | `fdn-prowler-production-001` exited zero: 25 release checks passed, including prior-v43 publication/resume refusal and current-profile round trip. Combined-feature release Clippy passed with warnings denied. |
| External natural-terminal replay | `fdn-prowler-external-001` exited zero: two seed-123 games finished naturally with identical results and transcripts. |
| Hosted CI | Draft [kernel PR #133](https://github.com/jackmaiorino/mtg-kernel/pull/133) is running at `b68eb5b4`; no green claim. |

Kernel functional source is local `6abb312d`, remote `bf4f45f3c7400ecd97307678f0b12d8a6e9fbb1e` in the owned
`C:/Users/haley/mtg-kernel-fdn-prowler-codex` worktree. Rust/Cargo 1.94.1,
MSVC linker 14.50.35725.0, two Cargo jobs, no incremental compilation,
no debug symbols, seed 123 and no GPU. Small manifests bind source/input
and output hashes. Checks-004 exited zero above the 60 GiB reserve. During release verification,
the Cargo launcher was observed as `rustup.exe`, exposing a process-name
mismatch in the original stop guard. The release build received a
companion guard that checks the exact launcher path, Cargo/rustup image name
and the verified parent PowerShell script before stopping only its own tree.
It monitors the 384 MiB allowance and 60 GiB reserve every three seconds;
`fdn-prowler-production-001.guard-companion.json` records its actual activity.
The healthy release build was not restarted. It finished successfully; the
companion receipt records `job_completed` and exit zero. No storage stop was
triggered, so this records observed monitoring rather than a tested kill path.
Compiler-failure logs remain preserved outside Git; no measurement run was
started, and no frozen experiment or other agent's tree was changed.

Both original decks retain their pinned SHA-256s. Coverage is now UG 37/40
and WG 36/40 copies, with five distinct card names still missing. London
mulligans, original-deck natural terminals, their deterministic external
replays and complete verification/CI remain required by the active goal.
This slice makes no playing-strength or full-set FDN claim.

The symbol-free debug interface binary is 5,780,992 bytes with SHA-256
`5be5b0c6cfa068d1d97be831bb8b25aa37bf933c781e5984599be95d9330b324`.
Verified cold copies are under both PCs' `fdn-pinned-binaries` directories
and `E:/pinned-binaries/`, each keyed by that hash.

The bounded external deck was 24 Forest, eight Prowler and eight Elvish
Mystic, seed 123, episode 7. Both games ended naturally with P0 winning,
458 policy steps and 452 physical decisions. Each cast three Prowler and
six Mystic and exposed Prowler in fifteen decision menus. Transcript
SHA-256 `772208683fffcfb7c6a577e5379979a1216ac31a0148908d827f5edefe4ec6ab`.
Result SHA-256 `217dbe52f9d104adeb0bad23d8674637742b2a00c41507e28f51c9642e5c6dd8`;
log SHA-256 `419ee8b640f866e5336df0039211f0974e07e9d84c065955cf42687fa7ed5e84`.
The external check establishes interface/game completion and replay;
focused rules/reference cases establish the individual morbid/ward behavior.
These custom games do not complete either original fixture deck.

## Windows compiler environment correction

The completed Windows Kiora job 111079733482 failed 33 default-library cases
with `ForbiddenBuildFlagOverride`: the CI bootstrap exported `RUSTC`, which
the existing compile-time guard correctly rejects. The bootstrap now keeps
the normalized Windows `RUSTUP_HOME` and pinned Rust install without exporting
`RUSTC`. Hosted default and native-build capture checks are pending. The guard
and toolchain pin remain unchanged.

## Current-main integration

The Prowler prefix incorporates Kiora a1473019 and main's 192-definition/v34
catalog. Its generated Limited identity is 230 definitions/v46
6630c9c09989878f. Original and earlier composed tuples remain readable;
mutation uses the live identity. Fixture ID expectations now match the
appended catalog. The main explicit-deck constructor initializes the Limited
trigger state, and the existing human bridge rejects the unsupported Limited
incremental trigger prompt explicitly.

Prowler and monarch end-step events are logged before a single trigger
collection. A regression exercises both orders in their shared ordering window
and requires both the monarch draw and Prowler counter. The reference scanner
classifies the new end-step marker as having no physical object binding.
Catalog generation, formatting, workflow lint and diff checks are local checks;
complete composed-source hosted qualification is pending.

## Isolated Windows timing check

Windows job `111173440630` at `fa963e4e` failed the isolated snapshot check at
64.659 microseconds against the unchanged 40-microsecond limit. Its other
workspace summaries passed, but native, FDN and CUDA steps were skipped; the
overall job remains a failure. The complete terminal log was retained before
publication of this repair.

Only the short timing child now receives Windows `HIGH_PRIORITY_CLASS`, matching
the procedure that passed the complete lifegain-mechanics, Voyage and Witness
Windows matrices. Correctness tests retain their existing priority. The arena,
80-object workload, 200 warmups, 2,000 iterations, 40-microsecond assertion and
compiler/linker pins are unchanged. Both native CI runner scripts are classified
as Rust-relevant so runner changes execute the complete matrix. This prefix's
new timing result and complete runtime qualification remain pending. The new
arena storage implementation and its qualification are tracked separately in
PR #140.

Python compilation, workflow lint and diff checks pass. The timing runner
matches the final CI runner exactly, and a native Windows child-process check
observes normal priority `0x20` and timing priority `0x80`. These checks verify
the runner configuration; hosted timing and full qualification remain pending.
