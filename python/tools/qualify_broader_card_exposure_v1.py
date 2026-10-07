"""Four-update matched qualification of additional learner-card exposure."""
import argparse
from collections import Counter
import copy
from pathlib import Path
import shutil
import subprocess
import time

from prepare_postboard_qualification_v1 import BASE, REPO, BINARY, BINARY_SHA, read, write, pin
from postboard_bo3_qualification_v1 import seed
from run_postboard_qualification_v1 import execute, audit_arm, compare_replay, checked_pin
from phase1_breadth_v1.catalog_v1 import to_native

PRIOR=Path('E:/mtg-postboard-campaign-20260920/qualification-001')
VARIANTS=Path('E:/mtg-postboard-campaign-20260920/published-variant-qualification-001')
EXPOSURE=Path('E:/mtg-postboard-campaign-20260920/embedding-exposure-audit-001/audit.json')


def prepare(root):
    prior=read(PRIOR/'manifest.json'); assert read(PRIOR/'qualification-result.json')['verdict']=='ENGINEERING-PASS'
    template=PRIOR/'preboard/config.json'; control=read(template)
    assert pin(BINARY)['sha256']==BINARY_SHA
    source=read(checked_pin(control['initial_source']['checkpoint']))
    assert source['adam_step']==32400 and source['loss_identity']=='gae_advantage_value/v1'
    v=read(VARIANTS/'manifest.json'); selected=read(checked_pin(v['selection']))
    registry_path=checked_pin(v['registry']); cards={c['name']:(i,c) for i,c in enumerate(read(registry_path)['cards'])}
    decks=[to_native(r['zones'],'published-'+r['zoned_sha256'][:16],cards) for r in selected]
    assert len(decks)==8 and len(control['iterations'])==4
    excluded=set(read(BASE/'catalog/g-settings.json')['excluded_seeds'])
    for file in [template,BASE/'campaign-002-g-block115-cuda.json',Path('E:/mtg-postboard-campaign-20260920/pilot-001/preboard/config.json')]:
        excluded.update(x['episode']['seed'] for batch in read(file)['iterations'] for x in batch['episodes'])
    for i,batch in enumerate(control['iterations']):
        for j,item in enumerate(batch['episodes']):
            episode=item['episode']
            assert not episode['postboard'] and episode['selected']==episode['registered']
            episode['seed']=seed('g115-broader-card-exposure-qualification-20260920-v1',i,j)
            episode['id']=f'broader-card-exposure-20260920-i{i}-s{j}'
            assert episode['seed'] not in excluded
            excluded.add(episode['seed'])
    broad=copy.deepcopy(control)
    # Twenty replacement slots, balanced ten per physical learner seat. Each
    # list appears in both seats; four lists appear three times, four twice.
    required_p1=[3,1,4,2]; counters=[0,4]; schedule=[]
    for i,batch in enumerate(broad['iterations']):
        positions={s:[j for j,x in enumerate(batch['episodes']) if x['episode']['learner_seat']==s] for s in (0,1)}
        chosen=set(positions[1][:required_p1[i]]+positions[0][:5-required_p1[i]])
        assert len(chosen)==5
        for j,item in enumerate(batch['episodes']):
            before=control['iterations'][i]['episodes'][j]; e=item['episode']; seat=e['learner_seat']
            if j in chosen:
                deck=decks[counters[seat]%8]; counters[seat]+=1
                e['registered'][seat]=copy.deepcopy(deck);e['selected'][seat]=copy.deepcopy(deck)
            restored=copy.deepcopy(item)
            for z in ('registered','selected'):restored['episode'][z][seat]=before['episode'][z][seat]
            assert restored==before
            schedule.append({'iteration':i,'slot':j,'seed':e['seed'],'learner_seat':seat,
                'starting_player':e['starting_player'],'opponent_policy':item['opponent'],
                'additional':j in chosen,'before_registration':before['episode']['registered'][seat],
                'after_registration':e['registered'][seat]})
    counts=Counter(r['after_registration']['label'] for r in schedule if r['additional'])
    assert sorted(counts.values())==[2]*4+[3]*4
    assert Counter(r['learner_seat'] for r in schedule if r['additional'])=={0:10,1:10}
    assert len({(r['after_registration']['label'],r['learner_seat']) for r in schedule if r['additional']})==16
    root.mkdir()
    shutil.copyfile(BINARY,root/BINARY.name)
    configs={}
    for arm,config in [('preboard',control),('mixed',broad),('mixed-replay',copy.deepcopy(broad))]:
        (root/arm).mkdir();config['output_directory']=(root/arm/'run').as_posix()
        path=root/arm/'config.json';write(path,config);configs[arm]=pin(path)
    write(root/'paired-schedule.json',schedule)
    manifest={'schema':'g115-broader-card-exposure-qualification/v1','script':pin(__file__),
        'helpers':[pin(REPO/'python/tools/run_postboard_qualification_v1.py'),pin(REPO/'python/tools/prepare_postboard_qualification_v1.py')],
        'binary':pin(root/BINARY.name),'producer_launch':prior['producer_launch'],
        'producer_source_commit':prior['source_commit'],'template':pin(template),'prior_qualification':pin(PRIOR/'qualification-result.json'),
        'initial_source':control['initial_source'],'initial_state_sha256':source['state_sha256'],'initial_adam_step':32400,
        'configs':configs,'variant_selection':v['selection'],'registry':pin(registry_path),
        'embedding_audit':pin(EXPOSURE),'schedule':pin(root/'paired-schedule.json'),
        'arm_semantics':{'preboard':'Existing-seven-list control','mixed':'Half additional learner lists; ALL games preboard','mixed-replay':'Same broader-exposure branch with a native process boundary after update one'},
        'gates':{'max_arm_wall_seconds':120,'natural_games_per_arm':40,'additional_games_per_broad_arm':20,
                 'full_optimizer_replay_updates':4,'minimum_newly_updated_additional_card_rows':1},
        'limits':['Engineering only; no win-rate interpretation or promotion.',
            'Additional training registrations are explicitly development-exposed; not pristine heldouts.',
            'A48 is a familiar training opponent. GAE/reward/optimizer and both full initial Adam states remain fixed.',
            'Preboard only. No supplied canonical sideboard plans are applied to additional lists.',
            'Fable zero-read HTTP429 until Sep22 07:00 EDT; no retry or endorsement. The maintainer authorized bounded local work.',
            'No CP7 outcome selection, paid compute or broad campaign.']}
    write(root/'manifest.json',manifest)
    print({'prepared':True,'games_per_arm':40,'additional_per_broad_arm':20,'additional_list_counts':dict(counts)},flush=True)


