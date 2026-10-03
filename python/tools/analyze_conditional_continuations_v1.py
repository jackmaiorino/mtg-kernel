"""Fixed two-root conditional-risk analysis. Never a strength/promotion gate."""
import argparse
import hashlib
import json
from pathlib import Path
from scipy.stats import beta, binom

ROOTS = {'cell25-r0-s0-g115': (1, 242), 'cell45-r0-s0-g115': (2, 113)}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def seeds(name, decision, index):
    return tuple(int.from_bytes(hashlib.sha256(
        f'conditional-continuation-v1|20260922|{name}|{decision}|{index}|{seat}'.encode('ascii')
    ).digest()[:8], 'little') for seat in (0, 1))


def upper(failures, n):
    return 1.0 if failures == n else float(beta.ppf(.975, failures + 1, n - failures))


def summarize(rows, actor='p1'):
    wins = losses = draws = incomplete = 0
    for row in rows:
        terminal = row.get('terminal')
        natural = bool(terminal and terminal.get('terminal_classification') == 'natural'
                       and row.get('error') is None)
        require(row.get('natural') is natural, 'natural flag contradicts terminal/error')
        if not natural:
            incomplete += 1
        elif terminal['winner'] is None:
            require(terminal['terminal_outcome'] == 'draw', 'draw outcome disagrees')
            draws += 1
        else:
            require(terminal['winner'] in ('p0', 'p1'), 'invalid natural winner')
            require(terminal['terminal_outcome'] == terminal['winner'] + '_win', 'winner/outcome disagrees')
            if terminal['winner'] == actor:
                wins += 1
            else:
                losses += 1
    failures = losses + draws
    n = len(rows)
    return dict(n=n, wins=wins, losses=losses, draws=draws, incomplete=incomplete,
        failures=failures, analysis_complete=incomplete == 0,
        rejects_one_percent_null=(failures >= 6) if incomplete == 0 else None,
        failure_upper_97_5=upper(failures, n) if incomplete == 0 else None,
        natural_only_failure_rate=failures / (n-incomplete) if n > incomplete else None,
        sensitivity_incomplete_as_wins=dict(failures=failures, n=n, upper=upper(failures, n)),
        sensitivity_incomplete_as_failures=dict(failures=failures+incomplete, n=n,
            upper=upper(failures+incomplete, n)),
        distinct_trajectories=len({r['trajectory_sha256'] for r in rows}))


def analyze(refs):
    grouped = {name: {} for name in ROOTS}
    identities = {}
    for ref in refs:
        data = Path(ref['path']).read_bytes()
        require(hashlib.sha256(data).hexdigest() == ref['sha256'], 'output hash mismatch')
        result = json.loads(data)
        require(result['schema'] == 'mtg-kernel-bo3-continuation/v1', 'wrong output schema')
        value = result['continuation']
        opts = value['options']
        name = opts['match_id']
        require(name in ROOTS, 'undeclared root')
        game, decision = ROOTS[name]
        require((opts['game_index'], opts['decision_index'], opts['actor'], opts['selected_index'])
                == (game, decision, 'p1', 2), 'root selection differs')
        require(not opts['retain_records'], 'formal output used engineering record mode')
        require(len(value['rows']) == len(opts['policy_seeds']), 'missing declared continuation rows')
        require(isinstance(value['root'], dict), 'root was never admitted')
        identity = dict(root=value['root'], packages=result['collection']['packages'],
                        config=result['collection']['config'])
        if name in identities:
            require(identities[name] == identity, 'cross-worker root/package/config identity differs')
        else:
            identities[name] = identity
        for row, declared in zip(value['rows'], opts['policy_seeds']):
            require(declared is not None and row['policy_seeds'] == declared, 'seed entry differs')
            pair = tuple(declared)
            require(pair not in grouped[name], 'duplicate formal seed pair')
            grouped[name][pair] = row
    summaries = {}
    for name, (_, decision) in ROOTS.items():
        expected = [seeds(name, decision, i) for i in range(200)]
        require(set(grouped[name]) == set(expected), 'formal seed set is not the declared 200')
        summaries[name] = summarize([grouped[name][s] for s in expected])
    complete = all(s['analysis_complete'] for s in summaries.values())
    advance = any(s['rejects_one_percent_null'] is True for s in summaries.values())
    return dict(schema='conditional-continuation-analysis/v1', complete=complete,
        outcome='ADVANCE' if advance else 'NO-ADVANCE',
        analysis_status='COMPLETE' if complete else 'INCOMPLETE',
        interpretation='Return to reviewed proposal development only' if advance else
            ('Incomplete primary analysis; retain every row' if not complete else 'No root rejected the one-percent null'),
        roots=summaries, inputs=refs,
        rule=dict(n_per_root=200, reject_at_failures=6, null_failure_probability=.01,
            alpha_per_root=.025, null_tail=float(binom.sf(5, 200, .01)),
            power_at_five_percent=float(binom.sf(5, 200, .05))),
        independent_training_n=0,
        non_claim='Conditional policy continuation risk at two fixed hidden states and environment randomness streams. Not causal first-action regret, prevalence, whole-match strength, M1 or promotion.')


if __name__ == '__main__':
    ap = argparse.ArgumentParser()
    ap.add_argument('--inputs', type=Path, required=True, help='JSON list of pinned native output files')
    ap.add_argument('--output', type=Path, required=True)
    args = ap.parse_args()
    result = analyze(json.loads(args.inputs.read_bytes()))
    with args.output.open('x', encoding='utf8') as stream:
        json.dump(result, stream, indent=2, allow_nan=False)
    print(json.dumps({k: v for k, v in result.items() if k not in ('roots', 'inputs')}))
