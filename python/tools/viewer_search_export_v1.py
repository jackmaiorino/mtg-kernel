"""Allowlisted conversion of an actual V4 search trace into a public replay."""
from collections import defaultdict

def export_replay(raw, exporter, decision_key, root_actions, search):
    if raw.get('schema') != 'mtg-kernel-search-capture/v1' or not raw.get('complete'):
        raise ValueError('Search replay capture incomplete or unsupported')
    from viewer_export_v1 import public_ids, references
    contexts = defaultdict(list)
    leaves = {}
    for row in raw['contexts']:
        if 'snapshot' in row:
            index = row['snapshot']
            if not isinstance(index,int) or not 0 <= index < len(raw.get('snapshots',[])):
                raise ValueError('Broken search snapshot reference')
            row = dict(raw['snapshots'][index], **row)
        simulation = row['simulation']
        if row.get('leaf'):
            if simulation in leaves: raise ValueError('Repeated search leaf')
            leaves[simulation] = row
        else: contexts[simulation].append(row)
    nodes = []
    for i, node in enumerate(raw['nodes']):
        count = len(node['priors'])
        if not count or any(len(node[k]) != count for k in ('edge_visits','edge_value_sums','children')):
            raise ValueError('Search node action mapping mismatch')
        nodes.append({'id':i,'actor':f"p{node['actor']}",'visits':node['visits'],'priors':node['priors'],
          'actions':[{'index':j,'label':f'Move {j+1}','kind':'unobserved','refs':[]} for j in range(count)],
          'children':node['children'],'edgeVisits':node['edge_visits'],
          'edgeValueSums':[str(n) for n in node['edge_value_sums']]})
    if not nodes or len(nodes[0]['actions']) != len(root_actions): raise ValueError('Replay root mapping mismatch')
    nodes[0]['actions'] = root_actions
    simulations = []
    visits = [[0]*len(n['actions']) for n in nodes]
    sums = [[0]*len(n['actions']) for n in nodes]
    # V4 allocates exactly one sequential node at each recorded expansion.
    # Bind that allocation to its captured parent/action, rather than guessing
    # from a board image (sampled worlds can have identical public boards).
    next_node = 1
    seen = {0}
    graph_edges = set()
    node_visits = [0]*len(nodes)
    for ordinal, trace in enumerate(raw['trace']):
        if trace['ordinal'] != ordinal or ordinal not in leaves or len(contexts[ordinal]) != len(trace['choices']):
            raise ValueError('Search trace/context mapping mismatch')
        choices = []
        for position, (choice, context) in enumerate(zip(trace['choices'],contexts[ordinal])):
            n, a = choice['node'], choice['action']
            if context['node'] != n or context['action'] != a or not 0 <= n < len(nodes) or not 0 <= a < len(nodes[n]['actions']):
                raise ValueError('Search choice reference mismatch')
            if len(choice['scores']) != len(nodes[n]['actions']): raise ValueError('Search scores differ from legal actions')
            if context['actor'] != nodes[n]['actor']: raise ValueError('Search context actor differs from its node')
            if n not in seen: raise ValueError('Search visits a node before its recorded expansion')
            if position:
                previous = trace['choices'][position-1]
                if n not in nodes[previous['node']]['children'][previous['action']]:
                    raise ValueError('Search continuation differs from captured graph')
                graph_edges.add((previous['node'],previous['action'],n))
            node_visits[n] += 1
            # Sampled hands are never presentation data, including the featured seat's.
            observation = dict(context['board'], own_hand=[])
            board = exporter.board(observation)
            if n != 0:
                visible = public_ids(board)
                names = {o['id']:exporter.cards[o['card']]['name'] for p in board['players'] for zone in ('battlefield','graveyard') for o in p[zone]}
                if len(context['actions']) != len(nodes[n]['actions']): raise ValueError('Search menu shape mismatch')
                actions = []
                for j, semantic in enumerate(context['actions']):
                    refs = [r for r in references(semantic) if r in visible]
                    kind = semantic['action_kind']
                    label = kind.replace('_',' ').capitalize()
                    public_names = list(dict.fromkeys(names[r] for r in refs if r in names))
                    if public_names: label += ' involving ' + ', '.join(public_names)
                    actions.append({'index':j,'label':label,'kind':kind,'refs':refs})
                # Labels refer to the first observed context, while each path keeps its actual board.
                if nodes[n]['actions'][0]['kind'] == 'unobserved': nodes[n]['actions'] = actions
            choices.append({'node':n,'action':a,'mode':choice['mode'],
              'scores':[str(v) if v is not None else None for v in choice['scores']], 'board':board})
            visits[n][a] += 1
            sums[n][a] += trace['value']
        leaf = leaves[ordinal]
        leaf_node = None
        if leaf['reason'] == 'expanded':
            previous = trace['choices'][-1]
            if next_node >= len(nodes) or next_node not in nodes[previous['node']]['children'][previous['action']]:
                raise ValueError('Search expansion differs from captured parent/action identity')
            leaf_node = next_node
            graph_edges.add((previous['node'],previous['action'],leaf_node))
            seen.add(leaf_node)
            node_visits[leaf_node] += 1
            next_node += 1
        simulations.append({'ordinal':ordinal,'value':str(trace['value']),'reason':leaf['reason'],
          'choices':choices,'leaf':exporter.board(dict(leaf['board'],own_hand=[])), 'leafNode':leaf_node})
    if len(simulations) != search['simulations'] or set(contexts) != set(leaves) or len(leaves) != len(simulations):
        raise ValueError('Search simulation coverage mismatch')
    for i, node in enumerate(nodes):
        if visits[i] != node['edgeVisits'] or sums[i] != list(map(int,node['edgeValueSums'])):
            raise ValueError('Replay differs from final node statistics')
        if node_visits[i] != node['visits']:
            raise ValueError('Replay differs from recorded node visits')
    captured_edges = {(n['id'],a,child) for n in nodes for a,children in enumerate(n['children']) for child in children}
    if next_node != len(nodes) or graph_edges != captured_edges:
        raise ValueError('Search expansion coverage differs from captured graph')
    for i, stat in enumerate(search['stats']):
        if stat['index'] != i or visits[0][i] != stat['visits'] or str(sums[0][i]) != stat['valueSum']:
            raise ValueError('Replay differs from captured root digest table')
    return {'schema':'mtg-kernel-search-replay/v2','complete':True,'key':decision_key,
      'nodes':nodes,'simulations':simulations,'worlds':'Sampled hypothetical worlds; private hands excluded.'}
