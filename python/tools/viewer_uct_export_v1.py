"""Validate and allowlist full persistent UCT search attempts for presentation."""
import hashlib
import json
import struct
from viewer_export_v1 import key, public_ids, references, terminal
from viewer_reconstruction_export_v1 import export_root

SCHEMA = 'mtg-kernel-uct-viewer-capture/v1'

def public_action(raw):
    return {k: raw[k] for k in ('index', 'label', 'kind', 'refs')}

def replay(raw, exporter, frame):
    if not raw.get('complete') or not raw['nodes'] or not raw['simulations']:
        raise ValueError('Full UCT capture is incomplete')
    record = raw['record']
    if key(record['decision']) != frame['key']:
        raise ValueError('UCT trace belongs to a different physical decision')
    boards, board_ids, board_map = [], {}, {}
    for i, observed in enumerate(raw['boards']):
        board = exporter.board(dict(observed, own_hand=[]))
        for player in board['players']: player['hand'] = []
        encoded = json.dumps(board, separators=(',', ':'), sort_keys=True)
        if encoded not in board_ids:
            board_ids[encoded] = len(boards); boards.append(board)
        board_map[i] = board_ids[encoded]
    nodes, counts, wins = [], [], []
    for i, n in enumerate(raw['nodes']):
        if n['id'] != i or n['board'] not in board_map or sorted(n['live_order']) != list(range(len(n['actions']))):
            raise ValueError('UCT node mapping differs')
        actions = [public_action(a) for a in n['actions']]
        if i == 0:
            if len(actions) != len(frame['actions']): raise ValueError('UCT root menu differs')
            actions = [dict(frame['actions'][live], index=j) for j, live in enumerate(n['live_order'])]
        if any(a['index'] != j for j,a in enumerate(actions)):
            raise ValueError('UCT canonical action identifiers differ')
        nodes.append({'id':i,'actor':'p0','board':board_map[n['board']], 'parent':n['parent'],
            'actions':actions,'liveOrder':n['live_order'],'committed':n['committed'],
            'visits':n['visits'],'edgeVisits':n['n'],'edgeWins':n['wins']})
        counts.append([0]*len(actions)); wins.append([0]*len(actions))
    actions, action_ids, simulations, first_combo = [], {}, [], None
    for i, s in enumerate(raw['simulations']):
        if s['ordinal'] != i or s['backed_up'] != (s['reason'] in ('win','natural_nonwin')):
            raise ValueError('UCT attempt identity or backup status differs')
        choices = []
        for j,c in enumerate(s['choices']):
            node, a = c['node'], c['action']
            if not 0 <= node < len(nodes) or not 0 <= a < len(nodes[node]['actions']) or c['board'] not in board_map:
                raise ValueError('UCT path reference differs')
            if j and nodes[node]['parent'] != [s['choices'][j-1]['node'], s['choices'][j-1]['action']]:
                raise ValueError('UCT parent/action path differs')
            if len(c['scores']) != len(nodes[node]['actions']) or any(v is None for v in c['scores']):
                raise ValueError('UCT selection scores were not captured')
            if c['scores'][a] != max(c['scores']): raise ValueError('UCT selected score differs')
            choices.append({'node':node,'action':a,'board':board_map[c['board']], 'step':c['step'],'physical':c['physical'],'substep':c['substep'],'live':c['live'],'expanded':c['expanded'],'scores':c['scores']})
            if s['backed_up']:
                counts[node][a] += 1; wins[node][a] += int(s['reason']=='win')
        moves = []; stage = 0
        for move in s['rollout']:
            a = public_action(move['action'])
            before, after = move.get('before'), move.get('after')
            if any(x is not None and x not in board_map for x in (before,after)):
                raise ValueError('UCT rollout snapshot reference differs')
            visible=set().union(*(public_ids(boards[board_map[x]]) for x in (before,after) if x is not None))
            a['refs']=[ref for ref in a['refs'] if ref in visible]
            encoded=json.dumps(a,sort_keys=True)
            if encoded not in action_ids: action_ids[encoded]=len(actions); actions.append(a)
            moves.append([0 if move['actor']=='p0' else 1,move['step'],move['physical'],move['substep'],
                action_ids[encoded],move['flags'],move['resolution'],board_map.get(before),board_map.get(after)])
            if move['actor']=='p0' and move['flags']&2: stage=max(stage,1)
            if stage==1 and move['resolution']&2: stage=2
            if stage==2 and move['actor']=='p0' and move['flags']&4: stage=3
            if stage==3 and move['resolution']&4: stage=4
        for j,m in enumerate(moves):
            if j and m[1]!=moves[j-1][1]+1:raise ValueError('UCT rollout step ordering differs')
        by_key={(m[1],m[2],m[3]):m for m in moves}
        for j,c in enumerate(choices):
            m=by_key.get((c['step'],c['physical'],c['substep']))
            if m is None:
                if s['reason']=='truncated' and j==len(choices)-1 and (not moves or c['step']==moves[-1][1]+1):continue
                raise ValueError('UCT choice has no applied rollout move')
            if m[0]!=0 or actions[m[4]]['index']!=c['live']:raise ValueError('UCT tree/live rollout action differs')
        captured_terminal=s.get('terminal')
        outcome = terminal(captured_terminal) if s['backed_up'] else ({'kind':'Non-natural termination','winner':None} if captured_terminal else None)
        if s['backed_up'] and (not outcome or outcome['kind']!='Natural terminal' or (s['reason']=='win')!=(outcome['winner']=='p0')):
            raise ValueError('UCT terminal outcome differs from backup')
        leaf = s.get('leaf')
        if s['backed_up'] and leaf not in board_map: raise ValueError('UCT natural outcome snapshot missing')
        if first_combo is None and stage==4 and s['reason']=='win': first_combo=i
        simulations.append({'ordinal':i,'reason':s['reason'],'backedUp':s['backed_up'],
            'choices':choices,'rollout':moves,'leaf':board_map.get(leaf),'terminal':outcome,'combo':stage==4})
    digest = hashlib.sha256()
    for n in raw['nodes']:
        if n['n']!=counts[n['id']] or n['wins']!=wins[n['id']] or n['visits']!=sum(n['n']) or (not n['committed'] and n['visits']):
            raise ValueError('UCT paths do not reconcile native visits/wins')
    for n in sorted((n for n in raw['nodes'] if n['committed']),key=lambda n:n['key']):
        if n['n']!=counts[n['id']] or n['wins']!=wins[n['id']] or n['visits']!=sum(n['n']):
            raise ValueError('UCT paths do not reconcile native visits/wins')
        digest.update(bytes.fromhex(n['key'])); digest.update(struct.pack('<Q',n['visits']))
        for visits, won in zip(n['n'],n['wins']): digest.update(struct.pack('<QQ',visits,won))
    if digest.hexdigest()!=record['tree']['hash']:
        raise ValueError('UCT native tree digest differs')
    if sum(len(s['rollout']) for s in simulations)!=record['transitions'] or len(simulations)!=record['attempted'] or sum(s['backedUp'] for s in simulations)!=record['completed_natural']:
        raise ValueError('UCT raw transition/attempt accounting differs')
    table=bytearray()
    for n in nodes:
        table.extend(struct.pack('<IIII',n['id'],int(n['committed']),n['visits'],len(n['actions'])))
        for visits,won in zip(n['edgeVisits'],n['edgeWins']):table.extend(struct.pack('<II',visits,won))
    return {'schema':'mtg-kernel-uct-search-replay/v1','complete':True,'key':frame['key'],
        'treeDigest':record['tree']['hash'],'tableDigest':hashlib.sha256(table).hexdigest(),'boards':boards,'nodes':nodes,'actions':actions,
        'simulations':simulations,'firstCombo':first_combo,
        'worlds':'Approximate sampled information sets with a uniform public deck prior. Both sampled hands are hidden; only focal non-forced choices become UCT tree nodes.'}

