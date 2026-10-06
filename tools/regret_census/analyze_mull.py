#!/usr/bin/env python3
"""Keep-7 vs mulligan value per opening hand, cross-fit, game-clustered bootstrap."""
import json, sys, random, collections
rows = [json.loads(l) for f in sys.argv[1:] for l in open(f) if l.strip()]
H = [r for r in rows if r['kind'] == 'mulligan' and r['bottoms']]
def mean(x): return sum(x) / len(x)
def hand(r):
    K = r['keep_scores']; B = [b['scores'] for b in r['bottoms']]; m = len(B[0]); h = m // 2
    out = {}
    out['keep'] = mean(K); out['mull_avg_bottom'] = mean([mean(b) for b in B]); out['mull_best_naive'] = max(mean(b) for b in B)
    gain = 0
    for (dlo, dhi), (elo, ehi) in (((0, h), (h, m)), ((h, m), (0, h))):
        Kd = mean(K[dlo:dhi] + K[m + dlo:m + dhi]) if len(K) >= 2 * m else mean(K[dlo:dhi])
        Ke = mean(K[elo:ehi] + K[m + elo:m + ehi]) if len(K) >= 2 * m else mean(K[elo:ehi])
        bi = max(range(len(B)), key=lambda i: mean(B[i][dlo:dhi]))
        if mean(B[bi][dlo:dhi]) > Kd:
            gain += mean(B[bi][elo:ehi]) - Ke
    out['cf_gain'] = gain / 2
    out['mull_decided'] = None
    return out
S = []
for r in H:
    o = hand(r); o['game'] = r['game']; o['deck'] = r['decks'][r['observer']]; o['play'] = r['starting_player'] == r['observer']; S.append(o)
games = sorted({s['game'] for s in S}); byg = collections.defaultdict(list)
for s in S: byg[s['game']].append(s)
def boot(f, B=2000):
    pt = f(games); rng = random.Random(5); b = sorted(f([rng.choice(games) for _ in games]) for _ in range(B))
    return pt, b[int(.025 * B)], b[int(.975 * B)]
def avg(key, filt=lambda s: True):
    return lambda gs: mean([s[key] for g in gs for s in byg[g] if filt(s)] or [0])
print(f"hands={len(S)} games={len(games)}")
for k in ('keep', 'mull_avg_bottom', 'mull_best_naive', 'cf_gain'):
    pt, lo, hi = boot(avg(k)); print(f"  {k:16s} {100*pt:6.1f} [{100*lo:6.1f},{100*hi:6.1f}]")
print("cross-fit gain from mulliganing when the rollouts favour it, per hand (pp), by deck:")
for d in sorted({s['deck'] for s in S}):
    pt, lo, hi = boot(avg('cf_gain', lambda s, d=d: s['deck'] == d)); pk, _, _ = boot(avg('keep', lambda s, d=d: s['deck'] == d), B=10)
    print(f"  {d:10s} n={sum(s['deck']==d for s in S):3d} keep {100*pk:5.1f}  gain {100*pt:5.1f} [{100*lo:5.1f},{100*hi:5.1f}]")
bad = sum(s['keep'] < .2 and s['mull_avg_bottom'] > s['keep'] + .2 for s in S)
print(f"hands with keep<20% and average mulligan >= keep+20pp: {bad}/{len(S)}")
