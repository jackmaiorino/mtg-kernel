"""Frozen matched broader-card exposure pilot, staged and complete-only."""
import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import copy
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import numpy as np

from prepare_postboard_qualification_v1 import BASE, REPO, read, write, pin
from postboard_bo3_qualification_v1 import execute, seed
from run_postboard_pilot_v1 import verify_pin
from g115_opponent_profile_v1 import checkpoint_metadata

QUALIFICATION=Path('E:/mtg-postboard-campaign-20260920/broader-exposure-qualification-001')
PROFILE=Path('E:/mtg-postboard-campaign-20260920/opponent-profile-002')
ARMS=('g115','control','broader')


def prepare(root):
    q=read(QUALIFICATION/'manifest.json'); assert read(QUALIFICATION/'qualification-result.json')['verdict']=='ENGINEERING-PASS'
    p=read(PROFILE/'manifest.json'); verify_pin(q['binary']);verify_pin(p['binary'])
    template=Path('E:/mtg-postboard-campaign-20260920/pilot-001/preboard/config.json')
    control=read(template);assert len(control['iterations'])==200
    assert control['initial_source']==q['initial_source']
    excluded=set(read(BASE/'catalog/g-settings.json')['excluded_seeds'])
    for f in [template,BASE/'campaign-002-g-block115-cuda.json',Path(q['configs']['preboard']['path'])]:
        excluded.update(x['episode']['seed'] for b in read(f)['iterations'] for x in b['episodes'])
    for i,b in enumerate(control['iterations']):
        for j,x in enumerate(b['episodes']):
            e=x['episode'];assert not e['postboard'] and e['registered']==e['selected']
            e['seed']=seed('g115-broader-exposure-pilot-training-20260920-v1',i,j)
            e['id']=f'broader-exposure-pilot-20260920-i{i}-s{j}'
            assert e['seed'] not in excluded;excluded.add(e['seed'])
    variant_manifest=read('E:/mtg-postboard-campaign-20260920/published-variant-qualification-001/manifest.json')
    decks={}
    for j in variant_manifest['jobs']:
        for item in read(verify_pin(j['config']))['matches']:
            for d in item['registered']:decks[d['label']]=d
    additional=[decks[n] for n in sorted(decks) if n.startswith('published-')]
    canonical=[decks[n] for n in sorted(decks) if not n.startswith('published-')]
    assert len(additional)==8 and len(canonical)==7
    broad=copy.deepcopy(control);bounds=[]
    for b in broad['iterations']:
        counts=Counter(x['episode']['learner_seat'] for x in b['episodes'])
        bounds.append((max(0,5-counts[0]),min(5,counts[1])))
    remaining=500;counters=[0,4];schedule=[]
    for i,b in enumerate(broad['iterations']):
        lower,upper=bounds[i]
        future_min=sum(lo for lo,_ in bounds[i+1:]);future_max=sum(hi for _,hi in bounds[i+1:])
        lower=max(lower,remaining-future_max);upper=min(upper,remaining-future_min)
        assert lower<=upper
        desired=round((i+1)*2.5)-(500-remaining);take1=min(upper,max(lower,desired));remaining-=take1
        positions={s:[j for j,x in enumerate(b['episodes']) if x['episode']['learner_seat']==s] for s in (0,1)}
        chosen=set(positions[1][:take1]+positions[0][:5-take1]);assert len(chosen)==5
        for j,x in enumerate(b['episodes']):
            before=control['iterations'][i]['episodes'][j];e=x['episode'];seat=e['learner_seat']
            if j in chosen:
                deck=additional[counters[seat]%8];counters[seat]+=1
                for zone in ('registered','selected'):e[zone][seat]=copy.deepcopy(deck)
            check=copy.deepcopy(x)
            for zone in ('registered','selected'):check['episode'][zone][seat]=before['episode'][zone][seat]
            assert check==before
            schedule.append({'iteration':i,'slot':j,'seed':e['seed'],'learner_seat':seat,
                'starting_player':e['starting_player'],'opponent':x['opponent'],'substituted':j in chosen,
                'before_label':before['episode']['registered'][seat]['label'],'after_label':e['registered'][seat]['label']})
    assert remaining==0
    substitutions=[r for r in schedule if r['substituted']]
    assert Counter(r['learner_seat'] for r in substitutions)=={0:500,1:500}
    assert sorted(Counter(r['after_label'] for r in substitutions).values())==[125]*8
    root.mkdir()
    for name in ('configs','templates','outputs','receipts','control','broader'):(root/name).mkdir()
    shutil.copyfile(q['binary']['path'],root/'native_expanded_training_run_v1.exe')
    shutil.copyfile(p['binary']['path'],root/'learned_sideboard_v1.exe')
    configs={}
    for arm,c in [('control',control),('broader',broad)]:
        c['output_directory']=(root/arm/'run').as_posix();file=root/arm/'config.json';write(file,c);configs[arm]=pin(file)
    write(root/'paired-training-schedule.json',schedule)
    jobs=[]
    for arm in ARMS:
        for deck in canonical+additional:
            cohort='additional' if deck['label'].startswith('published-') else 'canonical'
            repeats=8 if cohort=='additional' else 4
            for seat in (0,1):
                name=f"{arm}-{deck['label']}-p{seat}";matches=[];cases=[]
                for other in canonical:
                    for rep in range(repeats):
                        order=[deck,other] if seat==0 else [other,deck]
                        environment=seed('g115-broader-exposure-pilot-evaluation-20260920-v1',deck['label'],other['label'],rep)
                        assert environment not in excluded
                        matches.append({'registered':order,'config':{'deck_ids':[d['label'] for d in order],
                            'seed':environment,'game_one_chooser':seat if rep%2==0 else 1-seat,
                            'max_physical_games':6,'max_physical_decisions':4000,'max_policy_steps':40000,'opening_protocol':'keep_seven_v2'}})
                        cases.append({'own':deck['label'],'other':other['label'],'replicate':rep,'seat':seat,
                                      'cohort':cohort,'arm':arm,'seed':environment})
                sources=[p['candidate'],p['references']['a48']] if seat==0 else [p['references']['a48'],p['candidate']]
                c={'mode':'run_population_batch','model_sources':sources,'policies':[{'kind':'keep'}]*2,
                   'cross_generation_evaluation':False,'matches':matches,'output_directory':(root/'outputs'/name).as_posix()}
                file=root/'templates'/(name+'.json');write(file,c)
                jobs.append({'name':name,'arm':arm,'seat':seat,'template':pin(file),'cases':cases})
    assert len(jobs)==90 and sum(len(j['cases']) for j in jobs)==3864
    manifest={'schema':'g115-broader-exposure-development/v1','script':pin(__file__),
        'helpers':[pin(REPO/'python/tools/postboard_bo3_qualification_v1.py'),pin(REPO/'python/tools/run_postboard_pilot_v1.py'),
                   pin(REPO/'python/tools/g115_opponent_profile_v1.py')],
        'binary':pin(root/'learned_sideboard_v1.exe'),'training_binary':pin(root/'native_expanded_training_run_v1.exe'),
        'producer_launch':q['producer_launch'],'producer_source_commit':q['producer_source_commit'],
        'qualification':pin(QUALIFICATION/'qualification-result.json'),'training_template':pin(template),
        'training_configs':configs,'training_schedule':pin(root/'paired-training-schedule.json'),
        'initial_source':q['initial_source'],'a48':p['references']['a48'],'opponent_provenance':p['opponent_provenance'],
        'initial_adam_step':32400,'final_adam_step':32600,'evaluation_jobs':jobs,
        'gates':{'max_job_seconds':120,'workers':4,'max_evaluation_worker_seconds':3600,'max_training_arm_seconds':1800,
                 'expected_matches':3864,'additional_matches_per_arm':896,'canonical_matches_per_arm':392,
                 'minimum_additional_net_g1_wins':27,'additional_score_95_lower_gt':0.0,'canonical_score_95_lower_gt':-.05,
                 'additional_wins_at_least_g115':True,'bootstrap_replicates':10000,'bootstrap_seed':202609201901},
        'analysis':{'primary':'Broader minus control G1 wins on additional lists >=27/896 and paired G1 score interval lower >0.',
            'retention':'Canonical G1 score lower bound >-5pp versus both control and untouched g115; broader additional wins at least g115.',
            'draws':'Count G1 wins as integers; G1/BO3 score treats a draw as0.5. Report draw counts separately.',
            'pairing':'Within each ordered own/other matchup resample environment seeds, retaining both physical seats and all three arms.',
            'endpoint':'Only final iteration199/fullAdam32600; no intermediate selection.',
            'decision':'REPLICATE only if every gate passes, otherwise NO-ADVANCE. Neither verdict promotes a model.'},
        'limits':['Additional lists are development-exposed and now trained, not held out; only a conditional curriculum effect is measured.',
            'A48 has a distinct fresh initialization but is familiar training opposition. One g115 lineage, no independent replication.',
            'All training/evaluation preboard or Keep-sideboard, Keep7, no search. Learned openings, actual postboard competence, rules parity and human/meta evidence remain open.',
            'Retention is an aggregate seven-list gate, not simultaneous per-archetype confirmation.',
            'Fable known zero-read HTTP429 until Sep22 07:00 EDT; no repeated retry/endorsement. Jack authorized bounded local continuation.',
            'No CP7 outcome selection, paid compute or broad campaign.']}
    write(root/'manifest.json',manifest)
    print({'prepared_training_games_per_arm':2000,'substitutions':1000,'evaluation_matches':3864},flush=True)


