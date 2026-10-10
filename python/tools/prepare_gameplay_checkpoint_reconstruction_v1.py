"""Prepare the fixed 22-case no-model reconstruction; never execute a game."""
import argparse
import hashlib
import json
from pathlib import Path

LIMITS = {'human': 292, 'case0-g115-seat0': 201, 'case0-g115-seat1': 114,
          'case1-g115-seat0': 8, 'case1-g115-seat1': 11,
          'case2-g115-seat1': 23, 'natural-burn': 65}
CASE_IDS = {'H107', 'H262', 'H271', 'H273', 'H292', 'G004', 'G019', 'G114',
            'P036', 'P024', 'P026', 'P115', 'P117', 'N113', 'C110', 'C255',
            'C265', 'C004', 'C204', 'C011', 'C014', 'C026'}


def require(ok, message):
    if not ok:
        raise ValueError(message)


def pin(path):
    path = Path(path).resolve()
    return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


def read(path):
    path = Path(path)
    require(path.stat().st_size <= 64 * 1024 ** 2, 'source exceeds64MiB')
    return json.loads(path.read_bytes())


def lines(path):
    path = Path(path)
    require(path.stat().st_size <= 64 * 1024 ** 2, 'journal exceeds64MiB')
    return [json.loads(line) for line in path.read_bytes().splitlines()]


def target(case, step, source):
    return {'case_id': case['id'], 'step': step, 'actor': case['actor'],
            'origin': {'source_pin': source, 'pointer': case['pointer'],
                       'game_index': case['native_game_ordinal'],
                       'original_key': case['decision_key']}}


def bo3_prefix(name, source, cases):
    doc = read(source['path'])
    collection = doc['collection'] if name == 'natural-burn' else doc
    cfg, collected = collection['config'], collection['collected']
    game_index = 2 if name == 'natural-burn' else 1
    game = collected['trajectory']['games'][game_index - 1]
    require(game['game_index'] == game_index, 'game ordinal differs')
    require(all(p['opening'] == {'kind': 'existing', 'protocol': 'keep_seven_v2'}
                for p in collection['packages']), 'unsupported original opening')
    require(all(p['sideboard'] == {'kind': 'keep'} for p in collection['packages']),
            'source is not fixed keep-sideboard')
    registrations = cfg['registrations']
    require(game['start']['choice'] == 'play', 'unexpected original play/draw choice')
    # The game2 witness is direct setup, not a fabricated new BO3 history.
    if game_index == 2:
        sideboards = [r for r in game['decisions'] if r['visible']['kind'] == 'sideboard']
        require(len(sideboards) == 2, 'missing both retained keep-sideboard records')
        for row in sideboards:
            require(row['visible']['ordered_actions'][row['behavior']['selected_index']] == {'kind': 'done'},
                    'game2 sideboard changed the registration')
    gameplay = [r for r in game['decisions'] if r['visible']['kind'] == 'gameplay']
    limit = LIMITS[name]
    require(len(gameplay) > limit, 'incomplete source prefix')
    decisions = []
    for step, row in enumerate(gameplay[:limit + 1]):
        visible = row['visible']
        require(visible['observation']['step_index'] == step, 'noncontiguous gameplay steps')
        selected = row['behavior']['selected_index']
        require(0 <= selected < len(visible['ordered_actions']), 'bad historical selection')
        decisions.append({'step': step, 'actor': row['actor'],
                          'source_pointer': ('/collection' if name == 'natural-burn' else '') + f'/collected/trajectory/games/{game_index-1}/decisions/{game["decisions"].index(row)}',
                          'observation': visible['observation'],
                          'ordered_actions': visible['ordered_actions'], 'selected_index': selected})
    by_record = {r['decision_index']: r for r in gameplay}
    targets = [target(c, by_record[c['decision_key']]['visible']['observation']['step_index'], source)
               for c in cases]
    opening_rows = [r for r in game['decisions'] if r['visible']['kind'] == 'mulligan']
    require(len(opening_rows) == 2, 'opening records missing')
    require(all(r['visible']['input']['mulligans_taken'] == 0 and
                r['behavior']['selected_index'] == 0 for r in opening_rows), 'original did not keep seven')
    require(all(r['visible']['input']['own_configuration'] == registrations[int(r['actor'][1])]
                for r in opening_rows), 'opening registration differs')
    return {'id': name, 'source_pin': source, 'human_responses': None, 'episode_id': game_index,
            'environment_seed': collected['games'][game_index - 1]['environment_seed'],
            'deck_ids': cfg['deck_ids'], 'mainboards': [r['mainboard'] for r in registrations],
            'starting_player': game['start']['starting_player'], 'human_seat': 0,
            'opening': [{'command': 'keep', 'expected_view': None}],
            'opening_inputs': [{'actor': r['actor'], 'input': r['visible']['input']} for r in opening_rows],
            'action_ceiling': limit, 'decisions': decisions, 'targets': targets}


