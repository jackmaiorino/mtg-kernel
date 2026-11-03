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
  capture0b.py convert-vl DIR --budget B --host-class C --binary-sha256 H [--out FILE]
                                          W5 Stage 2a value-leaf records (`w5-2a-vl-capture/v1`, read by
                                          value_leaf_2a.py) from jobs made with --vl-horizons/--vl-sims/--vl-seeds;
                                          the comparator is the 0b candidate at budget B (same binary, host and
                                          key memo as the value-leaf searches, ruling 2)
  capture0b.py parity DIR_A DIR_B         0b parity of a run without (A) and with (B) value-leaf searches over the
                                          same jobs: per shared episode, equal step, policy, candidates (CPU aside)
                                          and every A Q_ref action's counts in B

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
VL_SCHEMA = 'w5-2a-vl-capture/v1'
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
         rollouts=128, key_memo_mib=512, value_leaf=None):
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
        if value_leaf:
            command['ex1']['w5']['capture_0b']['value_leaf'] = {
                'horizons': list(value_leaf['horizons']), 'sims': list(value_leaf['sims']),
                'search_seeds': list(value_leaf['search_seeds'])}
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


def sidecars(out_dir):
    """(ordinal, natural attempt) per episode in episode order, as `convert` walks them."""
    manifest = read(Path(out_dir) / 'jobs' / 'manifest.json')
    ordinal = 0
    for job in manifest['jobs']:
        command = read(job)
        collection = read(Path(command['output_directory']) / 'collection.json')
        for slot, pin in enumerate(collection['ex1']['episodes']):
            sidecar = read(pin['path'])
            if sidecar['slot'] != slot or sidecar['attempts'][-1]['natural'] is not True:
                raise SystemExit(f'stop: {pin["path"]} is out of order or has no natural attempt')
            yield ordinal, sidecar['attempts'][-1]
            ordinal += 1


def convert_vl(out_dir, budget, host_class, binary_sha256):
    """W5 Stage 2a value-leaf records for every reached episode, aligned with `convert`'s episode ordinals."""
    manifest = read(Path(out_dir) / 'jobs' / 'manifest.json')
    limit = int(manifest['key_memo_mib']) << 20
    if limit <= 0:
        raise SystemExit('stop: the value-leaf read needs the key memo on')
    records = []
    for ordinal, last in sidecars(out_dir):
        capture = last['capture_0b']
        if not capture['reached']:
            continue
        if 'value_leaf' not in capture:
            raise SystemExit(f'stop: episode {ordinal} has no value-leaf searches (jobs made without --vl-*)')
        configs = []
        for row in capture['value_leaf']:
            c = {k: row[k] for k in ('horizon', 'n_sims', 'search_seed', 'seed_root', 'status', 'cpu_s',
                                     'primary_sha256')}
            if row['status'] == 'ok':
                c.update({k: row[k] for k in ('n', 'value_units', 'draws')})
            configs.append(c)
        comparator = [{'budget': c['budget'], 'search_seed': c['search_seed'], 'cpu_s': c['cpu_s'],
                       'key_memo': 'on', 'key_memo_limit_bytes': limit}
                      for c in capture['candidates'] if c['budget'] == budget]
        extra = capture['qref_extra']
        records.append({'schema': VL_SCHEMA, 'episode': ordinal, 'game': last['episode_id'],
                        'host_class': host_class, 'binary_sha256': binary_sha256, 'key_memo': 'on',
                        'key_memo_limit_bytes': limit, 'configs': configs, 'comparator': comparator,
                        'qref_extra': {k: extra[k] for k in ('rollouts', 'seed_root', 'actions', 'n', 'wins',
                                                             'draws')}})
    return records


