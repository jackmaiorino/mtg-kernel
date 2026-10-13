"""Bounded fresh UCT viewer capture, inside a supported host-slots claim."""
from __future__ import annotations
import argparse
import concurrent.futures
import json
import re
import shutil
import subprocess
import time
from pathlib import Path
from viewer_capture_launcher_v1 import require_guard, sha, RESERVE
from viewer_export_v1 import terminal, Exporter

RAW_CAP = 2 * 1024**3
WORKING_CAP = 8 * 1024**3

def reserve_output(root, count):
    require_guard(root,'spy-full-search-trace-239')
    allowance=(512*1024**2+8192)*count
    if size(root,True)+allowance>RAW_CAP or size(root)+allowance>WORKING_CAP or shutil.disk_usage(root).free<RESERVE+allowance:
        raise ValueError('Cumulative retained capture or working-storage allowance exhausted')

def pins(binary, template):
    config=json.loads(template.read_text())
    out={'binary':{'path':str(binary),'sha256':sha(binary)},'template':{'path':str(template),'sha256':sha(template)}}
    for name in ('spec','focal','opponent'):
        item=config[name]
        if sha(Path(item['path']))!=item['sha256']: raise ValueError(f'{name} input pin differs')
        out[name]=item
        if name!='spec':
            source=json.loads(Path(item['path']).read_text())
            for part in ('checkpoint','play_import'):
                if source.get(part):
                    dependency=source[part]
                    if sha(Path(dependency['path']))!=dependency['sha256']:raise ValueError(f'{name}/{part} pin differs')
                    out[f'{name}/{part}']=dependency
    return out

def size(root, raw=False):
    return sum(p.stat().st_size for p in root.rglob('*') if p.is_file() and (not raw or p.name=='capture.jsonl'))

def invoke(binary, template, root, name, record, steps, seconds, failed_recorder=False):
    folder=root/name;folder.mkdir()
    config=json.loads(template.read_text());config.update(record=record,max_steps=steps,
        output=str((folder/('missing/capture.jsonl' if failed_recorder else 'capture.jsonl')).resolve()),primary=str((folder/'primary.jsonl').resolve()))
    path=folder/'config.json';path.write_text(json.dumps(config),encoding='utf-8')
    started=time.perf_counter()
    with (folder/'process.log').open('wb') as log:
        try:code=subprocess.run([str(binary),str(path)],stdout=log,stderr=log,timeout=seconds).returncode
        except subprocess.TimeoutExpired:code=-1
    primary=folder/'primary.jsonl';data=json.loads(primary.read_text()) if primary.exists() else None
    report={'name':name,'seconds':time.perf_counter()-started,'exit_code':code,
        'primary_sha256':sha(primary) if data else None,'actions':len(data['actions']) if data else 0,
        'complete':bool(data and data['complete']),'capture':str(folder/'capture.jsonl') if record and not failed_recorder else None}
    (folder/'result.json').write_text(json.dumps(report,indent=2))
    return report

