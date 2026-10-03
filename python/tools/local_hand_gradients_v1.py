"""Fixed-root, frozen-model local gradients and tanh activation diagnostics."""
import hashlib,json,sys
from pathlib import Path
import numpy as np
import torch
torch.set_num_threads(1);torch.set_num_interop_threads(1)
torch.use_deterministic_algorithms(True)
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from mtg_kernel_rl.model import KernelPolicyValueNet,ModelConfig
from mtg_kernel_rl.features import EncodedDecision,FeatureSchema
B=Path('E:/mtg-meta-recovery-20260921');out=B/'local-hand-gradients-001.json'
assert not out.exists()
checkpoint=Path('D:/phase1-live/campaign-002/g/block115/run/iterations/000199/attempt-000000/update/checkpoint.json')
raw=checkpoint.read_bytes();assert hashlib.sha256(raw).hexdigest()=='88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1'
c=json.loads(raw);cfg=ModelConfig();model=KernelPolicyValueNet(cfg)
def tensor(bits):return torch.from_numpy(np.array(bits,dtype=np.uint32).view(np.float32).copy())
params={p['name']:tensor(p['values']).reshape(p['shape']) for p in c['parameters']}
model.load_state_dict(params,strict=True);model.eval()
for p in model.parameters():p.requires_grad_(False)
schema=FeatureSchema(version=cfg.feature_schema_version,registry_version=cfg.feature_registry_version,
    contract_digest=cfg.feature_contract_digest,encoding_digest=cfg.feature_encoding_digest,
    state_dim=cfg.state_dim,object_feature_dim=cfg.object_feature_dim,edge_feature_dim=cfg.edge_feature_dim,
    action_feature_dim=cfg.action_feature_dim,object_group_count=cfg.object_group_count,action_ref_feature_dim=cfg.action_ref_feature_dim)
def encoded(t):
    kwargs={}
    for k,v in t.items():
        if k=='state':kwargs[k]=tensor(v).requires_grad_()
        elif k.endswith('_features'):kwargs[k]=tensor(v).reshape(-1,getattr(cfg,k[:-1]+'_dim'))
        else:kwargs[k]=torch.tensor(v,dtype=torch.long)
    return EncodedDecision(schema=schema,**kwargs)
paths=[B/'hand-channel-diagnostic-001/first/result.json',B/'natural-hand-channel-input-001/input.json',B/'burn-recorder-natural-002/on.json']
controls=json.loads(paths[0].read_bytes());roots=[]
for r in controls['records']:
    if r['actor']==0 and not r['reverse']:
        s=r['signature'];roots.append((f"constructed-{r['has_second_bolt']}",s['tensor'],s['logits'],s['value_bits'],r['face_index'],r['removal_index']))
natural=json.loads(paths[1].read_bytes())
for i,r in enumerate(natural['records']):
    d=r['row'];z=tensor(d['logits']);order=torch.argsort(z,descending=True)
    roots.append((f'natural-channel-{i}',d['tensor'],d['logits'],d['value'],int(order[0]),int(order[1])))
r=json.loads(paths[2].read_bytes())['burn_audit']['roots'][0]
face=next(i for i,a in enumerate(r['record']['visible']['ordered_actions']) if a['target'].get('player')=='p0')
roots.append(('natural-bolt',r['tensor_bits'],r['logits_bits'],r['value_bits'],face,r['record']['behavior']['selected_index']))
assert len(roots)==11
activation={};embedding=[]
def embed_hook(module,args,output):
    output=output.detach().requires_grad_();embedding.append(output);return output
model.card_embedding.register_forward_hook(embed_hook)
for name,module in model.named_modules():
    if isinstance(module,torch.nn.Tanh):
        module.register_forward_hook(lambda m,a,o,n=name:activation.setdefault(n,[]).append(o.detach().reshape(-1,o.shape[-1])))
def metrics(g,x):
    p=g*x
    return dict(elements=g.numel(),gradient_l1=float(g.abs().sum()),gradient_l2=float(g.norm()),signed_grad_input=float(p.sum()),absolute_grad_input=float(p.abs().sum()))
results=[]
for name,t,lb,vb,a,b in roots:
    embedding.clear();e=encoded(t);z,v=model(e);expected=tensor(lb);ev=tensor([vb])[0]
    assert torch.allclose(z,expected,atol=2e-5,rtol=2e-5) and torch.allclose(v,ev,atol=2e-5,rtol=2e-5),(name,z.tolist(),expected.tolist(),float(v),float(ev))
    assert len(embedding)==1
    g,s=torch.autograd.grad(z[a]-z[b],(embedding[0],e.state));hand=e.object_groups==0
    results.append(dict(root=name,target_indices=[a,b],target=float(z[a]-z[b]),max_logit_error=float((z-expected).abs().max()),
        hand=metrics(g[hand],embedding[0][hand]),other_objects=metrics(g[~hand],embedding[0][~hand]),
        digest=metrics(s[123:219],e.state[123:219]),other_state=metrics(torch.cat([s[:123],s[219:]]),torch.cat([e.state[:123],e.state[219:]]))))
layers={}
for name,parts in activation.items():
    x=torch.cat(parts);unit=x.abs().mean(0);sv=torch.linalg.svdvals(x);total=sv.sum()
    rank=int(torch.searchsorted(sv.cumsum(0),.99*total))+1 if total>0 else 0
    layers[name]=dict(rows=x.shape[0],width=x.shape[1],mean_abs=float(unit.mean()),low_activity_units=int((unit<.01*unit.mean()).sum()),
        saturated_fraction=float((x.abs()>.99).float().mean()),singular_mass_rank99=rank)
result=dict(complete=True,checkpoint_sha256=hashlib.sha256(raw).hexdigest(),torch_version=torch.__version__,roots=results,layers=layers,
    inputs=[dict(path=str(p),sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for p in paths],
    limits='Local gradients in embedding coordinates, not categorical derivatives, integrated gradients or historical causal attribution. Eleven selected roots, tanh low activity not ReLU dormancy, no trainability or strength claim.')
out.write_text(json.dumps(result,indent=2))
print(json.dumps(dict(roots=len(results),maximum_error=max(r['max_logit_error'] for r in results),layers=layers),indent=2))
