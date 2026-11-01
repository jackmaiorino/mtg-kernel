"""Read checkpoints in either format (v1 JSON or the checkpoint container) from Python tools.

A checkpoint file is either v1 JSON (starts with `{`) or a container (starts with the magic `MTGCKPT\\0`, Rust
`expanded_deck_training_v1::checkpoint_container_v1`). Tools that read checkpoint fields use this module instead of
`json.loads` so they accept both:

- `export_v1(path)`: the checkpoint's v1 JSON bytes;
- `read_document(path)`: the checkpoint as a dict, exactly as `json.loads` of its v1 JSON;
- `has_moments(path)`: whether the Adam moment lists are present and non-empty.

A container is never parsed in Python. Its validation (canonical form, schema, section layout) is the Rust
loader's, so every container read goes through the Rust exporter, the `checkpoint_container_v1` binary named by the
environment variable MTG_KERNEL_CHECKPOINT_CONTAINER_BIN, which verifies the file and writes its v1 JSON. Without
the binary a container is refused, never read with weaker checks.
"""
import hashlib
import json
import os
import subprocess
import tempfile
from pathlib import Path

MAGIC = b'MTGCKPT\x00'
BINARY_ENV = 'MTG_KERNEL_CHECKPOINT_CONTAINER_BIN'


def is_container(data: bytes) -> bool:
    return data.startswith(MAGIC)


def export_v1(path) -> bytes:
    """The v1 JSON bytes of a checkpoint in either format."""
    path = Path(path)
    data = path.read_bytes()
    if not is_container(data):
        return data
    binary = os.environ.get(BINARY_ENV)
    if not binary:
        raise RuntimeError(f'{path} is a checkpoint container; set {BINARY_ENV} to the checkpoint_container_v1 '
                           'binary to read it')
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp).resolve() / 'checkpoint.json'
        result = subprocess.run([binary, 'export', '--input', str(path.resolve()), '--sha256',
                                 hashlib.sha256(data).hexdigest(), '--output', str(out)],
                                capture_output=True, text=True)
        if result.returncode != 0:
            raise ValueError(f'{path}: {result.stderr.strip()}')
        return out.read_bytes()


def read_document(path) -> dict:
    """The checkpoint as `json.loads` of its v1 JSON, for either format."""
    return json.loads(export_v1(path))


def has_moments(path) -> bool:
    """Whether the checkpoint carries Adam moments."""
    doc = read_document(path)
    return bool(doc.get('first_moments')) and bool(doc.get('second_moments'))
