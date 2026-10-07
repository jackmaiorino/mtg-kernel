"""Summarize trace-mode games for a combo pilot deck (default: Spy)."""
import json, sys, math, collections

def wilson(k, n, z=1.96):
    if n == 0: return (float('nan'),)*2
    p = k/n; d = 1+z*z/n; c = p+z*z/(2*n); h = z*math.sqrt(p*(1-p)/n+z*z/(4*n*n))
    return ((c-h)/d, (c+h)/d)

games, dec = {}, collections.defaultdict(list)
for line in open(sys.argv[1]):
    r = json.loads(line)
    if r['kind'] == 'trace_game': games[r['game']] = r
    elif r['kind'] == 'decision': dec[r['game']].append(r)

def label(c):
    k = c.get('action_kind'); src = c.get('source')
    return f"{k}:{src}" if isinstance(src, str) else k

n = len(games); w = sum(g['score'] for g in games.values())
lo, hi = wilson(w, n)
print(f"games {n}  pilot score {w/n:.3f}  95% CI [{lo:.3f}, {hi:.3f}]")
by_opp = collections.defaultdict(list)
for g in games.values(): by_opp[g['opp_deck']].append(g['score'])
for o, s in sorted(by_opp.items()): print(f"  vs {o:9s} {sum(s):4.1f}/{len(s)}")

feat = collections.Counter(); fw = collections.Counter()
casts = collections.Counter(); offered = collections.Counter()
for gid, g in games.items():
    ds = dec[gid]; f = set()
    for d in ds:
        ch = d['cands'][d['chosen']]; lab = label(ch)
        for c in {label(c) for c in d['cands']}: offered[c] += 1
        casts[lab] += 1
        if lab == 'cast_spell:Balustrade Spy': f.add('cast Spy')
        if lab == 'cast_spell:Dread Return' or (ch.get('action_kind') or '').startswith('flashback') and 'Dread' in json.dumps(ch): f.add('cast Dread Return')
        if 'Dread Return' in json.dumps(ch) and ch.get('action_kind') != 'cast_spell': f.add('Dread Return other')
        if d['lib'] == 0: f.add('own library empty')
        if 'Lotleth Giant' in d['bf']: f.add('Lotleth on battlefield')
        if lab == 'choose_target:' or ch.get('action_kind') == 'choose_target':
            if 'Balustrade Spy' in json.dumps(ch): f.add('Spy target chosen: ' + json.dumps(ch.get('target'))[:40])
    f.add(f"final lib {'0' if g['final_lib'][0]==0 else '>0'}")
    for x in f: feat[x] += 1; fw[x] += g['score']
print("\nper-game features (games, pilot score in those games):")
for x, c in feat.most_common(): print(f"  {c:4d}  {fw[x]/c:.2f}  {x}")
print("\nmost chosen actions:")
for x, c in casts.most_common(25): print(f"  {c:6d} chosen / {offered[x]:6d} offered  {x}")
