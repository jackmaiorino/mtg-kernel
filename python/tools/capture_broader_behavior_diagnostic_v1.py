"""Bounded, outcome-conditioned development traces from a completed pilot."""
import argparse
import copy
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

PILOT = Path('E:/mtg-postboard-campaign-20260920/broader-exposure-pilot-001')
BASE = Path('E:/mtg-human-g115-20260920/delivery-002/requests/Rally-vs-Spy.json')
ARMS = ('g115', 'control', 'broader')


def read(p): return json.loads(Path(p).read_bytes())
def pin(p):
    p = Path(p).resolve()
    return {'path': str(p), 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()}
def checked(p):
    assert pin(p['path'])['sha256'] == p['sha256'], p['path']
    return Path(p['path'])
def write(p, value):
    with Path(p).open('x', encoding='utf-8') as f:
        json.dump(value, f, indent=2); f.write('\n')


def prepare(root):
    assert read(PILOT/'analysis.json')['complete']
    rows = [r for arm in ARMS for r in read(PILOT/(arm+'-evaluation-audit.json'))['rows']]
    grouped = {}
    for r in rows:
        grouped.setdefault((r['own'], r['other'], r['replicate']), []).append(r)
    gates = min(k for k in grouped if k[0] == 'published-44ae71e1e126b63d')
    selected = [gates]
    for own in ('Burn', 'Elves'):
        candidates = [k for k, group in grouped.items() if k[0] == own and
            sum(r['g1_win'] for r in group if r['arm']=='g115') >
            sum(r['g1_win'] for r in group if r['arm']=='broader')]
        selected.append(min(candidates))
    root.mkdir(parents=True, exist_ok=False)
    for directory in ('requests', 'outputs', 'receipts'): (root/directory).mkdir()
    base = read(BASE)
    binary = root/'phase1_bo3_collect_v1.exe'
    shutil.copyfile(checked(base['packages'][0]['runtime']['executable']), binary)
    manifest = {'script': pin(__file__), 'base_request': pin(BASE), 'pilot_analysis': pin(PILOT/'analysis.json'),
        'binary': pin(binary), 'selection': selected, 'jobs': [],
        'selection_rule': 'Lexicographically first Gates case; first Burn and Elves cases where paired g115 G1 wins exceed broader. All three arms and both physical seats retained.',
        'limits': 'Outcome-conditioned development diagnosis, not a win-rate estimate or confirmation. A48 familiar; no source/model/input changes. Existing evaluation has no action traces, so matching its summaries does not prove identical internal trajectories.',
        'cost': {'max_job_seconds': 60, 'total_worker_seconds': 240, 'workers': 4, 'planned_executions_with_replay': 19},
        'review_gap': 'Known zero-read Fable HTTP429 until September22 07:00EDT, no retry; authorized bounded local diagnosis.'}
    sources = {'g115': read(PILOT/'manifest.json')['initial_source'],
        **{arm: read(PILOT/arm/'training-audit.json')['source'] for arm in ('control','broader')}}
    a48 = read(PILOT/'manifest.json')['a48']
    for group_index, key in enumerate(selected):
        for arm in ARMS:
            for seat in (0, 1):
                row = next(r for r in grouped[key] if r['arm']==arm and r['seat']==seat)
                original = read(checked(row['match']))
                request = copy.deepcopy(base)
                cfg = original['config']
                name = f'case{group_index}-{arm}-seat{seat}'
                request['config'].update(match_id='broader-behavior-'+name, seed=cfg['seed'],
                    initial_chooser='p'+str(cfg['game_one_chooser']), deck_ids=cfg['deck_ids'],
                    registrations=[{k:r[k] for k in ('mainboard','sideboard')} for r in original['explicit_registrations']],
                    **{k:cfg[k] for k in ('max_physical_games','max_physical_decisions','max_policy_steps')})
                assert cfg['opening_protocol']=='keep_seven_v2'
                for s, package in enumerate(request['packages']):
                    package['runtime']['executable'] = pin(binary)
                    source = sources[arm] if s==seat else a48
                    package['gameplay'] = {'source': source, 'identity': original['play_models'][s]}
                    for p in (source['checkpoint'], source['play_import']): checked(p)
                    assert package['opening']=={'kind':'existing','protocol':'keep_seven_v2'}
                    assert package['play_draw']=={'kind':'fixed','choice':'play'}
                    assert package['sideboard']=={'kind':'keep'} and package['search']=={'kind':'disabled'}
                path = root/'requests'/(name+'.json'); write(path, request)
                manifest['jobs'].append({'name':name,'case':group_index,'row':row,'request':pin(path)})
    assert len(manifest['jobs'])==18
    write(root/'manifest.json', manifest)
    print({'prepared_jobs':18,'selected_cases':selected})


