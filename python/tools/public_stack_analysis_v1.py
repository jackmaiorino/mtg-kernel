"""Complete four-endpoint paired stack-input screen; no prefix interpretation."""
import argparse
from pathlib import Path
import numpy as np
from public_training_dispatch_v2 import read, write, pin, checked

ARMS = ['g115','disabled','permuted','structured']
GATES = dict(net_wins_over_each_control=20, paired_95_lower=0.0,
    net_wins_over_g115=0, game_one_paired_95_lower=-0.05,
    minimum_nonnegative_own_decks=6, maximum_own_deck_net_loss=8,
    bootstrap_replicates=10000, final_update=199)


def bootstrap(values, repeats, seed):
    assert values.ndim == 5 and values.shape[2:] == (2,4,2)
    assert np.all(np.isfinite(values)) and np.all((values >= 0) & (values <= 1))
    rng = np.random.Generator(np.random.PCG64(seed))
    result = np.zeros((repeats,4,2))
    for stratum in values:
        indices = rng.integers(0,len(stratum),size=(repeats,len(stratum)))
        result += stratum[indices].mean(axis=(1,2))/len(values)
    return result


def count_guards(differences):
    assert differences.shape == (8,3)
    assert np.all(differences == np.round(differences))
    return dict(own_deck_breadth=bool(np.all((differences >= 0).sum(axis=0) >= 6)),
        own_deck_retention=bool(np.all(differences >= -8)))


def self_check():
    values = np.zeros((64,8,2,4,2))
    values[:,:,0,:,:] = np.arange(8)[None,:,None,None] % 2
    values[:,:,1] = 1-values[:,:,0]
    samples = bootstrap(values,100,901)
    assert np.array_equal(samples,np.full((100,4,2),.5))
    assert np.array_equal(samples,bootstrap(values,100,901))
    values.fill(.25);values[:,:,:,3,:] += .25
    samples = bootstrap(values,100,901)
    assert np.array_equal(samples[:,3]-samples[:,0],np.full((100,2),.25))
    differences = np.array([[0]*3]*6+[[-8]*3]*2)
    assert all(count_guards(differences).values())
    differences[0,1] = -1
    assert not count_guards(differences)['own_deck_breadth']
    differences[7,2] = -9
    assert not count_guards(differences)['own_deck_retention']


