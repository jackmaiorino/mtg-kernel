"""FlatV7 inputs for arm M (TOOLING-T5 item 5): a parent dose record and a base's named-opponent sources that declare
the exact V4-to-FlatV7 feature transfer, which the collector requires on every seat of a London game.

  v7_dose.py dose --dose PARENT-DOSE.json --out V7-DOSE.json [--check]
  v7_dose.py base --base BASE.json --out V7-BASE.json [--check]

`dose` rewrites `source.feature_transfer` of a dose record (the parent source a chain's segments start from; the
chain tools build `initial_source` from it). `base` rewrites `opponents[*].source.feature_transfer` of a base (the
named fixed opponents of chain A's mix: d6107922 and A48). The base's own `initial_source` is not touched: every
segment config replaces it with the dose record's source (xact.build_config), and `base` leaves everything but the
opponents' transfers byte-for-byte as it was.

The transfer is the one the probe and the London reader use (probe.py, london_reader.py): both V7 digests, the
checkpoint and play-import pins unchanged (the transfer is exact: parameters and Adam state are not converted).
Nothing but the two digests changes: key order, every other field and every pin are kept. The output is
`json.dumps(indent=1)` in the document's own key order plus a newline (LF); the expected sha256 is written beside it
as `OUT.sha256` (`<hex>  <file name>`).

A source is converted only from the transfer chain A's sources carry (the V4 pair, c4af415a / 271c0e5a) or one that
already is the V7 pair (the conversion is then a no-op, so a rerun gives the same bytes). Refused, nothing written:
any other pair (a different contract, or one digest of a pair: "FlatV7 transfer requires both V7 feature digests"),
a transfer with other keys, a source with no transfer, a dose without a source, a base with no named opponents.
--check prints what would be written and writes nothing. The conversion is independent of london_base.py (which
changes episodes only), so the two commute.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path

V7 = {'expected_feature_contract_digest': '1f894f0dcaaeb4e5ac0233e3f39d866c0dddca494f1afc8152107bded0551a4d',
      'expected_feature_encoding_digest': '4c592d136a74240f40b12c638203ab1758ed3cd63748ab7cd2788a60a78850a3'}
V4 = {'expected_feature_contract_digest': 'c4af415a3b0cf1e9c9960dbe2bc2d134c63e9f08206a9a364e113121fea5538b',
      'expected_feature_encoding_digest': '271c0e5a0fdce75663c897e89a9d7280ab1a3bbb6679bd10ecb5f524991952de'}
TRANSFER_KEYS = frozenset(V7)


class DoseError(ValueError):
    """A refusal: the input is not one this tool converts."""


def convert_source(source, where, accept_from=(V4,)):
    """A copy of `source` whose feature_transfer is the V7 pair. Only the transfer's two values change."""
    if not isinstance(source, dict):
        raise DoseError(f'{where}: not a model source')
    transfer = source.get('feature_transfer')
    if not isinstance(transfer, dict):
        raise DoseError(f'{where}: the source declares no feature_transfer')
    if set(transfer) != TRANSFER_KEYS:
        raise DoseError(f'{where}: feature_transfer has keys {sorted(transfer)}, expected {sorted(TRANSFER_KEYS)}')
    if transfer != V7 and transfer not in accept_from:
        raise DoseError(f'{where}: the source already carries a different transfer '
                        f'({str(transfer["expected_feature_contract_digest"])[:8]} / '
                        f'{str(transfer["expected_feature_encoding_digest"])[:8]}); only V4 or V7 is converted')
    out = copy.deepcopy(source)
    out['feature_transfer'].update(V7)  # the two values only: the input's own key order survives
    return out


def convert_dose(record, accept_from=(V4,)):
    """A copy of a dose record (`{origin, source}`) with the parent source's transfer converted."""
    if not isinstance(record, dict) or 'source' not in record:
        raise DoseError('not a dose record (no source)')
    out = copy.deepcopy(record)
    out['source'] = convert_source(record['source'], 'dose source', accept_from)
    return out


def convert_base(base, accept_from=(V4,)):
    """A copy of a base with every named opponent's source transfer converted; (base, ids converted)."""
    if not isinstance(base, dict):
        raise DoseError('not a base config')
    opponents = base.get('opponents')
    if not isinstance(opponents, list) or not opponents:
        raise DoseError('the base declares no named opponents')
    out = copy.deepcopy(base)
    ids = []
    for i, item in enumerate(out['opponents']):
        if not isinstance(item, dict) or 'source' not in item:
            raise DoseError(f'opponents[{i}] has no source')
        item['source'] = convert_source(item['source'], f'opponents[{i}] ({item.get("id")!r})', accept_from)
        ids.append(item.get('id'))
    return out, ids


def render(doc):
    """The exact bytes written: indent 1, the document's own key order, LF, one trailing newline."""
    return (json.dumps(doc, indent=1) + '\n').encode('utf-8')


def sha256_bytes(data):
    return hashlib.sha256(data).hexdigest()


def write_atomic(path, data):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f'{path.name}.{os.getpid()}.tmp')
    with open(tmp, 'wb') as f:
        f.write(data)
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, path)


def write_with_hash(doc, out):
    """Writes the document and `<out>.sha256`; returns its sha256. An existing different output is refused."""
    out = Path(out)
    data = render(doc)
    digest = sha256_bytes(data)
    if out.exists() and out.read_bytes() != data:
        raise DoseError(f'{out} exists with different content (never overwritten)')
    write_atomic(out, data)
    write_atomic(out.with_name(out.name + '.sha256'), f'{digest}  {out.name}\n'.encode('utf-8'))
    return digest


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest='cmd', required=True)
    d = sub.add_parser('dose', help='a parent dose record with the V7 transfer')
    d.add_argument('--dose', required=True)
    b = sub.add_parser('base', help="a base's named-opponent sources with the V7 transfer")
    b.add_argument('--base', required=True)
    for p in (d, b):
        p.add_argument('--out', required=True, help='the file to write; its sha256 goes beside it')
        p.add_argument('--check', action='store_true', help='print what would be written and write nothing')
    a = ap.parse_args(argv)
    raw = Path(a.dose if a.cmd == 'dose' else a.base).read_bytes()
    try:
        doc = json.loads(raw.decode('utf-8'))
        if a.cmd == 'dose':
            out, extra = convert_dose(doc), {'checkpoint': (doc['source'].get('checkpoint') or {}).get('sha256')}
        else:
            out, ids = convert_base(doc)
            extra = {'opponents': ids}
        summary = dict(extra, input_sha256=sha256_bytes(raw), output_sha256=sha256_bytes(render(out)),
                       out=Path(a.out).as_posix(), transfer=V7, written=False)
        if not a.check:
            write_with_hash(out, a.out)
            summary['written'] = True
    except DoseError as exc:
        raise SystemExit(f'refused: {exc}')
    print(json.dumps(summary, indent=1, sort_keys=True))


if __name__ == '__main__':
    main()
