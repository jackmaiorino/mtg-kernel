# Exact-prefix activation implementation status

2026-09-22. Engineering progress, no strength gate or lineage promotion. g115 unchanged and M1 unmet. Design: v4_activation_design_20260922.md. Literature posted before actual-g115 use: collab/lit/20260922-v4-activation.md (collab commita665cba).

Implementation adds evaluation/activation.rs and --activate-search, with ordinary prefix and root verification, designated-seat search-package identity, unchanged opponent, typed mismatch/abort, and target-game-only result scope. Existing evaluate/Report calls pass no activation. A current-runtime ordinary package and strictly validated search-only modification bind the same model. No training path is added.

Independent review B/v4-activation-review-001 COMPLETEexit0/is_errorfalse/nonempty response/10actualreads; session6daefb5f-e241-443b-9f8f-6c00972208c4 (B=E:/mtg-meta-recovery-20260921). Judgment proceed, no concrete source bug. Accepted integration observations: game2 must exercise prior-game checks; search diagnostics count toward the archived byte cap. Planned Hand fixture is game2/index113/P1; Burn is game1/index242/P1. Both original configs allow536870912record bytes; full archived ordinary stores used10858216 and18795784 respectively. No cap change. Metadata plus loaded identical ordinary V4 package was deemed sufficient for the derived search package. No further source change requested.

Controls001 preserved compile failure: missing activation:None in an existing Report test constructor, exit101 after42.45965seconds. Added that test-only initialization. Controls002 is the corrected invocation, still running at this document revision, handle1461, native CargoPID67256, rootB/v4-activation-controls-002. Latest live process inspection confirmed compilerPID62312 accumulating CPU711.94seconds at parent elapsed399.4seconds. Do not restart on an observation timeout. Source pins include24files; verify them after terminal completion. No actual-g115 activation result exists yet.

Prepared supported fixed helper B/check-v4-activation-integration.py (not launched), build and integration modes. Clean owner-guarded four-job BelowNormal build; two consumed positions, oldmean/E, S128/T1024/depth8/seed20260922, ordinary opponent, two fresh-process semantics repeats. It retains all four arm results including aborts and does not require a win or relabel a partial match. Archive files are compact and under64MiB; input config/packages match archived collection exactly. Scope manifest pins literature and compute policy. This small fixed correctness scope supplies no population effect estimate or throughput qualification for substantial work.

Next: inspect the same live control handle, preserve any failure, verify source hashes if tests pass, commit source, build actual executable, then collect the fixed natural/abort results. Do not tune from fixture outcomes. Future whole-match evidence still needs separate literature/power/monitor-seed authority and useful-compute qualification.


## Corrected-build environmental interruption

Controls002 completed unsuccessfully, exit15 after697.2681seconds, before any test ran. CompilerPID62312 stopped accumulating CPU at711.9375seconds; two saved compiler-wait snapshots have identical CPU/IO, no child/linker, ample RAM and107GiB free E storage. A verified own-process termination request left Windows termination pending; closing verified Cargo parents20804/67256 ended the controller, and the compiler subsequently disappeared. cleanup-complete.json verifies all three PIDs absent and all24source hashes unchanged. No unrelated process was touched and no guard was bypassed. This is an interrupted preflight, not a measurement failure or discarded replication.

Controls003 is a fresh retry of exactly the same source, flags and tests through B/check-v4-activation.py after the ownership guard cleared. Live unifiedexec handle60250; inspect B/v4-activation-controls-003/process.json and test.log, not the now-terminal controls002. No actual-g115 activation launched yet. Do not infer a passing control result from successful restart.


## Current verification: controls passed

Controls003 COMPLETE, exit0,17/17passed,0failed;124.90seconds tests and407.325939seconds invocation. All24source pins rechecked unchanged after completion. This closes the pending control status above. Both controllers/both seats/semantic repeats, switched-package/action records, ordinary opponent, negative prefix/root boundary, and existing ordinary/mean/E/Report compatibility controls passed. All native control processes are terminal.

Proceed to clean executable build and the already-declared fixed actual-g115 activation comparison. No source tuning or test change occurred between interrupted002 and passing003. The interrupted compile remains recorded; no actual experiment outcome has been repeated or dropped.
