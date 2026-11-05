"""Checkpoint container v2 identity replay: the verified correspondence of design section 6.2 (bookkeeping part 2,
collab LANES/bookkeeping-format-review-20261031, design revision 10).

Two native run trees, a v1 run and a v2 run of the same build, configs, inputs and absolute paths (run one after
the other, the first tree moved aside), correspond when every file of each is matched with its counterpart at the
same relative path. S maps v2 file hashes (and derived digests) to v1 ones; every entry is earned by an equality,
an entry never changes, and no two v2 hashes map to one v1 hash.

- **Substitution** replaces only JSON string values that are exactly a 64-hex digest. Hashes inside other strings
  (paths) are compared literally.
- **Containers.** For a v2 container C and the v1 checkpoint V at its path: export(C) (the Rust exporter, through
  `checkpoint_container_v1.export_v1`), with S applied, must equal V as JSON. Then sha(C) -> sha(V) joins S.
- **Seed pairs** (opponents staged as containers in the v2 run and as v1 checkpoints in the v1 run): the container
  must export to exactly the v1 checkpoint before its pair joins S.
- **Inherited correspondence.** A segment that continues another (case B after A) starts from the earlier pair's
  verified receipt: its S, every entry of which was earned there.
- **Derived digests.** `episodes_sha256` in an iteration's `complete.json` is SHA-256 of the compact JSON of the
  resolved schedule, the `episodes` of that iteration's `collect-command.json`. Each tree's preimage must hash to its
  own recorded value; the v2 schedule with S applied must equal the v1 schedule; then the pair joins S.
- **Every other file** must be equal after S, apart from the allowed classes only, at their own locations: timing
  telemetry (`*_seconds`, worker timings) in receipts (collection, update input, update, complete, completion);
  `record_format` in run.json's config and in update commands; build provenance at run.json's top level. Anything
  else, configuration included, is compared.
- **No unearned digest.** Every 64-hex digest of a v2 file absent from its counterpart must be a key of S (so a
  digest in a stripped field fails rather than passes).
- **States.** Every update records a 64-hex `after_state_sha256`, equal in both trees.

Matching runs to a fixpoint (a pair is matched once S holds what it needs); then every matched pair is checked
again against the final S, so no early match survives a later entry. Anything unmatched fails the receipt.

  checkpoint_correspondence_v1.py V1_TREE V2_TREE [--seed-pair V2_CONTAINER=V1_CHECKPOINT ...]
                                  [--inherit EARLIER_RECEIPT.json] [--out RECEIPT]
"""
import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

import checkpoint_container_v1 as container

DIGEST = re.compile(r'[0-9a-f]{64}')
RECEIPTS = {'collection.json', 'update-input.json', 'update.json', 'complete.json', 'completion.json'}
TIMING_LISTS = {'collection_worker_timings', 'worker_timings'}
BUILD_KEYS = {'git_commit', 'tracked_tree_sha256', 'implementation_sha256'}


class Mismatch(Exception):
    pass


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def files(root: Path) -> dict:
    return {p.relative_to(root).as_posix(): p for p in sorted(root.rglob('*')) if p.is_file()}


def digests(value, out=None):
    """Every string value that is exactly a 64-hex digest."""
    out = set() if out is None else out
    if isinstance(value, dict):
        for v in value.values():
            digests(v, out)
    elif isinstance(value, list):
        for v in value:
            digests(v, out)
    elif isinstance(value, str) and DIGEST.fullmatch(value):
        out.add(value)
    return out


def substitute(value, mapping):
    if isinstance(value, dict):
        return {k: substitute(v, mapping) for k, v in value.items()}
    if isinstance(value, list):
        return [substitute(v, mapping) for v in value]
    if isinstance(value, str) and DIGEST.fullmatch(value):
        return mapping.get(value, value)
    return value


def strip_timing(value):
    if isinstance(value, dict):
        return {k: strip_timing(v) for k, v in value.items()
                if not (k.endswith('_seconds') or k in TIMING_LISTS)}
    if isinstance(value, list):
        return [strip_timing(v) for v in value]
    return value


def allowed(rel: str, doc):
    """The document without the allowed classes, each only at its own location."""
    name = rel.rsplit('/', 1)[-1]
    if name in RECEIPTS:
        doc = strip_timing(doc)
    if name == 'run.json' and isinstance(doc, dict):
        doc = {k: v for k, v in doc.items() if k not in BUILD_KEYS}
        if isinstance(doc.get('config'), dict):
            doc['config'] = {k: v for k, v in doc['config'].items() if k != 'record_format'}
    if name == 'update-command.json' and isinstance(doc, dict):
        doc = {k: v for k, v in doc.items() if k != 'record_format'}
    return doc


def parse(data: bytes):
    try:
        return json.loads(data)
    except ValueError:
        return None


def compact(value) -> bytes:
    return json.dumps(value, separators=(',', ':'), ensure_ascii=False).encode()


def same(a, b) -> bool:
    """Strict JSON equality: types (true is not 1, 1 is not 1.0) and key order count."""
    return compact(a) == compact(b)


