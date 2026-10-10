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
Construct is appended after the retained Elephant subtype, preserving all
accepted subtype IDs, and gains the existing creature-type/codegen mapping.
No effect variant or state/continuation field is added. Tentative IDs359-360
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

Source receipt, October 10: preparation196479b8 initially lacked Construct;
repair867a79ae725a66b6e3ea1a0bf3b2ef2c65a2b7bb appends it without shifting
accepted subtype IDs. Separate read-only review found no remaining actionable
defects. Formatting/diff checks pass. Haley's unstarted guard expired with
no_room after its whole-host reservation held for the full wait; no native
command executed there. Jack's eligible cores16-17 then admitted supported
guard9d511dd432b6475daa5f8d000da0ea97, supervisor29824, on a retained owned
checkout at exact867a79ae. It reuses the owned build cache and also checks
Limited-plus-Standard test compilation. Native results remain pending.

Native preflight at source867a79ae passed 48 card-definition tests, 28 effect
tests, Lunar Insight, a repeated Pilfer filter check, Madness and Duress.
Limited test compilation then failed: the fixture referenced nonexistent
Step::EndStep and state_hash_v4 APIs. Both new fixture files now use Step::End
and the existing full-state diagnostic_state_hash alongside state_hash.
The failed local-6 log is retained; remaining type checks are retried separately.
Unregistered card gameplay and catalog admission remain pending.

Repair a0f2ca6282dadf08c1058b6569698da69091b76b passed native cargo check
--tests with Limited alone (39.04 seconds) and Limited plus Standard (95
seconds), Rust1.94.1/MSVC19.50.35725. Supported guardce63fdf1949e45fab24384c80351be79
used cores16-17 and exited zero. Original failed source/log remain retained.
These compilation checks do not execute the unregistered card games.
