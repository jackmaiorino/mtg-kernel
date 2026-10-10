# FDN fight cards preparation

Affectionate Indrik is a 4/4 Beast for {5}{G}, with an ETB trigger that may
fight target creature its trigger controller does not control. Bushwhack
is a {G} sorcery choosing a revealed basic-land search to hand then shuffle,
or a controlled creature fighting an opposing creature. Pinned XMage
[Indrik](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/a/AffectionateIndrik.java)
and [Bushwhack](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/b/Bushwhack.java)
define these contracts. Tentative admission is v77, IDs351-352, after v63-v76.

Append `FightObjects`, using existing target specifications and incarnation
contracts. Both objects must still be legal battlefield creatures; a source
operand must be the live incarnation captured by its ability, with no
last-known-power fallback. Sample both current nonnegative powers before
any damage, then commit reciprocal noncombat damage simultaneously through
the shared replacement/lifelink/deathtouch path. State actions run after the
whole effect. Fighting itself deals two packets; nonpositive power deals
none. Indrik reuses the existing optional effect choice after target placement;
Bushwhack reuses basic-land search and spell modes. Existing enum variants,
continuation wire shapes, definitions and profile identities stay intact.

Three primitive and seven card cases are prepared for both seats, exact
metadata/costs, optional decline/acceptance, target legality, departed source,
pending/answered restore, search failure/reveal, resolution-time powers,
reciprocal damage, keyword handling, self-fight, nonpositive power, stale
incarnations and changed control/type. Rules facets record both power reads
and damage directions; exact reciprocal binding/simultaneity is opaque in
v1's fixed vocabulary. The cases have not executed. Registry, fixture,
generated catalog/profile and gameplay qualification await serial admission.