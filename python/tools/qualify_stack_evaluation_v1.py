"""Fixed CPU engineering panel: at most18 BO3 executions, no strength selection."""
import argparse,copy,hashlib,json,os,pathlib,subprocess,time
from concurrent.futures import ThreadPoolExecutor

def read(p):return json.loads(pathlib.Path(p).read_bytes())
def pin(p):return dict(path=str(p),sha256=hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest())
def write(p,v):
 with pathlib.Path(p).open('x') as f:json.dump(v,f,indent=2)

def run(build,root):
 repo=pathlib.Path(__file__).resolve().parents[2]
 receipt=read(build/'completion.json');assert receipt['complete']
 assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()==read(build/'start.json')['commit']
 assert not subprocess.check_output(['git','status','--porcelain'],cwd=repo,text=True).strip()
 binaries=receipt['binaries']
 for value in binaries.values():assert pin(value['path'])==value
 baseline=pathlib.Path('E:/mtg-meta-recovery-20260921/entropy-scorer-qualification-003')
 training=pathlib.Path('E:/mtg-meta-recovery-20260921/public-stack-terminal-training-001')
 assert read(training/'result.json')['status']=='STACK-TERMINAL-TRAINING-ENGINEERING-PASS'
 root.mkdir()
 write(root/'manifest.json',dict(schema='stack-evaluation-engineering/v1',build=pin(build/'completion.json'),runner=pin(__file__),binaries=binaries,training=pin(training/'result.json'),baseline=pin(baseline/'qualification.json'),maximum_bo3_executions=18,cpu_workers=2,question='Full archived trained-score replay, baseline evaluator compatibility, zero stack parity, both-seat nonzero BO3 and deterministic fresh-process replay. No win-rate selection.',review='Known Fable zero-read429 until September22 07:00EDT; bounded integration under Jack research authority, no endorsement.'))
 env=os.environ.copy();env['PATH']='C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v12.8/bin;'+env['PATH']
 def launch(label,binary,command,error=None):
  folder=root/label;folder.mkdir();command=copy.deepcopy(command);command['output_directory']=str(folder/'outputs');write(folder/'request.json',command)
  before=time.time()
  with (folder/'native.log').open('wb') as log:
   child=subprocess.Popen([binaries[binary]['path'],str(folder/'request.json')],cwd=repo,env=env,stdout=log,stderr=subprocess.STDOUT,creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS|subprocess.CREATE_NO_WINDOW)
   write(folder/'process.json',dict(pid=child.pid,started_unix=before))
   try:code=child.wait(timeout=180)
   except subprocess.TimeoutExpired:
    subprocess.run(['taskkill','/PID',str(child.pid),'/T','/F'],check=True,capture_output=True);child.wait();code=124
  write(folder/'execution.json',dict(exit_code=code,seconds=time.time()-before,binary=binaries[binary],request=pin(folder/'request.json')))
  if error:
   assert code!=0 and error in (folder/'native.log').read_text() and not (folder/'outputs').exists(),label
  else:assert code==0,(label,(folder/'native.log').read_text()[-1500:])
  return folder/'outputs'
 replay_rows=0
 def replay_job(mode,age):
  base=training/(mode+'-w2')/'outputs'
  command=dict(config=pin(base/'config.json'),checkpoint=pin(base/f'{age-1:04}/checkpoint.json'),trajectories=[pin(base/f'{age:04}/episode-{i:03}.json') for i in range(2)])
  output=launch(f'replay-{mode}-{age}','stack_policy_replay_v1',command)
  report=read(output/'result.json');assert len(report['archives'])==2 and {x['learner_seat'] for x in report['archives']}=={0,1}
  return report['exact_behavior_rows']
 with ThreadPoolExecutor(max_workers=2) as pool:
  replay_rows=sum(pool.map(lambda pair:replay_job(*pair),[(mode,age) for mode in ['structured','permuted','disabled'] for age in [1,2]]))
 base=training/'permuted-w2/outputs'
 command=dict(config=pin(base/'config.json'),checkpoint=pin(base/'0000/checkpoint.json'),trajectories=[pin(base/'0001/episode-000.json')])
 bad=read(base/'config.json');bad['input_mode']='structured';write(root/'wrong-mode.json',bad)
 launch('reject-mode','stack_policy_replay_v1',dict(command,config=pin(root/'wrong-mode.json')),'evaluation checkpoint/config identity differs')
 launch('reject-age','stack_policy_replay_v1',dict(command,checkpoint=pin(base/'0001/checkpoint.json')),'archive identity differs')
 bad=read(base/'0001/episode-000.json');aux=next(x for x in bad['auxiliary'] if x is not None);aux['permutation']['columns'][1]=aux['permutation']['columns'][0];write(root/'corrupt-permutation.json',bad)
 launch('reject-permutation','stack_policy_replay_v1',dict(command,trajectories=[pin(root/'corrupt-permutation.json')]),'not a permutation')
 old={job['id']:pathlib.Path(job['output_directory']) for name in ['cheap-one','remaining-eleven'] for job in read(baseline/name/'result.json')['jobs']}
 requests={seat:read(baseline/f'legacy-canonical-Affinity-p{seat}.request.json') for seat in range(2)}
 jobs=[]
 for seat in range(2):
  for prefix in ['legacy','entropy']:
   name=f'{prefix}-canonical-Affinity-p{seat}';jobs.append((name,read(baseline/(name+'.request.json')),'old',old[name]))
  for mode in ['structured','permuted','disabled']:
   zero=copy.deepcopy(requests[seat]);zero['sources'][seat]=dict(kind='stack_warm_start',source=zero['sources'][seat]['source'],input_mode=mode)
   jobs.append((f'zero-{mode}-p{seat}',zero,'zero',old[f'legacy-canonical-Affinity-p{seat}']))
   base=training/(mode+'-w2')/'outputs';trained=copy.deepcopy(requests[seat]);trained['sources'][seat]=dict(kind='stack_checkpoint',config=pin(base/'config.json'),checkpoint=pin(base/'0002/checkpoint.json'));trained['capture_decisions']=True
   jobs.append((f'trained-{mode}-p{seat}',trained,'trained',None))
 assert len(jobs)==16 and all(len(job[1]['matches'])==1 for job in jobs)
 reports={}
 def match(job):
  name,command,kind,reference=job
  output=launch(name,'public_feature_evaluation_v1',command);completed=read(output/'completion.json');assert completed['matches']==1
  actual=read(output/'match-000000.json');assert len(actual['games'])==completed['natural_games'] and actual['decision_count']==completed['decisions']
  if kind=='old':assert pin(output/'match-000000.json')['sha256']==pin(reference/'match-000000.json')['sha256'],name
  if kind=='zero':
   expected=read(reference/'match-000000.json');assert {k:v for k,v in actual.items() if k!='models'}=={k:v for k,v in expected.items() if k!='models'},name
  if kind=='trained':
   assert len(actual['decisions'])==actual['decision_count'] and actual['decisions']
   for row in actual['decisions']:assert 0<=row['selected']<row['legal_action_count']
  return name,dict(output=str(output),games=completed['natural_games'],decisions=completed['decisions'],kind=kind)
 with ThreadPoolExecutor(max_workers=2) as pool:reports.update(pool.map(match,jobs))
 for mode in ['structured','permuted']:
  name=f'trained-{mode}-p0';command=read(root/name/'request.json');repeat=launch(name+'-repeat','public_feature_evaluation_v1',command)
  first=pathlib.Path(reports[name]['output']);assert (repeat/'match-000000.json').read_bytes()==(first/'match-000000.json').read_bytes()
  reports[name+'-repeat']=dict(output=str(repeat),games=reports[name]['games'],decisions=reports[name]['decisions'],kind='repeat')
 result=dict(status='STACK-BO3-AND-LOADED-REPLAY-ENGINEERING-PASS',bo3_executions=len(reports),natural_game_executions=sum(x['games'] for x in reports.values()),decision_executions=sum(x['decisions'] for x in reports.values()),complete_archived_behavior_rows=replay_rows,rejected_before_output=['mode','age','corrupt-permutation'],cases=reports,non_claim='Fixed engineering seeds, repeated for parity. No playing-strength estimate, trained model selection, general opponent coverage or production allocation qualification.')
 write(root/'result.json',result);print(json.dumps({k:v for k,v in result.items() if k!='cases'},indent=2))

if __name__=='__main__':
 if not __debug__:raise RuntimeError('Qualification requires assertions enabled')
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--build',type=pathlib.Path,required=True);p.add_argument('--root',type=pathlib.Path,required=True);a=p.parse_args();run(a.build,a.root)
