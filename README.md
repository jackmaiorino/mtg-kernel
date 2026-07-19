# MTG Kernel

`mtg-kernel` is an experimental, deterministic Magic: The Gathering rules kernel and an integrity-checked reinforcement-learning runner, trainer, and evaluator designed for reproducible experiments. The Rust process exposes a strict JSONL environment; the Python package supplies the client, schema-v5 feature encoder, policy/value model, crash-consistent training store, and paired evaluators.

This repository is the independent extraction of the kernel work formerly developed inside the Mage/XMage tree. Building or running it does not require Java, Maven, or a parent Mage checkout.

## Current status

The deterministic engine, schema-v5 Rust/Python policy boundary, artifact integrity layer, and a substantial regression suite are implemented. A build-time-validated catalog freezes the exact canonical Burn and Rally 60-card mainboards. The runner admits all four ordered Burn/Rally seat pairings. Training and both paired-evaluation lanes have bounded mirror smoke coverage for `Burn`/`Burn` and `Rally`/`Rally`; training, greedy evaluation, and sampled evaluation deliberately reject mixed-deck pairings until the separately versioned multi-deck seat-swap and seed contract exists. Policy schema v5 layers canonical binary attacker/blocker scans and grouped physical-decision accounting over the preserved schema-v4 engine semantics and H2 surface, so wide combat has two legal actions per policy substep instead of an exponential subset list.

These admission and smoke results are engineering evidence, not completion of the roadmap's 32-pair mirror gate, cross-deck gate, full-pool protocol, or science-release gate.

The Python-authoritative common initial-model interchange used by the matched
native trainer lane is documented in
[COMMON_MODEL_SNAPSHOT_V1.md](COMMON_MODEL_SNAPSHOT_V1.md). It is an
initial-parameter artifact with a step-zero optimizer bootstrap, not a resume
checkpoint or an initializer-, learning-, or speed-parity claim.

This is **not yet science-ready**. Seven canonical Pauper decks remain incomplete, the full-pool training protocol and sampled-primary 9x9 matrix have not been run, and the clean-clone artifact-reproduction release gate remains open. [ROADMAP.md](ROADMAP.md) is the authoritative definition of completion and records the pinned nine-deck scope.

## Requirements

- Rust `1.94.1` through `rustup` (selected automatically by `rust-toolchain.toml`)
- Python `3.13.14` for the pinned development/test environment (`pyproject.toml` supports Python 3.11 or newer)
- `uv 0.11.29` for the cross-platform Python dependency lock
- Bash for the one-command verification script; Git Bash works on Windows

PyTorch is the Python runtime's main external dependency and is a large download. `uv.lock` pins dependency resolution for the CPU reference environment across supported platforms, including NumPy for checkpoint logical-byte hashing and process-state regression tests, while `pyproject.toml` records the package's supported dependency ranges. Cross-platform locking means that supported platforms resolve the intended package versions; it does not promise bit-identical PyTorch results or training, checkpoint, or evaluation artifact bytes across operating systems or hardware. With source/protocol inputs and the environment binary held fixed, exact Torch-derived and artifact reproduction is scoped to the recorded runtime compatibility tuple on designated hardware. The current tuple records Python and Torch runtime details, OS/release, machine/architecture, device/dtype, deterministic mode, and thread counts, but it is not a complete hardware fingerprint, so a separate versioned hardware record is required before a science-ready run. Accelerator-specific training environments will be pinned separately with that hardware record for the full-pool science run; they do not silently replace this CI/reference lock.

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
  --max-physical-decisions 5000 \
  --max-policy-steps 640000 \
  --p0 uniform \
  --p1 uniform \
  --deck-ids Burn Burn
```

On Windows, use `target/release/kernel_rl_env.exe` for `--env-bin`. Run `mtg-kernel-rl --help` to see the `run`, `train`, `evaluate`, and `evaluate-sampled` interfaces.

## Performance evidence

The bounded Rust sampler
`f32-q8-expq63-hamilton-splitmix64-v1` is documented in
[FAST_SAMPLER_V1_VALIDATION.md](FAST_SAMPLER_V1_VALIDATION.md). It has a new
sampler identity and does not reinterpret historical
`decimal-softmax-hamilton-splitmix64-v1` artifacts. Its immutable canonical
sampler-only matrix passed all 19 predeclared claim checks: median throughput
was 20,494,573.554883800 decisions/second at one thread and
190,915,808.754026413 decisions/second at 16 threads. The probability and
capacity workload uses a provenance-bound 2,048-decision all-policy Rally
shape. That source's own performance/timing gate was invalid and contributes
no source rate; it supplies workload-shape provenance only. The result is
Rally-only, not nine-deck coverage. The sampler is not wired into the live
Python trainer/evaluator and makes no learning-noninferiority or end-to-end
training claim.

`bench_kernel --ceiling-json-v1` is an H2 engine-plus-`HarnessSurfaceV2` upper-bound diagnostic. It excludes production `PolicySurfaceV5` scalarization/transactional cloning, RL-session observation and legal-action encoding, privileged integrity checks, JSONL/IPC, neural inference and sampling, optimization, and artifact persistence. It therefore must not be compared directly with end-to-end XMage trainer throughput. An end-to-end training speedup over XMage has not yet been established; designated-hardware trainer throughput remains an open roadmap gate.

The opt-in raw-ceiling lane emits one strict JSON record for a matched runtime deck and never counts safety-capped, halted, or apply-error episodes as completed games:

```bash
cargo run --release --locked --example bench_kernel -- \
  --ceiling-json-v1 \
  --git-commit 0123456789abcdef0123456789abcdef01234567 \
  --deck Rally \
  --actors 1,4,8,16 \
  --warmup-ms 1000 \
  --measure-ms 10000 \
  --seed 71501
