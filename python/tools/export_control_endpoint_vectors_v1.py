"""Export existing matched endpoint outcomes for later power planning. No estimate or gate."""
import sys
if sys.flags.optimize:raise RuntimeError('Optimized execution prohibited for pinned legacy reader')
import hashlib,importlib.util,json,time
from pathlib import Path
import numpy as np
B=Path('E:/mtg-meta-recovery-20260921');root=B/'control-variance-bo3-002';out=B/'control-endpoint-vectors-001'
def read(p):return json.loads(Path(p).read_bytes())
def pin(p):
 p=Path(p)
 with p.open('rb') as f:h=hashlib.file_digest(f,'sha256').hexdigest()
 return {'path':str(p),'sha256':h}
def checked(ref):
 p=Path(ref['path'])
 if pin(p)['sha256']!=ref['sha256']:raise ValueError('input pin differs: '+str(p))
 return p
start=time.monotonic();analysis=read(root/'analysis.json')
plan=read(checked(analysis['plan']));evaluation=read(checked(analysis['evaluation']))
module_path=checked(analysis['analysis']);checked(analysis['bootstrap_implementation'])
if not evaluation['complete'] or out.exists():raise ValueError('incomplete source or existing output')
sys.path.insert(0,str(module_path.parent))
spec=importlib.util.spec_from_file_location('pinned_variance_reader',module_path);mod=importlib.util.module_from_spec(spec);spec.loader.exec_module(mod)
# Only the pinned validation/assembly routines run. Do not call legacy estimate/power or write the original analysis.
records,manifests,_,games=mod.endpoint_records(plan,evaluation)
keys=plan['endpoint_keys'];values,pairs,summary=mod.values_array(records,keys)
for key,row in summary.items():
 for metric,value in row.items():
  if analysis['endpoints'][key][metric]!=value:raise ValueError('published endpoint total differs')
seeds=np.array([[records[*pair,r,0,keys[0]]['seed'] for r in range(8)] for pair in pairs],dtype=np.uint64)
if len(set(seeds.ravel().tolist()))!=512:raise ValueError('seed reuse across supposed distinct conditions')
if not np.all((values==0)|(values==1)):raise ValueError('nonbinary endpoint vector')
rates=values[:,:,:, [i for i,k in enumerate(keys) if k!='g115'],0].mean(axis=(0,1,2))
sd=100*float(rates.std(ddof=1))
if not np.isclose(sd,analysis['measurement']['between_run_sd_pp'],rtol=0,atol=1e-12):raise ValueError('published SD differs')
out.mkdir();np.save(out/'values.npy',values.astype(np.uint8),allow_pickle=False);np.save(out/'seeds.npy',seeds,allow_pickle=False)
receipt=dict(complete=True,source_analysis=pin(root/'analysis.json'),plan=analysis['plan'],evaluation=analysis['evaluation'],reader=analysis['analysis'],script=pin(__file__),values=pin(out/'values.npy'),seeds=pin(out/'seeds.npy'),axes=['matchup','environment_seed_replicate','candidate_seat','endpoint','metric'],shape=list(values.shape),metrics=['bo3_win','game_one_win'],endpoint_keys=keys,pairs=pairs,summary=summary,distinct_seed_pairs=512,between_run_sd_pp=sd,games=games,seconds=time.monotonic()-start,limits='Artifact validation/export only. Same 512 environment seeds shared across11 endpoints, both seats kept together. No candidate selection, model update, new matches, power estimate, independence proof or launch authority. Future simulation must define an intervention/effect and the entire gate; do not reuse legacy normal-approximation power or covariance subtraction.')
with (out/'completion.json').open('x') as f:json.dump(receipt,f,indent=2)
print(json.dumps({k:receipt[k] for k in ['complete','shape','distinct_seed_pairs','between_run_sd_pp','seconds']}))
