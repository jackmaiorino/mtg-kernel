"""Fixed-hand table by own count for a London arm: measurement only, no game is played and no state is updated.

The fixed hands are the announcements `mull_calibrate.py run` kept (with their request tensors): the parent's own walk
over the arm's base mixture, every announcement at every count k (mulligans the acting seat has already taken). Each
model state is scored on exactly those requests through the pinned scorer (read-only forward passes, no lean field
anywhere), and for every count the table gives: n, median gap (mulligan logit minus keep logit), p10, p90, spread
(p90 minus p10), the share of hands with gap above 6.9 (a mulligan favored at 99.9% or more) and the share below 0 (a
keep favored), plus the mean value head estimate. The hands are the parent's, bottomed by the parent, so the table says
how a state prices those hands, not what its own play would reach.

  mull_fixedhand.py --announcements DIR/announcements.jsonl --scorer SCORER_BIN --play-import IMPORT.json
        --model NAME=EXPORTED_CHECKPOINT.json [--model ...] --out TABLE.json
        [--limit-per-count N] [--workdir DIR]

An exported checkpoint is the container tool's `export` of a checkpoint (one JSON file per state). `--play-import` is the
play import the sources use; the V7 feature transfer is named in every scorer config, as in the chain recipes. For a
model that is the very state that collected the announcements, `logits_equal_recorded` counts the requests whose
scored logits equal the recorded ones bit for bit (a check that the scorer and the trainer agree).
"""
import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mull_probe as mp  # noqa: E402
import v7_dose  # noqa: E402

GAP_FAVORS_MULLIGAN = 6.9  # logit(0.999)


def scorer_config(play_import, checkpoint):
    return {'schema': 'mtg-kernel-spellbench-scorer-config/v1', 'selection': 'argmax-first-v1',
            'source': {'play_import': {'path': str(play_import), 'sha256': mp.sha256_file(play_import)},
                       'feature_transfer': dict(v7_dose.V7),
                       'checkpoint': {'path': str(checkpoint).replace('\\', '/'), 'sha256': mp.sha256_file(checkpoint)}}}


def select_requests(records, limit_per_count=None):
    """The requests to score: every record, or the first `limit_per_count` of each count in file order."""
    if not limit_per_count:
        return list(records)
    seen, out = {}, []
    for r in records:
        seen[r['k']] = seen.get(r['k'], 0) + 1
        if seen[r['k']] <= limit_per_count:
            out.append(r)
    return out


