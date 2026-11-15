#!/usr/bin/env python3
"""A stand-in for the pinned scorer (`--config CONFIG`; JSON lines on stdin and stdout) for mull_fixedhand's tests.
The checkpoint it is configured with is a JSON file `{"gap": X}`; a request's mulligan-minus-keep gap is X plus the
request's own count k (read from action column 63 of its first row), the value is 0.1 k, and the keep and mulligan rows
are found from action columns 15 and 69 as the real rows are."""
import json
import struct
import sys
from pathlib import Path


def f32(b):
    return struct.unpack('<f', struct.pack('<I', b))[0]


def bits(x):
    return struct.unpack('<I', struct.pack('<f', x))[0]


def main():
    cfg = json.loads(Path(sys.argv[sys.argv.index('--config') + 1]).read_text())
    gap0 = json.loads(Path(cfg['source']['checkpoint']['path']).read_text())['gap']
    print(json.dumps({'schema': 'mtg-kernel-spellbench-scorer-ready/v1', 'feature_contract_digest': 'c' * 64,
                      'feature_encoding_digest': 'e' * 64, 'model_state_sha256': 'd' * 64}), flush=True)
    for line in sys.stdin:
        req = json.loads(line)
        flat = req['tensor']['action_features']
        w = len(flat) // 2
        mull = [r for r in (0, 1) if f32(flat[r * w + 69]) == 1.0][0]
        k = round(f32(flat[63]) * 16)
        logits = [0.0, 0.0]
        logits[mull] = gap0 + k
        print(json.dumps({'schema': 'mtg-kernel-spellbench-scorer-choice/v1', 'logits_bits': [bits(x) for x in logits],
                          'value_bits': bits(0.1 * k)}), flush=True)


if __name__ == '__main__':
    main()
