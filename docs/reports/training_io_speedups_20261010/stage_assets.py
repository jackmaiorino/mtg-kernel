"""Copy owned immutable inputs, tool snapshots and harness into a fresh run root.

No builds, training, reservation changes or pruning. Invoke only with both
source revisions committed and their independent runtime receipts available.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

BASELINE = 'b3bd1c1c0d7b5b6766359a3901549de768bcc175'


def pin(path):
    with Path(path).open('rb') as stream:
        return {'path': str(path), 'sha256': hashlib.file_digest(stream, 'sha256').hexdigest()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path('D:/training-io-speedups-20261010'))
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--candidate', type=Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve(); desktop = root / 'desktop'
    if desktop.exists():
        raise ValueError('preserve existing staged sources')
    source = Path(__file__).resolve().parent
    old = Path('D:/training-speedups-20261009')
    refs, revisions = [], {}

    def copy(src, dst):
        dst.parent.mkdir(parents=True, exist_ok=True)
        with src.open('rb') as a, dst.open('xb') as b:
            shutil.copyfileobj(a, b); b.flush(); os.fsync(b.fileno())
        original, copied = pin(src), pin(dst)
        if original['sha256'] != copied['sha256']:
            raise ValueError('staging copy differs')
        refs.append({'source': original, 'copy': copied})

    for name, checkout in [('baseline', args.baseline), ('candidate', args.candidate)]:
        if subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=checkout).strip():
            raise ValueError('tracked source changes must be committed: ' + name)
        revisions[name] = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=checkout, text=True).strip()
        runtime = json.loads((root / 'runtimes' / (name + '.json')).read_bytes())
        if runtime['engine_commit'] != revisions[name] or pin(runtime['binary']['path']) != runtime['binary']:
            raise ValueError('runtime/source identity differs: ' + name)
        if name == 'baseline' and revisions[name] != BASELINE:
            raise ValueError('baseline revision differs')
        for path in sorted((checkout / 'python/tools').rglob('*.py')):
            copy(path, desktop / name / path.relative_to(checkout))
        copy(root / 'runtimes' / (name + '.json'), desktop / 'runtimes' / (name + '.json'))
    for path in sorted((source / 'desktop').glob('*.py')):
        copy(path, desktop / path.name)
    for path in sorted((source / 'helpers').glob('*.py')):
        copy(path, root / 'helpers' / path.name)
    for name in ['prepare_comparison.py', 'measure_allocations.py', 'resume_qualifications.py']:
        copy(source / name, (desktop if name == 'prepare_comparison.py' else root) / name)
    for name in ['source-config.json', 'runtime_decks_v1.json']:
        copy(old / 'desktop' / name, desktop / name)
    dependencies = []
    for name in ['data/cards_v1.json', 'docs/research/sideboard_plan_inputs_2026-09/registered_nine_75s.json']:
        copy(old / name, root / name)
        dependencies.append(pin(root / name))
    (desktop / 'maintenance-dependencies-20261010.json').write_text(json.dumps(dependencies, indent=2), encoding='utf-8')
    for name in ['measure/hot', 'measure/cold', 'controllers', 'requests']:
        (desktop / name).mkdir(parents=True, exist_ok=True)
    for name in ['measure/hot', 'measure/cold']:
        subprocess.run(['compact.exe', '/C', '/I', str(desktop / name)], check=True, capture_output=True)
    with (root / 'staged-assets.json').open('x', encoding='utf-8') as stream:
        json.dump({'revisions': revisions, 'files': refs, 'training_started': False}, stream, indent=2)
    print(json.dumps({'staged': str(desktop), 'revisions': revisions, 'files': len(refs)}))


if __name__ == '__main__':
    main()
