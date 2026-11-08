"""SD1 capture driver (collab LANES/search-diagnostic-20261108/DESIGN.md v4 sections 3.1, 3.2 and 6 E2).

b512 episodes under C's recipe and schedule in a chain-seed namespace (`sd1/panel`, confirmation `sd1/confirm`),
W5 section 3's placement (one placed W1-eligible learner decision per seat-episode), and at each placed decision
the engine's SD1 capture (`W5CaptureOptionsV1::sd1`): the declared arms under their world lifecycles and the split
true-state references. One `collect` command per update, one collection worker per process (so each search's
process-CPU delta is that search's CPU); the learner plays its own draw throughout.

Two passes over the same jobs:
  1. panel:  arms empty, no references. Records which episodes reach their placed decision, the step, the policy
             and the actor-visible features. `panel` hashes the first N reached states (one per source game) before
             any arm runs.
  2. arms:   the declared arms and references. `convert` checks every panel state's game, step and policy against
             the panel pass before writing records.

  capture.py jobs  --base BASE.json --parent DOSE.json --out-dir DIR --updates N [--first-update U]
                   [--chain sd1/panel] --arms ARMS.json [--panel-only] [--key-memo-mib 512]
  capture.py ladder BINARY JOB --out-dir DIR --steps 1 8 12 16
                   throughput qualification (COMPUTE-POLICY items 2 and 4): k identical copies of one representative
                   job run k at a time per step; completed reached states per wall second per step; writes
                   DIR/ladder.json with the selected worker count (most completed work per second, ties to fewer)
  capture.py run   BINARY DIR --workers W --throughput LADDER.json [--log-every S]
                   runs every job whose collection.json is missing, W processes at a time, in update order; refuses
                   (arms pass) without a ladder for the same binary whose selected worker count is at least W
  capture.py panel DIR --size 900 --subset 150 [--out PANEL.json]
  capture.py convert DIR --panel PANEL.json [--out FILE]

ARMS.json: {"version": "sd1-capture-v1", "half_rollouts": 64, "coverage_max": 16,
            "arms": [{"label": "D64", "world": "fresh", "budget": 64000, "search_seeds": [0, 1]},
                     {"label": "VL8-h0", "world": "fresh", "search_seeds": [0, 1],
                      "value_leaf": {"horizon": "h0", "sims": 512}}, ...]}
"""
import argparse
import hashlib
import json
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / 'w5'))
sys.path.insert(0, str(HERE.parent / 'ex1'))
import capture0b  # noqa: E402
import qual  # noqa: E402

RECORD_SCHEMA = 'sd1-record/v1'
PANEL_SCHEMA = 'sd1-panel/v1'
VERSION = 'sd1-capture-v1'
ARM_KEYS = ('arm', 'world', 'search_seed', 'seed_root', 'status', 'cpu_s', 'visits', 'n', 'wins', 'draws',
            'value_units', 'perm_live', 'frozen_choice', 'pick_v2', 'fallback')
SEARCH_KEYS = ('budget', 'transitions', 'attempted', 'completed', 'nonnatural', 'cap_reached', 'incomplete',
               'faults', 'sampler', 'world_receipt', 'failed', 'value_leaf')


def read(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))


def write(path, doc):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(doc, indent=1) + '\n', encoding='utf-8', newline='\n')


