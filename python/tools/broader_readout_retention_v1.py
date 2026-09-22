"""Fixed disposable local fitting probe. Never exports model or optimizer."""
import hashlib,json,sys,time
from pathlib import Path
import numpy as np
import psutil
import torch
psutil.Process().nice(psutil.BELOW_NORMAL_PRIORITY_CLASS)
torch.set_num_threads(1);torch.set_num_interop_threads(1);torch.use_deterministic_algorithms(True)
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from mtg_kernel_rl.model import KernelPolicyValueNet,ModelConfig
from mtg_kernel_rl.features import EncodedDecision,FeatureSchema
B=Path('E:/mtg-meta-recovery-20260921');out=B/'broader-readout-retention-002.json'
if out.exists():raise RuntimeError('preserve existing result')
started=time.monotonic();cfg=ModelConfig()
schema=FeatureSchema(version=cfg.feature_schema_version,registry_version=cfg.feature_registry_version,contract_digest=cfg.feature_contract_digest,encoding_digest=cfg.feature_encoding_digest,state_dim=cfg.state_dim,object_feature_dim=cfg.object_feature_dim,edge_feature_dim=cfg.edge_feature_dim,action_feature_dim=cfg.action_feature_dim,object_group_count=cfg.object_group_count,action_ref_feature_dim=cfg.action_ref_feature_dim)
def tensor(x):return torch.from_numpy(np.array(x,dtype=np.uint32).view(np.float32).copy())
def encode(t):return EncodedDecision(schema=schema,**{k:(tensor(v) if k=='state' else tensor(v).reshape(-1,getattr(cfg,k[:-1]+'_dim')) if k.endswith('_features') else torch.tensor(v,dtype=torch.long)) for k,v in t.items()})

from scipy.optimize import linprog
cp=Path('D:/phase1-live/campaign-002/g/block115/run/iterations/000199/attempt-000000/update/checkpoint.json')
raw=cp.read_bytes();digest=hashlib.sha256(raw).hexdigest()
if digest!='88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1':raise RuntimeError('checkpoint mismatch')
c=json.loads(raw);model=KernelPolicyValueNet(cfg)
model.load_state_dict({p['name']:tensor(p['values']).reshape(p['shape']) for p in c['parameters']},strict=True);model.eval()
for p in model.parameters():p.requires_grad_(False)
paths=[B/'archived-gae-batch-001.json',B/'burn-tree-natural-002/on.json',B/'hand-burn-tree-natural-001/on.json']
import zipfile
roots=[];selection=[]
for slot,ref in enumerate(json.loads(paths[0].read_bytes())['episode_refs']):
 with zipfile.ZipFile(ref[0]) as z:raw_member=z.read(ref[1])
 if hashlib.sha256(raw_member).hexdigest()!=ref[2]:raise RuntimeError('archive member mismatch')
 ep=json.loads(raw_member);seat=ep['episode']['learner_seat']
 eligible=[d for d in ep['decisions'] if d['actor']==seat and d['substep_index']==0 and len(d['logits'])>1]
 indices=np.linspace(0,len(eligible)-1,min(32,len(eligible)),dtype=int)
 if len(set(indices))!=len(indices):raise RuntimeError('duplicate selection')
 selection.append(dict(slot=slot,deck=ep['episode']['selected'][seat]['label'],available=len(eligible),steps=[eligible[i]['step'] for i in indices],member_sha256=ref[2]))
 for i in indices:
  d=eligible[i];roots.append((f'retention-{slot}-{d["step"]}',d['tensor'],d['logits'],d['value']))
retention_count=len(roots)
faces=[]
for name,path in zip(('natural-bolt','natural-hand-burn'),paths[1:]):
 r=json.loads(path.read_bytes())['burn_audit']['roots'][0];record=r['record'];opponent='p0' if record['actor']=='p1' else 'p1'
 faces.append(next(i for i,a in enumerate(record['visible']['ordered_actions']) if a['target'].get('player')==opponent))
 roots.append((name,r['tensor_bits'],r['logits_bits'],r['value_bits']))
captured=[]
model.scorer[2].register_forward_pre_hook(lambda module,args:captured.append(args[0].detach().double().numpy().reshape(-1,args[0].shape[-1])))
features=[];logits=[];parity=[]
with torch.no_grad():
 for name,t,lb,vb in roots:
  captured.clear();z,v=model(encode(t));expected=tensor(lb)
  if not torch.allclose(z,expected,atol=2e-5,rtol=2e-5) or not torch.allclose(v,tensor([vb])[0],atol=2e-5,rtol=2e-5):raise RuntimeError('native parity failed '+name)
  features.append(np.concatenate(captured));logits.append(z.double().numpy());parity.append(dict(root=name,max_logit_error=float((z-expected).abs().max())))
