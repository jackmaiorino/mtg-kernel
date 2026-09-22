# Joint gate planning: strength-only power is insufficient

Completed2026-09-22, analytic artifact calculation only. Source `python/tools/joint_gate_power_envelope_v1.py`; output `E:/mtg-meta-recovery-20260921/joint-gate-power-envelope-001.json`. No training, simulation campaign, GPU or paid work.

Retain the measured total endpoint SD .76pp per arm and sensitivity1.39pp. The primary variance result and monitor's E06 resolution were reread: a shared panel does not eliminate policy-dependent evaluation variation. No .54 substitution. As in the prior conditional calculation, assume a true2pp effect against both matched continuation and a fixed parent, Gaussian equal-SD endpoints, and two one-sidedalpha.025 tests. The strength conjunction lower bound uses the union bound; it excludes parent-panel uncertainty and unmeasured treatment variance inflation.

| SD pp | n per arm | Conditional strength lower bound | Sufficient total failure budget for ALL remaining gates to certify90% jointly |
| --- | ---: | ---: | ---: |
| .76 | 5 | 94.101% | 4.101% |
| .76 | 12 | 99.9986% | 9.9986% |
| 1.39 | 12 | 91.4957% | 1.4957% |
| 1.39 | 16 | 97.5601% | 7.5601% |
| 1.39 | 24 | 99.8237% | 9.8237% |

If S is the strength conjunction and O every remaining requirement, P(S and O) >= L(S) - P(not O). Thus the last column is a sufficient failure budget under this conservative bound, not an estimate of actual failure probability or a necessary mathematical limit. No independence between strength and other gates is assumed for that bound. Integer count rules, paired-bootstrap requirements and tactical checks must fit together within O; their errors cannot be ignored or assumed independent.

For illustration only, if the sole remaining requirement were all12 independent replicas passing a tactical check with common probability q, n12/SD1.39 would need q>=.99874497 to satisfy that sufficient bound. At q=.95, all-pass probability is .95^12=.54036009, an upper bound on the complete conjunction's power irrespective of its dependence on strength. Raising n to16 or24 reduces this particular ceiling to.44013 or.29199. The iid q model is unmeasured and not asserted for this lineage.

This does not change any completed gate, drop a failed replica, or select an aggregate tactical rule. It shows why a future rule must distinguish retaining all replicas in the analysis from requiring every replica to clear every threshold. Neither the old strength bound nor this envelope is full ADVANCE power. The actual tactical distribution, dependency structure, integer thresholds, bootstrap rule and plausible effect remain unspecified. Both natural witnesses came from observed wins; a2pp benefit is still a required useful effect, not an evidence-based expectation. No gated launch or final statistical decision is authorized; independent review remains outstanding.