def capture_receipt(result, steps=None):
    if result['exit_code']!=0 or not result['capture']:raise ValueError('Native capture did not finish')
    header=footer=None;decisions=0;searches={};combo=False;actual_stage=0;actual_win=False;giant=None
    with Path(result['capture']).open('rb') as stream:
        for line in stream:
            if not line.endswith(b'\n') or footer is not None:raise ValueError('Capture truncated or follows footer')
            row=json.loads(line);kind=row['kind']
            if kind=='header':
                header=row;giant=next((int(i) for i,c in row['cards'].items() if c['name']=='Lotleth Giant'),None)
            if kind=='decision':
                decisions+=1;e=row.get('evidence') or {};before=row['before']['projection'];after=row['after']['projection']
                controlled=e.get('chooser') in ('new_root','reused')
                if controlled and e.get('spy_self_chosen'):actual_stage=max(actual_stage,1)
                if actual_stage==1 and before['library_counts'][0]>after['library_counts'][0]==0:actual_stage=2
                if actual_stage==2 and controlled and e.get('dr_giant_chosen'):actual_stage=3
                has_giant=lambda p:any(o['stable']['card_db_id']==giant for o in p['battlefield'][0])
                if actual_stage==3 and not has_giant(before) and has_giant(after):actual_stage=4
                if row.get('terminal'):actual_win=terminal(row['terminal'])['winner']=='p0'
            if kind=='search':searches[row['index']]=None
            if kind=='search_end':searches[row['search']]=row['complete']
            if kind=='search_simulation':
                s=row['simulation'];stage=0
                for m in s['rollout']:
                    if m['actor']=='p0' and m['flags']&2:stage=max(stage,1)
                    if stage==1 and m['resolution']&2:stage=2
                    if stage==2 and m['actor']=='p0' and m['flags']&4:stage=3
                    if stage==3 and m['resolution']&4:stage=4
                combo|=stage==4 and s['reason']=='win'
            if kind=='footer':footer=row
    if not header or not footer or footer.get('stop_reason') or footer['written']!=decisions or decisions!=result['actions'] or not searches or any(v is False for v in searches.values()):
        raise ValueError('Recorder lost gameplay decisions or requested full search')
    if steps is not None and decisions!=steps:raise ValueError('Short correctness case did not reach its bound')
    return {'written':decisions,'complete':footer['complete'],'traced_searches':sum(v is True for v in searches.values()),'winning_combo_rollout':combo,'actual_search_combo_win':actual_stage==4 and actual_win}

