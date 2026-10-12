"""Guarded bounded viewer acquisition. Invoke inside host_slots_v1.py run."""
from __future__ import annotations
import argparse
import concurrent.futures
import hashlib
import json
import os
import shutil
import subprocess
import time
from pathlib import Path

RAW_CAP = 2 * 1024**3
RESERVE = 60 * 1024**3

def sha(path: Path) -> str:
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''): h.update(block)
    return h.hexdigest()

def pin_inputs(binary: Path, source: Path) -> dict:
    data = json.loads(source.read_text(encoding='utf-8'))
    pins = {'binary':{'path':str(binary),'sha256':sha(binary)},'source':{'path':str(source),'sha256':sha(source)}}
    for name in ('play_import','checkpoint'):
        if data.get(name):
            item = data[name]
            if sha(Path(item['path'])) != item['sha256']: raise ValueError(f'{name} differs from its pin')
            pins[name] = item
    return pins

def used_bytes(root: Path) -> int:
    return sum(p.stat().st_size for p in root.rglob('*') if p.is_file())

def capture_complete(result: dict) -> bool:
    if result['exit_code'] != 0 or not result.get('capture'):
        return False
    try:
        footer, natural = None, False
        with Path(result['capture']).open('rb') as stream:
            for line in stream:
                if not line.endswith(b'\n') or footer is not None:
                    return False
                row = json.loads(line)
                terminal = row.get('terminal')
                natural |= isinstance(terminal, dict) and terminal.get('terminal_classification') == 'natural'
                if row.get('kind') == 'footer':
                    footer = row
        return bool(footer and footer.get('complete') and natural)
    except (OSError, ValueError, TypeError):
        return False

def verify_development_capture(result: dict, steps: int, replay_steps: tuple[int, ...]):
    rows=[json.loads(line) for line in Path(result['capture']).open(encoding='utf-8')]
    decisions=[r for r in rows if r['kind']=='decision']
    if len(decisions) != steps or rows[-1].get('written') != steps:
        raise ValueError('Development recorder lost a decision; inspect retained capture footer')
    for r in decisions:
        if r['decision']['step'] in replay_steps and r['decision']['acting_player']=='p0':
            if not r['search'].get('replay',{}).get('complete'):
                raise ValueError('Requested development search replay unavailable')

def require_guard(root: Path, lane: str = 'cinematic-viewer-232'):
    claim = os.environ.get('HOST_SLOTS_CLAIM')
    if not claim: raise ValueError('Use host_slots_v1.py run; unguarded capture refused')
    from host_slots_v1 import slots_dir, read_json, conflicts
    row = read_json(slots_dir() / f'claim-{claim}.json')
    if not row or row.get('lane') != lane or row.get('state') != 'running' or conflicts(row['cores']):
        raise ValueError('No active nonconflicting viewer claim')
    if shutil.disk_usage(root).free < RESERVE: raise ValueError('Capture volume below 60 GiB reserve')
    return row