def audit_embeddings(root,manifest,arms):
    prior=read(checked_pin(manifest['embedding_audit']))
    original=read(checked_pin(manifest['initial_source']['checkpoint']))
    tensor=lambda checkpoint,field:next(t['values'] for t in checkpoint[field] if t['name']=='card_embedding.weight')
    original_weights=tensor(original,'parameters')
    rows={r['card_id']:r for r in prior['models']['g115']['rows'] if r['no_retained_embedding_update']}
    assert prior['models']['g115']['source']['checkpoint']==manifest['initial_source']['checkpoint']
    substituted={card for r in read(checked_pin(manifest['schedule'])) if r['additional'] for card in r['after_registration']['mainboard']}
    reports={}
    for arm in ('preboard','mixed','mixed-replay'):
        endpoint=read(checked_pin(arms[arm]['iterations'][-1]['checkpoint']))
        fields={f:tensor(endpoint,f) for f in ('parameters','first_moments','second_moments')}
        updated=[]
        for card,row in rows.items():
            start=(card+1)*16; sl=slice(start,start+16)
            changed=fields['parameters'][sl]!=original_weights[sl]
            has_moment=any(x!=0 for f in ('first_moments','second_moments') for x in fields[f][sl])
            if changed or has_moment:
                updated.append({'card_id':card,'name':row['name'],'weights_changed':changed,
                                'nonzero_moment':has_moment,'in_substituted_mainboards':card in substituted})
        reports[arm]=updated
    learned=[r for r in reports['mixed'] if r['weights_changed'] and r['nonzero_moment'] and r['in_substituted_mainboards']]
    assert learned,'No previously unchanged additional mainboard embedding received an actual retained update'
    assert reports['mixed']==reports['mixed-replay']
    # The control should not acquire updates on entirely new card identities.
    assert not reports['preboard'],reports['preboard']
    return {'newly_updated_additional_mainboard_rows':len(learned),'per_arm_previously_unchanged_rows':reports,
        'control_rows_unchanged':True,'does_not_establish_strength':True}


def run(root):
    manifest=read(root/'manifest.json');assert pin(__file__)==manifest['script']
    for helper in manifest['helpers']:checked_pin(helper)
    validations={}
    for arm,p in manifest['configs'].items():
        result=subprocess.run([str(checked_pin(manifest['binary'])),'--validate-config',str(checked_pin(p))],capture_output=True,text=True,check=True)
        validations[arm]=__import__('json').loads(result.stdout)
    write(root/'native-validation.json',validations)
    write(root/'environment.json',{'epoch':time.time(),'gpu':subprocess.check_output(['nvidia-smi','--query-gpu=index,uuid,name,driver_version,utilization.gpu,memory.used','--format=csv'],text=True),
        'rustc':subprocess.check_output(['rustc','-Vv'],text=True),'cargo':subprocess.check_output(['cargo','-V'],text=True),
        'producer_environment_reference':pin(PRIOR/'environment.json'),'binary_rebuilt':False})
    results={}
    for arm in ('preboard','mixed'):results[arm]=execute(root,manifest,arm,'complete')
    results['replay-prefix']=execute(root,manifest,'mixed-replay','prefix',1)
    results['replay-resume']=execute(root,manifest,'mixed-replay','resume')
    assert results['replay-prefix']['elapsed_seconds']+results['replay-resume']['elapsed_seconds']<=120
    arms={arm:audit_arm(root,manifest,arm) for arm in ('preboard','mixed','mixed-replay')}
    comparison=compare_replay(root,arms)
    embeddings=audit_embeddings(root,manifest,arms)
    result={'schema':'g115-broader-card-exposure-qualification-result/v1','verdict':'ENGINEERING-PASS',
        'arms':arms,'executions':results,'replay':comparison,'embedding_updates':embeddings,
        'natural_games':120,'training_games_with_additional_learner_lists':40,'strength_claim':False,'paid_compute':False}
    write(root/'qualification-result.json',result)
    print({'verdict':result['verdict'],'natural_games':120,'replay':comparison,
           'newly_updated_additional_mainboard_rows':embeddings['newly_updated_additional_mainboard_rows']},flush=True)


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('mode',choices=['prepare','run']);p.add_argument('--root',required=True,type=Path)
    args=p.parse_args();globals()[args.mode](args.root.resolve())
