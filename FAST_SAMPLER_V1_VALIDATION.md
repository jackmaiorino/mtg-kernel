# Fast categorical sampler V1 validation

Status: **audit hold**. The bounded Rust candidate is implemented for review,
but no canonical observed-workload diagnostic or benchmark evidence is accepted
for this revision. The sampler is not wired into the live Python
trainer/evaluator.

Validation record date: 2026-07-18.

## Claim boundary

This document defines the candidate contract and the gates required before a
sampler-only throughput claim can be recorded. It does not establish learning
noninferiority, end-to-end training throughput, an XMage speedup, policy
quality, or completion of the final all-nine-deck sampler gate.

Historical artifacts remain bound to
`decimal-softmax-hamilton-splitmix64-v1`. The candidate has a new identity,
`f32-q8-expq63-hamilton-splitmix64-v1`, and cannot reinterpret those artifacts.

## Candidate contract

The admitted input is 1 through 64 finite IEEE-754 binary32 logits in legal
action order. The implementation:

1. Finds the maximum by finite binary32 order.
2. Computes every nonnegative gap exactly from the binary32 bit patterns in
   units of the minimum subnormal, without floating-point subtraction.
3. Multiplies the exact gap by 256, rounds to nearest integer with ties to even,
   and clamps to 4096 (`delta = -16`).
4. Looks up a Q63 exponential weight. The 4,097-entry table is generated at
   compile time from `w[0] = 2**63` and the round-to-even recurrence
   `w[k] = w[k-1] * 9187413517043429148 / 2**63`.
5. Uses bounded `u128` arithmetic and Hamilton largest-remainder apportionment
   to produce exactly `2**64` mass in legal order, with checked release
   postconditions.
6. Takes the first SplitMix64-v1 output from the supplied seed and performs an
   inverse-CDF selection with `u128` cumulative mass.

All admitted-width scratch is caller-owned fixed capacity. Empty, over-width,
and nonfinite inputs fail closed. Release code explicitly checks residual
range, exact mass total, big-integer carry/borrow, sign ordering, and
inverse-CDF totality instead of relying on debug assertions.

Pinned candidate digests:

| Input | SHA-256 |
|---|---|
| Canonical sampler contract | `276407494966b195b7c011caf984d2354484f7532161107b19ecc83388de92b6` |
| Q63 table as little-endian `u64` entries | `2cdd19abdec245d7a9f892e8757c299a282ae097361baecc46cfd6a57c476e2a` |
| Schema-2 Decimal oracle fixture | `bb42f0cacae9902d67851941678cf2fb34a90cb8459403126a8026085dcae033` |
| Workload-provenance envelope | `d9471ee78ee8b656040d1920118f962f4b239e55603220e3679b1d11b847e579` |
| Canonical workload-provenance record | `33490dc1fbf21555cc469595beadbda70c30092ac95cac297bc6f0e48ef18f7c` |

## Decimal-oracle diagnostic gate

The schema-2 generator requires exactly one workload source:

- `--width-evidence data/rally_all_policy_legal_action_width_histogram_v1.json`
  for provenance-bound Rally-versus-Rally evidence; or
- `--provisional-synthetic` for a nonclaiming development fixture.

The committed fixture binds 2,048 all-policy Rally-versus-Rally decisions:
mean width 4.2109375, nearest-rank p95 9, and maximum 13. It is explicitly not
learner-only. Its source is clean deterministic workload-shape provenance at
`d71dca82dfe36292328ecbc4962a0d6764d9ca5c`, but the source timing and
interference gate was invalid. The envelope therefore says
`performance_gate_valid: false`, includes no rates, and limits coverage to
Rally-versus-Rally rather than nine decks. The histogram may weight sampler
diagnostics; it cannot support a source-performance claim. The final
all-nine-deck gate remains deferred.

The workload record also binds the 64,453-byte raw source artifact
(`682198c7e169a67a2c885dd8362db0c67c329b8cb1e6390f4fbc905c3f9bd7ee`),
clean before/after source states, the empty status digest
`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`,
and a 132-file source manifest. Its before/after and bound-build manifest
digest is
`09e816949de05d76cf37148e015eb973b4f6568e256e755e5b727480df56d9d3`.
The environment binary is stably bound at
`b81b5ad88e6f728922b8635405aead28588066b2563cdd9644439100715d4c51`
and the encoder benchmark binary at
`04802ed2cb953b6ef0f071f42304221de16fd9f411b8decc025ffbfa56b1fbe8`
across prebuild, postbuild, before, and after observations. These are stable
source/binary observations, not a formal binary-to-source proof: both
`formal_binary_source_attestation_present` and
`compiled_input_closure_attested` remain false.