w=model.scorer[2].weight.detach().double().numpy().reshape(-1);width=len(w)
R=np.concatenate([np.array([h[i]-h[j] for i in range(len(h)) for j in range(i)]) for h in features[:retention_count]])
patterns=[('face',faces),('incumbent_argmax',[int(z.argmax()) for z in logits[retention_count:]])]
for seed in (0,1):
 rng=np.random.default_rng(seed);patterns.append((f'random-{seed}',[int(rng.integers(len(z))) for z in logits[retention_count:]]))
results=[]
for name,targets in patterns:
 D=[];needed=[];baseline=[]
 for h,z,target in zip(features[retention_count:],logits[retention_count:],targets):
  others=[i for i in range(len(z)) if i!=target]
  D.extend(h[target]-h[others]);gaps=z[target]-z[others];needed.extend(1-gaps);baseline.append(float(min(gaps)))
 D=np.array(D);needed=np.array(needed)
 # delta=positive-negative; minimizing sum yields its L1 norm.
 sol=linprog(np.ones(2*width),A_ub=np.concatenate([-D,D],axis=1),b_ub=-needed,
     A_eq=np.concatenate([R,-R],axis=1),b_eq=np.zeros(len(R)),bounds=(0,None),method='highs',options={'time_limit':30})
 row=dict(pattern=name,targets=targets,baseline_minimum_margins=baseline,success=sol.success,status=int(sol.status),message=sol.message)
 if sol.success:
  delta=sol.x[:width]-sol.x[width:];ret=float(np.max(np.abs(R@delta)));violation=float(max(0,np.max(needed-D@delta)))
  row.update(l1=float(np.linalg.norm(delta,1)),l2=float(np.linalg.norm(delta)),relative_l2=float(np.linalg.norm(delta)/np.linalg.norm(w)),retention_max_gap_shift=ret,target_max_violation=violation,
      verified=ret<=1e-6 and violation<=1e-6)
 results.append(row)
 # Different fixed objective: minimize worst retained pairwise logit shift.
 A=np.vstack([np.column_stack([-D,D,np.zeros(len(D))]),np.column_stack([R,-R,-np.ones(len(R))]),np.column_stack([-R,R,-np.ones(len(R))])])
 q=linprog(np.r_[np.zeros(2*width),1.],A_ub=A,b_ub=np.r_[-needed,np.zeros(2*len(R))],bounds=(0,None),method='highs',options={'time_limit':30})
 trade=dict(pattern=name,objective='minimax_retention',success=q.success,status=int(q.status),message=q.message)
 if q.success:
  delta=q.x[:width]-q.x[width:2*width];t=q.x[-1];ret=float(np.max(np.abs(R@delta)));violation=float(max(0,np.max(needed-D@delta)))
  trade.update(minimax_shift=float(t),actual_max_gap_shift=ret,target_max_violation=violation,l2=float(np.linalg.norm(delta)),relative_l2=float(np.linalg.norm(delta)/np.linalg.norm(w)),verified=ret<=t+1e-6 and violation<=1e-6)
 results.append(trade)
if hashlib.sha256(cp.read_bytes()).hexdigest()!=digest:raise RuntimeError('checkpoint changed')
result=dict(complete=True,seconds=time.monotonic()-started,checkpoint_sha256=digest,inputs=[dict(path=str(p),sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for p in paths],
 selection=selection,retention_roots=retention_count,retention_singular_values=np.linalg.svd(R,compute_uv=False).tolist(),width=width,retention_constraints=len(R),retention_rank=int(np.linalg.matrix_rank(R)),baseline_readout_l2=float(np.linalg.norm(w)),parity=parity,results=results,saved_model=False,source_unchanged=True,
 non_claim='Exact finite-set geometry only. Two provisional natural targets; selected roots across ten archived episodes. No exported displacement, optimizer, training label, policy intervention, generalization or strength claim.')
out.write_text(json.dumps(result,indent=2,allow_nan=False));print(json.dumps({k:v for k,v in result.items() if k not in ('inputs','parity','selection','retention_singular_values')},indent=2))
