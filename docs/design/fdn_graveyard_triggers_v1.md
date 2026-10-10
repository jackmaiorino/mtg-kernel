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

Append `PermanentCardInOwnGraveyard` (57), preserving accepted Standard
land target IDs55-56, and reuse the already integrated
`UpToOneCardInGraveyards` (54), and reuse the existing incarnation-bound
`MoveAllTargets` hand/exile effects. Optional trigger completion uses the
existing Standard optional-target placement and immediate stack admission.
No pending-trigger field, hash change or serialization extension is needed.
Trigger finishing uses the exact ordered trigger's legal optional prefix.
An active interpreter's finish answer retains precedence over a waiting trigger.
Required targeting, a second
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
support. Current main already has the generic optional-trigger finish primitive;
integration reuses it and retains the original derived PendingTrigger hash.
Registration/Python admission are retained on the original batch
branch and will be copied after v63 admission so reserved card IDs 334-335
remain correct. This staging branch adds source and fixture tests only;
metadata admission and native qualification remain pending.

The staged source and integration tests compile under Rust 1.94.1 with the
Limited feature at `275709b4`; this is a type check, not gameplay qualification.
The active-effect precedence fixture was then aligned with the reused Standard
behavior. Gameplay execution still awaits registration.

October10 admission preparation begins from accepted defaultbc23a8da after
PR208 merged and its33 affected Python cases passed. The candidate appends
only IDs334-335, adds its exact40-card import case and focused CI target, and
reuses the source-reviewed current optional-target engine primitive without
adding a PendingTrigger field. Candidate coverage is136 full, one partial and
149 missing; accepted coverage remains134 until gameplay and profile checks
pass and this batch merges. Native catalog generation supplies the new hash
before any live-profile pin is finalized. No paid/formal run is started.
