# FlatV3 / PythonV6 feature contract

This successor explicitly adds decision-local library search cards, resolving
public source history, and the pending Escape object-cost prefix. The frozen
`python/mtg_kernel_rl/features.py` and its V5 identities remain separate.
Net8 dimensions stay state 219, object 98, edge 41, action 195, action reference
25, and 20 object pooling groups. Reusing weights requires explicit feature
transfer; these dimensions do not establish unchanged identity or learned
competence with the extensions.

The rich observation requires `extensions` with all three keys, using null or
an empty list when absent. Canonical state JSON always includes these fields.
Stable references contribute only card ID, actor-relative owner/controller,
and zone. The 96 existing hash slots encode the extensions; no layer widens.

Search cards equal the union of the active chooser's selected and legal library
targets. They contain no library positions. Sort cards by observable card
class, then selected-prefix index before unselected cards; operational ties
within an otherwise identical class are erased by model features. Sort legal
targets/actions by observable class before assigning action indices. Python
preserves the supplied canonical action indices. Identical copies share their
class ordinal. A card already exposed by actual prior library knowledge reuses
its existing node and retains that legitimately known position. The current
engine contract permits only the chooser's own library.

Historical records bind each nonspell public stack context and the active
resolving context exactly. Announced Hand-origin cycling is valid public
history. Detached sources do not claim a current arena incarnation. Missing
search nodes use PrivateContext/Private; missing historical nodes use
PendingContext/Pending. Register missing search nodes after common nodes, then
historical nodes. Exact captured facts are reused. A validated initiative
transfer may capture a controller different from the live permanent without
changing its arena or zone generation. That historical view gets a separate
node; the live node stays unchanged, and card ID, owner, and zone must agree.
This exception applies only to the declared historical source context.
Append self-edges
after common edges: cost source 32, selected prefix 33, search cards 34,
historical sources 35. Primary order is respectively zero, selection index,
public class ordinal, and record index; associated order is zero.

Public stack targets may retain Battlefield, Stack, or Graveyard provenance,
including a graveyard target that has since left that public zone. Such a
reference preserves the captured public facts and does not reveal its current
hidden destination. Hand and Library target provenance remain rejected.

Escape exposes the announced Stack source, original Graveyard cast origin,
required count, selected own Graveyard cards, and remaining count. The legacy
`sacrifice_chosen` projection stays empty. Prefix information reaches the value
state directly even though the value network does not pool legal actions.

For attacker selection, V3 removes exclusion whenever the current eligible
creature has an active visible goad requirement. This applies at every prefix,
so the last selection cannot inherit an earlier illegal omission. Python
checks the same include-only mask from the current public card's goad expiry;
ordinary attacker and blocker choices retain ordered exclude/include pairs.
The frozen V5 pair contract is unchanged.

The semantic cost vocabulary explicitly includes
`ChooseCreatureOrRevealCreature`. Its full name remains in the action hash;
the historical 11 cost one-hot columns are zero for this exact new category.
Unknown categories still fail, and action width stays 195. This enum addition
does not expose the separate pending chosen-creature zone or captured damage
power. Those value-state omissions require their own observation extension;
successful execution alone does not establish complete Markov coverage.

Python rejects inconsistent JSON and inappropriate references. It cannot
authenticate a mutually coherent fabricated public-history record. The Rust
producer establishes actual engine prefix validity, search scope, and frozen
public source provenance. Source and descriptor SHA-256 pins cover the Python
implementation separately from schema/encoding fingerprints.

Regenerate new pins with `python data/flat_policy_v3/generate_contract.py`.
Run the focused suite with `PYTHONPATH=python;python/tests` and
`python -m unittest python/tests/test_features_v6.py -v` on Windows PowerShell.
Compare actual native fixture output with `PYTHONPATH=python` and
`python -m mtg_kernel_rl.check_features_v6_parity PATH`.
Fixture equivalence and privacy checks are engineering validation, with no
playing-strength, general deck compatibility, or training-result claim.