```

After the H2 continuation gate, `--three-lane-ceiling-json-v1` attributes the
remaining in-process boundary tax without changing the legacy ceiling
contract. It emits `kernel_rl_three_lane_ceiling/v1` with sequential
`engine_raw`, `harness_surface_v2`, and `rl_session_v5_inproc` lanes. H2 and
V5 share collision-free episode/environment/policy seed prefixes and the
`seeded_uniform_h2_semantics/v1` policy; V5 pre-samples each combat group so
its Boolean scan commits the same aggregate H2 action. Raw-engine and H2
trajectories are explicitly non-equivalent. The production-boundary lane
calls `current_response` once after reset and thereafter carries each response
returned by `step`, avoiding duplicate response materialization.

```bash
cargo run --release --locked --example bench_kernel -- \
  --three-lane-ceiling-json-v1 \
  --git-commit 0123456789abcdef0123456789abcdef01234567 \
  --deck Rally \
  --actors 1,4,8,16 \
  --warmup-ms 1000 \
  --measure-ms 10000 \
  --seed 71501
```

All three lanes stop before JSONL, IPC, Python, model inference, learning, and
artifact persistence. Their rates locate Rust boundary costs; none is an
end-to-end training rate or an XMage speedup claim. Each lane divides natural
terminals by the slowest actor's finish offset from one shared start, records
overshoot, and reports every non-natural outcome separately.
Any raw-lane halt makes that trial invalid and opens a separate raw-engine
correctness investigation; it is never discarded from the denominator or
silently treated as H2/V5 evidence.

`--matched-uniform-runtime-json-v2` is the fixed-Rally candidate for a later
paired XMage runtime trial. Its timed lane is the in-process fast actor and
its policy uses the exact `kernel-python-rl-seed-v2` group/leaf schedule with
one physical-decision counter per actor seat. Before emitting any rate it
runs untimed full-v5 versus fast-actor parity oracles on fixed Burn and Rally
episodes, comparing metadata, semantic action order, selected ranks,
per-seat counters, core hashes, and terminals. Any non-natural warmup or
measurement game, driver error, truncation, or oracle mismatch refuses the
record. Warmup and measurement use XMage v2's exact actor-striped uint63
episode schedule (warmup starts at zero; measurement starts at `1 << 62`)
and exact decimal durations. Each process accepts exactly one of 1, 4, 8, or
16 actors. Main establishes a common start and deadline for each phase; actors
launch only before that deadline, finish every in-flight game, and join. The
denominator is the slowest actor's finish offset from the common start. The
record includes each actor's attempted/natural count, first/last episode,
finish offset, deadline-tail completions, plus actual policy action-selection
and leaf-evaluation counters. Optional validation transcripts are materialized
outside the timed loop.

The trial, host, CPU, topology, power, affinity, and available-processor
contracts intentionally mirror the XMage v2 adapter for an external same-host
AB/BA validator. `strict` requires an exact commit and clean source tree;
`dirty-smoke` always emits a nonclaiming diagnostic record. The Rust record
always leaves `formal_comparison_claim` false: neither this candidate alone nor
its absolute rates establish an XMage multiplier or training throughput. The
running binary also exact-matches seven enumerated embedded runtime source
components against the claimed commit before validation/timing and again after
both phases, using UTF-8 bytes after CRLF-to-LF normalization while rejecting
bare carriage returns; SHA-256 values are diagnostics, never a substitute for
exact byte equality. A separate commit-tree proof binds the build HEAD and
clean/dirty state to a deterministic SHA-256 over every tracked path, mode,
type, and exact Git blob content (or gitlink object ID). Strict local-candidate
trials recompute that expected-commit tree before and after timing and require
it to equal the clean build-time tree.

Those checks are commit-tree integrity plus seven-component defense in depth,
not complete compiled-input provenance. They do not attest every byte consumed
by rustc, build scripts, procedural macros, dependencies, the toolchain, or the
build environment. Every emitted record therefore sets
`compiled_input_closure_attested=false`,
`formal_build_attestation_present=false`,
`formal_build_attestation_required=true`, and
`formal_paired_multiplier_authorized=false`. An external sealed-builder
attestation covering the full compiled-input closure remains a required gate
before any formal paired multiplier can be authorized. Effective
available-processor/runtime bindings are likewise captured before and after
the trial and must remain identical and match the declared count.

```bash
cargo run --release --locked --example bench_kernel -- \
  --matched-uniform-runtime-json-v2 \
  --expected-commit 0123456789abcdef0123456789abcdef01234567 \
  --actors 16 \
  --base-seed 71501 \
  --warmup-seconds 1 \
  --measure-seconds 10 \
  --trial-id pair-0001-rust \
  --binding-mode strict \
  --affinity-contract-id fixed-cpuset.v1 \
  --expected-available-processors 16 \
  --cpu-contract-id cpu-model.v1 \
  --topology-contract-id topology.v1 \
  --host-contract-id designated-host.v1 \
  --power-contract-id fixed-power.v1 \
  --transcript-games-per-deck 0
