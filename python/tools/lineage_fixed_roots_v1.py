"""Read-only fixed-input checkpoint comparison; no optimizer or candidate."""
import hashlib,json,sys
from pathlib import Path
import numpy as np
import torch
torch.set_num_threads(1);torch.set_num_interop_threads(1);torch.use_deterministic_algorithms(True)
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from mtg_kernel_rl.model import KernelPolicyValueNet,ModelConfig
from mtg_kernel_rl.features import EncodedDecision,FeatureSchema
B=Path('E:/mtg-meta-recovery-20260921');out=B/'lineage-fixed-roots-001.json';assert not out.exists()
cfg=ModelConfig();model=KernelPolicyValueNet(cfg)
schema=FeatureSchema(version=cfg.feature_schema_version,registry_version=cfg.feature_registry_version,contract_digest=cfg.feature_contract_digest,encoding_digest=cfg.feature_encoding_digest,state_dim=cfg.state_dim,object_feature_dim=cfg.object_feature_dim,edge_feature_dim=cfg.edge_feature_dim,action_feature_dim=cfg.action_feature_dim,object_group_count=cfg.object_group_count,action_ref_feature_dim=cfg.action_ref_feature_dim)
def tensor(x):return torch.from_numpy(np.array(x,dtype=np.uint32).view(np.float32).copy())
def encode(t):
    return EncodedDecision(schema=schema,**{k:(tensor(v) if k=='state' else tensor(v).reshape(-1,getattr(cfg,k[:-1]+'_dim')) if k.endswith('_features') else torch.tensor(v,dtype=torch.long)) for k,v in t.items()})
paths=[B/'hand-channel-diagnostic-001/first/result.json',B/'natural-hand-channel-input-001/input.json',B/'burn-recorder-natural-002/on.json']
roots=[]
for r in json.loads(paths[0].read_bytes())['records']:
    if r['actor']==0 and not r['reverse']:
        s=r['signature'];roots.append((f"constructed-{r['has_second_bolt']}",s['tensor'],s['logits'],s['value_bits'],r['face_index'],r['removal_index']))
for i,r in enumerate(json.loads(paths[1].read_bytes())['records']):
    d=r['row'];order=torch.argsort(tensor(d['logits']),descending=True)
    roots.append((f'natural-channel-{i}',d['tensor'],d['logits'],d['value'],int(order[0]),int(order[1])))
r=json.loads(paths[2].read_bytes())['burn_audit']['roots'][0]
face=next(i for i,a in enumerate(r['record']['visible']['ordered_actions']) if a['target'].get('player')=='p0')
roots.append(('natural-bolt',r['tensor_bits'],r['logits_bits'],r['value_bits'],face,r['record']['behavior']['selected_index']))
activation={}
for name,module in model.named_modules():
    if isinstance(module,torch.nn.Tanh):module.register_forward_hook(lambda m,a,o,n=name:activation.setdefault(n,[]).append(o.detach().reshape(-1,o.shape[-1])))
results=[]
for block in [110,114,115]:
    path=Path(f'D:/phase1-live/campaign-002/g/block{block}/run/iterations/000199/attempt-000000/update/checkpoint.json')
    raw=path.read_bytes();c=json.loads(raw)
    if block==115:assert hashlib.sha256(raw).hexdigest()=='88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1'
    params={p['name']:tensor(p['values']).reshape(p['shape']) for p in c['parameters']};model.load_state_dict(params,strict=True);model.eval();activation.clear()
    norms={}
    for name,p in params.items():norms[name.split('.')[0]]=norms.get(name.split('.')[0],0.)+float(p.double().square().sum())
    norms={k:v**.5 for k,v in norms.items()};outputs=[]
    with torch.no_grad():
        for name,t,lb,vb,a,b in roots:
            z,v=model(encode(t))
            if block==115:assert torch.allclose(z,tensor(lb),atol=2e-5,rtol=2e-5) and torch.allclose(v,tensor([vb])[0],atol=2e-5,rtol=2e-5)
            outputs.append(dict(root=name,contrast=float(z[a]-z[b]),value=float(v),argmax=int(z.argmax())))
    layers={}
    for name,parts in activation.items():
        x=torch.cat(parts);unit=x.abs().mean(0);sv=torch.linalg.svdvals(x)
        layers[name]=dict(rows=x.shape[0],low_activity_units=int((unit<.01*unit.mean()).sum()),saturation=float((x.abs()>.99).float().mean()),rank99=int(torch.searchsorted(sv.cumsum(0),.99*sv.sum()))+1)
    results.append(dict(block=block,checkpoint=dict(path=str(path),sha256=hashlib.sha256(raw).hexdigest()),adam_step=c['adam_step'],norms=norms,outputs=outputs,layers=layers))
out.write_text(json.dumps(dict(complete=True,checkpoints=results,inputs=[dict(path=str(p),sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for p in paths],limits='Fixed g115-selected roots, not representative older-policy visitation. g115 native parity checked; older checkpoints evaluated in same Torch reference, no older native parity claim. Observational weight/activation trend only, no trainability/causal/strength claim.'),indent=2))
for r in results:print(json.dumps(dict(block=r['block'],adam_step=r['adam_step'],norms=r['norms'],contrasts=[x['contrast'] for x in r['outputs'] if not x['root'].startswith('natural-channel')],scorer=r['layers']['scorer.1'],state=r['layers']['state_encoder.1'])))