def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('mode',choices=('qualify','capture'));ap.add_argument('--binary',type=Path,required=True)
    ap.add_argument('--template',type=Path,required=True);ap.add_argument('--root',type=Path,required=True);ap.add_argument('--inventory',type=Path,required=True)
    ap.add_argument('--attempt',default='',help='Retain a separate qualification namespace under the same bounded artifact root')
    args=ap.parse_args();args.root.mkdir(exist_ok=True,parents=True)
    if args.attempt and not re.fullmatch('[a-z0-9-]{1,48}',args.attempt):raise ValueError('Invalid attempt namespace')
    case_root=args.root/args.attempt if args.attempt else args.root;case_root.mkdir(exist_ok=True)
    guard=require_guard(args.root,'spy-full-search-trace-239')
    if len(guard['cores'])<2:raise ValueError('Two admitted cores required for useful-work qualification')
    pinned=pins(args.binary,args.template);inventory=json.loads(args.inventory.read_text())
    if not all(k in inventory for k in ('jacks_pc','haleys_pc','runpod')):raise ValueError('Three-host placement inventory missing')
    qualification=case_root/'qualification.json'
    if args.mode=='qualify':
        if qualification.exists():raise ValueError('Preserve existing qualification')
        results=[]
        for name,record,failed in [('parity-off',False,False),('parity-on',True,False),('parity-recorder-failed',True,True)]:
            reserve_output(args.root,int(record and not failed))
            results.append(invoke(args.binary,args.template,case_root,name,record,185,600,failed))
        if any(r['exit_code']!=0 or r['actions']!=185 for r in results) or len({r['primary_sha256'] for r in results})!=1:raise ValueError('Off/on/failed recorder primary parity differs')
        capture_receipt(results[1],185)
        started=time.perf_counter();serial=[]
        for i in range(2):
            reserve_output(args.root,1)
            serial.append(invoke(args.binary,args.template,case_root,f'serial-{i}',True,185,600))
        serial_seconds=time.perf_counter()-started
        reserve_output(args.root,2)
        started=time.perf_counter()
        with concurrent.futures.ThreadPoolExecutor(2) as pool:
            jobs=[pool.submit(invoke,args.binary,args.template,case_root,f'parallel-{i}',True,185,600) for i in range(2)]
            parallel=[j.result() for j in jobs]
        parallel_seconds=time.perf_counter()-started
        for r in serial+parallel:
            capture_receipt(r,185)
            if r['primary_sha256']!=results[0]['primary_sha256']:raise ValueError('Scaling changed actual game or native search output')
        report={'schema':'mtg-kernel-spy-viewer-qualification/v1','pins':pinned,'cores':guard['cores'],'inventory_sha256':sha(args.inventory),
            'primary_parity_sha256':results[0]['primary_sha256'],'serial_seconds':serial_seconds,'parallel_seconds':parallel_seconds,
            'workers':2 if parallel_seconds<serial_seconds else 1,'results':results+serial+parallel,
            'scope':'Two identical independently executed short cases, policy-generated opening and 128000-transition UCT search, inference, stepping, tracing and serialization included. No strength claim.'}
        qualification.write_text(json.dumps(report,indent=2));print(json.dumps(report),flush=True)
    else:
        q=json.loads(qualification.read_text())
        if q['pins']!=pinned or q['cores']!=guard['cores'] or q['inventory_sha256']!=sha(args.inventory):raise ValueError('Missing or incompatible throughput qualification')
        # Validate complete search tables even though short qualification games stop at step185.
        from viewer_uct_export_v1 import export_capture
        for result in q['results']:
            if result.get('capture'):
                path=Path(result['capture'])
                export_capture(path,Exporter({},'p0',None,path.parent/'unused-assets',False),allow_incomplete_game=True)
        acquisition=case_root/'acquisition.json'
        if acquisition.exists():raise ValueError('Preserve existing acquisition and interrupted attempts')
        budget_path=args.root/'acquisition-budget.json'
        budget=json.loads(budget_path.read_text()) if budget_path.exists() else {'games_started':0,'seconds_used':0.,'active':None}
        if budget['active']:raise ValueError('Reconcile the retained active acquisition before any new game')
        previous_seconds=budget['seconds_used']
        started=time.monotonic();results=[];report={'state':'started','pins':pinned,'max_games':3,'max_seconds':5400,'raw_cap':RAW_CAP,'qualification_sha256':sha(qualification),'results':results,
            'allocation':'One sequential fresh execution minimizes latency to one traced combo; independent two-worker work was qualified. Reused development setup, new frozen-policy gameplay, no saved actions.'}
        acquisition.write_text(json.dumps(report,indent=2))
        for i in range(3):
            require_guard(args.root,'spy-full-search-trace-239');remaining=int(5400-previous_seconds-(time.monotonic()-started))
            # Each native recording caps itself at 512 MiB. Reserve that entire next output.
            allowance=512*1024**2+8192
            if budget['games_started']>=3 or remaining<=0 or size(args.root,True)+allowance>RAW_CAP or size(args.root)+allowance>WORKING_CAP:break
            budget['games_started']+=1;budget['active']={'folder':str(case_root/f'game-{i+1}'),'timeout_seconds':remaining,'started_unix':time.time()}
            budget_path.write_text(json.dumps(budget,indent=2))
            result=invoke(args.binary,args.template,case_root,f'game-{i+1}',True,100000,remaining);results.append(result)
            budget.update(active=None,seconds_used=previous_seconds+time.monotonic()-started)
            budget_path.write_text(json.dumps(budget,indent=2))
            try:result['receipt']=capture_receipt(result)
            except ValueError as e:result['capture_error']=str(e)
            report['raw_bytes']=size(args.root,True);report['elapsed_seconds']=time.monotonic()-started
            acquisition.write_text(json.dumps(report,indent=2))
            budget['seconds_used']=previous_seconds+time.monotonic()-started
            budget_path.write_text(json.dumps(budget,indent=2))
            if result.get('receipt',{}).get('complete') and result['receipt']['winning_combo_rollout'] and result['receipt']['actual_search_combo_win']:break
            if result['complete']:break  # Repeating this completed deterministic case adds no new coverage.
        success=any(r.get('receipt',{}).get('complete') and r['receipt']['winning_combo_rollout'] and r['receipt']['actual_search_combo_win'] for r in results)
        report['state']='complete' if success else 'insufficient-demo-coverage';acquisition.write_text(json.dumps(report,indent=2));print(json.dumps(report),flush=True)
        budget['seconds_used']=previous_seconds+time.monotonic()-started
        budget_path.write_text(json.dumps(budget,indent=2))
        if not success:raise SystemExit(1)

if __name__=='__main__':main()
