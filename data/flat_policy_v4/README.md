# FlatV4 / PythonV7 feature contract

This is the Phase 1 fresh-lineage successor to FlatV3/PythonV6. It adds
exactly one thing on top of the V3 contract: a third
`historical_public_sources[].context` arm, `pending_trigger`, alongside the
existing `stack` and `pending_effect` arms, for a same-controller pending
trigger whose live object is not yet revealed. `pending_trigger` carries a
`position` field (`MODEL_INPUT`, `u32`), shaped exactly like `stack`'s
`stack_index` field. The frozen `python/mtg_kernel_rl/features.py` (V5) and
`python/mtg_kernel_rl/features_v6.py` (V3 contract generation) and their
identities remain separate and are never regenerated again; this contract
generation forks beside them, it does not replace them.

Net8 dimensions are **unchanged** from V3: state 219, object 98, edge 41,
action 195, action reference 25, and 20 object pooling groups. Adding a new
`VariantSpec` arm to `historical_public_sources[].context` does not add a
tensor column: the Rust producer (`add_validated_historical_source_v3` and
its V4 successor) pushes an ordinary `PendingContext` object/edge row
regardless of which context variant produced it, keyed only on
`(stable, actor, ordinal)`, never on the context enum's own shape. Reusing
Net8 weights across this bump requires explicit feature transfer; identical
dimensions do not establish unchanged checkpoint identity or learned
competence with the extension. **Shape equality is not identity equality**:
`feature_contract_v4.json`'s `dimensions` block is byte-for-byte identical to
`feature_contract_v3.json`'s, but `feature_contract_digest` /
`feature_encoding_digest` / `feature_schema_version` /
`feature_registry_version` / `model_contract_version` all differ, and a V3
checkpoint's identity constants must never be compared against V4's, or vice
versa.

The `historical_sources` encoding-contract description is corrected from V3's
(which named only the public-stack binding) to name all three context kinds:
`"exact-stack-pending-effect-or-pending-trigger-context-binding-not-json-provenance-authentication"`.
Every other encoding-contract entry, the `extension_edge_subroles` table, and
the five revision-2/3 `extensions` keys are otherwise unchanged from V3; see
`data/flat_policy_v3/README.md` for the full prose on search cards, historical
record binding, Ward relations, the `ChooseCreatureOrRevealCreature` cost
category, and the attacker-goad exclusion rule, none of which this generation
touches.

Wiring an actual `pending_trigger` producer (the Rust engine emitting a
`HistoricalSourceContextV7::PendingTrigger { position }` row for a hidden
same-controller pending trigger, and the corresponding V4 registry validator)
is separate, later, Rust-side work; this generation's Python/data files only
add the schema shape the producer will target.

Regenerate new pins with `python data/flat_policy_v4/generate_contract.py`.
Run the focused suite with `PYTHONPATH=python;python/tests` and
`python -m unittest python/tests/test_features_v7.py -v` on Windows
PowerShell, or `PYTHONPATH=<repo>/python:<repo>/python/tests python -m pytest
python/tests/test_features_v7.py -q` from a POSIX shell.
Fixture equivalence and privacy checks are engineering validation, with no
playing-strength, general deck compatibility, or training-result claim.