```

H2 suppression auditing is explicit and policy-inert. `HarnessSurfaceV2::new`
and the legacy raw-ceiling v1 retain `Full` ordered suppression records for
replay diagnostics; `CountOnly` retains fixed reason counters; production
RL sessions and the paired three-lane H2/V5 lanes select `Off`, which performs
no suppression state hashing, action-text construction, or log retention.
The three-lane artifact records `not_applicable/off/off` for its three lanes,
and fixed Burn/Rally regressions require all modes to produce identical public
contexts, response bytes, stable action IDs, state hashes, and terminal results.

`kernel_rl_env --phase-profile-v1` leaves stdout protocol bytes unchanged and emits exactly one aggregate `MTG_KERNEL_PROFILE_V1` record on stderr after graceful EOF. The separate end-to-end harness collects that Rust record plus external Python `perf_counter_ns` phases into `kernel_rl_training_benchmark/v1`; timings never enter the deterministic training store:

```bash
python -m mtg_kernel_rl.training_benchmark \
  --env-bin target/release/kernel_rl_env \
  --out-dir runs/perf-rally \
  --git-commit 0123456789abcdef0123456789abcdef01234567 \
  --repo-root . \
  --profile-mode off \
  --deck-id Rally --trials 3 --until-update 10 \
  --batch-episodes 2 --base-seed 71501 \
  --learning-rate 0.001 --value-coef 0.5 \
  --max-physical-decisions 5000 --max-policy-steps 640000
```

Use `--profile-mode off` for the primary uninstrumented throughput lane and `--profile-mode phase_v1` for diagnostic attribution. The H2 ceiling is a boundary-capacity diagnostic, not a core-engine physics verdict or training-speed claim. Failure routes the H2/kernel boundary to redesign; success only authorizes the next three in-process lanes: core engine, H2, and production `PolicySurfaceV5`/RL-session throughput. Unprofiled end-to-end training is a separate fourth boundary afterward. Only a fresh matched same-host XMage Rally benchmark and that end-to-end lane can establish a multiplier.

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

GitHub Actions runs the same substantive checks in independent Linux and Windows jobs. Each job validates platform-local behavior; CI does not compare cross-OS training, checkpoint, or evaluation artifact bytes. The Windows environment binary has the `.exe` suffix.

## Repository layout

- `mtg-kernel/`: Rust rules kernel, JSONL environment, tests, and diagnostic examples
- `python/mtg_kernel_rl/`: Python client, model, trainer, runner, evaluators, and artifact stores
- `python/tests/`: unit, failure-boundary, determinism, and real-environment tests
- `data/`: generated card registry, canonical Pauper pool/support manifests, and the build-time-validated Burn/Rally runtime catalog
- `corpus_archives_v1.json`: release-asset byte locks and content-lock linkage
- `uv.lock`: exact CPU reference-environment dependency resolution
- `ROADMAP.md`: science-readiness gates and ordered deck implementation plan
- `RUNTIME_DECKS_V1_VALIDATION.md`: bounded Burn/Rally runtime, reproducibility, and throughput evidence
- `CORPUS_CONTENT_LOCKS.md`: replay-corpus integrity contract
- `EXTRACTION_PROVENANCE.md`: exact parent/standalone cutover mapping

## License and provenance

This extraction is distributed under the same MIT license as the parent Mage repository. Its copyright notice is preserved verbatim in [LICENSE.txt](LICENSE.txt), and the exact history cutover is recorded in [EXTRACTION_PROVENANCE.md](EXTRACTION_PROVENANCE.md). Mage/XMage remains a rules oracle and provenance source for bounded reference evidence; it is not a runtime dependency of this repository.
