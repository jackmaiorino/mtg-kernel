"""Pinned train-only registration catalog. No engine, model or network access."""
from collections import Counter, defaultdict
from datetime import date, timedelta
import hashlib
import json
from pathlib import Path
import re

MAX_INPUT_BYTES = 64 * 1024 * 1024
MAX_ROWS = 100_000
SPLIT_SEED = 'phase1-meta-grouped-split-2026091301'


def require(condition, message):
    if not condition:
        raise ValueError(message)


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'Duplicate JSON key: ' + key)
        result[key] = value
    return result


def strict_json(raw):
    return json.loads(raw, object_pairs_hook=unique_object,
        parse_constant=lambda value: (_ for _ in ()).throw(ValueError('Nonfinite JSON: '+value)))


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, ensure_ascii=False,
        separators=(',', ':'), allow_nan=False).encode()).hexdigest()


def hash_text(value):
    return type(value) is str and re.fullmatch('[0-9a-f]{64}', value) is not None


def integer(value, label, minimum=0, maximum=MAX_ROWS):
    require(type(value) is int and minimum <= value <= maximum, 'Invalid '+label)
    return value


def read_pin(pin, evidence, limit=MAX_INPUT_BYTES):
    require(type(pin) is dict and set(pin) <= {'path', 'sha256', 'bytes'}, 'Invalid file pin')
    require(hash_text(pin.get('sha256')), 'Invalid file digest')
    require(type(pin.get('path')) is str and Path(pin['path']).is_absolute(), 'Absolute input path required')
    path = Path(pin['path'])
    with path.open('rb') as stream:
        raw = stream.read(limit+1)
    require(len(raw) <= limit, 'Input exceeds its byte bound')
    require(hashlib.sha256(raw).hexdigest() == pin['sha256'], 'Input pin differs: '+str(path))
    if 'bytes' in pin:
        require(type(pin['bytes']) is int and pin['bytes'] == len(raw), 'Input byte count differs')
    evidence.append({'path':path.as_posix(), 'sha256':pin['sha256'], 'bytes':len(raw)})
    return strict_json(raw)


def canonical_zones(row, aliases):
    result = {}
    for zone in ('mainboard', 'sideboard'):
        raw = row.get(zone)
        require(type(raw) is dict and len(raw) <= 128, 'Bounded named card counts required')
        counts = Counter()
        for name, count in raw.items():
            require(type(name) is str and 0 < len(name) <= 256, 'Invalid card name')
            normalized = aliases.get(name, name)
            counts[normalized] += integer(count, 'card count', 1, 250)
        require(sum(counts.values()) <= 250, 'Registration zone exceeds bound')
        result[zone] = dict(sorted(counts.items()))
    return result


def combined_key(zones):
    counts = Counter(zones['mainboard']) + Counter(zones['sideboard'])
    return digest({'cards':dict(sorted(counts.items()))})


def to_native(zones, label, cards):
    require(sum(zones['mainboard'].values()) == 60 and sum(zones['sideboard'].values()) == 15,
            'Current native curriculum requires exact 60/15 registrations')
    combined = Counter(zones['mainboard']) + Counter(zones['sideboard'])
    for name, count in combined.items():
        require(name in cards, 'Unimplemented card: '+name)
        _, card = cards[name]
        require(card.get('engine_capability') == 'full' and not card.get('is_token'),
                'Card is not declared fully executable: '+name)
        require(count <= 4 or 'Basic' in card.get('supertypes', []), 'Registration exceeds four copies')
    return {'label':label, **{zone:sorted(cards[name][0] for name, count in zones[zone].items()
        for _ in range(count)) for zone in ('mainboard', 'sideboard')}}


