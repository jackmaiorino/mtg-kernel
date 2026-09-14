"""Pure, deterministic compiler for one ordinary ten-game breadth block.

The catalog loader owns train/reserved, field coverage, runtime and registry
admission. This module checks its compact input contract, never loads a model,
and does not create independent lineages, evaluate, or launch native work.
"""
from collections import Counter
import copy
from fractions import Fraction
import hashlib
import json
import math
from pathlib import PurePosixPath, PureWindowsPath
import re
import struct

SCHEMA = 'phase1-breadth-block/v1'
RNG_SCHEMA = 'phase1-breadth-sha256-rng/v1'
MAX_JSON_BYTES = 16 * 1024 * 1024
MAX_PHYSICAL_DECISIONS = 100_000
MAX_POLICY_STEPS = 200_000
U64_MAX = 2**64 - 1
SETTINGS_FIELDS = {
    'schema', 'lineage_id', 'block_index', 'seed', 'phase', 'initial_source',
    'opponents', 'policy_pool', 'opponent_archetype_weights', 'learning_rate',
    'value_coefficient', 'collection_workers', 'preparation_workers',
    'output_directory', 'excluded_seeds',
}


def _require(ok, message):
    if not ok:
        raise ValueError(message)


def _json(value):
    try:
        return json.dumps(value, sort_keys=True, separators=(',', ':'),
                          ensure_ascii=True, allow_nan=False).encode('ascii')
    except (TypeError, ValueError, OverflowError, RecursionError) as error:
        raise ValueError('Inputs must contain bounded finite JSON data') from error


def _object(value, fields, label):
    _require(type(value) is dict and set(value) == set(fields), label + ' fields differ')


def _integer(value, low, high, label):
    _require(type(value) is int and low <= value <= high, label + ' integer exceeds bounds')
    return value


def _text(value, label, limit=128):
    _require(type(value) is str and 0 < len(value.encode('utf-8')) <= limit
             and all(ord(char) >= 32 and ord(char) != 127 for char in value), label + ' is invalid')
    return value


def _absolute(value):
    _text(value, 'absolute path', 4096)
    _require(PureWindowsPath(value).is_absolute() or PurePosixPath(value).is_absolute(),
             'absolute path required')


def _sha(value):
    _require(type(value) is str and re.fullmatch('[0-9a-f]{64}', value) is not None,
             'lowercase SHA-256 required')


def _source(value):
    _require(type(value) is dict and set(value) in (
        {'play_import', 'feature_transfer'}, {'play_import', 'checkpoint', 'feature_transfer'}),
        'model source fields differ')
    for pin in [value['play_import']] + ([] if value.get('checkpoint') is None else [value['checkpoint']]):
        _object(pin, {'path', 'sha256'}, 'model file pin')
        _absolute(pin['path'])
        _sha(pin['sha256'])
    transfer = value['feature_transfer']
    _object(transfer, {'expected_feature_contract_digest', 'expected_feature_encoding_digest'}, 'feature transfer')
    for digest in transfer.values():
        _sha(digest)


def _deck(value):
    _object(value, {'label', 'mainboard', 'sideboard'}, 'deck')
    _text(value['label'], 'deck label')
    for zone, count in [('mainboard', 60), ('sideboard', 15)]:
        _require(type(value[zone]) is list and len(value[zone]) == count, zone + ' size differs')
        for card in value[zone]:
            _integer(card, 0, 65535, 'card id')
    result = {'label': value['label'], 'mainboard': sorted(value['mainboard']),
              'sideboard': sorted(value['sideboard'])}
    zoned = (tuple(result['mainboard']), tuple(result['sideboard']))
    return result, zoned, tuple(sorted(result['mainboard'] + result['sideboard']))


