#!/usr/bin/env python3
"""Regret census summary as shares and per-decision means, game-clustered bootstrap CIs."""
import json, sys, random, collections
rows = [json.loads(l) for f in sys.argv[1:] for l in open(f) if l.strip()]
games = {r['game']: r for r in rows if r['kind'] == 'game'}
roots = [r for r in rows if r['kind'] == 'root' and r['game'] in games]
errors = [r for r in rows if r['kind'] == 'error']

def est(r):
    w = r['wins']; ev = r['evaluated']; m = len(w[0]); h = m // 2
    p = [r['probs'][a] for a in ev]; s = sum(p); p = [x / s for x in p]
    q = [sum(x) / m for x in w]
    qa = [sum(x[:h]) / h for x in w]; qb = [sum(x[h:]) / (m - h) for x in w]
    va = sum(a * b for a, b in zip(p, qa)); vb = sum(a * b for a, b in zip(p, qb))
    ia = max(range(len(q)), key=qa.__getitem__); ib = max(range(len(q)), key=qb.__getitem__)
    v = sum(a * b for a, b in zip(p, q))
    best = max(range(len(q)), key=q.__getitem__)
    return dict(cross=.5 * ((qb[ia] - vb) + (qa[ib] - va)), naive=max(q) - v, v=v,
                best=r['kinds'][ev[best]])

def cat(r, e):
    kinds = set(r['kinds'])
    if 'ChooseTarget' in kinds or 'ChooseCostTarget' in kinds: return 'targeting'
    if any('Attack' in k for k in kinds): return 'attacks'
    if any('Block' in k for k in kinds): return 'blocks'
    if kinds <= {'Pass', 'ActivateManaAbility'}: return 'pass or tap mana only'
    if 'ActivateManaAbility' in kinds: return 'cast/ability with mana choices'
    if kinds & {'CastSpell', 'PlayLand', 'ActivateAbility', 'ActivateManaAbility'}: return 'cast/land/ability/pass choice'
    return 'other choices (modes, colors, discard, ...)'

S = []
for r in roots:
    e = est(r); e['cat'] = cat(r, e); e['game'] = r['game']; e['deck'] = r['decks'][r['actor']]; e['r'] = r
    S.append(e)
print(f"games={len(games)} roots={len(S)} errors={len(errors)}")
dec = sum(v < .1 or v > .9 for v in (e['v'] for e in S))
print(f"roots already decided (V<0.1 or V>0.9): {dec}/{len(S)} = {100*dec/len(S):.0f}%")
gl = sorted(games)
byg = collections.defaultdict(list)
for e in S: byg[e['game']].append(e)

def summary(keyfn, B=2000):
    keys = sorted({keyfn(e) for e in S})
    def stat(sample):
        tot = sum(max(0, 0) + e['cross'] for g in sample for e in byg[g])
        out = {}
        for k in keys:
            xs = [e['cross'] for g in sample for e in byg[g] if keyfn(e) == k]
            out[k] = (sum(xs) / max(1, len(xs)), sum(xs) / tot if tot else 0, len(xs))
        return out
    pt = stat(gl); rng = random.Random(7); bs = []
    for _ in range(B): bs.append(stat([rng.choice(gl) for _ in gl]))
    res = []
    for k in keys:
        m = sorted(b[k][0] for b in bs); sh = sorted(b[k][1] for b in bs)
        res.append((k, pt[k][2], pt[k][0], m[int(.025*B)], m[int(.975*B)], pt[k][1], sh[int(.025*B)], sh[int(.975*B)]))
    return sorted(res, key=lambda x: -x[5])

print("\nBy decision type: n, mean regret per decision (pp) [95% CI], share of all regret [95% CI]")
for k, n, m, lo, hi, sh, slo, shi in summary(lambda e: e['cat']):
    print(f"  {k:45s} n={n:4d}  {100*m:5.1f} [{100*lo:5.1f},{100*hi:5.1f}]  share {100*sh:4.0f}% [{100*slo:4.0f},{100*shi:4.0f}]")
print("\nBy acting deck:")
for k, n, m, lo, hi, sh, slo, shi in summary(lambda e: e['deck']):
    print(f"  {k:12s} n={n:4d}  {100*m:5.1f} [{100*lo:5.1f},{100*hi:5.1f}]  share {100*sh:4.0f}% [{100*slo:4.0f},{100*shi:4.0f}]")
tot = sum(e['cross'] for e in S)
print(f"\nmean cross-fit regret per sampled decision: {100*tot/len(S):.2f}pp; naive {100*sum(e['naive'] for e in S)/len(S):.2f}pp")
