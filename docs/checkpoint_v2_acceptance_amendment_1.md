# Checkpoint v2 acceptance (design 7.2): amendment 1, a floor under the relative read baseline

Status: pre-run amendment to design revision 10, section 7.2 (container PR #468, harness PR #508). No full-scale block has run. The only data cited are the reduced-scale validation receipts from the second host and the run disk's specification.

## The rule as frozen

A comparison block is contaminated if its mean run-disk (D:) reads exceed twice the D: read baseline. The baseline is the median of the surviving calibration blocks' means.

## Why it cannot work as written

The harness's own inputs (start checkpoints, opponents, play imports) are re-read every update and are mostly served from the page cache. So the baseline measures cache misses, not disk activity, and cache misses vary by orders of magnitude between clean blocks.

Validation run 2 on the second host had 4 runs, no foreign claim on the pool and no harness interruption. Its block means were:

- **32 calibration blocks:** 0.007 to 1.33 MB/s, median 0.12 MB/s. 12 of the 32 were above twice that median.
- **The first comparison group:** all four blocks read 0.86 to 1.80 MB/s, against a threshold of 0.24 MB/s.

The attempt therefore ended inconclusive on the measurement's own cache behaviour. Nothing suggests a full-scale attempt would avoid the same outcome.

## Amendment

The relative threshold becomes twice the larger of the calibration baseline and a declared floor of **10 MB/s**. That is a threshold of at least 20 MB/s, and at most the 60 MB/s absolute cap whenever the baseline is under 30 MB/s.

Nothing else changes:

- the baseline is still the median of the surviving calibration blocks, computed once and recorded in the manifest with n before the first v2 block;
- the 60 MB/s absolute cap, the E: rule, the claim rule, the missing-row rule and the start precondition are unchanged;
- the margin, the interval, the sign-flip guard, the schedule and the re-measurement rule are unchanged.

A cold-read calibration (evicting the inputs, or reading fresh copies, before every block) was considered and rejected. It would change the measured work, adding reads that production never makes, and on Windows it cannot be done reliably without privileges the harness does not hold.

## Why 10 MB/s

- **Above the measurement's own variation, with margin.** The largest clean block mean on the second host was 1.80 MB/s at 4 runs. Scaling linearly to 16 runs gives about 7.2 MB/s, which is conservative because each run's inputs are shared. The 20 MB/s threshold is about 2.8 times that.
- **Small against the device.** The production run disk is a Samsung 870 EVO 1 TB (SATA), rated at 560 MB/s sequential read. 20 MB/s is 3.6% of the rated read rate, and a third of the absolute cap. Foreign reads large enough to compete for the disk are still caught well below the cap.
- **Small against the measurement's own traffic.** Design 7.2's byte budget is about 16.4 GB written per block, about 150 MB/s over a block of about 110 s. 20 MB/s of reads is about one eighth of that.

## Effect on the decision

The amendment changes only which comparison blocks count as contaminated. It reads no contrast and no comparison data, and it is fixed before any full-scale block runs.