def parity(dir_a, dir_b):
    """0b parity of two capture runs over the same jobs: DIR_A without value-leaf searches, DIR_B with them. DIR_A's
    jobs must be DIR_B's first jobs (same episodes), every DIR_A episode must be in DIR_B, and each must have the
    same game, step, policy, candidates (CPU aside) and Q_ref (actions, counts, denominators, rollouts)."""
    ja = [read(j) for j in read(Path(dir_a) / 'jobs' / 'manifest.json')['jobs']]
    jb = [read(j) for j in read(Path(dir_b) / 'jobs' / 'manifest.json')['jobs']]
    same_jobs = bool(ja) and len(ja) <= len(jb) and all(
        x['episodes'] == y['episodes'] and x['ex1']['run'] == y['ex1']['run']
        and x['ex1']['update'] == y['ex1']['update'] for x, y in zip(ja, jb))
    a = {r['episode']: r for r in convert(dir_a)}
    b = {r['episode']: r for r in convert(dir_b)}
    missing = sorted(set(a) - set(b))
    differ = []

    def strip(cs):
        return [{k: v for k, v in c.items() if k != 'cpu_s'} for c in cs]
    for e in sorted(set(a) & set(b)):
        x, y = a[e], b[e]
        same = (x['game'], x['reached']) == (y['game'], y['reached'])
        if same and x['reached']:
            same = (x['step'] == y['step'] and x['policy'] == y['policy']
                    and strip(x['candidates']) == strip(y['candidates']) and x['qref'] == y['qref'])
        if not same:
            differ.append(e)
    return {'jobs': len(ja), 'same_jobs': same_jobs, 'episodes': len(a), 'missing': missing,
            'reached': sum(r['reached'] for r in a.values()), 'differ': differ,
            'equal': same_jobs and bool(a) and not missing and not differ}


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
    p.add_argument('--vl-horizons', nargs='+', default=None, help='W5 2a: value-leaf horizons, e.g. h0 t0')
    p.add_argument('--vl-sims', type=int, nargs='+', default=None, help='W5 2a: simulation counts, e.g. 128 512')
    p.add_argument('--vl-seeds', type=int, nargs='+', default=[0, 1])
    p = sub.add_parser('run')
    p.add_argument('binary')
    p.add_argument('out_dir')
    p = sub.add_parser('convert')
    p.add_argument('out_dir')
    p.add_argument('--out')
    p = sub.add_parser('convert-vl')
    p.add_argument('out_dir')
    p.add_argument('--budget', type=int, required=True)
    p.add_argument('--host-class', required=True)
    p.add_argument('--binary-sha256', required=True)
    p.add_argument('--out')
    p = sub.add_parser('parity')
    p.add_argument('dir_a')
    p.add_argument('dir_b')
    a = ap.parse_args(argv)
    if a.cmd == 'jobs':
        if (a.vl_horizons is None) != (a.vl_sims is None):
            raise SystemExit('stop: give both --vl-horizons and --vl-sims, or neither')
        value_leaf = ({'horizons': a.vl_horizons, 'sims': a.vl_sims, 'search_seeds': a.vl_seeds}
                      if a.vl_horizons else None)
        made = jobs(read(a.base), read(a.parent), a.out_dir, a.updates, a.first_update, a.chain, a.budgets,
                    a.seeds, a.rollouts, a.key_memo_mib, value_leaf)
        print(json.dumps({'jobs': len(made)}))
    elif a.cmd == 'run':
        run(a.binary, a.out_dir)
    elif a.cmd == 'parity':
        print(json.dumps(parity(a.dir_a, a.dir_b)))
    elif a.cmd == 'convert-vl':
        records = convert_vl(a.out_dir, a.budget, a.host_class, a.binary_sha256)
        out = Path(a.out or Path(a.out_dir) / 'vl' / 'vl-000.jsonl')
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(''.join(json.dumps(r) + '\n' for r in records), encoding='utf-8')
        print(json.dumps({'records': len(records), 'out': out.as_posix(),
                          'sha256': hashlib.sha256(out.read_bytes()).hexdigest()}))
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
