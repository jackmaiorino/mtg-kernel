# MTG Kernel

`mtg-kernel` is a deterministic Magic: The Gathering rules kernel written in Rust, together with a reinforcement-learning training and evaluation stack. An XMage bridge lets the kernel be checked against XMage, an independent Magic engine, so its rules behavior and trained agents have an external point of comparison.

This repository is an independent extraction of kernel work formerly developed inside the Mage/XMage tree. Building and running it does not require Java, Maven, or a parent Mage checkout; see `docs/archive/EXTRACTION_PROVENANCE.md` for the exact cutover mapping.

## Layout

- `mtg-kernel/`: the Rust rules engine, JSONL environment, and diagnostic examples
- `python/mtg_kernel_rl/`: the Python client, feature encoder, model, trainer, and evaluators
- `scripts/`: experiment runners and qualification tooling, including `scripts/experiments/`
- `docs/`: `contracts/` (frozen specs and manifests), `reports/` (experiment results), `design/` (design drafts), `archive/` (superseded notes)
- `data/`: the card registry, deck manifests, and generated goldens
- `oracle/xmage/`: XMage reference material used for external anchoring
- `qualification/`: vendored forks pinned for reproducible builds

A handful of manifests and result records with pinned SHA-256 hashes in `scripts/` (for example `ACTION_INGRESS_ADMISSION_V1.md`, `ACTION_INGRESS_ADMISSION_V2.md`, `OBSERVATION_DIAGNOSTICS_EXECUTION_V2.md`) stay at the repository root because audit tooling reads them from that exact path; moving them would break the hash-pinned verification chain.

## Build and test

```bash
uv sync --locked --extra test
cargo build --release --locked --bin kernel_rl_env
cargo test --release --locked --workspace --all-targets
```

`bash scripts/verify_all.sh` runs the full local gate: formatting, Clippy, Rust tests, and the Python test suite together. CUDA-backed training paths (for example the `cuda-flat-training-capacity-v1` feature) are opt-in Cargo features that require a CUDA toolchain and are not built by default.

## Research status

Work is ongoing on self-play population training over a fixed deck pool, with periodic external anchoring against XMage's CP7 AI as a reference point. This is early-stage research: there is no claim of professional- or expert-level play, and results here are engineering evidence, not competitive benchmarks.

## License and provenance

This extraction is distributed under the same MIT license as the parent Mage repository; see [LICENSE.txt](LICENSE.txt). Mage/XMage remains a rules oracle and provenance source for bounded reference evidence; it is not a runtime dependency of this repository.
