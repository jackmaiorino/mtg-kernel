"""Read checkpoint containers (Rust `expanded_deck_training_v1::checkpoint_container_v1`) from Python tools.

A checkpoint file is either v1 JSON (starts with `{`) or a container (starts with the magic `MTGCKPT\\0`). Tools
that read checkpoint fields use this module instead of `json.loads` so they accept both:

- `read_document(path)`: the checkpoint's metadata fields as a dict (for a container its tensor lists are empty);
- `has_moments(path)`: whether the Adam moment sections are present (non-empty);
- `export_v1(path)`: the v1 JSON bytes of the checkpoint.

Every reader checks a container's structure first: header, compact document and table, the document's top-level
members known and in the Rust struct's order, sections in order, aligned, contiguous and inside the file, every
padding byte zero, and the file ending after the last tensor. The Rust loader is the authority on canonical form
(it re-encodes what it decodes, nested members included); `export_v1` delegates to the Rust exporter when the
environment variable MTG_KERNEL_CHECKPOINT_CONTAINER_BIN names the `checkpoint_container_v1` binary.

Layout (little-endian): magic (8), version u32 = 1, reserved u32 = 0, document length u64, table length u64, the
document, the section table, zero padding to the data start (a multiple of 256), then each tensor's f32 bits at a
256-aligned offset from the data start, in the order parameters, first moments, second moments.
"""
import hashlib
import json
import os
import struct
import subprocess
import tempfile
from pathlib import Path

MAGIC = b'MTGCKPT\x00'
STATE_FORMAT = 'mtgts-sections-v1'
SECTIONS = ('parameters', 'first_moments', 'second_moments')
HEADER = 32
ALIGN = 256
# `ExpandedCheckpointV1`'s fields in declaration (serialization) order.
FIELD_ORDER = (
    'schema', 'feature_contract_digest', 'feature_encoding_digest', 'card_db_hash', 'source_import', 'state_sha256',
    'e_supported', 'soft_q', 'search_targets', 'ex1', 'adam_step', 'scorer_bias_anchor_bits', 'parameters',
    'first_moments', 'second_moments', 'trajectories', 'loss_identity', 'gamma_bits', 'gae_lambda_bits',
    'entropy_coefficient_bits', 'exploration', 'learning_rate_bits', 'value_coefficient_bits', 'registry_transfer',
    'archive_distillation', 'training_math', 'input_contract',
)
TABLE_KEYS = ('state_format', 'sections')
ENTRY_KEYS = ('section', 'ordinal', 'name', 'shape', 'offset', 'byte_length')


def is_container(data: bytes) -> bool:
    return data.startswith(MAGIC)


def _align(value: int) -> int:
    return -(-value // ALIGN) * ALIGN


def _dumps(value) -> bytes:
    # serde_json's compact form for the strings, integers, booleans and nulls checkpoints hold.
    return json.dumps(value, separators=(',', ':'), ensure_ascii=False).encode('utf-8')


def _in_order(keys, order) -> bool:
    position = {name: i for i, name in enumerate(order)}
    indices = [position.get(k) for k in keys]
    return None not in indices and indices == sorted(indices)


def _check(data: bytes):
    """Structure check; returns (document dict, list of (section, name, shape, start, end))."""
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
    document, table = data[HEADER:doc_end], data[doc_end:table_end]
    doc = json.loads(document)
    if _dumps(doc) != document or not _in_order(list(doc), FIELD_ORDER):
        raise ValueError('checkpoint container is not in canonical form')
    if any(doc.get(name) for name in SECTIONS) or any(name not in doc for name in SECTIONS):
        raise ValueError('checkpoint container document carries tensor values')
    parsed = json.loads(table)
    if _dumps(parsed) != table or list(parsed) != list(TABLE_KEYS):
        raise ValueError('checkpoint container is not in canonical form')
    if parsed['state_format'] != STATE_FORMAT:
        raise ValueError('checkpoint container state format differs')
    if any(data[table_end:data_start]):
        raise ValueError('checkpoint container is not in canonical form')
    sections, offset, data_end, section_index, ordinal = [], 0, 0, 0, 0
    for entry in parsed['sections']:
        if list(entry) != list(ENTRY_KEYS):
            raise ValueError('checkpoint container is not in canonical form')
        while section_index < len(SECTIONS) and SECTIONS[section_index] != entry['section']:
            section_index, ordinal = section_index + 1, 0
        if section_index == len(SECTIONS):
            raise ValueError('checkpoint container sections are out of order')
        count = 1
        for dim in entry['shape']:
            count *= dim
        if entry['ordinal'] != ordinal or entry['offset'] != offset or entry['byte_length'] != 4 * count:
            raise ValueError('checkpoint container section layout differs')
        start, end = data_start + offset, data_start + offset + 4 * count
        if end > len(data) or any(data[data_start + data_end:start]):
            raise ValueError('checkpoint container section layout differs')
        sections.append((entry['section'], entry['name'], entry['shape'], start, end))
        ordinal, data_end, offset = ordinal + 1, offset + 4 * count, _align(offset + 4 * count)
    if data_start + data_end != len(data):
        raise ValueError('checkpoint container length differs from its sections')
    return doc, sections


def decode(data: bytes) -> dict:
    """The checkpoint document with its tensor lists filled from the sections (values as u32 bit patterns)."""
    doc, sections = _check(data)
    for section, name, shape, start, end in sections:
        values = list(struct.unpack_from(f'<{(end - start) // 4}I', data, start))
        doc[section].append({'name': name, 'shape': shape, 'values': values})
    return doc


def encode(doc: dict) -> bytes:
    """The container for a checkpoint document (the Rust encoder's layout)."""
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
        return _check(data)[0]
    return json.loads(data)


def has_moments(path) -> bool:
    """Whether the checkpoint carries Adam moments, from the section table for a container."""
    data = Path(path).read_bytes()
    if is_container(data):
        present = {section for section, *_ in _check(data)[1]}
        return 'first_moments' in present and 'second_moments' in present
    doc = json.loads(data)
    return bool(doc.get('first_moments')) and bool(doc.get('second_moments'))


def export_v1(path) -> bytes:
    """The v1 JSON bytes of a checkpoint in either format, through the Rust exporter when it is configured."""
    data = Path(path).read_bytes()
    if not is_container(data):
        return data
    binary = os.environ.get('MTG_KERNEL_CHECKPOINT_CONTAINER_BIN')
    if binary:
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp).resolve() / 'checkpoint.json'
            subprocess.run([binary, 'export', '--input', str(Path(path).resolve()), '--sha256',
                            hashlib.sha256(data).hexdigest(), '--output', str(out)], check=True,
                           capture_output=True)
            return out.read_bytes()
    doc = decode(data)
    if encode(doc) != data:
        raise ValueError('checkpoint container is not in canonical form')
    return _dumps(doc)
