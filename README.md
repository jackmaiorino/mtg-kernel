# MTG Kernel

`mtg-kernel` is an experimental, deterministic Magic: The Gathering rules kernel and a reproducible reinforcement-learning runner, trainer, and evaluator. The Rust process exposes a strict JSONL environment; the Python package supplies the client, schema-v4 feature encoder, policy/value model, crash-consistent training store, and paired evaluators.

This repository is the independent extraction of the kernel work formerly developed inside the Mage/XMage tree. Building or running it does not require Java, Maven, or a parent Mage checkout.

## Current status

The deterministic engine, schema-v4 Rust/Python boundary, artifact integrity layer, Burn-mirror runner/trainer/evaluator, and a substantial regression suite are implemented. Burn and Rally have complete mainboard card coverage, and reusable mechanics for cards in the remaining decks are landing incrementally. The public RL environment still admits only the exact canonical `Burn`/`Burn` pairing.

This is **not yet science-ready**. Seven canonical Pauper decks remain incomplete, the full-pool training protocol and sampled-primary 9x9 matrix have not been run, and the clean-clone artifact-reproduction release gate remains open. [ROADMAP.md](ROADMAP.md) is the authoritative definition of completion and records the pinned nine-deck scope.

## Requirements

- Rust `1.94.1` through `rustup` (selected automatically by `rust-toolchain.toml`)
- Python `3.13.14` for the reproducible development/test environment (`pyproject.toml` supports Python 3.11 or newer)
- `uv 0.11.29` for the cross-platform locked Python environment
- Bash for the one-command verification script; Git Bash works on Windows

PyTorch is the Python runtime's main external dependency and is a large download. `uv.lock` pins the complete cross-platform CPU reference environment, including NumPy for a process-state regression test, while `pyproject.toml` records the package's supported dependency ranges. Accelerator-specific training environments will be pinned separately with the hardware record for the full-pool science run; they do not silently replace this CI/reference lock.

## Quick start

Install the pinned `uv` version and synchronize the locked environment:

```bash
python -m pip install --disable-pip-version-check uv==0.11.29
uv sync --locked --extra test
source .venv/bin/activate  # Windows PowerShell: .venv\Scripts\Activate.ps1
```

Build the release environment:

```bash
cargo build --release --locked --bin kernel_rl_env
```

Run a deterministic two-episode Burn mirror:

```bash
mtg-kernel-rl run \
  --env-bin target/release/kernel_rl_env \
  --out-dir runs/smoke \
  --episodes 2 \
  --base-seed 71501 \
  --max-decisions 5000 \
  --p0 uniform \
  --p1 uniform \
  --deck-ids Burn Burn
```

On Windows, use `target/release/kernel_rl_env.exe` for `--env-bin`. Run `mtg-kernel-rl --help` to see the `run`, `train`, `evaluate`, and `evaluate-sampled` interfaces.

## Reference replay corpora

The two formal Phase-0 XMage replay corpora are published as GitHub release
assets rather than committed to Git. Fetch both byte-locked archives and verify
their contents against the tracked locks with:

```bash
uv run --no-sync python python/tools/fetch_corpora.py --destination corpora
```

The command downloads the assets recorded in `corpus_archives_v1.json`, checks
each archive's exact size and SHA-256 before extraction, rejects unsafe archive
entries, and verifies every installed trace and manifest against
`corpus_content_locks_v1.json`. Existing corpus directories are reverified and
fail closed on drift. The resulting `corpora/` directory is intentionally
ignored by Git.

## Verification

The full local gate runs formatting, Clippy with warnings denied, release-mode Rust tests, canonical deck/hash validation, a release environment build, and all Python tests with the real Rust process enabled:

```bash
bash scripts/verify_all.sh
```

The equivalent individual commands are:

```bash
cargo fmt --all -- --check
cargo clippy --release --locked --workspace --all-targets -- -D warnings
cargo test --release --locked --workspace --all-targets
cargo build --release --locked --bin kernel_rl_env
uv run --no-sync python python/tools/generate_pauper_manifests.py --check --repo-root .
MTG_KERNEL_RL_ENV_BIN=target/release/kernel_rl_env \
  uv run --no-sync python -m unittest discover -s python/tests -v
```

For the final command in Windows PowerShell:

```powershell
$env:MTG_KERNEL_RL_ENV_BIN = (Resolve-Path "target/release/kernel_rl_env.exe")
uv run --no-sync python -m unittest discover -s python/tests -v
```

GitHub Actions runs the same substantive checks on Linux and Windows. The Windows environment binary has the `.exe` suffix.

## Repository layout

- `mtg-kernel/`: Rust rules kernel, JSONL environment, tests, and diagnostic examples
- `python/mtg_kernel_rl/`: Python client, model, trainer, runner, evaluators, and artifact stores
- `python/tests/`: unit, failure-boundary, determinism, and real-environment tests
- `data/`: generated card registry and canonical Pauper pool/support manifests
- `corpus_archives_v1.json`: release-asset byte locks and content-lock linkage
- `uv.lock`: exact CPU reference-environment dependency resolution
- `ROADMAP.md`: science-readiness gates and ordered deck implementation plan
- `CORPUS_CONTENT_LOCKS.md`: replay-corpus integrity contract
- `EXTRACTION_PROVENANCE.md`: exact parent/standalone cutover mapping

## License and provenance

This extraction is distributed under the same MIT license as the parent Mage repository. Its copyright notice is preserved verbatim in [LICENSE.txt](LICENSE.txt), and the exact history cutover is recorded in [EXTRACTION_PROVENANCE.md](EXTRACTION_PROVENANCE.md). Mage/XMage remains a rules oracle and provenance source for bounded reference evidence; it is not a runtime dependency of this repository.
