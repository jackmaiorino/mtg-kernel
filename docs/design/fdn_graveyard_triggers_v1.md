# FDN graveyard trigger batch

Elvish Regrower is a green 4/3 Elf Druid for `{2}{G}{G}`. Its mandatory
entry trigger returns one target permanent card from its controller's
graveyard to hand. Lands qualify; instant/sorcery cards, tokens and spell
copies do not. Ambush Wolf is a green 4/2 Wolf for `{2}{G}`, has flash, and
its entry trigger exiles zero or one target card from either graveyard.

Primary references are XMage `Mage.Sets/src/mage/cards/e/ElvishRegrower.java`
and `Mage.Sets/src/mage/cards/a/AmbushWolf.java`, checked against the retained
Scryfall FDN oracle pages. The batch reserves card ids 334-335 and Limited
catalog v64 after the v61-v63 prerequisites. Wolf reuses the already appended Standard subtype discriminant. Catalog hash qualification remains pending.

Append `PermanentCardInOwnGraveyard` (55) and reuse the already integrated
`UpToOneCardInGraveyards` (54), and reuse the existing incarnation-bound
`MoveAllTargets` hand/exile effects. Optional trigger completion uses the
existing target bounds and an explicit pending-trigger finish marker. False
is omitted from serialized state, preserving earlier required-target bytes.
Finishing is permitted only for the exact ordered trigger's legal optional
prefix with no competing choice producer. Required targeting, a second
target after the maximum and repeated completion refuse before mutation.

Gameplay regressions cover exact characteristics and mana payment, flash
outside sorcery timing, mandatory permanent/owner filtering and no-target
placement, optional refusal and acceptance from either graveyard, empty
graveyards, pending and answered-choice restore, historical source departure,
leave-and-return target incarnation fizzle, and public returned-card knowledge
without revealing another card in the opponent's hand. A 40-card reference
deck exercises deck import. Existing catalog profiles and measurements keep
their prior identities; this batch introduces no training or formal run.

October 10 source integration preserves Standard target IDs and subtype
support. The new finish marker has a manual Hash implementation: false
contributes no bytes to historical pending-trigger hashes; true adds a tagged
extension. Registration/Python admission are retained on the original batch
branch and will be copied after v63 admission so reserved card IDs 334-335
remain correct. This staging branch adds source and fixture tests only;
metadata admission and native qualification remain pending.
