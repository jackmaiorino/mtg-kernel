"""Never-sampled audit over trace-mode games: actions the pilot is offered
in many games but almost never takes. Labels are action kind + source card,
plus the target's side (self/opp) or object name for targeting actions."""
import json, sys, collections

games, rows = {}, []
for line in open(sys.argv[1]):
    r = json.loads(line)
    if r['kind'] == 'trace_game': games[r['game']] = r
    elif r['kind'] == 'decision': rows.append(r)

def label(c, pilot):
    k = c.get('action_kind'); src = c.get('source')
    lab = f"{k}:{src}" if isinstance(src, str) else str(k)
    t = c.get('target')
    if isinstance(t, dict):
        if t.get('target_kind') == 'player':
            lab += ' -> ' + ('self' if t['player'] == f"p{pilot}" else 'opp')
        elif isinstance(t.get('object'), str):
            lab += ' -> ' + t['object']
    if k == 'activate_ability': lab += f"#{c.get('ability_index')}"
    return lab

offer_games = collections.defaultdict(set); offers = collections.Counter()
chosen = collections.Counter(); mass = collections.Counter(); win_when_taken = collections.defaultdict(list)
for r in rows:
    pilot = games[r['game']]['pilot_seat']
    labs = [label(c, pilot) for c in r['cands']]
    for lab, p in zip(labs, r['probs']):
        mass[lab] += p
    for lab in set(labs):
        offers[lab] += 1; offer_games[lab].add(r['game'])
    chosen[labs[r['chosen']]] += 1
    win_when_taken[labs[r['chosen']]].append(games[r['game']]['score'])
min_games = int(sys.argv[2]) if len(sys.argv) > 2 else 50
print(f"{'games':>6} {'offers':>7} {'taken':>6} {'mean p':>8}  action")
out = []
for lab, gs in offer_games.items():
    if len(gs) < min_games or lab in ('pass', 'None'): continue
    rate = chosen[lab] / offers[lab]
    if rate < 0.01: out.append((rate, lab, len(gs)))
for rate, lab, g in sorted(out, key=lambda x: -x[2]):
    print(f"{g:6d} {offers[lab]:7d} {chosen[lab]:6d} {mass[lab]/offers[lab]:8.5f}  {lab}")
