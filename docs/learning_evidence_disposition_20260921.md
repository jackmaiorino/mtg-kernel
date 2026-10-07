# Learning evidence and the next coverage question

The retained128 candidate failed its frozen whole-match gate: 103/256 versus the parent's 123/256. The first-update native counterfactual and full targeting census now narrow useful follow-up work. Preserve the sampled g115 reference, complete optimizer ancestry and all consumed measurements. No successor is promoted here.

Kimi's source/artifact audits at kimi/training-variance-v1 commit 4a3cc4c7 provide useful diagnostics. The following distinctions matter before choosing a learning intervention:

| Evidence | Supported use | Unsupported inference |
| --- | --- | --- |
| Broadly monotone pooled value bins versus observed returns | Describe predictive calibration on the behavior distribution | Treat V as an action-quality oracle or a -0.15 transition as a causal 7.5-point loss |
| Fewer consecutive-actor value drops after 200 updates | Describe changing value self-consistency and state visitation | Attribute each drop to the preceding decision, or prove improved play |
| Low selected probability or low policy rank | Describe sampled behavior relative to its own distribution | Label a sampled move as a mistake |
| Endpoint displacement anti-aligned with initial Adam momentum | Compare two endpoint vectors | Exclude all momentum-mediated effects across 128 nonlinear adaptive updates |
| Exactly reproduced zero-gradient value-head updates | Establish inherited-moment movement in those tensors | Call the readout unchanged from g115, or attribute policy regression to that readout |
| Correct-label versus self-target-label checkpoints | Compare those two complete trained interventions | Uniquely separate label correctness, synthetic distribution and trunk interference |

Source details: blunder_index_audit_v1.py includes all decision rows in the calibration accumulator despite its choice-only comment. Its collapse count per game is not normalized by eligible transitions, and excluding opponent decisions does not exclude chance, automatic engine events or information revelation. These do not invalidate the reported counts; they limit the interpretation. Even perfect state-value calibration does not identify an unobserved alternative action's return. The teaching value head is identical across the four arms, not identical to the original parent; our exact recurrence and native first-step probe make that distinction directly.

A simple counterexample makes the action-quality limitation concrete. Suppose a policy selects action A with probability 0.9 and B with probability 0.1, while their actual win probabilities are 0.1 and 0.9. Its overall win probability is 0.18, so a state value of -0.64 is perfectly calibrated for terminal rewards in {-1,+1}. Nevertheless its highest-ranked action is the worse action. Similarly, revealing an unlucky chance outcome can lower a correct value estimate without an opponent decision or a bad preceding move. Neither policy rank nor a consecutive value drop supplies the missing counterfactual comparison.

Our first-update mean policy movement is about ten times larger under the actual update than the zero-gradient update on the fixed retention inputs. This supports a bounded first-step statement. It is not an additive fraction of a 128-update treatment effect. Neither this nor endpoint cosine supports an optimizer reset or a claim that all regression mechanisms are identified.

The inference-temperature shortcut has already been tested in the separate September 20 supported-root greedy screen: raw 40/72, greedy 36/72, paired interval [-13.89,+2.78] points, INCONCLUSIVE. That intervention covered only supported roots and an older domain. Preserve the result without treating it as a global greedy theorem or rerunning it as if untested.

Next question: do ordinary complete games expose public combat or other action families suitable for broader terminal-grounded teaching? The old natural terminal auditor explicitly restricts its roots to four burn target spells and an empty opposing hand. Its low coverage therefore does not establish absence of natural tactical opportunities elsewhere. Before building another teacher, measure public menu/hand/stack coverage across all eight decks using the complete parent traces. These counts are engineering opportunity measures, not certified tactical labels. A generalized prover would still need adversarial continuation handling, unknown-information boundaries, actual V4 replay and tests that unresolved branches never become losses. Do not remove the opponent-hand guard and clone true hidden information into a teacher.

The distribution concern has a primary-literature basis: [Ross, Gordon and Bagnell (2011)](https://proceedings.mlr.press/v15/ross11a.html) explain why actions change the subsequent input distribution and motivate collecting supervision on visited states. Their DAgger result assumes an expert and does not supply one for this game. This project still has to establish useful, legal-information labels and demonstrate fresh whole-match improvement. The paper motivates the coverage question, not a performance prediction.

Fable's known zero-read HTTP429 lasts until September 22 at 07:00 EDT. No new consultation is claimed. This bounded, read-only preparation proceeds under the maintainer's execution assignment, with independent design review still outstanding before a major learning decision. No paid compute, broad training campaign, changed rewards or CP7-based selection is authorized by this note.
