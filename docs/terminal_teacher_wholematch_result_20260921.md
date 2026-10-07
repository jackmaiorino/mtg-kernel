# Retained128 whole-match result: NO ADVANCE

The complete frozen replacement panel rejects advancement of retained128. It wins 103/256 BO3 matches against fixed g115, versus 123/256 for the matched g115 control. The paired difference is -20 wins (-7.8125 percentage points); the declared 95% paired interval is [-12.50, -3.515625] percentage points. Both strength gates fail. Preserve g115 and the existing human preview. Tactical teaching success and lower historical policy movement did not translate into stronger play on this panel.

All 512 unique matches passed final identity, output-hash, request, legal-selection and natural-terminal checks. They contain 1,204 physical games, zero drawn games, 215,766 committed decisions and 210,770 gameplay decisions. There are no missing or unresolved failed cases. The original 493/512 panel remains incomplete and uninterpreted; none of its outcomes were pooled into this replacement.

| Gate | Requirement | Result |
| --- | --- | --- |
| Completion | All 512 natural matches | PASS |
| Paired wins | At least +13/256 | FAIL: -20 |
| Paired uncertainty | Lower 95% bound above zero | FAIL: -12.50 pp |

Analysis is unchanged from the frozen plan: 20,000 NumPy PCG64 percentile bootstrap resamples, seed 2026092201, over 64 ordered matchup clusters, retaining both seeds and seats in each cluster. Paired outcomes: both win 95, both lose 125, control-only wins 28, candidate-only wins 8. This interval describes the declared panel analysis, not human or league performance.

## Deck and seat breakdown

| Candidate deck | Matches per arm | g115 wins | Retained wins | Difference |
| --- | ---: | ---: | ---: | ---: |
| Affinity | 32 | 15 | 15 | +0 |
| Burn | 32 | 13 | 10 | -3 |
| Elves | 32 | 19 | 16 | -3 |
| Faeries | 32 | 12 | 11 | -1 |
| Gates | 32 | 2 | 2 | +0 |
| Rally | 32 | 30 | 27 | -3 |
| Terror | 32 | 23 | 15 | -8 |
| Wildfire | 32 | 9 | 7 | -2 |

No candidate-deck aggregate improves. Terror contributes 8 of the 20 net lost wins, but this descriptive concentration does not establish a mechanism or justify deck-specific tuning. Physical seat 0: control 61/128, candidate 48/128. Seat 1: control 62/128, candidate 55/128. The complete 128 deck/opponent/seat rows, each with two seeds per arm, are exported to `E:/mtg-meta-recovery-20260921/teacher-wholematch-dispatch-repaired-001/matchup-seat-results.csv`. Individual cells are too small for precise matchup-strength claims.

## Runtime and evidence

Native repair source is 6d0a59ee; source checkout at launch is 45368d9e (later documentation only). Collector SHA-256: `9e9271fdc8eb3f03e381f10c05584c50ba87558c360d715c2fdb26b9a0a47f51`. Recorder dispatch follows the actual policy generation. Models, optimizer ancestry, seeds, terminal rewards, opponent, and analysis gates were unchanged. Requests explicitly use the repaired runtime and 512 MiB record budget. Full plan and binary/toolchain/model/input identities are pinned in the evidence root.

| Qualification placement | Projected 512-match seconds |
| --- | ---: |
| desktop-d-w1 | 1168.64 |
| desktop-d-w4 | 355.13 |
| desktop-d-w8 | 267.98 |
| desktop-d-w16 | 251.31 |
| desktop-d-w24 | 247.73 |
| desktop-e-w8 | 412.24 |
| desktop-c-w16 | 273.65 |
| computehost-w1 | 1841.75 |
| computehost-w4 | 689.11 |
| computehost-w8 | 572.25 |
| computehost-w16 | 556.47 |
| both-8-8 | 415.80 |
| both-24-16 | 415.23 |
| both-24-16-weight3 | 378.06 |

All 14 placements completed the identical 32-job timing workload with matching complete-output signatures. The supported launcher selected the maintainer's D drive, 24 BelowNormal workers. Qualification cases totalled 486.26 seconds of measured execution/recovery, excluding separate staging and controller inventory overhead. These sampled forecasts do not establish an exhaustive optimum.

Formal execution and recovery took 109.03 seconds, versus a 247.73-second forecast. The local worker group took 107.01 seconds and recovery 1.63 seconds. Final four-reader analysis took 34.66 seconds. Sum of per-job native wall durations was 1981.10 seconds, verification 360.59, compression 103.58; these overlap across workers and must not be added to elapsed time. Longest native job was 25.12 seconds, below the unchanged 180-second cap.

Raw result bytes: 8,379,820,539; recovered compressed job bundles: 622,612,088. Both PCs were qualified; sampled combined placements were slower. RunPod was ineligible without new spending authority and the read-only inventory returned HTTP403. No pod was allocated and no new paid compute or training was launched. Native collectors were absent after successful controller exit. Existing human sessions were preserved.

## Limits and next action

This is a uniform fixed eight-deck development pool against g115. Candidate is initial chooser and always chooses play in game one, despite coverage of both physical seats. Keep-seven opening, no learned sideboard swaps, unchanged registrations, and no search are fixed. It is not a balanced initial play/draw estimate, real-meta-frequency estimate, competent external-opponent test, or human/league evidence. The candidate is not promoted and this panel will not be extended or retuned.

The next research question is why successful terminal-target teaching changes ordinary gameplay adversely. Use a separately declared read-only diagnostic on these now-consumed traces to distinguish broad policy movement from the specific taught targeting behavior. Any new intervention requires fresh evaluation. Independently, the live human V3 action binding still rejects a shared-source stack fixture handled by V4. Repair and test that interface before a human pilot; offline replay projection is insufficient. Source audit: `E:/mtg-meta-recovery-20260921/human-live-binding-audit-20260921.md`.

Independent Fable review remains missing: the prior consultation failed HTTP429 with zero source reads until September 22 at 07:00 EDT. No repeated retry or endorsement. Proceeding under the maintainer's explicit research authority preserves this unresolved review gap. CP7 outcomes were excluded. Kimi's Escape branch remains separate and unadopted. Human competitiveness and eventual league readiness remain unproven.

Evidence root: `E:/mtg-meta-recovery-20260921/teacher-wholematch-dispatch-repaired-001`. Primary files: `plan` referenced by `completion.json`, `choice.json`, `manifest.json`, `formal-result.json`, `analysis.json`, `matchup-seat-results.csv`, and immutable job bundles.
