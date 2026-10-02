"""Prepare the exact shared panel. This tool cannot launch a native process."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import numpy as np

G115='88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1'
def read(path):return json.loads(Path(path).read_bytes())
def pin(path):
    path=Path(path)
    return dict(path=str(path.resolve()),sha256=hashlib.sha256(path.read_bytes()).hexdigest())
def checked(ref):
    path=Path(ref['path'])
    if pin(path)['sha256']!=ref['sha256']:raise ValueError('Input hash changed: '+str(path))
    return path
def digest(value):return hashlib.sha256(json.dumps(value,sort_keys=True,separators=(',',':')).encode()).hexdigest()

def make_panel(plan,metadata,seeds):
    pairs=metadata['pairs']
    if len(pairs)!=64 or len({tuple(p) for p in pairs})!=64:raise ValueError('Expected64 distinct matchup strata')
    if seeds.shape!=(64,8) or len(set(seeds.ravel().tolist()))!=512:raise ValueError('Expected512 distinct seed blocks')
    own=plan['endpoints']['g115'];opponent=plan['opponent']
    if own['source']['checkpoint']['sha256']!=G115:raise ValueError('Not the g115 lineage')
    if opponent['kind']!='legacy' or opponent['source']['checkpoint'] is not None or not opponent['v3_forced_actions'] or not opponent['v3_spell_target_reference_adapter']:
        raise ValueError('Frozen V3 opponent or adapters changed')
    cells={tuple(p):i for i,p in enumerate(pairs)}
    references={};base_jobs=[]
    for job in plan['jobs']:
        if job['arm']!='g115':continue
        command=job['command'];sources=command['sources']
        seats=[i for i,s in enumerate(sources) if s==own]
        if len(seats)!=1:raise ValueError('Ambiguous candidate seat')
        seat=seats[0]
        if sources[1-seat]!=opponent or command['cross_generation_evaluation'] is not True:
            raise ValueError('Shared opponent or cross-generation contract changed')
        for index,match in enumerate(command['matches']):
            decks=match['config']['deck_ids'];pair=(decks[seat],decks[1-seat]);cell=cells[pair]
            matches=np.flatnonzero(seeds[cell]==match['config']['seed'])
            if len(matches)!=1:raise ValueError('Match seed outside shared panel')
            replica=int(matches[0]);key=(cell,replica,seat)
            if key in references:raise ValueError('Duplicate shared condition')
            references[key]=(job['id'],index)
            # Retain the full native input, including registered/postboard lists.
            # It is a template, not a runnable command until final binding.
            native=copy.deepcopy(command);native['matches']=[copy.deepcopy(match)];native['output_directory']='UNBOUND'
            base_jobs.append(dict(condition_id=f'c{cell:02}-r{replica}-s{seat}',cell=cell,replica=replica,candidate_seat=seat,
                seed=int(seeds[cell,replica]),own_deck=pair[0],opponent_deck=pair[1],
                archive_job=job['id'],archive_match_index=index,match_sha256=digest(match),base_command=native))
    expected={(c,r,s) for c in range(64) for r in range(8) for s in range(2)}
    if set(references)!=expected:raise ValueError('Missing shared conditions')
    jobs=[]
    for j in sorted(base_jobs,key=lambda j:(j['cell'],j['replica'],j['candidate_seat'])):
        for arm in ('baseline','search'):
            jobs.append(dict(**j,id=j['condition_id']+'-'+arm,arm=arm,
                candidate_transform='none' if arm=='baseline' else 'REVIEW_PENDING_FUTURE_RNG_ISOLATED_V4_SEARCH'))
    return dict(schema='g115-d3-panel-preparation/v1',launchable=False,reason='Pending reviewed search version, final runtime/model binding and guarded allocation.',
        matches_per_arm=1024,paired_seed_blocks=512,pairs=pairs,seeds=seeds.tolist(),
        candidate=own,opponent=opponent,search_limits=dict(simulations=128,transitions=1024,depth=8,seed=20260922),
        jobs=jobs,scope='Read-only derivation from shared panel. No outcomes, new seeds, engine invocation or model selection.')

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--plan',type=Path,required=True);parser.add_argument('--vectors',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
    metadata=read(args.vectors)
    if pin(args.plan)['sha256']!=metadata['plan']['sha256']:raise ValueError('Panel differs from power source')
    checked(metadata['values']);seeds=np.load(checked(metadata['seeds']),allow_pickle=False)
    plan=read(args.plan)
    for source in (plan['endpoints']['g115'],plan['opponent']):
        for name in ('checkpoint','play_import'):
            if source['source'][name] is not None:checked(source['source'][name])
    result=make_panel(plan,metadata,seeds)
    result['provenance']=dict(plan=pin(args.plan),vectors=pin(args.vectors),script=pin(__file__))
    with args.output.open('x',encoding='utf-8') as f:json.dump(result,f,indent=2)
    print(json.dumps(dict(output=pin(args.output),jobs=len(result['jobs']),launchable=False)))
if __name__=='__main__':main()
