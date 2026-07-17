# Extraction provenance

## Cutover checkpoint

The standalone repository starts its independent development line from this exact mapping:

| Role | Repository | Checkpoint |
|---|---|---|
| Source subtree | `jackmaiorino/mage`, path `kernel/` | `a5c90fe180021e70e2a644ade00eeab07f857a40` |
| Standalone tree | `jackmaiorino/mtg-kernel` | `8feceefa3213cce950876c1fb7c3668afb56f69c` |

At cutover, the source `kernel/` tree and standalone root both had Git tree ID `5d4b46fba6afbea03aeb500e24922640fc92557d`. A repository containing both objects can verify the mapping with:

```text
git diff --exit-code a5c90fe180021e70e2a644ade00eeab07f857a40:kernel 8feceefa3213cce950876c1fb7c3668afb56f69c
```

The standalone history is a path-filtered rewrite of commits that changed `kernel/`: the leading `kernel/` path was stripped, unrelated XMage files and changes were excluded, and the retained commit metadata and order were preserved. Rewriting paths and parent topology necessarily produced new commit IDs, so the pair above is the authoritative transition mapping rather than an ancestry claim or a squash commit. New kernel development continues in the standalone repository.

## XMage oracle boundary

Normal builds and tests use only tracked standalone files. XMage remains an optional reference/oracle input, not a runtime dependency.

The ignored `source_traces_match_fixture` test resolves paths from `data/xmage_counter_reference_windows_v1.json` against either:

1. `MTG_KERNEL_XMAGE_ORACLE_ROOT`, set to an XMage checkout or artifact root; or
2. `oracle/xmage` under this repository, when the pinned material is vendored there.

Relative environment overrides are resolved from the standalone repository root. The normal, non-ignored Rust test suite never requires either location.