The generator independently produces SplitMix64 first-draw bytes and Decimal
selected-index goldens. The Rust diagnostic must match every Python draw and
the Python selected-index digest; it uses those independent draws for both
candidate and Decimal inverse-CDF comparisons. The pinned draw digest is
`d4cb1a34980c7c43234e771122c19ce0f3003f585ea6286c7929f6e5c09d341e`;
the pinned Decimal selected-index digest is
`68efffcf39bfbd30addcfb5fb1f53a2895f12d33d2a053d2f9bfd3efa9d50bed`.

The predeclared candidate bounds are:

| Metric | Result | Bound |
|---|---:|---:|
| Maximum total variation | 0.0007807736273533305 | <= 0.00125 |
| Mean total variation, all cases | 0.00019719468621583235 | report only |
| Width-profile-weighted mean total variation | 0.00015000850857101712 | <= 0.0005 |
| Maximum legacy-to-candidate KL, nats | 0.0000015246155737768286 | <= 0.00001 |
| Mean legacy-to-candidate KL, nats | 0.00000028787162687400694 | report only |
| Aggregate selected-index agreement | 0.9993896484375 | >= 0.9985 |
| Width-profile-weighted selected-index agreement | 0.9996061325073242 | report only |

The post-repair diagnostic passed every predeclared bound and emitted
`canonical_observed_workload_claim_valid: true`. That field is limited to the
sampler diagnostic against the provenance-bound workload shape;
`source_performance_claim_valid` remains false.

## Canonical benchmark evidence gate

The release benchmark measures quantization, table lookup, bounded Hamilton
apportionment with checked release postconditions, one SplitMix64 draw, and
inverse-CDF selection. Logit generation is outside the timed region, and each
worker reuses fixed scratch.

A canonical run is exactly five 2,000 ms repeats at one and 16 pinned threads.
It requires a runtime-supplied, repo-relative evidence manifest containing:

- the expected clean 40-hex revision;
- expected source-bundle, `Cargo.lock`, and toolchain SHA-256 values; and
- the expected benchmark executable SHA-256 and exact `rustc -Vv` stdout
  SHA-256;
- the exact Cargo build-command identity and the digest of the rejected
  build-override contract, with an empty override-state digest; and
- a previously absent JSON output under `evidence/fast_sampler/`.

The executable compares embedded bytes with on-disk bytes before and after the
matrix, requires the full tracked tree to remain clean, binds both attestations
to the manifest, compares executable, compiler, Cargo-command, override
contract, and override-state identities before and after the matrix, and
refuses a canonical run while the width profile is provisional. Rejected
overrides include Rust flags, encoded flags, compiler/wrapper selection,
build target, incremental mode, release-profile overrides, and target linker
or Rust-flag overrides. Arbitrary process names and host paths are never
serialized.
Endpoint interference records contain only fixed process-category counts and
normalized digests. GPU/WDDM inventory is explicitly non-gating because its
visibility is incomplete.

Every repeat takes aligned Windows system/process CPU snapshots around the
common timing window. It computes external CPU as system busy CPU minus the
benchmark process CPU, divided by total system capacity. Canonical evidence
fails closed if either alignment slack exceeds 5 ms or external busy exceeds
10% in any repeat. Affinity, timing, worker error, hardware, throughput,
manifest, source, and endpoint-process gates must also pass.

Canonical evidence is emitted only when every gate is true. Its privacy-screened
validation record has a recomputable SHA-256 field and is published with a
same-directory create-new temporary file, flush, `sync_all`, atomic hard-link
publication to the destination as the no-replace primitive, exact destination
byte readback, and mandatory temporary cleanup. Existing evidence is never
overwritten, including under a concurrent collision.

When any gate fails, the JSON summary written to standard output includes the
per-repeat timing and CPU-accounting diagnostics needed to identify the failed
gate, but it never writes or promotes a canonical evidence artifact.

The executable's manifest-template mode accepts only two distinct direct JSON
children of `evidence/fast_sampler/`. It creates `evidence` and
`evidence/fast_sampler` one component at a time, rejects traversal, repository
metadata, files, symlinks, and Windows reparse points, and verifies every
canonical component remains under the repository root. It refuses dirty or
uncommitted sampler source inputs, an unpinned compiler, or any rejected build
override. The generated template binds the already-built executable and
compiler before any timing run.

The prior benchmark binary/source hashes and rate table are superseded by this
audit repair. They are deliberately omitted because they do not satisfy the
runtime-manifest, external-CPU, privacy, or observed-workload contracts above.

## Validation sequence

```powershell
python python\tools\generate_fast_sampler_oracle_v1.py `
  --width-evidence data/rally_all_policy_legal_action_width_histogram_v1.json `
  --check
python -m unittest python.tests.test_fast_sampler_oracle_generator -v
cargo fmt --all -- --check
cargo clippy -p mtg-kernel --release --all-targets -- -D warnings
cargo test --workspace --release --all-targets
cargo test -p mtg-kernel --release --example bench_fast_sampler
cargo run -p mtg-kernel --release --example fast_sampler_diagnostic
```