def analyze(root, evaluation_path):
    self_check()
    m = read(root/'manifest.json')
    assert m['schema'] == 'matched-public-stack-screen/v1' and m['gates'] == GATES
    assert m['analysis'] == pin(Path(__file__))
    checked(m['design']);checked(m['evaluation_binary'])
    assert read(checked(m['evaluation_qualification']))['status'] == 'STACK-BO3-AND-LOADED-REPLAY-ENGINEERING-PASS'
    training = read(root/'training-audit.json')
    assert training['complete'] and training['full_natural_games'] == 6000
    evaluation = read(evaluation_path)
    assert evaluation['pilot'] == pin(root/'manifest.json')
    endpoints = dict(g115=dict(kind='legacy',source=m['source'],v3_forced_actions=False))
    for arm in ARMS[1:]:
        item = training['arms'][arm]
        assert item['updates'] == 200 and item['natural_games'] == 2000
        checked(item['checkpoint']);checked(item['optimizer'])
        endpoints[arm] = dict(kind='stack_checkpoint',config=m['training_configs'][arm],checkpoint=item['checkpoint'])
    assert evaluation['endpoints'] == endpoints
    templates = {(a,j['label']):j for a in ARMS for j in m['jobs']}
    assert len(templates) == len(evaluation['jobs']) == 512
    records, seen = {}, set()
    total_games = 0
    for job in evaluation['jobs']:
        key = job['arm'],job['label']
        assert key in templates and key not in seen
        seen.add(key);template = templates[key];seat=template['candidate_seat']
        expected = read(checked(template['template']))
        expected['sources'][seat]=endpoints[job['arm']]
        expected['sources'][1-seat]=m['evaluation_opponent']
        request = read(checked(job['request']))
        assert {k:v for k,v in request.items() if k!='output_directory'} == {k:v for k,v in expected.items() if k!='output_directory'}
        assert not request['capture_decisions'] and request['cross_generation_evaluation']
        execution = read(checked(job['execution']))
        assert execution['exit_code']==0 and not execution['timeout']
        assert execution['binary']==m['evaluation_binary'] and execution['request']==job['request']
        folder=Path(job['output_directory'])
        start,completion=[read(folder/n) for n in ['start.json','completion.json']]
        assert start['command']==request
        assert completion['matches']==len(completion['match_sha256'])==len(template['cases'])==8
        games=decisions=0
        for index,case in enumerate(template['cases']):
            path=folder/f'match-{index:06}.json'
            assert pin(path)['sha256']==completion['match_sha256'][index]
            match=read(path)
            assert match['match']==request['matches'][index] and match['models']==start['models']
            assert not match['decisions'] and match['diagnostic_spell_target_repairs']==[0,0]
            assert len(match['games'])==len(match['seed_resets'])
            assert match['match']['config']['seed']==case['seed']
            assert (match['games'][0]['start']['starting_player']==seat)==case['candidate_starts']
            record_key=case['own'],case['opponent'],case['replicate'],seat,job['arm']
            assert record_key not in records
            winner=None if match['outcome']=='draw' else match['outcome']['winner']['winner']
            g1=match['games'][0]['winner']
            records[record_key]=dict(seed=case['seed'],win=int(winner==seat),draw=int(winner is None),
                g1=int(g1==seat),g1_draw=int(g1 is None),match=pin(path))
            games+=len(match['games']);decisions+=match['decision_count']
        assert games==completion['natural_games'] and decisions==completion['decisions']
        total_games+=games
    assert seen==set(templates) and len(records)==4096
    pairs=sorted({k[:2] for k in records});assert len(pairs)==64
    values=np.empty((64,8,2,4,2))
    summary={a:dict(matches=0,wins=0,draws=0,game_one_wins=0) for a in ARMS}
    breakdown=[]
    for p,pair in enumerate(pairs):
        for seed_index in range(8):
            cluster=[records[(*pair,seed_index,seat,a)] for seat in range(2) for a in ARMS]
            assert len({r['seed'] for r in cluster})==1
            for seat in range(2):
                for a,arm in enumerate(ARMS):
                    row=records[(*pair,seed_index,seat,arm)]
                    values[p,seed_index,seat,a]=[row['win'],row['g1']]
                    s=summary[arm];s['matches']+=1;s['wins']+=row['win'];s['draws']+=row['draw'];s['game_one_wins']+=row['g1']
        for seat in range(2):
            breakdown.append(dict(own=pair[0],opponent=pair[1],seat=seat,
                wins={arm:int(values[p,:,seat,a,0].sum()) for a,arm in enumerate(ARMS)}))
    samples=bootstrap(values,GATES['bootstrap_replicates'],m['bootstrap_seed'])
    comparisons={}
    for a,b in [(3,0),(3,1),(3,2),(1,0),(2,0)]:
        comparisons[f'{ARMS[a]}-minus-{ARMS[b]}']=dict(net_wins=summary[ARMS[a]]['wins']-summary[ARMS[b]]['wins'],
            paired_95_interval=np.quantile(samples[:,a,0]-samples[:,b,0],[.025,.975]).tolist(),
            game_one_paired_95_interval=np.quantile(samples[:,a,1]-samples[:,b,1],[.025,.975]).tolist())
    own_decks={}
    for own in sorted({p[0] for p in pairs}):
        subset=values[[i for i,p in enumerate(pairs) if p[0]==own]]
        assert subset.shape==(8,8,2,4,2)
        ds=bootstrap(subset,GATES['bootstrap_replicates'],m['bootstrap_seed'])
        own_decks[own]={ARMS[b]:dict(net_wins=int((subset[:,:,:,3,0]-subset[:,:,:,b,0]).sum()),
            paired_95_interval=np.quantile(ds[:,3,0]-ds[:,b,0],[.025,.975]).tolist()) for b in range(3)}
    guards=count_guards(np.array([[d[a]['net_wins'] for a in ARMS[:3]] for d in own_decks.values()]))
    controls=[comparisons['structured-minus-'+a] for a in ['disabled','permuted']]
    guards.update(net_wins_over_each_control=all(d['net_wins']>=20 for d in controls),
        paired_positive_vs_each_control=all(d['paired_95_interval'][0]>0 for d in controls),
        no_net_loss_vs_g115=comparisons['structured-minus-g115']['net_wins']>=0,
        game_one_retention=all(comparisons['structured-minus-'+a]['game_one_paired_95_interval'][0]>-.05 for a in ARMS[:3]))
    write(root/'analysis.json',dict(complete=True,matches=4096,natural_games=total_games,summary=summary,
        comparisons=comparisons,own_decks=own_decks,matchup_seat_breakdown=breakdown,gates=guards,
        verdict='REPLICATE-STRUCTURED' if all(guards.values()) else 'NO-ADVANCE',promotion=False,
        pilot=pin(root/'manifest.json'),evaluation=pin(evaluation_path),analysis=pin(__file__),
        draw_rule='Draws count as zero wins; reported separately.',
        limitations='Conditional on one training seed stream and one development opponent. Paired intervals describe evaluation uncertainty only. Deck intervals descriptive, not simultaneous guarantees. No pooled rescue, CP7 selection, promotion or human-strength claim.'))


if __name__=='__main__':
    if not __debug__:raise RuntimeError('Assertions required')
    p=argparse.ArgumentParser();p.add_argument('--root',type=Path);p.add_argument('--evaluation',type=Path);p.add_argument('--self-check',action='store_true');a=p.parse_args()
    if a.self_check:self_check();print('Four-arm paired resampling and breadth boundary checks passed.')
    else:analyze(a.root,a.evaluation)