def _float_bits(value, label):
    _require(type(value) in (int, float), label + ' must be numeric')
    try:
        _require(math.isfinite(value) and value > 0, label + ' must be positive finite')
        raw = struct.pack('>f', value)
    except (OverflowError, struct.error) as error:
        raise ValueError(label + ' is outside binary32') from error
    rounded = struct.unpack('>f', raw)[0]
    _require(math.isfinite(rounded) and rounded > 0, label + ' is outside positive binary32')
    return struct.unpack('>I', raw)[0]


class _Rng:
    """SHA256(key || big-endian u64 counter), with rejection sampling.

    Each domain owns a distinct key. Modulo is used only below the largest
    multiple of n representable in 256 bits, avoiding reduction bias.
    """
    def __init__(self, seed, lineage, block, domain):
        self.key = hashlib.sha256(_json([RNG_SCHEMA, seed, lineage, block, domain])).digest()
        self.counter = 0

    def below(self, n):
        _integer(n, 1, 2**256, 'RNG range')
        cutoff = 2**256 - (2**256 % n)
        while True:
            _require(self.counter <= U64_MAX, 'RNG counter exhausted')
            value = int.from_bytes(hashlib.sha256(self.key + self.counter.to_bytes(8, 'big')).digest(), 'big')
            self.counter += 1
            if value < cutoff:
                return value % n

    def shuffle(self, rows):
        for end in range(len(rows) - 1, 0, -1):
            chosen = self.below(end + 1)
            rows[end], rows[chosen] = rows[chosen], rows[end]


