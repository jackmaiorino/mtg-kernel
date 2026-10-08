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

## Exile until this leaves, Auras and Equipment

Seam Rip, Dusk Rose Reliquary, Sheltered by Ghosts and Hardlight
Containment reuse Journey to Nowhere's linked exile: an entering trigger
exiles its target until the source leaves, and a leave trigger returns it.
Their targets use `TargetSpec::StandardV1`, a one-object filter owned by
the Standard module (opponent's nonland permanent with mana value at most
2, opponent's nonland permanent, opponent's artifact or creature, artifact
you control, and later an instant or sorcery card in your graveyard); its
stable id 55 follows the ids 42 to 54 that the FDN and other Standard
batches claim. Sheltered by Ghosts ("enchant creature you control") and Hardlight
Containment ("enchant artifact you control") enter attached to their
target; their enchant restriction, control included, is checked as a
state-based action (704.5m). Sheltered by Ghosts' +1/+0 and lifelink use
the static attachment profile Equipment uses, and both Auras grant ward
to the enchanted permanent through the existing ward trigger. Dusk Rose
Reliquary sacrifices an artifact or creature as an additional cost and has
ward {2}. Basilisk Collar is Equipment with equip {2} granting deathtouch
and lifelink.

## Turn watchers, anthems and Cases

`GameState::standard_v1` records, per turn, the life each player gained
and lost (paid life included, 119.4) and the creatures declared as
attackers. Lunar Convocation's two end-step abilities check those records
when the end step begins and again on resolution (603.4); its {1}{B}, pay
2 life ability draws a card. Warleader's Call gives creatures you control
+1/+1 and deals 1 damage to each opponent whenever a creature you control
enters. Simulacrum Synthesizer scries 2 on entry and makes a Construct
token whenever another artifact you control with mana value 3 or more
enters; the Construct gets +1/+1 for each artifact you control. Candy
Trail scries 2 on entry and sacrifices for 3 life and a card. These
power and toughness changes are read alongside the existing static boosts.

Case of the Gateway Express's entering ability has each creature you
control deal 1 damage to target creature you don't control, as one
simultaneous batch. At the beginning of your end step, if three or more
creatures attacked this turn and it is unsolved, it becomes solved (719.3,
719.4), and while solved creatures you control get +1/+0. Solved state
is kept per exact incarnation, so a Case that leaves and returns is
unsolved.

## Classes

Innkeeper's Talent and Stormchaser's Talent enter at level 1 (716.3).
Their level-up abilities are ordinary sorcery-speed activated abilities
that use the stack and can be activated only at the level before them
(716.2a); the level lives in `GameState::standard_v1` per exact
incarnation, and each gain commits `ClassLevelGainedV1` for "when this
Class becomes level N". The beginning of combat step commits
`BeginningCombatV1` under the Standard feature for "at the beginning of
combat on your turn". Innkeeper's Talent level 2 gives each permanent you
control with a counter on it ward {1} through the existing ward trigger,
and level 3 doubles the +1/+1, lifelink and stun counters you put on
permanents, once per such Class (616.1). Stormchaser's Talent makes a 1/1
Otter with prowess on entry, returns an instant or sorcery card from your
graveyard at level 2, and makes an Otter whenever you cast an instant or
sorcery at level 3. Prowess is a trigger on the token, as in the keyword
batch.

## Case of the Uneaten Feast

Whenever a creature you control enters, you gain 1 life. It is solved at
your end step if you gained 5 or more life that turn. Once solved, you may
sacrifice it: each creature card then in your graveyard may be cast from
there this turn. The grant names each card's exact graveyard incarnation
and turn, and such a cast uses the appended `GraveyardPermissionV1` cast
route, paying the card's normal costs.

## Liliana of the Veil

Liliana of the Veil's +1 has each player discard a card: the controller
chooses first, then the opponent's discard is staged once the first one
finishes, so the ability stays on the stack until both are done. Its -2
reuses the edict sacrifice for target player. Its -6 is resumable: the
controller selects the first pile from every permanent the target player
controls (the rest form the second pile), then that player chooses a pile
and sacrifices it as one batch. Both answers carry guards naming the exact
incarnations in each pile, and each prompt is checked against the
definition's operation at its structural path.

## Breach the Multiverse

Each player mills ten cards, the controller first. Then, for each player in
turn order whose graveyard holds a creature or planeswalker card, the
controller chooses one of those cards (not targeted) through the resumable
interpreter, and the chosen cards enter the battlefield under the
controller's control together. Each creature the controller then controls
becomes a Phyrexian in addition to its other types; the affected set is
fixed at resolution (611.2c) and recorded per exact incarnation in
`GameState::standard_v1`, and the effective subtype queries read it.

## Fable of the Mirror-Breaker

Fable is a Saga on the existing lore-counter path (714.3b) whose chapters
come from the Standard module. Chapter I makes a 2/2 red Goblin Shaman
token whose attack trigger makes a Treasure. Chapter II lets its
controller choose up to two cards in hand through the resumable
interpreter, discard them together and draw as many. Chapter III reuses
The Modern Age's exile-and-return-transformed operation. Saga chapter
programs now count as definition-owned triggers when a resumable choice
is validated (714.2b). Reflection of Kiki-Jiki's {1}, {T} ability targets
another nonlegendary creature you control; the module's target filters
now receive the targeting source so "another" can exclude it. The token
copy has haste as part of the copy, recorded per exact incarnation and
read by the keyword query, and a delayed "sacrifice it at the beginning of
the next end step" trigger (603.7) fires from the next `BeginningEndStep`.
The stack accepts that trigger only for a token Reflection created.

