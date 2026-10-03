# Phase 1 breadth block preparation

This tool prepares one fixed curriculum for the existing ordinary CPU/V3 trainer. It reads pinned catalog inputs and writes a schedule. It does not load models, initialize a lineage, play games, update checkpoints, evaluate a block or allocate compute. Campaign training remains paused.

## API and CLI

`catalog_v1.load_catalog_v1(catalog_source)` checks pinned catalog inputs and returns the compact catalog below. `schedule_v1.compile_block_v1(catalog, settings)` is pure and returns `{"native": ..., "report": ...}` without modifying its inputs. Direct callers remain responsible for catalog admission; the pure compiler cannot independently verify reserved groups, runtime evidence or field coverage.

From the repository root, the preparation command is:

```text
python -B python/tools/phase1_breadth_v1/compile.py --spec C:/YOUR_INPUTS/preparation.json --output E:/YOUR_OUTPUTS/fresh-block
```

The paths above are illustrative, not existing qualified inputs. The command requires a fresh output directory and produces `native-config.json`, `schedule-report.json`, `catalog.json` and a SHA-pinned `manifest.json`. It computes the catalog and schedule before creating that directory. Preserve any partial output after a write failure.

The preparation specification has exactly these fields:

```text
{
  "schema": "phase1-breadth-preparation/v1",
  "catalog": FilePin,
  "settings": FilePin
}
```

`FilePin` is `{"path": absolute_path, "sha256": lowercase_64_hex}` with optional exact integer `bytes`. The preparation specification and referenced catalog specification are limited to 1 MiB each; settings and each generated artifact are limited to 16 MiB. Duplicate JSON keys and nonfinite JSON numbers are rejected by the file reader.

## Catalog source

The referenced catalog specification has exactly these fields:

```text
{
  "schema": "phase1-breadth-catalog-source/v1",
  "scope": "engineering" | "campaign",
  "registry": FilePin,
  "qualification_runtime_sha256": lowercase_64_hex,
  "data": DataSource,
  "roster": [archetype_id, ...],
  "postboards": FilePin,
  "prior_reserved_groups": FilePin
}
```

`roster` contains 2 through 128 unique archetype IDs. `DataSource` is exactly one of:

```text
{"kind": "engineering", "registrations": FilePin, "provenance": description}
{"kind": "mtgo_snapshot", "bundle": FilePin}
```

Engineering registrations are explicitly described fixtures and cannot supply a campaign catalog. Their pinned file contains registration rows with unique `registration_id`, named-count `mainboard` and `sideboard` objects, the canonical `list_sha256`, `archetype`, `archetype_provenance`, and frozen `split` (`train`, `development`, `confirmation` or `unassigned`). Existing normalized snapshot rows may contain additional metadata.

A snapshot bundle is a list of five pins named `inventory.json`, `events.json`, `registrations.json`, `splits.json` and `snapshot.json`. The loader checks their event/registration joins, eight complete calendar weeks, denominator accounting, pinned spelling aliases, and the existing grouped split rule. It does not invent missing registrations or erase earlier training exposure. The authoritative detailed snapshot checks are in `catalog_v1.py`.

The other two referenced documents are:

```text
{"schema": "phase1-postboard-configurations/v1",
 "configurations": {exact_zoned_list_sha256: [
   {"mainboard": {card_name: count, ...}, "sideboard": {card_name: count, ...}}, ...
 ]}}

{"schema": "phase1-reserved-list-groups/v1",
 "groups": [{"list_sha256": lowercase_64_hex,
             "combined75_sha256": lowercase_64_hex}, ...]}
```

Only admitted training groups may appear in the postboard document. Their exact zoned configurations and combined 75 must avoid prior reserved groups and current development/confirmation groups. Each supplied postboard configuration preserves its registered 75. Different training zonings of the same 75 are allowed; duplicate exact zoned training rows are aggregated into multiplicity by the loader. Native card IDs come from the pinned registry's array order. Static admission requires declared Full support, exact 60/15 sizes and the copy limit, with the Basic exception.

Campaign admission additionally requires a reconciled complete inventory, the intended registry/runtime, existing pinned `phase1-registration-qualification/v1` reports for `complete-bo3-registration`, and at least 90% qualified registration coverage in the selected roster. These are metadata/evidence checks, not a new execution of those qualification games. Synthetic unit-test reports are not actual qualification evidence.

The compact catalog passed to the pure compiler has exactly this shape:

```text
{
  "scope": "engineering" | "campaign",
  "provenance": {opaque_metadata: ...},
  "archetypes": [{
    "id": archetype_id, "field_count": positive_integer,
    "train": [{"id": variant_id, "multiplicity": positive_integer,
               "registered": ExpandedDeckList,
               "postboard": [ExpandedDeckList, ...]}]
  }]
}
```

