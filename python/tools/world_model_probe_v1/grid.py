"""Validation-only model selection for the world-model probe (no test split is read).

Builds the focal-view dataset (and the full-information one for the disclosed
upper-bound arm), trains the grid, evaluates every config on validation with and
without integer feedback, and picks the config with the best mean primary
accuracy over the decision horizons."""
import json, subprocess, sys
from pathlib import Path

ROOT = Path(sys.argv[1])
PARTS, MANIFEST = sys.argv[2], sys.argv[3]
PY = [sys.executable, '-B', str(Path(__file__).with_name('probe.py'))]
GRID = [dict(hidden=h, unroll=u) for h in (512, 1024) for u in (1, 5)]
HORIZONS = ['1', '2', '3', '5', '10']


def run(*args):
    print('>>', ' '.join(map(str, args)), flush=True)
    subprocess.run(PY + list(map(str, args)), check=True)


for name, extra in (('data', []), ('data-full', ['--full-info'])):
    if not (ROOT / name / 'meta.json').exists():
        run('build', '--parts', PARTS, '--manifest', MANIFEST, '--out', ROOT / name, *extra)
results = []
for g in GRID:
    tag = f"h{g['hidden']}-u{g['unroll']}"
    run('train', '--data', ROOT / 'data', '--out', ROOT / 'models' / tag, '--hidden', g['hidden'], '--unroll', g['unroll'])
    for feed in (False, True):
        out = ROOT / 'validation' / f"{tag}-{'round' if feed else 'soft'}.json"
        out.parent.mkdir(exist_ok=True)
        run('eval', '--data', ROOT / 'data', '--model', ROOT / 'models' / tag, '--split', 'validation', '--out', out,
            *(['--round-feed'] if feed else []))
        rep = json.loads(out.read_text())
        score = sum(rep['horizons'][k]['model']['primary'] for k in HORIZONS) / len(HORIZONS)
        results.append(dict(tag=tag, round_feed=feed, score=score, **g))
best = max(results, key=lambda r: r['score'])
(ROOT / 'SELECTION.json').write_text(json.dumps(dict(results=results, best=best), indent=1))
print(json.dumps(best))
