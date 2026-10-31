# FlatV8 / PythonV11 feature contract

The V8 inclusion contract on FlatV7 (collab
`LANES/w4-combat-20261030/DESIGN.md` v5.1, section 5). W4 Step A nominated
P: include and exclude rows of an inclusion decision differed only in the 96
action-hash coordinates (include-flag probe AUC 0.53), and earlier blocks of
an in-progress declaration were explicit only through edge order and the
state hash. `python/mtg_kernel_rl/features_v11.py` wraps the frozen
`features_v10.py` (FlatV7), which stays byte-identical, and adds exactly two
things:

- **Include bit.** One action column appended to every action row (index
  195, so the action width is 196): 1.0 on the `include: true` row of every
  attacker-inclusion and blocker-inclusion decision, 0.0 on every other row
  (exclude rows, every other action and every London pregame row).
- **Pairing relation.** `pairing_node_indices`, one `(blocker node, attacker
  node)` row per block already chosen in the pending blocker declaration:
  each pair of `surface_context.private_blockers.accumulated` (blocks for
  earlier attackers), in order, then each blocker already selected for the
  current attacker (`policy_surface_context.private_combat_selection` at a
  blocker-inclusion decision), in order. It is empty at every other decision.

Nothing else is added: no "would die", "good block" or other value judgment,
and no new card reference. Every other tensor equals the FlatV7 tensor bit
for bit, London pregame decisions included.

The model is the V8 inclusion net (`native_policy_value_net_v1::
v8_inclusion_v1`): net-8 with `action_encoder.0.weight` one column wider and
an additive pairing path (two object-encoder outputs in, 64 tanh units, a 64
output projection added into both nodes' pooled edge messages under the
stack-input zero guard). A V4 or FlatV7 checkpoint converts to it by the
function-preserving conversion (`native_policy_train_step_v1::
v8_inclusion_v1`: zero include column, seeded `torch_default_uniform_v1`
pairing fan-in, zero output projection, zero new moments, `adam_step` kept);
`v8_inclusion_convert_v1` (an example binary) converts and checks the
preservation on real games, with a JSON receipt.

`blocker_parity_fixture_v1.json` holds actor-visible blocker declarations with
pending blocks and their Rust V8 tensors
(`flat_v8_tensors_include_bits_and_pairs_match_the_sessions`, rewritten with
`MTG_KERNEL_WRITE_FLAT_V8_FIXTURE=1`); `python/tests/test_features_v11.py`
checks the Python encoder against it.

Regenerate the pins with `python data/flat_policy_v8/generate_contract.py`.
These checks are engineering validation, with no playing-strength or
training-result claim.