Oracle regeneration, all nine focused Python generator/provenance tests, the
Rust diagnostic, formatting, strict release Clippy across all targets, the
full release all-target Rust suite, the allocation regression, all eleven
benchmark-contract unit tests, and release builds of the environment and
benchmark executable pass on the current worktree. An independent read-only
cross-check found no discrepancy in the d71dca82 workload-provenance binding.

The broader locked Python discovery suite is not recorded as passing: it
exceeded both 180-second and 600-second command ceilings without emitting a
test diagnostic. Only exact-uv 0.11.29, locked/Torch broad-discovery scope, and
those ceilings survived the orchestration handoff; the literal historical
argv and environment were not durably captured. Those attempts are therefore
non-authoritative and are not recorded as a broad pass. Sampler-specific
Python coverage passed with the exact focused command shown above.

For a future authoritative broad run, first execute
`uv sync --locked --extra test` with exact uv 0.11.29, then run
`uv run --no-sync python -m unittest discover -s python/tests -v` while
capturing the complete argv, environment contract, exit status, and output.
This is the repository's canonical future command, not a reconstruction of
the timed-out historical argv.

The exact source-commit to external-matrix sequence is:

```powershell
# 1. Start from the intended source commit and verify its tracked tree is clean.
git rev-parse HEAD
git status --porcelain=v1 --untracked-files=no

# 2. Build the bound executable from that clean commit with no rejected override.
cargo build --release --locked -p mtg-kernel --example bench_fast_sampler

# 3. Invoke the built executable directly to create the bound manifest template.
.\target\release\examples\bench_fast_sampler.exe `
  --write-manifest-template evidence/fast_sampler/run.manifest.json `
  --template-evidence-output evidence/fast_sampler/run.evidence.json

# 4. In the separately admitted external matrix, invoke that same executable.
.\target\release\examples\bench_fast_sampler.exe `
  --evidence-manifest evidence/fast_sampler/run.manifest.json
```

## Immutable canonical capacity result

The subsequent canonical matrix was built from clean source commit
`4692d7d931b253f32ad68e282acff37df6f61f7e`. Its manifest is
`evidence/fast_sampler/run2.manifest.json` (SHA-256
`2488401aa832022c5101697d53ca17f5381bd50d34ae00d5d46161de5fc457f8`),
and its immutable evidence is `evidence/fast_sampler/run2.evidence.json`
(file SHA-256
`fd1f1ddb5329ad825ec07cb1ff7722103ac935cdaccbc8d88f8ed329993c3d6f`,
canonical validation-record SHA-256
`35d0ae490c9ae5f2b5088bf739bcbcf2c2d47478b6a347ec4ab8512199ef1109`).

The bound executable SHA-256 is
`d7cd1930e5096744b04d6cb6b5086e33f316cd7d0c70667baa6975d9196878c3`;
the source-bundle SHA-256 is
`d7383ea8591592319691dbcddddc8f54f6047ab76a277b35ff8546ec4bc8ae8c`;
the `Cargo.lock` SHA-256 is
`19881eb38bda75d87f82f05edc7f7b41d5a87ea6f4bfa17ce775462ba30e2dc4`;
the toolchain SHA-256 is
`d52c5633ea77aefd345519d0a6c87e19c2636a1e90178585c30db481b3de9de0`;
and the exact `rustc -Vv` stdout SHA-256 is
`9a8a529345c2377f0558d65a1327aa81c7f327e88bdfaade069e0d5f73e82064`.
The Rally-derived observed-width artifact SHA-256 is
`d9471ee78ee8b656040d1920118f962f4b239e55603220e3679b1d11b847e579`,
and its canonical aggregate-record SHA-256 is
`33490dc1fbf21555cc469595beadbda70c30092ac95cac297bc6f0e48ef18f7c`.

The canonical parameters were five 2,000 ms repeats at one and 16 pinned
threads, with a target of 1,250,000 decisions/second. Observed results were:

| Threads | Minimum decisions/s | Median decisions/s | Maximum decisions/s | Minimum target multiple |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 20,286,121.685266145 | 20,494,573.554883800 | 20,579,096.075564817 | 16.228897348212914x |
| 16 | 190,334,831.105957597 | 190,915,808.754026413 | 190,999,278.205630004 | 152.26786488476608x |

All 19 claim checks were true across 85 workers with zero worker errors.
External busy ranged from `0.04799474030243261` to
`0.08733766233766234`; maximum start delay was `6,318,200 ns`, maximum
finish overshoot was `19,300 ns`, and maximum CPU-snapshot alignment slack
was `4,388,200 ns`. Every value remained within its predeclared bound.

This positive result is a Rally-derived legal-action-width sampler
microbenchmark only. It excludes model inference, environment stepping, IPC,
training, all-nine-deck completion, any XMage speedup, and any learning claim.
Broad Python discovery remains non-authoritative, and the all-nine-deck sampler
gate remains deferred.
