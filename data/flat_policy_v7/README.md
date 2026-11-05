# FlatV7 / PythonV10 feature contract

The FlatV4 contract plus London pregame decisions for the opt-in Pauper
London opening (`docs/design/pauper_london_mulligans_v1.md`, section 5).
`python/mtg_kernel_rl/features_v10.py` wraps the frozen `features_v7.py`
(FlatV4), which stays byte-identical: every in-game decision is encoded by
the V4 encoder itself, so in-game V7 tensors equal V4 tensors. A numeric
choice (`choose_effect_number`) is refused instead of aliased.

Net8 dimensions are **unchanged** from V4: state 219, object 98, edge 41,
action 195, action reference 25, and 20 object pooling groups. Shape equality
is not identity equality: the contract, encoding, schema and registry
identities all differ from V4's, and a V4 checkpoint is scored under V7 only
through the explicit V4-to-V7 transfer.

Pregame decisions (the observation carries `extensions.london_pregame` while
London is pending) reuse three V4 columns that no admitted pregame row sets:

- action-kind column 15 (V4 `choose_effect_number`) is `london_opening`;
- engine-stage slot 9 (V4 `halted`, state column 89) is `london_pregame`;
- the `has_lost` columns (state 30 self, 49 opponent) carry "opponent
  announced a mulligan this round" and "opponent has kept". In-game rows
  encode `has_lost` exactly as V4.

Announcement rows: column 69 (`VALUE`) is the mulligan flag; 63 own
mulligans and 66 opponent mulligans (over 16); 52 opponent has kept and 53
opponent announced a mulligan (0 or 1, over 16, the V4 scale of those
columns). Bottom rows: a `card` reference to the exact own hand object, 49
cards still to bottom (over 8) and 63 own mulligans. Each reused column keeps
its V4 scale. The action hash covers the canonical London semantic; the
state hash covers the canonical V4 observation plus `london_pregame` in actor
order, so only pregame state hashes change.

`pregame_parity_fixture_v1.json` holds actor-visible pregame observations and
menus of both seats with their Rust V7 tensors
(`flat_v7_pregame_parity_fixture_matches_rust_tensors`, rewritten with
`MTG_KERNEL_WRITE_FLAT_V7_FIXTURE=1`); `python/tests/test_features_v10.py`
checks the Python encoder against it.

Regenerate the pins with `python data/flat_policy_v7/generate_contract.py`.
Run `python -m pytest python/tests/test_features_v10.py -q` with
`python;python/tests` on `PYTHONPATH`. These checks are engineering
validation, with no playing-strength or training-result claim.
