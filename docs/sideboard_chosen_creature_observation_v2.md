# Chosen-creature value observations, revision 2

Jack assigned this bounded observation repair while the independent native
screen continues on its frozen inputs. No screen configuration, outcome,
training Store or native campaign worktree is changed.

The prior V6/flat V3 mapping could give its value head the same input after
choosing Battlefield versus Hand for Monstrous Emergence. It also discarded the
paid creature's recorded power, which remains relevant after the exact chosen
incarnation leaves. The rules engine already stores and refreshes both facts;
this change projects them without altering game rules.

`PolicyObservationExtensionsV6` now requires:

- `pending_chosen_creature_cost`: null or `{source, controller, selected_zone}`.
  Only the acting controller receives this unfinished cost branch. No additional
  hand identity is exposed before revelation.
- `finalized_chosen_creature_costs`: ordered records
  `{stack_index, source, chosen, power_lki}`. Each record names one exact validated
  spell and its visible paid-cost reference. The original payment visibility
  mask is preserved. Public battlefield costs and actually revealed hand costs
  remain observable after later movement. Another incarnation cannot rewrite
  the recorded power. Monstrous Emergence has no intervening resolving-effect
  choice; these records cover its live stack/priority boundaries.

Rust validates the engine's independently bound cast metadata before projecting
the records. Flat and Python validation cross-check each record against its
declared stack item and visible payment. Python checks internal consistency,
not authenticity of a coherently forged engine history or invented power value.

Both typed records enter canonical value-state hashing. The existing
219/98/41/195/25 dimensions and 20 object groups are unchanged; no numeric column
is repurposed. Hash features convey a state distinction, not a claim that the
warm-start network already understands the magnitude of the power.

The feature identity is `actor-relative-v6-python-2`, registry revision 2 and
`actor-relative-node-graph-14`. The generated descriptor has schema
`flat-policy-v3-feature-contract-2`; its exact digests are in
`data/flat_policy_v3/feature_contract_v3.json`. Old V3 transfer pins fail. Existing
artifacts remain bound to their original feature identities. V5/V2 schemas,
Python features and golden case content stay unchanged.

Verification includes actual casting branch choices, a revealed hand cost,
live versus departed battlefield costs, and two departed states whose common
public observations are equal but recorded powers differ. Tests also reject
foreign spell/payment bindings, altered engine power and obsolete observation
shapes. The Rust emitter adds six cases for native/Python comparison across all
13 tensors. Python's 24 focused feature tests pass; coordinated Rust tests and
fixture parity are pending at implementation time.