def human_prefix(source, response_pin, cases):
    journal, responses = lines(source['path']), lines(response_pin['path'])
    cfg = journal[0]['payload']['config']
    start = next(r['payload'] for r in journal if r['event'] == 'game_start')
    require(start['start']['game_index'] == 1 and start['start']['choice'] == 'play',
            'unexpected human game start')
    prompts, openings = {}, {}
    for response in responses:
        view = response['view']
        if not response['ok'] or response['error'] is not None:
            require(view['phase'] == 'stopped' and 'decision' not in view and 'opening' not in view,
                    'failed response contains a required gameplay/opening view')
            # Retained final ui-138 is a stopped session response, never an action prompt.
            continue
        if 'decision' in view:
            decision = view['decision']; seq = decision['prompt_seq']
            require(seq not in prompts or prompts[seq] == decision, 'conflicting historical prompt')
            prompts[seq] = decision
        if 'opening' in view:
            rev = view['opening_revision']
            require(rev not in openings or openings[rev] == view['opening'], 'conflicting opening')
            openings[rev] = view['opening']
    opening, decisions = [], []
    for line, row in enumerate(journal, 1):
        payload, event = row['payload'], row['event']
        if event == 'human_command' and payload['command'] in ('mulligan', 'keep', 'bottom'):
            opening.append({'command': payload['command'], 'hand_indices': payload.get('hand_indices', []),
                            'expected_view': openings[payload['opening_revision']]})
        if event == 'human_command' and payload['command'] == 'action':
            decision = prompts[payload['prompt_seq']]
            require(decision['human_seat'] == 'p'+str(cfg['human_seat']), 'human seat mismatch')
            decisions.append({'step': len(decisions), 'actor': decision['human_seat'],
                              'source_pointer': f'line {line} /payload',
                              'human_decision': decision, 'human_action_index': payload['action_index']})
        elif event == 'offline_model_decision':
            require(payload['step'] == len(decisions), 'model/human timeline is not contiguous')
            decisions.append({'step': payload['step'], 'actor': payload['actor'],
                              'source_pointer': f'line {line} /payload',
                              'observation': payload['observation'], 'ordered_actions': payload['actions'],
                              'selected_index': payload['selected_engine_index']})
    require([r['command'] for r in opening] == ['mulligan']*3+['keep', 'bottom'],
            'historical opening differs')
    require(len(decisions) > LIMITS['human'], 'human prefix incomplete')
    return {'id': 'human', 'source_pin': source, 'human_responses': response_pin, 'episode_id': 1, 'environment_seed': start['environment_seed'],
            'deck_ids': [r['label'] for r in cfg['registered']],
            'mainboards': [r['mainboard'] for r in start['configurations']],
            'starting_player': start['start']['starting_player'], 'human_seat': cfg['human_seat'],
            'opening': opening, 'opening_inputs': [], 'action_ceiling': LIMITS['human'],
            'decisions': decisions[:LIMITS['human']+1],
            'targets': [target(c, c['decision_key'], source) for c in cases]}


def prepare(manifest, responses, output):
    manifest, output = Path(manifest), Path(output)
    require(not output.exists(), 'preserve existing request')
    corpus = read(manifest); cases = corpus['cases']
    require(len(cases) == 22 and {c['id'] for c in cases} == CASE_IDS, 'fixed case membership differs')
    require({c['dataset'] for c in cases} == set(LIMITS), 'fixed source clusters differ')
    source_pins = [pin(manifest), pin(responses)]
    prefixes = []
    for name in LIMITS:
        group = [c for c in cases if c['dataset'] == name]
        require(len({c['path'] for c in group}) == 1, 'cluster has mixed source files')
        source = pin(group[0]['path']); source_pins.append(source)
        require(corpus['sources'][group[0]['path']]['sha256'] == source['sha256'],
                'source differs from frozen corpus pin')
        prefixes.append(human_prefix(source, source_pins[1], group) if name == 'human'
                        else bo3_prefix(name, source, group))
    request = {'schema': 'gameplay-checkpoint-reconstruction/v1', 'mode': 'primary',
               'preparer': pin(__file__), 'source_pins': source_pins,
               'prefixes': prefixes, 'max_opening_responses': 32}
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open('x', encoding='utf-8') as stream:
        json.dump(request, stream, separators=(',', ':'), sort_keys=True); stream.write('\n')
    return pin(output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--responses', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(prepare(args.manifest, args.responses, args.output)))