def checked_manifest(root):
    m=read(root/'manifest.json');assert pin(__file__)==m['script']
    for p in m['helpers']:verify_pin(p)
    return m


def endpoint(root,m,arm):
    if arm=='g115':return m['initial_source']
    a=read(root/arm/'training-audit.json');assert a['complete'] and a['updates']==200
    assert a['source']['checkpoint']['path'].replace('\\','/').endswith('/iterations/000199/attempt-000000/update/checkpoint.json')
    c=read(verify_pin(a['source']['checkpoint']));assert c['adam_step']==32600 and c['state_sha256']==a['full_model_state_sha256']
    return a['source']


def audit_evaluation(root,m,arm,jobs):
    source=endpoint(root,m,arm);candidate=checkpoint_metadata(**source['checkpoint']);opponent=checkpoint_metadata(**m['a48']['checkpoint'])
    rows=[]
    for job in jobs:
        if job['arm']!=arm:continue
        c=read(verify_pin(job['config']));d=Path(c['output_directory']);n=len(job['cases'])
        assert read(d/'completion.json')['completed_matches']==n and len(list(d.glob('match-*.json')))==n
        assert read(d/'run-start.json')['binary']['sha256']==m['binary']['sha256']
        for i,(item,case) in enumerate(zip(c['matches'],job['cases'])):
            file=d/f'match-{i:06}.json';match=read(file);seat=case['seat']
            assert match['config']==item['config'] and match.get('search_usage') is None and match['seat_generations']==['v4','v4']
            for s,src,cp in [(seat,source,candidate),(1-seat,m['a48'],opponent)]:
                model=match['play_models'][s]
                assert model['checkpoint_sha256']==src['checkpoint']['sha256'] and model['state_sha256']==cp['state_sha256']
                assert model['source_import']==cp['source_import']
            for actual,expected in zip(match['explicit_registrations'],item['registered']):
                assert {k:actual[k] for k in ('label','mainboard','sideboard')}==expected
            hashes=[r['mainboard_sha256'] for r in match['explicit_registrations']]
            assert 2<=len(match['games'])<=6 and all(g['mainboard_sha256']==hashes for g in match['games'])
            assert all(r['selected_actions']==[{'kind':'done'}] for r in match['sideboard_decisions'])
            assert {(r['input']['next_game_number'],r['acting_player']) for r in match['sideboard_decisions']}=={(g['start']['game_index'],s) for g in match['games'][1:] for s in (0,1)}
            assert (match['games'][0]['start']['starting_player']==seat)==(case['replicate']%2==0)
            g1=match['games'][0]['winner'];winner=None if match['outcome']=='draw' else match['outcome']['winner']['winner']
            rows.append(dict(case,g1_win=int(g1==seat),g1_draw=g1 is None,g1_score=.5 if g1 is None else float(g1==seat),
                bo3_win=int(winner==seat),bo3_draw=winner is None,bo3_score=.5 if winner is None else float(winner==seat),
                games=len(match['games']),match=pin(file)))
    assert len(rows)==1288 and Counter(r['cohort'] for r in rows)=={'canonical':392,'additional':896}
    write(root/(arm+'-evaluation-audit.json'),{'complete':True,'candidate_source':source,'matches':len(rows),'rows':rows})
    print({'arm':arm,'complete_matches':len(rows),'natural_games':sum(r['games'] for r in rows),'outcomes_interpreted':False},flush=True)


