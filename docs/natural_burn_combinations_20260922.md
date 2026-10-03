# Natural burn motif census, 2026-09-22

Read-only diagnostic, not an experimental gate. No new games, training, promotion or independent training replication. g115 unchanged; M1 unmet.

All 256 already consumed g115 control matches were read, including both actors: 600 games and 106,819 gameplay decisions. Archive and raw-result hashes verified. Four families: Lightning Bolt, Lava Dart, Fireblast and Galvanic Blast, the last optimistically assigned 4 damage. Serial reading took 42.993 seconds and four readers 43.291 seconds, with exact result agreement over 4,324,390,430 uncompressed bytes. Serial was slightly faster. This artifact-reader comparison does not qualify a training allocation.

| Filter | Records |
| --- | ---: |
| Burn target menus | 953 |
| Another burn card in own hand | 288 |
| Also opponent hand empty | 15 |
| Nominal hand-dependent lethal damage sum | 5 |
| Nominal immediate lethal damage | 3 |

These are candidates, not certified errors. The filter ignores costs, prevention, public responses and existing stack effects. It covers only these four families and requires another burn card, so it is not a comprehensive immediate-lethal census. All 15 empty-hand candidates remain in the output, including seven outside the nominal lethal filters.

Inspection retained all eight nominal candidates. Two cell45 records have exactly equal visible JSON after deleting only step_index, physical_decision_id, substep_index and substep_count, including equal action probabilities. Thus eight records give seven distinct counter-free snapshots, not eight independent examples. Different hashes do not establish statistical independence.

First engineering replay target, selected before engine branch outcomes: lexicographically first nominal immediate candidate selecting a non-face action, cell25-r0-s0-g115, game0, decision242, actor p1, physical236, turn11 Main1. Opponent has 2 life and empty hand; Lightning Bolt targets Humbling Elder with behavior probability .9957185223, while the available face target receives .0042812536. Public battlefield includes untapped Islands and Humbling Elder, so an empty hand must not be equated with no response. Use the existing public burn oracle and preserve abstentions. No natural win is certified yet. Keep cell32's selected-face root as a control; retain all remaining candidates and failures.

Evidence: E:/mtg-meta-recovery-20260921/natural-burn-combinations-001.json and natural-burn-candidate-inspection-001.json. Source: python/tools/read_natural_burn_combinations_v1.py and inspect_natural_burn_candidates_v1.py. Existing BO3 recorder exposes combat audit, while paired BO1 policy input exposes audit_live_burn_targets_v1; integration/replay verification remains necessary before claiming a natural terminal witness.

Monitor E-20260922-01 adds attribution, plasticity and credit audits before intervention selection. These remain pending. No repair lever selected, no causal mechanism established, and the conditional proposal remains non-launchable.