`ExpandedDeckList` has exactly `label`, `mainboard` (60 integer card IDs) and `sideboard` (15 integer card IDs). Labels do not distinguish otherwise identical card configurations. Provenance retains the actual field basis, missing/excluded mass, input pins and qualification limitations.

## Block settings

Settings have exactly these fields:

```text
{
  "schema": "phase1-breadth-block/v1",
  "lineage_id": nonempty_string,
  "block_index": nonnegative_u64,
  "seed": u64,
  "phase": "preboard" | "mixed",
  "initial_source": ExpandedModelSource,
  "opponents": [{"id": opponent_id, "source": ExpandedModelSource}, ...],
  "policy_pool": [{"id": policy_id, "weight": positive_integer,
                   "assignment": Assignment}, ...],
  "opponent_archetype_weights": [{"id": archetype_id,
                                  "weight": positive_integer}, ...],
  "learning_rate": positive_binary32_value,
  "value_coefficient": positive_binary32_value,
  "collection_workers": integer_1_through_1024,
  "preparation_workers": integer_1_through_32,
  "output_directory": absolute_path,
  "excluded_seeds": [u64, ...]
}
```

`ExpandedModelSource` contains `play_import: ModelPin`, optional/null `checkpoint: ModelPin`, and `feature_transfer` with exactly `expected_feature_contract_digest` and `expected_feature_encoding_digest`. Both digests are lowercase SHA-256 strings. A `ModelPin` contains exactly `path` and `sha256`, with an absolute path. The compiler preserves supplied sources and optimizer values; existing checkpoints carry their actual parameters and Adam state through the native continuation loader. Compiling several named lineages from one source creates no independent initialization.

`Assignment` is one of these exact objects:

```text
{"kind": "current"}
{"kind": "initial"}
{"kind": "fixed", "id": opponent_id}
{"kind": "recent_completed", "lag": positive_integer, "fallback": "initial"}
```

Fixed IDs must exist in the supplied opponent roster. At iteration `i`, a recent assignment becomes `completed_iteration` at `i - lag` when available, otherwise `initial`. These indexes are local to the block; `initial` means that block's initial source. Prior-block opponents can be supplied as fixed pins. Opponent archetype weights must cover every catalog archetype exactly once. Archetype choice and policy choice are separate inputs.

## Deterministic allocation

For `K` archetypes, one block has `max(200, 20*K)` updates and ten fresh games per update. Exactly five slots per update have field origin and five have uniform origin. Each branch first allocates its block totals with integer Hamilton rounding, breaking remainder ties by archetype ID, then independently shuffles its allocation. Field probabilities are conditional on supplied catalog counts; missing or excluded field mass remains in provenance.

Training variants are sampled by registration multiplicity. Opponent archetypes and policies use their explicit integer weights. Preboard games use registered configurations. Mixed blocks independently shuffle five preboard and five postboard slots per update, and require at least one supplied postboard option for every variant. Postboard options are sampled uniformly. An explicitly supplied Keep configuration is valid; reports distinguish actual changed and unchanged card zones for both players. No fitted sideboard head is called.

The four learner-seat/relative-start roles have equal block totals and a separate shuffle. Branch, phase, role, variant, opponent, policy, postboard and physical-seed streams have separate SHA-256 domains. Rejection sampling avoids modulo bias. Exact marginal quotas do not imply exact joint balance: the report includes the full branch × phase × learner-seat × relative-start contingency, matchup counts and per-episode assignments.

`excluded_seeds` must contain all earlier physical seeds the caller intends to exclude. A colliding or excluded candidate is discarded and the next candidate is drawn deterministically. The report records the rejection count and emitted seeds. This allocates seeds before play; it never replaces a failed episode. Identical inputs reproduce identical output. The report names the exact canonical JSON encoding used for `native_sha256`.

Every episode has fixed limits of 100,000 physical decisions and 200,000 policy steps. The existing trainer requires naturally completed games; a cap/error stops that iteration. Ten ordinary games must not be relabeled as ten BO3 matches.

## Validation and remaining scope

A newly built native binary with the validation option can check the emitted DTO:

```text
native_expanded_training_run_v1.exe --validate-config E:/YOUR_OUTPUTS/fresh-block/native-config.json
```

This checks strict parsing, registered-card/configuration legality, schedule references, dimensions and source-pin syntax against that binary. It does not follow model/checkpoint pins, restore Adam, verify runtime evidence or CUDA availability, create output directories, or play games. Old frozen binaries are unchanged. Actual execution and checkpoint compatibility still need separate checks.

The compiler does not establish full-field admission, unseen holdouts, fresh independent initializations, block-boundary evaluation, forgetting diagnosis, cloud speed or playing strength. It stops at one prepared block. Those remaining connections and actual qualification results must be reported separately.
