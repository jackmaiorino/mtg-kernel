//! Card programs for the MageZero Standard catalog's non-creature permanent,
//! planeswalker and transforming-legend families (inventory families E and F).
//!
//! Every program here is selected by printed card name from the registry, so
//! catalogs without these names never reach it. Behavior was read from each
//! card's XMage source (`magefree/mage` master).

use crate::card_def::{CardDef, CardType, Keywords, Subtype, CARD_DEFS};
use crate::effect::{EffectObjectBinding, EffectOp, ExecCtx, PlayerRef};
use crate::event::{self, CommittedEvent, DamageProposed, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::state::{GameState, Target, Zone};
use crate::trigger::{TriggerCondition, TriggeredAbilityDef};
use serde::{Deserialize, Serialize};

/// Per-game state for this module's cards. Absent until one of them needs
/// it, so other catalogs keep their snapshots and hashes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StandardStateV1 {
    /// Turn number and, per player, the noncombat damage dealt that turn by
    /// red sources they controlled (Temple of Power's activation condition).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    red_noncombat_damage: Option<(u32, [u32; 2])>,
    /// Unlocked doors of Room permanents, per exact battlefield incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rooms: Vec<RoomDoorsV1>,
    /// Turn number and, per player, the life gained and the life lost that
    /// turn (Lunar Convocation). Paying life is losing it (119.4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    life_this_turn: Option<(u32, [u32; 2], [u32; 2])>,
    /// Turn number and the creatures declared as attackers that turn (Case
    /// of the Gateway Express).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attackers_this_turn: Option<(u32, u32)>,
    /// Solved Case permanents, per exact battlefield incarnation (719.3).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    solved_cases: Vec<(ObjectId, u32)>,
    /// Class levels above 1, per exact battlefield incarnation (716.2).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    class_levels: Vec<(ObjectId, u32, u8)>,
    /// "You may cast this card from your graveyard" grants: card, its
    /// graveyard incarnation, the player who may cast it, and the turn the
    /// grant ends with.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    graveyard_casts: Vec<(ObjectId, u32, PlayerId, u32)>,
    /// Discards still owed by a resolving ability after its current
    /// discard lands: the ability, the player and the count.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    queued_discards: Vec<(crate::ids::StackItemId, PlayerId, u32)>,
    /// Creatures that became Phyrexian in addition to their other types
    /// (Breach the Multiverse), per exact battlefield incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    phyrexians: Vec<(ObjectId, u32)>,
    /// Token copies that have haste as part of their copy effect
    /// (Reflection of Kiki-Jiki), per exact battlefield incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    hasty_copies: Vec<(ObjectId, u32)>,
    /// "Sacrifice it at the beginning of the next end step" delayed
    /// triggers: the permanent's exact incarnation and the player who
    /// controls the delayed trigger.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    end_step_sacrifices: Vec<(ObjectId, u32, PlayerId)>,
    /// The Irencrag incarnations that became Everflame, Heroes' Legacy.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    everflames: Vec<(ObjectId, u32)>,
    /// Net counters removed from each Braided Net incarnation, which enters
    /// with three.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    net_counters_removed: Vec<(ObjectId, u32, u8)>,
    /// Permanents Braided Net tapped whose activated abilities can't be
    /// activated for as long as they remain tapped, per exact incarnation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    net_locks: Vec<(ObjectId, u32)>,
    /// Cards exiled together by Chandra, Hope's Beacon's +1, of which only
    /// one may be cast: each card's exile incarnation, and whether one of
    /// them was cast.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    exile_cast_groups: Vec<(Vec<(ObjectId, u32)>, bool)>,
    /// Creatures that are copies of a card exiled with Assimilation Aegis
    /// while that Aegis stays attached, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    aegis_copies: Vec<AegisCopyV1>,
    /// Every definition a permanent incarnation has had through an
    /// Assimilation Aegis copy, so its abilities already on the stack stay
    /// valid when the copy starts or ends.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    copied_card_defs: Vec<(ObjectId, u32, u16)>,
}

/// One Assimilation Aegis copy effect: the Aegis and the creature, per exact
/// battlefield incarnation, and the creature's values from before it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AegisCopyV1 {
    aegis: (ObjectId, u32),
    host: (ObjectId, u32),
    card_def: u16,
    name: String,
    face_index: u8,
    color_mask: u8,
    subtype_ids: Vec<u16>,
    ward_generic: u16,
}

/// The doors of one Room permanent incarnation: bit 0 is the left door,
/// bit 1 the right door. A Room absent from the list has both doors locked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RoomDoorsV1 {
    object: ObjectId,
    zone_change_count: u32,
    unlocked: u8,
}

/// One-object battlefield target filters owned by this module
/// (`TargetSpec::StandardV1`). "Opponent" is relative to the targeting
/// controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StandardTargetV1 {
    /// A nonland permanent an opponent controls with mana value at most N.
    OpponentNonlandPermanentManaValueAtMost(u8),
    /// A nonland permanent an opponent controls.
    OpponentNonlandPermanent,
    /// An artifact or creature an opponent controls.
    OpponentArtifactOrCreature,
    /// An artifact the targeting controller controls.
    ControlledArtifact,
    /// An instant or sorcery card in the targeting controller's graveyard.
    InstantOrSorceryCardInOwnGraveyard,
    /// Another nonlegendary creature the targeting controller controls.
    AnotherNonlegendaryControlledCreature,
    /// Another nonland permanent (any controller).
    AnotherNonlandPermanent,
    /// "Each of up to two targets": two distinct any-targets (115.4).
    UpToTwoAnyTargets,
    /// "Up to one target creature."
    UpToOneCreature,
    /// A card in any graveyard.
    CardInAGraveyard,
}

impl StandardTargetV1 {
    /// How many targets this filter takes, and how many at least.
    pub(crate) fn counts(self) -> (u8, u8) {
        match self {
            Self::UpToTwoAnyTargets => (2, 0),
            Self::UpToOneCreature => (1, 0),
            _ => (1, 1),
        }
    }
}

impl StandardTargetV1 {
    fn in_graveyard(self) -> bool {
        matches!(
            self,
            Self::InstantOrSorceryCardInOwnGraveyard | Self::CardInAGraveyard
        )
    }
}

/// The zone a `TargetSpec::StandardV1` target is chosen in.
pub(crate) fn target_zone(target: StandardTargetV1) -> Zone {
    if target.in_graveyard() {
        Zone::Graveyard
    } else {
        Zone::Battlefield
    }
}

/// The legal choices for `target`, in battlefield or graveyard order, after
/// the already chosen `prefix`.
pub(crate) fn legal_targets(
    target: StandardTargetV1,
    controller: PlayerId,
    source: Option<ObjectId>,
    prefix: &[Target],
    state: &GameState,
) -> Vec<Target> {
    if target == StandardTargetV1::UpToTwoAnyTargets {
        // 115.4: a creature, player or planeswalker; each target once.
        let players = [PlayerId::P0, PlayerId::P1].map(Target::Player);
        let permanents = [PlayerId::P0, PlayerId::P1]
            .iter()
            .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
            .filter(|&id| {
                crate::engine::object_has_type(state, id, CardType::Creature)
                    || crate::engine::object_has_type(state, id, CardType::Planeswalker)
            })
            .map(Target::Object);
        return players
            .into_iter()
            .chain(permanents)
            .filter(|candidate| !prefix.contains(candidate))
            .collect();
    }
    let candidates: Vec<ObjectId> = if target == StandardTargetV1::CardInAGraveyard {
        [PlayerId::P0, PlayerId::P1]
            .iter()
            .flat_map(|player| state.players[player.index()].graveyard.iter().copied())
            .collect()
    } else if target.in_graveyard() {
        state.players[controller.index()].graveyard.clone()
    } else {
        [PlayerId::P0, PlayerId::P1]
            .iter()
            .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
            .collect()
    };
    candidates
        .into_iter()
        .filter(|&id| target_matches(target, controller, id, state))
        .filter(|&id| {
            !matches!(
                target,
                StandardTargetV1::AnotherNonlegendaryControlledCreature
                    | StandardTargetV1::AnotherNonlandPermanent
            ) || Some(id) != source
        })
        .map(Target::Object)
        .collect()
}

/// Whether `object` satisfies `target` for `controller`.
pub(crate) fn target_matches(
    target: StandardTargetV1,
    controller: PlayerId,
    object: ObjectId,
    state: &GameState,
) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    if target == StandardTargetV1::CardInAGraveyard {
        return live.zone == Zone::Graveyard && !live.v4.is_token;
    }
    if target.in_graveyard() {
        let def = &CARD_DEFS[live.card_def as usize];
        return live.zone == Zone::Graveyard
            && live.owner == controller
            && !live.v4.is_token
            && (def.has_type(CardType::Instant) || def.has_type(CardType::Sorcery));
    }
    if live.zone != Zone::Battlefield {
        return false;
    }
    let has = |card_type| crate::engine::object_has_type(state, object, card_type);
    match target {
        StandardTargetV1::OpponentNonlandPermanentManaValueAtMost(maximum) => {
            live.controller != controller
                && !has(CardType::Land)
                && mana_value(state, object) <= u16::from(maximum)
        }
        StandardTargetV1::OpponentNonlandPermanent => {
            live.controller != controller && !has(CardType::Land)
        }
        StandardTargetV1::OpponentArtifactOrCreature => {
            live.controller != controller && (has(CardType::Artifact) || has(CardType::Creature))
        }
        StandardTargetV1::ControlledArtifact => {
            live.controller == controller && has(CardType::Artifact)
        }
        StandardTargetV1::AnotherNonlandPermanent => !has(CardType::Land),
        StandardTargetV1::UpToTwoAnyTargets => {
            has(CardType::Creature) || has(CardType::Planeswalker)
        }
        StandardTargetV1::UpToOneCreature => has(CardType::Creature),
        StandardTargetV1::CardInAGraveyard => false,
        StandardTargetV1::AnotherNonlegendaryControlledCreature => {
            live.controller == controller
                && has(CardType::Creature)
                && !CARD_DEFS[live.card_def as usize]
                    .supertypes
                    .contains(&crate::card_def::Supertype::Legendary)
        }
        StandardTargetV1::InstantOrSorceryCardInOwnGraveyard => false,
    }
}

/// Effect leaves owned by this module. Appended to `EffectOp` as one variant
/// so earlier serialized programs keep their shapes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StandardOpV1 {
    /// Put loyalty counters on the resolving ability's own source, if it is
    /// still the same battlefield planeswalker incarnation.
    AddLoyaltyToSource { amount: u8 },
    /// `player` chooses one permanent they control matching `filter` (a
    /// public choice, not targeted) and `action` applies to it. With one
    /// candidate the choice is automatic; with none nothing happens.
    PlayerChoosesControlledPermanent {
        player: PlayerRef,
        filter: StandardPermanentFilterV1,
        action: StandardChosenActionV1,
    },
    /// The answered half of `PlayerChoosesControlledPermanent`: interpreter
    /// owned, never part of a generated program.
    ApplyChosenPermanent {
        chosen: EffectObjectBinding,
        action: StandardChosenActionV1,
    },
    /// Each nonland permanent `player` controls goes into its owner's
    /// library, then every affected library is shuffled.
    ShuffleNonlandPermanentsIntoLibraries { player: PlayerRef },
    /// Transform the resolving ability's source to its other face, if it is
    /// still the same battlefield incarnation.
    TransformSource,
    /// Trigger-time template for "you lose that much life": materialized to
    /// `CecilDarkness` with the damage event's amount.
    BindCecilDarkness,
    /// Cecil, Dark Knight: its controller loses `amount` life. Then if their
    /// life total is at most half their starting life total, untap the
    /// source and transform it.
    CecilDarkness { amount: u32 },
    /// Other attacking creatures the controller controls gain `keywords`
    /// until end of turn.
    OtherAttackersGainUntilEndOfTurn { keywords: Keywords },
    /// The source, put into a graveyard from the battlefield by the event
    /// that triggered this ability, returns to the battlefield tapped and
    /// transformed under its owner's control.
    ReturnSourceTappedAndTransformed,
    /// Blue Sun's Twilight: gain control of target creature (mana value at
    /// most X); if X is 5 or more, create a token that's a copy of it.
    GainControlOfTargetCreatureCopyAtXFive,
    /// A Room spell resolves: it enters the battlefield, and if it was cast,
    /// the door of the half that was cast becomes unlocked (709.5).
    RoomEnters,
    /// Unlock `door` of the ability's source Room, if it is still the same
    /// battlefield incarnation and that door is locked.
    UnlockSourceDoor { door: u8 },
    /// A resolving Aura spell enters the battlefield attached to its target
    /// (303.4f), for this module's Auras whose enchant restriction is not
    /// "enchant creature".
    AuraEnters,
    /// Lunar Convocation: "if you gained life this turn, each opponent loses
    /// `amount` life", the intervening condition rechecked on resolution
    /// (603.4).
    OpponentLosesLifeIfYouGainedLife { amount: u8 },
    /// Lunar Convocation: "if you gained and lost life this turn, create"
    /// the token, the intervening condition rechecked on resolution.
    CreateTokenIfYouGainedAndLostLife { token_def: u16 },
    /// Case of the Gateway Express: each creature you control deals 1
    /// damage to the target creature, simultaneously.
    EachControlledCreatureDealsOneDamageToTarget,
    /// A Case's "to solve" end-step ability: the source becomes solved if
    /// it is still the same unsolved incarnation and its condition holds.
    SolveSourceCase,
    /// A Class's level-up ability resolves: the source gains `level` if it
    /// is still the same incarnation at the level before it (716.2a).
    GainClassLevel { level: u8 },
    /// Case of the Uneaten Feast: creature cards now in your graveyard gain
    /// "You may cast this card from your graveyard" until end of turn.
    GraveyardCreaturesCastableThisTurn,
    /// Liliana of the Veil's +1: each player discards `count` cards, the
    /// controller first. Like `EffectOp::DiscardCards`, it must be the last
    /// leaf of its program.
    EachPlayerDiscards { count: u32 },
    /// Liliana of the Veil's -6: the controller separates all permanents
    /// `player` controls into two piles, and `player` sacrifices all
    /// permanents in the pile of their choice.
    SeparatePilesThenSacrifice { player: PlayerRef },
    /// The separated piles, waiting for `player` to choose one:
    /// interpreter owned.
    ChooseSacrificePile {
        player: PlayerId,
        pile_a: Vec<EffectObjectBinding>,
        pile_b: Vec<EffectObjectBinding>,
    },
    /// `player` sacrifices the chosen pile's still-present permanents:
    /// interpreter owned.
    SacrificePile {
        player: PlayerId,
        pile: Vec<EffectObjectBinding>,
    },
    /// Breach the Multiverse: for each player in turn order, the controller
    /// chooses a creature or planeswalker card in that player's graveyard
    /// (not targeted); the chosen cards then enter under the controller's
    /// control together.
    PutCreatureOrPlaneswalkerFromEachGraveyard,
    /// The chosen cards, one per graveyard that had one: interpreter owned.
    PutChosenCardsOntoBattlefield { cards: Vec<EffectObjectBinding> },
    /// Each creature the controller controls becomes a Phyrexian in
    /// addition to its other types, for as long as it stays on the
    /// battlefield.
    ControlledCreaturesBecomePhyrexian,
    /// Fable of the Mirror-Breaker chapter II: `player` may discard up to
    /// `count` cards, then draws as many as they discarded.
    MayDiscardUpToThenDraw { player: PlayerRef, count: u8 },
    /// The chosen hand cards: interpreter owned.
    DiscardChosenThenDraw {
        player: PlayerId,
        cards: Vec<EffectObjectBinding>,
    },
    /// Reflection of Kiki-Jiki: create a token copy of the target, except
    /// it has haste, and sacrifice it at the beginning of the next end step.
    CopyTargetWithHasteSacrificeAtEndStep,
    /// The delayed trigger: its controller sacrifices the source, if it is
    /// still the same battlefield incarnation under their control.
    SacrificeSourceAtEndStep,
    /// Repurposing Bay: search your library for an artifact card with mana
    /// value one more than the sacrificed artifact's, put it onto the
    /// battlefield, then shuffle.
    SearchArtifactWithManaValueOneMoreThanSacrificed,
    /// The Irencrag: "you may have The Irencrag become a legendary Equipment
    /// artifact named Everflame, Heroes' Legacy".
    MayBecomeEverflame,
    /// The accepted choice: interpreter owned.
    BecomeEverflame { source: EffectObjectBinding },
    /// Craft (702.167a): the source, exiled to pay the ability's cost,
    /// returns to the battlefield transformed under its owner's control.
    ReturnExiledSourceTransformed,
    /// Braided Net: tap target permanent; its activated abilities can't be
    /// activated for as long as it remains tapped.
    TapTargetAndLockActivations,
    /// Braided Quipu: draw a card for each artifact the controller
    /// controls, then put the source into its owner's library third from
    /// the top.
    DrawPerArtifactThenSourceThirdFromTop,
    /// Chandra, Hope's Beacon's +2: "Add two mana in any combination of
    /// colors", one choice among the fifteen combinations.
    ChooseTwoManaInAnyCombination,
    /// Chandra's +1: exile the top five cards of your library; until the end
    /// of your next turn, you may cast an instant or sorcery spell from
    /// among them.
    ExileTopFiveMayCastOneInstantOrSorcery,
    /// Chandra's -X: it deals `amount` damage to each of its targets.
    DamageEachTarget { amount: u8 },
    /// Chandra's copy trigger template, bound to the cast spell when the
    /// trigger fires (`materialize_event`).
    BindCopyCastSpell,
    /// Copy `spell`; its controller may choose new targets for the copy.
    CopySpellMayChooseNewTargets { spell: ObjectId },
    /// The answered target for the copy: interpreter owned.
    RetargetSpellCopy {
        copy: crate::ids::StackItemId,
        target: Target,
    },
    /// Exile the targeted graveyard card with this ability's source
    /// (Agatha's Soul Cauldron), linking it to that source incarnation.
    ExileTargetCardWithSource,
}