def sha256_file(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def jobs(base, dose, out_dir, updates, arms, first_update=0, chain='sd1/panel', key_memo_mib=512,
         panel_only=False):
    """One collect command per update (EX1's qualification construction, as capture0b.py builds it)."""
    if arms.get('version') != VERSION:
        raise SystemExit(f'stop: ARMS version must be {VERSION}')
    sd1 = {'version': VERSION, 'arms': [] if panel_only else arms['arms'],
           'half_rollouts': 0 if panel_only else int(arms['half_rollouts']),
           'coverage_max': int(arms['coverage_max'])}
    budget = max([a.get('budget', 0) for a in arms['arms']] + [1])
    cfg = qual.build_config(base, dose, Path(out_dir) / 'unused', chain, chain, first_update, updates, 1)
    source = cfg['initial_source']
    out = []
    for i, item in enumerate(cfg['iterations']):
        update = first_update + i
        command = {'mode': 'collect', 'source': source,
                   'episodes': [capture0b.resolve(cfg, scheduled, source) for scheduled in item['episodes']],
                   'max_non_natural_episode_fraction': cfg.get('max_non_natural_episode_fraction', 0.0),
                   'ex1': {'run': chain, 'update': update, 'budget': budget,
                           'w5': {'recipe': capture0b.RECIPE, 'key_memo_mib': key_memo_mib,
                                  'capture_0b': {'budgets': [], 'search_seeds': [], 'rollouts': 0,
                                                 'sd1': sd1}}},
                   'output_directory': (Path(out_dir) / 'runs' / f'u{update:06d}').as_posix()}
        if not command['max_non_natural_episode_fraction']:
            del command['max_non_natural_episode_fraction']
        for key in ('collection_sampler', 'training_math'):
            if key in cfg:
                command[key] = cfg[key]
        path = Path(out_dir) / 'jobs' / f'u{update:06d}.json'
        write(path, command)
        out.append(path.as_posix())
    hashes = sorted(sha256_file(p) for p in out)
    write(Path(out_dir) / 'jobs' / 'manifest.json',
          {'schema': 'sd1-capture-jobs/v1', 'chain': chain, 'first_update': first_update, 'updates': updates,
           'source_checkpoint_sha256': source['checkpoint']['sha256'], 'sd1': sd1, 'key_memo_mib': key_memo_mib,
           'panel_only': panel_only, 'jobs': out,
           'job_set_sha256': hashlib.sha256(''.join(hashes).encode()).hexdigest()})
    return out


JOB_STAMP = '.job.sha256'


def stamp_path(out):
    """The job stamp sits beside the output directory (the engine creates the directory itself and refuses an
    existing one)."""
    out = Path(out)
    return out.with_name(out.name + JOB_STAMP)


def finished(job):
    """Whether `job` already ran to completion with exactly these command bytes. Each output directory has a
    stamp beside it holding the SHA-256 of the job file that produced it (written before the process starts); a
    finished or partial output from another command (changed arms, worlds, budgets or panel-only setting) is
    refused, never reused."""
    out = Path(read(job)['output_directory'])
    stamp = stamp_path(out)
    if not (out / 'collection.json').exists():
        if out.exists() and (not stamp.exists() or stamp.read_text().strip() != sha256_file(job)):
            raise SystemExit(f'stop: {out} holds a partial run of another command')
        if out.exists():
            raise SystemExit(f'stop: {out} holds a partial run; move it aside before rerunning')
        return False
    if not stamp.exists() or stamp.read_text().strip() != sha256_file(job):
        raise SystemExit(f'stop: {out} was produced by another command than {job}')
    return True


def run(binary, out_dir, workers, log_every=300.0):
    """Every job not yet finished with its own command bytes, `workers` processes at a time, started in update
    order. A failed job stops new starts; running ones finish."""
    manifest = read(Path(out_dir) / 'jobs' / 'manifest.json')
    pending = [j for j in manifest['jobs'] if not finished(j)]
    running, failed, done = {}, [], 0
    last = 0.0
    status = Path(out_dir) / 'status.json'
    started = time.time()
    while pending or running:
        while pending and len(running) < workers and not failed:
            job = pending.pop(0)
            out = Path(read(job)['output_directory'])
            out.parent.mkdir(parents=True, exist_ok=True)
            stamp_path(out).write_text(sha256_file(job) + '\n', encoding='utf-8')
            log = open(Path(job).with_suffix('.log'), 'wb')
            running[job] = (subprocess.Popen([binary, job], stdout=log, stderr=subprocess.STDOUT), log)
        for job, (proc, log) in list(running.items()):
            code = proc.poll()
            if code is None:
                continue
            log.close()
            del running[job]
            if code != 0:
                failed.append({'job': job, 'exit': code})
            else:
                done += 1
        if failed and not running:
            break
        now = time.time()
        if now - last >= log_every or not (pending or running):
            last = now
            write(status, {'schema': 'sd1-run-status/v1', 'done_this_run': done, 'running': len(running),
                           'pending': len(pending), 'failed': failed, 'workers': workers,
                           'elapsed_s': round(now - started, 1), 'updated_unix': round(now, 1)})
        time.sleep(2.0)
    write(status, {'schema': 'sd1-run-status/v1', 'done_this_run': done, 'running': 0, 'pending': len(pending),
                   'failed': failed, 'workers': workers, 'elapsed_s': round(time.time() - started, 1),
                   'updated_unix': round(time.time(), 1), 'finished': True})
    if failed:
        raise SystemExit(f'stop: {failed}')


LADDER_SCHEMA = 'sd1-throughput-ladder/v1'


def reached_states(run_dir):
    collection = read(Path(run_dir) / 'collection.json')
    count = 0
    for pin in collection['ex1']['episodes']:
        last = read(pin['path'])['attempts'][-1]
        count += bool(last['capture_0b']['reached'])
    return count


def ladder(binary, job, out_dir, steps):
    """k concurrent copies of one job per step (identical inputs, separate outputs); the identity of every copy's
    capture (CPU and wall aside) is checked, so parallel runs reproduce the serial bytes."""
    command = read(job)
    rows = []
    reference = None
    for k in steps:
        procs = []
        started = time.time()
        for i in range(k):
            out = Path(out_dir) / f'k{k}' / f'c{i}'
            out.parent.mkdir(parents=True, exist_ok=True)   # the engine creates `out` itself
            cmd = dict(command, output_directory=out.as_posix())
            path = Path(out_dir) / 'jobs' / f'k{k}-c{i}.json'
            write(path, cmd)
            log = open(path.with_suffix('.log'), 'wb')
            procs.append((subprocess.Popen([binary, str(path)], stdout=log, stderr=subprocess.STDOUT), log, out))
        codes = []
        for proc, log, _ in procs:
            codes.append(proc.wait())
            log.close()
        wall = time.time() - started
        if any(codes):
            raise SystemExit(f'stop: ladder step {k} exit codes {codes}')
        states = 0
        for _, _, out in procs:
            states += reached_states(out)
            digest = capture_digest(out)
            reference = reference or digest
            if digest != reference:
                raise SystemExit(f'stop: ladder step {k} copy {out} differs from the first copy')
        rows.append({'workers': k, 'wall_s': round(wall, 3), 'reached_states': states,
                     'states_per_s': states / wall})
    best = max(rows, key=lambda r: (round(r['states_per_s'], 9), -r['workers']))
    doc = {'schema': LADDER_SCHEMA, 'binary_sha256': sha256_file(binary), 'job_sha256': sha256_file(job),
           'steps': rows, 'selected_workers': best['workers'], 'serial_states_per_s': rows[0]['states_per_s'],
           'speedup_vs_serial': best['states_per_s'] / rows[0]['states_per_s'], 'capture_digest': reference}
    write(Path(out_dir) / 'ladder.json', doc)
    return doc


def capture_digest(run_dir):
    """SHA-256 over every episode's capture with CPU and wall times removed."""
    collection = read(Path(run_dir) / 'collection.json')
    h = hashlib.sha256()
    for pin in collection['ex1']['episodes']:
        c = read(pin['path'])['attempts'][-1]['capture_0b']
        for a in c.get('arms', []):
            a.pop('cpu_s', None)
            a.get('search', {}).pop('wall', None)
        h.update(json.dumps(c, sort_keys=True).encode())
    return h.hexdigest()


def check_throughput(binary, workers, ladder_path):
    if not ladder_path:
        raise SystemExit('stop: a substantial run needs --throughput (capture.py ladder, COMPUTE-POLICY item 5)')
    doc = read(ladder_path)
    if doc.get('schema') != LADDER_SCHEMA or doc.get('binary_sha256') != sha256_file(binary):
        raise SystemExit('stop: the throughput ladder is for another binary or not a ladder')
    if workers > doc['selected_workers']:
        raise SystemExit(f'stop: {workers} workers exceed the qualified {doc["selected_workers"]}')
    return doc


def records(out_dir):
    """Per episode in episode order (update, then slot): the natural attempt's SD1 capture as a record."""
    manifest = read(Path(out_dir) / 'jobs' / 'manifest.json')
    ordinal = 0
    for job in manifest['jobs']:
        command = read(job)
        if not finished(job):
            raise SystemExit(f'stop: {job} has not finished')
        collection = read(Path(command['output_directory']) / 'collection.json')
        for slot, pin in enumerate(collection['ex1']['episodes']):
            sidecar = read(pin['path'])
            last = sidecar['attempts'][-1]
            if sidecar['slot'] != slot or last['natural'] is not True:
                raise SystemExit(f'stop: {pin["path"]} is out of order or has no natural attempt')
            capture = last['capture_0b']
            record = {'schema': RECORD_SCHEMA, 'episode': ordinal, 'game': last['episode_id'],
                      'chain': last['options']['run'], 'source_checkpoint_sha256': manifest['source_checkpoint_sha256'],
                      'update': last['options']['update'], 'seed': last['seed'], 'slot': slot,
                      'k': last['w5']['placement_index'], 'learner_deck': last.get('learner_deck'),
                      'reached': capture['reached']}
            if capture['reached']:
                if capture.get('schema') != VERSION:
                    raise SystemExit(f'stop: episode {ordinal} is not an SD1 capture')
                record.update({k: capture[k] for k in ('step', 'width', 'policy', 'features')})
                record['arms'] = []
                for a in capture['arms']:
                    row = {k: a[k] for k in ARM_KEYS if k in a}
                    row['search'] = {k: a['search'][k] for k in SEARCH_KEYS if k in a['search']}
                    record['arms'].append(row)
                if 'references' in capture:
                    record['references'] = capture['references']
            yield record
            ordinal += 1


def panel(out_dir, size, subset):
    """The first `size` reached states in episode order, one per source game; the first `subset` are the declared
    subset. The hash covers (episode, game, step, policy logit bits) of every panel state."""
    chosen, games = [], set()
    for r in records(out_dir):
        if not r['reached']:
            continue
        if r['game'] in games:
            raise SystemExit(f'stop: game {r["game"]} gives two states')
        games.add(r['game'])
        chosen.append([r['episode'], r['game'], r['step'], r['policy']['logit_bits']])
        if len(chosen) == size:
            break
    if len(chosen) < size:
        raise SystemExit(f'stop: only {len(chosen)} reached states; the panel needs {size} (extend the updates)')
    manifest = read(Path(out_dir) / 'jobs' / 'manifest.json')
    body = json.dumps(chosen, separators=(',', ':'))
    return {'schema': PANEL_SCHEMA, 'chain': manifest['chain'], 'size': size, 'subset': subset,
            'source_checkpoint_sha256': manifest['source_checkpoint_sha256'],
            'panel_job_set_sha256': manifest['job_set_sha256'], 'last_episode': chosen[-1][0],
            'panel_sha256': hashlib.sha256(body.encode()).hexdigest(),
            'subset_sha256': hashlib.sha256(json.dumps(chosen[:subset], separators=(',', ':')).encode()).hexdigest(),
            'states': chosen}


def convert(out_dir, panel_doc):
    """Records of the panel states from the arms pass, each checked against the panel pass (game, step, policy)."""
    want = {s[0]: s for s in panel_doc['states']}
    out = []
    for r in records(out_dir):
        if r['episode'] > panel_doc['last_episode']:
            break
        s = want.get(r['episode'])
        if s is None:
            if r['reached']:
                raise SystemExit(f'stop: episode {r["episode"]} reached here but not in the panel pass')
            continue
        if not r['reached'] or [r['episode'], r['game'], r['step'], r['policy']['logit_bits']] != s:
            raise SystemExit(f'stop: episode {r["episode"]} differs from the panel pass')
        out.append(r)
    if len(out) != panel_doc['size']:
        raise SystemExit(f'stop: {len(out)} panel states converted, {panel_doc["size"]} expected')
    return out


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest='cmd', required=True)
    p = sub.add_parser('jobs')
    for name in ('--base', '--parent', '--out-dir', '--arms'):
        p.add_argument(name, required=True)
    p.add_argument('--updates', type=int, required=True)
    p.add_argument('--first-update', type=int, default=0)
    p.add_argument('--chain', default='sd1/panel')
    p.add_argument('--key-memo-mib', type=int, default=512)
    p.add_argument('--panel-only', action='store_true')
    p = sub.add_parser('run')
    p.add_argument('binary')
    p.add_argument('out_dir')
    p.add_argument('--workers', type=int, required=True)
    p.add_argument('--throughput')
    p.add_argument('--log-every', type=float, default=300.0)
    p = sub.add_parser('ladder')
    p.add_argument('binary')
    p.add_argument('job')
    p.add_argument('--out-dir', required=True)
    p.add_argument('--steps', type=int, nargs='+', required=True)
    p = sub.add_parser('panel')
    p.add_argument('out_dir')
    p.add_argument('--size', type=int, default=900)
    p.add_argument('--subset', type=int, default=150)
    p.add_argument('--out')
    p = sub.add_parser('convert')
    p.add_argument('out_dir')
    p.add_argument('--panel', required=True)
    p.add_argument('--out')
    a = ap.parse_args(argv)
    if a.cmd == 'jobs':
        made = jobs(read(a.base), read(a.parent), a.out_dir, a.updates, read(a.arms), a.first_update, a.chain,
                    a.key_memo_mib, a.panel_only)
        print(json.dumps({'jobs': len(made), 'job_set_sha256':
                          read(Path(a.out_dir) / 'jobs' / 'manifest.json')['job_set_sha256']}))
    elif a.cmd == 'run':
        # The panel-only pass plays episodes and records states only (no search, no references): a small run.
        if not read(Path(a.out_dir) / 'jobs' / 'manifest.json').get('panel_only'):
            check_throughput(a.binary, a.workers, a.throughput)
        run(a.binary, a.out_dir, a.workers, a.log_every)
    elif a.cmd == 'ladder':
        print(json.dumps(ladder(a.binary, a.job, a.out_dir, a.steps)))
    elif a.cmd == 'panel':
        doc = panel(a.out_dir, a.size, a.subset)
        out = Path(a.out or Path(a.out_dir) / 'panel.json')
        write(out, doc)
        print(json.dumps({k: doc[k] for k in ('size', 'last_episode', 'panel_sha256', 'subset_sha256')}))
    else:
        recs = convert(a.out_dir, read(a.panel))
        out = Path(a.out or Path(a.out_dir) / 'records' / 'records-000.jsonl')
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(''.join(json.dumps(r) + '\n' for r in recs), encoding='utf-8', newline='\n')
        print(json.dumps({'records': len(recs), 'out': out.as_posix(), 'sha256': sha256_file(out)}))


if __name__ == '__main__':
    main()
