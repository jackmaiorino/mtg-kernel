# Subtype cost reducers preparation

Issue110 prepares Arcane Epiphany ({3}{U}{U}, instant, draw three) and Claws
Out ({3}{W}{W}, instant, controlled creatures +2/+2 through cleanup) with the
existing generic cost reducer and spell effects. Two appended definition-only
queries count currently controlled permanents with an effective subtype or
test presence. Wizards reduce Epiphany once; every Cat reduces Claws Out.
Tokens, changelings, current control and effective type changes participate.
Only generic mana is reduced, flooring at zero; colored pips remain payable.
The existing rules-vector vocabulary records battlefield count or presence
without inventing a subtype facet. No state, effect or target variant is added.

Preparing these reducers exposed an existing Standard interaction: the normal
cost path floored a spell's own reduction before adding Thalia's tax. It now
applies battlefield increases and reductions before the spell's own reduction,
so excess reduction can offset the increase as required by cost ordering.
A registered Thoughtcast regression with six artifacts plus Thalia checks
this boundary and preserved colored mana. A separate runtime query case
covers both seats, tokens, changelings, ownership/control, zones and changed
effective subtypes. Existing reducer gameplay remains part of native checks.

Primary references are pinned XMage
[ArcaneEpiphany.java](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/a/ArcaneEpiphany.java)
and [ClawsOut.java](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/c/ClawsOut.java).
Tentative IDs362-363 follow Wardens and share the later coherent admission.
Four unregistered card cases cover exact metadata, actual payment for either
seat across zero to five qualifying permanents, failed colored payment and
rollback, recomputed payment after control/type/zone changes, draw three,
resolution-sampled team membership, reentry exclusion and stack restore.
Native verification, card gameplay and catalog admission remain pending.

Native sourcef2c60bb3 passed both subtype-cost runtime tests, including the
registered Thoughtcast/Thalia ordering regression, and four Affinity tests
(one existing ignored). The following combined-feature rules-vector check
failed two existing FDN-specific fixtures because Standard is the selected
registry when both flags are enabled. Their guards now follow that actual
catalog precedence; Limited-only retains both cases. The failed local-9 log
is preserved. Both rules-vector configurations and remaining type checks are
retried; full native and unregistered card qualification remain pending.
