# Evaluation recovery across eligible PCs

The v2 recovery correction measured full-output recovery on Haley only. Its qualifier deliberately rejects newly eligible Jack capacity. The new v3 path extends that engineering question without changing the model, native scorer, evaluation cases, rewards or scientific gates.

`qualify_evaluation_placements_v3.py` considers idle hosts from fresh native inventories. On Jack it measures C/D/E at one, twelve and twenty-four CPU workers; on Haley, C at one, eight and sixteen. Each exact 48-case sample runs twice. When both hosts are eligible, it also measures the fastest individual placements together at fixed Jack:Haley job weights 1:1 and 2:1. Maximum 1,344 engineering BO3 executions, 48 unique target cases, with a 20-minute case-launch budget. This is a bounded grid, not a claim of a global optimum. Remote staging and recovery count toward the projection.

Full recovery uses immutable files from an already completed 3,072-match reference. The files are partitioned with the dispatcher's exact sorted weighted assignment and copied to each candidate storage location. Local recovery performs copytree and full hash readback; remote recovery performs ZIP export, transfer, extraction and full hash readback. Recovery is measured twice and the slower full recovery time is added once. Each target host's sampled output-volume projection must fit its measured reference volume. Longer future games can still exceed the sample estimate and require production monitoring.

The new `placed-full-panel-recovery-fixture/v1` schema explicitly identifies a copied engineering fixture. It does not fabricate a new scientific dispatch receipt. It binds the real reference plan, dispatch, recovery hashes and assignment. Shared reference metadata is retained verbatim, so its fixed overhead can be conservative. Local fixtures omit the remote-only request archive. Existing artifacts and v1/v2 guards remain untouched.

`evaluation_throughput_v3.py` preserves exact request, executable, host, outcome-file and replay validation and adds the typed fixture validation. `public_balanced_schedule_evaluation_v2.py` calls its supported guarded dispatch path. Full evaluation cannot use missing/incompatible timing evidence or a newly eligible unmeasured host. Ownership snapshots are saved before and after cases; overlapping ownership stops qualification and preserves completed artifacts.

Validation so far: Python compilation, diff checks, and nine offline checks in `E:/mtg-meta-recovery-20260921/evaluation-placement-fixture-checks-002/result.json`. Two real completed match files passed copy/readback and ZIP payload checks. Corruption, extra files, duplicate timing samples, wrong assignment weights and escaping paths were rejected. Weighted partition order matches the production dispatcher. No new native games were executed for these checks. Full-size remote/local qualification and a complete launch through v3 remain unverified, and must precede use for a formal panel.

The two balanced-schedule training replicas remain frozen and live. Prepare an evaluation plan only after its complete training audit. Then use the new qualifier with the old entropy replica-two panel solely as a recovery-size fixture:

```powershell
python python/tools/public_balanced_schedule_evaluation_v2.py prepare --help
python python/tools/qualify_evaluation_placements_v3.py --root <fresh-qualification-root> --reference E:/mtg-meta-recovery-20260921/entropy-r2-bo3-001 --plan <prepared-evaluation-root>/plan.json
```

Inspect actual qualification and fresh owners before the guarded full launch. Do not inspect outcome prefixes, reuse contended timings, or change the frozen balanced-schedule analysis. No paid compute is involved. Independent Fable review remains unavailable: known HTTP429, zero source reads, reset September 22 at 07:00 EDT. This implementation proceeds under Jack's assigned research authority with that review gap explicitly unresolved.