def score_model(scorer, cfg_path, requests):
    """`[(gap, value, logits_bits)]` for each request, from one scorer process; raises RuntimeError on any protocol
    error. `scorer` is a path or a command list."""
    cmd = mp.command(scorer)
    proc = subprocess.Popen(cmd + ['--config', str(cfg_path)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL)
    try:
        ready = json.loads(proc.stdout.readline() or 'null')
        if not (isinstance(ready, dict) and ready.get('schema') == 'mtg-kernel-spellbench-scorer-ready/v1'):
            raise RuntimeError(f'the scorer did not report ready: {ready!r}')
        out = []
        for n, r in enumerate(requests):
            req = {'schema': 'mtg-kernel-spellbench-scorer-request/v1', 'request_id': f'{r["game"]}:{n}',
                   'game_id': r['game'], 'seat': f'p{r["actor"]}', 'step': n,
                   'feature_contract_digest': ready['feature_contract_digest'],
                   'feature_encoding_digest': ready['feature_encoding_digest'], 'row_candidate_ids': [0, 1],
                   'sample_seed': None, 'tensor': r['tensor']}
            proc.stdin.write(json.dumps(req, separators=(',', ':')).encode() + b'\n')
            proc.stdin.flush()
            choice = json.loads(proc.stdout.readline() or 'null')
            if not (isinstance(choice, dict) and choice.get('schema') == 'mtg-kernel-spellbench-scorer-choice/v1'):
                raise RuntimeError(f'request {n}: the scorer answered {choice!r}')
            logits = [mp.f32(b) for b in choice['logits_bits']]
            out.append((logits[r['mull_row']] - logits[r['keep_row']], mp.f32(choice['value_bits']),
                        choice['logits_bits']))
        return out, ready
    finally:
        for stream in (proc.stdin, proc.stdout):
            try:
                stream.close()
            except OSError:
                pass
        proc.wait(timeout=60)


def table(requests, scored):
    rows = []
    for k in range(mp.COUNTS):
        idx = [i for i, r in enumerate(requests) if r['k'] == k]
        if not idx:
            rows.append({'k': k, 'n': 0})
            continue
        gaps = [scored[i][0] for i in idx]
        p10, p90 = mp.quantile(gaps, 0.1), mp.quantile(gaps, 0.9)
        rows.append({'k': k, 'n': len(idx), 'median': mp.quantile(gaps, 0.5), 'p10': p10, 'p90': p90,
                     'spread': p90 - p10, 'share_gt_6_9': sum(g > GAP_FAVORS_MULLIGAN for g in gaps) / len(gaps),
                     'share_lt_0': sum(g < 0 for g in gaps) / len(gaps),
                     'mean_value': mp.mean(scored[i][1] for i in idx)})
    return rows


def run(args):
    records = [json.loads(line) for line in Path(args.announcements).read_text(encoding='utf-8').splitlines() if line.strip()]
    if not records or any('tensor' not in r for r in records[:1]):
        raise SystemExit('the announcements file has no request tensors (run mull_calibrate.py without --no-tensors)')
    requests = select_requests(records, args.limit_per_count)
    workdir = Path(args.workdir or tempfile.mkdtemp(prefix='fixedhand-'))
    workdir.mkdir(parents=True, exist_ok=True)
    out = {'schema': 'w5-mull-fixedhand/v1', 'announcements': {'file_sha256': mp.sha256_file(args.announcements),
                                                              'requests_scored': len(requests)},
           'gap_favors_mulligan_above': GAP_FAVORS_MULLIGAN, 'models': {}}
    for spec in args.model:
        name, _, path = spec.partition('=')
        if not path:
            raise SystemExit(f'--model wants NAME=PATH, got {spec!r}')
        cfg = workdir / f'{name}.scorer.json'
        cfg.write_text(json.dumps(scorer_config(args.play_import, path), indent=1), encoding='utf-8')
        scored, ready = score_model(args.scorer, cfg, requests)
        out['models'][name] = {'checkpoint_sha256': mp.sha256_file(path),
                               'model_state_sha256': ready.get('model_state_sha256'),
                               'logits_equal_recorded': sum(1 for r, s in zip(requests, scored)
                                                            if s[2] == r.get('logits_bits')),
                               'by_count': table(requests, scored)}
        print(f'{name}: scored {len(requests)} requests', flush=True)
    mp.write_json(args.out, out)
    print(format_text(out))
    return 0


def format_text(out):
    lines = []
    for name, m in out['models'].items():
        lines += [f'{name} (logits equal to the recorded ones on {m["logits_equal_recorded"]} requests)',
                  mp.markdown_table(['k', 'n', 'median', 'p10', 'p90', 'spread', 'share >6.9', 'share <0', 'mean V'],
                                    [[r['k'], r['n'], r.get('median'), r.get('p10'), r.get('p90'), r.get('spread'),
                                      r.get('share_gt_6_9'), r.get('share_lt_0'), r.get('mean_value')]
                                     for r in m['by_count']]), '']
    return '\n'.join(lines)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument('--announcements', required=True)
    ap.add_argument('--scorer', required=True)
    ap.add_argument('--play-import', required=True)
    ap.add_argument('--model', action='append', required=True, metavar='NAME=PATH')
    ap.add_argument('--out', required=True)
    ap.add_argument('--limit-per-count', type=int)
    ap.add_argument('--workdir')
    return run(ap.parse_args(argv))


if __name__ == '__main__':
    sys.exit(main())
