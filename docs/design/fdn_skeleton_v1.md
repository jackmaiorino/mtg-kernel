# FDN Reassembling Skeleton preparation

Reassembling Skeleton is a 1/1 Skeleton Warrior for {1}{B}. Its graveyard
activation costs {1}{B} and returns that card tapped to the battlefield
under its owner's control. The pinned
[XMage source](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/r/ReassemblingSkeleton.java)
supplies this contract. Tentative admission is v71, ID345, after v63-v70.

Reuse graveyard activation offer, mana payment and historical ability-source
contracts. Append `ReturnAbilitySourceFromGraveyard` to the effect enum,
checking the exact source, activation zone and zone-change count before
returning it, with the existing tapped-entry event. A returned graveyard
incarnation cannot satisfy an earlier activation. No Unearth exile flag or
delayed exile is installed. Its exact rules record includes the graveyard
move and tapped entry; earlier enum identities and profiles are preserved.

One primitive regression and five prepared card regressions cover both
players, payment/zone restrictions, tapped and untapped primitive variants,
pending-stack restore, repeated activations, exile/reentry, permanent return
and opponent-turn timing. They have not executed. Registry, fixture,
generated catalog/profile and gameplay qualification await serial admission.