def evaluate(root,arms):
    m=checked_manifest(root);verify_pin(m['binary']);jobs=[]
    for planned in m['evaluation_jobs']:
        if planned['arm'] not in arms:continue
        c=read(verify_pin(planned['template']));c['model_sources'][planned['seat']]=endpoint(root,m,planned['arm'])
        file=root/'configs'/(planned['name']+'.json');write(file,c);jobs.append(dict(planned,config=pin(file)))
    stage='reference' if arms==['g115'] else 'trained';write(root/(stage+'-dispatch.json'),{'jobs':jobs})
    start=time.monotonic()
    with ThreadPoolExecutor(max_workers=4) as pool:results=list(pool.map(lambda j:execute(root,j,m),jobs))
    write(root/(stage+'-execution.json'),{'results':results,'wall_seconds':time.monotonic()-start})
    assert all(r['exit_code']==0 and not r['timed_out'] for r in results),'Incomplete; preserve outputs and do not interpret'
    for arm in arms:audit_evaluation(root,m,arm,jobs)
    seconds=sum(r['elapsed_seconds'] for r in results)
    if stage=='reference':
        assert seconds*3<=3600,'Reference cost qualification failed'
        write(root/'reference-cost-qualification.json',{'passed':True,'worker_seconds':seconds,'projected_three_arm_worker_seconds':seconds*3})
    else:
        seconds+=sum(r['elapsed_seconds'] for r in read(root/'reference-execution.json')['results'])
        assert seconds<=3600,'Completed outside frozen cost cap'
    print({'stage':stage,'worker_seconds':seconds},flush=True)


