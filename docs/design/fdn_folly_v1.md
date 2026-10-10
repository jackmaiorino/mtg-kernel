# FDN Seeker's Folly preparation

Seeker's Folly is a sorcery for {2}{B}: choose target opponent discarding
two cards, or their creatures getting -1/-1 until end of turn. The pinned
[XMage card](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/s/SeekersFolly.java)
supplies the two-mode contract. Tentative admission is v75, ID349, after
v63-v74. The discard mode reuses existing spell-mode/target selection and
staged discard; the debuff is untargeted.

Append `BoostPlayerCreaturesUntilEndOfTurn`, sampling unique current
battlefield creatures controlled by the resolved player and retaining each
zone-change count. Reuse per-incarnation temporary-boost installation and
real cleanup expiry. Later entrants and returned incarnations escape the
modifier; a borrowed creature follows its current controller at sampling.
The exact recipe and rules facets retain player, stats, keywords and
until-end-of-turn duration. Existing effect discriminants remain unchanged.

One primitive regression and four prepared card cases cover both seats,
current control, noncreatures, later entrants, return/reentry, both modes,
opponent-only targeting, pending discard restore, invalid discard refusal,
empty/one-card hands, zero-toughness state actions, final-stack restore and
real cleanup. They have not executed. Registration, fixture, generated
catalog/profile and gameplay qualification await serial admission.