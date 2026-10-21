"""Arm B (set transformer) validation-only selection on the frozen probe-002 data."""
import json, subprocess, sys
from pathlib import Path

ROOT = Path(sys.argv[1])
PY = [sys.executable, '-B', str(Path(__file__).with_name('probe.py'))]
GRID = [dict(hidden=d, unroll=5) for d in (256, 384)]
HORIZONS = ['1', '2', '3', '5', '10']


def run(*args):
    print('>>', ' '.join(map(str, args)), flush=True)
    subprocess.run(PY + list(map(str, args)), check=True)


results = []
for g in GRID:
    tag = f"set-d{g['hidden']}-u{g['unroll']}"
    run('train', '--data', ROOT / 'data', '--out', ROOT / 'models' / tag, '--arch', 'set', '--hidden', g['hidden'],
        '--unroll', g['unroll'], '--lr', '5e-4')
    out = ROOT / 'validation' / f'{tag}.json'
    run('eval', '--data', ROOT / 'data', '--model', ROOT / 'models' / tag, '--split', 'validation', '--out', out)
    rep = json.loads(out.read_text())
    results.append(dict(tag=tag, score=sum(rep['horizons'][k]['model']['primary'] for k in HORIZONS) / len(HORIZONS), **g))
best = max(results, key=lambda r: r['score'])
(ROOT / 'SELECTION-B.json').write_text(json.dumps(dict(results=results, best=best), indent=1))
print(json.dumps(best))
