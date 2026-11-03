"""W5 Stage 0b capture producer (collab LANES/lead/W5-DESIGN.md v1.3 section 6.1): b512 episodes under C's recipe and
schedule in the chain-seed namespace `w5/0b`, the four candidate searches and the Q_ref rollouts at each episode's
placed decision, written as `w5-0b-capture/v1` records for tools/w5/stage0b.py (E8) and the value-leaf panel read.

The engine side is the w5-xact-v1 hook's capture mode (`W5CollectOptionsV1.capture_0b`): one `collect` command per
update of the schedule, one collection worker (so each candidate's process-CPU delta is that search's CPU), the
learner's own draw at every decision (the episode is the plain b512 episode).

  capture0b.py jobs --base BASE.json --parent DOSE.json --out-dir DIR --updates N [--first-update U]
                    [--chain w5/0b] [--budgets 16000 64000] [--seeds 0 1] [--rollouts 128] [--key-memo-mib 512]
      one collect command per update: DIR/jobs/u{update:06d}.json, output DIR/runs/u{update:06d}
  capture0b.py run BINARY DIR             runs every job whose collection.json is missing, in update order
  capture0b.py convert DIR [--out FILE]   capture records (JSON lines, episode order) from the finished jobs

Gate (c) (section 7.4) uses the same tool with --chain w5/gate-c/<arm> and the arm's chain-1 checkpoint as the
parent dose record.
"""
import argparse
import copy
import hashlib
import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / 'ex1'))
import qual  # noqa: E402

CAPTURE_SCHEMA = 'w5-0b-capture/v1'
RECIPE = 'w5-xact-v1'


def read(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))


def write(path, doc):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(doc, indent=1) + '\n', encoding='utf-8')


def resolve(base, scheduled, source):
    """The driver's episode resolution (resolve_episodes): every assignment names a source; `current` and
    `initial` are the fixed b512 source here (no update ever runs)."""
    kind = scheduled['opponent'].get('kind')
    if kind in ('current', 'initial'):
        opponent = source
    elif kind == 'fixed':
        matches = [o['source'] for o in base['opponents'] if o['id'] == scheduled['opponent']['id']]
        if len(matches) != 1:
            raise SystemExit(f"stop: opponent {scheduled['opponent']['id']} is not in the roster once")
        opponent = matches[0]
    else:
        raise SystemExit(f'stop: opponent kind {kind} has no meaning without training')
    episode = copy.deepcopy(scheduled['episode'])
    episode['opponent'] = opponent
    return episode


def jobs(base, dose, out_dir, updates, first_update=0, chain='w5/0b', budgets=(16000, 64000), seeds=(0, 1),
         rollouts=128, key_memo_mib=512):
    """One collect command per update: the base's schedule with ids and seeds keyed by the chain (EX1's
    qualification construction), opponents resolved, the hook in capture mode."""
    cfg = qual.build_config(base, dose, Path(out_dir) / 'unused', chain, chain, first_update, updates, 1)
    source = cfg['initial_source']
    out = []
    for i, item in enumerate(cfg['iterations']):
        update = first_update + i
        command = {'mode': 'collect', 'source': source,
                   'episodes': [resolve(cfg, scheduled, source) for scheduled in item['episodes']],
                   'max_non_natural_episode_fraction': cfg.get('max_non_natural_episode_fraction', 0.0),
                   'ex1': {'run': chain, 'update': update, 'budget': max(budgets),
                           'w5': {'recipe': RECIPE, 'key_memo_mib': key_memo_mib,
                                  'capture_0b': {'budgets': list(budgets), 'search_seeds': list(seeds),
                                                 'rollouts': rollouts}}},
                   'output_directory': (Path(out_dir) / 'runs' / f'u{update:06d}').as_posix()}
        if not command['max_non_natural_episode_fraction']:
            del command['max_non_natural_episode_fraction']
        # The run's sampler and math, as the native driver's collection command carries them.
        for key in ('collection_sampler', 'training_math'):
            if key in cfg:
                command[key] = cfg[key]
        path = Path(out_dir) / 'jobs' / f'u{update:06d}.json'
        write(path, command)
        out.append(path.as_posix())
    write(Path(out_dir) / 'jobs' / 'manifest.json',
          {'schema': 'w5-0b-capture-jobs/v1', 'chain': chain, 'first_update': first_update, 'updates': updates,
           'source_checkpoint_sha256': source['checkpoint']['sha256'], 'budgets': list(budgets),
           'seeds': list(seeds), 'rollouts': rollouts, 'key_memo_mib': key_memo_mib, 'jobs': out})
    return out


