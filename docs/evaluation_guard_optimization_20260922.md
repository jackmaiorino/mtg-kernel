# Evaluation guard optimization hardening

Completed2026-09-22 in the Codex research worktree. Converted259 Python assertions to explicit condition checks raising `AssertionError` in seven modules: evaluation_throughput_v1/v2/v3/v4 (31/62/41/42), stack_evaluation_throughput_v1 (41), and public_evaluation_dispatch_v1/v2 (27/15).

Each module has a small local `_require` helper, so remote copies acquire no new import dependency. Error messages remain lazy via zero-argument functions. The condition and message ASTs were compared against HEAD before edits: all259 are unchanged. No throughput thresholds, ownership rules, hash requirements, candidate selection, recovery rules or evaluation semantics were relaxed.

Validation: the existing five stack guard regressions passed under normal Python, `-O`, and `-OO` (15 test executions). These cover acceptance of a complete panel, rejection of duplicate/missing coverage, wrong seed content despite a valid hash, wrong model identity, and revocation before dispatch input. At each optimization level, all four allocation guard versions rejected a bad schema and dispatcher v1 rejected a real temporary file with a wrong hash; all seven helpers preserved lazy message evaluation. Syntax compilation at optimization2 passed; no `Assert` nodes remain in the converted modules. Evidence: `E:/mtg-meta-recovery-20260921/evaluation-optimization-guards-001.json`.

## Actual boundary

This closes assertion elimination inside these seven files only. The supported future evaluation path must still call the applicable qualification guard and bind its new source/dependency hashes. It does not qualify a new workload or make a raw native executable safe to launch. Historical bespoke dispatchers and immutable evidence trees were not edited; any assertions in those callers still disappear under optimization. The monitor's prohibition therefore remains applicable to such paths, including the archived teacher dispatcher. Worker-group bypasses, main-checkout launchers and raw binaries were not migrated or endorsed.

No local/remote native process, GPU operation, evaluation game, credentials change or paid compute was launched. No active frozen run or old receipt was changed. Before substantial work, requalify the exact next supported path with current ownership and completed-work scaling, and migrate remaining caller assertions or explicitly reject optimized Python. g115 unchanged, M1 unmet.
