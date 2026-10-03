"""Compare fixed native CUDA gradients/updates with the independent CPU graph."""
import argparse,hashlib,json,pathlib,runpy,sys
import numpy as np
import torch
p=argparse.ArgumentParser();p.add_argument('--root',type=pathlib.Path,required=True);p.add_argument('--output-directory',type=pathlib.Path);a=p.parse_args()
root=a.root;repo=pathlib.Path(__file__).resolve().parents[2];sys.path.insert(0,str(repo/'python'))
from mtg_kernel_rl.public_feature_model_v1 import import_checkpoint
report_root=a.output_directory or root
if report_root!=root:report_root.mkdir()
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
manifest=json.loads((root/'manifest.json').read_text());assert json.loads((root/'execution-completion.json').read_text())['complete']
fixture_path=pathlib.Path(manifest['fixture']);assert sha(fixture_path)==manifest['fixture_sha256']
sys.argv=['check_public_stack_reference_v1.py','--fixture',str(fixture_path),'--output',str(report_root/'cpu-reference.json')]
reference=runpy.run_path(str(repo/'python/tools/check_public_stack_reference_v1.py'),run_name='__main__')
forward=reference['forward'];encode=reference['encoded'];fixture=json.loads(fixture_path.read_text())
base,optimizer,source=import_checkpoint(manifest['checkpoint'],manifest['checkpoint_sha256'],False)
weight=torch.nn.Parameter(torch.zeros(64,440));optimizer.add_param_group({'params':[weight]})
optimizer.state[weight]=dict(step=torch.tensor(0.),exp_avg=torch.zeros_like(weight),exp_avg_sq=torch.zeros_like(weight))
for group in optimizer.param_groups:group['lr']=manifest['learning_rate']
parameters=dict(base.named_parameters());parameters['stack.weight']=weight
initial={name:param.detach().clone() for name,param in parameters.items()}
decisions=[(encode(s['native']),s['stack']) for s in fixture['samples']]
gates=manifest['gates'];checks=[]
def bits(values):return torch.from_numpy(np.asarray(values,dtype=np.uint32).view(np.float32).copy())
def unpack(path):
 saved=json.loads(path.read_text());result={}
 for category in ['parameters','first_moments','second_moments']:
  result[category]={r['name']:bits(r['values']).reshape(r['shape']) for r in saved[category]}
 for category,key in [('parameters','weight'),('first_moments','first'),('second_moments','second')]:result[category]['stack.weight']=bits(saved['stack'][key]).reshape(64,440)
 return saved,result
def compare(actual,expected,gate):
 assert actual.shape==expected.shape and torch.isfinite(actual).all() and torch.isfinite(expected).all()
 delta=(actual-expected).abs();assert torch.all(delta<=gate[0]+gate[1]*expected.abs()),float(delta.max())
 return float(delta.max())
for step in [1,2]:
 optimizer.zero_grad(set_to_none=True);logits=[];values=[]
 for decision,stack in decisions:
  l,v=forward(base,weight,decision,stack);logits.append(l);values.append(v)
 losses=[]
 for group in range(8):
  logprob=sum(torch.log_softmax(logits[i],0)[manifest['selected_indices'][i]] for i in range(group*2,group*2+2))
  losses.append(-logprob*manifest['advantages'][group]+manifest['value_coefficient']*(values[group*2]-manifest['targets'][group]).square())
 torch.stack(losses).mean().backward()
 for name,param in parameters.items():
  if param.grad is None:param.grad=torch.zeros_like(param)
  if name=='scorer.2.bias':param.grad.zero_()
  if name=='card_embedding.weight':param.grad[0].zero_()
 native=json.loads((root/f'gradients-{step}.json').read_text())
 gradients={r['name']:bits(r['values']).reshape(r['shape']) for r in native['legacy']};gradients['stack.weight']=bits(native['stack']).reshape(64,440)
 checks.append(dict(step=step,category='logits',max_delta=compare(torch.tensor(native['logits']),torch.cat(logits).detach(),gates['forward'])))
 checks.append(dict(step=step,category='values',max_delta=compare(torch.tensor(native['values']),torch.stack(values).detach().reshape(-1),gates['forward'])))
 for name,param in parameters.items():
  checks.append(dict(step=step,tensor=name,category='gradient',max_delta=compare(gradients[name],param.grad,gates['gradient'])))
  if name=='stack.weight':
   assert param.grad.norm()>0 and gradients[name].norm()>0
   relative=float((gradients[name]-param.grad).norm()/param.grad.norm());assert relative<=gates['stack_gradient_relative_l2'],relative
   checks.append(dict(step=step,category='stack_gradient_relative_l2',value=relative))
 optimizer.step()
 raw,actual=unpack(root/('step1.json' if step==1 else 'step2-uninterrupted.json'))
 assert raw['legacy_adam_step']==source['adam_step']+step and raw['stack']['adam_step']==step
 for name,param in parameters.items():
  expected_delta=param.detach()-initial[name];actual_delta=actual['parameters'][name]-initial[name]
  checks.append(dict(step=step,tensor=name,category='parameter_delta',max_delta=compare(actual_delta,expected_delta,gates['parameter_delta'])))
  if name=='stack.weight':
   relative=float((actual_delta-expected_delta).norm()/expected_delta.norm());assert relative<=gates['stack_update_relative_l2'],relative
   checks.append(dict(step=step,category='stack_update_relative_l2',value=relative))
  for category,key in [('first_moments','exp_avg'),('second_moments','exp_avg_sq')]:checks.append(dict(step=step,tensor=name,category=category,max_delta=compare(actual[category][name],optimizer.state[param][key],gates[category])))
 for category in ['parameters','first_moments','second_moments']:assert not torch.count_nonzero(actual[category]['card_embedding.weight'][0])
 assert torch.equal(actual['parameters']['scorer.2.bias'].view(torch.int32),initial['scorer.2.bias'].view(torch.int32))
_,full=unpack(root/'step1.json');_,chunked=unpack(root/'step1-chunked.json')
for category in full:
 for name in full[category]:
  if category=='parameters':actual,expected=chunked[category][name]-initial[name],full[category][name]-initial[name];gate=gates['parameter_delta']
  else:actual,expected=chunked[category][name],full[category][name];gate=gates[category]
  checks.append(dict(category='chunked_'+category,tensor=name,max_delta=compare(actual,expected,gate)))
assert (root/'step2-uninterrupted.json').read_bytes()==(root/'step2-resumed.json').read_bytes()
for name in ['start-execution.json','resume-execution.json']:
 receipt=json.loads((root/name).read_text());assert receipt['status']=='STACK-CUDA-ENGINEERING-EXECUTION-PASS' and receipt['manifest_sha256']==sha(root/'manifest.json')
result=dict(status='STACK-CUDA-GRADIENT-UPDATE-RESUME-ENGINEERING-PASS',gpu_ordinal=1,fixture_count=16,physical_groups=8,parameter_tensors=34,legacy_final_age=source['adam_step']+2,stack_final_age=2,fresh_process_resume_bit_exact=True,checks=checks,manifest_sha256=sha(root/'manifest.json'),script_sha256=sha(pathlib.Path(__file__)),non_claim=manifest['non_claim'])
with (report_root/'qualification.json').open('x') as f:json.dump(result,f,indent=2)
print(json.dumps({k:v for k,v in result.items() if k!='checks'},indent=2))
