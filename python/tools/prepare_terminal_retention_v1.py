"""Outcome-blind development retention extraction; no model or engine execution."""
import argparse, collections, concurrent.futures, hashlib, json, os, pathlib, time

P = pathlib.Path
BASE = P('E:/mtg-meta-recovery-20260921')
ARCHIVES = P('D:/mtg-training-working/stack-screen-training-native-0/computehost/recovered/jobs/structured/outputs')
AUDIT = BASE/'public-terminal-teacher-natural-audit-002/completion.json'
SCHEDULE = ARCHIVES.parent/'request.json'

def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()

def sha(data):
    return hashlib.sha256(data).hexdigest()

def read(path):
    return json.loads(P(path).read_bytes())

def pin(path):
    path = P(path)
    return dict(path=str(path), sha256=sha(path.read_bytes()))

def verified_bytes(ref):
    data = P(ref['path']).read_bytes()
    assert sha(data) == ref['sha256'], ref['path']
    return data

def checked(ref):
    return json.loads(verified_bytes(ref))

def write(path, value):
    with P(path).open('xb') as stream:
        stream.write(encoded(value))

def kind(kinds):
    k = set(kinds)
    for label, matches in [('blocker',{25}),('attacker',{24}),('discard',{23}),
        ('target',{6,7,12,17,21}),('choice',{8,9,10,11,13,14,15,16,18,19,20,22,26}),
        ('cast',{2}),('ability',{4,5}),('land',{1}),('mana',{3})]:
        if k & matches:
            return label
    assert k <= {0}
    return 'pass'