impl StandardOpV1 {
    /// Whether this leaf can yield a player decision, so its program must use
    /// the resumable interpreter.
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(
            self,
            Self::PlayerChoosesControlledPermanent { .. }
                | Self::SeparatePilesThenSacrifice { .. }
                | Self::PutCreatureOrPlaneswalkerFromEachGraveyard
                | Self::MayDiscardUpToThenDraw { .. }
                | Self::SearchArtifactWithManaValueOneMoreThanSacrificed
                | Self::MayBecomeEverflame
                | Self::ChooseTwoManaInAnyCombination
                | Self::CopySpellMayChooseNewTargets { .. }
        )
    }

    /// The exact object incarnations this leaf is bound to.
    pub(crate) fn bound_objects(&self) -> Vec<EffectObjectBinding> {
        match self {
            Self::ApplyChosenPermanent { chosen, .. } => vec![*chosen],
            Self::ChooseSacrificePile { pile_a, pile_b, .. } => {
                pile_a.iter().chain(pile_b).copied().collect()
            }
            Self::SacrificePile { pile, .. } => pile.clone(),
            Self::PutChosenCardsOntoBattlefield { cards }
            | Self::DiscardChosenThenDraw { cards, .. } => cards.clone(),
            Self::BecomeEverflame { source } => vec![*source],
            _ => Vec::new(),
        }
    }

    /// Interpreter-owned leaves that a generated program may never contain.
    pub(crate) fn is_bound_continuation(&self) -> bool {
        matches!(
            self,
            Self::ApplyChosenPermanent { .. }
                | Self::ChooseSacrificePile { .. }
                | Self::SacrificePile { .. }
                | Self::PutChosenCardsOntoBattlefield { .. }
                | Self::DiscardChosenThenDraw { .. }
                | Self::BecomeEverflame { .. }
                | Self::RetargetSpellCopy { .. }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StandardPermanentFilterV1 {
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StandardChosenActionV1 {
    ReturnToOwnersHand,
}

/// The exact battlefield permanents `player` controls that match `filter`,
/// in battlefield order.
pub(crate) fn controlled_permanent_candidates(
    state: &GameState,
    player: PlayerId,
    filter: StandardPermanentFilterV1,
) -> Vec<EffectObjectBinding> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| {
            let live = state.objects.get(object);
            live.zone == Zone::Battlefield
                && live.controller == player
                && match filter {
                    StandardPermanentFilterV1::Any => true,
                }
        })
        .map(|object| EffectObjectBinding {
            object,
            expected_zone: Zone::Battlefield,
            expected_zone_change_count: state.objects.get(object).zone_change_count,
        })
        .collect()
}

/// Applies a chosen-permanent action to its exact bound incarnation.
pub(crate) fn apply_chosen_permanent(
    state: &mut GameState,
    chosen: EffectObjectBinding,
    action: StandardChosenActionV1,
) {
    let live = state.objects.get(chosen.object);
    if live.zone != chosen.expected_zone
        || live.zone_change_count != chosen.expected_zone_change_count
    {
        return;
    }
    match action {
        StandardChosenActionV1::ReturnToOwnersHand => {
            event::propose_and_commit(state, ProposedEvent::zone_change(chosen.object, Zone::Hand))
        }
    }
}

/// True iff the resolving ability's source is still the incarnation that
/// created it, on the battlefield.
fn source_incarnation_live(ctx: &ExecCtx, state: &GameState) -> bool {
    let Some(contract) = ctx.ability_source_contract else {
        return false;
    };
    state.objects.try_get(ctx.source).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.card_def == contract.card_def
            && live.zone_change_count == contract.zone_change_count
    })
}

/// True iff the ability source is the same battlefield incarnation with a
/// second face.
fn transformable_source(ctx: &ExecCtx, state: &GameState) -> bool {
    source_incarnation_live(ctx, state)
        && CARD_DEFS[state.objects.get(ctx.source).card_def as usize]
            .transform_face
            .is_some()
}

fn transform_resolving_source(ctx: &ExecCtx, state: &mut GameState) {
    if transformable_source(ctx, state) {
        let face = 1 - state.objects.get(ctx.source).v4.face_index.min(1);
        event::propose_and_commit(state, ProposedEvent::transform_in_place(ctx.source, face));
    }
}

pub(crate) fn execute(op: &StandardOpV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        StandardOpV1::TransformSource => transform_resolving_source(ctx, state),
        StandardOpV1::RoomEnters => room_enters(ctx, state),
        StandardOpV1::AuraEnters => aura_enters_battlefield(ctx, state),
        StandardOpV1::OpponentLosesLifeIfYouGainedLife { amount } => {
            if life_changed_this_turn(state, ctx.controller, false) {
                event::propose_and_commit(
                    state,
                    ProposedEvent::life_loss(ctx.controller.opponent(), i32::from(*amount)),
                );
            }
        }
        StandardOpV1::CreateTokenIfYouGainedAndLostLife { token_def } => {
            if life_changed_this_turn(state, ctx.controller, true) {
                event::propose_and_commit(
                    state,
                    ProposedEvent::create_token(*token_def, ctx.controller),
                );
            }
        }
        StandardOpV1::EachControlledCreatureDealsOneDamageToTarget => {
            let Some(Target::Object(target)) = ctx.targets.first().copied() else {
                return;
            };
            let live = state.objects.get(target);
            if !ctx.target_incarnation_matches(0, state)
                || live.zone != Zone::Battlefield
                || live.controller == ctx.controller
                || !crate::engine::object_has_type(state, target, CardType::Creature)
            {
                return;
            }
            let damage = state.players[ctx.controller.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|&id| crate::engine::object_has_type(state, id, CardType::Creature))
                .map(|id| ProposedEvent::damage(id, Target::Object(target), 1))
                .collect::<Vec<_>>();
            if !damage.is_empty() {
                event::propose_and_commit_batch(state, damage);
            }
        }
        StandardOpV1::GainClassLevel { level } => {
            if source_incarnation_live(ctx, state)
                && class_level(state, ctx.source).checked_add(1) == Some(*level)
            {
                set_class_level(state, ctx.source, *level);
            }
        }
        StandardOpV1::GraveyardCreaturesCastableThisTurn => {
            let turn = state.turn;
            let grants = state.players[ctx.controller.index()]
                .graveyard
                .iter()
                .copied()
                .filter(|&id| {
                    CARD_DEFS[state.objects.get(id).card_def as usize].has_type(CardType::Creature)
                })
                .map(|id| {
                    (
                        id,
                        state.objects.get(id).zone_change_count,
                        ctx.controller,
                        turn,
                    )
                })
                .collect::<Vec<_>>();
            let standard = state.standard_v1.get_or_insert_with(Default::default);
            standard.graveyard_casts.retain(|grant| grant.3 == turn);
            standard.graveyard_casts.extend(grants);
        }
        StandardOpV1::SolveSourceCase => {
            if source_incarnation_live(ctx, state)
                && !case_solved(state, ctx.source)
                && case_condition_met(state, ctx.source)
            {
                let zone_change_count = state.objects.get(ctx.source).zone_change_count;
                let standard = state.standard_v1.get_or_insert_with(Default::default);
                standard.solved_cases.push((ctx.source, zone_change_count));
            }
        }
        StandardOpV1::UnlockSourceDoor { door } => {
            if source_incarnation_live(ctx, state) {
                unlock_door(state, ctx.source, *door);
            }
        }
        StandardOpV1::BindCecilDarkness => {}
        StandardOpV1::CecilDarkness { amount } => {
            let amount = i32::try_from(*amount).unwrap_or(i32::MAX);
            if amount > 0 {
                event::propose_and_commit(state, ProposedEvent::life_loss(ctx.controller, amount));
            }
            if state.players[ctx.controller.index()].life <= crate::state::STARTING_LIFE / 2
                && source_incarnation_live(ctx, state)
                && state.objects.get(ctx.source).v4.face_index == 0
            {
                state.objects.get_mut(ctx.source).tapped = false;
                release_untapped_locks(state);
                transform_resolving_source(ctx, state);
            }
        }
        StandardOpV1::OtherAttackersGainUntilEndOfTurn { keywords } => {
            let attackers = state
                .engine
                .combat
                .attackers
                .iter()
                .copied()
                .filter(|&id| {
                    let live = state.objects.get(id);
                    id != ctx.source
                        && live.zone == Zone::Battlefield
                        && live.controller == ctx.controller
                })
                .collect::<Vec<_>>();
            for object_id in attackers {
                let timestamp = crate::engine::next_timestamp(state);
                state.engine.until_end_of_turn.push(
                    crate::engine::UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
                        object_id,
                        object_zone_change_count: state.objects.get(object_id).zone_change_count,
                        layer: crate::engine::Layers::ABILITY_ADDING,
                        timestamp,
                        duration: crate::engine::EffectDuration::EndOfTurn,
                        keywords: *keywords,
                    },
                );
            }
        }
        StandardOpV1::GainControlOfTargetCreatureCopyAtXFive => {
            let Some(Target::Object(object)) = ctx.targets.first().copied() else {
                return;
            };
            let live = state.objects.get(object);
            if live.zone != Zone::Battlefield
                || !crate::engine::object_has_type(state, object, CardType::Creature)
                || mana_value(state, object) > ctx.x_value
            {
                return;
            }
            gain_control(state, object, ctx.controller);
            if ctx.x_value >= 5 {
                create_token_copy(state, object, ctx.controller);
            }
        }
        StandardOpV1::ReturnSourceTappedAndTransformed => {
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            let Some(card) = state.objects.try_get(ctx.source) else {
                return;
            };
            // Only the card the dying permanent became, still in the
            // graveyard it went to (400.7).
            if card.card_def != contract.card_def
                || card.zone != Zone::Graveyard
                || card.zone_change_count != contract.zone_change_count + 1
            {
                return;
            }
            let owner = card.owner;
            let mut entry = ProposedEvent::transformed_battlefield_return(ctx.source, 1, owner);
            if let ProposedEvent::ZoneChange(change) = &mut entry {
                change.force_battlefield_tapped = true;
            }
            event::propose_and_commit(state, entry);
        }
        StandardOpV1::ReturnExiledSourceTransformed => {
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            let Some(card) = state.objects.try_get(ctx.source) else {
                return;
            };
            // Only the card the source became when the cost exiled it
            // (400.7).
            if card.card_def != contract.card_def
                || card.zone != Zone::Exile
                || card.zone_change_count != contract.zone_change_count + 1
            {
                return;
            }
            let owner = card.owner;
            event::propose_and_commit(
                state,
                ProposedEvent::transformed_battlefield_return(ctx.source, 1, owner),
            );
        }
        StandardOpV1::TapTargetAndLockActivations => {
            let Some(Target::Object(object)) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || !target_matches(
                    StandardTargetV1::AnotherNonlandPermanent,
                    ctx.controller,
                    object,
                    state,
                )
            {
                return;
            }
            event::propose_and_commit(state, ProposedEvent::tap(object));
            let live = state.objects.get(object);
            if live.tapped && live.zone == Zone::Battlefield {
                let incarnation = (object, live.zone_change_count);
                let standard = state.standard_v1.get_or_insert_with(Default::default);
                if !standard.net_locks.contains(&incarnation) {
                    standard.net_locks.push(incarnation);
                }
            }
        }
        StandardOpV1::DrawPerArtifactThenSourceThirdFromTop => {
            let artifacts = state.players[ctx.controller.index()]
                .battlefield
                .iter()
                .filter(|&&id| crate::engine::object_has_type(state, id, CardType::Artifact))
                .count();
            for _ in 0..artifacts {
                event::propose_and_commit(state, ProposedEvent::draw(ctx.controller));
            }
            if source_incarnation_live(ctx, state) {
                event::propose_and_commit(
                    state,
                    ProposedEvent::public_library_insert(
                        ctx.source,
                        event::LibraryPlacement::ThirdFromTop,
                    ),
                );
            }
        }
        StandardOpV1::ExileTopFiveMayCastOneInstantOrSorcery => {
            exile_top_five_may_cast_one(state, ctx.controller);
        }
        StandardOpV1::DamageEachTarget { amount } => {
            for index in 0..ctx.targets.len() {
                if ctx.target_incarnation_matches(index, state)
                    && crate::engine::effect_target_is_legal(
                        state,
                        ctx.source,
                        ctx.controller,
                        crate::card_def::TargetSpec::StandardV1(
                            StandardTargetV1::UpToTwoAnyTargets,
                        ),
                        &ctx.targets,
                        index,
                    )
                {
                    event::propose_and_commit(
                        state,
                        ProposedEvent::damage(ctx.source, ctx.targets[index], i32::from(*amount)),
                    );
                }
            }
        }
        StandardOpV1::BindCopyCastSpell => {}
        StandardOpV1::ExileTargetCardWithSource => exile_card_with_source(ctx, state),
        StandardOpV1::AddLoyaltyToSource { amount } => {
            if source_incarnation_live(ctx, state) {
                crate::planeswalker_v1::change_loyalty(state, ctx.source, i32::from(*amount));
            }
        }
        StandardOpV1::ShuffleNonlandPermanentsIntoLibraries { player } => {
            let player = ctx.resolve_player(*player, state);
            let moving = state.players[player.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|&object| !crate::engine::object_has_type(state, object, CardType::Land))
                .collect::<Vec<_>>();
            if moving.is_empty() {
                return;
            }
            let mut owners = moving
                .iter()
                .map(|&object| state.objects.get(object).owner)
                .collect::<Vec<_>>();
            owners.sort_unstable();
            owners.dedup();
            event::propose_and_commit_batch(
                state,
                moving
                    .into_iter()
                    .map(|object| ProposedEvent::zone_change(object, Zone::Library))
                    .collect(),
            );
            for owner in owners {
                if state.shuffle_library(owner).is_err() {
                    state.engine.halted = Some((
                        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                        ctx.source,
                    ));
                    return;
                }
            }
        }
        StandardOpV1::EachPlayerDiscards { count } => {
            // The controller discards now; the opponent's discard is queued
            // until it lands (`stage_queued_discard`).
            state.engine.pending_discard = Some(crate::engine::PendingDiscard {
                player: ctx.controller,
                count: *count,
                resume: crate::engine::DiscardResume::None,
            });
            if let Some(stack_item_id) = ctx.stack_item_id {
                let standard = state.standard_v1.get_or_insert_with(Default::default);
                standard
                    .queued_discards
                    .push((stack_item_id, ctx.controller.opponent(), *count));
            }
        }
        StandardOpV1::ControlledCreaturesBecomePhyrexian => {
            become_phyrexian(state, ctx.controller);
        }
        StandardOpV1::CopyTargetWithHasteSacrificeAtEndStep => {
            let Some(Target::Object(object)) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || !target_matches(
                    StandardTargetV1::AnotherNonlegendaryControlledCreature,
                    ctx.controller,
                    object,
                    state,
                )
                || object == ctx.source
            {
                return;
            }
            if let Some(token) = create_token_copy(state, object, ctx.controller) {
                let incarnation = (token, state.objects.get(token).zone_change_count);
                let standard = state.standard_v1.get_or_insert_with(Default::default);
                standard.hasty_copies.push(incarnation);
                standard
                    .end_step_sacrifices
                    .push((incarnation.0, incarnation.1, ctx.controller));
            }
        }
        StandardOpV1::SacrificeSourceAtEndStep => {
            let Some(contract) = ctx.ability_source_contract else {
                return;
            };
            let Some(live) = state.objects.try_get(ctx.source) else {
                return;
            };
            if live.zone == Zone::Battlefield
                && live.zone_change_count == contract.zone_change_count
                && live.controller == ctx.controller
            {
                event::log_sacrifice(state, ctx.source);
                event::propose_and_commit(
                    state,
                    ProposedEvent::zone_change(ctx.source, Zone::Graveyard),
                );
            }
        }
        StandardOpV1::PlayerChoosesControlledPermanent { .. }
        | StandardOpV1::ApplyChosenPermanent { .. }
        | StandardOpV1::SeparatePilesThenSacrifice { .. }
        | StandardOpV1::ChooseSacrificePile { .. }
        | StandardOpV1::SacrificePile { .. }
        | StandardOpV1::PutCreatureOrPlaneswalkerFromEachGraveyard
        | StandardOpV1::PutChosenCardsOntoBattlefield { .. }
        | StandardOpV1::MayDiscardUpToThenDraw { .. }
        | StandardOpV1::DiscardChosenThenDraw { .. }
        | StandardOpV1::SearchArtifactWithManaValueOneMoreThanSacrificed
        | StandardOpV1::MayBecomeEverflame
        | StandardOpV1::BecomeEverflame { .. }
        | StandardOpV1::ChooseTwoManaInAnyCombination
        | StandardOpV1::CopySpellMayChooseNewTargets { .. }
        | StandardOpV1::RetargetSpellCopy { .. } => {
            panic!("choice-bearing effects must use the resumable interpreter")
        }
    }
}

