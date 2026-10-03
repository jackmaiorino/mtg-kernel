# V3 feature contract: frozen pin record (2026-09-15)

Produced under `FRESH-LINEAGE-CONTRACT-PLAN-001.md` Phase 1 (item 4). These three
files are the permanent V3 frozen baseline. Per the plan's fork-not-mutate
discipline (section 0/3), they must never be regenerated or edited again after
this record is committed; any future feature-contract change happens only in a
new generation (`data/flat_policy_v4/`, `python/mtg_kernel_rl/features_v7.py`)
that is additive beside these files, never a replacement of them.

Pinned at git commit `84c9ff2335278ca4574ecb78aed571679de2931b`
(`codex/learned-sideboarding-integration-v1`, worktree
`E:/mtg-kernel-learned-sideboarding-codex`, commit date 2026-09-15 19:08:17 -0400).

## Pins

| File | Bytes | SHA-256 |
| --- | --- | --- |
| `data/flat_policy_v3/feature_contract_v3.json` | 3490 | `6f87ba00e0f642611a10bec57f7aabbb550d52e3704144eb94b68da51c36f03b` |
| `data/flat_policy_v3/feature_identity.rs` | 739 | `834d654e0d797ec1ff292374c04311692d002fff84669a7d170496eccaf4764a` |
| `python/mtg_kernel_rl/features_v6.py` | 179572 | `df6d16cf9bc5889876ad001485363bb17e6ceb1504dcc9913b0ba5ec307b2143` |

Computed with:

```
python -c "
import hashlib
files = [
  'data/flat_policy_v3/feature_contract_v3.json',
  'data/flat_policy_v3/feature_identity.rs',
  'python/mtg_kernel_rl/features_v6.py',
]
for f in files:
    data = open(f,'rb').read()
    print(f, len(data), hashlib.sha256(data).hexdigest())
"
```

## Baseline regression gate

`python/tests/test_features_v6.py::FeaturesV6Tests::test_descriptor_and_rust_identity_match_actual_source`
(unittest-style test, collected and run via pytest) reads
`feature_contract_v3.json` and `feature_identity.rs`, asserts their recorded
`source_sha256`/`contract_digest`/`encoding_digest` match the live
`features_v6` module, and that the descriptor's own sha256 appears in
`feature_identity.rs`. Confirmed passing, unmodified, against the pins above.

Command that worked (the test module does a bare `from fixtures import ...`
against `python/tests/fixtures.py`, so `python/tests` must be on
`PYTHONPATH` alongside `python` itself; `pytest.ini`/`conftest.py` are absent
from this repo):

```bash
export TEMP=/e/tmp/lead TMP=/e/tmp/lead
export PYTHONPATH="$(pwd)/python:$(pwd)/python/tests"
python -B -m pytest python/tests/test_features_v6.py -k descriptor_and_rust_identity -q
```

Result: `1 passed, 27 deselected in 1.14s`.

## Scope note

This record is evidence only; it changes no code. Per the plan, any future
verification that "V3 never changed" should diff these three files against
`origin/HEAD` (or recompute the hashes above) rather than trust a passing test
suite alone, since a passing test proves internal self-consistency, not that
the constants were never regenerated in place.