def invoke(binary: Path, source: Path, root: Path, name: str, seed: int, record: bool, steps: int, seconds: int, replay_steps: tuple[int, ...] = ()) -> dict:
    folder = root / name
    folder.mkdir()
    config = {'source':str(source.resolve()),'output':str((folder/'capture.jsonl').resolve()),
        'primary':str((folder/'primary.jsonl').resolve()),'seed':seed,'episode':1,'decks':[5,1],
        'record':record,'max_steps':steps,'max_seconds':seconds,'search_replay_steps':list(replay_steps)}
    config_path = folder/'config.json'
    config_path.write_text(json.dumps(config),encoding='utf-8')
    started = time.perf_counter()
    with (folder/'process.log').open('wb') as log:
        try:
            process = subprocess.run([str(binary),str(config_path)],stdout=log,stderr=log,timeout=seconds+10)
            code = process.returncode
        except subprocess.TimeoutExpired:
            code = -1
    duration = time.perf_counter()-started
    primary = folder/'primary.jsonl'
    result = {'name':name,'seconds':duration,'exit_code':code,'seed':seed,
        'primary_sha256':sha(primary) if primary.exists() else None,
        'selected_steps':sum(1 for line in primary.open() if '"selected"' in line) if primary.exists() else 0,
        'capture':str(folder/'capture.jsonl') if record else None}
    (folder/'result.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
    return result

def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('mode',choices=('qualify','capture'))
    ap.add_argument('--binary',type=Path,required=True)
    ap.add_argument('--source',type=Path,required=True)
    ap.add_argument('--root',type=Path,required=True)
    ap.add_argument('--inventory',type=Path,required=True)
    ap.add_argument('--lane',default='cinematic-viewer-232')
    ap.add_argument('--replay-steps',type=int,nargs='*',default=[])
    ap.add_argument('--qualification-steps',type=int,default=8)
    ap.add_argument('--max-games',type=int,default=3)
    ap.add_argument('--previous-acquisition',type=Path)
    ap.add_argument('--seed-base',type=int,default=2026101201)
    args = ap.parse_args()
    args.root.mkdir(parents=True,exist_ok=True)
    if len(args.replay_steps) > 3 or len(set(args.replay_steps)) != len(args.replay_steps) or any(n < 0 for n in args.replay_steps):
        raise ValueError('At most three distinct nonnegative replay steps')
    replay_steps = tuple(args.replay_steps)
    if not 1 <= args.qualification_steps <= 32 or not 1 <= args.max_games <= 3: raise ValueError('Invalid bounded development settings')
    prior = json.loads(args.previous_acquisition.read_text()) if args.previous_acquisition else None
    prior_games = len(prior['results']) if prior else 0
    if prior_games + args.max_games > 3: raise ValueError('Development game allowance exceeded')
    check_steps = args.qualification_steps
    if not 0 <= args.seed_base < 2**64-3: raise ValueError('Invalid seed base')
    guard = require_guard(args.root,args.lane)
    pins = pin_inputs(args.binary,args.source)
    inventory = json.loads(args.inventory.read_text())
    if not all(k in inventory for k in ('jacks_pc','haleys_pc','runpod')): raise ValueError('Three-host inventory missing')
    report_path = args.root/'qualification.json'
    if args.mode == 'qualify':
        if report_path.exists(): raise ValueError('Retain the existing qualification; do not overwrite it')
        off = invoke(args.binary,args.source,args.root,'parity-off',2026101201,False,check_steps,180,replay_steps)
        on = invoke(args.binary,args.source,args.root,'parity-on',2026101201,True,check_steps,180,replay_steps)
        if any(r['exit_code'] != 0 or r['selected_steps'] != check_steps for r in (off,on)) or off['primary_sha256'] != on['primary_sha256']:
            raise ValueError('Capture on/off primary parity failed; inspect retained process logs')
        verify_development_capture(on,check_steps,replay_steps)
        serial_start=time.perf_counter()
        serial=[invoke(args.binary,args.source,args.root,f'serial-{i}',2026101201+i,True,check_steps,180,replay_steps) for i in range(2)]
        serial_seconds=time.perf_counter()-serial_start
        parallel_workers=min(2,len(guard['cores']))
        if parallel_workers != 2: raise ValueError('Two admitted cores required for parallel qualification')
        parallel_start=time.perf_counter()
        with concurrent.futures.ThreadPoolExecutor(2) as pool:
            jobs=[pool.submit(invoke,args.binary,args.source,args.root,f'parallel-{i}',2026101201+i,True,check_steps,180,replay_steps) for i in range(2)]
            parallel=[j.result() for j in jobs]
        parallel_seconds=time.perf_counter()-parallel_start
        for a,b in zip(serial,parallel):
            if a['exit_code'] != 0 or b['exit_code'] != 0 or a['selected_steps'] != check_steps or b['selected_steps'] != check_steps or a['primary_sha256'] != b['primary_sha256']:
                raise ValueError('Serial/parallel completed-work parity failed')
            verify_development_capture(a,check_steps,replay_steps)
            verify_development_capture(b,check_steps,replay_steps)
        report={'schema':'mtg-kernel-viewer-qualification/v1','pins':pins,'inventory_sha256':sha(args.inventory),
            'cores':guard['cores'],'replay_steps':args.replay_steps,'qualification_steps':check_steps,'primary_parity_sha256':on['primary_sha256'],'serial_seconds':serial_seconds,
            'parallel_seconds':parallel_seconds,'workers':2 if parallel_seconds < serial_seconds else 1,
            'scope':f'{check_steps} real T512 decisions per case; startup, inference, search, stepping, and recording included. No strength claim.'}
        report_path.write_text(json.dumps(report,indent=2),encoding='utf-8')
        print(json.dumps(report))
    else:
        qualification=json.loads(report_path.read_text())
        if qualification['pins'] != pins or qualification['cores'] != guard['cores'] or qualification.get('replay_steps',[]) != args.replay_steps:
            raise ValueError('Qualification incompatible with binary, inputs or allocation')
        if sha(args.inventory) != qualification['inventory_sha256']: raise ValueError('Inventory changed since qualification')
        started=time.monotonic()
        results=[]
        acquisition=args.root/'acquisition.json'
        if acquisition.exists(): raise ValueError('Acquisition already dispatched; retain and inspect its result')
        if prior and prior['pins']['checkpoint']['sha256'] != pins['checkpoint']['sha256']: raise ValueError('Prior acquisition checkpoint differs')
        prior_bytes = prior['raw_bytes'] if prior else 0
        prior_seconds = sum(r['seconds'] for r in prior['results']) if prior else 0
        acquisition.write_text(json.dumps({'state':'started','max_games':args.max_games,'prior_games':prior_games,'seconds':5400-prior_seconds,'raw_cap':RAW_CAP-prior_bytes,'pins':pins}),encoding='utf-8')
        for i in range(args.max_games):
            if prior_bytes + used_bytes(args.root) >= RAW_CAP or shutil.disk_usage(args.root).free < RESERVE: break
            remaining=int(5400-prior_seconds-(time.monotonic()-started))
            if remaining <= 0: break
            # Each attempt is one independent full game. Stop when a complete demonstration exists.
            result=invoke(args.binary,args.source,args.root,f'game-{i+1}',args.seed_base+i,True,100000,remaining,replay_steps)
            results.append(result)
            if capture_complete(result): break
        complete=[r for r in results if capture_complete(r)]
        report={'state':'complete' if complete else 'insufficient-demo-coverage','results':results,'raw_bytes':used_bytes(args.root),'pins':pins,
            'qualification_sha256':sha(report_path),'allocation':'Sequential first-game acquisition minimizes time to one complete recording; two-worker independent-work throughput qualified.'}
        acquisition.write_text(json.dumps(report,indent=2),encoding='utf-8')
        print(json.dumps(report))
        if not complete: raise SystemExit(1)

if __name__=='__main__': main()