/// A permanent's mana value (202.3), from its card's mana cost.
fn mana_value(state: &GameState, object: ObjectId) -> u16 {
    CARD_DEFS[state.objects.get(object).card_def as usize].mana_value
}

/// `player` gains control of `object` indefinitely (until it leaves the
/// battlefield). It is summoning sick for its new controller, and leaves
/// combat (506.4).
pub(crate) fn gain_control(state: &mut GameState, object: ObjectId, player: PlayerId) {
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield || live.controller == player {
        return;
    }
    let previous = live.controller;
    state.players[previous.index()]
        .battlefield
        .retain(|&id| id != object);
    state.players[player.index()].battlefield.push(object);
    let live = state.objects.get_mut(object);
    live.controller = player;
    live.summoning_sick = true;
    state.engine.combat.remove_from_combat(object);
}

/// Creates a token that's a copy of `original` (707.2): the same card
/// definition and shown face, without counters, damage or other state.
pub(crate) fn create_token_copy(
    state: &mut GameState,
    original: ObjectId,
    controller: PlayerId,
) -> Option<ObjectId> {
    let live = state.objects.get(original);
    let (card_def, face) = (live.card_def, live.v4.face_index);
    let start = state.engine.event_log.len();
    event::propose_and_commit(state, ProposedEvent::create_token(card_def, controller));
    let created = state.engine.event_log[start..]
        .iter()
        .find_map(|event| match event {
            CommittedEvent::CreateToken { object, .. } => Some(*object),
            _ => None,
        });
    if let Some(token) = created {
        state.objects.get_mut(token).v4.is_token = true;
        if face != 0 && CARD_DEFS[card_def as usize].transform_face.is_some() {
            event::propose_and_commit(state, ProposedEvent::transform_in_place(token, face));
        }
    }
    created
}

const fn trigger(condition: TriggerCondition, effect: fn() -> EffectOp) -> TriggeredAbilityDef {
    TriggeredAbilityDef {
        condition,
        home_zone: Zone::Battlefield,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        effect,
    }
}

// ---- Teferi, Temporal Pilgrim -------------------------------------------

fn teferi_draw_loyalty() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::AddLoyaltyToSource { amount: 1 })
}

const TEFERI_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::ControllerDraws,
    teferi_draw_loyalty,
)];

/// "-2: Create a 2/2 blue Spirit creature token with vigilance and 'Whenever
/// you draw a card, put a +1/+1 counter on this creature.'"
pub fn teferi_create_spirit() -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name("Teferi Spirit Token")
            .expect("Teferi Spirit Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}

/// "-12: Target opponent chooses a permanent they control and returns it to
/// its owner's hand. Then they shuffle each nonland permanent they control
/// into its owner's library."
pub fn teferi_ultimate() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::StandardV1(StandardOpV1::PlayerChoosesControlledPermanent {
            player: PlayerRef::Target(0),
            filter: StandardPermanentFilterV1::Any,
            action: StandardChosenActionV1::ReturnToOwnersHand,
        }),
        EffectOp::StandardV1(StandardOpV1::ShuffleNonlandPermanentsIntoLibraries {
            player: PlayerRef::Target(0),
        }),
    ])
}

fn spirit_draw_counter() -> EffectOp {
    EffectOp::BindPlusOnePlusOneCounterToTriggerSource
}

const TEFERI_SPIRIT_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::ControllerDraws,
    spirit_draw_counter,
)];

// ---- Cecil, Dark Knight // Cecil, Redeemed Paladin ----------------------

fn cecil_darkness() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::BindCecilDarkness)
}

fn cecil_protect() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::OtherAttackersGainUntilEndOfTurn {
        keywords: Keywords::INDESTRUCTIBLE,
    })
}

const CECIL_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::DealsDamage, cecil_darkness),
    trigger(TriggerCondition::Attacks, cecil_protect),
];

/// Replaces a trigger-time template with the event's data, if `effect` is
/// one of this module's templates.
pub(crate) fn materialize_event(effect: &EffectOp, event: &CommittedEvent) -> Option<EffectOp> {
    match (effect, event) {
        (
            EffectOp::StandardV1(StandardOpV1::BindCecilDarkness),
            CommittedEvent::Damage { amount, .. },
        ) => Some(EffectOp::StandardV1(StandardOpV1::CecilDarkness {
            amount: u32::try_from(*amount).unwrap_or(0),
        })),
        (
            EffectOp::StandardV1(StandardOpV1::BindCopyCastSpell),
            CommittedEvent::SpellCast { spell, .. },
        ) => Some(EffectOp::StandardV1(
            StandardOpV1::CopySpellMayChooseNewTargets { spell: *spell },
        )),
        _ => None,
    }
}

/// Whether `effect` is a trigger-time materialization of `template`.
pub(crate) fn template_matches(template: &EffectOp, effect: &EffectOp) -> bool {
    matches!(
        (template, effect),
        (
            EffectOp::StandardV1(StandardOpV1::BindCecilDarkness),
            EffectOp::StandardV1(StandardOpV1::CecilDarkness { .. }),
        ) | (
            EffectOp::StandardV1(StandardOpV1::BindCopyCastSpell),
            EffectOp::StandardV1(StandardOpV1::CopySpellMayChooseNewTargets { .. }),
        )
    )
}

// ---- Polukranos Reborn // Polukranos, Engine of Ruin ----------------------

/// "{6}{W/P}: Transform Polukranos Reborn. Activate only as a sorcery." and
/// Temple of Power's transform ability.
pub fn transform_source() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::TransformSource)
}

fn polukranos_hydras() -> EffectOp {
    let token = |name| crate::card_def::card_id_by_name(name).expect("Phyrexian Hydra token");
    EffectOp::Sequence(vec![
        EffectOp::CreateToken {
            token_def: token("Phyrexian Hydra Reach Token"),
            controller: PlayerRef::Controller,
        },
        EffectOp::CreateToken {
            token_def: token("Phyrexian Hydra Lifelink Token"),
            controller: PlayerRef::Controller,
        },
    ])
}

/// Trigger conditions owned by this module. Appended to `TriggerCondition`
/// as one variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StandardTriggerV1 {
    /// "Whenever this or another nontoken Hydra you control dies", printed on
    /// the back face.
    ThisOrAnotherNontokenHydraYouControlDies,
    /// "At the beginning of your end step", printed on a Room door, which
    /// works only while that door is unlocked.
    ControllerEndStepWhileDoorUnlocked { door: u8 },
    /// "When you unlock this door."
    YouUnlockThisDoor { door: u8 },
    /// "At the beginning of your end step, if you gained life this turn"
    /// (and, with `require_lost`, "and lost life").
    ControllerEndStepIfLifeChanged { require_lost: bool },
    /// "Whenever another artifact you control with mana value N or greater
    /// enters."
    AnotherControlledArtifactEntersManaValueAtLeast(u8),
    /// A Case's "to solve": at the beginning of your end step, if this Case
    /// is unsolved and its condition holds (719.4).
    ControllerEndStepSolveCase,
    /// "At the beginning of combat on your turn."
    ControllerBeginningOfCombat,
    /// "When this Class becomes level N" (716.2c).
    ClassBecomesLevel { level: u8 },
    /// "Whenever you cast an instant or sorcery spell", printed at a Class
    /// level, so it works only from that level on.
    YouCastInstantOrSorceryAtClassLevel { level: u8 },
    /// "Whenever a legendary creature you control enters", on a permanent
    /// that has not become Everflame.
    ControlledLegendaryCreatureEntersUnlessEverflame,
    /// "Whenever you cast an instant or sorcery spell, ... This ability
    /// triggers only once each turn" (`trigger_limit_per_turn`).
    YouCastInstantOrSorceryOncePerTurn,
    /// "When a creature card is exiled this way" (Agatha's Soul Cauldron).
    CreatureCardExiledWithThis,
}

/// How many times each turn a trigger condition may trigger, if limited.
pub(crate) fn trigger_limit_per_turn(condition: StandardTriggerV1) -> Option<u16> {
    match condition {
        StandardTriggerV1::YouCastInstantOrSorceryOncePerTurn => Some(1),
        _ => None,
    }
}

const POLUKRANOS_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::StandardV1(StandardTriggerV1::ThisOrAnotherNontokenHydraYouControlDies),
    polukranos_hydras,
)];

/// The face `object` showed when the zone change at `events[index]` took
/// it off the battlefield, or `None` when that event is not its departure.
pub(crate) fn departure_face(
    events: &[CommittedEvent],
    index: usize,
    object: ObjectId,
) -> Option<u8> {
    let CommittedEvent::ZoneChange {
        object: moved,
        from: Zone::Battlefield,
        ..
    } = events.get(index)?
    else {
        return None;
    };
    if *moved != object {
        return None;
    }
    Some(
        events[..index]
            .iter()
            .rev()
            .take(2)
            .find_map(|event| match event {
                CommittedEvent::LeftBattlefieldFaceV1 {
                    object: departed,
                    face_index,
                    ..
                } if *departed == object => Some(*face_index),
                _ => None,
            })
            .unwrap_or(0),
    )
}

/// Whether a departure from the battlefield records its face, so leave
/// triggers printed on one face see the face that left.
pub(crate) fn records_departure_face(def: &CardDef) -> bool {
    def.transform_face.is_some() && matches!(def.name, POLUKRANOS | OJER | CECIL)
}