def audit_training(root,m,arm):
    config=read(verify_pin(m['training_configs'][arm]));run=root/arm/'run';final=read(run/'completion.json')
    assert final['complete'] and final['completed_iterations']==200 and final['gpu_ordinal']==1 and final['loss_identity']=='gae_advantage_value/v1'
    source=config['initial_source'];state=read(verify_pin(source['checkpoint']))['state_sha256'];natural=decisions=additional=0
    for i,batch in enumerate(config['iterations']):
        receipt=read(run/'iterations'/f'{i:06}'/'complete.json');assert receipt['source']==source
        update=read(verify_pin(receipt['update']));collection=read(verify_pin(receipt['collection']))
        assert update['source']==collection['source']==source
        assert update['before_state_sha256']==collection['behavior_state_sha256']==state
        assert update['after_state_sha256']!=state and update['adam_step']==32401+i and update['checkpoint_readback']
        assert update['loss_identity']=='gae_advantage_value/v1' and update['gpu_ordinal']==1 and len(collection['trajectories'])==10
        for slot,p in enumerate(collection['trajectories']):
            t=read(verify_pin(p));expected=copy.deepcopy(batch['episodes'][slot]['episode']);assignment=batch['episodes'][slot]['opponent']
            if assignment['kind']=='current':opponent=source
            elif assignment['kind']=='initial':opponent=config['initial_source']
            else:
                assert assignment['kind']=='fixed';opponent=next(o['source'] for o in config['opponents'] if o['id']==assignment['id'])
            expected['opponent']=opponent
            assert t['episode']==expected and t['behavior_state_sha256']==state
            assert not expected['postboard'] and expected['selected']==expected['registered']
            terminal=t['terminal'];assert terminal['terminal_classification']=='natural' and terminal['terminal_reward'] in ([1,-1],[-1,1],[0,0])
            natural+=1;decisions+=terminal['physical_decision_count'];additional+=expected['registered'][expected['learner_seat']]['label'].startswith('published-')
        source=dict(config['initial_source'],checkpoint=update['checkpoint']);state=update['after_state_sha256']
    assert natural==2000 and additional==(1000 if arm=='broader' else 0) and final['source']==source
    checkpoint=read(verify_pin(source['checkpoint']));assert checkpoint['adam_step']==32600 and checkpoint['state_sha256']==state
    assert {k:checkpoint[k] for k in ('gamma_bits','gae_lambda_bits','entropy_coefficient_bits','learning_rate_bits','value_coefficient_bits')}=={
        'gamma_bits':1065353216,'gae_lambda_bits':1063675494,'entropy_coefficient_bits':0,'learning_rate_bits':953267991,'value_coefficient_bits':1056964608}
    result={'complete':True,'updates':200,'natural_games':natural,'additional_learner_games':additional,'physical_decisions':decisions,
            'source':source,'full_model_state_sha256':state,'final_adam_step':32600}
    write(root/arm/'training-audit.json',result);print({k:v for k,v in result.items() if k!='source'},flush=True)


