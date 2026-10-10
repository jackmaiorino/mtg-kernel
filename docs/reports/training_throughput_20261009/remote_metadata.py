import json
import os
import pathlib
import subprocess

BASE = pathlib.Path(__file__).parent
remote = r'''
import pathlib,json,hashlib,datetime
r=pathlib.Path('C:/mtg-node/nine-deck-baseline-20261007/campaign')
def pin(p):
    return {'path':str(p),'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'bytes':p.stat().st_size}
rows=[]
for run in ('r1','r2','r3','r4'):
 for block in sorted((r/'hot'/run).glob('b*-a*')):
  d=block/'dispatch'
  report_path=d/'report.json'
  if not report_path.exists():
   rows.append({'run':run,'block':block.name,'complete':False,'available_files':[p.name for p in d.iterdir()] if d.exists() else []})
   continue
  report=json.loads(report_path.read_text())
  stages=[{k:v for k,v in json.loads(line).items() if k in ('completed_iteration','scheduler_timing')} for line in (d/'stdout.jsonl').read_text().splitlines() if 'scheduler_timing' in line]
  telemetry=[json.loads(line) for line in (d/'telemetry.jsonl').read_text().splitlines() if line.strip()]
  rows.append({'run':run,'block':block.name,'complete':report.get('complete'),'report':{k:v for k,v in report.items() if k not in ('fingerprint','selected')},'execution':json.loads((d/'execution.json').read_text()),'pins':{n:pin(d/n) for n in ('report.json','execution.json','stdout.jsonl','telemetry.jsonl')},'stages':stages,'telemetry':telemetry})
states={p.stem:{'pin':pin(p),'status':json.loads(p.read_text()).get('status'),'blocks_count':len(json.loads(p.read_text()).get('blocks',[])),'updated':json.loads(p.read_text()).get('updated')} for p in (r/'state/runs').glob('*.json')}
qual=[]
for d in sorted((r/'hot/qual').iterdir()):
 for p in d.glob('**/report.json'):
  report=json.loads(p.read_text())
  if report.get('schema')=='native-expanded-cpu-dispatch/v1':
   qual.append({'pin':pin(p),'report':{k:v for k,v in report.items() if k not in ('fingerprint','selected')}})
print(json.dumps({'observed_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'host':'HALEYSPC','states':states,'blocks':rows,'qualifications':qual}))
'''
p=subprocess.run(['ssh','-o','BatchMode=yes','-o','ConnectTimeout=10',os.environ['MTG_AUDIT_SSH_TARGET'],os.environ['MTG_AUDIT_REMOTE_PYTHON'],'-B','-'],input=remote,capture_output=True,text=True,timeout=60)
if p.returncode:
    print(json.dumps({'code':p.returncode,'stderr':p.stderr}))
else:
    value=json.loads(p.stdout)
    output=BASE/'haley-training-timings.json'
    output.write_text(json.dumps(value,indent=2), newline="\n")
    print(json.dumps({'output':str(output),'bytes':output.stat().st_size,'states':value['states'],'blocks':len(value['blocks']),'complete':sum(b['complete'] for b in value['blocks']),'qualifications':len(value['qualifications'])}))
