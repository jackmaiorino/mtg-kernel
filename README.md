# MTG Kernel

`mtg-kernel` is a deterministic Magic: The Gathering rules kernel written in Rust, together with a reinforcement-learning training and evaluation stack. An XMage bridge lets the kernel be checked against XMage, an independent Magic engine, so its rules behavior and trained agents have an external point of comparison.

This repository is an independent extraction of kernel work formerly developed inside the Mage/XMage tree. Building and running the standalone kernel does not require Java, Maven, or a parent Mage checkout; see the [extraction provenance](docs/archive/EXTRACTION_PROVENANCE.md) for the exact cutover mapping. Running the optional XMage comparison bridge requires its separate Java runtime.

## Getting started

Run commands from the repository root. Install Rust through `rustup`, the platform's native C/C++ build tools and linker, and **uv 0.11.29**. The repository pins **Rust 1.94.1** in [`rust-toolchain.toml`](rust-toolchain.toml) and **Python 3.13.14** in [`.python-version`](.python-version); `uv` can provision the pinned Python. On Windows, use the Visual Studio C++ build tools; the Bash verification script requires Git Bash.

```bash
uv sync --locked --extra test
cargo build --release --locked --bin kernel_rl_env
uv run --no-sync mtg-kernel-rl --help
```

The Python CLI provides `run`, `train`, `evaluate`, and `evaluate-sampled` commands. For a bounded interface smoke, run one game with uniform legal-action selection:

```bash
uv run --no-sync mtg-kernel-rl run --env-bin target/release/kernel_rl_env --out-dir target/smoke-output --episodes 1 --base-seed 123 --max-physical-decisions 4096 --max-policy-steps 8192 --p0 uniform --p1 uniform --deck-ids Burn Burn
```

On Windows, replace the binary path with `target/release/kernel_rl_env.exe`. This CLI's deck choices are Burn and Rally; its training and evaluation commands require mirror pairings. The smoke writes its results to `target/smoke-output/` and may truncate at the declared limits. It is not a trained-policy or playing-strength result. Substantial training, simulation and evaluation use the supported qualified launchers and current resource policy.

## Build variants and verification

| Catalog | Cargo feature | Interface |
| --- | --- | --- |
| Default Pauper | Default features | `kernel_rl_env` and the Python CLI above |
| Supported Foundations cards | `limited-fdn-fixtures` | `kernel_limited_env` with custom mainboards |
| MageZero Standard fixtures | `standard-magezero-fixtures` | `kernel_limited_env` with the unchanged Standard deck pool |

The optional catalogs are separate builds. Standard enables shared FDN rules code but selects the Standard registry instead of the FDN registry. Frozen Pauper policy encoders can refuse state exposed by the optional catalogs.

```bash
cargo test --release --locked --workspace --all-targets
bash scripts/verify_all.sh
```

`scripts/verify_all.sh` checks the locked Python environment, formatting, Clippy, default-feature Rust tests, canonical Pauper manifests, the release environment binary, and Python tests against that binary. It does not run the optional-feature test matrix. The [CI workflow](.github/workflows/ci.yml) defines the separate Limited, Standard, CUDA, native Store, and other feature checks on Linux and Windows. CUDA training features require their supported backend and toolchain; device-dependent qualification is separate from hosted compile checks.

## Layout

- `mtg-kernel/`: the Rust rules engine, JSONL environment, and diagnostic examples
- `python/mtg_kernel_rl/`: the Python client, feature encoder, model, trainer, and evaluators
- `scripts/`: experiment runners and qualification tooling, including `scripts/experiments/`
- `docs/`: research documentation, indexed in [`docs/README.md`](docs/README.md), with `contracts/` (frozen specs and manifests), `reports/` (experiment results), `design/` (design drafts), and `archive/` (superseded notes)
- `data/`: the card registry, deck manifests, and generated goldens
- `oracle/xmage/`: XMage reference material used for external anchoring
- `qualification/`: vendored forks pinned for reproducible builds

The repository root is reserved for project-level entry points such as this README and the [roadmap](ROADMAP.md). Hash-pinned research records live under `docs/`; their bytes remain unchanged, while path-aware audit tooling resolves their current locations and preserves checks against their original Git paths.

## Limited prototype

An opt-in `kernel_limited_env` binary accepts custom mainboards of at least
40 cards using the currently supported registry. The standalone Python deck
importer and reset/step adapter are documented in
[`limited_custom_session_v1.md`](docs/design/limited_custom_session_v1.md).
Build the FDN interface with:

```bash
cargo build --release --locked --features limited-fdn-fixtures --bin kernel_limited_env
```

The process flag selects its immutable outer JSONL schema. Clients must send the matching schema:

| Flag | Schema | Behavior |
| --- | --- | --- |
| No flag | 1 | Legacy priority presentation and opening deal |
| `--engine-priority-v1` | 2 | Every engine priority window |
| `--foundations-combat-v1` | 3 | Engine priority and Foundations combat damage assignment |
| `--london-mulligans-v1` | 4 | Engine priority, Foundations combat and London mulligans |

See the [priority interface](docs/design/limited_priority_windows_v1.md),
[combat interface](docs/design/fdn_combat_damage_v1.md), and
[London mulligan implementation](docs/design/fdn_london_mulligans_v1.md).
Full Foundations card coverage and complete Limited rules are being developed
under [issue #110](https://github.com/jackmaiorino/mtg-kernel/issues/110).

## MageZero Standard decks

The opt-in `standard-magezero-fixtures` build implements the unchanged 16-deck MageZero
Standard pool and its 225 distinct nonbasic cards. The v7 catalog admits every deck,
with fixture checks preserving the 62-card Mono-U list and empty sideboards. Rules
regressions and deterministic terminal games through the public session interface
verify the implementation; the completion record binds their tested source and results.
See the [fixture README](data/standard/magezero_v1/README.md) and
[completion record](docs/reports/standard_completion_v1.md). Generic observations expose
Standard state; frozen Pauper policy encoders may explicitly refuse it.

## Research status

Work is ongoing on self-play population training over a fixed deck pool, with periodic external anchoring against XMage's CP7 AI as a reference point. This is early-stage research: there is no claim of professional- or expert-level play, and results here are engineering evidence, not competitive benchmarks.

The [roadmap](ROADMAP.md) describes the science-ready acceptance criteria. The
[documentation index](docs/README.md) links interfaces, engineering acceptance
records and research contracts, including the preserved
[Cycle-4 routing compatibility rules](docs/cycle4_routing_compatibility_v1.md).

## License and provenance

This extraction is distributed under the same MIT license as the parent Mage repository; see [LICENSE.txt](LICENSE.txt). Mage/XMage remains a rules oracle and provenance source for bounded reference evidence; it is not a runtime dependency of this repository.
