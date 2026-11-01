"""Read checkpoint containers (Rust `expanded_deck_training_v1::checkpoint_container_v1`) from Python tools.

A checkpoint file is either v1 JSON (starts with `{`) or a container (starts with the magic `MTGCKPT\\0`). Tools
that read checkpoint fields use this module instead of `json.loads` so they accept both:

- `read_document(path)`: the checkpoint's metadata fields as a dict (for a container its tensor lists are empty);
- `has_moments(path)`: whether the Adam moment sections are present (non-empty);
- `export_v1(path)`: the exact v1 JSON bytes the binary's exporter writes for the checkpoint.

Layout (little-endian): magic (8), version u32 = 1, reserved u32 = 0, document length u64, table length u64, the
document, the section table, zero padding to the data start (a multiple of 256), then each tensor's f32 bits at a
256-aligned offset from the data start, in the order parameters, first moments, second moments.
"""
import json
import struct
from pathlib import Path

MAGIC = b'MTGCKPT\x00'
STATE_FORMAT = 'mtgts-sections-v1'
SECTIONS = ('parameters', 'first_moments', 'second_moments')
HEADER = 32
ALIGN = 256


def is_container(data: bytes) -> bool:
    return data.startswith(MAGIC)


def _align(value: int) -> int:
    return -(-value // ALIGN) * ALIGN


def _parts(data: bytes):
    if len(data) < HEADER or not is_container(data):
        raise ValueError('not a checkpoint container')
    version, reserved, doc_len, table_len = struct.unpack_from('<IIQQ', data, 8)
    if version != 1 or reserved != 0:
        raise ValueError('checkpoint container version differs')
    doc_end = HEADER + doc_len
    table_end = doc_end + table_len
    data_start = _align(table_end)
    if data_start > len(data):
        raise ValueError('checkpoint container is truncated')
    return data[HEADER:doc_end], data[doc_end:table_end], data_start


def _dumps(value) -> bytes:
    # serde_json's compact form for the strings, integers, booleans and nulls checkpoints hold.
    return json.dumps(value, separators=(',', ':'), ensure_ascii=False).encode('utf-8')


def decode(data: bytes) -> dict:
    """The checkpoint document with its tensor lists filled from the sections (values as u32 bit patterns)."""
    document, table, data_start = _parts(data)
    doc = json.loads(document)
    if any(doc.get(name) for name in SECTIONS):
        raise ValueError('checkpoint container document carries tensor values')
    table = json.loads(table)
    if table.get('state_format') != STATE_FORMAT:
        raise ValueError('checkpoint container state format differs')
    offset = 0
    for entry in table['sections']:
        count = 1
        for dim in entry['shape']:
            count *= dim
        if entry['offset'] != offset or entry['byte_length'] != 4 * count:
            raise ValueError('checkpoint container section layout differs')
        start = data_start + offset
        end = start + 4 * count
        if end > len(data):
            raise ValueError('checkpoint container is truncated')
        values = list(struct.unpack_from(f'<{count}I', data, start))
        doc[entry['section']].append({'name': entry['name'], 'shape': entry['shape'], 'values': values})
        offset = _align(offset + 4 * count)
    return doc


def encode(doc: dict) -> bytes:
    """The container for a checkpoint document (the inverse of `decode`, byte for byte the Rust encoder's output)."""
    document = dict(doc)
    for name in SECTIONS:
        document[name] = []
    document = _dumps(document)
    entries, offset = [], 0
    for name in SECTIONS:
        for ordinal, tensor in enumerate(doc[name]):
            count = 1
            for dim in tensor['shape']:
                count *= dim
            if count != len(tensor['values']):
                raise ValueError('checkpoint container tensor length differs from its shape')
            entries.append({'section': name, 'ordinal': ordinal, 'name': tensor['name'], 'shape': tensor['shape'],
                            'offset': offset, 'byte_length': 4 * count})
            offset = _align(offset + 4 * count)
    table = _dumps({'state_format': STATE_FORMAT, 'sections': entries})
    out = bytearray(MAGIC + struct.pack('<IIQQ', 1, 0, len(document), len(table)) + document + table)
    out.extend(b'\x00' * (_align(len(out)) - len(out)))
    data_start = len(out)
    for name in SECTIONS:
        for tensor in doc[name]:
            out.extend(b'\x00' * (data_start + _align(len(out) - data_start) - len(out)))
            out.extend(struct.pack(f'<{len(tensor["values"])}I', *tensor['values']))
    return bytes(out)


def read_document(path) -> dict:
    """Metadata fields of a checkpoint in either format; a container's tensor lists come back empty."""
    data = Path(path).read_bytes()
    if is_container(data):
        return json.loads(_parts(data)[0])
    return json.loads(data)


def has_moments(path) -> bool:
    """Whether the checkpoint carries Adam moments, from the section table for a container."""
    data = Path(path).read_bytes()
    if is_container(data):
        table = json.loads(_parts(data)[1])
        present = {entry['section'] for entry in table['sections']}
        return 'first_moments' in present and 'second_moments' in present
    doc = json.loads(data)
    return bool(doc.get('first_moments')) and bool(doc.get('second_moments'))


def export_v1(path) -> bytes:
    """The v1 JSON bytes of a checkpoint in either format; refuses a container that is not canonical."""
    data = Path(path).read_bytes()
    if not is_container(data):
        return data
    doc = decode(data)
    if encode(doc) != data:
        raise ValueError('checkpoint container is not in canonical form')
    return _dumps(doc)
