# MTG Kernel

`mtg-kernel` is a deterministic Magic: The Gathering rules kernel written in Rust, together with a reinforcement-learning training and evaluation stack. An XMage bridge lets the kernel be checked against XMage, an independent Magic engine, so its rules behavior and trained agents have an external point of comparison.

This repository is an independent extraction of kernel work formerly developed inside the Mage/XMage tree. Building and running it does not require Java, Maven, or a parent Mage checkout; see `docs/archive/EXTRACTION_PROVENANCE.md` for the exact cutover mapping.

## Layout

- `mtg-kernel/`: the Rust rules engine, JSONL environment, and diagnostic examples
- `python/mtg_kernel_rl/`: the Python client, feature encoder, model, trainer, and evaluators
- `scripts/`: experiment runners and qualification tooling, including `scripts/experiments/`
- `docs/`: research documentation, indexed in [`docs/README.md`](docs/README.md), with `contracts/` (frozen specs and manifests), `reports/` (experiment results), `design/` (design drafts), and `archive/` (superseded notes)
- `data/`: the card registry, deck manifests, and generated goldens
- `oracle/xmage/`: XMage reference material used for external anchoring
- `qualification/`: vendored forks pinned for reproducible builds

The repository root is reserved for project-level entry points such as this README and `ROADMAP.md`. Hash-pinned research records live under `docs/`; their bytes remain unchanged, while path-aware audit tooling resolves their current locations and preserves checks against their original Git paths.

## Build and test

```bash
uv sync --locked --extra test
cargo build --release --locked --bin kernel_rl_env
cargo test --release --locked --workspace --all-targets
```

`bash scripts/verify_all.sh` runs the full local gate: formatting, Clippy, Rust tests, and the Python test suite together. CUDA-backed training paths (for example the `cuda-flat-training-capacity-v1` feature) are opt-in Cargo features that require a CUDA toolchain and are not built by default.

## Cycle-4 routing refusals

| Tool | Refusal |
| --- | --- |
| `cycle4_routing_v1` | `--reference-document` is required. Each M3 report must bind its canonical SHA-256, run, tip checkpoint, exact update 1537 through 2048 window, audit-note SHA-256, reference statistic bits, and derived dispersion allowance bits. |
| `cycle4_routing_v1` and the M3 reference decoder | The reference window must declare first update 1537, last update 2048, update count 512, and a count equal to `last - first + 1`. |
| `run_m2_common_root_panel_v1.py` | The immutable panel is published only by hard link from its complete staged file. An identical existing panel is a no-op, a different existing panel is refused, and unavailable or unsupported hard linking fails closed. |

## Research status

Work is ongoing on self-play population training over a fixed deck pool, with periodic external anchoring against XMage's CP7 AI as a reference point. This is early-stage research: there is no claim of professional- or expert-level play, and results here are engineering evidence, not competitive benchmarks.

## License and provenance

This extraction is distributed under the same MIT license as the parent Mage repository; see [LICENSE.txt](LICENSE.txt). Mage/XMage remains a rules oracle and provenance source for bounded reference evidence; it is not a runtime dependency of this repository.
