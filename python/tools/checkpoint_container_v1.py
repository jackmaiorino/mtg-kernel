"""Read checkpoints in either format (v1 JSON or the checkpoint container) from Python tools.

A checkpoint file is either v1 JSON (starts with `{`) or a container (starts with the magic `MTGCKPT\\0`, Rust
`expanded_deck_training_v1::checkpoint_container_v1`). The container is the default record format for ordinary fresh
V4, FlatV7 and FlatV8 checkpoints. Tools that read checkpoint fields use this module instead of `json.loads` so they
accept both:

- `document_from_bytes(data, **json_kwargs)`: the checkpoint as a dict from bytes the caller already pinned, exactly
  as `json.loads` of its v1 JSON (so a pin check and the parse use the same bytes);
- `read_document(path)`: the same from a file;
- `has_moments(path)`: whether the Adam moment lists are present and non-empty;
- `export_v1(path)`: the checkpoint's exact v1 JSON bytes. Only a reader that needs those bytes (a hash of the v1
  file, a byte comparison) uses it; it runs the Rust exporter, which needs the environment variable
  MTG_KERNEL_CHECKPOINT_CONTAINER_BIN and fails by name without it;
- `read_s_document(path)`: a native S train-state container's document (config, step, hashes, parameter names
  and shapes), through the binary's `s-document` mode.

A container is read in pure Python, with no binary. Layout (little-endian): magic, version u32 = 1, reserved u32 = 0,
document length u64, table length u64, the document (the checkpoint's canonical JSON with its three tensor lists
empty), the section table, zero padding to a multiple of 256, then each tensor's f32 bits, 256-aligned. The decoder
checks the header, the table and every section's order, offset, length and padding, and that the file ends after the
last tensor, then fills `parameters`, `first_moments` and `second_moments` with `{name, shape, values}` rows of u32
bit patterns, which is what the v1 JSON holds. It does not re-serialize the document or recompute `state_sha256`
(the Rust loader does both); the caller's pin of the file's SHA-256 binds every byte, so use a pinned read.
"""
import hashlib
import json
import os
import struct
import subprocess
import tempfile
from pathlib import Path

MAGIC = b'MTGCKPT\x00'
BINARY_ENV = 'MTG_KERNEL_CHECKPOINT_CONTAINER_BIN'
_HEADER = 32
_ALIGN = 256
_STATE_FORMAT = 'mtgts-sections-v1'
_SECTIONS = ('parameters', 'first_moments', 'second_moments')


def is_container(data: bytes) -> bool:
    return data.startswith(MAGIC)


def _align(value: int) -> int:
    return -(-value // _ALIGN) * _ALIGN


def decode_container(data: bytes, **json_kwargs) -> dict:
    """A checkpoint container's v1 document as a dict (see the module docstring for what is checked)."""
    if len(data) < _HEADER or not is_container(data):
        raise ValueError('not a checkpoint container')
    version, reserved, document_length, table_length = struct.unpack_from('<IIQQ', data, 8)
    if version != 1 or reserved != 0:
        raise ValueError('checkpoint container version differs')
    document_end = _HEADER + document_length
    table_end = document_end + table_length
    data_start = _align(table_end)
    if data_start > len(data):
        raise ValueError('checkpoint container is truncated')
    document = json.loads(data[_HEADER:document_end], **json_kwargs)
    table = json.loads(data[document_end:table_end])
    if not isinstance(document, dict) or any(document.get(name) != [] for name in _SECTIONS):
        raise ValueError('checkpoint container document is not a checkpoint with empty tensor lists')
    if (not isinstance(table, dict) or set(table) != {'state_format', 'sections'}
            or table['state_format'] != _STATE_FORMAT or not isinstance(table['sections'], list)):
        raise ValueError('checkpoint container section table differs')
    if any(data[table_end:data_start]):
        raise ValueError('checkpoint container padding is not zero')
    body = memoryview(data)[data_start:]
    expected_offset = data_end = section_index = ordinal = 0
    for entry in table['sections']:
        if (not isinstance(entry, dict)
                or set(entry) != {'section', 'ordinal', 'name', 'shape', 'offset', 'byte_length'}):
            raise ValueError('checkpoint container section entry differs')
        while section_index < len(_SECTIONS) and entry['section'] != _SECTIONS[section_index]:
            section_index += 1
            ordinal = 0
        shape = entry['shape']
        if (section_index >= len(_SECTIONS) or not isinstance(entry['name'], str) or not isinstance(shape, list)
                or not all(type(dim) is int and dim >= 0 for dim in shape)):
            raise ValueError('checkpoint container sections are out of order')
        count = 1
        for dim in shape:
            count *= dim
        end = expected_offset + 4 * count
        if (entry['ordinal'] != ordinal or entry['offset'] != expected_offset or entry['byte_length'] != 4 * count
                or end > len(body) or any(body[data_end:expected_offset])):
            raise ValueError('checkpoint container section layout differs')
        values = list(struct.unpack_from(f'<{count}I', body, expected_offset))
        document[_SECTIONS[section_index]].append({'name': entry['name'], 'shape': shape, 'values': values})
        ordinal += 1
        data_end = end
        expected_offset = _align(end)
    if data_end != len(body):
        raise ValueError('checkpoint container length differs from its sections')
    return document


def document_from_bytes(data: bytes, **json_kwargs) -> dict:
    """The checkpoint as `json.loads` of its v1 JSON, from bytes in either format."""
    if is_container(data):
        return decode_container(data, **json_kwargs)
    return json.loads(data, **json_kwargs)


def read_document(path) -> dict:
    """The checkpoint as `json.loads` of its v1 JSON, for either format."""
    return document_from_bytes(Path(path).read_bytes())


def export_v1(path) -> bytes:
    """The exact v1 JSON bytes of a checkpoint in either format (a container goes through the Rust exporter)."""
    path = Path(path)
    data = path.read_bytes()
    if not is_container(data):
        return data
    binary = os.environ.get(BINARY_ENV)
    if not binary:
        raise RuntimeError(f'export_v1: {path} is a checkpoint container and its exact v1 JSON bytes need the Rust '
                           f'exporter; set {BINARY_ENV} to the checkpoint_container_v1 binary (read_document needs '
                           'no binary)')
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp).resolve() / 'checkpoint.json'
        result = subprocess.run([binary, 'export', '--input', str(path.resolve()), '--sha256',
                                 hashlib.sha256(data).hexdigest(), '--output', str(out)],
                                capture_output=True, text=True)
        if result.returncode != 0:
            raise ValueError(f'{path}: {result.stderr.strip()}')
        return out.read_bytes()


def read_s_document(path) -> dict:
    """A native S train-state container's document, decoded and verified by the Rust binary."""
    path = Path(path)
    data = path.read_bytes()
    if not is_container(data):
        raise ValueError(f'{path} is not an S train-state container')
    binary = os.environ.get(BINARY_ENV)
    if not binary:
        raise RuntimeError(f'{path} is an S container; set {BINARY_ENV} to the checkpoint_container_v1 binary to '
                           'read it')
    result = subprocess.run([binary, 's-document', '--input', str(path.resolve()), '--sha256',
                             hashlib.sha256(data).hexdigest()], capture_output=True, text=True)
    if result.returncode != 0:
        raise ValueError(f'{path}: {result.stderr.strip()}')
    return json.loads(result.stdout)


def has_moments(path) -> bool:
    """Whether the checkpoint carries Adam moments."""
    doc = read_document(path)
    return bool(doc.get('first_moments')) and bool(doc.get('second_moments'))
