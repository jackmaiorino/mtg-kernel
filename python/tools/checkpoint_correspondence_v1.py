"""Checkpoint container v2 identity replay: the verified correspondence of design section 6.2 (bookkeeping part 2,
collab LANES/bookkeeping-format-review-20261031, design revision 10).

Two native run trees, a v1 run and a v2 run of the same build, configs, inputs and absolute paths (run one after
the other, the first tree moved aside), correspond when every file of each is matched with its counterpart at the
same relative path:

- **Containers.** For a v2 container C and the v1 checkpoint V at its path: export(C) (the Rust exporter, through
  `checkpoint_container_v1.export_v1`), with the substitution map S applied, must equal V byte for byte. Then
  sha(C) -> sha(V) joins S.
- **Derived digests.** `episodes_sha256` in an iteration's `complete.json` is SHA-256 of the compact JSON of the
  resolved episode schedule, which is the `episodes` list of that iteration's `collect-command.json`. Each tree's
  preimage must hash to its own recorded value; the v2 preimage with S applied must equal the v1 preimage; then the
  v2 digest -> v1 digest joins S. (A preimage that does not reproduce its recorded digest fails the receipt.)
- **Every other file.** F (v2) with S applied must equal G (v1), after removing only the allowed classes: timing
  telemetry (`*_seconds`, worker timings), `record_format` (configs, commands, run.json) and, in run.json, build
  provenance. If F differs from G before substitution, sha(F) -> sha(G) joins S.
- **No unearned hash.** Every 64-hex string of a v2 file absent from its counterpart must be a key of S (so a hash
  in a stripped field, or an unlisted derived digest, fails rather than passes).
- **States.** Every update's `after_state_sha256` is equal (it is never a file hash), and so is each container's
  `state_sha256` (its export equals the v1 checkpoint).

Matching runs to a fixpoint: a pair is matched once S holds every hash it needs, so the order files were published in
never has to be restated. Anything left unmatched fails the receipt.

  checkpoint_correspondence_v1.py V1_TREE V2_TREE [--seed V2SHA=V1SHA ...] [--out RECEIPT.json]
      --seed: converter pin pairs (opponents staged as containers in the v2 run, as v1 checkpoints in the v1 run).
"""
import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

import checkpoint_container_v1 as container

HEX64 = re.compile(rb'(?<![0-9a-f])[0-9a-f]{64}(?![0-9a-f])')
TELEMETRY_KEYS = {'collection_worker_timings', 'worker_timings', 'record_format'}
BUILD_KEYS = {'git_commit', 'tracked_tree_sha256', 'implementation_sha256'}


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def files(root: Path) -> dict:
    return {p.relative_to(root).as_posix(): p for p in sorted(root.rglob('*')) if p.is_file()}


def substitute(data: bytes, mapping: dict) -> bytes:
    return HEX64.sub(lambda m: mapping.get(m.group(0).decode(), m.group(0).decode()).encode(), data)


def strip(value):
    if isinstance(value, dict):
        return {k: strip(v) for k, v in value.items() if not (k.endswith('_seconds') or k in TELEMETRY_KEYS)}
    if isinstance(value, list):
        return [strip(v) for v in value]
    return value


def normalized(rel: str, data: bytes):
    """The comparable form of a file: JSON without the allowed classes, or the raw bytes."""
    if rel.endswith('.json'):
        try:
            doc = json.loads(data)
        except ValueError:
            return data
        doc = strip(doc)
        if rel.endswith('run.json') and isinstance(doc, dict):
            doc = {k: v for k, v in doc.items() if k not in BUILD_KEYS}
        return json.dumps(doc, sort_keys=True).encode()
    return data


def preimage(command: bytes) -> bytes:
    """The resolved episode schedule as the binary hashes it (`identity(&episodes)`: compact serde_json)."""
    episodes = json.loads(command)['episodes']
    return json.dumps(episodes, separators=(',', ':'), ensure_ascii=False).encode()


def correspond(v1_root, v2_root, seed=None) -> dict:
    v1_root, v2_root = Path(v1_root), Path(v2_root)
    a, b = files(v1_root), files(v2_root)
    out = {'schema': 'checkpoint-container-correspondence/v1', 'v1': v1_root.as_posix(), 'v2': v2_root.as_posix(),
           'only_v1': sorted(set(a) - set(b)), 'only_v2': sorted(set(b) - set(a))}
    mapping = dict(seed or {})
    v1_bytes = {rel: a[rel].read_bytes() for rel in a}
    v2_bytes = {rel: b[rel].read_bytes() for rel in b}
    pending = sorted(set(a) & set(b))
    matched, kinds, derived = {}, {'container': 0, 'identical': 0, 'substituted': 0}, 0
    states = []
    progress = True
    while pending and progress:
        progress = False
        for rel in list(pending):
            v1, v2 = v1_bytes[rel], v2_bytes[rel]
            if container.is_container(v2):
                exported = container.export_v1(b[rel])
                if substitute(exported, mapping) != v1:
                    continue
                kind = 'container'
            elif v1 == v2:
                kind = 'identical'
            elif normalized(rel, substitute(v2, mapping)) == normalized(rel, v1):
                kind = 'substituted'
            else:
                continue
            unearned = {h.decode() for h in HEX64.findall(v2)} - {h.decode() for h in HEX64.findall(v1)}
            if kind != 'container' and unearned - set(mapping):
                continue
            if v1 != v2:
                mapping[sha(v2)] = sha(v1)
            matched[rel] = kind
            kinds[kind] += 1
            pending.remove(rel)
            progress = True
            if rel.endswith('collect-command.json'):
                # The iteration's derived digest, once its schedule corresponds.
                complete = rel.split('/attempt-')[0] + '/complete.json'
                if complete in v1_bytes and complete in v2_bytes:
                    p1, p2 = preimage(v1), preimage(v2)
                    d1 = json.loads(v1_bytes[complete])['episodes_sha256']
                    d2 = json.loads(v2_bytes[complete])['episodes_sha256']
                    if sha(p1) != d1 or sha(p2) != d2:
                        out.update(ok=False, failure=f'{complete}: the preimage does not reproduce episodes_sha256')
                        return out
                    if substitute(p2, mapping) != p1:
                        out.update(ok=False, failure=f'{complete}: the v2 schedule does not correspond')
                        return out
                    if d1 != d2:
                        mapping[d2] = d1
                        derived += 1
            if rel.endswith('update/update.json'):
                d1, d2 = json.loads(v1), json.loads(v2)
                states.append(d1.get('after_state_sha256') == d2.get('after_state_sha256'))
    out.update(files=len(set(a) & set(b)), matched=len(matched), kinds=kinds, derived_digests=derived,
               substitutions=len(mapping), unmatched=sorted(pending)[:50],
               updates=len(states), after_states_equal=all(states))
    out['ok'] = (not out['only_v1'] and not out['only_v2'] and not pending and all(states) and bool(states)
                 and kinds['container'] > 0)
    return out


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('v1_tree')
    ap.add_argument('v2_tree')
    ap.add_argument('--seed', action='append', default=[])
    ap.add_argument('--out')
    a = ap.parse_args(argv)
    seed = dict(item.split('=', 1) for item in a.seed)
    receipt = correspond(a.v1_tree, a.v2_tree, seed)
    text = json.dumps(receipt, indent=1)
    if a.out:
        Path(a.out).write_text(text, encoding='utf-8')
    print(text)
    return 0 if receipt['ok'] else 1


if __name__ == '__main__':
    sys.exit(main())