def run(binary, out_dir):
    manifest = read(Path(out_dir) / 'jobs' / 'manifest.json')
    for job in manifest['jobs']:
        command = read(job)
        if Path(command['output_directory'], 'collection.json').exists():
            continue
        Path(command['output_directory']).parent.mkdir(parents=True, exist_ok=True)
        log = Path(job).with_suffix('.log')
        with open(log, 'wb') as f:
            code = subprocess.call([binary, job], stdout=f, stderr=subprocess.STDOUT)
        if code != 0:
            raise SystemExit(f'stop: {job} exited {code} (log {log})')


CANDIDATE_KEYS = ('budget', 'search_seed', 'seed_root', 'status', 'cpu_s', 'n', 'wins', 'draws', 'q_bits')


def convert(out_dir):
    """Capture records in episode order (update, then slot), one per source game (the natural attempt)."""
    manifest = read(Path(out_dir) / 'jobs' / 'manifest.json')
    records = []
    ordinal = 0
    for job in manifest['jobs']:
        command = read(job)
        collection = read(Path(command['output_directory']) / 'collection.json')
        for slot, pin in enumerate(collection['ex1']['episodes']):
            sidecar = read(pin['path'])
            if sidecar['slot'] != slot:
                raise SystemExit(f'stop: {pin["path"]} is out of order')
            last = sidecar['attempts'][-1]
            if last['natural'] is not True:
                raise SystemExit(f'stop: {pin["path"]} has no natural attempt')
            placement = last['w5']
            capture = last['capture_0b']
            record = {'schema': CAPTURE_SCHEMA, 'episode': ordinal, 'game': last['episode_id'],
                      'chain': last['options']['run'], 'source_checkpoint_sha256': manifest['source_checkpoint_sha256'],
                      'update': last['options']['update'], 'seed': last['seed'], 'slot': slot,
                      'k': placement['placement_index'], 'reached': capture['reached']}
            if capture['reached']:
                record['policy'] = capture['policy']
                record['candidates'] = [{k: c[k] for k in CANDIDATE_KEYS if k in c} for c in capture['candidates']]
                record['qref'] = capture['qref']
                record['qref_nonnatural'] = capture['qref_nonnatural']
                record['step'] = capture['step']
            records.append(record)
            ordinal += 1
    return records


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest='cmd', required=True)
    p = sub.add_parser('jobs')
    for name in ('--base', '--parent', '--out-dir'):
        p.add_argument(name, required=True)
    p.add_argument('--updates', type=int, required=True)
    p.add_argument('--first-update', type=int, default=0)
    p.add_argument('--chain', default='w5/0b')
    p.add_argument('--budgets', type=int, nargs='+', default=[16000, 64000])
    p.add_argument('--seeds', type=int, nargs='+', default=[0, 1])
    p.add_argument('--rollouts', type=int, default=128)
    p.add_argument('--key-memo-mib', type=int, default=512)
    p = sub.add_parser('run')
    p.add_argument('binary')
    p.add_argument('out_dir')
    p = sub.add_parser('convert')
    p.add_argument('out_dir')
    p.add_argument('--out')
    a = ap.parse_args(argv)
    if a.cmd == 'jobs':
        made = jobs(read(a.base), read(a.parent), a.out_dir, a.updates, a.first_update, a.chain, a.budgets,
                    a.seeds, a.rollouts, a.key_memo_mib)
        print(json.dumps({'jobs': len(made)}))
    elif a.cmd == 'run':
        run(a.binary, a.out_dir)
    else:
        records = convert(a.out_dir)
        out = Path(a.out or Path(a.out_dir) / 'captures' / 'capture-000.jsonl')
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(''.join(json.dumps(r) + '\n' for r in records), encoding='utf-8')
        print(json.dumps({'records': len(records), 'reached': sum(r['reached'] for r in records),
                          'out': out.as_posix(),
                          'sha256': hashlib.sha256(out.read_bytes()).hexdigest()}))


if __name__ == '__main__':
    main()
