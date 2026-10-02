"""Public-state summaries of previously found motifs; never certify a win."""
import hashlib
import json
import zipfile
from pathlib import Path

B = Path('E:/mtg-meta-recovery-20260921')
source = B / 'natural-burn-combinations-001.json'
out = B / 'natural-burn-candidate-inspection-001.json'
assert not out.exists()
scan = json.loads(source.read_bytes())
cards = json.loads(Path('E:/mtg-kernel-public-stack-features-codex/data/cards_v1.json').read_bytes())['cards']
def digest(x):
    return hashlib.sha256(json.dumps(x, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
rows = []
for c in scan['candidates']:
    if not (c['nominal_hand_dependent'] or c['nominal_immediate']):
        continue
    path = Path(c['bundle']['path'])
    assert hashlib.sha256(path.read_bytes()).hexdigest() == c['bundle']['sha256']
    with zipfile.ZipFile(path) as z:
        r = json.loads(z.read('result.json'))
    row = next(x for x in r['collected']['trajectory']['games'][c['game_index']]['decisions'] if x['decision_index'] == c['decision_index'])
    v = row['visible']; o = v['observation']; p = o['projection']
    # Removing only recording counters does not claim semantic equivalence.
    canonical = json.loads(json.dumps(v))
    for key in ['step_index', 'physical_decision_id', 'substep_index', 'substep_count']:
        canonical['observation'].pop(key, None)
    names = [[dict(name=cards[x['stable']['card_db_id']]['name'], arena_id=x['stable']['arena_id'], tapped=x['tapped']) for x in side] for side in p['battlefield']]
    nums = [int(x) for x in row['behavior']['mass_numerators']]
    probs = [x / sum(nums) for x in nums]
    rows.append(dict(candidate=c, visible_sha256=digest(v), counter_free_visible_sha256=digest(canonical), battlefield=names,
        public_other={k:val for k,val in p.items() if k not in ['battlefield']},
        action_probabilities=probs, ordered_actions=v['ordered_actions']))
groups = {}
for row in rows:
    c = row['candidate']; groups.setdefault(row['counter_free_visible_sha256'], []).append([c['match'],c['game_index'],c['decision_index']])
result = dict(complete=True, scan_sha256=hashlib.sha256(source.read_bytes()).hexdigest(), rows=rows,
    exact_counter_free_groups=list(groups.values()), limits='No engine branches or outcomes read. Hash equality excludes only recording counters; unequal hashes do not establish strategic independence.')
out.write_text(json.dumps(result, indent=2), encoding='utf8')
print(json.dumps(dict(rows=len(rows), groups=list(groups.values())),indent=2))
for row in rows:
    c=row['candidate']
    print(c['match'],c['decision_index'],row['battlefield'],row['action_probabilities'])
