# FDN attack and draw trigger preparation

Battlesong Berserker (FDN78), {3}{R}, 3/4 Human Berserker, triggers once
when its controller declares at least one attacker, including when it stays
home. A targeted controlled creature receives +1/+0 and menace through
cleanup, bound to the same battlefield incarnation. Scrawling Crawler
(FDN132), {3}, 3/2 Phyrexian Construct artifact creature, makes each player
draw one at its controller's upkeep and causes untargeted loss of one life
per successful opponent draw. The upkeep controller draws first, followed by
the opponent. Empty-library attempts do not match the opponent-draw condition.

Both reuse existing trigger conditions, stack contracts and effect primitives.
No enum variant or state/continuation field is added. Tentative IDs359-360
follow the retained morbid/landfall preparation. The definition-owned trigger
recipe strings bind these semantics when catalog registration is qualified.
Primary references are pinned XMage
[BattlesongBerserker.java](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/b/BattlesongBerserker.java)
and [ScrawlingCrawler.java](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/s/ScrawlingCrawler.java).

Six unregistered card cases cover exact metadata/payment, both seats,
multiple/empty attack declarations, an idle Berserker as recipient, both
temporary effects/cleanup, source departure and target reentry, ordered upkeep
draws, one life-loss trigger per successful draw, own-draw and failed-draw
exclusion, untargeted placement, and finalized-stack/pending-order restore.
Source preparation does not change registry/profile/catalog identities or
accepted coverage. Native and card gameplay qualification remain pending.
