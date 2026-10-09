"""Reciprocal g115/A48 policy assignment with immutable prior-match anchors."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import copy
from pathlib import Path
import numpy as np
from prepare_postboard_qualification_v1 import read, write, pin
from run_postboard_pilot_v1 import verify_pin
from postboard_bo3_qualification_v1 import execute
from g115_opponent_profile_v1 import checkpoint_metadata

PRIOR = Path('E:/mtg-postboard-campaign-20260920/opponent-profile-002')


def prepare(root):
    prior = read(PRIOR / 'manifest.json')
    assert read(PRIOR / 'analysis.json')['complete']
    audited = read(PRIOR / 'audited-matches.json')['rows']
    anchors = [r for r in audited if r['reference'] == 'a48' and r['own'].startswith('published-')]
    assert len(anchors) == 224
    for row in anchors: verify_pin(row['match'])
    verify_pin(prior['binary'])
    for source in (prior['candidate'], prior['references']['a48']):
        checkpoint_metadata(**source['checkpoint']); verify_pin(source['play_import'])
    root.mkdir()
    for name in ('configs', 'outputs', 'receipts'): (root / name).mkdir()
    write(root / 'anchors.json', anchors)
    jobs = []
    for old in prior['jobs']:
        if old['reference'] != 'a48' or not old['cases'][0]['own'].startswith('published-'): continue
        before = read(verify_pin(old['config']))
        after = copy.deepcopy(before)
        after['model_sources'].reverse()
        after['output_directory'] = (root / 'outputs' / old['name']).as_posix()
        assert after['matches'] == before['matches'] and after['policies'] == before['policies']
        file = root / 'configs' / (old['name'] + '.json'); write(file, after)
        jobs.append({'name': old['name'], 'config': pin(file), 'anchor_config': old['config'],
                     'cases': old['cases'], 'additional_list_seat': old['seat']})
    assert len(jobs) == 16 and sum(len(j['cases']) for j in jobs) == 224
    preflight = []
    for i, job in enumerate(jobs[:2]):
        c = read(job['config']['path']); c['matches'] = c['matches'][:1]
        name = f'preflight-{i}'; c['output_directory'] = (root / 'outputs' / name).as_posix()
        file = root / 'configs' / (name + '.json'); write(file, c)
        preflight.append(dict(job, name=name, config=pin(file), cases=job['cases'][:1]))
    replay = dict(preflight[0], name='replay')
    c = read(replay['config']['path']); c['output_directory'] = (root / 'outputs/replay').as_posix()
    file = root / 'configs/replay.json'; write(file,c); replay['config'] = pin(file)
    manifest = {'schema': 'g115-deck-assignment-crossover/v1', 'script':pin(__file__),
        'binary':prior['binary'], 'candidate':prior['candidate'], 'a48':prior['references']['a48'],
        'prior_manifest':pin(PRIOR/'manifest.json'), 'prior_analysis':pin(PRIOR/'analysis.json'),
        'anchors':pin(root/'anchors.json'), 'jobs':jobs, 'preflight':preflight, 'replay':replay,
        'gates':{'max_job_seconds':60,'max_worker_seconds':900,'workers':4,'expected_new_matches':224,
                 'preflight_max_worker_seconds':12,'max_projected_worker_seconds':900},
        'question':'Separate marginal deck-assignment and model-score patterns by crossing g115/A48 over the same additional/canonical list pairs.',
        'analysis':{'primary':'G1 score. Let A be additional-list score with g115 and B its score with A48. Balanced g115 score=(A+1-B)/2; balanced additional-list score=(A+B)/2.',
            'paired_unit':'112 environment cases retaining both physical seats and both reciprocal assignments.',
            'bootstrap':'10000 paired resamples of 14 opponent/seed cases within each of eight additional-list strata, PCG64 seed202609201809.',
            'secondary':'Keep-sideboard BO3 score with the same decomposition.',
            'gates':'Complete-only descriptive diagnostic; no promotion, futility or per-list significance gate.',
            'scope':'All eight earlier selected additional lists; no selection of extreme cells. Explicit reuse of completed anchors, not a new independent replication.'},
        'limits':['A48 is a familiar training opponent despite different fresh initialization.',
            'Crossover balances these deck assignments, but cannot separate intrinsic deck quality from shared policy blind spots, model/deck interactions or engine/observation errors.',
            'Keep7, Keep sideboards, sampled policy, no search; no learned-opening/postboard or human-strength claim.',
            'Fable zero-read HTTP429 until Sep22 07:00 EDT; no retry or endorsement. The maintainer authorized bounded local continuation.',
            'No CP7 selection, paid compute, training or model promotion.']}
    write(root/'manifest.json',manifest)


def audit(job, manifest):
    c=read(verify_pin(job['config'])); directory=Path(c['output_directory'])
    assert read(directory/'completion.json')['completed_matches']==len(job['cases'])
    assert len(list(directory.glob('match-*.json')))==len(job['cases'])
    assert read(directory/'run-start.json')['binary']['sha256']==manifest['binary']['sha256']
    rows=[]
    for i,(item,case) in enumerate(zip(c['matches'],job['cases'])):
        file=directory/f'match-{i:06}.json'; m=read(file); additional_seat=case['seat']
        assert m['config']==item['config'] and m['seat_generations']==['v4','v4']
        assert m.get('search_usage') is None
        for seat in (0,1):
            source=manifest['a48'] if seat==additional_seat else manifest['candidate']
            checkpoint=checkpoint_metadata(**source['checkpoint'])
            model=m['play_models'][seat]
            assert model['checkpoint_sha256']==source['checkpoint']['sha256']
            assert model['state_sha256']==checkpoint['state_sha256'] and model['source_import']==checkpoint['source_import']
            assert {k:m['explicit_registrations'][seat][k] for k in ('label','mainboard','sideboard')}==item['registered'][seat]
        hashes=[r['mainboard_sha256'] for r in m['explicit_registrations']]
        assert 2<=len(m['games'])<=6 and all(g['mainboard_sha256']==hashes for g in m['games'])
        assert all(r['selected_actions']==[{'kind':'done'}] for r in m['sideboard_decisions'])
        assert {(r['input']['next_game_number'],r['acting_player']) for r in m['sideboard_decisions']}=={(g['start']['game_index'],s) for g in m['games'][1:] for s in (0,1)}
        assert (m['games'][0]['start']['starting_player']==additional_seat)==(case['replicate']==0)
        winner=None if m['outcome']=='draw' else m['outcome']['winner']['winner']; g1=m['games'][0]['winner']
        rows.append(dict(case, additional_g1_score=.5 if g1 is None else float(g1==additional_seat),
            additional_bo3_score=.5 if winner is None else float(winner==additional_seat),
            games=len(m['games']),g1_draw=g1 is None,bo3_draw=winner is None,match=pin(file)))
    return rows


def run(root):
    m=read(root/'manifest.json'); assert pin(__file__)==m['script']; verify_pin(m['binary'])
    checks=[execute(root,j,m) for j in m['preflight']+[m['replay']]]
    assert all(r['exit_code']==0 and not r['timed_out'] for r in checks)
    for j in m['preflight']+[m['replay']]: audit(j,m)
    a=Path(read(m['preflight'][0]['config']['path'])['output_directory'])/'match-000000.json'; b=root/'outputs/replay/match-000000.json'
    assert a.read_bytes()==b.read_bytes()
    seconds=sum(r['elapsed_seconds'] for r in checks); projected=seconds/3*224
    write(root/'preflight-result.json',{'worker_seconds':seconds,'projected_worker_seconds':projected,'byte_identical_replay':pin(b),'results':checks})
    assert seconds<=12 and projected<=900
    print({'preflight':'PASS','projected_worker_seconds':projected},flush=True)
    with ThreadPoolExecutor(max_workers=4) as pool: results=list(pool.map(lambda j:execute(root,j,m),m['jobs']))
    write(root/'execution.json',results)
    assert all(r['exit_code']==0 and not r['timed_out'] for r in results),'Incomplete: preserve output and do not analyze'
    rows=[r for j in m['jobs'] for r in audit(j,m)]
    assert len(rows)==224 and len({(r['own'],r['other'],r['replicate'],r['seat']) for r in rows})==224
    seconds+=sum(r['elapsed_seconds'] for r in results); assert seconds<=900
    write(root/'audited-matches.json',{'rows':rows,'worker_seconds_including_preflight':seconds})
    print({'new_matches':224,'natural_games':sum(r['games'] for r in rows),'worker_seconds':seconds},flush=True)


def analyze(root):
    m=read(root/'manifest.json'); assert pin(__file__)==m['script']
    anchors=read(verify_pin(m['anchors'])); new=read(root/'audited-matches.json'); rows=new['rows']
    key=lambda r:tuple(r[k] for k in ('own','other','replicate','seat'))
    before={key(r):r for r in anchors}; after={key(r):r for r in rows}
    assert len(before)==len(after)==224 and before.keys()==after.keys()
    owns=sorted({r['own'] for r in rows}); others=sorted({r['other'] for r in rows})
    values=np.empty((8,14,2,2,2))
    for i,own in enumerate(owns):
        for j,other in enumerate(others):
            for rep in range(2):
                seeds=set()
                for seat in (0,1):
                    a,b=before[(own,other,rep,seat)],after[(own,other,rep,seat)]
                    verify_pin(a['match']); verify_pin(b['match']); seeds.update((a['seed'],b['seed']))
                    # Compare the literal game configs/registrations, not just labels.
                    am,bm=read(a['match']['path']),read(b['match']['path'])
                    assert am['config']==bm['config'] and am['explicit_registrations']==bm['explicit_registrations']
                    assert am['play_models']==list(reversed(bm['play_models']))
                    values[i,j*2+rep,seat,0]=[a['g1_score'],a['bo3_score']]
                    values[i,j*2+rep,seat,1]=[b['additional_g1_score'],b['additional_bo3_score']]
                assert len(seeds)==1
    rng=np.random.Generator(np.random.PCG64(202609201809)); samples=np.empty((10000,8,2,2))
    for i in range(8): samples[:,i]=values[i,rng.integers(0,14,size=(10000,14))].mean(axis=(1,2))
    def summarize(v,s):
        means=v.mean(axis=(0,1,2)); boot=s.mean(axis=1)
        return {metric:{
            'additional_with_g115':float(means[0,k]),'additional_with_a48':float(means[1,k]),
            'g115_on_canonical':float(1-means[1,k]),
            'balanced_g115_score':float((means[0,k]+1-means[1,k])/2),
            'balanced_g115_95':np.quantile((boot[:,0,k]+1-boot[:,1,k])/2,[.025,.975]).tolist(),
            'balanced_additional_list_score':float(means[:,k].mean()),
            'balanced_additional_list_95':np.quantile(boot[:,:,k].mean(axis=1),[.025,.975]).tolist()
        } for k,metric in enumerate(('g1','bo3'))}
    result={'complete':True,'new_matches':224,'reused_anchor_matches':224,'total_assignments':448,
        'new_natural_games':sum(r['games'] for r in rows),'new_g1_draws':sum(r['g1_draw'] for r in rows),
        'new_bo3_draws':sum(r['bo3_draw'] for r in rows),'worker_seconds_including_preflight':new['worker_seconds_including_preflight'],
        'summary':summarize(values,samples),'per_additional_list':[{'own':own,**summarize(values[i:i+1],samples[:,i:i+1])} for i,own in enumerate(owns)],
        'manifest':pin(root/'manifest.json'),'promotion':False,'human_strength_claim':False,'limits':m['limits']}
    write(root/'analysis.json',result); print(result['summary'],flush=True)


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('mode',choices=['prepare','run','analyze']);p.add_argument('--root',required=True,type=Path)
    args=p.parse_args();globals()[args.mode](args.root.resolve())
