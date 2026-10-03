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
B=Path('E:/mtg-meta-recovery-20260921');out=B/'ephemeral-trainability-001.json'
if out.exists():raise RuntimeError('preserve existing result')
started=time.monotonic();cfg=ModelConfig()
schema=FeatureSchema(version=cfg.feature_schema_version,registry_version=cfg.feature_registry_version,contract_digest=cfg.feature_contract_digest,encoding_digest=cfg.feature_encoding_digest,state_dim=cfg.state_dim,object_feature_dim=cfg.object_feature_dim,edge_feature_dim=cfg.edge_feature_dim,action_feature_dim=cfg.action_feature_dim,object_group_count=cfg.object_group_count,action_ref_feature_dim=cfg.action_ref_feature_dim)
def tensor(x):return torch.from_numpy(np.array(x,dtype=np.uint32).view(np.float32).copy())
def encode(t):return EncodedDecision(schema=schema,**{k:(tensor(v) if k=='state' else tensor(v).reshape(-1,getattr(cfg,k[:-1]+'_dim')) if k.endswith('_features') else torch.tensor(v,dtype=torch.long)) for k,v in t.items()})
ip=B/'natural-hand-channel-input-001/input.json';inputs=json.loads(ip.read_bytes())['records'];encoded=[encode(x['row']['tensor']) for x in inputs]
if len(encoded)!=8:raise RuntimeError('wrong fixed root count')
def center(z):return z-z.mean()
perturbations={}
for seed in (0,1,2):
    generator=torch.Generator().manual_seed(seed);deltas=[]
    for e in encoded:
        v=center(torch.randn(e.action_features.shape[0],generator=generator));v=.1*v/v.square().mean().sqrt();deltas.append(v)
    perturbations[seed]=deltas
results=[];pins=[]
for block in (110,115):
    path=Path(f'D:/phase1-live/campaign-002/g/block{block}/run/iterations/000199/attempt-000000/update/checkpoint.json')
    raw=path.read_bytes();digest=hashlib.sha256(raw).hexdigest();c=json.loads(raw);pins.append(dict(path=str(path),sha256=digest))
    if block==115 and digest!='88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1':raise RuntimeError('g115 differs')
    params={p['name']:tensor(p['values']).reshape(p['shape']) for p in c['parameters']}
    for scope in ('all_policy','final_readout'):
        for seed in (0,1,2):
            model=KernelPolicyValueNet(cfg);model.load_state_dict(params,strict=True)
            for name,p in model.named_parameters():p.requires_grad_(not name.startswith('value_head.') if scope=='all_policy' else name.startswith('scorer.2.'))
            active=[p for p in model.parameters() if p.requires_grad];optimizer=torch.optim.SGD(active,lr=.001)
            with torch.no_grad():
                baseline=[model(e)[0] for e in encoded]
                if block==115:
                    for z,r in zip(baseline,inputs):
                        if not torch.allclose(z,tensor(r['row']['logits']),atol=2e-5,rtol=2e-5):raise RuntimeError('native parity failed')
                targets=[center(z)+d for z,d in zip(baseline,perturbations[seed])]
            trace=[]
            for step in range(21):
                if time.monotonic()-started>90:raise RuntimeError('fixed probe deadline exceeded; no automatic rerun')
                losses=[(center(model(e)[0])-target).square().mean() for e,target in zip(encoded,targets)]
                fit=torch.stack(losses[:6]).mean();held=torch.stack(losses[6:]).mean()
                trace.append(dict(step=step,fit=float(fit.detach()),held=float(held.detach())))
                if not torch.isfinite(fit):break
                if step<20:optimizer.zero_grad();fit.backward();optimizer.step()
            with torch.no_grad():movement=sum(float((p-params[n]).double().square().sum()) for n,p in model.named_parameters())**.5
            results.append(dict(block=block,scope=scope,seed=seed,trace=trace,parameter_l2_movement=movement,fit_reduction=1-trace[-1]['fit']/trace[0]['fit']))
            del optimizer,model
for pin in pins:
    if hashlib.sha256(Path(pin['path']).read_bytes()).hexdigest()!=pin['sha256']:raise RuntimeError('source checkpoint mutated')
result=dict(complete=True,seconds=time.monotonic()-started,checkpoints=pins,input_sha256=hashlib.sha256(ip.read_bytes()).hexdigest(),results=results,
    target_seed_count=3,strength_training_replications=0,updates_per_copy=20,source_checkpoints_unchanged=True,saved_model=False,
    limits='Arbitrary local residual fit on six states, two held out from same episode. Not tactical labels, independent training seeds, optimizer-history test, generalization or strength. No reset/repair selected.')
out.write_text(json.dumps(result,indent=2,allow_nan=False))
for r in results:print(r['block'],r['scope'],r['seed'],round(r['fit_reduction'],6),round(r['trace'][-1]['held'],6))
print('seconds',result['seconds'])
