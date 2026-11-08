# Drake history comparison, 2026-11-08

The four fixed cases preserve every terminal result and final-state hash, but
the immutable-history candidate is slower in every case. Total recorded
decision time increases 37.3%. This does not establish the flat-cost improvement
required by #474; PR #583 remains draft and the issue remains open.

The table sums native decision-window time, then averages the borrowed and
owned staging variants of each identical game. It excludes process startup.

| Fixed case | Baseline seconds | Candidate seconds | Time increase |
| --- | ---: | ---: | ---: |
| train 241 | 16.086 | 21.856 | 35.9% |
| train 436 | 14.833 | 19.347 | 30.4% |
| train 597 | 20.505 | 29.964 | 46.1% |
| eval 458 | 12.468 | 16.539 | 32.6% |
| Sum | 63.892 | 87.706 | 37.3% |

All 100-decision windows are retained. The final partial window is normalized
only for plotting; totals use its actual elapsed time. For the owned candidate,
the windows containing 50%, 70%, and 90% of completed decisions cost:

| Case | 50% seconds | 70% seconds | 90% seconds |
| --- | ---: | ---: | ---: |
| train 241 | 1.587 | 1.823 | 4.622 |
| train 436 | 0.949 | 1.541 | 2.269 |
| train 597 | 1.834 | 2.587 | 3.084 |
| eval 458 | 0.986 | 1.833 | 2.014 |

The completed-work comparison used the same two qualification cases, exact
Windows binaries and runtime inputs on both PCs. Serial/two-worker batch times
were 168.601/83.945 seconds on Jack's PC and 175.157/85.250 on Haley. Both chose
two workers and reproduced identical terminal hashes. The guarded launcher
selected Jack's PC, workers 0 and 2 with SMT siblings reserved on 0-3, for the
remaining cases. Other research claims stayed outside those cores. There were
40 bounded engineering games across qualification and comparison, no paid
allocation, and no training or playing-strength measurement. Haley drained its
descendants, removed its logon-local mapping and released its reservation.

Baseline native source: `384c8a4e5103f40384928adb69720268e4ecd8cb`.
Candidate native source: `17856aa9b6e6c116440b64acab0dcbfe4306e03a`.
Runner source: `15ccba4ce346d7eadc85ea34d013b6840de1d94a`.
The retained plan pins both binaries, all input bytes, seed 1, the exact cases
and guarded commands. Serial qualification is not counted as an independent
statistical sample. These are descriptive results from four fixed cases, with
no confidence interval or general performance claim.

Primary evidence directory:
`D:/mtg-kernel-history474-timing-evidence-20261108-01`.
`comparison-v2/comparison.json` SHA-256:
`b084f76ba4742e2b796f20cf71b8d3917f4de3c555e2bdbf5b7959454357873a`.
`selection-v2.json` SHA-256:
`ccce57d5e79af2fd7dd79b6c860fe1e16af6feea857f5d4709e3b24e65aa1a8c`.
`result-v2/` contains the complete-window CSV, numeric summary and inspected
`history-window-comparison.png`. No bulk artifacts are committed.

Next: diagnose the remaining growing work and the regression against these
retained receipts before changing or measuring another candidate. The baseline,
candidate, failed attempts and completed comparison remain intact.
