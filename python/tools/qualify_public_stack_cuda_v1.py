"""Bounded correctness checks only, not a training or evaluation launcher."""
import argparse,hashlib,json,os,pathlib,re,subprocess,time
p=argparse.ArgumentParser();p.add_argument('--build',type=pathlib.Path,required=True);p.add_argument('--root',type=pathlib.Path,required=True);p.add_argument('--fixture',type=pathlib.Path);a=p.parse_args()
repo=pathlib.Path(__file__).resolve().parents[2]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(name,data):
 with (a.root/name).open('x') as f:json.dump(data,f,indent=2)
build=json.loads((a.build/'start.json').read_text());assert json.loads((a.build/'completion.json').read_text())['complete']
assert not subprocess.check_output(['git','status','--porcelain'],cwd=repo,text=True).strip()
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()==build['commit']
binary=pathlib.Path('E:/cargo-target-public-cost/release/deps/mtg_kernel-73164b849c0d4084.exe');assert binary.stat().st_mtime>=build['started_unix']
inventory=subprocess.check_output(['nvidia-smi','--query-gpu=index,uuid,name,utilization.gpu,memory.used','--format=csv,noheader'],text=True)
line=[line for line in inventory.splitlines() if line.startswith('1,')][0]
assert 'GPU-0642d3ca-e3d4-ba16-96ab-c561c6da90e3' in line and ', 0 %,' in line,line
assert int(line.rsplit(',',1)[1].strip().split()[0])<100,'GPU1 memory is occupied'
a.root.mkdir();(a.root/'temp').mkdir();(a.root/'gpu-before.txt').write_text(inventory)
fixture_path=a.fixture or a.build/'g115-reference.json';fixture=json.loads(fixture_path.read_text());samples=fixture['samples'];assert len(samples)==16
manifest=dict(schema='public-stack-cuda-engineering/v1',source_commit=build['commit'],gpu_ordinal=1,gpu_uuid='GPU-0642d3ca-e3d4-ba16-96ab-c561c6da90e3',checkpoint=fixture['checkpoint'],checkpoint_sha256=fixture['checkpoint_sha256'],fixture=str(fixture_path),fixture_sha256=sha(fixture_path),binary=str(binary),binary_sha256=sha(binary),selected_indices=[i%len(s['zero']['logits']) for i,s in enumerate(samples)],targets=[[-1.,0.,1.][i%3] for i in range(8)],advantages=[[-.5,.25,.75][i%3] for i in range(8)],group_size=2,learning_rate=0.0001,value_coefficient=.5,entropy_coefficient=0.,gates=dict(forward=[1e-3,1e-3],gradient=[1e-4,1e-3],parameter_delta=[2e-6,.005],first_moments=[1e-5,1e-3],second_moments=[1e-7,1e-3],stack_gradient_relative_l2=1e-3,stack_update_relative_l2=.005),non_claim='Two synthetic grouped GAE updates on 16 fixed actor fixtures, control derivative and chunking checks. No collected games, reward-based training, model selection or strength measurement.')
write('manifest.json',manifest)
env=os.environ.copy();env['PATH']='C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v12.8/bin;'+env['PATH'];env['TEMP']=env['TMP']=str(a.root/'temp');env['MTG_STACK_CUDA_ROOT']=str(a.root)
assert not env.get('CUDA_VISIBLE_DEVICES'),'GPU ordinal mapping must not be remapped'
phases=[]
for name,args in [('snapshot',['stack_optimizer_snapshot_rejects_contract_and_projection_corruption']),('start',['stack_cuda_g115_grouped_update_and_fresh_process_resume','--ignored']),('resume',['stack_cuda_g115_grouped_update_and_fresh_process_resume','--ignored'])]:
 if name=='resume':env['MTG_STACK_CUDA_RESUME']='1'
 else:env.pop('MTG_STACK_CUDA_RESUME',None)
 began=time.monotonic()
 with (a.root/f'{name}.log').open('wb') as log:
  child=subprocess.Popen([str(binary),*args,'--nocapture'],cwd=repo,env=env,stdout=log,stderr=subprocess.STDOUT,creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS|subprocess.CREATE_NO_WINDOW)
  write(f'{name}-process.json',dict(pid=child.pid,started_unix=time.time()))
  try:code=child.wait(timeout=360)
  except subprocess.TimeoutExpired:
   subprocess.run(['taskkill','/PID',str(child.pid),'/T','/F'],check=True,capture_output=True);code=124
 text=(a.root/f'{name}.log').read_text(encoding='utf-8')
 passed=code==0 and '1 passed; 0 failed' in text
 phases.append(dict(name=name,exit_code=code,passed=passed,seconds=time.monotonic()-began));print(phases[-1],flush=True)
 if not passed:break
write('execution-completion.json',dict(complete=len(phases)==3 and all(p['passed'] for p in phases),phases=phases))
raise SystemExit(0 if len(phases)==3 and all(p['passed'] for p in phases) else 1)
