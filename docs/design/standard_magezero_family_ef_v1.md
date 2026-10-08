# MageZero Standard families E and F

Add the MageZero Standard catalog's non-creature permanent frameworks
(family E) and its planeswalkers, transforming legends and big spells
(family F) behind the `standard-magezero-fixtures` feature. Card behavior
follows each card's XMage source. Every program is selected by printed
card name through `build_standard_v1.rs` and `src/standard_cards_v1.rs`,
so the Pauper and FDN catalogs keep their identities and hashes.

## Planeswalkers and attacks

Loyalty abilities are sorcery-speed activated abilities with a
`Loyalty(delta)` cost, once per turn per permanent. Teferi, Temporal
Pilgrim's -12 uses the resumable interpreter for the opponent's choice.
Creatures may attack planeswalkers (506.3, 508.1b). When the defending
player controls one, declaring attackers asks the attacking player, one
attacker at a time, for the player or a planeswalker; `ChooseAttackTarget`
is appended to `Decision`, `Action` and `ActionSemanticV1`, and flat
scorers refuse it. Each attack records the planeswalker's exact
incarnation. Unblocked and trample damage goes to the attack target; a
planeswalker that left the battlefield receives nothing (506.4c). Without
a defending planeswalker the extension stays absent, so earlier state
hashes and decision streams are unchanged. Any-target spells may target
planeswalkers (115.4).

## Transforming legends

Transform works in both directions and restores the front face's colors,
subtypes and name. A permanent leaving the battlefield while showing a
back face records that face (`LeftBattlefieldFaceV1`), so a leave trigger
printed on one face sees the face that left. Cecil, Dark Knight loses life
equal to the damage it deals and transforms at half life; its back face
gives other attackers indestructible. Polukranos Reborn transforms for
{6}{W/P}, and its back face makes two Phyrexian Hydra tokens whenever it
or another nontoken Hydra you control dies. Ojer Axonil raises red
noncombat damage to opponents to its power, returns as Temple of Power
when it dies, and Temple of Power transforms back after four noncombat
red damage in a turn. A back face that is not a creature cannot attack and
its mana abilities are not summoning sick.

## Blue Sun's Twilight

The kernel announces targets before X. A creature is a legal target only
if some payable X reaches its mana value, and X is then at least that
mana value. The spell gains control of the target and, at X 5 or more,
creates a token copy of it.

## Rooms

Unholy Annex // Ritual Chamber is one definition. Its left half is the
normal cost and its right half the alternative cost, so the existing
cast-mode choice picks the half. As the spell resolves it enters and the
cast half's door unlocks (709.5). The other door unlocks through a
special action at its half's mana cost and sorcery timing: the cost is
paid like an activated ability, but nothing goes on the stack and the
activator keeps priority. Each unlock commits `RoomDoorUnlockedV1`, which
"when you unlock this door" abilities trigger on, including the unlock on
entry. Abilities printed on a locked door do not function. A Room put onto
the battlefield without being cast has both doors locked. Door state lives
in `GameState::standard_v1` per exact incarnation.

## Known limits

The stack and battlefield use the Room card's combined mana value and
name rather than the unlocked halves'. Sacrifice bindings, Bargain and
collect evidence fail closed on a back face. The legend rule counts
Temple of Power as legendary. Ninjutsu attackers attack the player.

## Catalog identity

Each batch appends its cards and tokens to
`data/standard/magezero_v1/cards_v1.json` and updates the Standard
identity: the `kernel_carddb_standard` version, the frozen hash and the
`STANDARD_APPENDED`/`SUPPORTED_NONBASIC` lists in Rust and Python.

Validation lives in `tests/standard_magezero_family_ef_v1.rs`, alongside
the Pauper and FDN regression suites. Implementation is routine
engineering; no research design, measurement, training or paid compute
changes here.