pub(crate) fn trigger_matches(
    condition: StandardTriggerV1,
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    match condition {
        StandardTriggerV1::ControllerBeginningOfCombat => {
            let live = state.objects.get(source);
            matches!(
                events[index],
                CommittedEvent::BeginningCombatV1 { active_player }
                    if active_player == live.controller
            ) && live.zone == Zone::Battlefield
                && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
        }
        StandardTriggerV1::CreatureCardExiledWithThis => {
            let CommittedEvent::ZoneChange {
                object,
                to: Zone::Exile,
                ..
            } = events[index]
            else {
                return false;
            };
            let live = state.objects.get(source);
            let exiled = state.objects.get(object);
            live.zone == Zone::Battlefield
                && exiled.zone == Zone::Exile
                && !exiled.v4.is_token
                && exiled.v4.exiled_by
                    == Some(crate::state::ObjectLinkV4 {
                        object: source,
                        zone_change_count: live.zone_change_count,
                    })
                && CARD_DEFS[exiled.card_def as usize].has_type(CardType::Creature)
        }
        StandardTriggerV1::ClassBecomesLevel { level } => matches!(
            events[index],
            CommittedEvent::ClassLevelGainedV1 { object, zone_change_count, level: gained }
                if object == source
                    && gained == level
                    && zone_change_count == state.objects.get(source).zone_change_count
        ),
        StandardTriggerV1::YouCastInstantOrSorceryOncePerTurn => {
            let CommittedEvent::SpellCast { spell, controller } = events[index] else {
                return false;
            };
            let live = state.objects.get(source);
            let def = &CARD_DEFS[state.objects.get(spell).card_def as usize];
            controller == live.controller
                && live.zone == Zone::Battlefield
                && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
                && (def.has_type(CardType::Instant) || def.has_type(CardType::Sorcery))
        }
        StandardTriggerV1::YouCastInstantOrSorceryAtClassLevel { level } => {
            let CommittedEvent::SpellCast { spell, controller } = events[index] else {
                return false;
            };
            let live = state.objects.get(source);
            let def = &CARD_DEFS[state.objects.get(spell).card_def as usize];
            controller == live.controller
                && live.zone == Zone::Battlefield
                && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
                && class_level(state, source) >= level
                && (def.has_type(CardType::Instant) || def.has_type(CardType::Sorcery))
        }
        StandardTriggerV1::ControllerEndStepIfLifeChanged { require_lost } => {
            let live = state.objects.get(source);
            controller_end_step(events, index, source, state)
                && life_changed_this_turn(state, live.controller, require_lost)
        }
        StandardTriggerV1::ControllerEndStepSolveCase => {
            controller_end_step(events, index, source, state)
                && !case_solved(state, source)
                && case_condition_met(state, source)
        }
        StandardTriggerV1::ControlledLegendaryCreatureEntersUnlessEverflame => {
            let Some(object) = crate::trigger::battlefield_entry_object(&events[index]) else {
                return false;
            };
            let live = state.objects.get(source);
            live.zone == Zone::Battlefield
                && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
                && !is_everflame(state, source)
                && state.objects.get(object).controller == live.controller
                && crate::engine::object_has_type(state, object, CardType::Creature)
                && CARD_DEFS[state.objects.get(object).card_def as usize]
                    .supertypes
                    .contains(&crate::card_def::Supertype::Legendary)
        }
        StandardTriggerV1::AnotherControlledArtifactEntersManaValueAtLeast(minimum) => {
            let Some(object) = crate::trigger::battlefield_entry_object(&events[index]) else {
                return false;
            };
            let live = state.objects.get(source);
            live.zone == Zone::Battlefield
                && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
                && object != source
                && state.objects.get(object).controller == live.controller
                && crate::engine::object_has_type(state, object, CardType::Artifact)
                && mana_value(state, object) >= u16::from(minimum)
        }
        StandardTriggerV1::ControllerEndStepWhileDoorUnlocked { door } => {
            let live = state.objects.get(source);
            matches!(
                events[index],
                CommittedEvent::BeginningEndStep { active_player, .. }
                    if active_player == live.controller
            ) && live.zone == Zone::Battlefield
                && door_unlocked(state, source, door)
        }
        StandardTriggerV1::YouUnlockThisDoor { door } => matches!(
            events[index],
            CommittedEvent::RoomDoorUnlockedV1 { object, zone_change_count, door: unlocked }
                if object == source
                    && unlocked == door
                    && zone_change_count == state.objects.get(source).zone_change_count
        ),
        StandardTriggerV1::ThisOrAnotherNontokenHydraYouControlDies => {
            let CommittedEvent::ZoneChange {
                object,
                from: Zone::Battlefield,
                to: Zone::Graveyard,
                controller_before,
            } = &events[index]
            else {
                return false;
            };
            if *object == source {
                return departure_face(events, index, source) == Some(1);
            }
            // The source sees another Hydra die while it is on the
            // battlefield showing this face, or as it leaves alongside it.
            let source_live = state.objects.get(source);
            let watching = (source_live.zone == Zone::Battlefield
                && source_live.v4.face_index == 1
                && crate::continuous_characteristics_v1::printed_abilities_active(state, source))
                || events.iter().enumerate().any(|(i, _)| {
                    departure_face(events, i, source) == Some(1)
                        && matches!(
                            events[i],
                            CommittedEvent::ZoneChange {
                                to: Zone::Graveyard,
                                ..
                            }
                        )
                });
            let source_controller = if source_live.zone == Zone::Battlefield {
                source_live.controller
            } else {
                events
                    .iter()
                    .find_map(|event| match event {
                        CommittedEvent::ZoneChange {
                            object,
                            from: Zone::Battlefield,
                            controller_before,
                            ..
                        } if *object == source => Some(*controller_before),
                        _ => None,
                    })
                    .unwrap_or(source_live.controller)
            };
            let died = state.objects.get(*object);
            let def = &CARD_DEFS[died.card_def as usize];
            watching
                && *controller_before == source_controller
                && !died.v4.is_token
                && def.has_type(CardType::Creature)
                && def.subtypes.contains(&Subtype::Hydra)
        }
    }
}

// ---- Ojer Axonil, Deepest Might // Temple of Power -------------------------

const CECIL: &str = "Cecil, Dark Knight";
const POLUKRANOS: &str = "Polukranos Reborn";
const OJER: &str = "Ojer Axonil, Deepest Might";

fn ojer_returns() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::ReturnSourceTappedAndTransformed)
}

const OJER_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::LeftBattlefieldToGraveyard,
    ojer_returns,
)];

/// "If a red source you control would deal an amount of noncombat damage
/// less than Ojer Axonil's power to an opponent, that source deals damage
/// equal to Ojer Axonil's power instead."
pub(crate) fn replace_damage(state: &GameState, damage: &mut DamageProposed) {
    if damage.is_combat || damage.amount <= 0 {
        return;
    }
    let Target::Player(damaged) = damage.target else {
        return;
    };
    let Some(source) = state.objects.try_get(damage.source) else {
        return;
    };
    let controller = source.controller;
    if damaged != controller.opponent()
        || crate::engine::object_color_mask(state, damage.source)
            & crate::card_def::mana_colors_mask(&[ManaColor::R])
            == 0
    {
        return;
    }
    let power = state.players[controller.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| {
            let live = state.objects.get(id);
            live.v4.face_index == 0
                && CARD_DEFS[live.card_def as usize].name == OJER
                && crate::continuous_characteristics_v1::printed_abilities_active(state, id)
        })
        .map(|id| crate::engine::effective_power(state, id))
        .max();
    if let Some(power) = power {
        if damage.amount < power {
            damage.amount = power;
        }
    }
}

/// Records noncombat damage dealt by red sources for Temple of Power.
#[cfg(feature = "standard-magezero-fixtures")]
pub(crate) fn after_damage(state: &mut GameState, source: ObjectId, amount: i32, is_combat: bool) {
    if is_combat || amount <= 0 {
        return;
    }
    let Some(live) = state.objects.try_get(source) else {
        return;
    };
    if crate::engine::object_color_mask(state, source)
        & crate::card_def::mana_colors_mask(&[ManaColor::R])
        == 0
    {
        return;
    }
    let controller = live.controller;
    let turn = state.turn;
    let standard = state.standard_v1.get_or_insert_with(Default::default);
    let (recorded_turn, totals) = standard.red_noncombat_damage.get_or_insert((turn, [0, 0]));
    if *recorded_turn != turn {
        *recorded_turn = turn;
        *totals = [0, 0];
    }
    let total = &mut totals[controller.index()];
    *total = total.saturating_add(amount.unsigned_abs());
}

fn red_noncombat_damage_this_turn(state: &GameState, player: PlayerId) -> u32 {
    state
        .standard_v1
        .as_ref()
        .and_then(|standard| standard.red_noncombat_damage)
        .filter(|(turn, _)| *turn == state.turn)
        .map_or(0, |(_, totals)| totals[player.index()])
}

/// Extra activation restrictions printed on this module's abilities.
pub(crate) fn activation_allowed(
    state: &GameState,
    source: ObjectId,
    def: &CardDef,
    ability_index: usize,
) -> bool {
    if activations_locked(state, source) {
        return false;
    }
    if def.name == ROOM {
        // A door can be unlocked only while it is locked.
        return u8::try_from(ability_index)
            .is_ok_and(|door| door < 2 && !door_unlocked(state, source, door));
    }
    if def.name == THE_IRENCRAG {
        // Equip {3} exists only once it is Everflame.
        return is_everflame(state, source);
    }
    if def.name == CASE_OF_THE_UNEATEN_FEAST {
        // "Solved -- Sacrifice this Case: ..."
        return case_solved(state, source);
    }
    if is_class(def.name) {
        // Level N+1 can be gained only at level N (716.2a).
        return usize::from(class_level(state, source)) == ability_index + 1;
    }
    if def.transform_face.is_none() {
        return true;
    }
    match (def.name, ability_index) {
        // "Activate only if red sources you controlled dealt 4 or more
        // noncombat damage this turn."
        (OJER, 0) => {
            red_noncombat_damage_this_turn(state, state.objects.get(source).controller) >= 4
        }
        _ => true,
    }
}

/// Whether `object`'s mana abilities exist on the face it shows.
pub(crate) fn mana_abilities_active(state: &GameState, object: ObjectId, def: &CardDef) -> bool {
    if activations_locked(state, object) {
        return false;
    }
    if def.name == THE_IRENCRAG {
        // Everflame loses all other abilities.
        return !is_everflame(state, object);
    }
    def.transform_face.is_none() || def.name != OJER || state.objects.get(object).v4.face_index == 1
}

/// Triggered abilities of this module's cards, by registry name.
pub(crate) fn triggers_for(name: &str) -> &'static [TriggeredAbilityDef] {
    match name {
        "Teferi, Temporal Pilgrim" => &TEFERI_TRIGGERS,
        "Teferi Spirit Token" => &TEFERI_SPIRIT_TRIGGERS,
        CECIL => &CECIL_TRIGGERS,
        POLUKRANOS => &POLUKRANOS_TRIGGERS,
        OJER => &OJER_TRIGGERS,
        ROOM => &ROOM_TRIGGERS,
        CANDY_TRAIL => &SCRY_TWO_ON_ENTRY_TRIGGERS,
        WARLEADERS_CALL => &WARLEADERS_CALL_TRIGGERS,
        LUNAR_CONVOCATION => &LUNAR_CONVOCATION_TRIGGERS,
        SIMULACRUM_SYNTHESIZER => &SIMULACRUM_SYNTHESIZER_TRIGGERS,
        CASE_OF_THE_GATEWAY_EXPRESS => &GATEWAY_EXPRESS_TRIGGERS,
        CASE_OF_THE_UNEATEN_FEAST => &UNEATEN_FEAST_TRIGGERS,
        INNKEEPERS_TALENT => &INNKEEPERS_TALENT_TRIGGERS,
        STORMCHASERS_TALENT => &STORMCHASERS_TALENT_TRIGGERS,
        FABLE_GOBLIN_SHAMAN_TOKEN => &FABLE_GOBLIN_SHAMAN_TRIGGERS,
        THE_IRENCRAG => &THE_IRENCRAG_TRIGGERS,
        CLAY_FIRED_BRICKS => &CLAY_FIRED_BRICKS_TRIGGERS,
        CHANDRA => &CHANDRA_TRIGGERS,
        AGATHAS_SOUL_CAULDRON => &CAULDRON_TRIGGERS,
        "Otter Prowess Token" => &PROWESS_TRIGGERS,
        SEAM_RIP
        | DUSK_ROSE_RELIQUARY
        | SHELTERED_BY_GHOSTS
        | HARDLIGHT_CONTAINMENT
        | ASSIMILATION_AEGIS => &EXILE_UNTIL_LEAVES_TRIGGERS,
        _ => &[],
    }
}

/// The transforming-card face each triggered ability is printed on. Abilities
/// of single-faced cards report face 0.
pub(crate) fn trigger_face(name: &str, ability_index: usize) -> u8 {
    match (name, ability_index) {
        (CECIL, 1) | (POLUKRANOS, 0) | (CLAY_FIRED_BRICKS, 1) => 1,
        _ => 0,
    }
}

// ---- Blue Sun's Twilight ---------------------------------------------------

const BLUE_SUNS_TWILIGHT: &str = "Blue Sun's Twilight";

pub fn blue_suns_twilight() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::GainControlOfTargetCreatureCopyAtXFive)
}

/// Whether this card's spell targets depend on more than its target spec.
pub(crate) fn filters_cast_targets(def: &CardDef) -> bool {
    def.cost.x_count != 0 && def.name == BLUE_SUNS_TWILIGHT
}

/// "Target creature with mana value X or less": the kernel announces X after
/// targets, so a creature is a legal choice when some payable X reaches its
/// mana value, and X is then at least that mana value (`minimum_x`).
pub(crate) fn cast_target_allowed(
    def: &CardDef,
    controller: PlayerId,
    target: Target,
    state: &GameState,
) -> bool {
    match target {
        Target::Object(object) if def.name == BLUE_SUNS_TWILIGHT => {
            crate::engine::maximum_payable_x(&def.cost, controller, state)
                .is_some_and(|maximum| mana_value(state, object) <= u16::from(maximum))
        }
        _ => true,
    }
}

/// The least X this spell may announce with these targets.
pub(crate) fn minimum_x(def: &CardDef, targets: &[Target], state: &GameState) -> u8 {
    match (def.name, targets.first()) {
        (BLUE_SUNS_TWILIGHT, Some(Target::Object(object))) if def.cost.x_count != 0 => {
            u8::try_from(mana_value(state, *object)).unwrap_or(u8::MAX)
        }
        _ => 0,
    }
}

// ---- Rooms: Unholy Annex // Ritual Chamber ---------------------------------

const ROOM: &str = "Unholy Annex // Ritual Chamber";

/// Whether `door` (0 left, 1 right) of this exact Room incarnation is
/// unlocked.
pub(crate) fn door_unlocked(state: &GameState, object: ObjectId, door: u8) -> bool {
    let live = state.objects.get(object);
    live.zone == Zone::Battlefield
        && state.standard_v1.as_ref().is_some_and(|standard| {
            standard.rooms.iter().any(|room| {
                room.object == object
                    && room.zone_change_count == live.zone_change_count
                    && room.unlocked & (1 << door) != 0
            })
        })
}

/// Unlocks `door` of a battlefield Room and records the unlock event that
/// "when you unlock this door" abilities trigger on. Unlocking an unlocked
/// door does nothing.
fn unlock_door(state: &mut GameState, object: ObjectId, door: u8) {
    if door > 1 || door_unlocked(state, object, door) {
        return;
    }
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield {
        return;
    }
    let zone_change_count = live.zone_change_count;
    let standard = state.standard_v1.get_or_insert_with(Default::default);
    // Forget Rooms that have since left the battlefield.
    let objects = &state.objects;
    standard.rooms.retain(|room| {
        objects.try_get(room.object).is_some_and(|live| {
            live.zone == Zone::Battlefield && live.zone_change_count == room.zone_change_count
        })
    });
    match standard
        .rooms
        .iter_mut()
        .find(|room| room.object == object && room.zone_change_count == zone_change_count)
    {
        Some(room) => room.unlocked |= 1 << door,
        None => standard.rooms.push(RoomDoorsV1 {
            object,
            zone_change_count,
            unlocked: 1 << door,
        }),
    }
    state
        .engine
        .event_log
        .push(CommittedEvent::RoomDoorUnlockedV1 {
            object,
            zone_change_count,
            door,
        });
}