def execute(root, m, job, suffix=''):
    name = job['name']+suffix
    command = [str(checked(m['binary'])), '--request', str(checked(job['request'])),
               '--output', str(root/'outputs'/(name+'.json'))]
    start = time.monotonic()
    with (root/'receipts'/(name+'.stdout.log')).open('xb') as out, (root/'receipts'/(name+'.stderr.log')).open('xb') as err:
        child = subprocess.Popen(command, stdout=out, stderr=err,
            creationflags=subprocess.CREATE_NO_WINDOW|subprocess.BELOW_NORMAL_PRIORITY_CLASS)
        write(root/'receipts'/(name+'.start.json'), {'pid':child.pid,'epoch':time.time(),'command':command})
        timeout=False
        try: code=child.wait(timeout=60)
        except subprocess.TimeoutExpired: child.kill(); code=child.wait(); timeout=True
    receipt={'name':name,'exit_code':code,'timed_out':timeout,'seconds':time.monotonic()-start}
    write(root/'receipts'/(name+'.json'), receipt)
    assert code==0 and not timeout, receipt
    return receipt


def audit_job(root, job, suffix=''):
    path = root/'outputs'/(job['name']+suffix+'.json')
    result = read(path); request=read(checked(job['request'])); old=read(checked(job['row']['match']))
    assert result['config']==request['config'] and result['packages']==request['packages']
    collected = result['collected']; t=collected['trajectory']
    assert t['ending']['kind']=='complete' and len(t['games'])==len(old['games'])==len(collected['games'])
    for game, diagnostic, previous in zip(t['games'], collected['games'], old['games']):
        assert game['terminal']['classification']=='natural'
        terminal=diagnostic['observed_terminal']
        assert terminal['terminal_classification']=='natural'
        winner=terminal['winner']; expected=None if previous['winner'] is None else 'p'+str(previous['winner'])
        assert winner==expected and diagnostic['environment_seed']==previous['environment_seed']
        assert game['start']==previous['start']
        assert not diagnostic['discarded_pending_selections'] and diagnostic['error'] is None
    return {'name':job['name']+suffix,'output':pin(path),'games':len(t['games']),
        'decisions':collected['committed_decision_records'],'matches_original_summary':True}


def run(root):
    m=read(root/'manifest.json'); assert m['script']==pin(__file__)
    first=m['jobs'][0]; results=[execute(root,m,first)]; audits=[audit_job(root,first)]
    results.append(execute(root,m,first,'-replay')); replay=audit_job(root,first,'-replay')
    assert audits[0]['output']['sha256']==replay['output']['sha256'], 'Same-request replay must be byte-identical'
    projected=max(r['seconds'] for r in results)*19
    write(root/'qualification.json',{'passed':projected<=240,'projected_worker_seconds':projected,'replay':replay})
    assert projected<=240, 'Trace collection too expensive for the bounded question'
    with ThreadPoolExecutor(max_workers=4) as pool:
        results += list(pool.map(lambda job:execute(root,m,job),m['jobs'][1:]))
    audits += [audit_job(root,job) for job in m['jobs'][1:]]
    seconds=sum(r['seconds'] for r in results)
    assert seconds<=240
    write(root/'completion.json',{'complete':True,'matches':len(audits),'games':sum(a['games'] for a in audits),
        'decisions':sum(a['decisions'] for a in audits),'worker_seconds':seconds,'audits':audits,'limits':m['limits']})
    print({'complete':True,'matches':len(audits),'worker_seconds':seconds})


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('mode',choices=('prepare','run'));parser.add_argument('--root',required=True,type=Path)
    args=parser.parse_args(); globals()[args.mode](args.root.resolve())
