"""Summarize regret_census_v1 mode=validity output (one or more JSONL files)."""
import json, sys, collections, statistics as stx

for path in sys.argv[1:]:
    rows = [json.loads(l) for l in open(path)]
    games = [r for r in rows if r['kind'] == 'validity_game']
    dec = [r for r in rows if r['kind'] == 'validity_decision']
    errs = [r for r in rows if r['kind'] == 'error']
    pilot = games[0]['pilot_deck'] if games else '?'
    print(f'== {path}: pilot {pilot}, games {len(games)}, errors {len(errs)}, checked decisions {len(dec)}')
    print('  pilot score', round(stx.mean(g['score'] for g in games), 3))
    print('  stateless ok', sum(d['stateless'] for d in dec), '/', len(dec))
    hd = collections.Counter(x for d in dec for x in d['hidden_diff'])
    he = sum(1 for d in dec if d['hidden_errors'])
    print('  hidden-identity diffs', dict(hd), ' redeterminized encode errors at', he, 'decisions')
    if he:
        ex = collections.Counter()
        for d in dec:
            for e in d['hidden_errors']:
                ex[tuple(sorted(set(c.get('action_kind', '?') for c in e['cands'])))] += 1
        print('   error decision kinds', ex.most_common(5))
    tv = [t for d in dec for t in d['digest_tv'][:4]]
    tvz = [d['digest_tv'][4] for d in dec]
    flips = sum(d['digest_argmax_flips'] for d in dec)
    print(f'  digest resample: mean TV {stx.mean(tv):.4f}, p95 {sorted(tv)[int(.95*len(tv))]:.4f}, max {max(tv):.3f};'
          f' argmax flips {flips}/{4*len(dec)}; zeroed digest mean TV {stx.mean(tvz):.4f}')
    big = sorted(dec, key=lambda d: -max(d['digest_tv'][:4]))[:3]
    for d in big:
        print('    largest:', d['step'], d['chosen_kinds'], 'k', d['k'], 'TV', [round(x, 3) for x in d['digest_tv']])
    hog = collections.Counter()
    nh = 0
    for d in dec:
        if d['hash_only_groups']:
            nh += 1
        for g in d['hash_only_groups']:
            kinds = tuple(sorted(set(json.loads(x).get('action_kind', '?') for x in g)))
            hog[kinds] += 1
    print(f'  decisions with hash-only-distinguished actions: {nh}/{len(dec)}', hog.most_common(6))
    ed = collections.defaultdict(collections.Counter)
    for d in dec:
        for k, v in d['edits'].items():
            diff = v['diff'] if isinstance(v, dict) else v
            if isinstance(diff, str):
                ed[k]['encode_error'] += 1
                continue
            sem = any(x not in ('state_digest', 'action_hash', 'node_ids') for x in diff)
            ed[k]['visible' if sem else ('hash_only' if diff else 'invisible')] += 1
    for k, c in sorted(ed.items()):
        print(f'  edit {k}: {dict(c)}')
    cnt = collections.Counter()
    for g in games:
        for k, v in g['counts'].items():
            cnt[k] += v
    for k, v in sorted(cnt.items()):
        if 'strands' in k.lower() and ('True' in k or 'chosen' in k) or 'Basilisk' in k:
            print('  ', k, v)