class Correspondence:
    def __init__(self):
        self.mapping, self.inverse = {}, {}
        self.parsed = {}

    def doc(self, key, data: bytes):
        if key not in self.parsed:
            self.parsed[key] = parse(data)
        return self.parsed[key]

    def earn(self, v2: str, v1: str):
        if v2 == v1:
            return
        if self.mapping.get(v2, v1) != v1 or self.inverse.get(v1, v2) != v2:
            raise Mismatch(f'conflicting correspondence for {v2} or {v1}')
        self.mapping[v2], self.inverse[v1] = v1, v2

    def pair(self, rel, v1: bytes, v2: bytes, export):
        """The pair's kind if it corresponds under the current S, else None."""
        if container.is_container(v2):
            a, b = self.doc(('v1', rel), v1), self.doc(('export', rel), export(rel))
            if a is None or b is None or not same(substitute(b, self.mapping), a):
                return None
            return 'container'
        a, b = self.doc(('v1', rel), v1), self.doc(('v2', rel), v2)
        if v1 == v2 and (b is None or not digests(b) & set(self.mapping)):
            return 'identical'
        if a is None or b is None:
            return None
        if digests(b) - digests(a) - set(self.mapping):
            return None
        if not same(allowed(rel, substitute(b, self.mapping)), allowed(rel, a)):
            return None
        return 'substituted'


def correspond(v1_root, v2_root, seed_pairs=(), inherit=None) -> dict:
    v1_root, v2_root = Path(v1_root), Path(v2_root)
    a, b = files(v1_root), files(v2_root)
    out = {'schema': 'checkpoint-container-correspondence/v2', 'v1': v1_root.as_posix(), 'v2': v2_root.as_posix(),
           'only_v1': sorted(set(a) - set(b)), 'only_v2': sorted(set(b) - set(a)), 'ok': False}
    v1_bytes = {rel: a[rel].read_bytes() for rel in a}
    v2_bytes = {rel: b[rel].read_bytes() for rel in b}
    exports = {}

    def export(rel):
        if rel not in exports:
            exports[rel] = container.export_v1(b[rel])
        return exports[rel]

    c = Correspondence()
    try:
        if inherit is not None:
            if not (inherit.get('ok') and isinstance(inherit.get('mapping'), dict)):
                raise Mismatch('the inherited receipt is not a passed correspondence')
            for v2, v1 in inherit['mapping'].items():
                c.earn(v2, v1)
        for v2_path, v1_path in seed_pairs:
            v2_data, v1_data = Path(v2_path).read_bytes(), Path(v1_path).read_bytes()
            if not container.is_container(v2_data) or container.export_v1(v2_path) != v1_data:
                raise Mismatch(f'seed pair {v2_path} does not export to {v1_path}')
            c.earn(sha(v2_data), sha(v1_data))
        pending = sorted(set(a) & set(b))
        matched = {}
        derived = 0
        progress = True
        while pending and progress:
            progress = False
            for rel in list(pending):
                v1, v2 = v1_bytes[rel], v2_bytes[rel]
                kind = c.pair(rel, v1, v2, export)
                if kind is None:
                    continue
                c.earn(sha(v2), sha(v1))
                matched[rel] = kind
                pending.remove(rel)
                progress = True
                if rel.endswith('/collect-command.json'):
                    complete = rel.split('/attempt-')[0] + '/complete.json'
                    if complete in v1_bytes and complete in v2_bytes:
                        e1, e2 = parse(v1)['episodes'], parse(v2)['episodes']
                        d1 = parse(v1_bytes[complete])['episodes_sha256']
                        d2 = parse(v2_bytes[complete])['episodes_sha256']
                        if sha(compact(e1)) != d1 or sha(compact(e2)) != d2:
                            raise Mismatch(f'{complete}: the preimage does not reproduce episodes_sha256')
                        if not same(substitute(e2, c.mapping), e1):
                            raise Mismatch(f'{complete}: the v2 schedule does not correspond')
                        c.earn(d2, d1)
                        derived += 1
        # Every match again under the final S.
        stale = [rel for rel, kind in matched.items()
                 if c.pair(rel, v1_bytes[rel], v2_bytes[rel], export) is None]
        states = []
        for rel in matched:
            if rel.endswith('/update/update.json'):
                s1, s2 = parse(v1_bytes[rel]).get('after_state_sha256'), parse(v2_bytes[rel]).get('after_state_sha256')
                states.append(isinstance(s1, str) and DIGEST.fullmatch(s1) is not None and s1 == s2)
    except Mismatch as error:
        out['failure'] = str(error)
        return out
    kinds = {k: list(matched.values()).count(k) for k in ('container', 'identical', 'substituted')}
    out.update(files=len(set(a) & set(b)), matched=len(matched), kinds=kinds, derived_digests=derived,
               substitutions=len(c.mapping), unmatched=sorted(pending)[:50], stale=stale[:50],
               updates=len(states), after_states_equal=bool(states) and all(states), mapping=c.mapping)
    out['ok'] = (not out['only_v1'] and not out['only_v2'] and not pending and not stale
                 and out['after_states_equal'] and kinds['container'] > 0)
    return out


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('v1_tree')
    ap.add_argument('v2_tree')
    ap.add_argument('--seed-pair', action='append', default=[])
    ap.add_argument('--inherit')
    ap.add_argument('--out')
    a = ap.parse_args(argv)
    pairs = [tuple(item.split('=', 1)) for item in a.seed_pair]
    inherit = json.loads(Path(a.inherit).read_text(encoding='utf-8')) if a.inherit else None
    receipt = correspond(a.v1_tree, a.v2_tree, pairs, inherit)
    text = json.dumps(receipt, indent=1)
    if a.out:
        Path(a.out).write_text(text, encoding='utf-8')
    print(text)
    return 0 if receipt['ok'] else 1


if __name__ == '__main__':
    sys.exit(main())