## Repurposing Bay

Repurposing Bay's sorcery-speed ability costs {2}, {T} and sacrificing
another artifact. The appended `PermanentFilter::AnotherArtifact` matches
artifacts like `Artifact` but excludes the ability's source from the cost
candidates, the affordability check and payment validation. At
resolution the search filter is an artifact card with mana value one more
than the sacrificed artifact's, read from the payment's frozen
`PaidCostRefV4`, and the search reuses the existing search-to-battlefield
prompt and shuffle; this origin puts the card onto the battlefield
untapped.

## The Irencrag

The Irencrag taps for {C}. Whenever a legendary creature you control
enters, its controller may have it become Everflame, Heroes' Legacy: a
resumable yes-or-no choice whose answer is guarded by the exact
incarnation. Everflame is recorded per incarnation in
`GameState::standard_v1`; it renames the object (layer 3), adds the
Equipment subtype to the effective subtype queries, turns off the mana
ability and the trigger, and turns on the printed equip {3} with
"Equipped creature gets +3/+3" as the card's equipment profile, which
only an Everflame can be attached by.

## Craft

Craft with artifact (702.167a) is an activated ability, from the
battlefield and at sorcery speed, whose cost is the craft mana, `ExileSelf`
and the appended `CostComponent::ExileCraftArtifactMaterial`: one other
artifact the activator controls or one artifact card in their own
graveyard. The material is staged through `Decision::ChooseCostTargets`
like the other activation object costs, bound to its exact incarnation
(battlefield or graveyard), and reserved from the mana plan. The effect,
`StandardOpV1::ReturnExiledSourceTransformed`, returns only the card the
cost exiled, transformed under its owner's control, so the back face's
own entry triggers fire. Clay-Fired Bricks // Cosmium Kiln is the first
consumer: the front face searches for a basic Plains and gains 2 life; the
back face makes two 1/1 Gnome artifact creature tokens and gives creatures
its controller controls +1/+1.

Braided Net // Braided Quipu is the second. Its three net counters are
the entering count minus the removals recorded per incarnation in
`GameState::standard_v1`, paid by the appended
`CostComponent::RemoveNetCounterFromSelf`. Its tap ability records the
tapped target's incarnation; `standard_cards_v1::activations_locked` then
refuses that permanent's activated and mana abilities while it stays
tapped, and every untap path releases the lock
(`release_untapped_locks`), so a later tap does not renew it. Braided
Quipu draws a card per artifact its controller controls and then goes to
its owner's library third from the top (the appended
`LibraryPlacement::ThirdFromTop`).

## Chandra, Hope's Beacon

Chandra's +2 asks for one of the fifteen two-color combinations in WUBRG
order (`StandardManaCombinationV1`, an option choice whose options must
equal `two_mana_combinations`). Her +1 exiles the top five cards and gives
each castable instant or sorcery among them a cast permission until the end
of her controller's next turn; when more than one was exiled they form a
group, and casting one marks the group spent so the others stop being
offered. Her -X is one loyalty ability per X from 1 to 20, each dealing X to
each of up to two targets (`StandardTargetV1::UpToTwoAnyTargets`, chosen
through the variable-count activation selection), so X=0 and X above 20
are not offered. The copy trigger fires on the first instant or sorcery
its controller casts each turn (`trigger_limit_per_turn`). It copies the
spell onto the stack and, when the copy has exactly one target and
another legal one exists, asks for a new target
(`StandardCopyTargetV1`, which also accepts players); keeping the current
target is one of the options. A copy of a spell with more than one target
keeps its targets. The trigger's program is filled in from the cast event,
so root validation also accepts it through `template_matches`.

## Known limits

The stack and battlefield use the Room card's combined mana value and
name rather than the unlocked halves'. Sacrifice bindings, Bargain and
collect evidence fail closed on a back face. The legend rule counts
Temple of Power as legendary. Ninjutsu attackers attack the player. The
attack count adds each declaration, so a creature attacking in two combats
in one turn counts twice. Innkeeper's Talent does not double loyalty,
lore or poison counters; no other card in its deck puts them.
Reflection of Kiki-Jiki's delayed sacrifice trigger uses the token as its
source rather than Reflection (603.7d), so it reads as the token's
ability.
Craft's material decision reuses `CostKind::ExileFromGraveyard` even when
the candidate is a battlefield artifact, and net counters live in the
Standard state rather than `Counters`, so observations do not show them.

## Catalog identity

Each batch appends its cards and tokens to
`data/standard/magezero_v1/cards_v1.json` and updates the Standard
identity: the `kernel_carddb_standard` version, the frozen hash and the
`STANDARD_APPENDED`/`SUPPORTED_NONBASIC` lists in Rust and Python.

Validation lives in `tests/standard_magezero_family_ef_v1.rs`, alongside
the Pauper and FDN regression suites. Implementation is routine
engineering; no research design, measurement, training or paid compute
changes here.