def prepare(root):
    root.mkdir()
    completion = read(AUDIT); assert completion['complete']
    schedule = read(SCHEDULE)['config']['updates']
    dev = {e['id']: e for update in range(19,200,20) for e in schedule[update]}
    validation = []
    for update in range(9,200,20):
        for episode, spec in enumerate(schedule[update]):
            validation.append(dict(episode_id=spec['id'],seed=spec['seed'],learner_seat=spec['learner_seat'],
                postboard=spec['postboard'],decks=[d['label'] for d in spec['selected']],
                path=str(ARCHIVES/f'{update:04}'/f'episode-{episode:03}.json')))
    assert len(dev)==len(validation)==100
    assert not set(dev)&{v['episode_id'] for v in validation}
    assert not {v['seed'] for v in dev.values()}&{v['seed'] for v in validation}
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        validation_pins=list(pool.map(pin,[v['path'] for v in validation]))
    for entry,ref in zip(validation,validation_pins):
        entry['trajectory']=ref
        del entry['path']
    pools=collections.defaultdict(list); audit_pins=[]; archive_pins={}
    for job in completion['report']['jobs']:
        result=checked(job['result']); audit_pins.append(job['result'])
        for archive in result['archives']:
            episode=archive['episode_id']; assert episode in dev
            archive_pins[episode]=archive['trajectory']
            rows=archive['rows']; index=0
            while index<len(rows):
                first=rows[index]; n=first['substep_count']; group=rows[index:index+n]
                assert n>0 and len(group)==n and first['substep_index']==0
                for offset,row in enumerate(group):
                    assert row['archive_row']==index+offset and row['substep_index']==offset
                    assert row['substep_count']==n and row['physical_decision_id']==first['physical_decision_id']
                    assert row['actor']==first['actor'] and row['deck']==first['deck']
                kinds=sorted({k for row in group for k in row['action_kinds']})
                if any(row['action_count']>1 for row in group):
                    identity=f"{episode}:{first['actor']}:{first['physical_decision_id']}"
                    # Deliberately omit all candidate logits, outcomes, chosen
                    # actions and policy-change measurements from selection.
                    selected=dict(id=identity,episode_id=episode,deck=first['deck'],actor=first['actor'],
                        postboard=archive['postboard'],decision_kind=kind(kinds),action_kinds=kinds,
                        physical_decision_id=first['physical_decision_id'],archive_rows=[r['archive_row'] for r in group],
                        parent_logits_bits=[r['parent_logits_bits'] for r in group],parent_value_bits=[r['parent_value_bits'] for r in group],
                        rank=sha(('terminal-retention-selection-v1:'+identity).encode()))
                    pools[(selected['deck'],selected['actor'])].append(selected)
                index+=n
    assert len(archive_pins)==100 and len(pools)==16
    chosen=[]; availability={}
    for cell,candidates in sorted(pools.items()):
        strata=collections.defaultdict(list)
        for g in candidates:strata[(g['decision_kind'],g['postboard'])].append(g)
        for values in strata.values():values.sort(key=lambda g:g['rank'])
        keys=sorted(strata,key=lambda k:sha(encoded(['terminal-retention-stratum-v1',cell,k])))
        selected=[]; games=collections.Counter()
        while len(selected)<16:
            for key in keys:
                if len(selected)==16:break
                remaining=strata[key]
                if not remaining:continue
                group=min(remaining,key=lambda g:(games[g['episode_id']],g['rank']))
                remaining.remove(group);selected.append(group);games[group['episode_id']]+=1
            assert any(strata.values()) or len(selected)==16
        chosen.extend(selected)
        availability[str(cell)]=dict(available=len(candidates),available_strata=[list(k) for k in keys],selected=16)
    jobs=[]
    for episode in sorted({g['episode_id'] for g in chosen}):
        jobs.append(dict(id=episode,trajectory=archive_pins[episode],groups=[g for g in chosen if g['episode_id']==episode]))
    validation_manifest=dict(schema='terminal-retention-validation-reservation/v1',episodes=validation,
        schedule=pin(SCHEDULE),excluded_development_episode_ids=sorted(dev),
        role='Reserved game-level policy-retention validation for a future experiment. Not a tactical or strength holdout.',
        limitations='Archived structured-policy games, predating trample correction. Not globally unseen training ancestry.',
        contents_parsed=False,bytes_hashed=True,outcomes_used=False)
    write(root/'validation-reservation.json',validation_manifest)
    write(root/'manifest.json',dict(schema='terminal-retention-development-selection/v1',runner=pin(__file__),
        source_audit=pin(AUDIT),audit_reports=audit_pins,teacher=pin(BASE/'public-terminal-teacher-data-001/train.json'),
        validation=pin(root/'validation-reservation.json'),jobs=jobs,groups=len(chosen),availability=availability,
        selection='16 per deck/physical seat, round robin kind/postboard strata, prefer unused games then stable identity hash.',
        duplicate_rule='Drop exact duplicate tensor groups and exact teacher-overlap groups after extraction; report shortfall, do not silently refill.',
        no_training=True))
    print(json.dumps(dict(groups=len(chosen),archives=len(jobs),validation_games=len(validation))))

def extract(job):
    start=time.monotonic(); archive=checked(job['trajectory'])
    assert archive['episode']['id']==job['id']
    result=[]
    for group in job['groups']:
        copied=[]
        for offset,index in enumerate(group['archive_rows']):
            row=archive['decisions'][index]
            assert row['actor']==group['actor'] and row['physical_decision_id']==group['physical_decision_id']
            assert row['substep_index']==offset and row['substep_count']==len(group['archive_rows'])
            assert len(row['logits'])==len(group['parent_logits_bits'][offset])
            copied.append(dict(tensor=row['tensor'],parent_logits_bits=group['parent_logits_bits'][offset],
                parent_value_bits=group['parent_value_bits'][offset]))
        result.append(dict(selection=group,rows=copied,tensor_sha256=sha(encoded([r['tensor'] for r in copied]))))
    return dict(id=job['id'],trajectory=job['trajectory'],groups=result)

