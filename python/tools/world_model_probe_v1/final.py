"""The single test read of the world-model probe, after the freeze and review.

Evaluates the selected config once on test and unseen_test, trains the same
config on the full-information data (disclosed upper bound, never the
decision), applies the frozen decision rule to the held-out-seed test set and
writes RESULT.json."""
import json, subprocess, sys
from pathlib import Path

ROOT = Path(sys.argv[1])
PY = [sys.executable, '-B', str(Path(__file__).with_name('probe.py'))]
HORIZONS = [1, 2, 3, 5, 10]
sel = json.loads((ROOT / 'SELECTION.json').read_text())['best']


def run(*args):
    print('>>', ' '.join(map(str, args)), flush=True)
    subprocess.run(PY + list(map(str, args)), check=True)


reports = {}
for split in ('test', 'unseen_test'):
    out = ROOT / 'final' / f'{split}.json'
    out.parent.mkdir(exist_ok=True)
    run('eval', '--data', ROOT / 'data', '--model', ROOT / 'models' / sel['tag'], '--split', split, '--out', out)
    reports[split] = json.loads(out.read_text())
full_model = ROOT / 'models' / f"{sel['tag']}-full"
run('train', '--data', ROOT / 'data-full', '--out', full_model, '--hidden', sel['hidden'], '--unroll', sel['unroll'])
for split in ('validation', 'test'):
    out = ROOT / 'final' / f'full-info-{split}.json'
    run('eval', '--data', ROOT / 'data-full', '--model', full_model, '--split', split, '--out', out)
    reports[f'full_info_{split}'] = json.loads(out.read_text())

passing = []
for k in HORIZONS:
    h = reports['test']['horizons'][str(k)]
    # Frozen thresholds in exact integer form: M/N >= 0.80 and (M - P)/N >= 0.10.
    M, P, N = h['model']['successes'], h['persistence']['successes'], h['model']['windows']
    assert N == h['persistence']['windows']
    if 5 * M >= 4 * N and 10 * (M - P) >= N:
        passing.append(k)
H = max(passing) if passing else 0
prefix = 0
for k in HORIZONS:
    if k in passing:
        prefix = k
    else:
        break
verdict = 'GO' if H >= 5 else 'MIDDLE' if H >= 3 else 'STOP'
result = dict(selection=sel, passing_horizons=passing, useful_horizon=H, contiguous_horizon=prefix,
              verdict=verdict, reports=reports)
(ROOT / 'RESULT.json').write_text(json.dumps(result, indent=1))
print(json.dumps(dict(passing=passing, H=H, contiguous=prefix, verdict=verdict)))
