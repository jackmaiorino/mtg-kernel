# Natural-game terminal shadow census

All 128 fixed development BO3 matches completed, comprising 292 natural games and 33,537 V4 decisions. The opt-in diagnostic reproduced every original corrected-engine match record after removing its added audit field and normalizing only the independently verified V3 build-envelope hash. No model weights, policy choices, seeds or rewards changed. Source 5930283f; evaluator SHA256 1fa29c5b8d4da9f9fc9c83355be9de7907ed44b870d6be6a478945b46db45b20.

| Coverage | Count |
| --- | ---: |
| Unsupported target decision | 32,510 |
| Menu outside bounds | 779 |
| Opponent hand nonempty | 211 |
| Admitted roots | 37 |
| Counterfactual branches | 213 |
| Certified winning branches | 1 |
| Certified losing branches | 2 |
| Unresolved branches | 210 |
| Available winning lines not selected | 1 |

The single missed immediate win occurred in the Rally mirror, candidate seat 1, game 1, turn 6 Main1, physical decision 73 / step 87. Both hands were empty; opponent life was 1 and candidate life 3. Lightning Bolt targeting the opponent (index 0) produced a natural win in one step. The policy selected the opposing Samurai Token (index 4); this branch crossed the diagnostic information boundary. The unchanged policy subsequently won that game. This is evidence of a missed immediate finish, not an observed lost game or a proven disadvantage in eventual terminal return. The checkpoint uses gamma=1, so equally winning continuations need not receive different terminal returns merely because one is faster.

Coverage was concentrated: Burn 26 admitted roots, Affinity 6, Rally 5; the other five decks had none. Seat 0 had 21 roots and seat 1 had 16. Every own deck has 16 matches and each seat 64, but this is only one seed per ordered deck pair against one V3 opponent. These data cannot establish a population tactical-error rate. The witness follows a bounded available plan; unresolved continuations are never losses. This shadow implementation is not a deployed fair-information search policy, and synthetic hidden-order invariance does not prove invariance for every natural state.

Engineering validation exercised the same shadow path on 16 synthetic distractor records and two nonempty-hand abstention checks. Four fixed natural cases, each with audit off/on/fresh replay and full decision traces, preserved actions and results exactly; replay bytes matched. Those four natural cases had no admitted branches, so positive-path validation there came from the synthetic controls. The full census supplied 37 natural admitted roots.

The guarded launcher compared seven placements: The maintainer 1/8/24 workers, the compute host 1/8/16, and both PCs. All 224 qualification executions matched byte-for-byte for their common inputs. The maintainer's D SSD with 24 workers was fastest including transfer and recovery, with a 46.80-second full-panel forecast. Qualification cost 303.83 seconds plus 3.98 seconds initial remote staging; full-panel staging/native/recovery took 3.57/12.81/3.40 seconds. Qualification cost more than the diagnostic itself; retain compatible evidence for subsequent launches. RunPod's recent authenticated inventory returned 403; no paid allocation occurred. Final inventories show no active owned worker on either PC and preserve seven idle human sessions.

Evidence: E:/mtg-meta-recovery-20260921/public-terminal-shadow-census-001/analysis.json, REPORT.md, completion.json and owners-after.json; compute evidence in public-terminal-shadow-compute-001/compute-choice.json; engineering in public-terminal-shadow-engineering-001/completion.json. Launcher terminal-shadow-census.py enforces the pinned completed-work qualification; no claim is made that every legacy launcher has this guard.

Next distinguish a missed finish from a genuinely inferior terminal outcome using a predeclared public combat control where face damage wins but removing either of two lethal attackers loses. Require actual natural engine witnesses, legal mapping, both seats and hidden-state invariance before scoring. This is a proposed bounded correctness diagnostic, not an approved broad training campaign or a demonstrated teacher advantage. Keep terminal rewards and CP7 exclusion unchanged. Fable's recorded zero-read HTTP429 review gap lasts until September 22 at 07:00 EDT; no endorsement is claimed. Human and league competence remain unproven.

Kimi's observation-sufficiency audit remains a source of representation hypotheses. The subsequent structured stack-feature screen failed its matched-control advancement gates; do not repeat or promote it solely because the audit identifies a real representation gap. Escape commit f74d2784 remains separate and unadopted.