/// A resolving Room spell enters the battlefield. The half that was cast is
/// the one whose door unlocks: the left half is the card's normal cost and
/// the right half its alternative cost.
fn room_enters(ctx: &ExecCtx, state: &mut GameState) {
    let door = match state
        .objects
        .get(ctx.source)
        .v4
        .spell_cast_origin
        .and_then(|origin| origin.finalized_method)
    {
        Some(crate::state::CastMethodV4::Normal) => Some(0),
        Some(crate::state::CastMethodV4::Alternative) => Some(1),
        _ => None,
    };
    crate::effect::execute(
        &EffectOp::MoveObject {
            object: crate::effect::ObjectRef::ThisSource,
            to_zone: Zone::Battlefield,
        },
        ctx,
        state,
    );
    if let Some(door) = door {
        if state.objects.get(ctx.source).zone == Zone::Battlefield {
            unlock_door(state, ctx.source, door);
        }
    }
}

pub fn room_enters_program() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::RoomEnters)
}

/// Unlocking a door is a special action (709.5e): it doesn't use the stack.
pub fn unlock_left_door() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::UnlockSourceDoor { door: 0 })
}

pub fn unlock_right_door() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::UnlockSourceDoor { door: 1 })
}

/// Whether this activated ability is a special action that takes effect
/// as soon as its cost is paid instead of using the stack.
pub(crate) fn is_special_action(def: &CardDef, ability_index: u8) -> bool {
    def.name == ROOM && ability_index < 2
}

/// Performs a paid special action of `source`.
pub(crate) fn special_action(state: &mut GameState, source: ObjectId, ability_index: u8) {
    unlock_door(state, source, ability_index);
}

/// Unholy Annex: "At the beginning of your end step, draw a card. If you
/// control a Demon, each opponent loses 2 life and you gain 2 life.
/// Otherwise, you lose 2 life."
fn unholy_annex_end_step() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
        EffectOp::Conditional {
            cond: crate::effect::EffectCond::ControlsOtherSubtypeCount {
                subtype: Subtype::Demon,
                minimum_count: 1,
            },
            then: Box::new(EffectOp::Sequence(vec![
                EffectOp::LoseLife {
                    player: PlayerRef::Opponent,
                    amount: 2,
                },
                EffectOp::GainLife {
                    player: PlayerRef::Controller,
                    amount: 2,
                },
            ])),
            else_: Box::new(EffectOp::LoseLife {
                player: PlayerRef::Controller,
                amount: 2,
            }),
        },
    ])
}

/// Ritual Chamber: "When you unlock this door, create a 6/6 black Demon
/// creature token with flying."
fn ritual_chamber_demon() -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name("Demon Flying Token")
            .expect("Demon Flying Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}

const ROOM_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::ControllerEndStepWhileDoorUnlocked {
            door: 0,
        }),
        unholy_annex_end_step,
    ),
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::YouUnlockThisDoor { door: 1 }),
        ritual_chamber_demon,
    ),
];

// ---- Exile until this leaves, and Auras -------------------------------------

const SEAM_RIP: &str = "Seam Rip";
const DUSK_ROSE_RELIQUARY: &str = "Dusk Rose Reliquary";
const SHELTERED_BY_GHOSTS: &str = "Sheltered by Ghosts";
const HARDLIGHT_CONTAINMENT: &str = "Hardlight Containment";

fn exile_target_until_source_leaves() -> EffectOp {
    EffectOp::ExileTargetLinkedToSource {
        object: crate::effect::ObjectRef::Target(0),
    }
}

fn return_exiled_by_source() -> EffectOp {
    EffectOp::ReturnObjectsExiledBySource
}

/// "When this enters, exile target ... until this leaves the battlefield."
const EXILE_UNTIL_LEAVES_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, exile_target_until_source_leaves),
    TriggeredAbilityDef {
        condition: TriggerCondition::LeftBattlefield,
        // Leave triggers use the departed battlefield incarnation.
        home_zone: Zone::Graveyard,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
        effect: return_exiled_by_source,
    },
];

/// The target of a triggered ability of this module's cards, by printed
/// name and the trigger's effect; `None` when the module doesn't own it.
pub(crate) fn trigger_target_spec(
    name: &str,
    effect: &EffectOp,
) -> Option<crate::card_def::TargetSpec> {
    use crate::card_def::TargetSpec;
    if name == CASE_OF_THE_GATEWAY_EXPRESS && *effect == gateway_express_damage() {
        return Some(TargetSpec::OpponentControlledCreature);
    }
    if name == AGATHAS_SOUL_CAULDRON && *effect == cauldron_counter() {
        return Some(TargetSpec::ControlledCreature);
    }
    if name == INNKEEPERS_TALENT && *effect == innkeeper_counter() {
        return Some(TargetSpec::ControlledCreature);
    }
    if name == STORMCHASERS_TALENT && *effect == stormchaser_regrowth() {
        return Some(TargetSpec::StandardV1(
            StandardTargetV1::InstantOrSorceryCardInOwnGraveyard,
        ));
    }
    if *effect != exile_target_until_source_leaves() {
        return None;
    }
    Some(match name {
        SEAM_RIP => {
            TargetSpec::StandardV1(StandardTargetV1::OpponentNonlandPermanentManaValueAtMost(2))
        }
        DUSK_ROSE_RELIQUARY => TargetSpec::StandardV1(StandardTargetV1::OpponentArtifactOrCreature),
        SHELTERED_BY_GHOSTS => TargetSpec::StandardV1(StandardTargetV1::OpponentNonlandPermanent),
        HARDLIGHT_CONTAINMENT => TargetSpec::OpponentControlledCreature,
        ASSIMILATION_AEGIS => TargetSpec::StandardV1(StandardTargetV1::UpToOneCreature),
        _ => return None,
    })
}

pub fn aura_enters() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::AuraEnters)
}

/// Whether `def` is one of this module's Auras.
pub(crate) fn is_aura(def: &CardDef) -> bool {
    matches!(def.name, SHELTERED_BY_GHOSTS | HARDLIGHT_CONTAINMENT)
}

/// Whether this module's Aura `def`, controlled by `controller`, may
/// enchant `host`: Sheltered by Ghosts "enchant creature you control",
/// Hardlight Containment "enchant artifact you control". `None` for every
/// other definition.
pub(crate) fn aura_host_legal(
    def: &CardDef,
    controller: PlayerId,
    host: ObjectId,
    state: &GameState,
) -> Option<bool> {
    let live = state.objects.get(host);
    let type_ok = |card_type| crate::engine::object_has_type(state, host, card_type);
    let card_type = match def.name {
        SHELTERED_BY_GHOSTS => CardType::Creature,
        HARDLIGHT_CONTAINMENT => CardType::Artifact,
        _ => return None,
    };
    Some(live.zone == Zone::Battlefield && live.controller == controller && type_ok(card_type))
}

fn aura_enters_battlefield(ctx: &ExecCtx, state: &mut GameState) {
    let Some(Target::Object(host)) = ctx.targets.first().copied() else {
        return;
    };
    let def = &CARD_DEFS[state.objects.get(ctx.source).card_def as usize];
    let legal = state.objects.get(ctx.source).zone == Zone::Stack
        && ctx.target_incarnation_matches(0, state)
        && aura_host_legal(def, ctx.controller, host, state) == Some(true);
    if !legal {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
        return;
    }
    event::propose_and_commit(
        state,
        ProposedEvent::zone_change(ctx.source, Zone::Battlefield),
    );
    let aura = state.objects.get(ctx.source);
    if aura.zone != Zone::Battlefield {
        return;
    }
    let aura_generation = aura.zone_change_count;
    let host_generation = state.objects.get(host).zone_change_count;
    if state
        .attach_object_exact(ctx.source, aura_generation, host, host_generation)
        .is_err()
    {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
    }
}

/// Ward amounts `host` gains from this module's Auras attached to it
/// ("Enchanted permanent has ward {N}").
pub(crate) fn granted_wards(state: &GameState, host: ObjectId) -> Vec<u8> {
    let Some(host_live) = state.objects.try_get(host) else {
        return Vec::new();
    };
    let link = crate::state::ObjectLinkV4 {
        object: host,
        zone_change_count: host_live.zone_change_count,
    };
    let mut wards = host_live
        .attachments
        .iter()
        .filter_map(|&attached| {
            let aura = state.objects.try_get(attached)?;
            if aura.zone != Zone::Battlefield || aura.v4.attached_to != Some(link) {
                return None;
            }
            granted_ward_of(CARD_DEFS[aura.card_def as usize].name)
        })
        .collect::<Vec<_>>();
    // Innkeeper's Talent level 2: "Permanents you control with counters on
    // them have ward {1}."
    if host_live.zone == Zone::Battlefield && has_counters(state, host) {
        let classes = controlled_classes_at(state, host_live.controller, INNKEEPERS_TALENT, 2);
        wards.extend((0..classes).map(|_| 1));
    }
    wards
}

fn granted_ward_of(name: &str) -> Option<u8> {
    match name {
        SHELTERED_BY_GHOSTS => Some(2),
        HARDLIGHT_CONTAINMENT => Some(1),
        _ => None,
    }
}

/// Whether some definition in this catalog grants ward `generic`, so a
/// ward trigger restored after its Aura left can still be recognized.
pub(crate) fn ward_grant_exists(generic: u8) -> bool {
    CARD_DEFS
        .iter()
        .any(|def| granted_ward_of(def.name) == Some(generic))
}

// ---- Life, attack and anthem watchers --------------------------------------

/// Records a life total change for this turn's "gained life"/"lost life"
/// conditions.
#[cfg(feature = "standard-magezero-fixtures")]
pub(crate) fn record_life(state: &mut GameState, player: PlayerId, gained: i32, lost: i32) {
    if gained <= 0 && lost <= 0 {
        return;
    }
    let turn = state.turn;
    let standard = state.standard_v1.get_or_insert_with(Default::default);
    let (recorded_turn, gains, losses) =
        standard
            .life_this_turn
            .get_or_insert((turn, [0, 0], [0, 0]));
    if *recorded_turn != turn {
        *recorded_turn = turn;
        *gains = [0, 0];
        *losses = [0, 0];
    }
    let index = player.index();
    gains[index] = gains[index].saturating_add(gained.max(0).unsigned_abs());
    losses[index] = losses[index].saturating_add(lost.max(0).unsigned_abs());
}

fn life_gained_this_turn(state: &GameState, player: PlayerId) -> u32 {
    state
        .standard_v1
        .as_ref()
        .and_then(|standard| standard.life_this_turn)
        .filter(|(turn, _, _)| *turn == state.turn)
        .map_or(0, |(_, gains, _)| gains[player.index()])
}

fn life_changed_this_turn(state: &GameState, player: PlayerId, require_lost: bool) -> bool {
    state
        .standard_v1
        .as_ref()
        .and_then(|standard| standard.life_this_turn)
        .filter(|(turn, _, _)| *turn == state.turn)
        .is_some_and(|(_, gains, losses)| {
            gains[player.index()] > 0 && (!require_lost || losses[player.index()] > 0)
        })
}

/// Records declared attackers for this turn's "creatures attacked this
/// turn" count.
#[cfg(feature = "standard-magezero-fixtures")]
pub(crate) fn record_attackers(state: &mut GameState, count: usize) {
    if count == 0 {
        return;
    }
    let turn = state.turn;
    let standard = state.standard_v1.get_or_insert_with(Default::default);
    let (recorded_turn, total) = standard.attackers_this_turn.get_or_insert((turn, 0));
    if *recorded_turn != turn {
        *recorded_turn = turn;
        *total = 0;
    }
    *total = total.saturating_add(u32::try_from(count).unwrap_or(u32::MAX));
}

fn attackers_this_turn(state: &GameState) -> u32 {
    state
        .standard_v1
        .as_ref()
        .and_then(|standard| standard.attackers_this_turn)
        .filter(|(turn, _)| *turn == state.turn)
        .map_or(0, |(_, total)| total)
}

/// "At the beginning of your end step", for a source on the battlefield
/// whose printed abilities function.
fn controller_end_step(
    events: &[CommittedEvent],
    index: usize,
    source: ObjectId,
    state: &GameState,
) -> bool {
    let live = state.objects.get(source);
    matches!(
        events[index],
        CommittedEvent::BeginningEndStep { active_player, .. } if active_player == live.controller
    ) && live.zone == Zone::Battlefield
        && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
}

/// Power and toughness this module's static abilities give `object`:
/// Warleader's Call's and a solved Case of the Gateway Express's anthems,
/// and the Construct token's "+1/+1 for each artifact you control".
#[cfg(feature = "standard-magezero-fixtures")]
pub(crate) fn controlled_boost(state: &GameState, object: ObjectId) -> (i32, i32) {
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, object, CardType::Creature)
    {
        return (0, 0);
    }
    let controller = live.controller;
    let mut boost = (0, 0);
    let mut artifacts = 0;
    for &id in &state.players[controller.index()].battlefield {
        if crate::engine::object_has_type(state, id, CardType::Artifact) {
            artifacts += 1;
        }
        let name = CARD_DEFS[state.objects.get(id).card_def as usize].name;
        if !matches!(
            name,
            WARLEADERS_CALL | CASE_OF_THE_GATEWAY_EXPRESS | CLAY_FIRED_BRICKS
        ) || !crate::continuous_characteristics_v1::printed_abilities_active(state, id)
        {
            continue;
        }
        if name == CLAY_FIRED_BRICKS {
            // Cosmium Kiln: "Creatures you control get +1/+1."
            if state.objects.get(id).v4.face_index == 1 {
                boost.0 += 1;
                boost.1 += 1;
            }
        } else if name == WARLEADERS_CALL {
            boost.0 += 1;
            boost.1 += 1;
        } else if case_solved(state, id) {
            boost.0 += 1;
        }
    }
    if CARD_DEFS[live.card_def as usize].name == KARN_CONSTRUCT_TOKEN
        && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
    {
        boost.0 += artifacts;
        boost.1 += artifacts;
    }
    boost
}

// ---- Candy Trail, Warleader's Call, Lunar Convocation, Simulacrum ----------

const CANDY_TRAIL: &str = "Candy Trail";
const WARLEADERS_CALL: &str = "Warleader's Call";
const LUNAR_CONVOCATION: &str = "Lunar Convocation";
const SIMULACRUM_SYNTHESIZER: &str = "Simulacrum Synthesizer";
const KARN_CONSTRUCT_TOKEN: &str = "Karn Construct Token";

fn scry_two() -> EffectOp {
    EffectOp::Scry {
        player: PlayerRef::Controller,
        count: 2,
    }
}

/// "When this enters, scry 2."
const SCRY_TWO_ON_ENTRY_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Etb, scry_two)];

/// Candy Trail: "{2}, {T}, Sacrifice this: You gain 3 life and draw a card."
pub fn candy_trail_sacrifice() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 3,
        },
        EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        },
    ])
}

fn warleaders_call_damage() -> EffectOp {
    EffectOp::DealDamage {
        target: crate::effect::TargetRef::Opponent,
        amount: 1,
    }
}

/// "Whenever a creature you control enters, this deals 1 damage to each
/// opponent." Its anthem is `controlled_boost`.
const WARLEADERS_CALL_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::OtherControlledCreatureEnters { subtype: None },
    warleaders_call_damage,
)];

fn lunar_convocation_drain() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::OpponentLosesLifeIfYouGainedLife { amount: 1 })
}