def execute(jobs,workers):
    start=time.monotonic()
    if workers==1:results=[extract(j) for j in jobs]
    else:
        with concurrent.futures.ProcessPoolExecutor(max_workers=workers) as pool:results=list(pool.map(extract,jobs))
    return results,time.monotonic()-start

def qualify(root):
    m=read(root/'manifest.json');verified_bytes(m['runner'])
    # Four archive sizes spanning this exact selected workload. Correctness
    # comparison includes JSON decoding and returning extracted tensor payloads.
    ranked=sorted(m['jobs'],key=lambda j:os.path.getsize(j['trajectory']['path']))
    sample=[ranked[i] for i in [0,len(ranked)//3,2*len(ranked)//3,len(ranked)-1]]
    serial,one=execute(sample,1);parallel,four=execute(sample,4)
    assert encoded(serial)==encoded(parallel)
    write(root/'extraction-timing.json',dict(complete=True,manifest=pin(root/'manifest.json'),sample=[j['id'] for j in sample],
        workers1_seconds=one,workers4_seconds=four,selected_workers=4 if four<one else 1,exact_results=True,
        scope='Local file extraction only, not training/simulation/evaluation allocation qualification.',
        placement='Inputs already local; no model execution, GPU or paid allocation. Separate compute qualification required for training.'))
    print(json.dumps(dict(workers1_seconds=one,workers4_seconds=four)))

def run(root):
    q=read(root/'extraction-timing.json');assert q['complete'] and q['exact_results']
    m=checked(q['manifest']);verified_bytes(m['runner'])
    for ref in m['audit_reports']+[m['teacher'],m['validation']]:checked(ref)
    results,seconds=execute(m['jobs'],q['selected_workers'])
    teacher=checked(m['teacher']);forbidden={sha(encoded(r['data']['tensor'])) for r in teacher['records']}
    candidates=sorted((g for r in results for g in r['groups']),key=lambda g:g['selection']['rank'])
    groups=[];seen=set();excluded=[]
    for group in candidates:
        overlap=any(sha(encoded(r['tensor'])) in forbidden for r in group['rows'])
        duplicate=group['tensor_sha256'] in seen
        if overlap or duplicate:excluded.append(dict(id=group['selection']['id'],reason='teacher_overlap' if overlap else 'duplicate_tensor_group'));continue
        seen.add(group['tensor_sha256']);groups.append(group)
    groups.sort(key=lambda g:g['selection']['id'])
    coverage=collections.Counter((g['selection']['deck'],g['selection']['actor']) for g in groups)
    kinds=collections.Counter(g['selection']['decision_kind'] for g in groups)
    dataset=dict(schema='terminal-parent-retention-data/v1',parent_source=teacher['source'],manifest=pin(root/'manifest.json'),
        groups=groups,trajectories=[r['trajectory'] for r in results],no_rewards_or_selected_actions=True)
    write(root/'train.json',dataset)
    write(root/'completion.json',dict(complete=True,dataset=pin(root/'train.json'),groups=len(groups),rows=sum(len(g['rows']) for g in groups),
        selected_groups=m['groups'],excluded=excluded,coverage=[dict(deck=k[0],actor=k[1],groups=v) for k,v in sorted(coverage.items())],
        decision_kinds=dict(sorted(kinds.items())),preboard=sum(not g['selection']['postboard'] for g in groups),
        postboard=sum(g['selection']['postboard'] for g in groups),archives=len(results),seconds=seconds,workers=q['selected_workers'],
        selected_episode_ids=sorted({g['selection']['episode_id'] for g in groups}),new_model_scores=0,updates=0,
        validation_contents_read=False,tensor_parity='Exact copied V4 input bits; frozen g115 logits from hash-verified completed audit. Native loader must replay them.',
        no_strength_claim=True))
    print(json.dumps(read(root/'completion.json'),indent=2))

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('mode',choices=['prepare','qualify','run']);parser.add_argument('--root',type=P,required=True)
    args=parser.parse_args();globals()[args.mode](args.root)
