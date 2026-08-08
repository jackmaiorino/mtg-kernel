# Common model snapshot v1

`mtg-kernel-python-authoritative-common-model-snapshot-v1` is the frozen
initial-model interchange for matched Python/Rust native-training trials. The
artifact lives at:

- `data/common_model_snapshot_v1/manifest.json`
- `data/common_model_snapshot_v1/parameters.f32le`

The payload is exactly 1,230,994 IEEE-754 binary32 values (4,923,976 bytes) in
the 33-entry Torch `named_parameters()` order. The manifest binds the full
model/config/feature contract, base seed 0, model-init seed
6,443,515,232,517,447,393, the native trainer schedule, the exact Python
authority runtime, and the authority source bundle at
`authority.source_bundle_sha256`. Parameter-layout and named-stream digests
live at `integrity.parameter_layout_sha256` and
`integrity.named_parameter_stream_sha256`, so the manifest-core and snapshot
digests bind both.

Portable verification never constructs a seeded model:

```bash
python python/tools/generate_common_model_snapshot_v1.py --check
```

Exact regeneration is intentionally narrower. It refuses to run unless the
host is Windows/AMD64 with Python 3.13.14, CPU Torch 2.13.0+cpu, little-endian
f32, deterministic algorithms, and one intra-op and inter-op thread:

```bash
python python/tools/generate_common_model_snapshot_v1.py --authority-check
```

`--generate` is an authority-maintenance command, not a portable build step.
Any change to an authority source requires exact regeneration and review; any
semantic contract change requires a new snapshot version.

Both loaders capture bounded regular non-link files with pre/post identity
checks, reject noncanonical or duplicate-key JSON, verify every binding and
digest, explicitly decode finite f32le values, and require the token-zero
embedding row to be positive zero. Each loader builds a private candidate,
re-exports all 33 tensors, verifies the named stream again, bootstraps Adam at
step zero with positive-zero moments, and only then replaces live state.

The canonical optimizer gauge set is exactly `scorer.2.bias`. Its payload bit
pattern is stored as the anchor. A matched update that exceeds its
scale-derived residual bound fails before canonicalization or mutation; a
passing update zeros only that gradient and its moments and holds the frozen
anchor. No value-head parameter is a gauge.

The exact scope statement carried by every matched record is:

> Rust does not reproduce the Python trainer-seeded-v1 initializer in this snapshot configuration; the snapshot proves bit-exact initial parameters only and does not establish seeded-initializer parity, cross-runtime numerical bit parity, learning parity, or speedup.

This artifact is not a trainer-resume checkpoint. Exact Torch initializer
reproduction, native checkpoint/resume, learning noninferiority, and the
end-to-end speed ratio remain separate gates.