fn lunar_convocation_bat() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::CreateTokenIfYouGainedAndLostLife {
        token_def: crate::card_def::card_id_by_name("Bat Flying Token")
            .expect("Bat Flying Token in CARD_DEFS"),
    })
}

const LUNAR_CONVOCATION_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::ControllerEndStepIfLifeChanged {
            require_lost: false,
        }),
        lunar_convocation_drain,
    ),
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::ControllerEndStepIfLifeChanged {
            require_lost: true,
        }),
        lunar_convocation_bat,
    ),
];

fn simulacrum_construct() -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name(KARN_CONSTRUCT_TOKEN)
            .expect("Karn Construct Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}

const SIMULACRUM_SYNTHESIZER_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, scry_two),
    trigger(
        TriggerCondition::StandardV1(
            StandardTriggerV1::AnotherControlledArtifactEntersManaValueAtLeast(3),
        ),
        simulacrum_construct,
    ),
];

// ---- Cases -------------------------------------------------------------------

const CASE_OF_THE_GATEWAY_EXPRESS: &str = "Case of the Gateway Express";

/// Whether `object`'s current battlefield incarnation is a solved Case.
pub(crate) fn case_solved(state: &GameState, object: ObjectId) -> bool {
    let zone_change_count = state.objects.get(object).zone_change_count;
    state
        .standard_v1
        .as_ref()
        .is_some_and(|standard| standard.solved_cases.contains(&(object, zone_change_count)))
}

/// A Case's "to solve" condition.
fn case_condition_met(state: &GameState, object: ObjectId) -> bool {
    match CARD_DEFS[state.objects.get(object).card_def as usize].name {
        // "Three or more creatures attacked this turn."
        CASE_OF_THE_GATEWAY_EXPRESS => attackers_this_turn(state) >= 3,
        // "You've gained 5 or more life this turn."
        CASE_OF_THE_UNEATEN_FEAST => {
            life_gained_this_turn(state, state.objects.get(object).controller) >= 5
        }
        _ => false,
    }
}

fn gateway_express_damage() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EachControlledCreatureDealsOneDamageToTarget)
}

fn solve_source_case() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::SolveSourceCase)
}

/// "When this Case enters, choose target creature you don't control. Each
/// creature you control deals 1 damage to that creature." To solve: three
/// or more creatures attacked this turn. Solved: creatures you control get
/// +1/+0 (`controlled_boost`).
const GATEWAY_EXPRESS_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, gateway_express_damage),
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::ControllerEndStepSolveCase),
        solve_source_case,
    ),
];

// ---- Classes -----------------------------------------------------------------

const INNKEEPERS_TALENT: &str = "Innkeeper's Talent";
const STORMCHASERS_TALENT: &str = "Stormchaser's Talent";

fn is_class(name: &str) -> bool {
    matches!(name, INNKEEPERS_TALENT | STORMCHASERS_TALENT)
}

/// A Class permanent's level; a Class enters at level 1 (716.3).
pub(crate) fn class_level(state: &GameState, object: ObjectId) -> u8 {
    let zone_change_count = state.objects.get(object).zone_change_count;
    state
        .standard_v1
        .as_ref()
        .and_then(|standard| {
            standard
                .class_levels
                .iter()
                .find(|(id, count, _)| *id == object && *count == zone_change_count)
        })
        .map_or(1, |(_, _, level)| *level)
}

fn set_class_level(state: &mut GameState, object: ObjectId, level: u8) {
    let zone_change_count = state.objects.get(object).zone_change_count;
    let live = |entry: &(ObjectId, u32, u8)| {
        state
            .objects
            .try_get(entry.0)
            .is_some_and(|live| live.zone == Zone::Battlefield && live.zone_change_count == entry.1)
    };
    let mut levels = state
        .standard_v1
        .as_ref()
        .map(|standard| standard.class_levels.clone())
        .unwrap_or_default();
    levels.retain(|entry| live(entry) && entry.0 != object);
    levels.push((object, zone_change_count, level));
    state
        .standard_v1
        .get_or_insert_with(Default::default)
        .class_levels = levels;
    let committed = CommittedEvent::ClassLevelGainedV1 {
        object,
        zone_change_count,
        level,
    };
    state.engine.event_log.push(committed.clone());
    state.engine.event_history.push(committed);
}

pub fn gain_level_two() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::GainClassLevel { level: 2 })
}

pub fn gain_level_three() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::GainClassLevel { level: 3 })
}

/// Whether `player` controls a permanent showing a Class at `level` or
/// higher with functioning abilities, counted once per such Class.
fn controlled_classes_at(state: &GameState, player: PlayerId, name: &str, level: u8) -> u32 {
    let count = state.players[player.index()]
        .battlefield
        .iter()
        .filter(|&&id| {
            CARD_DEFS[state.objects.get(id).card_def as usize].name == name
                && class_level(state, id) >= level
                && crate::continuous_characteristics_v1::printed_abilities_active(state, id)
        })
        .count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// Innkeeper's Talent level 3: "If you would put one or more counters on a
/// permanent or player, put twice that many of each of those kinds of
/// counters on that permanent or player instead." Each such Class doubles
/// again (616.1).
#[cfg(feature = "standard-magezero-fixtures")]
pub(crate) fn scale_counters(state: &GameState, player: PlayerId, count: i32) -> i32 {
    if count <= 0 {
        return count;
    }
    let doublings = controlled_classes_at(state, player, INNKEEPERS_TALENT, 3);
    (0..doublings).fold(count, |count, _| count.saturating_mul(2))
}

/// Whether `object` has any counters on it.
fn has_counters(state: &GameState, object: ObjectId) -> bool {
    let live = state.objects.get(object);
    let counters = &live.counters;
    counters.plus1_plus1 > 0
        || counters.minus1_minus1 > 0
        || counters.minus0_minus1 > 0
        || counters.stun > 0
        || counters.lore > 0
        || live.v4.lifelink_keyword_counters > 0
        || crate::planeswalker_v1::loyalty(state, object).is_some_and(|loyalty| loyalty > 0)
}

fn innkeeper_counter() -> EffectOp {
    EffectOp::AddCountersToTarget {
        target_index: 0,
        optional: false,
        plus1_plus1: 1,
        lifelink: 0,
        stun: 0,
    }
}

/// Level 1: "At the beginning of combat on your turn, put a +1/+1 counter
/// on target creature you control." Level 2's ward is `granted_wards`;
/// level 3's doubling is `scale_counters`.
const INNKEEPERS_TALENT_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::StandardV1(StandardTriggerV1::ControllerBeginningOfCombat),
    innkeeper_counter,
)];

fn otter_token() -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name("Otter Prowess Token")
            .expect("Otter Prowess Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}

fn stormchaser_regrowth() -> EffectOp {
    EffectOp::MoveObject {
        object: crate::effect::ObjectRef::Target(0),
        to_zone: Zone::Hand,
    }
}

/// Level 1: an Otter on entry. Level 2: "When this Class becomes level 2,
/// return target instant or sorcery card from your graveyard to your hand."
/// Level 3: "Whenever you cast an instant or sorcery spell, create" an
/// Otter.
const STORMCHASERS_TALENT_TRIGGERS: [TriggeredAbilityDef; 3] = [
    trigger(TriggerCondition::Etb, otter_token),
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::ClassBecomesLevel { level: 2 }),
        stormchaser_regrowth,
    ),
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::YouCastInstantOrSorceryAtClassLevel {
            level: 3,
        }),
        otter_token,
    ),
];

fn prowess_boost() -> EffectOp {
    EffectOp::BindTemporaryBoostToTriggerSource {
        power: 1,
        toughness: 1,
    }
}

/// Prowess (702.108): "Whenever you cast a noncreature spell, this creature
/// gets +1/+1 until end of turn."
const PROWESS_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::CastNoncreatureSpell,
    prowess_boost,
)];

// ---- Case of the Uneaten Feast ----------------------------------------------

const CASE_OF_THE_UNEATEN_FEAST: &str = "Case of the Uneaten Feast";

/// Whether `holder` may cast `object` from its graveyard incarnation
/// `zone_change_count` this turn.
pub(crate) fn graveyard_cast_granted(
    state: &GameState,
    object: ObjectId,
    holder: PlayerId,
    zone_change_count: u32,
) -> bool {
    state.standard_v1.as_ref().is_some_and(|standard| {
        standard
            .graveyard_casts
            .contains(&(object, zone_change_count, holder, state.turn))
    })
}

fn gain_one_life() -> EffectOp {
    EffectOp::GainLife {
        player: PlayerRef::Controller,
        amount: 1,
    }
}

/// "Solved -- Sacrifice this Case: Creature cards in your graveyard gain
/// 'You may cast this card from your graveyard' until end of turn." Usable
/// only while solved (`activation_allowed`).
pub fn uneaten_feast_grant() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::GraveyardCreaturesCastableThisTurn)
}

/// "Whenever a creature you control enters, you gain 1 life." To solve:
/// you've gained 5 or more life this turn.
const UNEATEN_FEAST_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(
        TriggerCondition::OtherControlledCreatureEnters { subtype: None },
        gain_one_life,
    ),
    trigger(
        TriggerCondition::StandardV1(StandardTriggerV1::ControllerEndStepSolveCase),
        solve_source_case,
    ),
];

// ---- Liliana of the Veil -------------------------------------------------------

/// After a resolving ability's discard lands, stages the next discard that
/// ability still owes, keeping it on the stack. Returns whether one was
/// staged.
#[cfg(feature = "standard-magezero-fixtures")]
pub(crate) fn stage_queued_discard(
    state: &mut GameState,
    stack_item_id: crate::ids::StackItemId,
) -> bool {
    let Some(standard) = state.standard_v1.as_mut() else {
        return false;
    };
    let Some(index) = standard
        .queued_discards
        .iter()
        .position(|(item, _, _)| *item == stack_item_id)
    else {
        return false;
    };
    let (_, player, count) = standard.queued_discards.remove(index);
    state.engine.pending_discard = Some(crate::engine::PendingDiscard {
        player,
        count,
        resume: crate::engine::DiscardResume::FinishAbilityResolution { stack_item_id },
    });
    true
}

pub fn each_player_discards_one() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EachPlayerDiscards { count: 1 })
}

pub fn target_player_sacrifices_creature() -> EffectOp {
    EffectOp::SacrificeCreature {
        player: PlayerRef::Target(0),
        filter: crate::effect::CreatureSacrificeFilter::Any,
    }
}

// ---- Fable of the Mirror-Breaker // Reflection of Kiki-Jiki ---------------

const FABLE_GOBLIN_SHAMAN_TOKEN: &str = "Fable Goblin Shaman Token";

/// Chapter I: a 2/2 red Goblin Shaman with "Whenever this creature
/// attacks, create a Treasure token."
pub fn fable_chapter_one() -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name(FABLE_GOBLIN_SHAMAN_TOKEN)
            .expect("Fable Goblin Shaman Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}

/// Chapter II: "You may discard up to two cards. If you do, draw that many
/// cards."
pub fn fable_chapter_two() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::MayDiscardUpToThenDraw {
        player: PlayerRef::Controller,
        count: 2,
    })
}

/// Chapter III: exile the Saga, then return it transformed.
pub fn fable_chapter_three() -> EffectOp {
    EffectOp::TransformSagaSource
}

fn goblin_shaman_treasure() -> EffectOp {
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name("Treasure Token")
            .expect("Treasure Token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    }
}

const FABLE_GOBLIN_SHAMAN_TRIGGERS: [TriggeredAbilityDef; 1] =
    [trigger(TriggerCondition::Attacks, goblin_shaman_treasure)];

pub fn reflection_of_kiki_jiki_copy() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::CopyTargetWithHasteSacrificeAtEndStep)
}

/// The exact cards in `player`'s hand, in hand order.
pub(crate) fn hand_candidates(state: &GameState, player: PlayerId) -> Vec<EffectObjectBinding> {
    state.players[player.index()]
        .hand
        .iter()
        .map(|&card| EffectObjectBinding {
            object: card,
            expected_zone: Zone::Hand,
            expected_zone_change_count: state.objects.get(card).zone_change_count,
        })
        .collect()
}

/// `player` discards the chosen cards still in their hand together, then
/// draws that many.
pub(crate) fn discard_chosen_then_draw(
    state: &mut GameState,
    player: PlayerId,
    cards: &[EffectObjectBinding],
) {
    let present = hand_candidates(state, player)
        .into_iter()
        .filter(|binding| cards.contains(binding))
        .map(|binding| ProposedEvent::zone_change(binding.object, Zone::Graveyard))
        .collect::<Vec<_>>();
    let count = present.len();
    if count == 0 {
        return;
    }
    event::propose_and_commit_batch(state, present);
    for _ in 0..count {
        event::propose_and_commit(state, ProposedEvent::draw(player));
    }
}

/// Whether `object` is a token copy that has haste from its copy effect.
#[cfg(feature = "standard-magezero-fixtures")]
pub(crate) fn has_copied_haste(state: &GameState, object: ObjectId) -> bool {
    let Some(standard) = state.standard_v1.as_ref() else {
        return false;
    };
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && standard
                .hasty_copies
                .contains(&(object, live.zone_change_count))
    })
}

/// Whether `contract` names a battlefield incarnation that Reflection of
/// Kiki-Jiki created, the only source of its delayed sacrifice trigger.
#[cfg(feature = "standard-magezero-fixtures")]
pub(crate) fn is_hasty_copy_incarnation(
    state: &GameState,
    contract: crate::state::AbilitySourceContractV4,
) -> bool {
    contract.zone == Zone::Battlefield
        && state.standard_v1.as_ref().is_some_and(|standard| {
            standard
                .hasty_copies
                .contains(&(contract.source, contract.zone_change_count))
        })
}

/// The "sacrifice it at the beginning of the next end step" delayed
/// triggers that the beginning of an end step sets off (603.7). Each fires
/// once; one whose permanent already left the battlefield is dropped.
#[cfg(feature = "standard-magezero-fixtures")]
pub(crate) fn end_step_delayed_triggers(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Vec<crate::trigger::PendingTrigger> {
    if !events
        .iter()
        .any(|event| matches!(event, CommittedEvent::BeginningEndStep { .. }))
    {
        return Vec::new();
    }
    let Some(standard) = state.standard_v1.as_mut() else {
        return Vec::new();
    };
    let due = std::mem::take(&mut standard.end_step_sacrifices);
    due.into_iter()
        .filter(|&(object, zone_change_count, _)| {
            let live = state.objects.get(object);
            live.zone == Zone::Battlefield && live.zone_change_count == zone_change_count
        })
        .map(|(object, _, controller)| {
            let mut source_contract = crate::state::AbilitySourceContractV4::capture(state, object);
            source_contract.controller = controller;
            crate::trigger::PendingTrigger {
                controller,
                source: object,
                effect: EffectOp::StandardV1(StandardOpV1::SacrificeSourceAtEndStep),
                is_madness_offer: false,
                kicked: false,
                target_spec: crate::card_def::TargetSpec::None,
                targets: Vec::new(),
                target_contracts: Vec::new(),
                placement_ordered: false,
                source_contract: Some(source_contract),
                granted_by: None,
                optional_additional_cost_paid: None,
                paid_cost_refs: Vec::new(),
            }
        })
        .collect()
}

// ---- The Irencrag // Everflame, Heroes' Legacy -----------------------------

const THE_IRENCRAG: &str = "The Irencrag";
const EVERFLAME: &str = "Everflame, Heroes' Legacy";

fn irencrag_may_become_everflame() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::MayBecomeEverflame)
}

