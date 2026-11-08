# Portable native artifact content identity

Issue #475: raw episode hashes include import descriptor paths and hashes,
previous checkpoint paths and hashes, and attempt directories. Dropping those
hashes compares less than the complete record. `native_artifact_identity_v1.py`
provides a separate, read-only content identity for the complete retained graph.
Raw files, storage pins, resume validation and frozen G(i) qualification rules
keep their existing meaning.

Each referenced file is read and SHA-256 checked before interpretation. A JSON
`{path, sha256}` pin becomes a logical path and the recursively computed content
hash of its target. The import descriptor's path fields are normalized too.
Hash-only `checkpoint_sha256`, `*_checkpoint_sha256`, `play_import_sha256` and
`trajectory_sha256` fields resolve through a supplied catalog of retained pins;
missing entries refuse. Repeated copies are memoized within one read session.
No trajectory, optimizer tensor, model bit, action, outcome or ancestry field
is dropped. Unrecognized fields remain committed, so a new unhandled path or
hash can cause a conservative mismatch rather than an equal comparison.

Declared roots have stable logical labels. Paths under those exact directory
boundaries replace the physical prefix with that label, and `bNNN-aK` path
components replace only K with a typed attempt marker. A pin stored by the cloud
stager at `inputs/objects/<raw-sha>/name` replaces its own verified raw-hash
component with a typed recursive content-hash marker. Other hash-looking
components stay literal. Run, block, episode and other path components
remain distinct. External paths retain their spelling. Dot traversal inside a
root refuses. Use equivalent logical labels and root boundaries in both runs.

The hash domain is `mtg-kernel-native-artifact-content/v1` followed by NUL and
canonical UTF-8 JSON (sorted keys, no whitespace). The hash input declares its
format. Documents use tagged objects, arrays, scalars, paths and dependencies
so ordinary data cannot imitate a normalization marker. Integers stay exact;
finite floats use their hexadecimal binary64 form, preserving signed zero.
JSON duplicate keys and non-finite numbers refuse. Native checkpoint containers
use the existing strict decoder, including every parameter and both moment
buffers, and share their reconstructed JSON document's identity. Opaque leaf
files retain their full byte hash. Compressed record envelopes currently refuse
explicitly; this version does not claim support for those records.

Run with the repository's pinned Python:

```text
python python/tools/native_artifact_identity_v1.py request.json
```

The request is a closed JSON object:

```json
{
  "schema": "mtg-kernel-native-artifact-identity-request/v1",
  "roots": {"native": "D:/campaign"},
  "artifact": {"path": "D:/campaign/G1/b002-a1/checkpoint.json", "sha256": "<raw sha256>"},
  "dependencies": [
    {"path": "D:/campaign/G1/b001-a1/checkpoint.json", "sha256": "<raw sha256>"}
  ]
}
```

All artifact pins are absolute. Nested pins discover their own dependencies;
the catalog supplies earlier checkpoints referenced only by hash. A missing
retained file cannot be reconstructed from a digest and causes refusal. Output
includes the root file pin, portable content hash and every verified artifact.
Compare `rule` and `content_sha256`, retaining the report's physical pins as
evidence. This comparison is an opt-in successor, not a rewrite of prior
qualification verdicts or authority to launch a campaign.

Regression coverage includes a two-block import/episode/checkpoint chain in
two roots and attempts (including cloud content-addressed import directories),
a changed prior action and a changed prior moment with
the same final state hash, missing dependencies and pin corruption, native
container/JSON equivalence, path boundaries, typed-marker collisions and CLI
execution. These are deterministic engineering checks, not training evidence.