def _hamilton(total, weighted):
    denominator = sum(weight for _, weight in weighted)
    base = {name: total * weight // denominator for name, weight in weighted}
    remaining = total - sum(base.values())
    order = sorted(weighted, key=lambda row: (-(total * row[1] % denominator), row[0]))
    for name, _ in order[:remaining]:
        base[name] += 1
    return base


def _choose(rng, weighted):
    selected = rng.below(sum(weight for _, weight in weighted))
    for item, weight in weighted:
        if selected < weight:
            return item
        selected -= weight
    raise AssertionError('weighted choice exhausted its exact integer range')


def _fraction(numerator, denominator):
    value = Fraction(numerator, denominator)
    return {'numerator': value.numerator, 'denominator': value.denominator}


def compile_block_v1(catalog, settings):
    """Return {'native': existing runner DTO, 'report': exact schedule facts}.

    Input lists are order-insensitive where keyed by IDs. Output episode order
    is fixed. The caller must preserve excluded seeds across blocks/lineages.
    This checks syntax and conservation, not native card legality or admission.
    """
    catalog_bytes, settings_bytes = _json(catalog), _json(settings)
    _require(len(catalog_bytes) <= MAX_JSON_BYTES and len(settings_bytes) <= MAX_JSON_BYTES,
             'compiler input exceeds 16 MiB')
    _object(catalog, {'archetypes', 'scope', 'provenance'}, 'catalog')
    _require(catalog['scope'] in ('engineering', 'campaign') and type(catalog['provenance']) is dict,
             'catalog scope/provenance differs')
    _object(settings, SETTINGS_FIELDS, 'settings')
    _require(settings['schema'] == SCHEMA and settings['phase'] in ('preboard', 'mixed'),
             'unknown block schema or phase')
    lineage = _text(settings['lineage_id'], 'lineage id')
    block = _integer(settings['block_index'], 0, U64_MAX, 'block index')
    seed = _integer(settings['seed'], 0, U64_MAX, 'seed')
    _absolute(settings['output_directory'])
    _source(settings['initial_source'])
    scalar_bits = {name: _float_bits(settings[name], name) for name in ('learning_rate', 'value_coefficient')}
    _integer(settings['collection_workers'], 1, 1024, 'collection workers')
    _integer(settings['preparation_workers'], 1, 32, 'preparation workers')
    excluded = settings['excluded_seeds']
    _require(type(excluded) is list and len(excluded) <= 1_000_000, 'excluded seed list exceeds bounds')
    for item in excluded:
        _integer(item, 0, U64_MAX, 'excluded seed')
    _require(len(set(excluded)) == len(excluded), 'duplicate excluded seed')
    used_seeds = set(excluded)

    raw_archetypes = catalog['archetypes']
    _require(type(raw_archetypes) is list and 1 <= len(raw_archetypes) <= 204,
             'archetype count exceeds native block bounds')
    archetypes, variant_ids, zoned_seen = {}, set(), set()
    variant_count = 0
    for row in raw_archetypes:
        _object(row, {'id', 'field_count', 'train'}, 'archetype')
        name = _text(row['id'], 'archetype id')
        _require(name not in archetypes, 'duplicate archetype id')
        _integer(row['field_count'], 1, U64_MAX, 'field count')
        _require(type(row['train']) is list and 1 <= len(row['train']) <= 4096, 'train variant count exceeds bounds')
        variants = {}
        for variant in row['train']:
            variant_count += 1
            _require(variant_count <= 4096, 'total train variants exceed 4096')
            _object(variant, {'id', 'multiplicity', 'registered', 'postboard'}, 'training variant')
            identity = _text(variant['id'], 'training variant id')
            _require(identity not in variant_ids, 'duplicate training variant id')
            variant_ids.add(identity)
            _integer(variant['multiplicity'], 1, U64_MAX, 'training multiplicity')
            registered, zoned, combined = _deck(variant['registered'])
            _require(zoned not in zoned_seen, 'duplicate exact zoned training configuration')
            zoned_seen.add(zoned)
            _require(type(variant['postboard']) is list and len(variant['postboard']) <= 128,
                     'postboard variant count exceeds bounds')
            postboards, postboard_seen = [], set()
            for selected in variant['postboard']:
                selected, selected_zones, selected_combined = _deck(selected)
                _require(selected_combined == combined, 'postboard changed registered 75')
                _require(selected_zones not in postboard_seen, 'duplicate exact postboard configuration')
                postboard_seen.add(selected_zones)
                postboards.append(selected)
            _require(settings['phase'] != 'mixed' or postboards,
                     'mixed block requires explicit postboard configurations for every train variant')
            postboards.sort(key=lambda deck: (deck['mainboard'], deck['sideboard']))
            variants[identity] = {'registered': registered, 'postboard': postboards, 'multiplicity': variant['multiplicity']}
        archetypes[name] = {'field_count': row['field_count'], 'variants': variants}

    opponents = settings['opponents']
    _require(type(opponents) is list and len(opponents) <= 128, 'opponent roster exceeds bounds')
    opponent_ids = set()
    for row in opponents:
        _object(row, {'id', 'source'}, 'opponent')
        _text(row['id'], 'opponent id')
        _require(row['id'] not in opponent_ids, 'duplicate opponent id')
        opponent_ids.add(row['id'])
        _source(row['source'])
    pool = settings['policy_pool']
    _require(type(pool) is list and 1 <= len(pool) <= 128, 'policy pool exceeds bounds')
    policies = {}
    for row in pool:
        _object(row, {'id', 'weight', 'assignment'}, 'policy pool row')
        name = _text(row['id'], 'policy id')
        _require(name not in policies, 'duplicate policy id')
        _integer(row['weight'], 1, U64_MAX, 'policy weight')
        assignment = row['assignment']
        _require(type(assignment) is dict, 'policy assignment must be an object')
        kind = assignment.get('kind')
        if kind in ('current', 'initial'):
            _object(assignment, {'kind'}, 'policy assignment')
        elif kind == 'fixed':
            _object(assignment, {'kind', 'id'}, 'fixed assignment')
            _text(assignment['id'], 'fixed opponent id')
            _require(assignment['id'] in opponent_ids, 'fixed opponent is absent from roster')
        elif kind == 'recent_completed':
            _object(assignment, {'kind', 'lag', 'fallback'}, 'recent assignment')
            _integer(assignment['lag'], 1, 4096, 'historical lag')
            _require(assignment['fallback'] == 'initial', 'recent fallback must be initial')
        else:
            raise ValueError('unsupported policy assignment')
        policies[name] = row
    weights = settings['opponent_archetype_weights']
    _require(type(weights) is list and len(weights) == len(archetypes), 'opponent weights must cover catalog')
    opponent_weights = {}
    for row in weights:
        _object(row, {'id', 'weight'}, 'opponent archetype weight')
        _text(row['id'], 'opponent archetype id')
        _require(row['id'] in archetypes and row['id'] not in opponent_weights, 'unknown or duplicate opponent archetype')
        opponent_weights[row['id']] = _integer(row['weight'], 1, U64_MAX, 'opponent archetype weight')

    count = len(archetypes)
    updates, games = max(200, 20 * count), 10 * max(200, 20 * count)
    weighted_field = sorted((name, row['field_count']) for name, row in archetypes.items())
    weighted_uniform = [(name, 1) for name in sorted(archetypes)]
    weighted_opponents = sorted(opponent_weights.items())
    weighted_policies = sorted((name, row['weight']) for name, row in policies.items())
    weighted_variants = {name: sorted((key, row['multiplicity']) for key, row in data['variants'].items())
                         for name, data in archetypes.items()}
    rng = {domain: _Rng(seed, lineage, block, domain) for domain in (
        'field-order', 'uniform-order', 'branch-slots', 'roles', 'phases',
        'learner-variant', 'opponent-archetype', 'opponent-variant', 'policy',
        'learner-postboard', 'opponent-postboard', 'physical-seeds')}
    allocations, branches = {}, {}
    for branch, weighted in [('field', weighted_field), ('uniform', weighted_uniform)]:
        allocations[branch] = _hamilton(games // 2, weighted)
        branches[branch] = [name for name, number in allocations[branch].items() for _ in range(number)]
        rng[branch + '-order'].shuffle(branches[branch])
    roles = [(seat, relative) for seat in (0, 1) for relative in ('play', 'draw') for _ in range(games // 4)]
    rng['roles'].shuffle(roles)
    counts = {name: Counter() for name in ('learner', 'opponent', 'learner_variants', 'opponent_variants',
              'policies', 'assignments', 'phase', 'roles', 'joint', 'matchups', 'configuration_changes')}
    iterations, episode_seeds, assignment_rows = [], [], []
    rejected_seed_candidates = 0
    for index in range(updates):
        slots = [('field', name) for name in branches['field'][index * 5:index * 5 + 5]]
        slots += [('uniform', name) for name in branches['uniform'][index * 5:index * 5 + 5]]
        rng['branch-slots'].shuffle(slots)
        phases = ['preboard'] * (5 if settings['phase'] == 'mixed' else 10)
        if settings['phase'] == 'mixed':
            phases += ['postboard'] * 5
        rng['phases'].shuffle(phases)
        episodes = []
        for slot, ((branch, archetype), phase) in enumerate(zip(slots, phases)):
            seat, relative = roles[index * 10 + slot]
            opposite = _choose(rng['opponent-archetype'], weighted_opponents)
            variant = _choose(rng['learner-variant'], weighted_variants[archetype])
            other_variant = _choose(rng['opponent-variant'], weighted_variants[opposite])
            own, other = archetypes[archetype]['variants'][variant], archetypes[opposite]['variants'][other_variant]
            selected = []
            for actor, row in [('learner', own), ('opponent', other)]:
                selected.append(row['registered'] if phase == 'preboard' else
                    row['postboard'][rng[actor + '-postboard'].below(len(row['postboard']))])
            registered = [own['registered'], other['registered']]
            changes = [any(chosen[zone] != original[zone] for zone in ('mainboard', 'sideboard'))
                       for chosen, original in zip(selected, registered)]
            for actor, changed in zip(('learner', 'opponent'), changes):
                counts['configuration_changes'][(phase, actor, changed)] += 1
            if seat == 1:
                registered.reverse()
                selected.reverse()
            policy = _choose(rng['policy'], weighted_policies)
            assignment = copy.deepcopy(policies[policy]['assignment'])
            if assignment['kind'] == 'recent_completed':
                assignment = ({'kind': 'completed_iteration', 'index': index - assignment['lag']}
                              if index >= assignment['lag'] else {'kind': 'initial'})
            physical_seed = rng['physical-seeds'].below(2**64)
            while physical_seed in used_seeds:
                rejected_seed_candidates += 1
                physical_seed = rng['physical-seeds'].below(2**64)
            used_seeds.add(physical_seed)
            episode_seeds.append(physical_seed)
            identity = hashlib.sha256(_json([SCHEMA, lineage, block, index, slot])).hexdigest()[:24]
            episode = {'id': f'breadth-{identity}-b{block}-i{index}-s{slot}', 'seed': physical_seed, 'starting_player': seat if relative == 'play' else 1-seat,
                'learner_seat': seat, 'registered': copy.deepcopy(registered), 'selected': copy.deepcopy(selected),
                'postboard': phase == 'postboard', 'max_physical_decisions': MAX_PHYSICAL_DECISIONS,
                'max_policy_steps': MAX_POLICY_STEPS}
            episodes.append({'episode': episode, 'opponent': assignment})
            assignment_rows.append({'episode_id': episode['id'], 'iteration': index, 'slot': slot,
                'branch': branch, 'phase': phase, 'learner_seat': seat, 'relative_start': relative,
                'learner_archetype': archetype, 'opponent_archetype': opposite,
                'learner_variant': variant, 'opponent_variant': other_variant, 'policy_id': policy,
                'learner_configuration_changed': changes[0], 'opponent_configuration_changed': changes[1]})
            for key, item in [('learner', archetype), ('opponent', opposite), ('learner_variants', variant),
                ('opponent_variants', other_variant), ('policies', policy), ('assignments', _json(assignment).decode()),
                ('phase', phase), ('roles', (seat, relative)), ('joint', (branch, phase, seat, relative)),
                ('matchups', (archetype, opposite))]:
                counts[key][item] += 1
        iterations.append({'episodes': episodes})
    native = {'schema': 'mtg-kernel-native-expanded-training-run/v1',
        'initial_source': copy.deepcopy(settings['initial_source']),
        'opponents': copy.deepcopy(sorted(opponents, key=lambda row: row['id'])), 'iterations': iterations,
        'learning_rate': settings['learning_rate'], 'value_coefficient': settings['value_coefficient'],
        'update_backend': {'kind': 'cpu'}, 'collection_workers': settings['collection_workers'],
        'preparation_workers': settings['preparation_workers'], 'output_directory': settings['output_directory']}
    native_bytes = _json(native)
    _require(len(native_bytes) <= MAX_JSON_BYTES, 'compiled native configuration exceeds 16 MiB')

    def distribution(weighted, realized, total):
        denominator = sum(weight for _, weight in weighted)
        return [{'id': name, 'target_probability': _fraction(weight, denominator),
                 'target_count': _fraction(total * weight, denominator), 'realized_count': realized[name],
                 'realized_probability': _fraction(realized[name], total)} for name, weight in weighted]

    field_total = sum(weight for _, weight in weighted_field)
    mixture = [(name, field_count * count + field_total) for name, field_count in weighted_field]
    report = {'schema': 'phase1-breadth-block-report/v1', 'scope': catalog['scope'],
        'provenance': copy.deepcopy(catalog['provenance']), 'lineage_id': lineage, 'block_index': block,
        'catalog_sha256': hashlib.sha256(catalog_bytes).hexdigest(), 'settings_sha256': hashlib.sha256(settings_bytes).hexdigest(),
        'native_sha256': hashlib.sha256(native_bytes).hexdigest(), 'native_json_bytes': len(native_bytes),
        'native_json_encoding': 'JSON sort_keys=True,separators=(comma,colon),ensure_ascii=True,allow_nan=False; no newline',
        'rng': RNG_SCHEMA, 'updates': updates, 'games': games, 'games_per_update': 10,
        'branch_games_per_update': {'field': 5, 'uniform': 5}, 'phase': settings['phase'],
        'episode_seeds': episode_seeds, 'episode_assignments': assignment_rows, 'excluded_seed_count': len(excluded),
        'rejected_seed_candidates': rejected_seed_candidates,
        'seed_exclusion_rule': 'Reject excluded/already allocated candidates and deterministically draw the next seed; never retry failed episodes',
        'optimizer_binary32_bits': scalar_bits, 'collection_workers': settings['collection_workers'],
        'preparation_workers': settings['preparation_workers'], 'update_backend': 'cpu',
        'episode_limits': {'max_physical_decisions': MAX_PHYSICAL_DECISIONS, 'max_policy_steps': MAX_POLICY_STEPS},
        'branches': {branch: distribution(weighted, allocations[branch], games // 2)
                     for branch, weighted in [('field', weighted_field), ('uniform', weighted_uniform)]},
        'learner_archetypes': distribution(mixture, counts['learner'], games),
        'opponent_archetypes': distribution(weighted_opponents, counts['opponent'], games),
        'policy_pool': distribution(weighted_policies, counts['policies'], games),
        'resolved_policy_assignments': [{'assignment': json.loads(key), 'games': value} for key, value in sorted(counts['assignments'].items())],
        'training_variants': [{'archetype': name, 'id': variant, 'multiplicity': weight,
            'conditional_target_probability': _fraction(weight, sum(n for _, n in weighted)),
            'learner_games': counts['learner_variants'][variant], 'opponent_games': counts['opponent_variants'][variant]}
            for name, weighted in sorted(weighted_variants.items()) for variant, weight in weighted],
        'phase_counts': dict(sorted(counts['phase'].items())),
        'selected_configuration_counts': [{'phase': phase, 'role': actor,
            'changed': counts['configuration_changes'][(phase, actor, True)],
            'unchanged': counts['configuration_changes'][(phase, actor, False)]}
            for phase in (('preboard', 'postboard') if settings['phase'] == 'mixed' else ('preboard',))
            for actor in ('learner', 'opponent')],
        'roles': [{'learner_seat': seat, 'relative_start': relative, 'target_count': _fraction(games, 4), 'games': value}
                  for (seat, relative), value in sorted(counts['roles'].items())],
        'branch_phase_role_contingency': [{'branch': branch, 'phase': phase, 'learner_seat': seat,
            'relative_start': relative, 'games': counts['joint'][(branch, phase, seat, relative)]}
            for branch in ('field', 'uniform')
            for phase in (('preboard', 'postboard') if settings['phase'] == 'mixed' else ('preboard',))
            for seat in (0, 1) for relative in ('play', 'draw')],
        'matchup_counts': [{'learner_archetype': own, 'opponent_archetype': other, 'games': counts['matchups'][(own, other)]}
                          for own in sorted(archetypes) for other in sorted(archetypes)],
        'independent_lineages_created': 0, 'native_validation_performed': False,
        'claims': ['Schedule only; catalog admission remains the caller responsibility',
            'Field targets are conditional on the supplied catalog, not a claim of full-field coverage',
            'Role/phase/branch margins are exact; their joint contingency is reported, not forced balanced',
            'Variant, opponent and policy choices are weighted random draws with recorded realized counts',
            'Recent history indexes are local to this block; fallback initial means this block initial source',
            'Predeclared postboard configurations do not train or invoke a sideboard head',
            'No model initialization, native execution, evaluation, or playing-strength claim']}
    _require(len(_json(report)) <= MAX_JSON_BYTES, 'compiler report exceeds 16 MiB')
    return {'native': native, 'report': report}
