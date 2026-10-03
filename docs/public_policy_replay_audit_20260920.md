# Same-decision policy audit, September20

Complete:112 archived trajectories selected by schedule across56 own-deck/pre-postboard/seat/opponent-model strata in each source arm. Each stratum contributes its first scheduled episode, no terminal outcome selection. There are5,960 scored unforced choice rows and3,079 distinct actor tensor hashes; these are not independent statistical samples. At most64 evenly spaced choice rows per trajectory.

| Comparison | Changed top action | Mean forward KL, nats | Mean total variation |
|---|---:|---:|---:|
| g115 to control | 281/5960 (4.71%) | 0.0603 | 0.0484 |
| g115 to structured | 215/5960 (3.61%) | 0.0324 | 0.0326 |
| control to structured | 376/5960 (6.31%) | 0.1196 | 0.0625 |

The feature endpoint is closer to the parent on average than the control, yet had the worse complete BO3 result. Aggregate policy drift therefore does not identify the cause or justify a retention penalty by itself. Small average drift can still contain important tactical changes. There is no new strength estimate or candidate nomination here.

**Concrete public target check:** among4distinct Timberwatch Elf target-choice tensors (8archive rows), parent and control both prefer self-controlled creatures in all4. The feature endpoint instead prefers an opponent-controlled Faerie Seer in1case with80.38% probability. In that same state, the parent prefers its own Elvish Mystic (33.00%); control prefers its own Timberwatch Elf (45.76%). Controller bits were decoded from the frozen native action-reference feature layout, not inferred solely from card/deck names. This is an exploratory warning sign on a development state, not a prevalence estimate or full-board tactical proof. No legal opponent target was removed or hardcoded out. Details: timberwatch-target-audit.json.

Other largest-change examples include moving from casting Krark-Clan Shaman to passing in Affinity/Wildfire and from Lava Dart to passing in Burn. The saved action kind and known card references alone do not prove those choices are wrong; full board/timing context is required. public-choice-examples.json is explicitly exploratory and does not select a future checkpoint.

Qualification:340 same-state model-row replays exactly reproduce saved f32 logit/value bits. Duplicate model copies produce identical scores and exactlyzero KL/TV. A one-bit corrupted saved value is rejected. Perturbing unused archive deck order and terminal rewards leaves score rows unchanged; this tests unused metadata, not a fresh engine hidden-state invariance theorem. Actual distributions use the frozen Hamilton-apportioned sampler masses converted to f64, with no sampler draws or optimizer updates. One/four-worker minibatch outputs match byte-for-byte.

The initial8trajectory timing (1.57seconds serial,1.68four workers) was dominated by setup and understated full-panel scaling. The full112trajectory follow-up is12.57seconds serial,7.40four workers,7.12eight workers; all113output files match exactly at each count. This is1.76x full-panel speedup, single execution per count, no remote placement claim. Future scoring panels of this size should use the representative full-panel evidence, not the tiny startup-dominated timing. Follow-up: E:/mtg-meta-recovery-20260920/public-policy-replay-scaling-001/result.json.

Native source9dc7ffef; runner eaf98151. Executable SHA2845ba5ff7be6af73803f21df85205f6e827fa4f26dfb87ae02ccba15fe86445. Release build115.78seconds with4BelowNormaljobs; primaryqualification/panel21.05seconds, follow-upscoring14.52seconds plus file verification. All processes completed. CPU scoring only; no new training, games or paid compute. Manifest, exact requests and execution receipts remain in this root.

Limits: development training states selected early (latest selected update23); both source-arm panels have many identical tensors. Row weighting favors longer/macro-decision trajectories up to the cap. No independent opponent/human evidence and no causal attribution of lost games follows. The full public-feature pilot remainsNO-ADVANCE. Fable zero-readHTTP429 review gap remains untilSep22 07:00EDT.