def train(root):
    m=checked_manifest(root);assert read(root/'reference-cost-qualification.json')['passed'] and read(root/'g115-evaluation-audit.json')['complete']
    verify_pin(m['training_binary'])
    validations={}
    for arm,p in m['training_configs'].items():
        x=subprocess.run([m['training_binary']['path'],'--validate-config',str(verify_pin(p))],capture_output=True,text=True,check=True)
        validations[arm]=json.loads(x.stdout)
    write(root/'training-native-validation.json',validations)
    write(root/'training-environment.json',{'epoch':time.time(),'gpu':subprocess.check_output(['nvidia-smi','--query-gpu=index,uuid,name,driver_version,utilization.gpu,memory.used','--format=csv'],text=True),
        'qualified_environment':pin(QUALIFICATION/'environment.json'),'binary_rebuilt':False})
    for arm in ('control','broader'):
        command=[m['training_binary']['path'],str(verify_pin(m['training_configs'][arm]))];start=time.monotonic()
        with (root/arm/'stdout.jsonl').open('xb') as out,(root/arm/'stderr.log').open('xb') as err:
            child=subprocess.Popen(command,stdout=out,stderr=err,env=dict(os.environ,TEMP='E:/tmp',TMP='E:/tmp'),
                creationflags=subprocess.CREATE_NO_WINDOW|subprocess.BELOW_NORMAL_PRIORITY_CLASS)
            write(root/arm/'start.json',{'pid':child.pid,'started_epoch':time.time(),'command':command,'gpu_ordinal':1,'priority':'BelowNormal',
                                      'binary':m['training_binary'],'config':m['training_configs'][arm]})
            timeout=False
            try:code=child.wait(timeout=1800)
            except subprocess.TimeoutExpired:child.kill();code=child.wait();timeout=True
        write(root/arm/'execution.json',{'exit_code':code,'timed_out':timeout,'elapsed_seconds':time.monotonic()-start,
                                      'stdout':pin(root/arm/'stdout.jsonl'),'stderr':pin(root/arm/'stderr.log')})
        assert code==0 and not timeout,'Incomplete training; preserve outputs, no prefix interpretation'
        audit_training(root,m,arm)


def bootstrap(values,repeats,seed):
    # [ordered matchup, environment seed, physical seat, arm, metric].
    assert values.ndim==5 and values.shape[2:]==(2,3,2)
    rng=np.random.Generator(np.random.PCG64(seed));samples=np.zeros((repeats,3,2))
    for stratum in values:
        indices=rng.integers(0,len(stratum),size=(repeats,len(stratum)))
        samples+=stratum[indices].mean(axis=(1,2))/len(values)
    return samples


