# Eternal native opening actions

`enable_eternal_opening_v1` opts a fresh native Eternal deal into London
mulligans and the following kept-hand window. The shared window starts after
both hands are kept and before the first untap. The starting player chooses
any remaining opening action from their own hand or finishes, then the other
player does the same. An empty hand or a hand without any eligible cards also
gets a finish action.

The native roster includes Leyline of the Void, Leyline of Sanctity and
Chancellor of the Annex. A combined catalog that exposes the exact supported
Leyline Axe definition could include it in the same window. In the current
builds the Eternal feature selects the Standard catalog and the exact FDN
Axe profile is unavailable. Leylines use actual hand-to-battlefield moves.
Chancellor uses the existing public reveal and first-spell delayed ability,
with consumption bound to the original cast and its trigger collection.
Serum Powder remains in the preceding London window. The explicit mode
suppresses the old first-upkeep offers and the separate Axe window.
Historical modes retain their original behavior.

The native continuation stores the original kept-hand bindings, selected
public outcomes and per-player finishes. Only the current player receives
their legal native object identifiers. The kept-hand journal is private
snapshot data. It is unavailable through the existing public observation
and infoset contracts, which refuse the explicit profile independent of
hidden hand composition. The unenabled mode omits the new state field and
retains existing event kind ordinals. The new native opening event is kind
317 in this branch.

The Eternal integration tests exercise both starting seats, player-selected
order, true London bottoms, declined-card privacy, native restore and the
first spell encountering the actual Chancellor trigger and Leyline of the
Void. Native execution and a genuine event sample harvest are still pending.
The Leylines remain Partial pending an admitted consumer and qualification.
