"""Analytic planning envelope; not complete gate power or launch authority."""
import json,math
from pathlib import Path
from scipy.stats import t,nct
rows=[]
for sd in (.76,1.39):
 for n in (5,12,16,24):
  effect=2.
  a=float(nct.sf(t.ppf(.975,2*n-2),2*n-2,effect/(sd*math.sqrt(2/n))))
  b=float(nct.sf(t.ppf(.975,n-1),n-1,effect/(sd/math.sqrt(n))))
  strength_lower=max(0.,a+b-1);remaining=strength_lower-.90
  rows.append(dict(sd_pp=sd,n_per_arm=n,effect_pp=effect,strength_lower=strength_lower,
    other_failure_budget_for_union_bound_90=max(0.,remaining),can_certify_90_with_this_bound=remaining>=0,
    sufficient_joint_other_pass_probability=1-remaining if remaining>=0 else None,
    sufficient_iid_per_replica_tactical_q_if_only_missing_gate=(1-remaining)**(1/n) if remaining>=0 else None,
    all_replica_pass_at_q095=.95**n,joint_upper_at_iid_q095=min(a,b,.95**n)))
result=dict(complete=True,rows=rows,assumptions=['Same conditional Gaussian strength model as prior planning: true2pp vs both control and fixed parent; equal arm SD; one-sidedalpha.025.',
 'Strength lower bound omits parent-panel uncertainty and uncalibrated treatment variance.',
 'Union failure budget needs no independence between gate components. Per-replica q exponent assumes iid tactical successes only.',
 'Integer wins, paired bootstrap, tactical distribution and dependence are unspecified; full ADVANCE power is NOT established.'],independent_training_runs_executed=0)
p=Path('E:/mtg-meta-recovery-20260921/joint-gate-power-envelope-001.json')
with p.open('x') as f:json.dump(result,f,indent=2)
for r in rows:print(json.dumps(r))