def analyze(root):
    m=checked_manifest(root);rows=[];seconds=0
    for stage in ('reference','trained'):
        results=read(root/(stage+'-execution.json'))['results'];assert all(r['exit_code']==0 and not r['timed_out'] for r in results)
        seconds+=sum(r['elapsed_seconds'] for r in results)
    assert seconds<=3600
    for arm in ARMS:
        audit=read(root/(arm+'-evaluation-audit.json'));assert audit['complete'] and audit['matches']==1288
        for r in audit['rows']:verify_pin(r['match'])
        rows+=audit['rows']
    keys={tuple(r[k] for k in ('cohort','own','other','replicate','seat','arm')):r for r in rows};assert len(keys)==len(rows)==3864
    cohorts={}
    for number,(cohort,nseeds,nstrata) in enumerate([('additional',8,56),('canonical',4,49)]):
        strata=sorted({(r['own'],r['other']) for r in rows if r['cohort']==cohort});assert len(strata)==nstrata
        values=np.empty((nstrata,nseeds,2,3,2))
        for i,(own,other) in enumerate(strata):
            for rep in range(nseeds):
                seeds=set()
                for seat in (0,1):
                    for j,arm in enumerate(ARMS):
                        row=keys[(cohort,own,other,rep,seat,arm)];seeds.add(row['seed'])
                        values[i,rep,seat,j]=[row['g1_score'],row['bo3_score']]
                assert len(seeds)==1
        samples=bootstrap(values,10000,m['gates']['bootstrap_seed']+number);means=values.mean(axis=(0,1,2));comparisons={}
        for i,j in ((2,1),(2,0),(1,0)):
            comparisons[f'{ARMS[i]}-minus-{ARMS[j]}']={'g1_score_difference':float(means[i,0]-means[j,0]),
                'g1_paired_95':np.quantile(samples[:,i,0]-samples[:,j,0],[.025,.975]).tolist(),
                'bo3_score_difference':float(means[i,1]-means[j,1]),
                'bo3_paired_95':np.quantile(samples[:,i,1]-samples[:,j,1],[.025,.975]).tolist()}
        summary={}
        for arm in ARMS:
            selected=[r for r in rows if r['cohort']==cohort and r['arm']==arm]
            summary[arm]={'matches':len(selected),**{k:sum(r[k] for r in selected) for k in ('g1_win','g1_draw','g1_score','bo3_win','bo3_draw','bo3_score')}}
        cohorts[cohort]={'summary':summary,'comparisons':comparisons}
    add,known=cohorts['additional'],cohorts['canonical'];net=add['summary']['broader']['g1_win']-add['summary']['control']['g1_win']
    gates={'additional_improvement':net>=27 and add['comparisons']['broader-minus-control']['g1_paired_95'][0]>0,
           'canonical_retention':all(known['comparisons'][f'broader-minus-{ref}']['g1_paired_95'][0]>-.05 for ref in ('control','g115')),
           'at_least_initial_additional_wins':add['summary']['broader']['g1_win']>=add['summary']['g115']['g1_win']}
    breakdown=[]
    for own in sorted({r['own'] for r in rows}):
        for arm in ARMS:
            selected=[r for r in rows if r['own']==own and r['arm']==arm]
            breakdown.append({'own':own,'arm':arm,'matches':len(selected),**{k:sum(r[k] for r in selected) for k in ('g1_win','g1_draw','bo3_win','bo3_draw')}})
    result={'complete':True,'matches':3864,'natural_games':sum(r['games'] for r in rows),'cohorts':cohorts,
            'additional_net_g1_wins':net,'gates':gates,'verdict':'REPLICATE' if all(gates.values()) else 'NO-ADVANCE',
            'per_own_registration':breakdown,'evaluation_worker_seconds':seconds,'manifest':pin(root/'manifest.json'),
            'promotion':False,'human_strength_claim':False,'limits':m['limits']}
    write(root/'analysis.json',result);print({k:v for k,v in result.items() if k in ('cohorts','additional_net_g1_wins','gates','verdict')},flush=True)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('mode',choices=['prepare','reference','train','evaluate','analyze']);parser.add_argument('--root',required=True,type=Path)
    args=parser.parse_args();root=args.root.resolve()
    if args.mode in ('reference','evaluate'):evaluate(root,['g115'] if args.mode=='reference' else ['control','broader'])
    else:globals()[args.mode](root)
