# FDN Lunar Insight preparation

Lunar Insight is a {2}{U} sorcery drawing one card for each distinct mana
value among nonland permanents its controller controls. The
[pinned XMage definition](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/l/LunarInsight.java)
samples the battlefield when it resolves. Tentative admission is v78,
ID353, after the preceding prepared families.

Append one dynamic value and reuse `DrawCardsDynamic`. Scan actual battlefield
objects by current controller and effective land type, then deduplicate their
definition-owned mana values. Tokens participate, copied definitions retain
their copied costs, and X is zero off-stack. Cost reductions and alternate
payments do not change mana value. Sampling finishes before any draw commits.
No continuation or state field changes. Rules facets expose the controlled
battlefield characteristic read and dynamic draw, with opaque marking for the
fixed vocabulary's missing nonland exclusion and distinct-value grouping.

One primitive and three card cases cover both seats, duplicate values,
zero-mana tokens, artifact lands, control changes after casting, ordinary costs,
zero draw and finalized-stack restore. They remain unexecuted. Registry,
fixture, catalog/profile identity and gameplay admission remain pending.

Read-only review at `a1c31fa1` found no actionable defects in current control,
effective land filtering, distinct-value grouping, resolution sampling or
restore. Unused fixture imports were removed in `5f8139c5`. Supported two-core
native checks use that exact committed source on Haley's PC; the result is
pending. This checkout also contains the fight fixture type repair `6876b395`.
