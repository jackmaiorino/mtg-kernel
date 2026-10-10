# FDN Hidetsugu's Second Rite preparation

Hidetsugu's Second Rite is a {3}{R} instant targeting any player. If that
player has exactly ten life when it resolves, it deals ten damage to them.
The [pinned XMage source](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/h/HidetsugusSecondRite.java)
performs the life-total check at resolution. Tentative admission is v79,
ID354, after the preceding prepared families.

Append a player-target life-equality condition and reuse conditional execution
and ordinary noncombat damage. No new effect, target specification, state or
continuation is needed. Existing condition ordinals remain fixed. The rules
vector records the chosen player's life read and conditional ten damage;
exact life-total equality remains opaque in the fixed vocabulary.

One primitive and two card cases cover both seats, self/opponent targets,
life totals nine/ten/eleven, changes after casting, damage events, ordinary
payment, finalized-stack restore and lethal state actions. These cases remain
unexecuted. Registry, fixture, catalog/profile and gameplay admission await
serial predecessors; accepted coverage is unchanged.

Read-only review at `16636107` found no actionable defects in resolution-time
player selection, equality, damage, restore or lethal state actions. Formatting
and diff checks passed. Native qualification remains pending after the preceding
guarded prepared-source check completes; no card case has executed.