const THE_IRENCRAG_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::StandardV1(
        StandardTriggerV1::ControlledLegendaryCreatureEntersUnlessEverflame,
    ),
    irencrag_may_become_everflame,
)];

/// Whether `object` is a battlefield incarnation that became Everflame.
pub(crate) fn is_everflame(state: &GameState, object: ObjectId) -> bool {
    let Some(standard) = state.standard_v1.as_ref() else {
        return false;
    };
    state.objects.try_get(object).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && standard
                .everflames
                .contains(&(object, live.zone_change_count))
    })
}

/// The ability source as the exact battlefield incarnation that may become
/// Everflame, if it still is that incarnation and has not already.
pub(crate) fn everflame_candidate(state: &GameState, ctx: &ExecCtx) -> Option<EffectObjectBinding> {
    if !source_incarnation_live(ctx, state) || is_everflame(state, ctx.source) {
        return None;
    }
    Some(EffectObjectBinding {
        object: ctx.source,
        expected_zone: Zone::Battlefield,
        expected_zone_change_count: state.objects.get(ctx.source).zone_change_count,
    })
}

/// The Irencrag becomes Everflame, Heroes' Legacy for as long as this
/// incarnation stays on the battlefield: a legendary Equipment artifact
/// with equip {3} and "Equipped creature gets +3/+3" that loses its other
/// abilities. Its name changes too (layer 3).
pub(crate) fn become_everflame(state: &mut GameState, source: EffectObjectBinding) {
    let live = state.objects.get(source.object);
    if live.zone != Zone::Battlefield || live.zone_change_count != source.expected_zone_change_count
    {
        return;
    }
    let incarnation = (source.object, source.expected_zone_change_count);
    let standard = state.standard_v1.get_or_insert_with(Default::default);
    if !standard.everflames.contains(&incarnation) {
        standard.everflames.push(incarnation);
    }
    state.objects.get_mut(source.object).name = EVERFLAME.to_string();
}

// ---- Repurposing Bay -------------------------------------------------------

pub fn repurposing_bay_search() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::SearchArtifactWithManaValueOneMoreThanSacrificed)
}

/// The search filter for Repurposing Bay's ability: an artifact card with
/// mana value one more than the artifact sacrificed to pay its cost, from
/// the payment's frozen last-known card (`None` without exactly one).
pub(crate) fn repurposing_bay_filter(ctx: &ExecCtx) -> Option<crate::effect::LibraryCardFilter> {
    let [sacrificed] = ctx.paid_cost_refs.as_slice() else {
        return None;
    };
    let mana_value = CARD_DEFS.get(sacrificed.card_def as usize)?.mana_value;
    Some(crate::effect::LibraryCardFilter::ArtifactWithManaValue(
        mana_value.checked_add(1)?,
    ))
}

// ---- Breach the Multiverse ------------------------------------------------

pub fn breach_the_multiverse() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::MillCards {
            player: PlayerRef::Controller,
            count: 10,
        },
        EffectOp::MillCards {
            player: PlayerRef::Opponent,
            count: 10,
        },
        EffectOp::StandardV1(StandardOpV1::PutCreatureOrPlaneswalkerFromEachGraveyard),
        EffectOp::StandardV1(StandardOpV1::ControlledCreaturesBecomePhyrexian),
    ])
}

/// Breach the Multiverse's choices in order: each player, starting with
/// `controller`, whose graveyard holds a creature or planeswalker card,
/// with those cards as exact incarnations in graveyard order.
pub(crate) fn breach_stages(
    state: &GameState,
    controller: PlayerId,
) -> Vec<(PlayerId, Vec<EffectObjectBinding>)> {
    [controller, controller.opponent()]
        .into_iter()
        .map(|player| {
            let cards = state.players[player.index()]
                .graveyard
                .iter()
                .copied()
                .filter(|&card| {
                    let def = &CARD_DEFS[state.objects.get(card).card_def as usize];
                    def.types.contains(&CardType::Creature)
                        || def.types.contains(&CardType::Planeswalker)
                })
                .map(|card| EffectObjectBinding {
                    object: card,
                    expected_zone: Zone::Graveyard,
                    expected_zone_change_count: state.objects.get(card).zone_change_count,
                })
                .collect::<Vec<_>>();
            (player, cards)
        })
        .filter(|(_, cards)| !cards.is_empty())
        .collect()
}

/// The chosen cards still in their graveyards enter the battlefield under
/// `controller`'s control simultaneously.
pub(crate) fn put_chosen_onto_battlefield(
    state: &mut GameState,
    controller: PlayerId,
    cards: &[EffectObjectBinding],
) {
    let present = cards
        .iter()
        .filter(|binding| {
            let live = state.objects.get(binding.object);
            live.zone == Zone::Graveyard
                && live.zone_change_count == binding.expected_zone_change_count
        })
        .map(|binding| {
            let mut proposed = ProposedEvent::zone_change(binding.object, Zone::Battlefield);
            if let ProposedEvent::ZoneChange(change) = &mut proposed {
                change.battlefield_controller = Some(controller);
            }
            proposed
        })
        .collect::<Vec<_>>();
    if !present.is_empty() {
        event::propose_and_commit_batch(state, present);
    }
}

fn become_phyrexian(state: &mut GameState, controller: PlayerId) {
    let creatures = state.players[controller.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| crate::engine::object_has_type(state, id, CardType::Creature))
        .map(|id| (id, state.objects.get(id).zone_change_count))
        .collect::<Vec<_>>();
    if creatures.is_empty() {
        return;
    }
    let standard = state.standard_v1.get_or_insert_with(Default::default);
    for creature in creatures {
        if !standard.phyrexians.contains(&creature) {
            standard.phyrexians.push(creature);
        }
    }
}

/// Whether `object` is a battlefield creature that Breach the Multiverse
/// made a Phyrexian.
#[cfg(feature = "standard-magezero-fixtures")]
pub(crate) fn is_phyrexian(state: &GameState, object: ObjectId) -> bool {
    let Some(standard) = state.standard_v1.as_ref() else {
        return false;
    };
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    live.zone == Zone::Battlefield
        && standard
            .phyrexians
            .contains(&(object, live.zone_change_count))
        && crate::engine::object_has_type(state, object, CardType::Creature)
}

pub fn liliana_piles() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::SeparatePilesThenSacrifice {
        player: PlayerRef::Target(0),
    })
}

/// Every permanent `player` controls, as exact incarnations, in battlefield
/// order.
pub(crate) fn pile_candidates(state: &GameState, player: PlayerId) -> Vec<EffectObjectBinding> {
    controlled_permanent_candidates(state, player, StandardPermanentFilterV1::Any)
}

/// `player` sacrifices every permanent of `pile` that is still the same
/// incarnation under their control, simultaneously.
pub(crate) fn sacrifice_pile(
    state: &mut GameState,
    player: PlayerId,
    pile: &[EffectObjectBinding],
) {
    let present = pile
        .iter()
        .filter(|binding| {
            let live = state.objects.get(binding.object);
            live.zone == Zone::Battlefield
                && live.zone_change_count == binding.expected_zone_change_count
                && live.controller == player
        })
        .map(|binding| binding.object)
        .collect::<Vec<_>>();
    if present.is_empty() {
        return;
    }
    for &object in &present {
        event::log_sacrifice(state, object);
    }
    event::propose_and_commit_batch(
        state,
        present
            .into_iter()
            .map(|object| ProposedEvent::zone_change(object, Zone::Graveyard))
            .collect(),
    );
}

// ---- Craft: Clay-Fired Bricks // Cosmium Kiln ------------------------------

const CLAY_FIRED_BRICKS: &str = "Clay-Fired Bricks";
const COSMIUM_GNOME_TOKEN: &str = "Cosmium Gnome Token";

/// "Craft with artifact {N}: Exile this artifact and another artifact you
/// control or an artifact card from your graveyard: Return this card
/// transformed under its owner's control. Craft only as a sorcery."
pub fn craft_return_transformed() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::ReturnExiledSourceTransformed)
}

/// "When Clay-Fired Bricks enters, search your library for a basic Plains
/// card, reveal it, put it into your hand, then shuffle. You gain 2 life."
fn clay_fired_bricks_entry() -> EffectOp {
    EffectOp::Sequence(vec![
        EffectOp::SearchLibraryToHand {
            player: PlayerRef::Controller,
            filter: crate::effect::LibraryCardFilter::BasicLandWithAnySubtype([Subtype::Plains; 3]),
        },
        EffectOp::GainLife {
            player: PlayerRef::Controller,
            amount: 2,
        },
    ])
}

/// "When Cosmium Kiln enters, create two 1/1 colorless Gnome artifact
/// creature tokens."
fn cosmium_kiln_entry() -> EffectOp {
    let gnome = || EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name(COSMIUM_GNOME_TOKEN)
            .expect("Gnome token in CARD_DEFS"),
        controller: PlayerRef::Controller,
    };
    EffectOp::Sequence(vec![gnome(), gnome()])
}

const CLAY_FIRED_BRICKS_TRIGGERS: [TriggeredAbilityDef; 2] = [
    trigger(TriggerCondition::Etb, clay_fired_bricks_entry),
    trigger(TriggerCondition::Etb, cosmium_kiln_entry),
];

// ---- Craft: Braided Net // Braided Quipu ----------------------------------

const BRAIDED_NET: &str = "Braided Net";
const NET_COUNTERS_ON_ENTRY: u8 = 3;

/// "{T}, Remove a net counter from Braided Net: Tap another target nonland
/// permanent. Its activated abilities can't be activated for as long as it
/// remains tapped."
pub fn braided_net_tap() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::TapTargetAndLockActivations)
}

/// "{3}{U}, {T}: Draw a card for each artifact you control, then put
/// Braided Quipu into its owner's library third from the top."
pub fn braided_quipu_draw() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::DrawPerArtifactThenSourceThirdFromTop)
}

/// Net counters on `object`: a Braided Net battlefield incarnation enters
/// with three (its front face's replacement) and keeps the rest.
pub(crate) fn net_counters(state: &GameState, object: ObjectId) -> u8 {
    let Some(live) = state.objects.try_get(object) else {
        return 0;
    };
    if live.zone != Zone::Battlefield
        || live.v4.face_index != 0
        || CARD_DEFS[live.card_def as usize].name != BRAIDED_NET
    {
        return 0;
    }
    let removed = state.standard_v1.as_ref().map_or(0, |standard| {
        standard
            .net_counters_removed
            .iter()
            .find(|(id, zcc, _)| *id == object && *zcc == live.zone_change_count)
            .map_or(0, |(_, _, removed)| *removed)
    });
    NET_COUNTERS_ON_ENTRY.saturating_sub(removed)
}

/// Pays "Remove a net counter from this".
pub(crate) fn remove_net_counter(state: &mut GameState, object: ObjectId) {
    if net_counters(state, object) == 0 {
        return;
    }
    let zone_change_count = state.objects.get(object).zone_change_count;
    let standard = state.standard_v1.get_or_insert_with(Default::default);
    if let Some(entry) = standard
        .net_counters_removed
        .iter_mut()
        .find(|(id, zcc, _)| *id == object && *zcc == zone_change_count)
    {
        entry.2 += 1;
    } else {
        standard
            .net_counters_removed
            .push((object, zone_change_count, 1));
    }
}

/// Whether Braided Net stops `object`'s activated abilities: it is the
/// incarnation Net tapped and it is still tapped.
pub(crate) fn activations_locked(state: &GameState, object: ObjectId) -> bool {
    let Some(standard) = state.standard_v1.as_ref() else {
        return false;
    };
    state.objects.try_get(object).is_some_and(|live| {
        live.tapped
            && standard
                .net_locks
                .contains(&(object, live.zone_change_count))
    })
}

/// Ends Braided Net's lock on every permanent that untapped or left; run
/// after anything untaps a permanent.
pub(crate) fn release_untapped_locks(state: &mut GameState) {
    let Some(standard) = state.standard_v1.as_ref() else {
        return;
    };
    if standard.net_locks.is_empty() {
        return;
    }
    let held = standard
        .net_locks
        .iter()
        .copied()
        .filter(|&(object, zone_change_count)| {
            state.objects.try_get(object).is_some_and(|live| {
                live.tapped
                    && live.zone == Zone::Battlefield
                    && live.zone_change_count == zone_change_count
            })
        })
        .collect();
    if let Some(standard) = state.standard_v1.as_mut() {
        standard.net_locks = held;
    }
}

// ---- Chandra, Hope's Beacon ----------------------------------------------

const CHANDRA: &str = "Chandra, Hope's Beacon";

fn chandra_copy() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::BindCopyCastSpell)
}

/// "Whenever you cast an instant or sorcery spell, copy it. You may choose
/// new targets for the copy. This ability triggers only once each turn."
const CHANDRA_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::StandardV1(StandardTriggerV1::YouCastInstantOrSorceryOncePerTurn),
    chandra_copy,
)];

/// +2: "Add two mana in any combination of colors."
pub fn chandra_two_mana() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::ChooseTwoManaInAnyCombination)
}

/// +1: "Exile the top five cards of your library. Until the end of your
/// next turn, you may cast an instant or sorcery spell from among those
/// exiled cards."
pub fn chandra_impulse() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::ExileTopFiveMayCastOneInstantOrSorcery)
}

/// -X: "Chandra, Hope's Beacon deals X damage to each of up to two
/// targets." Each X is its own loyalty ability (`build_standard_v1`).
macro_rules! chandra_minus {
    ($($name:ident = $x:literal),* $(,)?) => {
        $(
            pub fn $name() -> EffectOp {
                EffectOp::StandardV1(StandardOpV1::DamageEachTarget { amount: $x })
            }
        )*
    };
}

chandra_minus!(
    chandra_minus_1 = 1,
    chandra_minus_2 = 2,
    chandra_minus_3 = 3,
    chandra_minus_4 = 4,
    chandra_minus_5 = 5,
    chandra_minus_6 = 6,
    chandra_minus_7 = 7,
    chandra_minus_8 = 8,
    chandra_minus_9 = 9,
    chandra_minus_10 = 10,
    chandra_minus_11 = 11,
    chandra_minus_12 = 12,
    chandra_minus_13 = 13,
    chandra_minus_14 = 14,
    chandra_minus_15 = 15,
    chandra_minus_16 = 16,
    chandra_minus_17 = 17,
    chandra_minus_18 = 18,
    chandra_minus_19 = 19,
    chandra_minus_20 = 20,
);

/// The fifteen two-mana color combinations, in WUBRG order.
pub(crate) fn two_mana_combinations(player: PlayerRef) -> Vec<EffectOp> {
    let colors = [
        ManaColor::W,
        ManaColor::U,
        ManaColor::B,
        ManaColor::R,
        ManaColor::G,
    ];
    let mut options = Vec::with_capacity(15);
    for (index, &first) in colors.iter().enumerate() {
        for &second in &colors[index..] {
            options.push(EffectOp::AddMana {
                player,
                colors: vec![first, second],
            });
        }
    }
    options
}

