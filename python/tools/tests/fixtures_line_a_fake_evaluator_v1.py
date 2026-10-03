"""Offline stand-in for public_feature_evaluation_v1: same output files, no game engine.

It writes start.json, match-000000.json and completion.json in a fresh output
directory, with match bytes derived only from the request, so reruns and
worker counts reproduce them. FAKE_EVALUATOR_FAIL_SEED makes the matching
seed fail with failure.json; FAKE_EVALUATOR_SALT perturbs match bytes to model
a nondeterministic build. There are no outcomes in these files.
"""
import hashlib
import json
import os
from pathlib import Path
import sys

GIT_HEAD = 'f' * 40


def main():
    request = json.loads(Path(sys.argv[1]).read_bytes())
    folder = Path(request['output_directory'])
    folder.mkdir()
    (folder / 'start.json').write_text(json.dumps(dict(schema='public-input-evaluation-start/v1', command=request,
                                                       models=['fake', 'fake'], git_head=GIT_HEAD)))
    match = request['matches'][0]
    if str(match['config']['seed']) == os.environ.get('FAKE_EVALUATOR_FAIL_SEED'):
        (folder / 'failure.json').write_text(json.dumps(dict(error='forced failure')))
        raise SystemExit(1)
    body = json.dumps(dict(match=match, sources=request['sources'], salt=os.environ.get('FAKE_EVALUATOR_SALT', '')),
                      sort_keys=True).encode()
    (folder / 'match-000000.json').write_bytes(body)
    (folder / 'completion.json').write_text(json.dumps(dict(
        schema='public-input-evaluation-completion/v1', matches=1, natural_games=2, decisions=1,
        match_sha256=[hashlib.sha256(body).hexdigest()])))


if __name__ == '__main__':
    main()
