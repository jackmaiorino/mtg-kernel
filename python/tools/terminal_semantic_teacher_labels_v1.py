"""Export the audited, seat-consistent negative-control labels separately."""
import argparse, collections, hashlib, itertools, json, pathlib
P=pathlib.Path

def canonical(value,actor):
    if isinstance(value,dict):return {k:canonical(v,actor) for k,v in value.items()}
    if isinstance(value,list):return [canonical(v,actor) for v in value]
    if value in ('p0','p1'):return 'self' if value==f'p{actor}' else 'opponent'
    return value

def key(value):return json.dumps(value,sort_keys=True,separators=(',',':'))
def pin(path):return dict(path=str(path),sha256=hashlib.sha256(path.read_bytes()).hexdigest())

def target(actions,actor,winner):
    assert actor in [0,1] and 0<=winner<len(actions) and len(actions)>1
    keys=[key(canonical(a,actor)) for a in actions]
    assert len(set(keys))==len(keys),'ambiguous semantic actions'
    order=sorted(range(len(keys)),key=lambda i:keys[i])
    return order[(order.index(winner)+1)%len(order)],keys

def run(source,root):
    data=json.loads(source.read_bytes());assert data['schema']=='public-terminal-teacher-data/v1'
    assert len(data['records'])==32 and all(r['spec']['split']=='train' and not r['spec']['no_win_control'] for r in data['records'])
    rows=[];pairs=collections.defaultdict(list);permutations=0
    for row in data['records']:
        actions=row['data']['actions'];winners=row['data']['winning_indices'];actor=row['spec']['actor']
        assert len(actions)==4 and len(winners)==1
        winner=winners[0];witness=row['data']['outcomes'][winner]
        assert witness['classification']=='win' and witness['terminal']['terminal_classification']=='natural'
        assert witness['terminal']['winner']==f'p{actor}'
        control,keys=target(actions,actor,winner);assert control!=winner
        for order in itertools.permutations(range(4)):
            other,_=target([actions[i] for i in order],actor,order.index(winner))
            assert order[other]==control
            permutations+=1
        exported=dict(id=row['id'],winner=winner,control=control,semantic_action_keys=keys)
        rows.append(exported)
        pair_key=key({k:v for k,v in row['spec'].items() if k!='actor'})
        pairs[pair_key].append((actor,exported))
    assert len(pairs)==16
    for pair in pairs.values():
        assert len(pair)==2
        (aa,a),(bb,b)=sorted(pair);assert aa==0 and bb==1
        assert set(a['semantic_action_keys'])==set(b['semantic_action_keys'])
        assert a['semantic_action_keys'][a['winner']]==b['semantic_action_keys'][b['winner']]
        assert a['semantic_action_keys'][a['control']]==b['semantic_action_keys'][b['control']]
    result=dict(schema='terminal-semantic-teacher-labels/v1',training=pin(source),parent_source=data['source'],records=rows,
        control_contract='Cycle sorted actor-relative action semantics, replacing exact p0/p1 values with self/opponent; reject duplicate keys.',
        meaning='Control is a deliberately incorrect imitation label, never a certified win or gameplay reward.',
        seat_pairs=16,menu_permutations_verified=permutations,
        limitation='Verified for these fixtures. Arena-ID or hidden-state invariance of arbitrary action semantics is not established.')
    root.mkdir();(root/'labels.json').write_text(key(result),encoding='utf-8')
    (root/'completion.json').write_text(key(dict(complete=True,labels=pin(root/'labels.json'),runner=pin(P(__file__)),
        positions=32,seat_pairs=16,menu_permutations=permutations,updates=0,heldout_records_read=0)),encoding='utf-8')
    print(json.dumps(dict(complete=True,positions=32,seat_pairs=16,menu_permutations=permutations,labels=pin(root/'labels.json'))))

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--training',type=P,required=True);p.add_argument('--root',type=P,required=True)
    args=p.parse_args();run(args.training,args.root)
