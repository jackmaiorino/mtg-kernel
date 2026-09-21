"""Independent CPU autograd reference, synthetic probes only, no learning result."""
import argparse,copy,hashlib,io,json,pathlib,sys
import torch
from torch import nn
from torch.nn import functional as F

p=argparse.ArgumentParser();p.add_argument('--fixture',type=pathlib.Path,required=True);p.add_argument('--output',type=pathlib.Path,required=True);a=p.parse_args()
repo=pathlib.Path(__file__).resolve().parents[2];sys.path.insert(0,str(repo/'python'))
from mtg_kernel_rl import features_v7 as features
from mtg_kernel_rl.model import _apply_rowwise
from mtg_kernel_rl.public_feature_model_v1 import import_checkpoint
torch.set_num_threads(2);torch.set_num_interop_threads(1)
fixture=json.loads(a.fixture.read_bytes());assert fixture['schema']=='public-stack-g115-reference/v1'
contract=repo/'data/public_stack_features_v1/contract.json'
contract_sha=hashlib.sha256(contract.read_bytes()).hexdigest()
base,optimizer,checkpoint=import_checkpoint(fixture['checkpoint'],fixture['checkpoint_sha256'],False)

def encoded(raw):
 c=base.config
 schema=features.FeatureSchema(c.feature_schema_version,c.feature_registry_version,c.feature_contract_digest,c.feature_encoding_digest,c.state_dim,c.object_feature_dim,c.edge_feature_dim,c.action_feature_dim,c.object_group_count,c.action_ref_feature_dim)
 floats={'state':None,'object_features':c.object_feature_dim,'edge_features':c.edge_feature_dim,'action_features':c.action_feature_dim,'action_ref_features':c.action_ref_feature_dim}
 fields={}
 for key,value in raw.items():
  fields[key]=torch.tensor(value,dtype=torch.float32 if key in floats else torch.long)
  if key in floats and floats[key] is not None:fields[key]=fields[key].reshape(-1,floats[key])
 result=features.EncodedDecision(schema,**fields);base._validate_encoded(result);return result

def forward(model,weight,e,stack):
 # Explicit graph extension; no hooks, mutation of legacy forward, or hidden data.
 objects=model.object_encoder(torch.cat([e.object_features,model.card_embedding(e.object_card_ids)],-1))
 pooled=torch.zeros_like(objects)
 if len(e.edge_features):
  edges=model.edge_encoder(torch.cat([e.edge_features,objects[e.edge_source_indices],objects[e.edge_target_indices]],-1))
  pooled.index_add_(0,e.edge_source_indices,edges);pooled.index_add_(0,e.edge_target_indices,edges)
 # Sequential row additions match the declared native message order.
 for row in stack['rows']:
  source,target=row['source_node'],row['target_node']
  inputs=torch.cat([torch.tensor(row['features'],dtype=objects.dtype),objects[source],objects[target] if target is not None else torch.zeros_like(objects[source])])
  message=F.linear(inputs,weight).tanh()
  index=torch.tensor([source] if target is None or target==source else [source,target])
  pooled=pooled.index_add(0,index,message.unsqueeze(0).expand(len(index),-1))
 # Retain a connected zero derivative for an empty stack, without phantom rows.
 if not stack['rows']:pooled=pooled+weight.sum()*0
 nodes=model.node_update(torch.cat([objects,pooled],-1))
 groups=nodes.new_zeros((model.config.object_group_count,model.config.hidden_dim));groups.index_add_(0,e.object_groups,nodes)
 state=model.state_encoder(torch.cat([e.state,groups.reshape(-1)]))
 refs=nodes.new_zeros((len(e.action_features),model.config.hidden_dim))
 if len(e.action_ref_features):
  refs.index_add_(0,e.action_ref_action_indices,_apply_rowwise(model.action_ref_encoder,torch.cat([e.action_ref_features,nodes[e.action_ref_node_indices]],-1)))
 actions=_apply_rowwise(model.action_encoder,torch.cat([e.action_features,refs],-1))
 logits=_apply_rowwise(model.scorer,torch.cat([state.unsqueeze(0).expand(len(actions),-1),actions],-1)).squeeze(-1)
 return logits,model.value_head(state).squeeze(-1)

def flat(out):return torch.cat([out[0],out[1].reshape(1)])
samples=[]
for sample in fixture['samples']:
 assert sample['stack']['contract_sha256']==contract_sha
 samples.append((encoded(sample['native']),sample['stack']))
