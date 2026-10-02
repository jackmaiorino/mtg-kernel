# FDN Kiora, the Rising Tide

Kiora is a legendary blue 3/2 Merfolk Noble for `{2}{U}`, a creature in
both the original pinned UG fixture and XMage's source. Entry draws two
cards, then requires its controller to discard two cards. Attack has an
intervening threshold condition: seven or more cards in its controller's
graveyard at both trigger creation and resolution. On resolution the
controller may create Scion of the Deep, a legendary 8/8 blue Octopus
creature token with zero mana value and no additional abilities.

Reference pin: XMage `a5c90fe180021e70e2a644ade00eeab07f857a40`,
`Mage.Sets/src/mage/cards/k/KioraTheRisingTide.java` and
`Mage/src/main/java/mage/game/permanent/token/ScionOfTheDeepToken.java`.
This batch follows Koma PR #131 and preserves both original 40-card decks.

Reuse the engine's ordered draw/discard continuation, generic optional
choice and legend rule. Append Merfolk and Octopus subtype IDs without
renumbering any existing observation subtype. Use one graveyard-card
count predicate at both threshold checks; tokens and spell copies do not
count as cards. Bind optional creation to the queued trigger's controller,
including when Kiora has departed. The optional decision is offered only
after the resolution-time threshold check succeeds.

Required checks are exact printed characteristics and three-mana payment;
ordered two-card draw then two-card discard, short-hand and empty-library
behavior; no attack trigger at six cards, an optional trigger at seven,
refusal and acceptance; dropping below threshold before resolution; an
attack below threshold does not gain a trigger merely by crossing seven
later; source departure and pending-choice restore; and duplicate Scion
legend choices. Include focused XMage counterparts, catalog/default
compatibility, prior gameplay, external natural-terminal replay and CI.

The initial nine gameplay checks passed on HaleysPC in
`fdn-kiora-checks-002`, including pending discard, optional token and legend
restore. The first runtime attempt exposed a missing attack marker: the
engine emitted attack events only for its older attack condition. The
emitter now recognizes Kiora's threshold condition too. The failed attempt
is retained. Generated FDN v43 hash is `5a8469de3061a1fb`.

The next verification batch covers the extended gameplay checks, v43
catalog registration, read-only Koma v42 history and mutation refusal,
unchanged default compatibility and relevant earlier gameplay. XMage,
external natural-terminal replay and hosted CI remain pending.
