"""Bounded archived match parity for the new stack policy adapter."""
import argparse,concurrent.futures,hashlib,json,os,pathlib,subprocess,time
p=argparse.ArgumentParser();p.add_argument('--build',type=pathlib.Path,required=True);p.add_argument('--root',type=pathlib.Path,required=True);a=p.parse_args()
repo=pathlib.Path(__file__).resolve().parents[2];baseline=pathlib.Path('E:/mtg-postboard-campaign-20260920/prevention-correction-replay-001')
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def write(name,data):
 with (a.root/name).open('x') as f:json.dump(data,f,indent=2)
build=json.loads((a.build/'start.json').read_text());assert json.loads((a.build/'completion.json').read_text())['complete']
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()==build['commit']
assert not subprocess.check_output(['git','status','--porcelain'],cwd=repo,text=True).strip()
binary=pathlib.Path('E:/cargo-target-public-cost/release/deps/mtg_kernel-73164b849c0d4084.exe');assert binary.stat().st_mtime>=build['started_unix']
a.root.mkdir();env=os.environ.copy();env['PATH']='C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v12.8/bin;'+env['PATH']
write('start.json',dict(source_commit=build['commit'],binary_sha256=sha(binary),baseline=str(baseline),scope='Two archived zero-projection BO3 matches and one exact fresh-process replay. No strength measurement.'))
def execute(name,filter,environment,ignored=False):
 began=time.monotonic();command=[str(binary),filter,'--nocapture','--test-threads=2']+(['--ignored'] if ignored else [])
 with (a.root/f'{name}.log').open('wb') as log:
  child=subprocess.Popen(command,cwd=repo,env=environment,stdout=log,stderr=subprocess.STDOUT,creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS|subprocess.CREATE_NO_WINDOW)
  write(f'{name}-process.json',dict(pid=child.pid,started_unix=time.time()))
  try:code=child.wait(timeout=180)
  except subprocess.TimeoutExpired:
   subprocess.run(['taskkill','/PID',str(child.pid),'/T','/F'],check=True,capture_output=True);code=124
 text=(a.root/f'{name}.log').read_text(encoding='utf-8');assert code==0 and 'test result: ok.' in text and '0 passed;' not in text,(name,code,text[-1500:])
 return dict(name=name,exit_code=code,seconds=time.monotonic()-began)
reports=[execute('adapter','stack_policy_preserves_rng_hidden_invariance_replay_and_rejects_stale_capture',env),execute('legacy','native_policy_value_net_v1::tests::',env)]
def match(seat,name):
 request=baseline/f'request-seat{seat}.json';old=baseline/f'seat{seat}.json';output=a.root/f'{name}.json'
 e=env.copy();e.update(MTG_STACK_MATCH_REQUEST=str(request),MTG_STACK_MATCH_BASELINE=str(old),MTG_STACK_MATCH_REPORT=str(output))
 report=execute(name,'stack_input_zero_projection_full_match_replay',e,True)
 actual=json.loads(output.read_text());assert actual['request_sha256']==sha(request) and actual['baseline_sha256']==sha(old)
 assert actual['all_baseline_gameplay_fields_exact']
 report.update(games=len(actual['games']),decisions=len(actual['decisions']),sha256=sha(output));return report
with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:reports.extend(pool.map(lambda seat:match(seat,f'seat{seat}'),[0,1]))
reports.append(match(0,'seat0-replay'))
assert (a.root/'seat0.json').read_bytes()==(a.root/'seat0-replay.json').read_bytes()
result=dict(complete=True,source_commit=build['commit'],binary_sha256=sha(binary),phases=reports,unique_matches=2,unique_games=sum(r['games'] for r in reports[2:4]),unique_decisions=sum(r['decisions'] for r in reports[2:4]),fresh_process_replay_byte_identical=True,non_claim='Engineering parity only. No nonzero full-match or reward-based training result.')
write('completion.json',result);print(json.dumps(result,indent=2))