def export_capture(path, exporter):
    header, footer, rows, searches = None, None, [], {}
    with path.open('rb') as stream:
        while line:=stream.readline(8*1024*1024+1):
            if len(line)>8*1024*1024 or not line.endswith(b'\n'): raise ValueError('Truncated or oversized UCT capture')
            row=json.loads(line)
            if row.get('schema')!=SCHEMA or footer is not None: raise ValueError('Invalid UCT capture schema/order')
            kind=row['kind']
            if kind=='header' and header is None:
                header=row;exporter.names=row['cards']
                if row['perspective']!=exporter.perspective: raise ValueError('UCT perspective differs')
            elif kind=='footer': footer=row
            elif kind=='decision': rows.append(row)
            elif kind=='search':
                if row['index'] in searches: raise ValueError('Duplicate UCT search')
                searches[row['index']]={'record':row['record'],'boards':[],'nodes':[],'simulations':[],'complete':False}
            elif kind in ('search_snapshot','search_node','search_simulation','search_end'):
                r=searches.get(row['search'])
                if r is None or r['complete']: raise ValueError('Invalid UCT search row order')
                if kind=='search_snapshot':
                    if row['id']!=len(r['boards']):raise ValueError('UCT snapshot identity differs')
                    r['boards'].append(row['board'])
                elif kind=='search_node':r['nodes'].append(row['node'])
                elif kind=='search_simulation':r['simulations'].append(row['simulation'])
                else:r['complete']=row['complete']
            else: raise ValueError('Unknown UCT capture row')
    if not header or not footer or not footer['complete'] or footer['written']!=len(rows) or not rows or not rows[-1]['terminal']:
        raise ValueError('UCT gameplay capture is incomplete')
    frames=[]
    for i,row in enumerate(rows):
        d=row['decision']
        if d['step']!=i or len(row['actions'])!=len(row['labels']) or not 0<=row['selected']<len(row['actions']):raise ValueError('UCT gameplay action/key mapping differs')
        before,after=exporter.board(row['before']),exporter.board(row['after']); present=public_ids(before)|public_ids(after)
        featured=d['acting_player']==exporter.perspective;actions=[]
        for j,sem in enumerate(row['actions'] if featured else [row['actions'][row['selected']]]):
            actions.append({'index':j,'label':row['labels'][j] if featured else sem['action_kind'].replace('_',' ').capitalize(),
                'kind':sem['action_kind'],'refs':[r for r in references(sem) if r in present]})
        frames.append({'id':f'd{i}','key':key(d),'board':before,'after':after,'actions':actions,
            'executed':row['selected'] if featured else 0,'policy':None,'search':None,'terminal':terminal(row['terminal'])})
    by_step={f['key']['step']:f for f in frames}; source_hash=hashlib.sha256(path.read_bytes()).hexdigest()
    for index,raw in searches.items():
        if not raw['nodes']:continue
        record=raw['record'];f=by_step.get(record['step'])
        if not f or key(record['decision'])!=f['key']:raise ValueError('UCT root gameplay key differs')
        f['search']=export_root({'record':record,'live_order':raw['nodes'][0]['live_order']},f['executed'],len(f['actions']))
        f['decisionSource']={'kind':'search-root','root':f['id'],'treeDigest':record['tree']['hash'],'artifactSha256':source_hash}
        f['search']['uctReplay']=replay(raw,exporter,f)
    for row,f in zip(rows,frames):
        e=row.get('evidence');raw=searches.get(e.get('search')) if e else None
        if e and e.get('chooser')=='reused' and raw and raw['nodes']:
            root=by_step[raw['record']['step']]
            f['decisionSource']={'kind':'search-continuation','root':root['id'],'treeDigest':raw['record']['tree']['hash'],'artifactSha256':source_hash}
    return header,frames,True
