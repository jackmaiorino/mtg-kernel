"""Public allowlist for an engine-verified retained-action reconstruction."""
import json
from viewer_export_v1 import digest_search, key, public_ids, references, terminal

SCHEMA = 'mtg-kernel-viewer-reconstruction/v1'
VALUE_UNIT = 1_000_000

def export_root(raw, selected, count):
    record, order = raw['record'], raw['live_order']
    root = record['tree']['root']
    if sorted(order) != list(range(count)) or len(root['n']) != count or len(root['wins']) != count:
        raise ValueError('Reconstructed root action mapping differs')
    if order[root['frozen_choice']] != selected or sum(root['n']) != record['completed_natural']:
        raise ValueError('Recorded root selection or simulation count differs')
    stats = [None] * count
    for position, live in enumerate(order):
        n, wins = root['n'][position], root['wins'][position]
        if not isinstance(n,int) or not isinstance(wins,int) or not 0 <= wins <= n:
            raise ValueError('Invalid recorded rollout outcome counts')
        value = wins * VALUE_UNIT
        stats[live] = {'flat_action_index':live,'visits':n,'value_sum':value,'mean_value':value // n if n else 0}
    result = {'selected_index':selected,'transitions_used':record['transitions'],
        'simulations':record['completed_natural'],'tree_node_count':record['tree']['nodes'],'root_action_stats':stats}
    return {'digest':digest_search(result),'simulations':result['simulations'],'transitions':result['transitions_used'],
        'nodes':result['tree_node_count'],'tier':f"Persistent UCT · {record['budget']:,} transitions",
        'stabilityHalves':False,'algorithm':'persistent-uct','digestOrigin':'exported-table',
        'valueScale':'Observed natural-terminal rollout wins per million outcomes (1,000,000 = all won), from the featured agent’s perspective. These sampled-world frequencies are not calibrated win probabilities.',
        'stats':[{'index':s['flat_action_index'],'visits':s['visits'],'valueSum':str(s['value_sum']),'meanValue':str(s['mean_value'])} for s in stats]}

def export_reconstruction(path, exporter):
    header, footer, frames = None, None, []
    previous_step = -1
    with path.open('rb') as stream:
        for line in stream:
            if len(line) > 8*1024*1024 or not line.endswith(b'\n'):
                raise ValueError('Truncated or oversized reconstruction')
            row = json.loads(line)
            if row.get('schema') != SCHEMA or footer is not None:
                raise ValueError('Invalid reconstruction schema/order')
            if row['kind'] == 'header' and header is None and not frames:
                header = row
                if header['perspective'] != exporter.perspective:
                    raise ValueError('Reconstruction perspective differs')
                exporter.names = header['cards']
            elif row['kind'] == 'footer' and header is not None:
                footer = row
            elif row['kind'] == 'decision' and header is not None:
                d = row['decision']
                if d['step'] != previous_step + 1 or len(frames) >= 100000:
                    raise ValueError('Reconstruction decision order differs')
                previous_step = d['step']
                if len(row['labels']) != len(row['actions']) or not 0 <= row['selected'] < len(row['actions']):
                    raise ValueError('Reconstructed action mapping differs')
                before, after = exporter.board(row['before']), exporter.board(row['after'])
                present = public_ids(before) | public_ids(after)
                featured = d['acting_player'] == exporter.perspective
                actions = []
                for i, semantic in enumerate(row['actions'] if featured else [row['actions'][row['selected']]]):
                    refs = [ref for ref in references(semantic) if ref in present]
                    label = row['labels'][i] if featured else semantic['action_kind'].replace('_',' ').capitalize()
                    if not featured:
                        names = [exporter.cards[o['card']]['name'] for p in after['players'] for zone in ('battlefield','graveyard') for o in p[zone] if o['id'] in refs]
                        if names: label += ': ' + ', '.join(dict.fromkeys(names))
                    actions.append({'index':i,'label':label,'kind':semantic['action_kind'],'refs':refs})
                selected = row['selected'] if featured else 0
                search = export_root(row['search'], selected, len(actions)) if featured and row['search'] else None
                frame = {'id':f"d{d['step']}",'key':key(d),'board':before,'after':after,'actions':actions,
                    'executed':selected,'policy':None,'search':search,'terminal':terminal(row['terminal'])}
                evidence = row.get('evidence')
                if featured and evidence and evidence['chooser'] in ('new_root','reused'):
                    if evidence['witness_sha256'] != header['witness_sha256']:
                        raise ValueError('Decision evidence belongs to a different source')
                    frame['decisionSource'] = {'kind':'search-root' if search else 'search-continuation',
                        'root':f"d{evidence['root_step']}",'treeDigest':evidence['tree_hash'],
                        'artifactSha256':evidence['witness_sha256']}
                frames.append(frame)
            else:
                raise ValueError('Invalid reconstruction order')
    if not header or not footer or not footer.get('complete') or footer['written'] != len(frames) or not footer.get('anchor_verified') or not frames[-1]['terminal']:
        raise ValueError('Reconstruction is incomplete or unverified')
    return header, frames, True
