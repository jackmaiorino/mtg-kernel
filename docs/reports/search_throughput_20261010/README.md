# Current search throughput audit

Search is still expensive, and the largest measured target is the complete
policy decision pipeline: observation/action encoding, tensor construction,
network forward, and action sampling. This is distinct from the CPU learner
and artifact processing improved in [PR200](https://github.com/jackmaiorino/mtg-kernel/pull/200).
Optimizing engine transitions or adding more game workers alone does not
address the dominant work in the retained Stage 4a profiles.

This audit reuses completed timing evidence and inspects the current call
chain. It launches no games, training, scoring, native builds, or paid work.
It makes no playing-strength or search-quality claim. The reader projects
cost fields only; game outcomes, discovery results and CP7 are outside scope.

## Evidence and denominators

The formal cohort contains 200 completed Stage 4a roots, 100 per model. Its
272,016.176 summed root seconds divide into 264,605.741 seconds of selection
(97.28%) and approximately 2.72% evaluation, with negligible replay overhead.
Selection performs 295,504,204 policy calls for 307,200,000 transitions.
These are accumulated per-root elapsed times across parallel workers, not
campaign wall time or process CPU time. More than one policy call can occur
at an improved decision, so calls/transition is not a probability.

The median root takes 1,054.32 seconds; p95 is 2,927.59 seconds and the maximum
is 3,746.03 seconds. The original runs and successor shards used different
concurrency/placement conditions, so these distributions describe the retained
work, not intrinsic difficulty at fixed hardware. For the eight successful
shards, last-sampled CPU totals are 99.21% of summed job elapsed time. Together
with the policy timers this supports a CPU-compute bottleneck on those workers;
it does not measure all campaign startup, queueing, or artifact recovery.

E is persistent sequence search; A is the ordinary-policy trajectory control;
D is the locally improved-policy control. All three are part of the measured
instrument. Their combined cost must not be attributed entirely to E.

The reader retains 86 completed roots from two interrupted original jobs and
114 completed roots from successful successor shards. It verifies terminal
file hashes, distinguishes job exit status from completed-root status, and
rejects duplicate roots. It never sums overlapping or resumed job envelopes
and calls that campaign elapsed time.

Two separate engineering roots have detailed stage timers from commit
`59418e4cf16b7f39e0205a032308bab4ae615f99`. In E, the measured policy-decision
pipeline occupies 95.79% and 92.28% of selection time. These are actual V4
search positions with the formal-sized transition budget, not synthetic
network fixtures. They establish where to focus, but two engineering roots
do not establish a universal percentage for all 200 formal roots.

| Detailed profile scope | Policy-decision pipeline | Engine stepping |
|---|---:|---:|
| E, persistent search | 93.89% | 2.37% |
| A, ordinary-policy control | 97.55% | 2.13% |
| D, improved-policy control | 97.27% | 2.14% |
| All three arms, pooled | 96.32% | 2.21% |

Pooled measured seconds are 3,540.09 for the policy pipeline out of 3,675.33
selection seconds. Wrapped canonical-key work is 0.97%, sampling is 0.22%,
and explicit cloning is 0.006%. These partial instrumentation scopes must be
interpreted using the coverage limitations below. Timer overhead was not
separately qualified; no profiled/unprofiled speedup is claimed.

## What the timers mean

The `inference` profile wraps `select_fast_session_v1`, including encoding,
tensorization, forward computation and stochastic action sampling. It is not
a timer for neural-network arithmetic alone. Likewise, selection time divided
by inference-call count measures amortized selection cost per call, not
isolated inference latency.

The profiling branch clears replay/root-scoring counters before E, then
resets them between E, A, D and evaluation. Its scopes are partial:

- D's direct rescoring inside `improve` is counted as a policy call but is
  outside the `inference` timer.
- The sampler includes some state cloning. The explicit `session_clone`
  timer covers other clone sites; zero explicit clones in E does not mean
  that E does no cloning.
- Canonical-key and tree timers cover selected sites. Node selection, child
  keys and some root/control canonicalization are outside those scopes.
- Evaluation world sampling is outside the profile's sampler timer.

The existing residual remains unattributed. No missing category is treated
as zero cost. Source anchors and timer definitions are in [source-map.json](source-map.json).

## Already implemented, and what remains

| Priority | Finding | Consequence and next bounded step |
|---|---|---|
| 1 | Full policy decisions dominate both retained detailed search profiles. The aggregate call count is close to the transition count. Source still constructs temporary tensor and forward vectors repeatedly. | Target this pipeline. Persistent scratch/buffer reuse is a concrete exact-output candidate, with unmeasured savings. Before choosing its next kernel, split actual V4 calls into observation/action encoding, tensorization/digests, forward/activation, and sampling. Include both `act` and D's direct scoring site; bind counts to the existing call counters. |
| 2 | PR206's optional fast activation has a modest real-root gain. | Reuse its qualification before investing in another activation approximation. Any subsequent optimization needs complete fixed-budget root timings and compatible behavior evidence. |
| 3 | D scores an unchanged focal state once to sample an action and again to rank alternatives. | Reusing the first score is a concrete exact-work candidate. However, the two profiled roots contain only 3,978 such extra calls out of 1,002,725 D calls, about 0.397%. This is small cleanup, not the main speed opportunity. Preserve the sampled action, RNG draw order and transition budget. |
| 4 | Independent roots can run concurrently, but one adaptive search remains sequential. | Use the existing qualified root-level parallelism. Evaluate batching across independent roots only if pipeline profiling justifies it; maintain each root's adaptive order and deterministic identity. Neither idle GPU memory nor a core count establishes a useful speedup. |
| 5 | Measured engine-step and explicit cloning costs are small beside policy decisions in these profiles. | Do not start with an engine rewrite or clone-only optimization. Retain unmeasured tree/key residuals as unknown rather than asserting every unwrapped operation is cheap. |

[PR155](https://github.com/jackmaiorino/mtg-kernel/pull/155) already introduced
transposed inference weights, shared scorer state computation, multibuffer
SHA-512/prefix reuse and streamed observations. Its 87-to-33-second pilot
result predates this audit. Those changes must not be counted as new
opportunities or multiplied by later headline ratios.

On four matched real roots, [PR206's check](https://github.com/jackmaiorino/mtg-kernel/pull/206#issuecomment-6104188369)
reports last-sampled process CPU of 5,977.203 versus 5,417.781 seconds, a
1.103x ratio. Queue elapsed time was 8,229.1 versus 8,206.8 seconds, a 1.003x
ratio. These arms shared E-core capacity, so this does not demonstrate a
reproducible elapsed-time gain. Last CPU samples omit the final interval and
are not exact process-lifetime CPU totals. The approximate mode changes
behavior and remains distinct from the exact default path.

The earlier 0.50/0.44/0.30 ms per-call fixture timings came from a V3 test
fixture. The production search policies here use the V4 path. In particular,
the fixture's forward/tensorization/encoding split does not establish the
corresponding fractions on real V4 roots. The current audit therefore does
not nominate a new activation kernel, GPU implementation, or caching scheme
on the strength of that fixture split.

## Resources and practical recommendation

The [one-shot inventory](inventory.json), at 2026-10-11 02:00:54 UTC, found no
admissible cores on either PC: the research banks held their timed cores and
the remaining desktop pool was occupied. Haley was reachable. RunPod's
read-only inventory returned HTTP 403, so cloud availability was unverified;
this task has no new paid execution authority. Existing reservations were
preserved. No new profiling launch was needed to establish this audit's
principal result.

The next implementation target should be chosen inside the real V4 policy
pipeline, using a small bounded profile that includes its currently unwrapped
direct-score calls. Use representative completed roots, fixed seeds and
budgets, compare one worker with useful parallel counts through the supported
launcher, and report whole-job elapsed time alongside CPU and stage counts.
For an exact optimization, require matching trajectories/primary output;
for changed arithmetic, retain a separate behavior identity and qualification.

This is a ranked audit result, not a measured additional speedup. It narrows
the large opportunity to policy decision work and rejects the tempting but
small duplicate-score cleanup as the main performance project.

## Reproduction and provenance

[timings.json](timings.json) contains compact cost aggregates and source hashes;
[summarize.py](summarize.py) reproduces them from the explicit retained files.
[source-map.json](source-map.json) binds source revisions and interpretation.
[capture_inventory.py](capture_inventory.py) performs a separate read-only
fleet snapshot; its availability facts expire and are not launch permission.
The report and its small evidence files are retained in Git. Original frozen
artifacts and failed/interrupted attempts remain in their owners' locations.

Runtime source examined: default `ddff546aeac448143cd1b37193357b035ccd622e`,
PR206 integration `b61c9d49c47f6ab0ad3125c4da310237ced29cd7`, retained profiling
branch `59418e4cf16b7f39e0205a032308bab4ae615f99`, and the qualified PR206 check
source `860a3d5e`. These distinct revisions are not interchangeable benchmarks.

Existing q6 qualification used eight matching root IDs, equal reported work
counts, 20,000 transitions per arm and two evaluation worlds: serial queue
elapsed time was 265.7 seconds versus 50.8 seconds at eight workers, a 5.23x
completed-root throughput ratio. This establishes useful parallelism for that
bounded workload. It is not a fresh placement qualification or a guaranteed
5.23x ratio at the formal 512,000-transition budget.

Recompute the committed cost snapshot with Python 3.13.14 from the repository:

```powershell
& D:/mtg-kernel-uv-python-019f63a2/cpython-3.13.14-windows-x86_64-none/python.exe -B `
  docs/reports/search_throughput_20261010/summarize.py `
  --formal-root D:/stage4a-20261009 `
  --fast-root D:/stage4a-fastforward-check-20261010-860a `
  --profile-root D:/stage4a-20261009/prof `
  --output docs/reports/search_throughput_20261010/timings.json --check
```

Observed validation: exact regeneration of the cost snapshot with the pinned
interpreter, source/receipt hash and row-count checks, and `git diff --check`.
A separate read-only review independently reconciled the arithmetic, source
timer coverage, interrupted-job accounting and V3/V4 distinction with no
material findings.
