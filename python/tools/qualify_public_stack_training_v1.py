"""Fixed bounded integration check, 38 terminal games, no outcome selection."""
import argparse,copy,hashlib,json,pathlib,subprocess,sys,time
p=argparse.ArgumentParser();p.add_argument('--build',type=pathlib.Path,required=True);p.add_argument('--root',type=pathlib.Path,required=True);a=p.parse_args()
repo=pathlib.Path(__file__).resolve().parents[2]
def read(p):return json.loads(pathlib.Path(p).read_text())
def sha(p):return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
def write(p,v):
 with pathlib.Path(p).open('x') as f:json.dump(v,f,indent=2)
build=read(a.build/'completion.json');assert build['complete']
assert sha(build['binary'])==build['binary_sha256']
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()==read(a.build/'start.json')['commit']
assert not subprocess.check_output(['git','status','--porcelain'],cwd=repo,text=True).strip()
source=pathlib.Path('E:/mtg-meta-recovery-20260921/public-entropy-engineering-001/entropy-config.json')
config=read(source)
config['updates']=[b[:2] for b in config['updates'][:3]]
for k in ['inputs_enabled','projection_mode','entropy_coefficient']:config.pop(k,None)
config.update(schema='mtg-kernel-stack-training-config/v1',stack_contract_sha256=sha(repo/'data/public_stack_features_v1/contract.json'),permutation_contract_sha256=sha(repo/'data/public_stack_features_v1/permutation.json'))
a.root.mkdir()
write(a.root/'manifest.json',dict(source=str(source),source_sha256=sha(source),commit=read(a.build/'start.json')['commit'],binary=build['binary'],binary_sha256=build['binary_sha256'],runner_sha256=sha(__file__),maximum_natural_games=38,updates=3,games_per_update=2,workers=[1,2],question='Qualify full terminal-reward collection/update, same-state replay, serial/parallel and fresh-process checkpoint equality for structured/permuted inputs, and disabled projection invariance. No win-rate inference.',review='Fable zero-read HTTP429 through September22 07:00EDT; no retry or endorsement. Bounded engineering under Jack research authority.'))
guard_checks=[]
for label,extra,expected in [('missing-allocation',[],'substantial training requires allocation'),('oversized-engineering',['--engineering-question','bounded correctness'],'engineering scope exceeds')]:
 c=copy.deepcopy(config);c['input_mode']='structured';c['updates']=c['updates']+[c['updates'][0]]
 request=a.root/(label+'-request.json');write(request,dict(config=c,output_directory=str(a.root/(label+'-outputs')),resume=None,stop_after=4,collector_workers=2,execution_gpu_ordinal=1))
 cmd=[sys.executable,str(repo/'python/tools/run_stack_training_v1.py'),'--binary',build['binary'],'--binary-sha256',build['binary_sha256'],'--request',str(request),'--root',str(a.root/(label+'-execution')),*extra]
 rejected=subprocess.run(cmd,capture_output=True,text=True)
 assert rejected.returncode!=0 and expected in rejected.stderr and not (a.root/(label+'-outputs')).exists() and not (a.root/(label+'-execution')).exists()
 guard_checks.append(dict(case=label,rejected_before_spawn=True,error=expected))
write(a.root/'guard-checks.json',guard_checks)
results=[];compared=[];games=0
def run(label,mode,workers,resume=None):
 global games
 c=copy.deepcopy(config);c['input_mode']=mode
 folder=a.root/label;folder.mkdir()
 request=dict(config=c,output_directory=str(folder/'outputs'),resume=resume,stop_after=3,collector_workers=workers,execution_gpu_ordinal=1)
 write(folder/'request.json',request)
 command=[sys.executable,str(repo/'python/tools/run_stack_training_v1.py'),'--binary',build['binary'],'--binary-sha256',build['binary_sha256'],'--request',str(folder/'request.json'),'--root',str(folder/'execution'),'--engineering-question','Full terminal-game collector/update replay and optimizer continuation correctness; no strength estimate.']
 subprocess.run(command,check=True)
 output=folder/'outputs';done=read(output/'completion.json');first=read(resume['path'])['next_update'] if resume else 0
 assert done['first_update']==first and done['next_update']==3 and len(done['receipts'])==3-first
 first_step=None
 for update in range(first,3):
  rec=read(output/f'{update:04}/receipt.json');assert rec['natural_games']==2 and rec['episodes']==2 and rec['legacy_adam_step']==32401+update and rec['stack_adam_step']==update+1
  assert all(v>=0 for v in rec['stage_seconds'].values())
  for i in range(2):
   trajectory=read(output/f'{update:04}/episode-{i:03}.json')
   assert trajectory['terminal']['terminal_classification']=='natural'
   assert len(trajectory['decisions'])==len(trajectory['auxiliary'])
   assert trajectory['input_mode']==mode
   for row,aux in zip(trajectory['decisions'],trajectory['auxiliary']):
    if row['actor']==trajectory['episode']['learner_seat']:
     assert aux is not None
     assert ('permutation' in aux)==(mode=='permuted')
    else:assert aux is None
  state=read(output/f'{update:04}/optimizer.json')
  stack=state['stack']
  if mode=='disabled':assert all(v in [0,2147483648] for key in ['weight','first','second'] for v in stack[key])
  elif update==2:assert any(v not in [0,2147483648] for v in stack['weight']) and any(v not in [0,2147483648] for v in stack['first'])
 count=(3-first)*2;games+=count;assert games<=38
 result=dict(label=label,natural_games=count,execution=read(folder/'execution/execution.json'),outputs=str(output))
 results.append(result);write(folder/'verified.json',result);print(dict(label=label,natural_games=count,seconds=result['execution']['seconds']),flush=True)
 return output
def compare(left,right,updates):
 for update in updates:
  for name in ['optimizer.json','checkpoint.json','episode-000.json','episode-001.json']:
   rel=f'{update:04}/{name}';assert sha(left/rel)==sha(right/rel),(left,right,rel)
   compared.append(dict(left=str(left/rel),right=str(right/rel),sha256=sha(left/rel)))
for mode in ['structured','permuted']:
 serial=run(mode+'-w1',mode,1)
 parallel=run(mode+'-w2',mode,2);compare(serial,parallel,range(3))
 checkpoint=parallel/'0000/checkpoint.json'
 resumed=run(mode+'-resume',mode,2,dict(path=str(checkpoint),sha256=sha(checkpoint)));compare(parallel,resumed,range(1,3))
disabled=run('disabled-w2','disabled',2)
for i in range(2):
 base=read(disabled/f'0000/episode-{i:03}.json')
 for mode in ['structured','permuted']:
  other=read(a.root/(mode+'-w2')/f'outputs/0000/episode-{i:03}.json')
  for key in ['decisions','terminal','configuration_sha256','opponent','episode']:assert base[key]==other[key],(mode,key)
write(a.root/'result.json',dict(status='STACK-TERMINAL-TRAINING-ENGINEERING-PASS',natural_games=games,executions=results,exact_file_comparisons=compared,non_claim='38 engineering games do not estimate playing strength. No throughput allocation qualification across machines, no formal experiment, no promotion.'))
print(dict(status='PASS',natural_games=games,exact_files=len(compared)),flush=True)
