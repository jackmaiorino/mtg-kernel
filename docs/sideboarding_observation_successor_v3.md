# Sideboarding observation successor

This lane implements the three input-format gaps found during learned
sideboarding integration: a Map ability whose token source has ceased, a
chooser-only library search, and Sleep of the Dead's staged Escape cost.
Rich observation V6 and flat action/scoring V3 carry those facts explicitly.

Search candidates are a temporary unordered multiset. Only the chooser sees
them, with no library positions or hidden incarnation counters in model input.
Canonical public-card classes and visible selected-prefix roles determine
ordering. Each physical candidate retains its executable engine action.
Historical sources are authorized by their exact public stack or pending-effect
record. They are not inferred from the current contents of an arena slot.
Object costs carry their cast method, cost kind, selected prefix, required count,
and remaining count; the old sacrifice field remains sacrifice-only.

The producer validates the original engine decision, the full candidate set,
the exact source authority, and the current environment binding. Scorers receive
only actor-relative rows and row handles. A V3 binding cannot be consumed through
the V2 entry point. Failed encoding does not publish a partial output.

## Frozen play transfer

The Net8 weight and embedding arrays remain frozen. The successor retains the
219/98/41/195/25 state/object/edge/action/reference dimensions and 20 object
groups. Its feature identity is different, including for decisions with empty
extensions. A successful forward is evidence of executable transfer, not of
competent play on the new features or improved sideboarding.

Existing `run_batch` configurations default to the original contract. To use
the successor, add `play_observation_transfer_v3` with
`expected_feature_contract_digest` and `expected_feature_encoding_digest`
copied from `data/flat_policy_v3/feature_contract_v3.json`. The destination pins
are checked before the source export is loaded. The source export still passes
the original strict loader. The transfer receipt records both feature identities,
the Python authority source, descriptor hash, and unchanged weight identity.

The frozen `python/mtg_kernel_rl/features.py` and its feature digests stay
unchanged. Shared Rust implementation edits require refreshed implementation
source pins in the generated flat V2 inventory; the old tensor golden payloads
must remain unchanged. The build-time pin check remains enabled.

## Review boundary

Jack explicitly assigned these fixes. A fresh read-only Fable review was
dispatched as session `323e67e6-82c2-42db-b1ed-6df762f9d966`, but Claude returned
HTTP 429 for the weekly quota before reading any source. There is no Fable
endorsement. The recorded failure is
`C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/fable-observation-successor-review-001/`.
Implementation and bounded CPU verification continue under Jack's assignment;
independent Fable review remains an explicitly unresolved review limitation.
No GPU training, formal strength measurement, checkpoint promotion, or main-branch
merge is authorized by this note.