def load_catalog_v1(spec):
    """Convert pinned snapshot rows or explicit fixtures to schedule input.

    Qualification reports are verified metadata inputs. This reader does not
    itself establish their underlying playing/rules evidence or unseen holdouts.
    """
    require(type(spec) is dict and set(spec) == {'schema','scope','registry',
        'qualification_runtime_sha256','data','roster','postboards','prior_reserved_groups'},
        'Unknown or missing catalog specification fields')
    require(spec['schema'] == 'phase1-breadth-catalog-source/v1', 'Unknown catalog schema')
    scope = spec['scope']
    require(scope in ('engineering','campaign'), 'Unknown catalog scope')
    require(hash_text(spec['qualification_runtime_sha256']), 'Explicit qualification runtime required')
    evidence = []
    registry = read_pin(spec['registry'], evidence, 8*1024*1024)
    require(type(registry.get('cards')) is list and 0 < len(registry['cards']) <= 65536, 'Invalid registry')
    cards = {card['name']:(i,card) for i,card in enumerate(registry['cards'])}
    require(len(cards) == len(registry['cards']), 'Duplicate registry card name')
    data = spec['data']
    require(type(data) is dict, 'Catalog data descriptor required')
    summary = None
    if data.get('kind') == 'engineering':
        require(scope == 'engineering' and set(data) == {'kind','registrations','provenance'},
                'Engineering fixtures cannot supply campaign field weights')
        require(type(data['provenance']) is str and 0 < len(data['provenance']) <= 2048,
                'Explicit engineering provenance required')
        rows = read_pin(data['registrations'], evidence)
        aliases = {}
        field_basis = 'explicit_engineering_registration_multiplicities'
    elif data.get('kind') == 'mtgo_snapshot':
        require(set(data) == {'kind','bundle'}, 'Unknown snapshot descriptor fields')
        bundle = read_pin(data['bundle'], evidence, 1024*1024)
        require(type(bundle) is list and len(bundle) == 5, 'Five pinned snapshot files required')
        files = {}
        for pin in bundle:
            name = Path(pin['path']).name
            require(name not in files, 'Duplicate snapshot component')
            files[name] = read_pin(pin, evidence)
        require(set(files) == {'inventory.json','events.json','registrations.json','splits.json','snapshot.json'},
                'Snapshot components differ')
        summary = files['snapshot.json']
        require(summary.get('schema') == 'phase1-pauper-meta-snapshot/v1', 'Unknown meta snapshot schema')
        require(summary.get('registry',{}).get('sha256') == spec['registry']['sha256'], 'Snapshot registry differs')
        window = summary.get('window', {})
        require(type(window) is dict and type(window.get('weeks')) is int and window['weeks'] == 8,
                'Eight complete weeks are required')
        start = date.fromisoformat(window['start_inclusive'])
        end = date.fromisoformat(window['end_exclusive'])
        as_of = date.fromisoformat(window['as_of'])
        require(start.weekday() == 0 and end-start == timedelta(weeks=8) and end <= as_of,
                'Invalid complete eight-week calendar window')
        aliases = summary.get('build_provenance',{}).get('card_name_aliases')
        require(type(aliases) is dict and len(aliases) <= 256
                and all(type(k) is str and 0 < len(k) <= 256
                        and type(v) is str and 0 < len(v) <= 256 for k,v in aliases.items()),
                'Invalid pinned spelling aliases')
        rows = files['registrations.json']
        events = files['events.json']
        inventory = files['inventory.json']
        require(type(events) is list and len(events) <= 5000
                and all(type(event) is dict and type(event.get('registrations')) is list for event in events),
                'Invalid normalized events')
        require(type(inventory) is list and len(inventory) <= 5000
                and all(type(entry) is dict and type(entry.get('event_id')) is str for entry in inventory),
                'Invalid event inventory')
        inventory_ids = {entry['event_id'] for entry in inventory}
        event_ids = {event.get('event_id') for event in events}
        require(len(inventory_ids) == len(inventory) and len(event_ids) == len(events)
                and event_ids <= inventory_ids, 'Event inventory identities differ')
        require(all(start <= date.fromisoformat(event['date']) < end for event in events)
                and all(start <= date.fromisoformat(entry['index_date']) < end for entry in inventory),
                'Event or inventory entry is outside the pinned window')
        entrants, unknown_denominators = 0, []
        for event in events:
            require(all(row.get('event_id') == event['event_id'] for row in event['registrations']),
                    'Registration belongs to a different event')
            players, deck_ids = set(), set()
            for row in event['registrations']:
                require(all(type(row.get(field)) is str and re.fullmatch('[1-9][0-9]*',row[field])
                        for field in ('event_id','player_id','deck_id')), 'Invalid physical registration identity')
                require(row.get('registration_id') == row['event_id']+':'+row['player_id']
                        and row['player_id'] not in players and row['deck_id'] not in deck_ids,
                        'Duplicate or renamed physical registration')
                players.add(row['player_id']); deck_ids.add(row['deck_id'])
            count = event.get('entrant_count')
            if count is None:
                unknown_denominators.append(event['event_id'])
            else:
                entrants += integer(count, 'event entrant count', len(event['registrations']))
        missing_events = sorted(inventory_ids-event_ids)
        computed_denominator = None if missing_events or unknown_denominators else entrants
        require(summary.get('complete_inventory_denominator') == computed_denominator
                and summary.get('official_entrants_in_known_events') == entrants
                and summary.get('missing_event_ids') == missing_events
                and summary.get('unknown_denominator_event_ids') == sorted(unknown_denominators),
                'Snapshot denominator or missing-event accounting differs')
        event_rows = [row for event in events for row in event['registrations']]
        require(type(rows) is list and digest(sorted(rows,key=lambda row:row['registration_id']))
                == digest(sorted(event_rows,key=lambda row:row['registration_id'])), 'Snapshot event/row join differs')
        require(summary.get('published_registrations') == len(rows), 'Published denominator differs')
        if scope == 'campaign':
            require(summary.get('inventory_completeness_verified') is True
                    and not summary.get('missing_event_ids') and not summary.get('unknown_denominator_event_ids'),
                    'Complete competitive event inventory is not established')
            require(summary.get('status') == 'ready_for_frozen_design_review', 'Meta snapshot is not ready')
        field_basis = 'known_published_registrations_conditional_on_selected_archetypes'
    else:
        raise ValueError('Unknown catalog data source')
    require(type(rows) is list and 0 < len(rows) <= MAX_ROWS, 'Bounded registration rows required')
    require(type(aliases) is dict, 'Alias mapping required')
    roster = spec['roster']
    require(type(roster) is list and 1 < len(roster) <= 128
            and all(type(name) is str and 0 < len(name) <= 80 for name in roster)
            and len(set(roster)) == len(roster), 'At least two unique archetypes required')
    reserved = read_pin(spec['prior_reserved_groups'], evidence, 8*1024*1024)
    require(type(reserved) is dict and set(reserved) == {'schema','groups'}
            and reserved['schema'] == 'phase1-reserved-list-groups/v1'
            and type(reserved['groups']) is list and len(reserved['groups']) <= MAX_ROWS, 'Invalid prior reserved groups')
    reserved_zoned, reserved_combined = set(), set()
    for group in reserved['groups']:
        require(type(group) is dict and set(group) == {'list_sha256','combined75_sha256'}
                and all(hash_text(value) for value in group.values()), 'Invalid reserved identity')
        reserved_zoned.add(group['list_sha256']); reserved_combined.add(group['combined75_sha256'])
    groups = defaultdict(list)
    normalized = {}
    seen_rows = set()
    field_counts = Counter()
    unknown_labels = 0
    for row in rows:
        require(type(row) is dict and type(row.get('registration_id')) is str
                and 0 < len(row['registration_id']) <= 256 and row['registration_id'] not in seen_rows,
                'Duplicate or invalid registration identity')
        seen_rows.add(row['registration_id'])
        zones = canonical_zones(row, aliases)
        key = digest(zones)
        require(row.get('list_sha256') == key, 'Exact zoned registration digest differs')
        split = row.get('split')
        require(split in ('train','development','confirmation','unassigned'), 'Invalid frozen split')
        archetype = row.get('archetype')
        require(archetype is None or type(archetype) is str and 0 < len(archetype) <= 80, 'Invalid archetype')
        if archetype is None:
            unknown_labels += 1
        else:
            require(row.get('archetype_provenance'), 'Archetype provenance is missing')
            field_counts[archetype] += 1
        groups[key].append(row)
        normalized[key] = zones
        if split in ('development','confirmation'):
            reserved_zoned.add(key); reserved_combined.add(combined_key(zones))
    for key,members in groups.items():
        require(len({(row['archetype'],row['split']) for row in members}) == 1,
                'An exact list group crosses archetypes or splits')
    if summary is not None:
        require(summary.get('build_provenance',{}).get('split_seed') == SPLIT_SEED,
                'Frozen grouped split seed differs')
        by_archetype = defaultdict(list)
        for key,members in groups.items():
            archetype = members[0]['archetype']
            if archetype is None:
                require(members[0]['split'] == 'unassigned', 'Unlabeled list has assigned split')
            else:
                by_archetype[archetype].append(key)
        for archetype,keys in by_archetype.items():
            keys.sort(key=lambda key:digest([SPLIT_SEED,archetype,key]))
            count = len(keys)
            sizes = [count,0,0] if count < 10 else [count*8//10,count//10,count//10]
            if count >= 10:
                remainders = [count*8%10,count%10,count%10]
                for index in sorted(range(3),key=lambda i:(-remainders[i],i))[:count-sum(sizes)]:
                    sizes[index] += 1
            offset = 0
            for split,size in zip(('train','development','confirmation'),sizes):
                require(all(groups[key][0]['split'] == split for key in keys[offset:offset+size]),
                        'Frozen exact-list split membership differs')
                offset += size
        declared_splits = files['splits.json']
        require(type(declared_splits) is list, 'Invalid grouped split summary')
        actual_splits = defaultdict(Counter)
        for members in groups.values():
            row = members[0]
            if row['archetype'] is not None and row['split'] != 'unassigned':
                actual_splits[row['archetype']][row['split']] += 1
        seen_splits = set()
        for entry in declared_splits:
            require(type(entry) is dict and entry.get('archetype') in actual_splits
                    and entry['archetype'] not in seen_splits, 'Split summary has duplicate or absent archetype')
            seen_splits.add(entry['archetype'])
            counts = actual_splits[entry['archetype']]
            require(all(entry.get(split+'_groups') == counts[split]
                    for split in ('train','development','confirmation'))
                    and entry.get('unique_list_groups') == sum(counts.values())
                    and entry.get('sparse_all_training') is (sum(counts.values()) < 10),
                    'Frozen row membership and grouped split summary differ')
        require(seen_splits == set(actual_splits), 'Grouped split summary is incomplete')
    postboards = read_pin(spec['postboards'], evidence, 16*1024*1024)
    require(type(postboards) is dict and set(postboards) == {'schema','configurations'}
            and postboards['schema'] == 'phase1-postboard-configurations/v1'
            and type(postboards['configurations']) is dict, 'Invalid postboard configuration source')
    choices = postboards['configurations']
    require(set(choices) <= set(groups), 'Postboard data names an absent registration group')
    qualified = {}
    for key,members in groups.items():
        if scope != 'campaign':
            qualified[key] = False
            continue
        claims = [row.get('qualification') for row in members]
        if all(claim is None for claim in claims):
            qualified[key] = False
            continue
        require(all(type(claim) is dict and claim == claims[0] for claim in claims), 'Group qualification claims differ')
        claim = claims[0]
        expected = {'list_sha256':key,'registry_sha256':spec['registry']['sha256'],
                    'runtime_sha256':spec['qualification_runtime_sha256']}
        require(all(claim.get(field) == value for field,value in expected.items()), 'Qualification runtime/registry/list differs')
        report = read_pin(claim['evidence'], evidence, 8*1024*1024)
        require(report.get('schema') == 'phase1-registration-qualification/v1'
                and report.get('complete') is True and report.get('qualification_kind') == 'complete-bo3-registration'
                and all(report.get(field) == value for field,value in expected.items()), 'Qualification report differs')
        # Verify current static support as well as the exact evidence identity.
        to_native(normalized[key], key[:16], cards)
        qualified[key] = True
    selected_qualified = sum(len(members) for key,members in groups.items()
        if members[0]['archetype'] in roster and qualified[key])
    denominator = len(rows) if summary is None else summary.get('complete_inventory_denominator')
    if denominator is not None:
        integer(denominator, 'field denominator', len(rows))
    if scope == 'campaign':
        require(denominator is not None and selected_qualified*10 >= denominator*9,
                'Selected qualified registrations cover less than 90 percent of the field')
        require(unknown_labels == 0, 'Known rows retain unknown archetypes')
    archetypes = []
    admitted_ids = set()
    for archetype in roster:
        training = []
        for key,members in sorted(groups.items()):
            if members[0]['archetype'] != archetype or members[0]['split'] != 'train':
                continue
            zones = normalized[key]
            require(key not in reserved_zoned and combined_key(zones) not in reserved_combined,
                    'A training registration overlaps a reserved exact or combined-75 group')
            if scope == 'campaign' and not qualified[key]:
                continue
            registered = to_native(zones, archetype+'/'+key[:12], cards)
            options = choices.get(key, [])
            require(type(options) is list and len(options) <= 32, 'Too many postboard choices')
            selected, seen_choices = [], set()
            for option in options:
                require(type(option) is dict and set(option) == {'mainboard','sideboard'}, 'Unknown postboard fields')
                board = canonical_zones(option, aliases)
                board_key = digest(board)
                require(combined_key(board) == combined_key(zones), 'Postboard choice changes registered 75')
                require(board_key not in reserved_zoned and combined_key(board) not in reserved_combined,
                        'Postboard choice overlaps a reserved group')
                require(board_key not in seen_choices, 'Duplicate postboard configuration')
                seen_choices.add(board_key)
                selected.append(to_native(board, registered['label'], cards))
            training.append({'id':key,'multiplicity':len(members),'registered':registered,'postboard':selected})
            admitted_ids.add(key)
        require(training and field_counts[archetype] > 0, 'Archetype has no executable train group: '+archetype)
        archetypes.append({'id':archetype,'field_count':field_counts[archetype],'train':training})
    require(set(choices) <= admitted_ids, 'Postboard file contains non-training or excluded groups')
    selected_count = sum(field_counts[name] for name in roster)
    return {'scope':scope,'archetypes':archetypes,'provenance':{
        'schema':'phase1-breadth-catalog-provenance/v1','inputs':evidence,'field_basis':field_basis,
        'published_registrations':len(rows),'denominator':denominator,
        'missing_registrations':None if denominator is None else denominator-len(rows),
        'unknown_archetype_registrations':unknown_labels,'selected_known_registrations':selected_count,
        'excluded_known_registrations':len(rows)-selected_count,
        'selected_qualified_registrations':selected_qualified,
        'conditional_field_weights':True,'meta_coverage_claim':scope == 'campaign',
        'qualification_runtime_sha256':spec['qualification_runtime_sha256'],
        'registry_sha256':spec['registry']['sha256'],
        'reserved_exact_groups':len(reserved_zoned),'reserved_combined75_groups':len(reserved_combined),
        'combined75_identity':'sha256-canonical-json-cards-named-multiset-after-pinned-aliases/v1',
        'prior_training_exposure_audited':False,'unseen_holdout_claim':False,
        'native_validation':'required_separately','model_loaded':False}}
