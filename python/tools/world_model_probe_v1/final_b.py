"""Arm B's single test read (after freeze and review): test and unseen_test once
each with the selected set-transformer config; the frozen rule decides on test."""
import json, subprocess, sys
from pathlib import Path

ROOT = Path(sys.argv[1])
PY = [sys.executable, '-B', str(Path(__file__).with_name('probe.py'))]
HORIZONS = [1, 2, 3, 5, 10]
sel = json.loads((ROOT / 'SELECTION-B.json').read_text())['best']
reports = {}
for split in ('test', 'unseen_test'):
    out = ROOT / 'final-b' / f'{split}.json'
    out.parent.mkdir(exist_ok=True)
    print('>> eval', split, flush=True)
    subprocess.run(PY + ['eval', '--data', str(ROOT / 'data'), '--model', str(ROOT / 'models' / sel['tag']),
                         '--split', split, '--out', str(out)], check=True)
    reports[split] = json.loads(out.read_text())
passing = []
for k in HORIZONS:
    h = reports['test']['horizons'][str(k)]
    M, P, N = h['model']['successes'], h['persistence']['successes'], h['model']['windows']
    assert N == h['persistence']['windows']
    if 5 * M >= 4 * N and 10 * (M - P) >= N:
        passing.append(k)
H = max(passing) if passing else 0
verdict = 'GO' if H >= 5 else 'MIDDLE' if H >= 3 else 'STOP'
(ROOT / 'RESULT-B.json').write_text(json.dumps(dict(selection=sel, passing_horizons=passing, useful_horizon=H,
                                                    verdict=verdict, reports=reports), indent=1))
print(json.dumps(dict(passing=passing, H=H, verdict=verdict)))
