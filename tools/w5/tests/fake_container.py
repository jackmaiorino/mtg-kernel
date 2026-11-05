#!/usr/bin/env python3
"""A stand-in for `checkpoint_container_v1 weights-snapshot --input CK --sha256 S --output OUT` in the W5 chain
controller tests: a small weights file and the receipt retention.py reads (the fake trainer's after-state hash is
its checkpoint's hash)."""
import hashlib
import json
import sys
from pathlib import Path


def main(argv):
    assert argv[0] == 'weights-snapshot'
    args = dict(zip(argv[1::2], argv[2::2]))
    src, out = Path(args['--input']), Path(args['--output'])
    data = src.read_bytes()
    if hashlib.sha256(data).hexdigest() != args['--sha256']:
        print('input differs from its pin', file=sys.stderr)
        return 1
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(b'weights:' + hashlib.sha256(data).digest())
    digest = hashlib.sha256(out.read_bytes()).hexdigest()
    print(json.dumps({'output': {'path': out.as_posix(), 'sha256': digest}, 'snapshot_bytes': out.stat().st_size,
                      'cut_from_state_sha256': args['--sha256'], 'parameters_sha256': digest}))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
