"""R14 semantics report: compare generated CardDef tables of the training-era and destination builds.

For every card on the R14 allowlist, report the generated-table fields that differ (capability,
targets, effects, keywords, colors, abilities and the rest) and its presence in each canonical deck
(registered main, registered sideboard, any declared postboard main) across D3's pinned requests:
the Rally list the archival members pilot and the seven learner decks they face.
Evidence is the sealed card_defs.rs tables (build outputs of each commit's own build.rs), not the
registry diff. Identity metadata and static tables only; no game is played.

Usage: python python/tools/r14_semantics_report_v1.py TRAINING_CARD_DEFS DESTINATION_CARD_DEFS REQUESTS_DIR OUTPUT.json
"""
import collections, hashlib, json, re, sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
ALLOWLIST = REPO / 'data/line_a_source_registries/allowlists/r14-16d308da03202644-to-ef738001c3730760.json'


def split_top_level(body):
    parts, depth, current, in_string = [], 0, [], False
    for index, char in enumerate(body):
        if char == '"' and (index == 0 or body[index - 1] != chr(92)):
            in_string = not in_string
        if not in_string:
            if char in '([{':
                depth += 1
            elif char in ')]}':
                depth -= 1
            elif char == ',' and depth == 0:
                parts.append(''.join(current).strip()); current = []; continue
        current.append(char)
    if ''.join(current).strip():
        parts.append(''.join(current).strip())
    return parts


def parse_card_defs(path):
    text = Path(path).read_text(encoding='utf-8')
    cards = collections.OrderedDict()
    for match in re.finditer(r'CardDef \{', text):
        start, depth, index = match.end(), 1, match.end()
        while depth:
            depth += {'{': 1, '}': -1}.get(text[index], 0); index += 1
        fields = {}
        for part in split_top_level(text[start:index - 1]):
            if ':' not in part:
                continue
            key, value = part.split(':', 1)
            fields[key.strip()] = ' '.join(value.split())
        if 'name' in fields:
            cards[json.loads(fields['name'])] = fields
    return cards


STATIC_CHARACTERISTICS = ('types', 'subtypes', 'supertypes', 'colors', 'keywords', 'power', 'toughness', 'cost', 'is_land', 'produces_mana')


def deck_presence(requests_dir, names):
    """Union over D3's pinned baseline requests: per deck, registered main and sideboard counts and postboard main ids."""
    decks = collections.defaultdict(lambda: {'main': collections.Counter(), 'sideboard': collections.Counter(), 'postboard_main': set(), 'requests': 0})
    for path in sorted(Path(requests_dir).glob('*-baseline.json')):
        match = json.loads(path.read_bytes())['matches'][0]
        for seat in (0, 1):
            deck = decks[match['config']['deck_ids'][seat]]
            deck['requests'] += 1
            deck['main'] = collections.Counter({names[i]: c for i, c in collections.Counter(match['registered'][seat]['mainboard']).items()})
            deck['sideboard'] = collections.Counter({names[i]: c for i, c in collections.Counter(match['registered'][seat]['sideboard']).items()})
            if match.get('postboard'):
                deck['postboard_main'] |= {names[i] for i in match['postboard'][seat]['mainboard']}
    return decks


def main(training_path, destination_path, requests_dir, output):
    allowlist = json.loads(ALLOWLIST.read_bytes())
    training, destination = parse_card_defs(training_path), parse_card_defs(destination_path)
    names = [card['name'] for card in json.loads((REPO / 'data/cards_v1.json').read_bytes())['cards']]
    decks = deck_presence(requests_dir, names)
    rows = []
    for entry in allowlist['entries']:
        name = entry['name']
        old, new = training.get(name), destination.get(name)
        if old is None or new is None:
            raise SystemExit('generated table lacks ' + name)
        shared = sorted(set(old) & set(new) - {'name'})
        changed = [f for f in shared if old[f] != new[f]]
        rows.append({'id': entry['id'], 'name': name, 'registry_differences': [d['field'] for d in entry['differences']],
                     'training_capability': old.get('capability'), 'destination_capability': new.get('capability'),
                     'generated_fields_changed': {f: {'training': old[f], 'destination': new[f]} for f in changed},
                     'destination_only_fields': sorted(set(new) - set(old)),
                     'static_characteristics_changed': sorted(f for f in changed if f in STATIC_CHARACTERISTICS),
                     'castable_training': old.get('capability') != 'CardCapability::NoEffect' and old.get('is_land') != 'true',
                     'castable_destination': new.get('capability') != 'CardCapability::NoEffect' and new.get('is_land') != 'true',
                     'deck_presence': {deck: {'main': d['main'].get(name, 0), 'sideboard': d['sideboard'].get(name, 0),
                                              'in_any_postboard_main': name in d['postboard_main']}
                                       for deck, d in sorted(decks.items())
                                       if d['main'].get(name) or d['sideboard'].get(name) or name in d['postboard_main']}})
    in_rally = [r for r in rows if 'Rally' in r['deck_presence']]
    faced = sorted({deck for r in rows for deck in r['deck_presence']})
    report = {'schema': 'mtg-kernel-r14-semantics-report/v1',
              'allowlist_sha256': hashlib.sha256(ALLOWLIST.read_bytes()).hexdigest(),
              'training_card_defs': {'path': str(training_path), 'sha256': hashlib.sha256(Path(training_path).read_bytes()).hexdigest()},
              'destination_card_defs': {'path': str(destination_path), 'sha256': hashlib.sha256(Path(destination_path).read_bytes()).hexdigest()},
              'requests_dir': str(requests_dir), 'decks_covered': {deck: d['requests'] for deck, d in sorted(decks.items())},
              'encoding_facts': ['the V3 feature contract and the card-token mapping are unchanged by the route: names and order are identical, so every card keeps token id+1',
                                 'the V3 observation and tensorizer sources read engine object state, not CardDef accessors; a card changes what the model sees through engine behaviour (castability, targets, effects, abilities) and, where changed, static characteristics',
                                 'at 1804e9f9 CardCapability::NoEffect is not executable (card_def.rs:84-86 there), so is_castable() was false for all 87 cards: they were dead cards in the training-era engine'],
              'summary': {'cards': len(rows),
                          'executability_changed': sum(r['training_capability'] != r['destination_capability'] for r in rows),
                          'castable_training_to_destination': sum((not r['castable_training']) and r['castable_destination'] for r in rows),
                          'static_characteristics_changed_cards': [r['name'] for r in rows if r['static_characteristics_changed']],
                          'rally_cards': [r['name'] for r in in_rally],
                          'decks_containing_admitted_cards': faced},
              'cards': rows,
              'nonclaims': ['static generated tables only; no game or model output was produced or read',
                            'deck presence is the union of D3 attempt004 pinned registrations and declared postboards; line (a) registrations must be checked when frozen']}
    data = (json.dumps(report, indent=2) + '\n').encode('utf-8')
    Path(output).write_bytes(data)
    print(output, hashlib.sha256(data).hexdigest(), json.dumps(report['summary']))


if __name__ == '__main__':
    main(*sys.argv[1:5])
