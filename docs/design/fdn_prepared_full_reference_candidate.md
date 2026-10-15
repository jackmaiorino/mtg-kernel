# Unqualified seven-printing Full reference candidate

Local preparation only, based on checked-PT `0b6224fb5` and its attributed
`a3b93d035` and `9f0f1835f` follow-ups. Do not publish or accept this candidate
until genuine gameplay qualification covers every printing and required branch.
All changed Rust and the new reference fixture remain uncompiled and unrun.

Exactly seven registry fields change: Elenda 444, Phoenix 445, Spinner 446,
Quandary 447, Hatcher 448, Conqueror 450 and Season 451 become `full`.
The prospective inventory is 452 definitions / 260 FDN records, with frozen
booster coverage 245 Full / 0 Partial / 41 Missing. Names, IDs, printing data,
mechanics, first 252 records, all other record fields, frozen 286+24 pool,
reference input bytes and enum/schema prefixes remain unchanged. Phoenix's
strict selector accepts exact Partial or Full shape; its existing
`full_engine_admitted` field reflects actual capability. Model, sampler and
search admission remain false. Existing old negative bodies remain except
capability-dependent admission expectations. Full-only reset is unchanged;
still-Missing Abyssal Harvester retains the unsupported-deck negative.

The new active pure-Limited fixture reads each existing 40-card `_partial.dck`
input verbatim, using the same input for both seats. The suffix is provenance,
not an invented alternate deck. It calls real public PreparedV57 and schema-5
resets and wraps an independently reset Prepared owner in HumanV57. It chooses
only issued London/land/cast/priority/combat/mandatory actions, genuinely casts
the printing with natural draws and mana, continues after entry, and replays
from the paid stack checkpoint through all three owners. It checks native and
safe index correspondence, full core/wire gameplay/count parity, Human current
input parity, foreign/stale/owning restore and unchanged legacy
refusal/opaque-representation behavior. There is no wire or world injection.
Existing genuine paid branch fixtures remain active and are still required;
this reference path does not replace their rules coverage or establish Full.

Python deck tests preserve historical hash literals by restoring only the old
capability fields in historical copies. They verify current Full resolution
and historical Partial refusal. Observed: 66 deck tests and seven Limited
session tests pass. Pinned Rust formatting and static/diff checks only.

No generated catalog hash or successor profile is guessed. Existing v87
`fdn_dual_full_batch_frozen_literal_matches_the_live_build_constant` and
`fdn_dual_full_batch_fixture_decodes_live_without_model_admission` remain stale
for this future 452 Full export. A retained actual export and coherent
successor profile are required before current Store publication/resume. Frozen
historical literals, classifier/ordinal prefixes and mutation guards remain
unchanged. No native launch, runtime, complete-model, search or strength claim.