reports=[]
for name in ['zero','nonzero']:
 weight=torch.zeros(64,440) if name=='zero' else torch.tensor(fixture['weights']).reshape(64,440)
 outputs=[];deltas=[]
 for (e,stack),sample in zip(samples,fixture['samples']):
  with torch.no_grad():out=flat(forward(base,weight,e,stack));legacy=flat(base(e))
  if name=='zero':assert torch.equal(out,legacy),'zero reference changed legacy output'
  expected=torch.tensor(sample[name]['logits']+[sample[name]['value']]);delta=(out-expected).abs()
  assert torch.all(delta<=1e-3+1e-3*expected.abs()),(name,float(delta.max()))
  outputs.append(out);deltas.append(float(delta.max()))
 for i in [0,2]:assert torch.equal(outputs[i],outputs[i+1]),'hidden permutation changed scores'
 reports.append(dict(variant=name,max_absolute_delta=max(deltas),samples=len(samples)))

# A double-precision finite difference checks one largest nonzero derivative of
# the new projection. It is a math test, not a substituted terminal reward.
from dataclasses import replace
e,stack=samples[0];double_e=replace(e,**{k:getattr(e,k).double() for k in ['state','object_features','edge_features','action_features','action_ref_features']})
double_base=copy.deepcopy(base).double();w=torch.zeros((64,440),dtype=torch.float64,requires_grad=True)
def loss(out):return -torch.log_softmax(out[0],0)[0]+0.25*out[1].square()
probe=loss(forward(double_base,w,double_e,stack));probe.backward();assert torch.isfinite(w.grad).all()
idx=int(w.grad.abs().argmax());analytic=float(w.grad.flatten()[idx]);assert abs(analytic)>1e-9
epsilon=1e-5
with torch.no_grad():
 w.flatten()[idx]=epsilon;plus=float(loss(forward(double_base,w,double_e,stack)))
 w.flatten()[idx]=-epsilon;minus=float(loss(forward(double_base,w,double_e,stack)))
 w.flatten()[idx]=0
numerical=(plus-minus)/(2*epsilon);assert abs(analytic-numerical)<=1e-6+1e-4*abs(numerical),(analytic,numerical)

# Import old Adam moments/age unchanged, initialize only the new matrix at zero,
# and verify exact save/load continuation across two synthetic engineering steps.
weight=nn.Parameter(torch.zeros(64,440));optimizer.add_param_group({'params':[weight]})
optimizer.state[weight]=dict(step=torch.tensor(0.),exp_avg=torch.zeros_like(weight),exp_avg_sq=torch.zeros_like(weight))
assert all(int(optimizer.state[v]['step'])==checkpoint['adam_step'] for v in base.parameters())
def step(model,weight,opt):
 opt.zero_grad(set_to_none=True);loss(forward(model,weight,e,stack)).backward()
 assert weight.grad is not None and torch.isfinite(weight.grad).all() and weight.grad.abs().sum()>0
 for name,param in model.named_parameters():
  if param.grad is None:param.grad=torch.zeros_like(param)
  assert torch.isfinite(param.grad).all()
  if name=='scorer.2.bias':param.grad.zero_()
  if name=='card_embedding.weight':param.grad[0].zero_()
 opt.step()
step(base,weight,optimizer)
assert int(optimizer.state[weight]['step'])==1
assert all(int(optimizer.state[v]['step'])==checkpoint['adam_step']+1 for v in base.parameters())
buffer=io.BytesIO();torch.save(dict(model=base.state_dict(),weight=weight.detach(),optimizer=optimizer.state_dict()),buffer);buffer.seek(0)
saved=torch.load(buffer,weights_only=True)
resumed,ropt,_=import_checkpoint(fixture['checkpoint'],fixture['checkpoint_sha256'],False)
rw=nn.Parameter(saved['weight'].clone());resumed.load_state_dict(saved['model']);ropt.add_param_group({'params':[rw]});ropt.load_state_dict(saved['optimizer'])
step(base,weight,optimizer);step(resumed,rw,ropt)
assert torch.equal(weight,rw)
for left,right in zip(base.parameters(),resumed.parameters()):
 assert torch.equal(left,right)
 for key in ['step','exp_avg','exp_avg_sq']:assert torch.equal(optimizer.state[left][key],ropt.state[right][key])
for key in ['step','exp_avg','exp_avg_sq']:assert torch.equal(optimizer.state[weight][key],ropt.state[rw][key])
result=dict(status='REFERENCE-ENGINEERING-PASS',fixture_sha256=hashlib.sha256(a.fixture.read_bytes()).hexdigest(),script_sha256=hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),torch_version=torch.__version__,variants=reports,gradient_probe=dict(index=idx,analytic=analytic,numerical=numerical),old_adam_age=checkpoint['adam_step'],new_adam_age=0,optimizer_continuation_bit_exact=True,non_claim='CPU reference engineering only. No native device gradients, trainer/collector integration, rollout replay or strength measurement.')
with a.output.open('x') as f:json.dump(result,f,indent=2)
print(json.dumps(result,indent=2))