fn exile_top_five_may_cast_one(state: &mut GameState, player: PlayerId) {
    let mut group = Vec::new();
    for _ in 0..5 {
        let Some(&top) = state.players[player.index()].library.first() else {
            break;
        };
        event::propose_and_commit(state, ProposedEvent::zone_change(top, Zone::Exile));
        let live = state.objects.get(top);
        let def = &CARD_DEFS[live.card_def as usize];
        if live.zone != Zone::Exile
            || !def.is_castable()
            || !(def.has_type(CardType::Instant) || def.has_type(CardType::Sorcery))
        {
            continue;
        }
        let zone_change_count = live.zone_change_count;
        state
            .engine
            .exile_play_permissions
            .push(crate::engine::PlayPermission {
                object: top,
                holder: player,
                zone_change_generation: zone_change_count,
                play_or_cast: crate::engine::PlayOrCast::Cast,
                expiry: crate::engine::PlayPermissionExpiry::UntilHoldersNextTurn {
                    holder_turn_started: false,
                },
            });
        group.push((top, zone_change_count));
    }
    if group.len() > 1 {
        state
            .standard_v1
            .get_or_insert_with(Default::default)
            .exile_cast_groups
            .push((group, false));
    }
}

/// Whether `object` was exiled with others of which one was already cast.
pub(crate) fn exile_cast_group_spent(state: &GameState, object: ObjectId) -> bool {
    let Some(standard) = state.standard_v1.as_ref() else {
        return false;
    };
    let zone_change_count = state.objects.get(object).zone_change_count;
    standard
        .exile_cast_groups
        .iter()
        .any(|(members, spent)| *spent && members.contains(&(object, zone_change_count)))
}

/// Marks the group a card cast from exile belonged to as spent.
pub(crate) fn note_spell_cast(state: &mut GameState, spell: ObjectId) {
    let Some(standard) = state.standard_v1.as_mut() else {
        return;
    };
    let Some(live) = state.objects.try_get(spell) else {
        return;
    };
    let Some(exiled) = live.zone_change_count.checked_sub(1) else {
        return;
    };
    for (members, spent) in &mut standard.exile_cast_groups {
        if members.contains(&(spell, exiled)) {
            *spent = true;
        }
    }
}

// ---- Assimilation Aegis ------------------------------------------------------

const ASSIMILATION_AEGIS: &str = "Assimilation Aegis";

#[cfg(feature = "standard-magezero-fixtures")]
fn aegis_card_def() -> Option<u16> {
    static AEGIS: std::sync::OnceLock<Option<u16>> = std::sync::OnceLock::new();
    *AEGIS.get_or_init(|| crate::card_def::card_id_by_name(ASSIMILATION_AEGIS))
}

/// "Whenever Assimilation Aegis becomes attached to a creature, for as long
/// as Assimilation Aegis remains attached to it, that creature becomes a
/// copy of a creature card exiled with Assimilation Aegis." Applied before
/// each state-based action pass: copies whose Aegis left that creature end,
/// and a newly attached Aegis with an exiled creature card starts one. The
/// copy takes the card's front face (707.2, 707.8) and keeps the
/// creature's counters, damage, attachments and status.
#[cfg(feature = "standard-magezero-fixtures")]
pub(crate) fn refresh_aegis_copies(state: &mut GameState) {
    let Some(aegis_def) = aegis_card_def() else {
        return;
    };
    let attached = |state: &GameState, aegis: (ObjectId, u32), host: (ObjectId, u32)| {
        state.objects.try_get(aegis.0).is_some_and(|live| {
            live.zone == Zone::Battlefield
                && live.zone_change_count == aegis.1
                && live.v4.attached_to
                    == Some(crate::state::ObjectLinkV4 {
                        object: host.0,
                        zone_change_count: host.1,
                    })
        })
    };
    while let Some(index) = state.standard_v1.as_ref().and_then(|standard| {
        standard
            .aegis_copies
            .iter()
            .position(|copy| !attached(state, copy.aegis, copy.host))
    }) {
        end_aegis_copy(state, index);
    }
    let aegises: Vec<(ObjectId, u32, crate::state::ObjectLinkV4)> = [PlayerId::P0, PlayerId::P1]
        .iter()
        .flat_map(|player| state.players[player.index()].battlefield.iter().copied())
        .filter_map(|id| {
            let live = state.objects.get(id);
            (live.card_def == aegis_def)
                .then_some(live.v4.attached_to)
                .flatten()
                .map(|host| (id, live.zone_change_count, host))
        })
        .collect();
    for (aegis, aegis_zcc, host) in aegises {
        let known = state.standard_v1.as_ref().is_some_and(|standard| {
            standard.aegis_copies.iter().any(|copy| {
                copy.aegis == (aegis, aegis_zcc)
                    && copy.host == (host.object, host.zone_change_count)
            })
        });
        let host_live = state.objects.try_get(host.object).is_some_and(|live| {
            live.zone == Zone::Battlefield && live.zone_change_count == host.zone_change_count
        });
        if known || !host_live {
            continue;
        }
        let link = crate::state::ObjectLinkV4 {
            object: aegis,
            zone_change_count: aegis_zcc,
        };
        let Some(copied) = state.objects.iter().find_map(|(_, exiled)| {
            (exiled.zone == Zone::Exile
                && exiled.v4.exiled_by == Some(link)
                && !exiled.v4.is_token
                && CARD_DEFS[exiled.card_def as usize].has_type(CardType::Creature))
            .then_some(exiled.card_def)
        }) else {
            continue;
        };
        let live = state.objects.get_mut(host.object);
        let record = AegisCopyV1 {
            aegis: (aegis, aegis_zcc),
            host: (host.object, host.zone_change_count),
            card_def: live.card_def,
            name: std::mem::take(&mut live.name),
            face_index: live.v4.face_index,
            color_mask: live.v4.effective_color_mask,
            subtype_ids: std::mem::take(&mut live.v4.effective_subtype_ids),
            ward_generic: live.v4.ward_generic,
        };
        let had = [record.card_def, copied].map(|def| (host.object, host.zone_change_count, def));
        let base = crate::state::ObjectStateV4::from_card_def(copied);
        live.card_def = copied;
        live.name = CARD_DEFS[copied as usize].object_name.into();
        live.v4.face_index = 0;
        live.v4.effective_color_mask = base.effective_color_mask;
        live.v4.effective_subtype_ids = base.effective_subtype_ids;
        live.v4.ward_generic = base.ward_generic;
        let standard = state.standard_v1.get_or_insert_with(Default::default);
        standard.aegis_copies.push(record);
        for entry in had {
            if !standard.copied_card_defs.contains(&entry) {
                standard.copied_card_defs.push(entry);
            }
        }
    }
}

/// Whether `object`'s battlefield incarnation `zone_change_count` was ever
/// `card_def` through a copy effect.
pub(crate) fn had_card_def(
    state: &GameState,
    object: ObjectId,
    zone_change_count: u32,
    card_def: u16,
) -> bool {
    state.standard_v1.as_ref().is_some_and(|standard| {
        standard
            .copied_card_defs
            .contains(&(object, zone_change_count, card_def))
    })
}

/// Ends the copy at `index`: a later copy of the same creature inherits the
/// values it saved; otherwise the creature gets them back.
#[cfg(feature = "standard-magezero-fixtures")]
fn end_aegis_copy(state: &mut GameState, index: usize) {
    let Some(standard) = state.standard_v1.as_mut() else {
        return;
    };
    let record = standard.aegis_copies.remove(index);
    if let Some(later) = standard.aegis_copies[index..]
        .iter_mut()
        .find(|copy| copy.host == record.host)
    {
        later.card_def = record.card_def;
        later.name = record.name;
        later.face_index = record.face_index;
        later.color_mask = record.color_mask;
        later.subtype_ids = record.subtype_ids;
        later.ward_generic = record.ward_generic;
        return;
    }
    if state
        .objects
        .try_get(record.host.0)
        .is_none_or(|live| live.zone_change_count != record.host.1)
    {
        return;
    }
    let live = state.objects.get_mut(record.host.0);
    live.card_def = record.card_def;
    live.name = record.name;
    live.v4.face_index = record.face_index;
    live.v4.effective_color_mask = record.color_mask;
    live.v4.effective_subtype_ids = record.subtype_ids;
    live.v4.ward_generic = record.ward_generic;
}

/// A copied creature leaving the battlefield stops being a copy first, so
/// it arrives in its new zone as its own card.
#[cfg(feature = "standard-magezero-fixtures")]
pub(crate) fn end_aegis_copies_before_departure(state: &mut GameState, object: ObjectId) {
    let zone_change_count = state.objects.get(object).zone_change_count;
    while let Some(index) = state.standard_v1.as_ref().and_then(|standard| {
        standard
            .aegis_copies
            .iter()
            .rposition(|copy| copy.host == (object, zone_change_count))
    }) {
        end_aegis_copy(state, index);
    }
}

// ---- Agatha's Soul Cauldron --------------------------------------------------

const AGATHAS_SOUL_CAULDRON: &str = "Agatha's Soul Cauldron";

/// Granted activated abilities take indices past the host's printed ones
/// and the Equipment-granted slot: `printed + 1 + card * STRIDE + ability`,
/// so the exiled card's own ability index survives onto the stack.
const CAULDRON_GRANT_STRIDE: usize = 8;

/// "{T}: Exile target card from a graveyard."
pub fn cauldron_exile() -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::ExileTargetCardWithSource)
}

fn cauldron_counter() -> EffectOp {
    EffectOp::PutPlusOnePlusOneCounter {
        object: crate::effect::ObjectRef::Target(0),
    }
}

/// "When a creature card is exiled this way, put a +1/+1 counter on target
/// creature you control."
const CAULDRON_TRIGGERS: [TriggeredAbilityDef; 1] = [trigger(
    TriggerCondition::StandardV1(StandardTriggerV1::CreatureCardExiledWithThis),
    cauldron_counter,
)];

fn exile_card_with_source(ctx: &ExecCtx, state: &mut GameState) {
    let Some(Target::Object(card)) = ctx.targets.first().copied() else {
        return;
    };
    if !ctx.target_incarnation_matches(0, state) || state.objects.get(card).zone != Zone::Graveyard
    {
        return;
    }
    event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Exile));
    let Some(source) = ctx.ability_source_contract else {
        return;
    };
    if state.objects.get(card).zone == Zone::Exile {
        state.objects.get_mut(card).v4.exiled_by = Some(crate::state::ObjectLinkV4 {
            object: source.source,
            zone_change_count: source.zone_change_count,
        });
    }
}

/// The creature cards exiled with the Agatha's Soul Cauldron `player`
/// controls, in object order.
fn cauldron_exiled_creature_cards(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    let cauldrons = controlled_cauldrons(state, player);
    if cauldrons.is_empty() {
        return Vec::new();
    }
    state
        .exile
        .iter()
        .copied()
        .filter(|&id| {
            let live = state.objects.get(id);
            !live.v4.is_token
                && live
                    .v4
                    .exiled_by
                    .is_some_and(|link| cauldrons.contains(&link))
                && CARD_DEFS[live.card_def as usize].has_type(CardType::Creature)
        })
        .collect()
}

/// The ability of `card_def` a granted index names, given the host's
/// printed ability count; only front-face battlefield abilities are
/// granted.
pub(crate) fn cauldron_ability_of(
    card_def: u16,
    printed: usize,
    index: u8,
) -> Option<crate::card_def::ActivatedAbilityDef> {
    let slot = usize::from(index).checked_sub(printed + 1)?;
    let ability = *CARD_DEFS
        .get(card_def as usize)?
        .activated_abilities
        .get(slot % CAULDRON_GRANT_STRIDE)?;
    (ability.activation_zone == Zone::Battlefield
        && ability.face.is_none_or(|face| face == 0)
        && !ability.is_loyalty_ability())
    .then_some(ability)
}

/// "Creatures you control with +1/+1 counters on them have all activated
/// abilities of all creature cards exiled with Agatha's Soul Cauldron":
/// each granted ability of `host` with its index and the exiled card.
pub(crate) fn cauldron_granted_abilities(
    state: &GameState,
    host: ObjectId,
) -> Vec<(u8, ObjectId, crate::card_def::ActivatedAbilityDef)> {
    let live = state.objects.get(host);
    if live.zone != Zone::Battlefield
        || live.counters.plus1_plus1 <= 0
        || !crate::engine::object_has_type(state, host, CardType::Creature)
    {
        return Vec::new();
    }
    let printed = CARD_DEFS[live.card_def as usize].activated_abilities.len();
    let mut granted = Vec::new();
    for (position, card) in cauldron_exiled_creature_cards(state, live.controller)
        .into_iter()
        .enumerate()
    {
        let card_def = state.objects.get(card).card_def;
        for local in 0..CARD_DEFS[card_def as usize].activated_abilities.len() {
            let Ok(index) = u8::try_from(printed + 1 + position * CAULDRON_GRANT_STRIDE + local)
            else {
                break;
            };
            if local >= CAULDRON_GRANT_STRIDE {
                break;
            }
            if let Some(ability) = cauldron_ability_of(card_def, printed, index) {
                granted.push((index, card, ability));
            }
        }
    }
    granted
}

/// "You may spend mana as though it were mana of any color to activate
/// abilities of creatures you control": with an Agatha's Soul Cauldron, a
/// creature's activation cost asks for generic mana in place of its colored
/// and hybrid symbols. Costs with Phyrexian symbols are left as printed.
pub(crate) fn spend_as_any_color(
    state: &GameState,
    player: PlayerId,
    source: ObjectId,
    cost: crate::mana::Cost,
) -> crate::mana::Cost {
    if cost.pips.is_empty()
        || cost
            .pips
            .iter()
            .any(|pip| matches!(pip, crate::mana::Pip::Phyrexian(_)))
        || !controls_cauldron(state, player)
    {
        return cost;
    }
    let live = state.objects.get(source);
    if live.zone != Zone::Battlefield
        || live.controller != player
        || !crate::engine::object_has_type(state, source, CardType::Creature)
    {
        return cost;
    }
    crate::mana::Cost {
        pips: &[],
        generic: cost
            .generic
            .saturating_add(u8::try_from(cost.pips.len()).unwrap_or(u8::MAX)),
        x_count: cost.x_count,
    }
}

fn cauldron_card_def() -> Option<u16> {
    static CAULDRON: std::sync::OnceLock<Option<u16>> = std::sync::OnceLock::new();
    *CAULDRON.get_or_init(|| crate::card_def::card_id_by_name(AGATHAS_SOUL_CAULDRON))
}

/// The Agatha's Soul Cauldron incarnations `player` controls with their
/// abilities.
fn controlled_cauldrons(state: &GameState, player: PlayerId) -> Vec<crate::state::ObjectLinkV4> {
    let Some(cauldron) = cauldron_card_def() else {
        return Vec::new();
    };
    state.players[player.index()]
        .battlefield
        .iter()
        .filter_map(|&id| {
            let live = state.objects.get(id);
            (live.card_def == cauldron
                && crate::continuous_characteristics_v1::printed_abilities_active(state, id))
            .then_some(crate::state::ObjectLinkV4 {
                object: id,
                zone_change_count: live.zone_change_count,
            })
        })
        .collect()
}

fn controls_cauldron(state: &GameState, player: PlayerId) -> bool {
    cauldron_card_def().is_some() && !controlled_cauldrons(state, player).is_empty()
}
