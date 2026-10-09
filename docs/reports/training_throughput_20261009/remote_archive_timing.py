import json,pathlib,subprocess,hashlib
import os
root=pathlib.Path(__file__).parent
code=r'''
import pathlib,json,datetime,hashlib
r=pathlib.Path('C:/mtg-node/nine-deck-baseline-20261007/campaign')
rows=[]
for run,count in [('r1',20),('r2',20),('r3',20),('r4',3)]:
 for number in range(1,count+1):
  block=f'b{number:02d}-a1'
  report_path=r/'hot'/run/block/'dispatch/report.json'
  report_raw=report_path.read_bytes();report=json.loads(report_raw)
  ref=report['archive'];p=pathlib.Path(ref['path'])
  row={'run':run,'block':block,'report_pin':{'path':str(report_path),'sha256':hashlib.sha256(report_raw).hexdigest()},'archive_pin':ref,'present':p.exists(),'verified':False}
  if p.exists():
   raw=p.read_bytes();digest=hashlib.sha256(raw).hexdigest()
   row['observed_sha256']=digest;row['bytes']=len(raw)
   row['verified']=digest==ref['sha256']
   archive=json.loads(raw)
   row['archive_seconds']=archive.get('seconds')
   row['archive_fields']=list(archive)
  else:row['missing']='report-pinned archive metadata absent on Haley'
  rows.append(row)
print(json.dumps({'observed_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'host':'HALEYSPC','attempted_blocks':len(rows),'blocks':rows,'note':'Read report.json and pinned archive.json metadata only. No ZIP or trajectory read.'}))
'''
p=subprocess.run(['ssh','-o','BatchMode=yes','-o','ConnectTimeout=10',os.environ['MTG_AUDIT_SSH_TARGET'],os.environ['MTG_AUDIT_REMOTE_PYTHON'],'-B','-'],input=code,capture_output=True,text=True,timeout=60)
if p.returncode:print(json.dumps({'code':p.returncode,'stderr':p.stderr}))
else:
 v=json.loads(p.stdout);out=root/'haley-archive-timings.json';out.write_text(json.dumps(v,indent=2), newline="\n")
 print(json.dumps({'output':str(out),'sha256':hashlib.sha256(out.read_bytes()).hexdigest(),'attempted_blocks':len(v['blocks']),'present':sum(b['present'] for b in v['blocks']),'verified':sum(b['verified'] for b in v['blocks']),'seconds':sum(b.get('archive_seconds') or 0 for b in v['blocks']),'sample':next((b for b in v['blocks'] if b['present']),None)}))
