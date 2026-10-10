# FDN An Offer You Can't Refuse preparation

An Offer You Can't Refuse is an instant for {U}: counter target noncreature
spell; its controller creates two Treasure tokens. The pinned
[XMage card](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/a/AnOfferYouCantRefuse.java)
and `CounterTargetEffect` / `CreateTokenControllerTargetEffect` supply the
contract. Tentative admission is v74, ID348, after v63-v73.

Reuse noncreature spell targeting, X-independent casting/payment and the
shared physical, flashback and virtual-copy departure path. Append
`CounterTargetSpellThenCreateTokens`: verify the target's exact live stack
incarnation, capture its current stack-item controller before countering,
then create tokens for that controller even if the spell cannot be countered.
A stale/illegal target cannot earn tokens. Validate the fully executable
token definition before mutating; halt during departure prevents later work.
The exact program records the current controller and independent reward;
rules facets include the counter, token recipient/count and token dependency.
Prior enum discriminants and profiles remain unchanged.

Two primitive regressions and four prepared card cases cover both seats,
current controller overriding historical target-controller evidence,
uncounterable spells, stale targets, own/opposing noncreature spells,
creature rejection, pending targeting and final-stack restore. The card
cases await registration, fixture, generated identity/profile and serial
native/hosted gameplay qualification. None of these cases has executed.
Read-only review at `77d9c1df` confirmed the Koma mana-fixture and
owner-bound counter-departure facet repairs. No actionable findings remain.
Native/card tests remain unexecuted.
