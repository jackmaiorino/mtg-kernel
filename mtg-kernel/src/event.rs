//! propose -> replace/prevent -> commit.
//!
//! `effect::execute` never mutates `GameState` directly: every leaf op
//! builds a `ProposedEvent` and calls `propose_and_commit`, which runs the
//! replacement/prevention pass (`apply_replacements`) and then, if
//! anything survived, `commit`. `commit` is the *only* function that
//! mutates `GameState` in response to a game event, and it appends the
//! resulting `CommittedEvent` to `state.engine.event_log` for
//! `trigger::collect_and_process` to drain after the resolution finishes.

pub use crate::authority_of_the_consuls_v1::{
    AuthorityEntryCaptureV1, AuthorityEntryOperationKindV1,
};
use crate::ids::{ObjectId, PlayerId, StackItemId};
use crate::mana::ManaColor;
use crate::state::{GameState, PaidCostRefV4, StackItemKind, Target, Zone};
use serde::{Deserialize, Serialize};

pub type ReplacementId = u32;

/// Deterministic placement within the destination library for a zone-change
/// proposal. Ordinary library moves retain `Top`; effects such as Deem
/// Inferior select one of the appended positional variants without bypassing
/// the replace/commit/event-log pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LibraryPlacement {
    Top,
    SecondFromTop,
    Bottom,
    /// Appended for Braided Quipu.
    ThirdFromTop,
}

/// Which observers learn the identity of a card inserted into a library at
/// a publicly determined position. Position shifts are always public and
/// exact; this flag controls only whether the inserted identity is learned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LibraryInsertVisibility {
    Hidden,
    Owner,
    Public,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReplacementEffectKind {
    /// Prevent the next `remaining` damage that would be dealt to `target`.
    /// Synthetic -- no pool card grants this yet -- but proves the
    /// replacement pipeline shape end-to-end; see
    /// `tests::prevention_shield_absorbs_then_expires`.
    PreventNextDamage { target: Target, remaining: i32 },
    /// Prevent all damage that sources sharing `color` would deal during
    /// the turn in which this replacement was installed.
    PreventDamageFromColorUntilEndOfTurn {
        color: ManaColor,
        turn: u32,
        active_player: PlayerId,
    },
    PreventCombatDamageToObjectUntilEndOfTurn {
        object: crate::state::ObjectLinkV4,
        turn: u32,
        active_player: PlayerId,
    },
    /// Appended resolved effect; source departure does not end its duration.
    ExileIfDiesUntilEndOfTurnV1 {
        target: crate::state::StackTargetContractV4,
        source: crate::state::AbilitySourceContractV4,
        turn: u32,
        active_player: PlayerId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActiveReplacement {
    pub id: ReplacementId,
    pub source: ObjectId,
    pub kind: ReplacementEffectKind,
}

// ---------------------------------------------------------------- proposed

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DamageProposed {
    pub source: ObjectId,
    pub target: Target,
    pub amount: i32,
    pub touched_by: Vec<ReplacementId>,
    pub is_combat: bool,
    /// Ephemeral authority for the new definition-owned historical damage
    /// path. Ordinary proposals leave this absent; committed Damage bytes
    /// and hashes remain unchanged.
    pub historical_source: Option<StackItemId>,
    /// Private, in-flight origin carried by a genuine resolving permanent
    /// ability. It is intentionally absent from proposal serialization; the
    /// damage receipt is emitted only by Niv's final damage commit.
    #[serde(skip)]
    pub(crate) niv_damage_origin_v1: Option<NivDamageOriginV1>,
}

impl std::hash::Hash for DamageProposed {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.source.hash(state);
        self.target.hash(state);
        self.amount.hash(state);
        self.touched_by.hash(state);
        self.is_combat.hash(state);
        self.historical_source.hash(state);
        if let Some(origin) = self.niv_damage_origin_v1 {
            0x6e69_765f_646d_676f_u64.hash(state);
            origin.hash(state);
        }
    }
}

/// Narrow private handoff minted only from the actual resolving stack row.
/// Resumable state is checked against that saved row before execution continues.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NivDamageOriginV1 {
    stack_item_id: StackItemId,
    source: crate::state::AbilitySourceContractV4,
}

fn same_niv_source_incarnation_v1(
    left: crate::state::AbilitySourceContractV4,
    right: crate::state::AbilitySourceContractV4,
) -> bool {
    left.source == right.source
        && left.card_def == right.card_def
        && left.owner == right.owner
        && left.zone == right.zone
        && left.zone_change_count == right.zone_change_count
}

fn niv_departed_source_contract_v1(
    state: &GameState,
    wanted: crate::state::AbilitySourceContractV4,
) -> Option<crate::state::AbilitySourceContractV4> {
    let mut found = None;
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::NivMizzetVisionaryDamageSourceDepartureV1 {
            source,
            departure_history_index,
        } = event
        else {
            continue;
        };
        if !same_niv_source_incarnation_v1(*source, wanted) {
            continue;
        }
        let departure = usize::try_from(*departure_history_index).ok()?;
        if at != departure.checked_add(1)?
            || !matches!(state.engine.event_history.get(departure),
                Some(CommittedEvent::ZoneChange { object, from, to, controller_before })
                    if *object == source.source
                        && *from == source.zone
                        && *from == Zone::Battlefield
                        && to != from
                        && *controller_before == source.controller)
        {
            return None;
        }
        if found.replace(*source).is_some() {
            return None;
        }
    }
    found
}

fn niv_source_departure_before_v1(
    state: &GameState,
    wanted: crate::state::AbilitySourceContractV4,
    before_history_index: usize,
) -> Result<Option<crate::state::AbilitySourceContractV4>, ()> {
    let mut found = None;
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::NivMizzetVisionaryDamageSourceDepartureV1 {
            source,
            departure_history_index,
        } = event
        else {
            continue;
        };
        if !same_niv_source_incarnation_v1(*source, wanted) {
            continue;
        }
        let departure = usize::try_from(*departure_history_index).map_err(|_| ())?;
        if at != departure.checked_add(1).ok_or(())?
            || !matches!(state.engine.event_history.get(departure),
                Some(CommittedEvent::ZoneChange { object, from, to, controller_before })
                    if *object == source.source
                        && *from == source.zone
                        && *from == Zone::Battlefield
                        && to != from
                        && *controller_before == source.controller)
        {
            return Err(());
        }
        if departure < before_history_index && found.replace(*source).is_some() {
            return Err(());
        }
    }
    Ok(found)
}

pub(crate) fn niv_mizzet_catalog_enabled_v1() -> bool {
    crate::card_def::CARD_DEFS
        .iter()
        .any(|definition| definition.is_executable() && definition.name == "Niv-Mizzet, Visionary")
}

/// Validate the handoff expected for a native saved resolving row. Callers
/// must already have authenticated this saved row through the owning resolver.
pub(crate) fn niv_damage_origin_for_saved_item_v1(
    state: &GameState,
    item: &crate::state::StackItem,
) -> Option<NivDamageOriginV1> {
    if !niv_mizzet_catalog_enabled_v1()
        || !item
            .inline_effect
            .as_ref()
            .is_some_and(crate::effect::crystal_prevention_v1::has_damage)
        || !matches!(
            item.kind,
            crate::state::StackItemKind::TriggeredAbility
                | crate::state::StackItemKind::ActivatedAbility
        )
    {
        return None;
    }
    let source = item.v4.ability_source_contract?;
    if source.source != item.source
        || crate::engine::validate_historical_ability_source_contract(state, source).is_err()
    {
        return None;
    }
    Some(NivDamageOriginV1 {
        stack_item_id: item.v4.stack_item_id,
        source,
    })
}

/// Mint only while the actual, unique stack-top row remains present.
pub(crate) fn niv_damage_origin_for_resolving_item_v1(
    state: &GameState,
    item: &crate::state::StackItem,
) -> Option<NivDamageOriginV1> {
    if state.stack.last() != Some(item)
        || state
            .stack
            .iter()
            .filter(|live| live.v4.stack_item_id == item.v4.stack_item_id)
            .count()
            != 1
    {
        return None;
    }
    niv_damage_origin_for_saved_item_v1(state, item)
}

pub(crate) fn niv_damage_origin_matches_context_v1(
    origin: NivDamageOriginV1,
    stack_item_id: Option<StackItemId>,
    source: ObjectId,
    source_contract: Option<crate::state::AbilitySourceContractV4>,
) -> bool {
    stack_item_id == Some(origin.stack_item_id)
        && source == origin.source.source
        && source_contract == Some(origin.source)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneChangeProposed {
    pub object: ObjectId,
    pub to_zone: Zone,
    /// Meaningful only when `to_zone == Library`; other destinations retain
    /// the constructor-default `Top` marker.
    pub library_placement: LibraryPlacement,
    /// Preserve an already-known library identity if this move puts the
    /// card into a hidden zone. Ordinary library-to-hand moves remain
    /// secret; public reveal effects opt in after populating the existing
    /// perspective-scoped library-knowledge table.
    pub preserve_known_identity: bool,
    /// This is a private top-of-library insertion whose owner knows the
    /// inserted identity. Other observers learn no new identity, while their
    /// still-valid prior library position facts shift through the insertion.
    pub library_insert_visibility: LibraryInsertVisibility,
    /// Forces a permanent moved to the battlefield by this effect to enter
    /// tapped, independently of its own static entry rules.
    pub force_battlefield_tapped: bool,
    /// Optional transformed battlefield face. Ordinary zone changes retain
    /// `None`, which resets the object to its front face.
    pub battlefield_face_index: Option<u8>,
    /// Optional controller for a battlefield return whose effect says
    /// "under your control" rather than the ordinary owner-controlled rule.
    pub battlefield_controller: Option<PlayerId>,
    /// Actual played lands retain their rule-action counter origin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counter_origin_v48: Option<crate::doubling_season_v48::CounterOriginV48>,
    /// Already quoted effect stun placement, installed as this permanent enters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battlefield_stun_v48: Option<i16>,
    /// Original X placement and optional actual host of an effect entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battlefield_x_v48: Option<crate::doubling_season_v48::EffectXEntryV48>,
    /// A resolving entry effect adds this subtype simultaneously with entry (611.2e).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battlefield_added_subtype_v1: Option<crate::card_def::Subtype>,
    pub touched_by: Vec<ReplacementId>,
    /// Revival counters installed simultaneously with a delayed return entry,
    /// before entry-trigger collection and replacements.
    /// Kept last so the existing serialized field prefix remains stable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battlefield_revival_v1: Option<i32>,
    /// Installs a resolving-entry flying grant on the new incarnation.
    #[serde(default, skip_serializing_if = "is_false")]
    pub battlefield_grant_flying_v1: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifeLossProposed {
    pub player: PlayerId,
    pub amount: i32,
    pub touched_by: Vec<ReplacementId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifeGainProposed {
    pub player: PlayerId,
    pub amount: i32,
    pub touched_by: Vec<ReplacementId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawProposed {
    pub player: PlayerId,
    pub touched_by: Vec<ReplacementId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TapProposed {
    pub object: ObjectId,
    pub touched_by: Vec<ReplacementId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManaAddProposed {
    pub player: PlayerId,
    pub colors: Vec<ManaColor>,
    pub touched_by: Vec<ReplacementId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateTokenProposed {
    pub token_def: u16,
    pub controller: PlayerId,
    /// Initialize a copied transformed face before entry replacements and observers.
    pub entry_face_index: Option<u8>,
    #[serde(default, skip_serializing_if = "token_entry_untapped_v48")]
    pub entry_tapped: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initialization_v48: Option<crate::doubling_season_v48::TokenEntryV48>,
    pub touched_by: Vec<ReplacementId>,
}
fn token_entry_untapped_v48(value: &bool) -> bool {
    !*value
}

/// Flips a permanent's face in place, with no zone change: the same
/// `ObjectId` and the same `zone_change_count` before and after (Delver of
/// Secrets transforming into Insectile Aberration). `face_index` must name
/// a face the object's `CardDef::transform_face` defines; `commit` panics
/// rather than guess at an undefined one, matching the zone-change path's
/// own `battlefield_face_index` handling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransformProposed {
    pub object: ObjectId,
    pub face_index: u8,
    pub touched_by: Vec<ReplacementId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposedEvent {
    Damage(DamageProposed),
    ZoneChange(ZoneChangeProposed),
    LifeLoss(LifeLossProposed),
    LifeGain(LifeGainProposed),
    Draw(DrawProposed),
    Tap(TapProposed),
    ManaAdd(ManaAddProposed),
    CreateToken(CreateTokenProposed),
    Transform(TransformProposed),
}

impl ProposedEvent {
    pub fn damage(source: ObjectId, target: Target, amount: i32) -> ProposedEvent {
        ProposedEvent::Damage(DamageProposed {
            source,
            target,
            amount,
            touched_by: Vec::new(),
            is_combat: false,
            historical_source: None,
            niv_damage_origin_v1: None,
        })
    }
    pub fn zone_change(object: ObjectId, to_zone: Zone) -> ProposedEvent {
        ProposedEvent::ZoneChange(ZoneChangeProposed {
            object,
            to_zone,
            library_placement: LibraryPlacement::Top,
            preserve_known_identity: false,
            library_insert_visibility: LibraryInsertVisibility::Hidden,
            force_battlefield_tapped: false,
            battlefield_face_index: None,
            battlefield_controller: None,
            counter_origin_v48: None,
            battlefield_stun_v48: None,
            battlefield_x_v48: None,
            battlefield_added_subtype_v1: None,
            battlefield_revival_v1: None,
            battlefield_grant_flying_v1: false,
            touched_by: Vec::new(),
        })
    }
    pub fn zone_change_to_battlefield_tapped(object: ObjectId) -> ProposedEvent {
        ProposedEvent::ZoneChange(ZoneChangeProposed {
            object,
            to_zone: Zone::Battlefield,
            library_placement: LibraryPlacement::Top,
            preserve_known_identity: false,
            library_insert_visibility: LibraryInsertVisibility::Hidden,
            force_battlefield_tapped: true,
            battlefield_face_index: None,
            battlefield_controller: None,
            counter_origin_v48: None,
            battlefield_stun_v48: None,
            battlefield_x_v48: None,
            battlefield_added_subtype_v1: None,
            battlefield_revival_v1: None,
            battlefield_grant_flying_v1: false,
            touched_by: Vec::new(),
        })
    }
    pub fn zone_change_preserving_known_identity(object: ObjectId, to_zone: Zone) -> ProposedEvent {
        ProposedEvent::ZoneChange(ZoneChangeProposed {
            object,
            to_zone,
            library_placement: LibraryPlacement::Top,
            preserve_known_identity: true,
            library_insert_visibility: LibraryInsertVisibility::Hidden,
            force_battlefield_tapped: false,
            battlefield_face_index: None,
            battlefield_controller: None,
            counter_origin_v48: None,
            battlefield_stun_v48: None,
            battlefield_x_v48: None,
            battlefield_added_subtype_v1: None,
            battlefield_revival_v1: None,
            battlefield_grant_flying_v1: false,
            touched_by: Vec::new(),
        })
    }
    /// Moves one privately selected card to the top of its owner's library.
    /// The move remains a normal replaceable/logged zone change; this flag
    /// only carries the visibility needed for exact library knowledge.
    pub fn private_top_library_insert(object: ObjectId) -> ProposedEvent {
        ProposedEvent::ZoneChange(ZoneChangeProposed {
            object,
            to_zone: Zone::Library,
            library_placement: LibraryPlacement::Top,
            preserve_known_identity: false,
            library_insert_visibility: LibraryInsertVisibility::Owner,
            force_battlefield_tapped: false,
            battlefield_face_index: None,
            battlefield_controller: None,
            counter_origin_v48: None,
            battlefield_stun_v48: None,
            battlefield_x_v48: None,
            battlefield_added_subtype_v1: None,
            battlefield_revival_v1: None,
            battlefield_grant_flying_v1: false,
            touched_by: Vec::new(),
        })
    }
    /// A privately selected bottom card is known only to its owner.
    pub fn private_bottom_library_insert(object: ObjectId) -> ProposedEvent {
        let mut event = Self::private_top_library_insert(object);
        if let ProposedEvent::ZoneChange(proposal) = &mut event {
            proposal.library_placement = LibraryPlacement::Bottom;
        }
        event
    }

    /// Moves one publicly identified object to an exact position in its
    /// owner's library. Both observers retain that public identity at its
    /// new incarnation while every pre-existing known position shifts
    /// exactly around the insertion.
    pub fn public_library_insert(object: ObjectId, placement: LibraryPlacement) -> ProposedEvent {
        ProposedEvent::ZoneChange(ZoneChangeProposed {
            object,
            to_zone: Zone::Library,
            library_placement: placement,
            preserve_known_identity: false,
            library_insert_visibility: LibraryInsertVisibility::Public,
            force_battlefield_tapped: false,
            battlefield_face_index: None,
            battlefield_controller: None,
            counter_origin_v48: None,
            battlefield_stun_v48: None,
            battlefield_x_v48: None,
            battlefield_added_subtype_v1: None,
            battlefield_revival_v1: None,
            battlefield_grant_flying_v1: false,
            touched_by: Vec::new(),
        })
    }
    pub fn zone_change_to_battlefield_under_controller(
        object: ObjectId,
        controller: PlayerId,
    ) -> ProposedEvent {
        let ProposedEvent::ZoneChange(mut change) = Self::zone_change(object, Zone::Battlefield)
        else {
            unreachable!("zone-change constructor");
        };
        change.battlefield_controller = Some(controller);
        ProposedEvent::ZoneChange(change)
    }

    pub fn transformed_battlefield_return(
        object: ObjectId,
        face_index: u8,
        controller: PlayerId,
    ) -> ProposedEvent {
        ProposedEvent::ZoneChange(ZoneChangeProposed {
            object,
            to_zone: Zone::Battlefield,
            library_placement: LibraryPlacement::Top,
            preserve_known_identity: false,
            library_insert_visibility: LibraryInsertVisibility::Public,
            force_battlefield_tapped: false,
            battlefield_face_index: Some(face_index),
            battlefield_controller: Some(controller),
            counter_origin_v48: None,
            battlefield_stun_v48: None,
            battlefield_x_v48: None,
            battlefield_added_subtype_v1: None,
            battlefield_revival_v1: None,
            battlefield_grant_flying_v1: false,
            touched_by: Vec::new(),
        })
    }
    pub fn life_loss(player: PlayerId, amount: i32) -> ProposedEvent {
        ProposedEvent::LifeLoss(LifeLossProposed {
            player,
            amount,
            touched_by: Vec::new(),
        })
    }
    /// Paying life loses life (CR 119.4). Modifiers change the loss, not the
    /// printed amount checked for affordability (CR 118.11).
    pub fn life_payment(player: PlayerId, amount: i32) -> ProposedEvent {
        Self::life_loss(player, amount)
    }
    pub fn life_gain(player: PlayerId, amount: i32) -> ProposedEvent {
        ProposedEvent::LifeGain(LifeGainProposed {
            player,
            amount,
            touched_by: Vec::new(),
        })
    }
    pub fn draw(player: PlayerId) -> ProposedEvent {
        ProposedEvent::Draw(DrawProposed {
            player,
            touched_by: Vec::new(),
        })
    }
    pub fn tap(object: ObjectId) -> ProposedEvent {
        ProposedEvent::Tap(TapProposed {
            object,
            touched_by: Vec::new(),
        })
    }
    pub fn mana_add(player: PlayerId, colors: Vec<ManaColor>) -> ProposedEvent {
        ProposedEvent::ManaAdd(ManaAddProposed {
            player,
            colors,
            touched_by: Vec::new(),
        })
    }
    pub fn create_token(token_def: u16, controller: PlayerId) -> ProposedEvent {
        ProposedEvent::CreateToken(CreateTokenProposed {
            token_def,
            controller,
            entry_face_index: None,
            entry_tapped: false,
            initialization_v48: None,
            touched_by: Vec::new(),
        })
    }
    pub(crate) fn create_transformed_copy(token_def: u16, controller: PlayerId) -> ProposedEvent {
        ProposedEvent::CreateToken(CreateTokenProposed {
            token_def,
            controller,
            entry_face_index: Some(1),
            entry_tapped: false,
            initialization_v48: None,
            touched_by: Vec::new(),
        })
    }
    /// Flips `object`'s face in place with no zone change (Delver of
    /// Secrets transforming into Insectile Aberration). See
    /// `TransformProposed`'s doc.
    pub fn transform_in_place(object: ObjectId, face_index: u8) -> ProposedEvent {
        ProposedEvent::Transform(TransformProposed {
            object,
            face_index,
            touched_by: Vec::new(),
        })
    }

    fn touched_by(&self) -> &[ReplacementId] {
        match self {
            ProposedEvent::Damage(e) => &e.touched_by,
            ProposedEvent::ZoneChange(e) => &e.touched_by,
            ProposedEvent::LifeLoss(e) => &e.touched_by,
            ProposedEvent::LifeGain(e) => &e.touched_by,
            ProposedEvent::Draw(e) => &e.touched_by,
            ProposedEvent::Tap(e) => &e.touched_by,
            ProposedEvent::ManaAdd(e) => &e.touched_by,
            ProposedEvent::CreateToken(e) => &e.touched_by,
            ProposedEvent::Transform(e) => &e.touched_by,
        }
    }

    fn mark_touched(&mut self, id: ReplacementId) {
        let v = match self {
            ProposedEvent::Damage(e) => &mut e.touched_by,
            ProposedEvent::ZoneChange(e) => &mut e.touched_by,
            ProposedEvent::LifeLoss(e) => &mut e.touched_by,
            ProposedEvent::LifeGain(e) => &mut e.touched_by,
            ProposedEvent::Draw(e) => &mut e.touched_by,
            ProposedEvent::Tap(e) => &mut e.touched_by,
            ProposedEvent::ManaAdd(e) => &mut e.touched_by,
            ProposedEvent::CreateToken(e) => &mut e.touched_by,
            ProposedEvent::Transform(e) => &mut e.touched_by,
        };
        v.push(id);
    }
}

/// Preserve the privately minted resolving-ability origin across every
/// ordinary effect damage leaf. Unauthenticated direct proposals keep the
/// historical `None` shape.
pub(crate) fn damage_from_exec_ctx_v1(
    ctx: &crate::effect::ExecCtx,
    target: Target,
    amount: i32,
) -> ProposedEvent {
    let mut proposal = ProposedEvent::damage(ctx.source, target, amount);
    if let ProposedEvent::Damage(damage) = &mut proposal {
        attach_niv_damage_origin_v1(ctx, damage);
    }
    proposal
}

pub(crate) fn attach_niv_damage_origin_v1(
    ctx: &crate::effect::ExecCtx,
    damage: &mut DamageProposed,
) {
    if niv_mizzet_catalog_enabled_v1() {
        damage.niv_damage_origin_v1 = ctx.niv_damage_origin_v1.filter(|origin| {
            niv_damage_origin_matches_context_v1(
                *origin,
                ctx.stack_item_id,
                damage.source,
                ctx.ability_source_contract,
            )
        });
    }
}

// --------------------------------------------------------------- committed

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CommittedEvent {
    Damage {
        source: ObjectId,
        target: Target,
        amount: i32,
    },
    ZoneChange {
        object: ObjectId,
        from: Zone,
        to: Zone,
        /// Last-known controller immediately before the move. Zone-change
        /// bookkeeping resets control to the owner, but leave/dies triggers
        /// remain controlled by the player who controlled the permanent.
        controller_before: PlayerId,
    },
    LifeLoss {
        player: PlayerId,
        amount: i32,
    },
    LifeGain {
        player: PlayerId,
        amount: i32,
    },
    /// `object` is `None` when the draw was attempted against an empty
    /// library; SBA picks that up as a loss condition (704.5c).
    Draw {
        player: PlayerId,
        object: Option<ObjectId>,
    },
    Tap {
        object: ObjectId,
    },
    ManaAdded {
        player: PlayerId,
        colors: Vec<ManaColor>,
    },
    CreateToken {
        object: ObjectId,
        token_def: u16,
        controller: PlayerId,
        #[serde(
            default,
            skip_serializing_if = "crate::doubling_season_v48::TokenInputRefV48::is_none"
        )]
        replacement_input_v48: crate::doubling_season_v48::TokenInputRefV48,
    },
    /// Logged by `engine::finalize_cast` the moment a spell is placed on
    /// the stack (not routed through `propose_and_commit`: casting a spell
    /// is an engine action with no replaceable "cast" event in this
    /// increment's scope, same rationale as the Hand->Stack zone move
    /// itself -- see `commit_zone_change`'s doc). Exists purely so
    /// `trigger::TriggerCondition::CastInstantOrSorcery` (Guttersnipe) has
    /// something to match against; it is still appended to both
    /// `event_log` (drained by `trigger::collect_and_process`) and
    /// `event_history` for consistency with every other committed event.
    SpellCast {
        spell: ObjectId,
        controller: PlayerId,
    },
    /// A completed cast, activation, or copy-target choice made this exact
    /// permanent incarnation a target. This is nonreplaceable bookkeeping
    /// consumed by reusable Ward trigger matching.
    Targeted {
        target: ObjectId,
        target_zone_change_count: u32,
        targeting_stack_item: StackItemId,
        targeting_controller: PlayerId,
    },
    /// A permanent was sacrificed by its current controller. The effective
    /// subtype set is captured as last-known information before the ensuing
    /// zone change resets the object incarnation.
    Sacrificed {
        object: ObjectId,
        controller_before: PlayerId,
        effective_subtype_ids_before: Vec<u16>,
    },
    /// Nonreplaceable marker emitted after a combat-damage batch only when
    /// positive damage to a player survived prevention. The exact source
    /// incarnation prevents a later zone-change generation from inheriting
    /// the combat trigger.
    CombatDamageToPlayer {
        source: ObjectId,
        source_zone_change_count: u32,
        player: PlayerId,
        amount: i32,
    },
    /// A Saga received this exact lore counter and its corresponding chapter
    /// ability triggered. Captures the source incarnation at counter time so
    /// later zone changes cannot relabel the chapter ability.
    SagaChapter {
        source: ObjectId,
        source_zone_change_count: u32,
        controller: PlayerId,
        chapter: u8,
    },
    /// 505.2: the beginning of `player`'s own Upkeep step (Delver of
    /// Secrets). Exactly one Upkeep step happens each turn, belonging to
    /// the active player, so this fires once per turn.
    UpkeepBegan {
        player: PlayerId,
    },
    /// `object`'s face flipped to `face_index` in place, with no zone
    /// change (Delver of Secrets transforming into Insectile Aberration).
    /// Distinct from the Saga path's exile-then-return
    /// (`ZoneChange`/`ProposedEvent::transformed_battlefield_return`),
    /// which creates a new incarnation because that card's own rules text
    /// says so.
    Transformed {
        object: ObjectId,
        face_index: u8,
    },
    /// Transient cast-provenance marker consumed from `event_log` by the
    /// trigger collection immediately following this resolution. It is not
    /// appended to permanent `event_history`; the spell stack item and its
    /// paid-cost references remain the durable provenance.
    OptionalAdditionalCostPaid {
        source: ObjectId,
        kind: crate::card_def::OptionalAdditionalCostDef,
        paid_cost_refs: Vec<PaidCostRefV4>,
    },
    /// Nonreplaceable marker for one global Initiative or Undercity trigger.
    /// `history_index` is self-authenticating against the permanent event
    /// history before the resulting ability may be placed on the stack.
    InitiativeTrigger {
        binding: crate::state::InitiativeTriggerBindingV1,
    },
    /// Nonreplaceable marker for one engine-owned monarch end-step draw
    /// trigger (306.3). `history_index` is self-authenticating against the
    /// permanent event history before the resulting ability may be placed
    /// on the stack, the same discipline `InitiativeTrigger` uses.
    MonarchTrigger {
        binding: crate::state::MonarchTriggerBindingV1,
    },
    /// Exact declaration-time source for an attack trigger.
    DeclaredAttacker {
        source: ObjectId,
        source_zone_change_count: u32,
        controller: PlayerId,
    },
    /// A positive placement of +1/+1 counters on one exact incarnation.
    PlusOneCountersAdded {
        object: ObjectId,
        zone_change_count: u32,
        player: PlayerId,
        count: i32,
        #[serde(
            default,
            skip_serializing_if = "crate::doubling_season_v48::CounterInputRefV48::is_none"
        )]
        replacement_input_v48: crate::doubling_season_v48::CounterInputRefV48,
    },
    /// Morbid is captured when this step begins, before any later death.
    BeginningEndStep {
        active_player: PlayerId,
        creature_died_this_turn: bool,
    },
    /// Precedes a departure whose printed abilities were absent, before any
    /// exact departure-color snapshot for the same incarnation.
    PrintedAbilitiesRemovedBeforeZoneChange {
        object: ObjectId,
        zone_change_count: u32,
    },
    /// Exact declaration-time source for a "whenever you attack" trigger:
    /// `controller` declared one or more attackers while `source` was on the
    /// battlefield. Emitted only for permanents with such a trigger.
    ControllerAttacked {
        source: ObjectId,
        source_zone_change_count: u32,
        controller: PlayerId,
    },
    // Append new variants to preserve existing derived Hash discriminants.
    /// 700.13: `player` committed a crime by casting a spell, activating an
    /// ability or putting a triggered ability on the stack that targets an
    /// opponent, anything an opponent controls, or a card in an opponent's
    /// graveyard. Logged once per stack item, after its final targeting
    /// markers. Only `standard-magezero-fixtures` builds emit it.
    CrimeCommitted {
        player: PlayerId,
        targeting_stack_item: StackItemId,
    },
    /// Last-known power of a permanent about to leave the battlefield,
    /// logged just before its zone change for the Standard cards whose
    /// leave triggers read it (Heartfire Hero). Only
    /// `standard-magezero-fixtures` builds emit it.
    PowerBeforeLeavingBattlefield {
        object: ObjectId,
        zone_change_count: u32,
        power: i32,
    },
    /// 507.1: the beginning of `active_player`'s beginning of combat step
    /// (Manifold Mouse). Only `standard-magezero-fixtures` builds emit it.
    BeginningOfCombat {
        active_player: PlayerId,
    },
    /// A permanent that was a creature just before it left the battlefield,
    /// logged for the Enduring cards' "if it was a creature". Only
    /// `standard-magezero-fixtures` builds emit it.
    WasCreatureBeforeLeavingBattlefield {
        object: ObjectId,
        zone_change_count: u32,
    },
    /// Appended to preserve every historical event's Hash discriminant.
    /// Non-trigger history anchor, only emitted by opted-in game states.
    LifeGainTurnBeganV1 {
        turn: u32,
        active_player: PlayerId,
    },
    /// Precedes a transforming permanent's departure from the battlefield
    /// while it showed a face other than its front, so leave triggers see
    /// the face that left. Only the Standard catalog's transforming cards
    /// record it.
    LeftBattlefieldFaceV1 {
        object: ObjectId,
        zone_change_count: u32,
        face_index: u8,
    },
    /// A door of a Room permanent became unlocked (709.5), either as the
    /// Room entered after its half was cast or by the unlock special action.
    /// Only the Standard catalog's Rooms record it.
    RoomDoorUnlockedV1 {
        object: ObjectId,
        zone_change_count: u32,
        door: u8,
    },
    /// A Class permanent gained a level (716.2). Only the Standard catalog's
    /// Classes record it.
    ClassLevelGainedV1 {
        object: ObjectId,
        zone_change_count: u32,
        level: u8,
    },
    TurnedFaceUp {
        object: ObjectId,
        zone_change_count: u32,
    },
    BeginningPrecombatMainV1 {
        active_player: PlayerId,
    },
    LeftBattlefieldCopyV1 {
        source: crate::state::AbilitySourceContractV4,
        face_index: u8,
    },
    /// Appended engine-only capture. Emitted only for an executable printed
    /// death observer, immediately after the corresponding committed death.
    ControlledCreatureDeathCaptureV1 {
        source: crate::state::AbilitySourceContractV4,
        ability_index: u16,
        died: crate::state::ObjectLinkV4,
        controller_before: PlayerId,
    },
    /// Single-use collection receipt for a committed observer capture. The
    /// index names immutable event_history authority, not a mutable log entry.
    /// This engine-only event never enters event_log or creates a trigger.
    ControlledCreatureDeathCaptureCollectedV1 {
        capture_history_index: u64,
    },
    /// Immutable controller LKI for an exact Run Away Together target. This
    /// engine-only receipt never enters event_log or creates a trigger.
    DifferentControllerTargetDepartureV1 {
        target: crate::state::StackTargetContractV4,
        controller_before: PlayerId,
    },
    /// Independent actual-entry authority for the unregistered shared-graveyard ETB.
    SingleGraveyardEntryV1 {
        source_contract: crate::state::AbilitySourceContractV4,
        entry_event_index: usize,
    },
    /// History-only authority for a definition-owned source-power activation.
    /// Appended to preserve old event discriminants and absent-state bytes.
    SourcePowerActivationCommittedV1 {
        item: Box<crate::state::StackItem>,
        power: i32,
        toughness: i32,
        damage_source: HistoricalDamageSourceV1,
    },
    SourceDamageBeforeLeavingV1 {
        damage_source: HistoricalDamageSourceV1,
    },
    SourcePowerActivationFinishedV1 {
        stack_item_id: StackItemId,
    },
    /// Private committed lifecycle ownership for the new paid look-four root.
    /// Never placed in the public trigger event log.
    SquadRallierResolutionBoundaryV1 {
        receipt: crate::effect::SquadRallierResolutionReceiptV1,
    },
    /// Private committed Sower installation and exact Cleanup expiry.
    SowerRestrictionInstalledV1 {
        capture: Box<crate::sower_restriction_capture_v1::CaptureV1>,
    },
    SowerRestrictionExpiredV1 {
        capture: Box<crate::sower_restriction_capture_v1::CaptureV1>,
    },
    /// Exact validated item, captured before the resolver pops it.
    SowerRestrictionResolutionV1 {
        item: Box<crate::state::StackItem>,
        timestamp: u64,
        recipient_snapshot: String,
    },
    AuthorityEntryCaptureV1 {
        capture: AuthorityEntryCaptureV1,
    },
    AuthorityEntryCollectedV1 {
        capture_history_index: u64,
    },
    /// History-only original observer facts, independent of trigger captures.
    AuthorityEntryObserversV1 {
        entry_history_index: u64,
        entrant: crate::state::ObjectLinkV4,
        entrant_controller: PlayerId,
        visibility: crate::authority_public_capture_v1::EntryVisibilityV1,
        sources: Vec<crate::state::AbilitySourceContractV4>,
    },
    AuthorityEntryCompletedV1 {
        capture_history_index: u64,
        stack_item_id: crate::ids::StackItemId,
        operation_history_index: u64,
    },
    /// Independently committed actual gain/counter operation, not a dispatch
    /// completion marker. Gain also binds the real ordinary LifeGain event.
    AuthorityEntryOperationV1 {
        capture_history_index: u64,
        stack_item_id: StackItemId,
        source: crate::state::AbilitySourceContractV4,
        kind: crate::authority_of_the_consuls_v1::AuthorityEntryOperationKindV1,
    },
    /// History-only exact finalized trigger publication.
    AuthorityEntryPublishedV1 {
        capture_history_index: u64,
        item: Box<crate::state::StackItem>,
    },
    WildwoodCounterCaptureV1 {
        capture: Box<crate::wildwood_capture_v1::CaptureV1>,
    },
    WildwoodTriggerPublishedV1 {
        item: Box<crate::state::StackItem>,
    },
    WildwoodTriggerResolvingV1 {
        stack_item_id: StackItemId,
    },
    WildwoodTriggerOperationV1 {
        stack_item_id: StackItemId,
        counter_event_index: Option<usize>,
    },
    WildwoodTriggerClosedV1 {
        stack_item_id: StackItemId,
        countered: bool,
    },
    WildwoodTriggerCounteredV1 {
        item: Box<crate::state::StackItem>,
    },
    /// History-only authority for the appended temporary-control opcode.
    TemporaryControlResolutionV1 {
        item: Box<crate::state::StackItem>,
        recipient_snapshot: String,
        timestamp: u64,
        turn: u32,
        active_player: PlayerId,
    },
    TemporaryControlInstalledV1 {
        installation: Box<crate::effect::temporary_control_v1::authority::InstallationV1>,
    },
    TemporaryControlCleanupV1 {
        turn: u32,
        active_player: PlayerId,
        installations: Vec<usize>,
    },
    TemporaryControlExpiredV1 {
        installation_history_index: usize,
        cleanup_history_index: usize,
    },
    FrenziedGoblinPublishedV1 {
        publication: Box<crate::effect::FrenziedGoblinPublicationRecordV1>,
    },
    FrenziedGoblinResolutionBoundaryV1 {
        receipt: crate::effect::FrenziedGoblinResolutionReceiptV1,
    },
    FrenziedGoblinPaymentV1 {
        publication_history_index: u32,
    },
    FrenziedGoblinRemovalV1 {
        publication_history_index: u32,
        item: Box<crate::state::StackItem>,
        outcome: crate::effect::FrenziedGoblinOutcomeV1,
    },
    FrenziedGoblinCompletedV1 {
        publication_history_index: u32,
        outcome: crate::effect::FrenziedGoblinOutcomeV1,
    },
    FrenziedGoblinInstalledV1 {
        capture: Box<crate::effect::FrenziedGoblinRestrictionV1>,
    },
    FrenziedGoblinExpiredV1 {
        capture: Box<crate::effect::FrenziedGoblinRestrictionV1>,
        cleanup_history_index: u32,
    },
    FrenziedGoblinCleanupV1 {
        history_index: u32,
        turn: u32,
        ending_player: PlayerId,
        step: crate::state::Step,
        captures: Vec<crate::effect::FrenziedGoblinRestrictionV1>,
    },
    /// History-only event-time public entrant facts. Never copied to a DTO.
    AuthorityEntryPublicFactsV1 {
        capture: crate::authority_public_capture_v1::EntryPublicFactsV1,
    },
    CerberusObserversV1 {
        begin_history_index: u64,
        sources: Vec<crate::state::AbilitySourceContractV4>,
    },
    CerberusCaptureV1 {
        capture: crate::cerberus_capture_v1::CerberusCaptureV1,
    },
    CerberusCollectedV1 {
        capture_history_index: u64,
    },
    CerberusPublishedV1 {
        capture_history_index: u64,
        item: Box<crate::state::StackItem>,
    },
    CerberusOperationV1 {
        capture_history_index: u64,
        stack_item_id: StackItemId,
        source: crate::state::AbilitySourceContractV4,
        kind: crate::cerberus_capture_v1::CerberusOperationKindV1,
    },
    CerberusCompletedV1 {
        capture_history_index: u64,
        stack_item_id: StackItemId,
        operation_history_index: u64,
    },
    LibrarianActivationCommittedV1 {
        item: Box<crate::state::StackItem>,
    },
    LibrarianActivationFinishedV1 {
        stack_item_id: crate::ids::StackItemId,
        operation_history_index: u64,
    },
    LibrarianActivationOperationV1 {
        item: Box<crate::state::StackItem>,
        operation: crate::librarian_activation_v1::OperationV1,
    },
    LibrarianActivationCounterRemovalV1 {
        item: Box<crate::state::StackItem>,
    },
    PrivateKeepOneLibraryCapturedV1 {
        binding: Box<crate::effect::PrivateKeepOneBindingV1>,
    },
    PrivateKeepOneLibraryConsumedV1 {
        resolving_stack_item: StackItemId,
        graveyard: Vec<crate::effect::EffectObjectBinding>,
        move_history_indices: Vec<usize>,
    },
    RaidEntrySourceV1 {
        source: crate::state::AbilitySourceContractV4,
        entry_history_index: usize,
    },
    /// History-only actual initial Raid-false completion, never a public event.
    PrivateKeepOneRaidSkippedV1 {
        resolving_stack_item: StackItemId,
        initial_boundary_history_index: u32,
    },
    ExtraTurnStartedV1 {
        initial: crate::extra_turn_v1::ExtraTurnStateV1,
        round: u32,
        active_player: PlayerId,
    },
    ExtraTurnScheduledV1 {
        item: Box<crate::state::StackItem>,
    },
    ExtraTurnBeganV1 {
        previous: crate::extra_turn_v1::ExtraTurnStateV1,
        after: crate::extra_turn_v1::ExtraTurnStateV1,
        round: u32,
        active_player: PlayerId,
    },
    ExtraTurnOwnerPublishedV1 {
        item: Box<crate::state::StackItem>,
    },
    ExtraTurnOwnerRemovedV1 {
        item: Box<crate::state::StackItem>,
        outcome: crate::extra_turn_v1::RemovalV1,
    },
    ExtraTurnVirtualCeasedV1 {
        source: crate::state::StackSourceContractV4,
        stack_item_id: StackItemId,
    },
    /// Private scoped power snapshot before a genuine battlefield departure.
    AshrootPowerBeforeDepartureV1 {
        source: crate::state::ObjectLinkV4,
        owner: PlayerId,
        power: i32,
    },
    AshrootDispatchV2 {
        journal: crate::ashroot_dispatch_v2::JournalV2,
    },
    GorehornDepartureV1 {
        departure: Box<crate::gorehorn_damage_v1::DepartureV1>,
    },
    GorehornPublishedV1 {
        publication: Box<crate::gorehorn_damage_v1::PublicationV1>,
    },
    GorehornPreparedV1 {
        prepared: Box<crate::gorehorn_damage_v1::PreparedV1>,
    },
    GorehornDamageOperationV1 {
        operation: Box<crate::gorehorn_damage_v1::DamageOperationV1>,
    },
    GorehornOperationV1 {
        operation: Box<crate::gorehorn_damage_v1::OperationV1>,
    },
    RaidEntryCaptureV1 {
        capture: Box<crate::raid_entry_custody_v1::CaptureV1>,
    },
    RaidEntryPublishedV1 {
        publication: Box<crate::raid_entry_custody_v1::PublishedV1>,
    },
    RaidEntryDroppedV1 {
        dropped: Box<crate::raid_entry_custody_v1::DroppedV1>,
    },
    RaidEntryCounteredV1 {
        countered: Box<crate::raid_entry_custody_v1::CounteredV1>,
    },
    RaidEntryStartedV1 {
        started: Box<crate::raid_entry_custody_v1::StartedV1>,
    },
    /// History-only definition-owned entry and linked-duration authority.
    BanishingEntryV1 {
        source: crate::state::AbilitySourceContractV4,
        entry_history_index: u64,
    },
    BanishingCollectedV1 {
        source: crate::state::AbilitySourceContractV4,
    },
    BanishingNoTargetsV1 {
        source: crate::state::AbilitySourceContractV4,
    },
    BanishingPublishedV1 {
        item: Box<crate::state::StackItem>,
    },
    BanishingRemovedV1 {
        item: Box<crate::state::StackItem>,
        outcome: crate::banishing_light_v1::RemovalV1,
    },
    BanishingFinishedV1 {
        stack_item_id: StackItemId,
    },
    BanishingLinkInstalledV1 {
        record: crate::state::LinkedExileRecordV4,
        exile_history_index: u64,
        recipient_was_token: bool,
    },
    BanishingLinkExpiredV1 {
        record: crate::state::LinkedExileRecordV4,
        departure_history_index: u64,
        return_history_index: Option<u64>,
        no_return_reason: Option<crate::banishing_light_v1::NoReturnV1>,
    },
    BanishingRecipientCeasedV1 {
        record: crate::state::LinkedExileRecordV4,
    },
    BanishingAuraHostsCapturedV1 {
        choice: Box<crate::linked_aura_return_v1::HostChoiceV1>,
    },
    BanishingAuraHostChosenV1 {
        capture_history_index: u64,
        host: crate::effect::EffectObjectBinding,
        return_history_index: u64,
    },
    BanishingAuraOperationStartedV1 {
        operation: Box<crate::linked_aura_operation_v1::OperationV1>,
        original_sha256: String,
    },
    /// History-only actual turn anchor for a selected Limited life-loss consumer.
    LifeLossTurnBeganV1 {
        turn: u32,
        active_player: PlayerId,
    },
    StromkirkEndStepObserversV1 {
        beginning_index: u64,
        turn: u32,
        observers: Vec<crate::state::AbilitySourceContractV4>,
    },

    StromkirkTriggerCaptureV2 {
        observers_history_index: u64,
        observer_index: u32,
    },
    StromkirkCollectedV2 {
        capture_history_index: u64,
    },
    StromkirkPublishedV2 {
        capture_history_index: u64,
        item: Box<crate::state::StackItem>,
    },
    StromkirkPlacementRemovedV2 {
        capture_history_index: u64,
        pending: Box<crate::trigger::PendingTrigger>,
    },
    StromkirkResolutionRemovedV2 {
        capture_history_index: u64,
        item: Box<crate::state::StackItem>,
        input: Box<crate::stromkirk_dispatch_v1::ResolutionInputV2>,
    },
    StromkirkCounterRemovedV2 {
        capture_history_index: u64,
        item: Box<crate::state::StackItem>,
    },
    StromkirkOperationV2 {
        capture_history_index: u64,
        removal_history_index: u64,
        outcome: crate::stromkirk_dispatch_v1::OutcomeV2,
    },
    StromkirkCompletedV2 {
        capture_history_index: u64,
        operation_history_index: u64,
    },
    VampireGourmandResolutionBoundaryV1 {
        receipt: crate::effect::VampireGourmandResolutionReceiptV1,
    },
    VampireGourmandCostOperationV2 {
        operation: Box<crate::effect::VampireGourmandCostOperationV2>,
    },
    VampireGourmandCleanupV2 {
        turn: u32,
        ending_player: PlayerId,
        step: crate::state::Step,
        operation_history_indices: Vec<u32>,
    },
    VampireGourmandExpiredV2 {
        operation_history_index: u32,
        cleanup_history_index: u32,
    },
    VampireGourmandAttackCaptureV2 {
        declaration_history_index: u32,
        turn: u32,
        active_player: PlayerId,
        source: crate::state::AbilitySourceContractV4,
    },
    VampireGourmandCollectedV2 {
        capture_history_index: u32,
    },
    VampireGourmandPublishedV2 {
        capture_history_index: u32,
        item: Box<crate::state::StackItem>,
    },
    VampireGourmandCounterRemovedV2 {
        publication_history_index: u32,
        item: Box<crate::state::StackItem>,
    },
    VampireGourmandCounterClosedV2 {
        publication_history_index: u32,
        removal_history_index: u32,
    },
    GarrukEntryObservationV1 {
        entry: Box<crate::garruks_uprising_v1::EntryV1>,
        effective_creature: bool,
        effective_power: Option<i32>,
        sources: Vec<(
            crate::state::AbilitySourceContractV4,
            crate::garruks_uprising_v1::RootV1,
        )>,
    },
    GarrukDrawCaptureV1 {
        capture: crate::garruks_uprising_v1::CaptureV1,
    },
    GarrukDrawCollectedV1 {
        capture_history_index: u64,
    },
    GarrukDrawPublishedV1 {
        capture_history_index: u64,
        item: Box<crate::state::StackItem>,
    },
    GarrukDrawRemovedV1 {
        capture_history_index: u64,
        item: Box<crate::state::StackItem>,
        kind: crate::garruks_uprising_v1::RemovalKindV1,
    },
    GarrukDrawCompletedV1 {
        capture_history_index: u64,
        removal_history_index: u64,
        draw_history_index: Option<u64>,
    },
    CrystalPreventionBoundaryV1 {
        boundary: Box<crate::effect::crystal_prevention_v1::BoundaryV1>,
    },
    CrystalShieldOperationV1 {
        operation: crate::effect::crystal_prevention_v1::ShieldOperationV1,
    },
    CrystalExecutionV1 {
        execution: Box<crate::effect::crystal_prevention_v1::ExecutionV1>,
    },
    CrystalDamageOperationV3 {
        operation: Box<crate::effect::crystal_prevention_v1::DamageOperationV3>,
    },
    CrystalBatchPreparedV3 {
        capture: Box<crate::effect::crystal_prevention_v1::BatchCaptureV2>,
    },
    CrystalProgramChoiceV3 {
        operation: Box<crate::effect::crystal_prevention_v1::ProgramChoiceOperationV3>,
    },
    FixedDamageDepartureV3 {
        departure: Box<crate::fixed_source_damage_v3::DepartureV3>,
    },
    FixedDamagePaidV3 {
        payment: Box<crate::fixed_source_damage_v3::PaidV3>,
    },
    FixedDamageRemovedV3 {
        removal: Box<crate::fixed_source_damage_v3::RemovedV3>,
    },
    GorehornCrystalBeginV1 {
        begin: Box<crate::gorehorn_damage_v1::crystal::BeginV1>,
    },
    GorehornCrystalCompletionV1 {
        completion: Box<crate::gorehorn_damage_v1::crystal::CompletionV1>,
    },
    ArahboEntryObservationV1 {
        entry: Box<crate::arahbo_v1::EntryV1>,
        effective_cat: bool,
        was_token: bool,
        sources: Vec<(
            crate::state::AbilitySourceContractV4,
            crate::arahbo_v1::RootV1,
        )>,
    },
    ArahboTokenCaptureV1 {
        capture: crate::arahbo_v1::CaptureV1,
    },
    ArahboTokenCollectedV1 {
        capture_history_index: u64,
    },
    ArahboTokenPublishedV1 {
        capture_history_index: u64,
        item: Box<crate::state::StackItem>,
    },
    ArahboTokenRemovedV1 {
        capture_history_index: u64,
        item: Box<crate::state::StackItem>,
        kind: crate::arahbo_v1::RemovalKindV1,
    },
    ArahboTokenCompletedV1 {
        capture_history_index: u64,
        removal_history_index: u64,
        token_history_index: Option<u64>,
    },
    ArahboCounterInputV1 {
        capture_history_index: u64,
        item: Box<crate::state::StackItem>,
    },
    ExilePlaySelectionCapturedV1 {
        resolving_stack_item: crate::ids::StackItemId,
        binding: Box<crate::effect::ExileTopChoiceBindingV1>,
    },
    ExilePlaySelectionConsumedV1 {
        resolving_stack_item: crate::ids::StackItemId,
        selected: Option<crate::effect::EffectObjectBinding>,
    },
    StrongboxRaidSkippedV1 {
        resolving_stack_item: crate::ids::StackItemId,
        initial_boundary_history_index: u32,
    },
    StrongboxPermissionGrantedV1 {
        installation: Box<crate::effect::exile_top_choose_v1::PermissionInstallationV1>,
    },
    StrongboxPermissionTurnStartedV1 {
        installation: u32,
        turn: u32,
        holder: PlayerId,
    },
    StrongboxPermissionCleanupV1 {
        turn: u32,
        holder: PlayerId,
        installations: Vec<u32>,
    },
    StrongboxPermissionExpiredV1 {
        installation: u32,
        cleanup: u32,
    },
    /// Exact resolved Fiery death-shield installation.
    ExileIfDiesReplacementInstalledV1 {
        replacement: ActiveReplacement,
    },
    ExileIfDiesReplacementExpiredV1 {
        replacement: ActiveReplacement,
        turn: u32,
        active_player: PlayerId,
        cleanup_index: u32,
    },
    ExileIfDiesReplacementCleanupV1 {
        index: u32,
        installations: Vec<ActiveReplacement>,
        turn: u32,
        active_player: PlayerId,
    },
    EquipmentDamagePublishedV4 {
        publication: Box<crate::equipment_damage_source_v4::PublicationV4>,
    },
    EquipmentDamageDepartureV4 {
        departure: Box<crate::equipment_damage_source_v4::DepartureV4>,
    },
    EquipmentDamagePreparedV4 {
        prepared: Box<crate::equipment_damage_source_v4::PreparationV4>,
    },
    EquipmentDamageClosedV4 {
        closed: Box<crate::equipment_damage_source_v4::ClosedV4>,
    },
    EquipmentDamageCounterRemovalV4 {
        removal: Box<crate::equipment_damage_source_v4::CounterRemovalV4>,
    },
    KrenkoPaidV1 {
        paid: Box<crate::krenko_v1::PaidV1>,
    },
    KrenkoRemovedV1 {
        removed: Box<crate::krenko_v1::RemovedV1>,
    },
    KrenkoSampleV1 {
        sample: Box<crate::krenko_v1::SampleV1>,
    },
    KrenkoCompletedV1 {
        completed: Box<crate::krenko_v1::CompletedV1>,
    },
    KrenkoRemovalInputV1 {
        input: Box<crate::krenko_v1::RemovalInputV1>,
    },
    KrenkoPaymentInputV1 {
        input: Box<crate::krenko_v1::PaymentInputV1>,
    },
    ZulAshurPublishedV1 {
        publication: Box<crate::zul_ashur_v1::PublicationV1>,
    },
    ZulAshurRemovedV1 {
        removal: Box<crate::zul_ashur_v1::RemovalV1>,
    },
    ZulAshurInstalledV1 {
        installation: Box<crate::zul_ashur_v1::InstallationV1>,
    },
    ZulAshurCleanupEnteredV1 {
        index: u32,
        turn: u32,
        active_player: PlayerId,
    },
    ZulAshurCleanupV1 {
        cleanup: Box<crate::zul_ashur_v1::CleanupV1>,
    },
    ZulAshurExpiredV1 {
        expiry: crate::zul_ashur_v1::ExpiryV1,
    },
    ZulAshurRemovalInputV1 {
        input: Box<crate::zul_ashur_v1::RemovalInputV1>,
    },
    BannerEntryOperationStartedV1 {
        operation: Box<crate::linked_aura_operation_v1::OperationV1>,
        original_sha256: String,
    },
    BannerEntryColorCapturedV1 {
        operation: u32,
        choice: Box<crate::heraldic_banner_v1::ChoiceV1>,
    },
    BannerEntryColorChosenV1 {
        capture: u32,
        color: crate::mana::ManaColor,
    },
    BannerEntryColorCommittedV1 {
        entered: Box<crate::heraldic_banner_v1::EnteredV1>,
    },
    BannerEntryOperationCompletedV1 {
        operation: u32,
        entries: Vec<u32>,
    },
    /// Independent engine-published lifecycle boundary for the exact
    /// mill-three/mandatory instant-or-sorcery return definition program.
    /// History-only: never projected as a trigger event or public card fact.
    InspirationResolutionBoundaryV1 {
        receipt: crate::effect::InspirationResolutionReceiptV1,
    },
    InspirationResolutionInputV1 {
        input: Box<crate::effect::InspirationInputV1>,
    },
    InspirationResolutionOperationV1 {
        operation: Box<crate::effect::InspirationOperationV1>,
    },
    InspirationEngineDepartureV1 {
        departure: Box<crate::effect::InspirationDepartureV1>,
    },
    ProtectionSourceColorDepartureV1 {
        snapshot: crate::controlled_creature_grants_v1::DepartureColorV1,
        departure_history_index: u64,
    },
    FishingPoleJournalV39 {
        journal: Box<crate::fishing_pole_v39::lifecycle::JournalV39>,
    },
    TwinflameReplacementInputV38 {
        input: Box<crate::twinflame_tyrant_v1::ordering::AnswerInputV38>,
    },
    TwinflameReplacementOperationV38 {
        operation: Box<crate::twinflame_tyrant_v1::ordering::AnswerOperationV38>,
    },
    TwinflameDamagePreparationV38 {
        preparation: Box<crate::twinflame_tyrant_v1::ordering::PreparationV38>,
    },
    TwinflameCombatPublishedV38 {
        publication: Box<crate::twinflame_tyrant_v1::combat::PublicationV38>,
    },
    TwinflameCombatInputV38 {
        input: Box<crate::twinflame_tyrant_v1::combat::InputV38>,
    },
    TwinflameCombatOperationV38 {
        operation: Box<crate::twinflame_tyrant_v1::combat::OperationV38>,
    },
    TwinflameDynamicDamageSampleV38 {
        sample: Box<crate::twinflame_tyrant_v1::sampling::SampleV38>,
    },
    TwinflameFieryCaptureV38 {
        capture: Box<crate::twinflame_tyrant_v1::fiery::CaptureV38>,
    },
    TwinflameFieryFinishedV38 {
        operation: Box<crate::twinflame_tyrant_v1::fiery::FinishV38>,
    },
    AjaniJournalV42 {
        journal: Box<crate::ajani_full_v42::lifecycle::JournalV42>,
    },
    FaebloomJournalV43 {
        journal: Box<crate::faebloom_trick_v43::JournalV43>,
    },
    /// Original attack-time identity; actual declaration is an independent occurrence.
    HighSocietyHunterAttackCaptureV1 {
        declaration_history_index: u64,
        source: crate::state::AbilitySourceContractV4,
        turn_key: u32,
        active_player: PlayerId,
    },
    HighSocietyHunterAttackCollectedV1 {
        capture_history_index: u64,
    },
    /// Private original finalized payment. Not a replayable trigger event.
    MandatoryCastPaymentV1 {
        source: crate::state::StackSourceContractV4,
        choice: crate::engine::mandatory_cast_cost_v1::PaidV1,
        item: Box<crate::state::StackItem>,
    },
    MandatoryCastPaymentInputV1 {
        pending: Box<crate::engine::PendingCast>,
        item: Box<crate::state::StackItem>,
    },
    HighSocietyHunterJournalV45 {
        journal: Box<crate::high_society_hunter_custody_v45::JournalV45>,
    },
    FiendishPandaLifeGainCaptureV1 {
        source: crate::state::AbilitySourceContractV4,
        gain_history_index: u64,
        amount: i32,
    },
    FiendishPandaDeathCaptureV1 {
        source: crate::state::AbilitySourceContractV4,
        departure_history_index: u64,
        power: i32,
    },
    FiendishPandaCaptureCollectedV1 {
        capture_history_index: u64,
    },
    FiendishPandaJournalV1 {
        journal: Box<crate::fiendish_panda_v1::custody::JournalV1>,
    },
    TokenCreationReplacementV48 {
        creation: Box<crate::doubling_season_v48::TokenCreationV48>,
    },
    PlusCounterReplacementV48 {
        placement: Box<crate::doubling_season_v48::PlusCounterPlacementV48>,
    },
    TokenCreationInputV48 {
        input: Box<crate::doubling_season_v48::TokenInputV48>,
    },
    TokenAttachmentJournalV48 {
        journal: Box<crate::season_token_attachment_v48::JournalV48>,
    },
    BaitTransferInputV48 {
        input: Box<crate::doubling_season_v48::BaitTransferInputV48>,
    },
    BaitTransferAppliedV48 {
        placement: Box<crate::doubling_season_v48::BaitTransferAppliedV48>,
    },
    PlusCounterInputV48 {
        input: Box<crate::doubling_season_v48::PlusCounterInputV48>,
    },
    /// Private original simultaneous player-loss operation, never a public trigger event.
    ConquerorLossBatchV49 {
        batch: Box<crate::bloodthirsty_conqueror_v49::BatchV49>,
    },
    ConquerorCapturedV49 {
        batch_history_index: u64,
        observer_index: u64,
    },
    ConquerorCollectedV49 {
        capture_history_index: u64,
    },
    ConquerorJournalV49 {
        journal: Box<crate::bloodthirsty_conqueror_v49::custody::JournalV49>,
    },
    ConquerorObserverInputV49 {
        input: Box<crate::bloodthirsty_conqueror_v49::ObserverInputV49>,
    },
    FakeJournalV50 {
        journal: Box<crate::fake_your_own_death_v50::spell_custody::JournalV50>,
    },
    /// Appended without changing predecessor Hash discriminants. Soul
    /// placement retains the original source and recipient replacement inputs.
    SoulCounterAddedV1 {
        input_history_index: u64,
        source: crate::state::AbilitySourceContractV4,
        player: PlayerId,
        replacement: crate::doubling_season_v48::ReplacementInputV48,
        before: i32,
        count: i32,
        after: i32,
    },
    /// Private source/actor/recipient world immediately before soul placement.
    SoulCounterInputV1 {
        input: Box<crate::soul_counters_v1::PlacementInputV1>,
    },
    /// Private finalized payment with its genuine pre-self-sacrifice count.
    RavenousPaymentV1 {
        paid: Box<crate::ravenous_amulet_v1::PaidV1>,
    },
    RavenousRemovedV1 {
        removed: Box<crate::ravenous_amulet_v1::RemovedV1>,
    },
    RavenousCompletedV1 {
        completed: Box<crate::ravenous_amulet_v1::CompletedV1>,
    },
    /// Genuine Drake original input, capture, payment and operation custody.
    DrakeJournalV1 {
        record: Box<crate::drake_hatcher_v1::custody::RecordV1>,
    },
    /// Only inside a Drake operation snapshot: exact immutable outer record index.
    /// Never admitted as an actual event or substitute for an absent original.
    DrakeHistoryRefV1 {
        index: u64,
    },
    /// Engine-produced pre-departure power for one exact Alesha battlefield
    /// incarnation. Structural validation binds this receipt to the matching
    /// immediately preceding ZoneChange and effect payload. Because native
    /// GameState/event history is mutable, coordinated edits to both receipt
    /// and payload are outside this check; owning consumers must preserve the
    /// state/snapshot trust boundary before admitting this route.
    AleshaPowerLkiV1 {
        source: ObjectId,
        card_def: u16,
        zone_change_count: u32,
        departure_history_index: u64,
        power: i32,
    },
    /// Definition-owned capture for Niv-Mizzet, Visionary's noncombat-damage
    /// trigger. The indexed Damage event supplies the final amount/recipient;
    /// both exact source incarnations were captured before that event changed
    /// the world. Appended to preserve prior event discriminants.
    NivMizzetVisionaryDamageV1 {
        observer: crate::state::AbilitySourceContractV4,
        damage_source: crate::state::AbilitySourceContractV4,
        damage_history_index: u64,
    },
    /// Exact trigger stack item created from one Niv damage receipt. The draw
    /// effect names only the damage-history index; this publication binds that
    /// index and observer to the native stack item that may consume it.
    NivMizzetVisionaryTriggerStackedV1 {
        stack_item_id: crate::ids::StackItemId,
        observer: crate::state::AbilitySourceContractV4,
        damage_history_index: u64,
    },
    /// Exact source incarnation and controller immediately before it left
    /// the battlefield. Emitted only when the Niv printing is present in the
    /// selected catalog, including departures before Niv enters.
    NivMizzetVisionaryDamageSourceDepartureV1 {
        source: crate::state::AbilitySourceContractV4,
        departure_history_index: u64,
    },
    KinshipEntryV1 {
        journal: Box<crate::banner_of_kinship_v1::entry::JournalV1>,
    },
    /// Private event-time observer roster for one successful native draw.
    SphinxsTutelageDrawV1 {
        draw_history_index: u64,
        observers: Vec<crate::state::AbilitySourceContractV4>,
    },
    SphinxsTutelageCollectedV1 {
        capture_history_index: u64,
    },
    SphinxsTutelagePublishedV1 {
        capture_history_index: u64,
        item: Box<crate::state::StackItem>,
    },
    /// Private original library, captured only after the trigger resolves.
    SphinxsTutelageMillStartedV1 {
        item: Box<crate::state::StackItem>,
        library: Vec<crate::effect::EffectObjectBinding>,
    },
    SphinxsTutelageMillBatchV1 {
        start_history_index: u64,
        previous_batch: Option<u64>,
        ordered: Vec<crate::effect::EffectObjectBinding>,
        move_start: u64,
    },
    /// Only actual positive final combat damage to a player creates this receipt.
    LathrilCombatDamageV1 {
        damage_history_index: u64,
        source: crate::state::AbilitySourceContractV4,
        player: PlayerId,
        amount: i32,
    },
    LathrilCombatCollectedV1 {
        capture_history_index: u64,
    },
    LathrilCombatPublishedV1 {
        capture_history_index: u64,
        item: Box<crate::state::StackItem>,
    },
    LathrilCombatResolvedV1 {
        capture_history_index: u64,
    },
    LathrilCombatCounteredV1 {
        capture_history_index: u64,
    },
    /// Bound only by the genuine private combat-finish callback, in packet order.
    LathrilCombatObservedV1 {
        capture_history_index: u64,
        marker_history_index: u64,
    },
    CopiedTokenAbilitiesEnteredV1 {
        source: crate::state::AbilitySourceContractV4,
        creation_history_index: u64,
        instances: u32,
    },
    CopiedTokenEndStepV1 {
        end_step_history_index: u64,
        sources: Vec<(crate::state::AbilitySourceContractV4, u32)>,
    },
    CopiedTokenEndStepCollectedV1 {
        capture_history_index: u64,
    },
    CopiedTokenSacrificePublishedV1 {
        capture_history_index: u64,
        observer: u32,
        ability: u32,
        item: Box<crate::state::StackItem>,
    },
    CopiedTokenSacrificeFinishedV1 {
        stack_item_id: crate::ids::StackItemId,
    },
    CuratorEnteredV1 {
        entry_history_index: u64,
        source: crate::state::AbilitySourceContractV4,
    },
    CuratorCollectedV1 {
        entry_history_index: u64,
    },
    CuratorPublishedV1 {
        entry_history_index: u64,
        item: Box<crate::state::StackItem>,
    },
    CuratorFinishedV1 {
        entry_history_index: u64,
    },
    CuratorProgressV1 {
        entry_history_index: u64,
        choice: Box<crate::effect::curator_v1::ChoiceV1>,
    },
    KykarExiledV1 {
        source: crate::state::AbilitySourceContractV4,
        exiled: crate::state::ObjectLinkV4,
        owner: PlayerId,
        departure_history_index: u64,
    },
    KykarDelayedCollectedV1 {
        schedule_history_index: u64,
        end_step_history_index: u64,
    },
    GreatwurmCounterPaymentV1 {
        paid: Box<crate::engine::greatwurm_cast_v1::PaidV1>,
        input_history_index: u64,
    },
    GreatwurmCombatCaptureV1 {
        capture: Box<crate::effect::greatwurm_v1::CaptureV1>,
    },
    GreatwurmCombatBoundV1 {
        capture_history_index: u64,
        original: Box<crate::effect::greatwurm_v1::CaptureV1>,
    },
    GreatwurmCollectedV1 {
        capture_history_index: u64,
        observer: u32,
    },
    GreatwurmPublishedV1 {
        capture_history_index: u64,
        observer: u32,
        item: Box<crate::state::StackItem>,
    },
    GreatwurmFinishedV1 {
        stack_item_id: crate::ids::StackItemId,
    },
    GreatwurmCounterInputV1 {
        input: Box<crate::engine::greatwurm_cast_v1::PaidV1>,
    },
    SourceCopyDepartureV1 {
        departure: Box<crate::effect::source_token_copy_v1::DepartureV1>,
    },
    SphinxLoreV1 {
        journal: crate::effect::sphinx_lore_v1::JournalV1,
    },
    SphinxLoreCastOptionsV1 {
        options: crate::engine::sphinx_lore_cast_v1::OptionsV1,
    },
    SphinxLoreCastSelectedV1 {
        choice: crate::engine::sphinx_lore_cast_v1::ChoiceV1,
        source: crate::state::StackSourceContractV4,
        stack_item_id: crate::ids::StackItemId,
    },
    OmniscienceCastOptionsV1 {
        options: crate::engine::omniscience_cast_v1::OptionsV1,
    },
    OmniscienceCastSelectedV1 {
        choice: crate::engine::omniscience_cast_v1::ChoiceV1,
        source: crate::state::StackSourceContractV4,
        stack_item_id: crate::ids::StackItemId,
    },
}

pub use crate::fixed_source_damage_v3::OutcomeV3 as FixedDamageOutcomeV3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HistoricalDamageSourceV1 {
    pub source: crate::state::AbilitySourceContractV4,
    pub color_mask: u8,
    pub lifelink: bool,
    pub deathtouch: bool,
    pub werewolf: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NivMizzetVisionaryDamageCaptureV1 {
    observer: crate::state::AbilitySourceContractV4,
    damage_source: crate::state::AbilitySourceContractV4,
}

/// Capture only from the final replaceable damage proposal, before committing
/// it. This intentionally lives in the already-hashed event source: the
/// future card registration must not rely on an untracked child module.
fn capture_niv_mizzet_visionary_damage_v1(
    state: &GameState,
    event: &ProposedEvent,
) -> Vec<NivMizzetVisionaryDamageCaptureV1> {
    let ProposedEvent::Damage(damage) = event else {
        return Vec::new();
    };
    let Target::Player(recipient) = damage.target else {
        return Vec::new();
    };
    if damage.amount <= 0 || damage.is_combat {
        return Vec::new();
    }
    let bound_source = if let Some(stack_item) = damage.historical_source {
        match crate::source_power_activation_v1::damage_source(state, damage.source, stack_item) {
            Ok(facts) => facts.source,
            Err(_) => return Vec::new(),
        }
    } else if let Some(origin) = damage.niv_damage_origin_v1 {
        if origin.source.source != damage.source {
            return Vec::new();
        }
        origin.source
    } else {
        let Some(live) = state.objects.try_get(damage.source) else {
            return Vec::new();
        };
        if !matches!(live.zone, Zone::Battlefield | Zone::Stack) {
            return Vec::new();
        }
        crate::source_power_activation_v1::capture_damage_source(state, damage.source).source
    };
    let Some(source_object) = state.objects.try_get(bound_source.source) else {
        return Vec::new();
    };
    let source = if source_object.card_def == bound_source.card_def
        && source_object.owner == bound_source.owner
        && source_object.zone_change_count == bound_source.zone_change_count
        && matches!(source_object.zone, Zone::Battlefield | Zone::Stack)
    {
        // A source still in its exact incarnation uses its controller at the
        // damage event, even when control changed after its ability triggered.
        crate::state::AbilitySourceContractV4::capture(state, bound_source.source)
    } else if let Some(departed) = niv_departed_source_contract_v1(state, bound_source) {
        // When the source has left, only the exact generation's adjacent
        // departure receipt can supply its last controller. Never infer from
        // owner/current controller of a reentered object.
        departed
    } else if damage.niv_damage_origin_v1.is_some() {
        return Vec::new();
    } else {
        bound_source
    };
    let Some(source_object) = state.objects.try_get(source.source) else {
        return Vec::new();
    };
    if source_object.owner != source.owner
        || source_object.card_def != source.card_def
        || source_object.zone_change_count < source.zone_change_count
        || (damage.historical_source.is_none()
            && damage.niv_damage_origin_v1.is_none()
            && source.controller != state.objects.get(damage.source).controller)
    {
        return Vec::new();
    }

    let mut captures = Vec::new();
    for (observer_id, observer) in state.objects.iter() {
        if observer.zone != Zone::Battlefield
            || observer.controller == recipient
            || observer.controller != source.controller
            || !crate::continuous_characteristics_v1::printed_abilities_active(state, observer_id)
            || crate::card_def::CARD_DEFS
                .get(usize::from(observer.card_def))
                .is_none_or(|definition| {
                    !definition.is_executable() || definition.name != "Niv-Mizzet, Visionary"
                })
        {
            continue;
        }
        captures.push(NivMizzetVisionaryDamageCaptureV1 {
            observer: crate::state::AbilitySourceContractV4::capture(state, observer_id),
            damage_source: source,
        });
    }
    captures
}

/// Build pending draws from the exact appended receipt and its committed
/// damage. The receipt is definition-specific; it does not retrofit combat or
/// controller metadata onto the legacy Damage event.
pub(crate) fn niv_mizzet_visionary_triggers_v1(
    state: &GameState,
    events: &[CommittedEvent],
) -> Result<Vec<crate::trigger::PendingTrigger>, ObjectId> {
    let mut pending = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for event in events {
        let CommittedEvent::NivMizzetVisionaryDamageV1 {
            observer,
            damage_source,
            damage_history_index,
        } = event
        else {
            continue;
        };
        if niv_mizzet_visionary_damage_amount_v1(
            state,
            *observer,
            *damage_source,
            *damage_history_index,
        )
        .is_err()
            || !seen.insert((
                observer.source,
                observer.zone_change_count,
                *damage_history_index,
            ))
        {
            return Err(observer.source);
        }
        pending.push(crate::trigger::PendingTrigger {
            controller: observer.controller,
            source: observer.source,
            effect: crate::effect::EffectOp::NivMizzetVisionaryDrawV1 {
                damage_history_index: *damage_history_index,
            },
            is_madness_offer: false,
            kicked: false,
            target_spec: crate::card_def::TargetSpec::None,
            targets: Vec::new(),
            target_contracts: Vec::new(),
            placement_ordered: false,
            source_contract: Some(*observer),
            granted_by: None,
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        });
    }
    Ok(pending)
}

/// Validate one engine-produced Niv damage receipt and return the original
/// full-width committed amount. This follows the native mutable GameState /
/// event-history trust boundary; coordinated edits to both remain outside
/// structural validation.
fn niv_mizzet_visionary_damage_amount_v1(
    state: &GameState,
    observer: crate::state::AbilitySourceContractV4,
    damage_source: crate::state::AbilitySourceContractV4,
    damage_history_index: u64,
) -> Result<i32, ObjectId> {
    let history_index = usize::try_from(damage_history_index).map_err(|_| observer.source)?;
    let Some(CommittedEvent::Damage {
        source,
        target: Target::Player(recipient),
        amount,
    }) = state.engine.event_history.get(history_index)
    else {
        return Err(observer.source);
    };
    let Some((marker_index, _)) =
        state
            .engine
            .event_history
            .iter()
            .enumerate()
            .find(|(_, entry)| {
                matches!(entry,
            CommittedEvent::NivMizzetVisionaryDamageV1 {
                observer: found_observer,
                damage_source: found_source,
                damage_history_index: found_index,
            } if *found_observer == observer
                && *found_source == damage_source
                && *found_index == damage_history_index)
            })
    else {
        return Err(observer.source);
    };
    let Some(CommittedEvent::NivMizzetVisionaryDamageV1 { .. }) =
        state.engine.event_history.get(marker_index)
    else {
        return Err(observer.source);
    };
    let marker_count = state
        .engine
        .event_history
        .iter()
        .filter(|entry| {
            matches!(entry,
            CommittedEvent::NivMizzetVisionaryDamageV1 {
                observer: found_observer,
                damage_source: found_source,
                damage_history_index: found_index,
            } if *found_observer == observer
                && *found_source == damage_source
                && *found_index == damage_history_index)
        })
        .count();
    let observer_receipt_count = state
        .engine
        .event_history
        .iter()
        .filter(|entry| {
            matches!(entry,
            CommittedEvent::NivMizzetVisionaryDamageV1 {
                observer: found_observer,
                damage_history_index: found_index,
                ..
            } if *found_observer == observer && *found_index == damage_history_index)
        })
        .count();
    let departed_source = niv_source_departure_before_v1(state, damage_source, history_index)
        .map_err(|_| observer.source)?;
    if marker_index <= history_index
        || marker_count != 1
        || observer_receipt_count != 1
        || state.engine.event_history[history_index + 1..marker_index]
            .iter()
            .any(|entry| {
                !matches!(entry, CommittedEvent::NivMizzetVisionaryDamageV1 { damage_history_index: linked, .. } if *linked == damage_history_index)
            })
        || *amount <= 0
        || observer.zone != Zone::Battlefield
        || observer.controller == *recipient
        || damage_source.source != *source
        || damage_source.controller != observer.controller
        || departed_source.is_some_and(|departure| departure != damage_source)
    {
        return Err(observer.source);
    }
    let Some(observer_object) = state.objects.try_get(observer.source) else {
        return Err(observer.source);
    };
    let Some(source_object) = state.objects.try_get(*source) else {
        return Err(observer.source);
    };
    let observer_is_niv = crate::card_def::CARD_DEFS
        .get(usize::from(observer.card_def))
        .is_some_and(|definition| {
            definition.is_executable() && definition.name == "Niv-Mizzet, Visionary"
        });
    if !observer_is_niv
        || observer_object.card_def != observer.card_def
        || observer_object.owner != observer.owner
        || observer_object.zone_change_count < observer.zone_change_count
        || source_object.card_def != damage_source.card_def
        || source_object.owner != damage_source.owner
        || source_object.zone_change_count < damage_source.zone_change_count
    {
        return Err(observer.source);
    }
    Ok(*amount)
}

pub(crate) fn validate_niv_mizzet_visionary_pending_v1(
    state: &GameState,
    source: ObjectId,
    controller: PlayerId,
    effect: &crate::effect::EffectOp,
    source_contract: Option<crate::state::AbilitySourceContractV4>,
) -> Result<u64, String> {
    let crate::effect::EffectOp::NivMizzetVisionaryDrawV1 {
        damage_history_index,
    } = effect
    else {
        return Err("Niv-Mizzet trigger changed its receipt-bound draw effect".into());
    };
    let observer = source_contract.ok_or("Niv-Mizzet trigger lost its source contract")?;
    if observer.source != source
        || observer.controller != controller
        || niv_mizzet_visionary_damage_amount_v1(
            state,
            observer,
            find_niv_damage_source(state, observer, *damage_history_index)?,
            *damage_history_index,
        )
        .is_err()
    {
        return Err("Niv-Mizzet trigger no longer matches its damage receipt".into());
    }
    Ok(*damage_history_index)
}

fn find_niv_damage_source(
    state: &GameState,
    observer: crate::state::AbilitySourceContractV4,
    damage_history_index: u64,
) -> Result<crate::state::AbilitySourceContractV4, String> {
    state
        .engine
        .event_history
        .iter()
        .find_map(|entry| match entry {
            CommittedEvent::NivMizzetVisionaryDamageV1 {
                observer: found_observer,
                damage_source,
                damage_history_index: found_index,
            } if *found_observer == observer && *found_index == damage_history_index => {
                Some(*damage_source)
            }
            _ => None,
        })
        .ok_or_else(|| "Niv-Mizzet damage receipt is missing".into())
}

pub(crate) fn publish_niv_mizzet_visionary_trigger_v1(
    state: &mut GameState,
    stack_item_id: crate::ids::StackItemId,
    observer: crate::state::AbilitySourceContractV4,
    damage_history_index: u64,
) {
    state
        .engine
        .event_history
        .push(CommittedEvent::NivMizzetVisionaryTriggerStackedV1 {
            stack_item_id,
            observer,
            damage_history_index,
        });
}

pub(crate) fn resolve_niv_mizzet_visionary_draw_count_v1(
    state: &GameState,
    ctx: &crate::effect::ExecCtx,
    damage_history_index: u64,
) -> Result<i32, String> {
    let stack_item_id = ctx
        .stack_item_id
        .ok_or("Niv-Mizzet draw lost its stack item")?;
    let observer = ctx
        .ability_source_contract
        .ok_or("Niv-Mizzet draw lost its source binding")?;
    if ctx.source != observer.source || ctx.controller != observer.controller {
        return Err("Niv-Mizzet draw source/controller binding changed".into());
    }
    let bindings = state
        .engine
        .event_history
        .iter()
        .filter(|entry| {
            matches!(entry,
        CommittedEvent::NivMizzetVisionaryTriggerStackedV1 {
            stack_item_id: found_stack,
            observer: found_observer,
            damage_history_index: found_index,
        } if *found_stack == stack_item_id
            && *found_observer == observer
            && *found_index == damage_history_index)
        })
        .count();
    let unique_stack_binding = state
        .engine
        .event_history
        .iter()
        .filter(|entry| {
            matches!(entry,
        CommittedEvent::NivMizzetVisionaryTriggerStackedV1 { stack_item_id: found, .. }
        if *found == stack_item_id)
        })
        .count();
    if bindings != 1 || unique_stack_binding != 1 {
        return Err("Niv-Mizzet draw lacks its exact trigger-stack receipt".into());
    }
    let damage_source = find_niv_damage_source(state, observer, damage_history_index)?;
    niv_mizzet_visionary_damage_amount_v1(state, observer, damage_source, damage_history_index)
        .map_err(|_| "Niv-Mizzet draw damage receipt changed".to_string())
}

pub(crate) fn niv_mizzet_visionary_removes_hand_limit_v1(
    state: &GameState,
    player: PlayerId,
) -> bool {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .any(|id| {
            let object = state.objects.get(id);
            object.zone == Zone::Battlefield
                && object.controller == player
                && crate::continuous_characteristics_v1::printed_abilities_active(state, id)
                && crate::card_def::CARD_DEFS
                    .get(usize::from(object.card_def))
                    .is_some_and(|definition| {
                        definition.is_executable() && definition.name == "Niv-Mizzet, Visionary"
                    })
        })
}

/// Remembers the counters of a departing permanent whose own leave ability
/// reads them. Each exact departing incarnation remains available to its triggers.
fn record_counter_lki(state: &mut GameState, object: ObjectId) {
    let live = state.objects.get(object);
    #[cfg(feature = "standard-magezero-fixtures")]
    let reads_standard = crate::standard_statics_v1::reads_counter_lki(live.card_def);
    #[cfg(not(feature = "standard-magezero-fixtures"))]
    let reads_standard = false;
    let reads_soul = live.counters.soul != 0 || definition_reads_soul_lki_v1(live.card_def);
    let reads_revival = live.counters.revival != 0
        || crate::card_def::CARD_DEFS[usize::from(live.card_def)].name == "Nine-Lives Familiar";
    let reads_fellowship = live.counters.fellowship != 0;
    if !reads_standard && !reads_soul && !reads_revival && !reads_fellowship {
        return;
    }
    let mut entries = state.counter_lki_v1.take().unwrap_or_default();
    entries.retain(|entry| {
        entry.source.object != object || entry.source.zone_change_count != live.zone_change_count
    });
    let extras = crate::standard_creatures_v1::CounterExtrasV1 {
        loyalty: crate::planeswalker_v1::loyalty(state, object).unwrap_or(0),
        lifelink: live.v4.lifelink_keyword_counters,
        time: live.v4.time_counters_v1,
        finality: live.v4.creature_upgrade.as_ref().map_or(0, |u| u.finality),
    };
    if reads_soul || reads_revival || live.counters.any() || extras != Default::default() {
        entries.push(crate::state::CounterLkiV1 {
            source: crate::state::ObjectLinkV4 {
                object,
                zone_change_count: live.zone_change_count,
            },
            counters: live.counters,
            extras: (extras != Default::default()).then_some(extras),
        });
    }
    state.counter_lki_v1 = (!entries.is_empty()).then_some(entries);
}

fn definition_reads_soul_lki_v1(card_def: u16) -> bool {
    static READS: std::sync::OnceLock<Vec<bool>> = std::sync::OnceLock::new();
    READS
        .get_or_init(|| {
            crate::card_def::CARD_DEFS
                .iter()
                .map(|def| {
                    def.activated_abilities.iter().any(|ability| {
                        matches!(
                            (ability.effect)(),
                            crate::effect::EffectOp::LoseLifeEachOpponentEqualSourceSoulV1
                        )
                    })
                })
                .collect()
        })
        .get(usize::from(card_def))
        .copied()
        .unwrap_or(false)
}

fn initialize_entry_lore_v48(
    state: &mut GameState,
    object: ObjectId,
    input: Option<&crate::doubling_season_v48::ReplacementInputV48>,
) -> Result<(), String> {
    let live = state.objects.get(object);
    let def = &crate::card_def::CARD_DEFS[usize::from(live.card_def)];
    if !(crate::doubling_season_v48::requires_successor(state)
        || input.is_some_and(|input| !input.seasons.is_empty()))
        || live.v4.face_down_v1.is_some()
        || live.v4.face_index != 0
        || def.saga.is_none()
    {
        return Ok(());
    }
    let count = crate::doubling_season_v48::entry_amount(state, input, live.controller, 1)?;
    state.objects.get_mut(object).counters.lore =
        i16::try_from(count).map_err(|_| "entry lore count overflow")?;
    Ok(())
}

fn initialize_entry_counters(
    state: &mut GameState,
    object: ObjectId,
    kicked: bool,
    counter_input: Option<&crate::doubling_season_v48::ReplacementInputV48>,
) {
    let live = state.objects.get(object);
    let definition = &crate::card_def::CARD_DEFS[live.card_def as usize];
    if definition.is_executable() {
        if let Some(entry) = definition.enters_with_plus_one_counters {
            if !entry.if_kicked || kicked {
                let entry_count = match crate::doubling_season_v48::entry_amount(
                    state,
                    counter_input,
                    live.controller,
                    entry.count,
                ) {
                    Ok(count) => count,
                    Err(_) => {
                        state.engine.halted = Some((
                            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                            object,
                        ));
                        return;
                    }
                };
                state.objects.get_mut(object).counters.plus1_plus1 = entry_count;
            }
        }
    }
    crate::fdn_raid_v1::apply_entry_counters(state, object, counter_input);
    #[cfg(feature = "standard-magezero-fixtures")]
    crate::standard_statics_v1::apply_conditional_entry_counters(state, object, counter_input);
}

pub(crate) fn log_plus_one_counters_added(
    state: &mut GameState,
    object: ObjectId,
    player: PlayerId,
    count: i32,
) {
    log_plus_one_counters_added_with_input_v48(state, object, player, count, None);
}
fn log_plus_one_counters_added_with_input_v48(
    state: &mut GameState,
    object: ObjectId,
    player: PlayerId,
    count: i32,
    input: Option<u64>,
) {
    if count <= 0 || !cfg!(feature = "limited-fdn-fixtures") {
        return;
    }
    let committed = CommittedEvent::PlusOneCountersAdded {
        object,
        zone_change_count: state.objects.get(object).zone_change_count,
        player,
        count,
        replacement_input_v48: crate::doubling_season_v48::CounterInputRefV48(input),
    };
    state.engine.event_log.push(committed.clone());
    state.engine.event_history.push(committed.clone());
    crate::wildwood_scourge_v1::capture_counter_event(state, &committed);
}

/// Exact checked placement, shared by counter doubling and ordinary effects.
pub(crate) fn add_plus_one_counters(
    state: &mut GameState,
    object: ObjectId,
    player: PlayerId,
    count: i32,
) -> Result<(), String> {
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield || count < 0 {
        return Err("invalid +1/+1 counter placement".to_string());
    }
    let original = count;
    let replacement =
        crate::doubling_season_v48::ReplacementInputV48::capture(state, live.controller);
    let count = crate::doubling_season_v48::counter_amount(
        state,
        object,
        player,
        crate::doubling_season_v48::CounterOriginV48::Effect,
        count,
    )?;
    let total = live
        .counters
        .plus1_plus1
        .checked_add(count)
        .ok_or("+1/+1 counter overflow")?;
    let before = live.counters.plus1_plus1;
    let recipient = crate::state::ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    let replacement_input = if original > 0 && !replacement.seasons.is_empty() {
        Some(crate::doubling_season_v48::PlusCounterInputV48::capture(
            state, object, player, original,
        )?)
    } else {
        None
    };
    let input_index = replacement_input.map(|mut input| {
        input.index = state.engine.event_history.len() as u64;
        let at = input.index;
        state
            .engine
            .event_history
            .push(CommittedEvent::PlusCounterInputV48 {
                input: Box::new(input),
            });
        at
    });
    state.objects.get_mut(object).counters.plus1_plus1 = total;
    let counter_event = state.engine.event_history.len() as u64;
    log_plus_one_counters_added_with_input_v48(state, object, player, count, input_index);
    if let Some(input) = input_index {
        let index = state.engine.event_history.len() as u64;
        state
            .engine
            .event_history
            .push(CommittedEvent::PlusCounterReplacementV48 {
                placement: Box::new(crate::doubling_season_v48::PlusCounterPlacementV48 {
                    index,
                    input,
                    recipient,
                    actor: player,
                    replacement,
                    original,
                    before,
                    after: total,
                    counter_event,
                }),
            });
    }
    Ok(())
}

/// Checked source-relative soul placement. The ability controller remains
/// its placing actor; current control determines the recipient replacements.
pub(crate) fn add_soul_counter_to_ability_source_v1(
    state: &mut GameState,
    ctx: &crate::effect::ExecCtx,
) -> Result<(), String> {
    let source = ctx
        .ability_source_contract
        .ok_or("soul placement lacks original source")?;
    if source.source != ctx.source
        || source.controller != ctx.controller
        || source.zone != Zone::Battlefield
    {
        return Err("soul placement changed original source relation".into());
    }
    let live = state
        .objects
        .try_get(source.source)
        .ok_or("soul source missing")?;
    if live.owner != source.owner {
        return Err("soul placement changed physical source owner".into());
    }
    if live.zone_change_count < source.zone_change_count || live.counters.soul < 0 {
        return Err("soul placement has invalid generation or counter count".into());
    }
    if live.zone != Zone::Battlefield || live.zone_change_count != source.zone_change_count {
        return Ok(());
    }
    let replacement =
        crate::doubling_season_v48::ReplacementInputV48::capture(state, live.controller);
    let scaled = crate::standard_cards_v1::scale_counters_checked_v48(state, ctx.controller, 1)?;
    let count = replacement.replaced_count(scaled)?;
    let before = live.counters.soul;
    let after = before.checked_add(count).ok_or("soul counter overflow")?;
    let input = crate::soul_counters_v1::PlacementInputV1::capture(state, source, ctx.controller)?;
    let event = CommittedEvent::SoulCounterAddedV1 {
        input_history_index: input.index,
        source,
        player: ctx.controller,
        replacement,
        before,
        count,
        after,
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::SoulCounterInputV1 {
            input: Box::new(input),
        });
    state.objects.get_mut(source.source).counters.soul = after;
    state.engine.event_log.push(event.clone());
    state.engine.event_history.push(event);
    Ok(())
}

/// Runs the replace/prevent pass to a fixed point: repeatedly finds an
/// active replacement that applies to `event` and hasn't already touched it
/// (loop-prevention via `touched_by`), applies it, and marks it touched.
/// Returns `None` if the event ends up fully prevented.
pub fn apply_replacements(
    state: &mut GameState,
    mut proposed: ProposedEvent,
) -> Option<ProposedEvent> {
    if let Err(source) = validate_exile_if_dies_replacements(state) {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            source,
        ));
        return None;
    }
    if let ProposedEvent::Damage(damage) = &proposed {
        if let Some(id) = damage.historical_source {
            if crate::source_power_activation_v1::damage_source(state, damage.source, id).is_err() {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    damage.source,
                ));
                return None;
            }
        }
    }
    if let ProposedEvent::ZoneChange(change) = &mut proposed {
        if change.to_zone == Zone::Graveyard
            && state.objects.try_get(change.object).is_some_and(|o| {
                o.zone == Zone::Battlefield
                    && o.v4
                        .creature_upgrade
                        .as_ref()
                        .is_some_and(|u| u.finality > 0)
            })
        {
            change.to_zone = Zone::Exile;
        }
    }
    let damage_cannot_be_prevented = matches!(&proposed, ProposedEvent::Damage(_))
        && state.engine.until_end_of_turn.iter().any(|effect| {
            matches!(
                effect,
                crate::engine::UntilEndOfTurnEffect::DamageCannotBePrevented { .. }
            )
        });
    if let ProposedEvent::Damage(damage) = &mut proposed {
        if crate::twinflame_tyrant_v1::replace(state, damage).is_err() {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                damage.source,
            ));
            return None;
        }
        crate::standard_cards_v1::replace_damage(state, damage);
    }
    if let ProposedEvent::Damage(damage) = &proposed {
        if crate::effect::crystal_prevention_v1::needs_choice(state, damage) {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                damage.source,
            ));
            return None;
        }
        if !damage_cannot_be_prevented && crate::crystal_barricade_v1::prevents(state, damage) {
            return None;
        }
        if !damage_cannot_be_prevented
            && damage.historical_source.map_or_else(
                || {
                    crate::engine::damage_is_prevented_by_protection(
                        state,
                        damage.source,
                        damage.target,
                    )
                },
                |id| match damage.target {
                    Target::Object(target) => crate::source_power_activation_v1::protected(
                        state,
                        crate::source_power_activation_v1::damage_source(state, damage.source, id)
                            .expect("validated historical source"),
                        target,
                    ),
                    _ => false,
                },
            )
        {
            return None;
        }
    }
    loop {
        let damage_cannot_be_prevented = matches!(&proposed, ProposedEvent::Damage(_))
            && state.engine.until_end_of_turn.iter().any(|effect| {
                matches!(
                    effect,
                    crate::engine::UntilEndOfTurnEffect::DamageCannotBePrevented { .. }
                )
            });
        let hit = state
            .engine
            .active_replacements
            .iter()
            .find(|r| {
                !(proposed.touched_by().contains(&r.id)
                    || damage_cannot_be_prevented
                        && matches!(
                            &r.kind,
                            ReplacementEffectKind::PreventNextDamage { .. }
                                | ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn { .. }
                                | ReplacementEffectKind::PreventCombatDamageToObjectUntilEndOfTurn { .. }
                        ))
                    && replacement_applies(r, &proposed, state)
            })
            .cloned();

        let Some(repl) = hit else {
            return Some(proposed);
        };

        proposed.mark_touched(repl.id);
        match replacement_apply(&repl, proposed, state) {
            Some(rewritten) => proposed = rewritten,
            None => return None,
        }
    }
}

pub(crate) fn replacement_applies(
    repl: &ActiveReplacement,
    proposed: &ProposedEvent,
    state: &GameState,
) -> bool {
    match (&repl.kind, proposed) {
        (ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 { target, turn, active_player, .. }, ProposedEvent::ZoneChange(change)) => {
            *turn == state.turn_key_v1() && *active_player == state.active_player
                && change.to_zone == Zone::Graveyard
                && crate::fiery_annihilation_v1::original_battlefield_target(state, Target::Object(change.object), *target)
                && state.engine.event_history.iter().any(|event| matches!(event,
                    CommittedEvent::ExileIfDiesReplacementInstalledV1 { replacement } if replacement == repl))
        }
        (
            ReplacementEffectKind::PreventNextDamage { target, remaining },
            ProposedEvent::Damage(d),
        ) => *remaining > 0 && d.target == *target,
        (
            ReplacementEffectKind::PreventCombatDamageToObjectUntilEndOfTurn {
                object,
                turn,
                active_player,
            },
            ProposedEvent::Damage(d),
        ) => {
            d.is_combat
                && *turn == state.turn_key_v1()
                && *active_player == state.active_player
                && d.target == Target::Object(object.object)
                && state.objects.try_get(object.object).is_some_and(|live| {
                    live.zone == Zone::Battlefield
                        && live.zone_change_count == object.zone_change_count
                })
        }
        (
            ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn {
                color,
                turn,
                active_player,
            },
            ProposedEvent::Damage(d),
        ) => {
            *turn == state.turn_key_v1()
                && *active_player == state.active_player
                && damage_color_mask(state, d) & crate::card_def::mana_color_mask(*color) != 0
        }
        _ => false,
    }
}

/// Applies `repl` to `proposed`, mutating the replacement's own bookkeeping
/// in `state.engine.active_replacements` (e.g. decrementing/expiring a
/// prevention shield's remaining count) as a side effect.
pub(crate) fn replacement_apply(
    repl: &ActiveReplacement,
    proposed: ProposedEvent,
    state: &mut GameState,
) -> Option<ProposedEvent> {
    match (&repl.kind, proposed) {
        (
            ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 { .. },
            ProposedEvent::ZoneChange(mut change),
        ) => {
            change.to_zone = Zone::Exile;
            Some(ProposedEvent::ZoneChange(change))
        }
        (
            ReplacementEffectKind::PreventNextDamage { remaining, .. },
            ProposedEvent::Damage(mut d),
        ) => {
            if crate::effect::crystal_prevention_v1::shield_operation(state, repl, d.amount)
                .is_err()
            {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    d.source,
                ));
                return None;
            }
            let prevented = (*remaining).min(d.amount);
            d.amount -= prevented;

            if let Some(slot) = state
                .engine
                .active_replacements
                .iter_mut()
                .find(|r| r.id == repl.id)
            {
                // Only variant today, but matched explicitly (rather than
                // destructured directly) so this stays a real pattern match
                // -- and a compile error, not a silent no-op -- the moment
                // a second `ReplacementEffectKind` is added.
                #[allow(irrefutable_let_patterns)]
                if let ReplacementEffectKind::PreventNextDamage { remaining, .. } = &mut slot.kind {
                    *remaining -= prevented;
                }
            }
            state.engine.active_replacements.retain(|r| {
                !matches!(&r.kind, ReplacementEffectKind::PreventNextDamage { remaining, .. } if *remaining <= 0)
            });

            if d.amount <= 0 {
                None
            } else {
                Some(ProposedEvent::Damage(d))
            }
        }
        (
            ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn { .. },
            ProposedEvent::Damage(_),
        ) => None,
        (
            ReplacementEffectKind::PreventCombatDamageToObjectUntilEndOfTurn { .. },
            ProposedEvent::Damage(_),
        ) => None,
        (_, other) => Some(other),
    }
}

/// Installs a W/U/B/R/G prevention replacement through the same monotonic
/// identity allocator used by all replacement effects.
pub fn install_color_damage_prevention(
    state: &mut GameState,
    source: ObjectId,
    color: ManaColor,
) -> Result<ReplacementId, String> {
    if color == ManaColor::C {
        return Err("colorless is not a legal Prismatic Strands choice".to_string());
    }
    let id = state
        .engine
        .next_replacement_id
        .checked_add(1)
        .ok_or("replacement identity space exhausted")?;
    state.engine.next_replacement_id = id;
    state.engine.active_replacements.push(ActiveReplacement {
        id,
        source,
        kind: ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn {
            color,
            turn: state.turn_key_v1(),
            active_player: state.active_player,
        },
    });
    Ok(id)
}

pub(crate) fn install_combat_damage_prevention(
    state: &mut GameState,
    source: ObjectId,
    object: ObjectId,
) {
    let Some(live) = state.objects.try_get(object) else {
        return;
    };
    if live.zone != Zone::Battlefield {
        return;
    }
    let binding = crate::state::ObjectLinkV4 {
        object,
        zone_change_count: live.zone_change_count,
    };
    let Some(id) = state.engine.next_replacement_id.checked_add(1) else {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            source,
        ));
        return;
    };
    state.engine.next_replacement_id = id;
    state.engine.active_replacements.push(ActiveReplacement {
        id,
        source,
        kind: ReplacementEffectKind::PreventCombatDamageToObjectUntilEndOfTurn {
            object: binding,
            turn: state.turn_key_v1(),
            active_player: state.active_player,
        },
    });
}

pub(crate) fn install_exile_if_dies(
    state: &mut GameState,
    source: crate::state::AbilitySourceContractV4,
    target: crate::state::StackTargetContractV4,
) {
    let Some(id) = state.engine.next_replacement_id.checked_add(1) else {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            source.source,
        ));
        return;
    };
    let replacement = ActiveReplacement {
        id,
        source: source.source,
        kind: ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 {
            target,
            source,
            turn: state.turn_key_v1(),
            active_player: state.active_player,
        },
    };
    let marker = CommittedEvent::ExileIfDiesReplacementInstalledV1 {
        replacement: replacement.clone(),
    };
    state.engine.next_replacement_id = id;
    state.engine.event_log.push(marker.clone());
    state.engine.event_history.push(marker);
    state.engine.active_replacements.push(replacement);
}

pub(crate) fn validate_exile_if_dies_replacements(state: &GameState) -> Result<(), ObjectId> {
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::ExileIfDiesReplacementCleanupV1 {
            index,
            installations,
            turn,
            active_player,
        } = event
        else {
            continue;
        };
        if *index as usize != at || installations.is_empty() {
            return Err(installations.first().map_or(ObjectId(0), |r| r.source));
        }
        let expected: Vec<_> = state.engine.event_history[..at]
            .iter()
            .filter_map(|e| {
                let CommittedEvent::ExileIfDiesReplacementInstalledV1 { replacement } = e else {
                    return None;
                };
                (!state.engine.event_history[..at].iter().any(|e| {
                    matches!(e,
                CommittedEvent::ExileIfDiesReplacementExpiredV1 { replacement: r, .. }
                if r == replacement)
                }))
                .then_some(replacement)
            })
            .collect();
        if expected.len() != installations.len()
            || expected.iter().any(|r| {
                installations
                    .iter()
                    .filter(|captured| *captured == *r)
                    .count()
                    != 1
            })
        {
            return Err(expected
                .first()
                .map_or(installations[0].source, |r| r.source));
        }
        for replacement in installations {
            let install = state.engine.event_history[..at].iter().filter(|e| matches!(e,
                CommittedEvent::ExileIfDiesReplacementInstalledV1 { replacement: r } if r == replacement)).count();
            let expiry = state.engine.event_history.iter().enumerate().filter(|(position, e)| matches!(e,
                CommittedEvent::ExileIfDiesReplacementExpiredV1 {
                    replacement: r, cleanup_index, turn: t, active_player: p,
                } if r == replacement && cleanup_index == index && t == turn && p == active_player && *position > at)).count();
            if install != 1
                || expiry != 1
                || installations.iter().filter(|r| *r == replacement).count() != 1
                || state
                    .engine
                    .active_replacements
                    .iter()
                    .any(|r| r.id == replacement.id)
                || !matches!(replacement.kind,
                    ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 { turn: t, active_player: p, .. }
                    if t == *turn && p == *active_player)
            {
                return Err(replacement.source);
            }
        }
    }
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::ExileIfDiesReplacementExpiredV1 {
            replacement,
            turn,
            active_player,
            cleanup_index,
        } = event
        else {
            continue;
        };
        let Some(CommittedEvent::ExileIfDiesReplacementCleanupV1 {
            index,
            installations,
            turn: ending_turn,
            active_player: ending_player,
        }) = state.engine.event_history.get(*cleanup_index as usize)
        else {
            return Err(replacement.source);
        };
        let installed = state.engine.event_history.iter().position(|e| matches!(e,
            CommittedEvent::ExileIfDiesReplacementInstalledV1 { replacement: r } if r == replacement));
        if *index != *cleanup_index
            || installed.is_none_or(|i| i >= *cleanup_index as usize)
            || *cleanup_index as usize >= at
            || turn != ending_turn
            || active_player != ending_player
            || installations.iter().filter(|r| *r == replacement).count() != 1
        {
            return Err(replacement.source);
        }
    }
    for event in &state.engine.event_history {
        let CommittedEvent::ExileIfDiesReplacementInstalledV1 { replacement } = event else {
            continue;
        };
        let live = state
            .engine
            .active_replacements
            .iter()
            .filter(|r| *r == replacement)
            .count();
        let expired = state.engine.event_history.iter().filter(|e| matches!(e,
            CommittedEvent::ExileIfDiesReplacementExpiredV1 { replacement: r, turn, active_player, .. }
            if r == replacement && matches!(r.kind,
                ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 { turn: original, active_player: actor, .. }
                if original == *turn && actor == *active_player))).count();
        if !matches!((live, expired), (1, 0) | (0, 1)) {
            return Err(replacement.source);
        }
    }
    for replacement in &state.engine.active_replacements {
        let ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 {
            target,
            source,
            turn,
            active_player,
        } = &replacement.kind
        else {
            continue;
        };
        let target_valid = match *target {
            crate::state::StackTargetContractV4::Object {
                object,
                card_def,
                owner,
                zone: Zone::Battlefield,
                zone_change_count,
                ..
            } => state.objects.try_get(object).is_some_and(|live| {
                live.card_def == card_def
                    && live.owner == owner
                    && live.zone_change_count >= zone_change_count
            }),
            _ => false,
        };
        let source_valid = source.source == replacement.source
            && state.objects.try_get(source.source).is_some_and(|live| {
                live.card_def == source.card_def
                    && live.owner == source.owner
                    && live.zone_change_count >= source.zone_change_count
            });
        let receipt = state.engine.event_history.iter().filter(|event| matches!(event,
            CommittedEvent::ExileIfDiesReplacementInstalledV1 { replacement: captured } if captured == replacement)).count();
        if !target_valid
            || !source_valid
            || receipt != 1
            || replacement.id == 0
            || replacement.id > state.engine.next_replacement_id
            || state
                .engine
                .active_replacements
                .iter()
                .filter(|other| other.id == replacement.id)
                .count()
                != 1
            || *turn != state.turn_key_v1()
            || *active_player != state.active_player
        {
            return Err(replacement.source);
        }
    }
    Ok(())
}

pub(crate) fn expire_exile_if_dies_replacements(state: &mut GameState) -> Result<(), ObjectId> {
    validate_exile_if_dies_replacements(state)?;
    let expired: Vec<_> = state
        .engine
        .active_replacements
        .iter()
        .filter(|r| {
            matches!(
                r.kind,
                ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 { .. }
            )
        })
        .cloned()
        .collect();
    if expired.is_empty() {
        return Ok(());
    }
    if state.step != crate::state::Step::Cleanup {
        return Err(expired[0].source);
    }
    let cleanup_index =
        u32::try_from(state.engine.event_history.len()).map_err(|_| expired[0].source)?;
    state
        .engine
        .event_history
        .push(CommittedEvent::ExileIfDiesReplacementCleanupV1 {
            index: cleanup_index,
            installations: expired.clone(),
            turn: state.turn_key_v1(),
            active_player: state.active_player,
        });
    for replacement in expired {
        state
            .engine
            .event_history
            .push(CommittedEvent::ExileIfDiesReplacementExpiredV1 {
                replacement,
                turn: state.turn_key_v1(),
                active_player: state.active_player,
                cleanup_index,
            });
    }
    state.engine.active_replacements.retain(|r| {
        !matches!(
            r.kind,
            ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 { .. }
        )
    });
    Ok(())
}

fn lifelink_gain_for(state: &GameState, event: &ProposedEvent) -> Option<(PlayerId, i32)> {
    let ProposedEvent::Damage(damage) = event else {
        return None;
    };
    if damage.amount <= 0 {
        return None;
    }
    if let Some(id) = damage.historical_source {
        let facts =
            crate::source_power_activation_v1::damage_source(state, damage.source, id).ok()?;
        return facts
            .lifelink
            .then_some((facts.source.controller, damage.amount));
    }
    let source = state.objects.try_get(damage.source)?;
    crate::engine::has_effective_keyword(state, damage.source, crate::card_def::Keywords::LIFELINK)
        .then_some((source.controller, damage.amount))
}

/// Convenience: replace/prevent then commit if anything survived. Lifelink
pub(crate) fn damage_color_mask(state: &GameState, damage: &DamageProposed) -> u8 {
    damage.historical_source.map_or_else(
        || crate::engine::object_color_mask(state, damage.source),
        |id| {
            crate::source_power_activation_v1::damage_source(state, damage.source, id)
                .expect("validated historical source")
                .color_mask
        },
    )
}

/// Convenience: replace/prevent then commit if anything survived. Lifelink
/// is evaluated from the final damage amount and source immediately before
/// that damage is committed.
pub fn propose_and_commit(state: &mut GameState, event: ProposedEvent) {
    if unsupported_partial_token_v1(&event) {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ObjectId(0),
        ));
        return;
    }
    if crate::linked_aura_operation_v1::needs_event_owner(state)
        && crate::linked_aura_operation_v1::event(
            state,
            crate::linked_aura_operation_v1::OperationV1::Event(event.clone()),
        )
    {
        return;
    }
    propose_without_linked_aura_v1(state, event);
}

pub(crate) fn propose_without_linked_aura_v1(state: &mut GameState, event: ProposedEvent) {
    if unsupported_partial_token_v1(&event) {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ObjectId(0),
        ));
        return;
    }
    if crate::effect::crystal_prevention_v1::validate(state).is_err() {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            crate::ids::ObjectId(0),
        ));
        return;
    }
    propose_and_commit_inner(state, event);
}
/// Only the scoped pre-pop Crystal interpreter calls this while holding the
/// privately constructed removed-item loan. Generic public begin/resume refuse it.
pub(crate) fn propose_and_commit_crystal_owned(
    state: &mut GameState,
    damage: DamageProposed,
    ctx: &crate::effect::ExecCtx,
    op: &crate::effect::EffectOp,
    path: &[u16],
    owner: &crate::effect::crystal_prevention_v1::OwnerV1,
) -> Result<u32, String> {
    if !owner.permits_damage(state, &ProposedEvent::Damage(damage.clone())) {
        return Err("Crystal damage lost private removed-item authority".into());
    }
    prepare_commit_crystal_damage(state, damage, ctx, op, path, owner)
}
/// Genuine opaque Fiery final instruction under the same removed-item loan.
/// Any selected linked-Aura operation must retain its surrounding owning replay.
pub(crate) fn propose_and_commit_twinflame_fiery_equipment_owned(
    state: &mut GameState,
    capture: &crate::twinflame_tyrant_v1::fiery::CaptureV38,
    owner: &crate::effect::crystal_prevention_v1::OwnerV1,
) -> Result<(), String> {
    let Some(crate::state::Target::Object(equipment)) = capture.ctx.targets.get(1) else {
        return Err("Fiery equipment instruction lost actual selected object".into());
    };
    if !capture.legal[1]
        || owner
            .packet_before(
                state,
                &capture.ctx,
                &crate::effect::EffectOp::FieryAnnihilation,
                &capture.path,
            )
            .is_none_or(|(_, item)| item != *capture.item)
        || !matches!(state.engine.event_history.get(capture.index as usize),Some(CommittedEvent::TwinflameFieryCaptureV38{capture:original}) if original.as_ref()==capture)
        || crate::linked_aura_operation_v1::needs_surrounding_operation_owner(state)
    {
        return Err(
            "Fiery equipment instruction lost private original or linked-operation owner".into(),
        );
    }
    propose_and_commit_inner(
        state,
        ProposedEvent::zone_change(*equipment, crate::state::Zone::Exile),
    );
    if state.engine.halted.is_some() {
        return Err("Fiery actual equipment exile halted".into());
    }
    Ok(())
}
pub(crate) fn propose_and_commit_crystal_power_owned(
    state: &mut GameState,
    damage: DamageProposed,
    capture: &crate::effect::crystal_prevention_v1::CreaturePowerCaptureV2,
    owner: &crate::effect::crystal_prevention_v1::OwnerV1,
) -> Result<u32, String> {
    if !owner.permits_creature_power(state, capture, &ProposedEvent::Damage(damage.clone())) {
        return Err("Crystal chosen-power damage lost private removed-item authority".into());
    }
    prepare_commit_crystal_damage(
        state,
        damage,
        &capture.ctx,
        &capture.op,
        &capture.path,
        owner,
    )
}
/// Separate actual interpreter operation. Completion alone cannot invent its
/// replacement preparation or omit its accepted positive/fully-prevented result.
pub(crate) fn twinflame_packet_lifelink_player(
    state: &GameState,
    damage: DamageProposed,
) -> Option<PlayerId> {
    lifelink_gain_for(state, &ProposedEvent::Damage(damage)).map(|(player, _)| player)
}
pub(crate) fn twinflame_lifelink_player(
    state: &GameState,
    choice: &crate::effect::crystal_prevention_v1::ChoiceV1,
) -> Option<PlayerId> {
    let mut damage =
        match ProposedEvent::damage(choice.source, choice.target, choice.remaining_amount) {
            ProposedEvent::Damage(d) => d,
            _ => unreachable!(),
        };
    damage.historical_source = choice.historical_source;
    lifelink_gain_for(state, &ProposedEvent::Damage(damage)).map(|(player, _)| player)
}
pub(crate) fn commit_twinflame_ordered_scalar(
    state: &mut GameState,
    owner: &crate::effect::crystal_prevention_v1::OwnerV1,
    before: &crate::effect::EffectContinuation,
    choice: &crate::effect::crystal_prevention_v1::ChoiceV1,
) -> Result<(u32, Option<u32>), String> {
    if !owner.permits_replacement_answer(state, before)
        || choice.remaining_amount <= 0
        || !choice.options.is_empty()
        || choice.batch.is_some()
    {
        return Err("ordered positive packet lacks genuine scalar answer owner".into());
    }
    let mut damage =
        match ProposedEvent::damage(choice.source, choice.target, choice.remaining_amount) {
            ProposedEvent::Damage(d) => d,
            _ => unreachable!(),
        };
    damage.historical_source = choice.historical_source;
    damage.niv_damage_origin_v1 = choice.niv_damage_origin_v1;
    damage.touched_by = choice.touched.clone();
    let proposed = ProposedEvent::Damage(damage);
    let gain = lifelink_gain_for(state, &proposed);
    let at = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "ordered damage index overflow")?;
    commit(state, proposed);
    let gain_at = if let Some((player, amount)) = gain {
        let index = u32::try_from(state.engine.event_history.len())
            .map_err(|_| "ordered gain index overflow")?;
        commit(state, ProposedEvent::life_gain(player, amount));
        Some(index)
    } else {
        None
    };
    Ok((at, gain_at))
}

fn prepare_commit_crystal_damage(
    state: &mut GameState,
    damage: DamageProposed,
    ctx: &crate::effect::ExecCtx,
    op: &crate::effect::EffectOp,
    path: &[u16],
    owner: &crate::effect::crystal_prevention_v1::OwnerV1,
) -> Result<u32, String> {
    use crate::effect::crystal_prevention_v1::{BatchDamageV2, DamageOperationV3};
    if crate::effect::temporary_control_v1::authority::validate(state).is_err()
        || damage.amount <= 0
        || damage.is_combat
    {
        return Err("Crystal damage operation has invalid input or foreign authority".into());
    }
    let preventers = crate::effect::crystal_prevention_v1::applicable(state, &damage);
    let original = BatchDamageV2 {
        source: damage.source,
        target: damage.target,
        amount: damage.amount,
        touched: damage.touched_by.clone(),
    };
    let historical_source = damage.historical_source;
    let mut guaranteed_zero = false;
    if preventers.iter().any(|o| {
        matches!(
            o,
            crate::effect::crystal_prevention_v1::PreventionV1::Multiplier(_)
        )
    }) {
        let (previous, item) = owner
            .packet_before(state, ctx, op, path)
            .ok_or("multiplier preparation lacks genuine removed owner")?;
        let preparation = crate::twinflame_tyrant_v1::ordering::PreparationV38 {
            index: u32::try_from(state.engine.event_history.len())
                .map_err(|_| "multiplier preparation index overflow")?,
            previous,
            item: Box::new(item),
            ctx: ctx.clone(),
            op: op.clone(),
            path: path.to_vec(),
            input: original.clone(),
            historical_source,
            options: preventers.clone(),
            lifelink_player: lifelink_gain_for(state, &ProposedEvent::Damage(damage.clone()))
                .map(|(p, _)| p),
        };
        guaranteed_zero = crate::twinflame_tyrant_v1::ordering::prepared_amount(&preparation)? == 0;
        state
            .engine
            .event_history
            .push(CommittedEvent::TwinflameDamagePreparationV38 {
                preparation: Box::new(preparation),
            });
    }
    // Full prevention commutes to the exact zero result, even when an
    // unchosen multiplier-first intermediate exceeds the damage representation.
    // Its independently published options/preparation remain mandatory proof.
    let prepared = if guaranteed_zero {
        None
    } else {
        apply_replacements(state, ProposedEvent::Damage(damage))
    };
    if state.engine.halted.is_some() {
        return Err("Crystal damage preparation halted".into());
    }
    let output = match prepared {
        Some(ProposedEvent::Damage(ref d)) => Some(BatchDamageV2 {
            source: d.source,
            target: d.target,
            amount: d.amount,
            touched: d.touched_by.clone(),
        }),
        None => None,
        _ => return Err("Crystal damage operation changed proposal class".into()),
    };
    let mut damage_history = None;
    if let Some(final_event) = prepared {
        let gain = lifelink_gain_for(state, &final_event);
        let at = u32::try_from(state.engine.event_history.len())
            .map_err(|_| "Crystal damage history exceeds u32")?;
        commit(state, final_event);
        if !matches!(
            state.engine.event_history.get(at as usize),
            Some(CommittedEvent::Damage { .. })
        ) {
            return Err("Crystal positive damage lost actual committed occurrence".into());
        }
        damage_history = Some(at);
        if let Some((player, amount)) = gain {
            commit(state, ProposedEvent::life_gain(player, amount));
        }
    }
    let at = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "Crystal operation history exceeds u32")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::CrystalDamageOperationV3 {
            operation: Box::new(DamageOperationV3 {
                history_index: at,
                ctx: ctx.clone(),
                op: op.clone(),
                path: path.to_vec(),
                input: original,
                historical_source,
                preventers,
                output,
                damage_history,
            }),
        });
    Ok(at)
}
pub(crate) fn propose_and_commit_crystal_discard_draw_owned(
    state: &mut GameState,
    capture: &crate::effect::crystal_prevention_v1::DiscardCaptureV2,
    selected: crate::effect::EffectObjectBinding,
    owner: &crate::effect::crystal_prevention_v1::OwnerV1,
) {
    if !owner.permits_discard_draw(state, capture, selected) {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            capture.ctx.source,
        ));
        return;
    }
    propose_and_commit_inner(state, ProposedEvent::draw(capture.ctx.controller));
}
pub(crate) fn propose_and_commit_crystal_life_gain_owned(
    state: &mut GameState,
    player: crate::effect::PlayerRef,
    amount: i32,
    owner: &crate::effect::crystal_prevention_v1::OwnerV1,
) {
    let beneficiary = match player {
        crate::effect::PlayerRef::Controller => owner.controller(),
        crate::effect::PlayerRef::Opponent => owner.controller().opponent(),
        _ => {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                crate::ids::ObjectId(0),
            ));
            return;
        }
    };
    let proposed = ProposedEvent::life_gain(beneficiary, amount);
    if !owner.permits_life_gain(state, &proposed, player, amount) {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            crate::ids::ObjectId(0),
        ));
        return;
    }
    propose_and_commit_inner(state, proposed);
}
fn propose_and_commit_inner(state: &mut GameState, event: ProposedEvent) {
    if crate::effect::temporary_control_v1::authority::validate(state).is_err() {
        return;
    }
    if matches!(&event,ProposedEvent::CreateToken(t) if !crate::doubling_season_v48::sources(state,t.controller).is_empty())
    {
        batch_without_linked_aura_v1(state, vec![event]);
        return;
    }
    if needs_final_zone_pt_plan_v1(state, std::slice::from_ref(&event)) {
        // Replace on a private proposal scene. A refused final PT must not
        // consume a prevention shield or publish its bookkeeping either.
        let mut replaced = state.clone();
        let Some(final_event) = apply_replacements(&mut replaced, event) else {
            if let Some((_, source)) = replaced.engine.halted {
                halt_zone_pt_v1(state, source);
            } else {
                *state = replaced;
            }
            return;
        };
        if crate::doubling_season_v48::preflight_entry_roster(
            &replaced,
            std::slice::from_ref(&final_event),
        )
        .is_err()
            || crate::doubling_season_v48::preflight_token_initialization(
                &replaced,
                std::slice::from_ref(&final_event),
            )
            .is_err()
        {
            halt_zone_pt_v1(state, ObjectId(0));
            return;
        }
        match crate::linked_aura_operation_v1::prepare_entry_colors(
            &mut replaced,
            std::slice::from_ref(&final_event),
        ) {
            Ok(true) => {}
            Ok(false) => {
                *state = replaced;
                return;
            }
            Err(_) => {
                halt_zone_pt_v1(state, ObjectId(0));
                return;
            }
        }
        let input = crate::doubling_season_v48::entry_input(&replaced, &final_event);
        let plan = match FinalZonePtPlanV1::capture(
            &replaced,
            std::slice::from_ref(&final_event),
            None,
            None,
            std::slice::from_ref(&input),
            &[],
        ) {
            Ok(plan) => plan,
            Err(source) => {
                halt_zone_pt_v1(state, source);
                return;
            }
        };
        let lifelink_gain = lifelink_gain_for(&replaced, &final_event);
        *state = replaced;
        commit_with_entry_batch_and_source_color(
            state,
            final_event,
            None,
            None,
            None,
            (None, None),
            ((None, false), None, input, None, Some(&plan)),
        );
        if state.engine.halted.is_none() {
            if let Some((player, amount)) = lifelink_gain {
                commit(state, ProposedEvent::life_gain(player, amount));
            }
        }
        return;
    }
    if let Some(final_event) = apply_replacements(state, event) {
        let lifelink_gain = lifelink_gain_for(state, &final_event);
        commit(state, final_event);
        if let Some((player, amount)) = lifelink_gain {
            commit(state, ProposedEvent::life_gain(player, amount));
        }
    }
}

/// The selected observer sees genuine as-entry Disguise, X counters and
/// attachment characteristics. Ordinary catalogs retain their old entry path.
pub(crate) fn commit_garruk_initialized_entry(
    state: &mut GameState,
    owner: &crate::garruks_uprising_v1::PermanentEntryOwnerV1,
) -> Result<(), String> {
    crate::effect::temporary_control_v1::authority::validate(state)?;
    state
        .validate_attachment_relations()
        .map_err(|_| "Garruk entry begins with invalid attachment relations")?;
    let entering = owner.entering_characteristics(state)?;
    if let Some(event) = apply_replacements(
        state,
        ProposedEvent::zone_change(entering.0, Zone::Battlefield),
    ) {
        commit_with_ability_lki_and_entry_batch(
            state,
            event,
            None,
            None,
            None,
            (None, None),
            (Some(entering), false),
        );
    }
    Ok(())
}

/// Only the nonserializable owner obtained before actual engine removal may
/// commit the printed Raid damage. Ordinary proposals retain their old path.
pub(crate) fn propose_and_commit_gorehorn_owned(
    state: &mut GameState,
    damage: DamageProposed,
    preparation: u32,
    owner: &crate::gorehorn_damage_v1::OwnerV1,
) -> Result<u32, String> {
    if !owner.permits_damage(state, &damage) {
        return Err("Gorehorn damage lacks actual removed-item authority".into());
    }
    crate::effect::temporary_control_v1::authority::validate(state)?;
    let replacements_before = state.engine.active_replacements.clone();
    let turn = state.turn_key_v1();
    let active = state.active_player;
    let prevention_overridden = state.engine.until_end_of_turn.iter().any(|effect| {
        matches!(
            effect,
            crate::engine::UntilEndOfTurnEffect::DamageCannotBePrevented { .. }
        )
    });
    let start = state.engine.event_history.len();
    let final_event = apply_replacements(state, ProposedEvent::Damage(damage));
    let mut output = None;
    if let Some(event) = final_event {
        let ProposedEvent::Damage(damage) = &event else {
            return Err("Gorehorn replacement changed event kind".into());
        };
        output = Some(crate::gorehorn_damage_v1::DamageResultV1 {
            source: damage.source,
            target: damage.target,
            amount: damage.amount,
            touched: damage.touched_by.clone(),
        });
        let gain = lifelink_gain_for(state, &event);
        commit(state, event);
        if let Some((player, amount)) = gain {
            commit(state, ProposedEvent::life_gain(player, amount));
        }
    }
    let damage_index = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(i, e)| matches!(e, CommittedEvent::Damage { .. }).then_some(i))
        .map(u32::try_from)
        .transpose()
        .map_err(|_| "Gorehorn damage index overflow")?;
    let index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "Gorehorn operation overflow")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::GorehornDamageOperationV1 {
            operation: Box::new(crate::gorehorn_damage_v1::DamageOperationV1 {
                index,
                preparation,
                replacements_before,
                output,
                damage_index,
                turn,
                active,
                prevention_overridden,
            }),
        });
    Ok(index)
}

/// Applies the (possibly rewritten) proposal to `GameState` and appends the
/// resulting `CommittedEvent` to the event log for this resolution.
pub fn commit(state: &mut GameState, event: ProposedEvent) {
    if unsupported_partial_token_v1(&event) {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ObjectId(0),
        ));
        return;
    }
    if crate::linked_aura_operation_v1::needs_event_owner(state)
        && crate::linked_aura_operation_v1::event(
            state,
            crate::linked_aura_operation_v1::OperationV1::Commit(event.clone()),
        )
    {
        return;
    }
    commit_without_linked_aura_v1(state, event);
}

pub(crate) fn commit_without_linked_aura_v1(state: &mut GameState, event: ProposedEvent) {
    if unsupported_partial_token_v1(&event) {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ObjectId(0),
        ));
        return;
    }
    commit_with_ability_lki(state, event, None, None);
}

fn commit_with_ability_lki(
    state: &mut GameState,
    event: ProposedEvent,
    abilities_removed_before: Option<bool>,
    frozen_death_captures: Option<Vec<CommittedEvent>>,
) {
    commit_with_ability_lki_and_entry_batch(
        state,
        event,
        abilities_removed_before,
        frozen_death_captures,
        None,
        (None, None),
        (None, false),
    );
}

fn commit_with_ability_lki_and_entry_batch(
    state: &mut GameState,
    event: ProposedEvent,
    abilities_removed_before: Option<bool>,
    frozen_death_captures: Option<Vec<CommittedEvent>>,
    authority_batch: Option<&mut crate::authority_of_the_consuls_v1::EntryBatchV1>,
    entry_batches: (
        Option<&mut crate::garruks_uprising_v1::EntryBatchV1>,
        Option<&mut crate::arahbo_v1::EntryBatchV1>,
    ),
    entry_context: (
        Option<(ObjectId, crate::garruks_uprising_v1::EntryCharacteristicsV1)>,
        bool,
    ),
) {
    commit_with_entry_batch_and_source_color(
        state,
        event,
        abilities_removed_before,
        frozen_death_captures,
        authority_batch,
        entry_batches,
        (entry_context, None, None, None, None),
    );
}

fn prepare_zone_entry_characteristics_v1(
    event: &ProposedEvent,
    entry_characteristics: &mut Option<(
        ObjectId,
        crate::garruks_uprising_v1::EntryCharacteristicsV1,
    )>,
) -> Result<(), ObjectId> {
    if let ProposedEvent::ZoneChange(change) = event {
        if let Some(revival) = change.battlefield_revival_v1 {
            if revival < 0 {
                return Err(change.object);
            }
        }
        if let Some(request) = change.battlefield_x_v48.as_ref() {
            if change.to_zone != Zone::Battlefield || request.original < 0 {
                return Err(change.object);
            }
            let entry =
                entry_characteristics.get_or_insert_with(|| (change.object, Default::default()));
            if entry.0 != change.object
                || entry
                    .1
                    .x_counters
                    .is_some_and(|count| count != request.original)
                || entry
                    .1
                    .attached_to
                    .is_some_and(|host| Some(host) != request.host)
            {
                return Err(change.object);
            }
            entry.1.x_counters = Some(request.original);
            entry.1.attached_to = request.host;
            entry.1.bestowed = request.host.is_some();
        }
        // A destination rewrite that does not enter the battlefield must not
        // install the entry-only characteristic on the card in another zone.
        if change.to_zone == Zone::Battlefield {
            if let Some(subtype) = change.battlefield_added_subtype_v1 {
                let entry = entry_characteristics
                    .get_or_insert_with(|| (change.object, Default::default()));
                if entry.0 != change.object {
                    return Err(change.object);
                }
                entry.1.added_subtype_v1 = Some(subtype);
            }
            if change.battlefield_grant_flying_v1 {
                let entry = entry_characteristics
                    .get_or_insert_with(|| (change.object, Default::default()));
                if entry.0 != change.object {
                    return Err(change.object);
                }
                entry.1.grant_flying_v1 = true;
            }
        }
    }
    Ok(())
}

type EntryContextWithSourceColorV1<'a> = (
    (
        Option<(ObjectId, crate::garruks_uprising_v1::EntryCharacteristicsV1)>,
        bool,
    ),
    Option<crate::controlled_creature_grants_v1::DepartureColorV1>,
    Option<crate::doubling_season_v48::ReplacementInputV48>,
    Option<u64>,
    Option<&'a FinalZonePtPlanV1>,
);
fn commit_with_entry_batch_and_source_color(
    state: &mut GameState,
    event: ProposedEvent,
    abilities_removed_before: Option<bool>,
    frozen_death_captures: Option<Vec<CommittedEvent>>,
    mut authority_batch: Option<&mut crate::authority_of_the_consuls_v1::EntryBatchV1>,
    entry_batches: (
        Option<&mut crate::garruks_uprising_v1::EntryBatchV1>,
        Option<&mut crate::arahbo_v1::EntryBatchV1>,
    ),
    entry_context: EntryContextWithSourceColorV1<'_>,
) {
    let (
        mut entry_context,
        frozen_source_color,
        frozen_counter_input,
        token_input_index,
        zone_plan,
    ) = entry_context;
    if let Err(source) = prepare_zone_entry_characteristics_v1(&event, &mut entry_context.0) {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            source,
        ));
        return;
    }
    let counter_input =
        frozen_counter_input.or_else(|| crate::doubling_season_v48::entry_input(state, &event));
    if let ProposedEvent::ZoneChange(change) = &event {
        if let Some(revival) = change.battlefield_revival_v1 {
            let controller = change.battlefield_controller.unwrap_or_else(|| {
                let object = state.objects.get(change.object);
                if object.zone == Zone::Stack {
                    object.controller
                } else {
                    object.owner
                }
            });
            if crate::doubling_season_v48::entry_amount(
                state,
                counter_input.as_ref(),
                controller,
                revival,
            )
            .is_err()
            {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    change.object,
                ));
                return;
            }
        }
        let source = state.objects.get(change.object);
        let cast_nine_lives = change.to_zone == Zone::Battlefield
            && source.zone == Zone::Stack
            && crate::card_def::CARD_DEFS[usize::from(source.card_def)].name
                == "Nine-Lives Familiar"
            && source.spell_copy_origin.is_none()
            && !source.v4.is_token
            && source
                .v4
                .spell_cast_origin
                .is_some_and(|origin| origin.finalized_method.is_some());
        if cast_nine_lives {
            if crate::doubling_season_v48::entry_amount(
                state,
                counter_input.as_ref(),
                change.battlefield_controller.unwrap_or(source.controller),
                8,
            )
            .is_err()
            {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    change.object,
                ));
                return;
            }
        }
    }
    let season_entry_active = crate::doubling_season_v48::requires_successor(state)
        || counter_input
            .as_ref()
            .is_some_and(|input| !input.seasons.is_empty());
    if let Some(input) = counter_input.as_ref() {
        let (definition, kicked, face_down, x, time) = match &event {
            ProposedEvent::CreateToken(token) => (token.token_def, false, false, None, None),
            ProposedEvent::ZoneChange(change) => {
                let live = state.objects.get(change.object);
                let characteristics = entry_context.0.as_ref().map(|(_, entry)| entry);
                (
                    live.card_def,
                    live.zone == Zone::Stack
                        && state.engine.pending_kicked_source == Some(change.object),
                    characteristics.is_some_and(|entry| entry.face_down.is_some()),
                    characteristics.and_then(|entry| entry.x_counters),
                    characteristics.and_then(|entry| entry.time_counters),
                )
            }
            _ => unreachable!("entry input only exists for entering permanents"),
        };
        if crate::doubling_season_v48::preflight_entry_definition(
            state,
            definition,
            input,
            (kicked, x, face_down, time),
        )
        .is_err()
        {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ObjectId(0),
            ));
            return;
        }
    }

    match crate::linked_aura_operation_v1::prepare_entry_colors(state, std::slice::from_ref(&event))
    {
        Ok(true) => {}
        Ok(false) => return,
        Err(_) => {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ObjectId(0),
            ));
            return;
        }
    }
    let entry_color_proposal = event.clone();
    let source_color = frozen_source_color
        .or_else(|| crate::controlled_creature_grants_v1::capture_departure_color(state, &event));
    let (entering_characteristics, defer_raid_entry) = entry_context;
    let conqueror_single = (!defer_raid_entry)
        .then(|| crate::bloodthirsty_conqueror_v49::RecorderV49::capture(state))
        .flatten();
    let (mut garruk_batch, mut arahbo_batch) = entry_batches;
    // Direct commits are also used for simultaneous state-based deaths. Apply
    // an installed Fiery shield before any zone bookkeeping or frozen death
    // capture is published. Unrelated direct commits retain their old path.
    let fiery_death = state.engine.active_replacements.iter().any(|replacement| {
        matches!(
            replacement.kind,
            ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 { .. }
        ) && replacement_applies(replacement, &event, state)
    });
    let mut event = if fiery_death {
        let Some(rewritten) = apply_replacements(state, event) else {
            return;
        };
        rewritten
    } else {
        event
    };
    if let ProposedEvent::ZoneChange(change) = &mut event {
        if change.to_zone == Zone::Graveyard
            && state.objects.try_get(change.object).is_some_and(|o| {
                o.zone == Zone::Battlefield
                    && o.v4
                        .creature_upgrade
                        .as_ref()
                        .is_some_and(|u| u.finality > 0)
            })
        {
            change.to_zone = Zone::Exile;
        }
    }
    let local_zone_plan = if zone_plan.is_none()
        && needs_final_zone_pt_plan_v1(state, std::slice::from_ref(&event))
    {
        match FinalZonePtPlanV1::capture(
            state,
            std::slice::from_ref(&event),
            authority_batch
                .as_ref()
                .map(|batch| batch.observers.as_slice()),
            entry_context.0,
            std::slice::from_ref(&counter_input),
            &[],
        ) {
            Ok(plan) => Some(plan),
            Err(source) => {
                halt_zone_pt_v1(state, source);
                return;
            }
        }
    } else {
        None
    };
    let zone_plan = zone_plan.or(local_zone_plan.as_ref());
    // A frozen observer may outlive its source, but a rewritten destination
    // is not a death. Finality and sacrifice replacements must not publish
    // a capture beside a battlefield-to-exile event.
    let death_captures = if matches!(&event, ProposedEvent::ZoneChange(change)
        if change.to_zone == Zone::Graveyard)
    {
        match frozen_death_captures
            .map(Ok)
            .unwrap_or_else(|| crate::creature_death_capture_v1::capture(state, &event))
        {
            Ok(captures) => captures,
            Err(_) => {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ObjectId(0),
                ));
                return;
            }
        }
    } else {
        Vec::new()
    };
    let greatwurm_single = if !entry_context.1 {
        match crate::effect::greatwurm_v1::capture_wave(state, std::slice::from_ref(&event)) {
            Ok(wave) => Some(wave),
            Err(_) => {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ObjectId(0),
                ));
                return;
            }
        }
    } else {
        None
    };
    let greatwurm_history_start = state.engine.event_history.len();
    let source_copy_departure =
        match crate::effect::source_token_copy_v1::departure_input(state, &event) {
            Ok(input) => input,
            Err(_) => {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ObjectId(0),
                ));
                return;
            }
        };
    let niv_mizzet_damage_captures = capture_niv_mizzet_visionary_damage_v1(state, &event);
    let tutelage_observers = match &event {
        ProposedEvent::Draw(draw) => state
            .objects
            .iter()
            .filter_map(|(id, object)| {
                (object.zone == Zone::Battlefield
                    && object.controller == draw.player
                    && object.v4.face_index == 0
                    && crate::continuous_characteristics_v1::printed_abilities_active(state, id)
                    && is_sphinxs_tutelage_v1(object.card_def))
                .then(|| crate::state::AbilitySourceContractV4::capture(state, id))
            })
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    let lathril_source = match &event {
        ProposedEvent::Damage(d)
            if d.is_combat && d.amount > 0 && matches!(d.target, Target::Player(_)) =>
        {
            state
                .objects
                .try_get(d.source)
                .filter(|o| {
                    o.zone == Zone::Battlefield
                        && o.v4.face_index == 0
                        && o.v4.face_down_v1.is_none()
                        && is_lathril_v1(o.card_def)
                        && crate::continuous_characteristics_v1::printed_abilities_active(
                            state, d.source,
                        )
                })
                .map(|_| {
                    (
                        crate::state::AbilitySourceContractV4::capture(state, d.source),
                        d.target,
                        d.amount,
                    )
                })
        }
        _ => None,
    };
    let Some(MaterializedEventV1 {
        committed,
        alesha_power_departure,
        niv_source_departure,
    }) = materialize_event_v1(
        state,
        event,
        NativeMaterializationInputV1 {
            authority_observers: authority_batch
                .as_ref()
                .map(|batch| batch.observers.as_slice()),
            entering_characteristics,
            counter_input: &counter_input,
            season_entry_active,
            token_input_index,
            mode: NativeMaterializationV1::Commit(zone_plan),
            abilities_removed_before,
        },
    )
    else {
        return;
    };
    let saga_source = match &committed {
        CommittedEvent::ZoneChange {
            object,
            to: Zone::Battlefield,
            ..
        }
        | CommittedEvent::CreateToken { object, .. }
            if state.objects.get(*object).v4.face_index == 0
                && (!season_entry_active
                    || state.objects.get(*object).v4.face_down_v1.is_none())
                && crate::card_def::CARD_DEFS[usize::from(state.objects.get(*object).card_def)]
                    .saga
                    .is_some()
                && (matches!(&committed, CommittedEvent::ZoneChange { .. })
                    || season_entry_active) =>
        {
            Some(*object)
        }
        _ => None,
    };
    let entry_counter_object = match &committed {
        CommittedEvent::ZoneChange {
            object,
            to: Zone::Battlefield,
            ..
        }
        | CommittedEvent::CreateToken { object, .. } => Some(*object),
        _ => None,
    };
    if let Some(object) = entry_counter_object {
        if let Err(source) =
            initialize_materialized_entry_v1(state, &entry_color_proposal, object, &counter_input)
        {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                source,
            ));
            return;
        }
    }
    let left_battlefield = match &committed {
        CommittedEvent::ZoneChange {
            object,
            from: Zone::Battlefield,
            to,
            ..
        } if *to != Zone::Battlefield => Some(*object),
        _ => None,
    };
    let single_graveyard_entry = match &committed {
        CommittedEvent::ZoneChange {
            object,
            to: Zone::Battlefield,
            ..
        }
        | CommittedEvent::CreateToken { object, .. } => Some(*object),
        _ => None,
    };
    if let Some(snapshot) = source_color.filter(|snapshot| {
        matches!(&committed, CommittedEvent::ZoneChange { object, from, to, .. }
            if *object == snapshot.source.source && *from == snapshot.source.zone && to != from)
    }) {
        crate::controlled_creature_grants_v1::log_departure_color(state, snapshot);
    }
    state.engine.event_log.push(committed.clone());
    let committed_death = matches!(
        committed,
        CommittedEvent::ZoneChange {
            from: Zone::Battlefield,
            to: Zone::Graveyard,
            ..
        }
    );
    let entry_history_index = state.engine.event_history.len();
    state.engine.event_history.push(committed);
    record_copied_token_entry_v1(state, entry_history_index);
    crate::effect::curator_v1::record_entry(state, entry_history_index);
    if !tutelage_observers.is_empty()
        && matches!(
            state.engine.event_history.get(entry_history_index),
            Some(CommittedEvent::Draw {
                object: Some(_),
                ..
            })
        )
    {
        let receipt = CommittedEvent::SphinxsTutelageDrawV1 {
            draw_history_index: entry_history_index as u64,
            observers: tutelage_observers,
        };
        state.engine.event_log.push(receipt.clone());
        state.engine.event_history.push(receipt);
    }
    if let Some(source) = niv_source_departure {
        if let Some(departure_history_index) = state
            .engine
            .event_history
            .len()
            .checked_sub(1)
            .and_then(|index| u64::try_from(index).ok())
        {
            if matches!(state.engine.event_history.last(),
                Some(CommittedEvent::ZoneChange { object, from, to, controller_before })
                    if *object == source.source
                        && *from == source.zone
                        && *from == Zone::Battlefield
                        && to != from
                        && *controller_before == source.controller)
            {
                let receipt = CommittedEvent::NivMizzetVisionaryDamageSourceDepartureV1 {
                    source,
                    departure_history_index,
                };
                state.engine.event_history.push(receipt);
            }
        }
    }
    if !niv_mizzet_damage_captures.is_empty() {
        let damage_history_index = state.engine.event_history.len() - 1;
        let Some(CommittedEvent::Damage {
            source,
            target: Target::Player(recipient),
            amount,
        }) = state.engine.event_history.get(damage_history_index)
        else {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ObjectId(0),
            ));
            return;
        };
        let (source, recipient, amount) = (*source, *recipient, *amount);
        for capture in niv_mizzet_damage_captures {
            if capture.damage_source.source != source
                || capture.observer.controller == recipient
                || capture.damage_source.controller != capture.observer.controller
                || amount <= 0
            {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    capture.observer.source,
                ));
                return;
            }
            let Ok(damage_history_index) = u64::try_from(damage_history_index) else {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    capture.observer.source,
                ));
                return;
            };
            let receipt = CommittedEvent::NivMizzetVisionaryDamageV1 {
                observer: capture.observer,
                damage_source: capture.damage_source,
                damage_history_index,
            };
            state.engine.event_log.push(receipt.clone());
            state.engine.event_history.push(receipt);
        }
    }
    if let Some((source, Target::Player(player), amount)) = lathril_source {
        let receipt = CommittedEvent::LathrilCombatDamageV1 {
            damage_history_index: entry_history_index as u64,
            source,
            player,
            amount,
        };
        state.engine.event_log.push(receipt.clone());
        state.engine.event_history.push(receipt);
    }
    if let Some(wave) = greatwurm_single {
        if wave.finish(state, greatwurm_history_start).is_err() {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ObjectId(0),
            ));
            return;
        }
    }
    if let Some((source, card_def, zone_change_count, power)) = alesha_power_departure {
        if zone_change_count
            .checked_add(1)
            .is_some_and(|next| state.objects.get(source).zone_change_count == next)
            && state.objects.get(source).zone != Zone::Battlefield
        {
            let receipt = CommittedEvent::AleshaPowerLkiV1 {
                source,
                card_def,
                zone_change_count,
                departure_history_index: entry_history_index as u64,
                power,
            };
            state.engine.event_log.push(receipt.clone());
            state.engine.event_history.push(receipt);
        }
    }
    if let Some(object) = entry_counter_object {
        if crate::linked_aura_operation_v1::commit_entry_color(
            state,
            &entry_color_proposal,
            object,
            entry_history_index,
        )
        .and_then(|_| {
            crate::linked_aura_operation_v1::commit_kinship_entry(
                state,
                &entry_color_proposal,
                object,
                entry_history_index,
            )
        })
        .is_err()
        {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                object,
            ));
            return;
        }
    }
    crate::banishing_light_v1::record_entry(state, entry_history_index);
    crate::life_loss_turn_v1::capture_committed_loss_at(state, entry_history_index);
    if let Some(batch) = conqueror_single {
        batch.finish(state);
    }
    if committed_death {
        for mut capture in death_captures {
            crate::fiendish_panda_v1::bind_departure(&mut capture, entry_history_index);
            if crate::fake_your_own_death_v50::spell_custody::bind_death_departure(
                state,
                &mut capture,
                entry_history_index,
            )
            .is_err()
            {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ObjectId(0),
                ));
                return;
            }
            state.engine.event_log.push(capture.clone());
            state.engine.event_history.push(capture);
        }
    }
    if let Some(source) = single_graveyard_entry {
        crate::soul_shackled_zombie_v1::record_entry(state, source);
    }
    if let Some(capture) = crate::raid_entry_v1::record_entry(state, entry_history_index) {
        if !defer_raid_entry {
            crate::raid_entry_custody_v1::capture_entry(state, capture);
        }
    }
    if let Some(batch) = garruk_batch.as_mut() {
        batch.record(state, entry_history_index)
    } else {
        crate::garruks_uprising_v1::record_single_entry(state, entry_history_index)
    }

    if let Some(batch) = arahbo_batch.as_mut() {
        batch.record(state, entry_history_index);
    } else {
        crate::arahbo_v1::record_single_entry(state, entry_history_index);
    }
    if let Some(batch) = authority_batch.as_mut() {
        batch.record(entry_history_index);
    } else {
        crate::authority_of_the_consuls_v1::record_entry(state, entry_history_index);
    }
    if crate::effect::source_token_copy_v1::record_departure(
        state,
        source_copy_departure,
        entry_history_index,
    )
    .is_err()
    {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ObjectId(0),
        ));
        return;
    }
    if let Some(object) = left_battlefield {
        crate::standard_legends_v1::return_lagrella_exiles(
            state,
            object,
            state.objects.get(object).zone_change_count - 1,
        );
        if !state.engine.linked_exile_records.is_empty() {
            let left_zone_change_count = state.objects.get(object).zone_change_count - 1;
            crate::effect::return_cards_exiled_until_source_leaves(
                state,
                object,
                left_zone_change_count,
            );
        }
    }
    crate::life_gain_turn_v1::capture_committed_gain(state);
    // The predecessor first-gain hook consumes the actual last LifeGain.
    // Panda uses its original index and can then append its own markers.
    crate::fiendish_panda_v1::record_gain(state, entry_history_index);
    if let Some(object) = entry_counter_object {
        if !season_entry_active {
            crate::standard_cards_v1::initialize_entry_counters(
                state,
                object,
                counter_input.as_ref(),
            );
        }
        let live = state.objects.get(object);
        let count = live.counters.plus1_plus1;
        let controller = live.controller;
        log_plus_one_counters_added(state, object, controller, count);
    }
    if let Some(source) = saga_source {
        if season_entry_active {
            let live = state.objects.get(source);
            let chapters = crate::card_def::CARD_DEFS[usize::from(live.card_def)]
                .saga
                .as_ref()
                .filter(|_| {
                    crate::continuous_characteristics_v1::printed_abilities_active(state, source)
                })
                .map_or(0, |saga| saga.chapter_effects.len());
            let count = live
                .counters
                .lore
                .min(i16::try_from(chapters).unwrap_or(i16::MAX));
            for chapter in 1..=count {
                log_saga_chapter(state, source, chapter as u8);
            }
        } else {
            add_lore_counters(state, source, 1).expect("new Saga counters fit");
        }
    }
}

fn initialize_materialized_entry_v1(
    state: &mut GameState,
    entry_color_proposal: &ProposedEvent,
    object: ObjectId,
    counter_input: &Option<crate::doubling_season_v48::ReplacementInputV48>,
) -> Result<(), ObjectId> {
    if crate::linked_aura_operation_v1::initialize_kinship_entry(
        state,
        entry_color_proposal,
        object,
    )
    .is_err()
    {
        return Err(object);
    }
    // All fixed, X and token-entry counters already exist, while no entry
    // event or observer has been published. Use the original batch input.
    if crate::giada_v1::apply_entry_counters(state, object, counter_input.as_ref()).is_err() {
        return Err(object);
    }
    Ok(())
}

/// A fresh, private numeric scene plan. It contains no executable permission and
/// never survives this synchronous commit. Departures read the original scene.
struct FinalZonePtPlanV1 {
    departures: Vec<(crate::state::ObjectLinkV4, (i32, i32))>,
}
#[derive(Clone, Copy)]
enum NativeMaterializationV1<'a> {
    Commit(Option<&'a FinalZonePtPlanV1>),
    FinalScene(&'a FinalZonePtPlanV1),
}
impl NativeMaterializationV1<'_> {
    fn is_scene(self) -> bool {
        matches!(self, Self::FinalScene(_))
    }
    fn departure_pt(self, state: &GameState, id: ObjectId) -> Option<(i32, i32)> {
        let plan = match self {
            Self::Commit(plan) => plan?,
            Self::FinalScene(plan) => plan,
        };
        let generation = state.objects.get(id).zone_change_count;
        plan.departures.iter().find_map(|(link, pt)| {
            (link.object == id && link.zone_change_count == generation).then_some(*pt)
        })
    }
}
fn aberration_definition_v1(id: u16) -> bool {
    crate::card_def::CARD_DEFS
        .get(usize::from(id))
        .is_some_and(|d| {
            d.is_executable() && crate::engine::consuming_aberration_cda_for_v1(d.name)
        })
}
/// Select only actual battlefield characteristics, including suppressed forms
/// and reversible native copies. This is a cost guard, not an execution permit.
pub(crate) fn has_potential_aberration_battlefield_v1(state: &GameState) -> bool {
    state
        .objects
        .iter()
        .any(|(_, o)| o.zone == Zone::Battlefield && aberration_definition_v1(o.card_def))
        || crate::standard_cards_v1::has_battlefield_copy_origin_v1(state, aberration_definition_v1)
}
fn needs_final_zone_pt_plan_v1(state: &GameState, events: &[ProposedEvent]) -> bool {
    if !events.iter().any(|event| {
        matches!(
            event,
            ProposedEvent::ZoneChange(_)
                | ProposedEvent::CreateToken(_)
                | ProposedEvent::Transform(_)
        )
    }) {
        return false;
    }
    // Suppression and face-down characteristics do not exclude a source: this
    // very zone batch can remove the Aura or copy that currently hides its CDA.
    has_potential_aberration_battlefield_v1(state)
        || events.iter().any(|event| match event {
            ProposedEvent::ZoneChange(z) if z.to_zone == Zone::Battlefield => {
                aberration_definition_v1(state.objects.get(z.object).card_def)
            }
            ProposedEvent::CreateToken(t) => aberration_definition_v1(t.token_def),
            _ => false,
        })
}
impl FinalZonePtPlanV1 {
    fn capture(
        original: &GameState,
        events: &[ProposedEvent],
        authority_observers: Option<&[crate::state::AbilitySourceContractV4]>,
        characteristics: Option<(ObjectId, crate::garruks_uprising_v1::EntryCharacteristicsV1)>,
        counter_inputs: &[Option<crate::doubling_season_v48::ReplacementInputV48>],
        attachments: &[(
            crate::effect::EffectObjectBinding,
            crate::effect::EffectObjectBinding,
        )],
    ) -> Result<Self, ObjectId> {
        if events.len() != counter_inputs.len() {
            return Err(ObjectId(0));
        }
        let mut plan = Self {
            departures: Vec::new(),
        };
        for event in events {
            if let ProposedEvent::ZoneChange(z) = event {
                let object = original.objects.get(z.object);
                if object.zone == Zone::Battlefield {
                    let pt = crate::engine::checked_effective_pt(original, z.object)
                        .map_err(|_| z.object)?;
                    plan.departures.push((
                        crate::state::ObjectLinkV4 {
                            object: z.object,
                            zone_change_count: object.zone_change_count,
                        },
                        pt,
                    ));
                }
            }
        }
        // This is characteristic materialization, not event replay: no public
        // step/resume, replacement consumption, committed-event observation
        // or linked return runs here. Native entry initialization is shared;
        // its private bookkeeping never escapes this discarded scene. All
        // members exist before the one layer-7 query.
        let mut scene = original.clone();
        for (event, input) in events.iter().zip(counter_inputs) {
            let mut entry = characteristics;
            prepare_zone_entry_characteristics_v1(event, &mut entry)?;
            let season = crate::doubling_season_v48::requires_successor(original)
                || input
                    .as_ref()
                    .is_some_and(|input| !input.seasons.is_empty());
            let Some(materialized) = materialize_event_v1(
                &mut scene,
                event.clone(),
                NativeMaterializationInputV1 {
                    authority_observers,
                    entering_characteristics: entry,
                    counter_input: input,
                    season_entry_active: season,
                    token_input_index: None,
                    mode: NativeMaterializationV1::FinalScene(&plan),
                    abilities_removed_before: None,
                },
            ) else {
                return Err(scene
                    .engine
                    .halted
                    .map_or(ObjectId(0), |(_, source)| source));
            };
            if let Some((_, source)) = scene.engine.halted {
                return Err(source);
            }
            let entrant = match &materialized.committed {
                CommittedEvent::ZoneChange {
                    object,
                    to: Zone::Battlefield,
                    ..
                }
                | CommittedEvent::CreateToken { object, .. } => Some(*object),
                _ => None,
            };
            if let Some(object) = entrant {
                initialize_materialized_entry_v1(&mut scene, event, object, input)?;
                // Native static boosts authenticate the actual chosen entry
                // against its real materialized row. Reuse the existing private
                // constructor in this discarded scene; do not approximate that
                // boost by merely writing v4.chosen_color.
                let entry_index = scene.engine.event_history.len();
                scene
                    .engine
                    .event_history
                    .push(materialized.committed.clone());
                crate::linked_aura_operation_v1::commit_entry_color(
                    &mut scene,
                    event,
                    object,
                    entry_index,
                )
                .and_then(|_| {
                    crate::linked_aura_operation_v1::commit_kinship_entry(
                        &mut scene,
                        event,
                        object,
                        entry_index,
                    )
                })
                .map_err(|_| object)?;
                if !season {
                    crate::standard_cards_v1::initialize_entry_counters(
                        &mut scene,
                        object,
                        input.as_ref(),
                    );
                }
            }
        }
        for &(aura, host) in attachments {
            if scene.objects.get(aura.object).zone == Zone::Battlefield {
                let generation = scene.objects.get(aura.object).zone_change_count;
                scene
                    .attach_object_exact(
                        aura.object,
                        generation,
                        host.object,
                        host.expected_zone_change_count,
                    )
                    .map_err(|_| aura.object)?;
            }
        }
        // Lifelink is simultaneous with the damage. The exact source/controller
        // and replaced amount come from the original scene, as in actual commit.
        for (_, player, amount) in batch_lifelink_gains_v1(original, events)? {
            scene.players[player.index()].life = scene.players[player.index()]
                .life
                .checked_add(amount)
                .ok_or(ObjectId(0))?;
            #[cfg(feature = "standard-magezero-fixtures")]
            crate::standard_cards_v1::record_life(&mut scene, player, amount, 0);
        }
        if let Some((_, source)) = scene.engine.halted {
            return Err(source);
        }
        crate::engine::validate_battlefield_pt(&scene)?;
        Ok(plan)
    }
}
fn batch_lifelink_gains_v1(
    state: &GameState,
    events: &[ProposedEvent],
) -> Result<Vec<(ObjectId, PlayerId, i32)>, ObjectId> {
    let mut gains: Vec<(ObjectId, PlayerId, i32)> = Vec::new();
    for event in events {
        let Some((player, amount)) = lifelink_gain_for(state, event) else {
            continue;
        };
        let ProposedEvent::Damage(damage) = event else {
            unreachable!("only damage causes lifelink");
        };
        if let Some((_, _, total)) = gains
            .iter_mut()
            .find(|(source, controller, _)| *source == damage.source && *controller == player)
        {
            *total = total.checked_add(amount).ok_or(damage.source)?;
        } else {
            gains.push((damage.source, player, amount));
        }
    }
    Ok(gains)
}
fn direct_zone_scene_events_v1(state: &GameState, events: &[ProposedEvent]) -> Vec<ProposedEvent> {
    events
        .iter()
        .cloned()
        .map(|mut event| {
            let fiery = state.engine.active_replacements.iter().any(|r| {
                matches!(
                    r.kind,
                    ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 { .. }
                ) && replacement_applies(r, &event, state)
            });
            if fiery {
                // The only eligible replacement is destination-only, with no
                // consumable shield bookkeeping. Other replacements remain owned
                // by the caller's ordinary propose pass.
                if let ProposedEvent::ZoneChange(z) = &mut event {
                    z.to_zone = Zone::Exile;
                }
            }
            if let ProposedEvent::ZoneChange(z) = &mut event {
                if z.to_zone == Zone::Graveyard
                    && state
                        .objects
                        .get(z.object)
                        .v4
                        .creature_upgrade
                        .as_ref()
                        .is_some_and(|u| u.finality > 0)
                {
                    z.to_zone = Zone::Exile;
                }
            }
            event
        })
        .collect()
}
fn direct_zone_pt_plan_v1(
    state: &GameState,
    events: &[ProposedEvent],
) -> Result<Option<FinalZonePtPlanV1>, ObjectId> {
    if !needs_final_zone_pt_plan_v1(state, events) {
        return Ok(None);
    }
    let final_events = direct_zone_scene_events_v1(state, events);
    let inputs = final_events
        .iter()
        .map(|event| crate::doubling_season_v48::entry_input(state, event))
        .collect::<Vec<_>>();
    FinalZonePtPlanV1::capture(state, &final_events, None, None, &inputs, &[]).map(Some)
}

fn halt_zone_pt_v1(state: &mut GameState, source: ObjectId) {
    state.engine.halted = Some((
        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
        source,
    ));
}

/// The same native allocator/reset is used by commit and the final-scene check.
/// Scene mode never publishes departure receipts or invokes event observers.
struct NativeMaterializationInputV1<'a> {
    authority_observers: Option<&'a [crate::state::AbilitySourceContractV4]>,
    entering_characteristics:
        Option<(ObjectId, crate::garruks_uprising_v1::EntryCharacteristicsV1)>,
    counter_input: &'a Option<crate::doubling_season_v48::ReplacementInputV48>,
    season_entry_active: bool,
    token_input_index: Option<u64>,
    mode: NativeMaterializationV1<'a>,
    abilities_removed_before: Option<bool>,
}
fn materialize_event_v1(
    state: &mut GameState,
    event: ProposedEvent,
    input: NativeMaterializationInputV1<'_>,
) -> Option<MaterializedEventV1> {
    let NativeMaterializationInputV1 {
        authority_observers,
        entering_characteristics,
        counter_input,
        season_entry_active,
        token_input_index,
        mode,
        abilities_removed_before,
    } = input;
    let mut alesha_power_departure = None;
    let mut niv_source_departure = None;
    let committed = match event {
        ProposedEvent::Damage(d) => {
            let source_has_deathtouch = d.amount > 0
                && d.historical_source.map_or_else(
                    || {
                        state.objects.try_get(d.source).is_some()
                            && crate::engine::has_effective_keyword(
                                state,
                                d.source,
                                crate::card_def::Keywords::DEATHTOUCH,
                            )
                    },
                    |id| {
                        crate::source_power_activation_v1::damage_source(state, d.source, id)
                            .expect("validated historical source")
                            .deathtouch
                    },
                );
            match d.target {
                Target::Object(id) => {
                    let planeswalker = crate::engine::object_has_type(
                        state,
                        id,
                        crate::card_def::CardType::Planeswalker,
                    );
                    if planeswalker {
                        crate::planeswalker_v1::damage(state, id, d.amount);
                    }
                    let creature = crate::engine::object_has_type(
                        state,
                        id,
                        crate::card_def::CardType::Creature,
                    );
                    let obj = state.objects.get_mut(id);
                    if !planeswalker || creature {
                        let Some(damage) = obj.damage.checked_add(d.amount.max(0) as u32) else {
                            state.engine.halted = Some((
                                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                                d.source,
                            ));
                            return None;
                        };
                        obj.damage = damage;
                        obj.v4.deathtouch_damage |= source_has_deathtouch;
                    }
                }
                Target::StackItem(_) => return None,
                Target::Player(p) => {
                    let lost = d.amount;
                    #[cfg(feature = "standard-magezero-fixtures")]
                    let lost = crate::standard_statics_v1::modified_life_loss(state, p, lost);
                    #[cfg(feature = "standard-magezero-fixtures")]
                    if lost > 0 {
                        state.record_life_loss_v1(p);
                    }
                    state.players[p.index()].life -= lost;
                    #[cfg(feature = "standard-magezero-fixtures")]
                    crate::standard_cards_v1::record_life(state, p, 0, lost);
                }
            }
            #[cfg(feature = "standard-magezero-fixtures")]
            if let Some(id) = d.historical_source {
                let facts = crate::source_power_activation_v1::damage_source(state, d.source, id)
                    .expect("validated historical source");
                crate::standard_cards_v1::after_historical_damage(
                    state,
                    facts,
                    d.amount,
                    d.is_combat,
                );
            } else {
                crate::standard_cards_v1::after_damage(state, d.source, d.amount, d.is_combat);
            }
            CommittedEvent::Damage {
                source: d.source,
                target: d.target,
                amount: d.amount,
            }
        }
        ProposedEvent::ZoneChange(mut z) => {
            #[cfg(feature = "standard-magezero-fixtures")]
            crate::standard_keywords_v1::unearth_exile_instead(state, &mut z);
            #[cfg(not(feature = "standard-magezero-fixtures"))]
            let _ = &mut z;
            let from = state.objects.get(z.object).zone;
            let departure_pt = mode.departure_pt(state, z.object);
            let departure_power = departure_pt.map(|pt| pt.0);
            let controller_before = state.objects.get(z.object).controller;
            #[cfg(feature = "limited-fdn-fixtures")]
            let creature_died = from == Zone::Battlefield
                && z.to_zone == Zone::Graveyard
                && crate::engine::object_has_type(
                    state,
                    z.object,
                    crate::card_def::CardType::Creature,
                );
            if !mode.is_scene() {
                crate::standard_legends_v1::before_zone_change(state, z.object, z.to_zone);
                if from == Zone::Battlefield && z.to_zone != Zone::Battlefield {
                    crate::engine::record_different_controller_target_departure(state, z.object);
                }
                crate::standard_cards_v1::before_departure(state, z.object);
                if from == Zone::Battlefield
                    && crate::standard_cards_v1::active_aegis_copy(state, z.object)
                {
                    let marker = CommittedEvent::LeftBattlefieldCopyV1 {
                        source: crate::state::AbilitySourceContractV4::capture(state, z.object),
                        face_index: state.objects.get(z.object).v4.face_index,
                    };
                    state.engine.event_log.push(marker.clone());
                    state.engine.event_history.push(marker);
                }
                if from == Zone::Battlefield {
                    let live = state.objects.get(z.object);
                    if live.v4.face_index != 0
                        && crate::standard_cards_v1::records_departure_face(
                            &crate::card_def::CARD_DEFS[live.card_def as usize],
                        )
                    {
                        let marker = CommittedEvent::LeftBattlefieldFaceV1 {
                            object: z.object,
                            zone_change_count: live.zone_change_count,
                            face_index: live.v4.face_index,
                        };
                        state.engine.event_log.push(marker.clone());
                        state.engine.event_history.push(marker);
                    }
                }
                if from == Zone::Battlefield {
                    record_counter_lki(state, z.object);
                    if departure_pt.is_some() {
                        crate::creature_stats_lki_v1::record_creature_stats_lki_with_pt(
                            state,
                            z.object,
                            departure_pt,
                        );
                    } else {
                        #[cfg(feature = "standard-magezero-fixtures")]
                        crate::standard_statics_v1::record_creature_stats_lki(state, z.object);
                    }
                    if departure_power.is_some() {
                        crate::ashroot_animist_v1::before_departure_with_power(
                            state,
                            z.object,
                            departure_power,
                        );
                    } else {
                        crate::ashroot_animist_v1::before_departure(state, z.object);
                    }
                    if departure_pt.is_some() {
                        crate::source_power_activation_v1::before_departure_with_pt(
                            state,
                            z.object,
                            departure_pt,
                        );
                    } else {
                        crate::source_power_activation_v1::before_departure(state, z.object);
                    }
                    crate::gorehorn_damage_v1::before_departure(state, z.object);
                    crate::equipment_damage_source_v4::before_departure(state, z.object);
                }
                #[cfg(feature = "standard-magezero-fixtures")]
                if departure_power.is_some() {
                    crate::standard_keywords_v1::before_zone_change_with_power(
                        state,
                        z.object,
                        z.to_zone,
                        departure_power,
                    );
                } else {
                    crate::standard_keywords_v1::before_zone_change(state, z.object, z.to_zone);
                }
                if from == Zone::Battlefield
                    && abilities_removed_before.unwrap_or_else(|| {
                        !crate::continuous_characteristics_v1::printed_abilities_active(
                            state, z.object,
                        )
                    })
                {
                    let marker = CommittedEvent::PrintedAbilitiesRemovedBeforeZoneChange {
                        object: z.object,
                        zone_change_count: state.objects.get(z.object).zone_change_count,
                    };
                    state.engine.event_log.push(marker.clone());
                    state.engine.event_history.push(marker);
                }
                #[cfg(feature = "limited-fdn-fixtures")]
                if from == Zone::Battlefield {
                    crate::standard_cards_v1::end_aegis_copies_before_departure(state, z.object);
                }
                if from == Zone::Battlefield
                    && z.to_zone != Zone::Battlefield
                    && crate::card_def::CARD_DEFS
                        .get(usize::from(state.objects.get(z.object).card_def))
                        .is_some_and(|definition| definition.name == "Alesha, Who Laughs at Fate")
                {
                    let source = state.objects.get(z.object);
                    alesha_power_departure = Some((
                        z.object,
                        source.card_def,
                        source.zone_change_count,
                        departure_power
                            .unwrap_or_else(|| crate::engine::effective_power(state, z.object)),
                    ));
                }
                if from == Zone::Battlefield
                    && z.to_zone != Zone::Battlefield
                    && niv_mizzet_catalog_enabled_v1()
                {
                    niv_source_departure = Some(crate::state::AbilitySourceContractV4::capture(
                        state, z.object,
                    ));
                }
                refresh_creature_power_lki(state, z.object, from, departure_power);
            } else {
                #[cfg(feature = "standard-magezero-fixtures")]
                crate::standard_keywords_v1::record_descended_zone_change_v1(
                    state, z.object, z.to_zone,
                );
                #[cfg(feature = "limited-fdn-fixtures")]
                if from == Zone::Battlefield {
                    crate::standard_cards_v1::end_aegis_copies_before_departure(state, z.object);
                }
            }
            materialize_zone_change_v1(
                state,
                z.object,
                z.to_zone,
                z.library_placement,
                z.preserve_known_identity,
                z.library_insert_visibility,
                z.force_battlefield_tapped,
                z.battlefield_face_index,
                z.battlefield_controller,
                (
                    authority_observers,
                    {
                        let mut entry = entering_characteristics
                            .as_ref()
                            .filter(|(object, _)| *object == z.object)
                            .map(|(_, face)| *face);
                        if let Some(stun) = z.battlefield_stun_v48 {
                            entry.get_or_insert_with(Default::default).stun_counters_v48 =
                                Some(stun);
                        }
                        entry
                    },
                    counter_input.as_ref(),
                    z.battlefield_revival_v1,
                ),
            );
            #[cfg(feature = "limited-fdn-fixtures")]
            if !mode.is_scene()
                && creature_died
                && state.objects.get(z.object).zone == Zone::Graveyard
            {
                state.creature_death_turn_v1 = Some(crate::state::CreatureDeathTurnV1 {
                    turn: state.turn_key_v1(),
                    active_player: state.active_player,
                });
            }
            CommittedEvent::ZoneChange {
                object: z.object,
                from,
                to: z.to_zone,
                controller_before,
            }
        }
        ProposedEvent::LifeLoss(l) => {
            let amount = l.amount;
            #[cfg(feature = "standard-magezero-fixtures")]
            let amount = crate::standard_statics_v1::modified_life_loss(state, l.player, amount);
            #[cfg(feature = "standard-magezero-fixtures")]
            if amount > 0 {
                state.record_life_loss_v1(l.player);
            }
            state.players[l.player.index()].life -= amount;
            #[cfg(feature = "standard-magezero-fixtures")]
            crate::standard_cards_v1::record_life(state, l.player, 0, amount);
            CommittedEvent::LifeLoss {
                player: l.player,
                amount,
            }
        }
        ProposedEvent::LifeGain(g) => {
            if g.amount <= 0 {
                return None;
            }
            state.players[g.player.index()].life += g.amount;
            #[cfg(feature = "standard-magezero-fixtures")]
            crate::standard_cards_v1::record_life(state, g.player, g.amount, 0);
            CommittedEvent::LifeGain {
                player: g.player,
                amount: g.amount,
            }
        }
        ProposedEvent::Draw(d) => {
            let empty_before = state.players[d.player.index()].library.is_empty();
            let drawn = state.draw_card(d.player);
            if empty_before {
                state.players[d.player.index()].drew_from_empty = true;
            }
            if drawn.is_some() {
                state.players[d.player.index()].draws_this_turn += 1;
            }
            CommittedEvent::Draw {
                player: d.player,
                object: drawn,
            }
        }
        ProposedEvent::Tap(t) => {
            state.objects.get_mut(t.object).tapped = true;
            CommittedEvent::Tap { object: t.object }
        }
        ProposedEvent::ManaAdd(m) => {
            for &c in &m.colors {
                state.players[m.player.index()].mana_pool[c.pool_index()] += 1;
            }
            CommittedEvent::ManaAdded {
                player: m.player,
                colors: m.colors,
            }
        }
        ProposedEvent::CreateToken(t) => {
            let token_def = &crate::card_def::CARD_DEFS[t.token_def as usize];
            let name = token_def.object_name.to_string();
            let object = state.objects.push(crate::state::GameObject {
                card_def: t.token_def,
                name,
                owner: t.controller,
                controller: t.controller,
                zone: Zone::Battlefield,
                tapped: false,
                // A token entering the battlefield is exactly as summoning-
                // sick as any other permanent that just entered (see
                // `commit_zone_change`'s identical `= true` a few lines
                // down for the ordinary cast/move path) -- this was
                // hardcoded `false` and never flipped, the one "enters
                // battlefield" path that skipped setting it. Found via the
                // branch-differential pilot (Sol #89/#91): a Blood Token's
                // controlled-since-turn-start flag disagreed with the
                // reference engine's `wasControlledFromStartOfControllerTurn()`
                // immediately after Voldaren Epicure's ETB created it.
                summoning_sick: true,
                damage: 0,
                counters: Default::default(),
                attachments: Vec::new(),
                v4: {
                    let mut v4 = crate::state::ObjectStateV4::from_card_def(t.token_def);
                    // Creation itself makes this object a token, including
                    // copies whose printed definition is an ordinary card.
                    v4.is_token = true;
                    // Copy exceptions are copiable characteristics and must be
                    // present before any entry characteristic is consulted.
                    v4.copied_end_step_sacrifices_v1 = t
                        .initialization_v48
                        .as_ref()
                        .map_or(0, |init| init.copied_end_step_sacrifices_v1);
                    v4.entered_battlefield_turn = Some(state.turn_key_v1());
                    v4.entered_battlefield_this_turn = cfg!(feature = "standard-magezero-fixtures");
                    v4
                },
                spell_copy_origin: None,
                plotted_turn: None,
                zone_change_count: 0,
            });
            if let Some(face_index) = t.entry_face_index {
                let face = token_def
                    .transform_face
                    .as_ref()
                    .filter(|_| face_index == 1)
                    .expect("copied transformed token requires a defined back face");
                let live = state.objects.get_mut(object);
                live.v4.is_token = true;
                live.v4.face_index = face_index;
                live.v4.effective_color_mask = crate::card_def::mana_colors_mask(face.colors);
                live.v4.effective_subtype_ids =
                    face.subtypes.iter().map(|s| s.stable_id()).collect();
                live.v4.effective_subtype_ids.sort_unstable();
                live.v4.effective_subtype_ids.dedup();
                live.name = face.name.to_string();
            }
            if cfg!(feature = "limited-fdn-fixtures") {
                let timestamp = crate::engine::next_timestamp(state);
                state.objects.get_mut(object).v4.layer_timestamp = Some(timestamp);
            }
            state.players[t.controller.index()].battlefield.push(object);
            let enters_tapped = t.entry_tapped
                || permanent_enters_battlefield_tapped(state, object, t.controller)
                || authority_observers.map_or_else(
                    || {
                        crate::authority_of_the_consuls_v1::enters_tapped(
                            state,
                            object,
                            t.controller,
                        )
                    },
                    |observers| {
                        crate::authority_of_the_consuls_v1::enters_tapped_from_observers(
                            state,
                            object,
                            t.controller,
                            observers,
                        )
                    },
                );
            state.objects.get_mut(object).tapped = enters_tapped;
            if let Some(init) = &t.initialization_v48 {
                if let Some(host) = init.attached_to {
                    // Legality and incarnation were checked for the whole roster
                    // before any entry. This is untargeted attachment on entry.
                    if state
                        .attach_object_exact(object, 0, host.object, host.zone_change_count)
                        .is_err()
                    {
                        state.engine.halted = Some((
                            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                            object,
                        ));
                        return None;
                    }
                }
                if init.haste_until_end_of_turn {
                    let timestamp = crate::engine::next_timestamp(state);
                    state.engine.until_end_of_turn.push(
                        crate::engine::UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
                            object_id: object,
                            object_zone_change_count: 0,
                            layer: crate::engine::Layers::ABILITY_ADDING,
                            timestamp,
                            duration: crate::engine::EffectDuration::EndOfTurn,
                            keywords: crate::card_def::Keywords::HASTE,
                        },
                    );
                }
                if let Some(source) = init.sacrifice_at_end_step {
                    let binding = crate::effect::EffectObjectBinding {
                        object,
                        expected_zone: Zone::Battlefield,
                        expected_zone_change_count: 0,
                    };
                    crate::standard_cards_v1::register_reflection_delayed_sacrifice(
                        state, binding, source,
                    );
                }
                if init.attacking
                    && state.engine.combat.attackers_declared
                    && state.active_player == t.controller
                    && matches!(
                        state.step,
                        crate::state::Step::DeclareAttackers
                            | crate::state::Step::DeclareBlockers
                            | crate::state::Step::CombatDamage
                    )
                {
                    state.engine.combat.attackers.push(object);
                }
            }
            initialize_entry_counters(state, object, false, counter_input.as_ref());
            if season_entry_active {
                crate::standard_cards_v1::initialize_entry_counters(
                    state,
                    object,
                    counter_input.as_ref(),
                );
            }
            if initialize_entry_lore_v48(state, object, counter_input.as_ref()).is_err() {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    object,
                ));
                return None;
            }
            if let Some(init) = &t.initialization_v48 {
                if init.plus_one_counters > 0 {
                    let amount = crate::doubling_season_v48::entry_amount(
                        state,
                        counter_input.as_ref(),
                        t.controller,
                        init.plus_one_counters,
                    )
                    .expect("initialized token counter roster preflighted");
                    state.objects.get_mut(object).counters.plus1_plus1 += amount;
                }
            }
            if crate::ajani_full_v42::selected_definition(t.token_def) {
                crate::planeswalker_v1::after_zone_change(state, object, counter_input.as_ref());
                crate::ajani_full_v42::lifecycle::initialize_entry(state, object);
            }
            CommittedEvent::CreateToken {
                object,
                token_def: t.token_def,
                controller: t.controller,
                replacement_input_v48: crate::doubling_season_v48::TokenInputRefV48(
                    token_input_index,
                ),
            }
        }
        ProposedEvent::Transform(t) if t.face_index == 0 => {
            // Back to the front face (a nightbound permanent as it becomes
            // day): the definition's own characteristics.
            let obj = state.objects.get_mut(t.object);
            let def = &crate::card_def::CARD_DEFS[obj.card_def as usize];
            assert!(
                obj.v4.face_index == 1 && def.transform_face.is_some(),
                "transform_in_place to the front face needs a transformed permanent"
            );
            obj.v4.face_index = 0;
            obj.v4.effective_color_mask = crate::card_def::mana_colors_mask(def.colors);
            obj.v4.effective_subtype_ids = def
                .subtypes
                .iter()
                .map(|subtype| subtype.stable_id())
                .collect();
            obj.v4.effective_subtype_ids.sort_unstable();
            obj.v4.effective_subtype_ids.dedup();
            obj.name = def.object_name.to_string();
            CommittedEvent::Transformed {
                object: t.object,
                face_index: 0,
            }
        }
        ProposedEvent::Transform(t) => {
            let obj = state.objects.get_mut(t.object);
            let def = &crate::card_def::CARD_DEFS[obj.card_def as usize];
            let Some(face) = (t.face_index == 1)
                .then_some(def.transform_face.as_ref())
                .flatten()
            else {
                panic!("transform_in_place requested an undefined face");
            };
            obj.v4.face_index = t.face_index;
            obj.v4.effective_color_mask = crate::card_def::mana_colors_mask(face.colors);
            obj.v4.effective_subtype_ids = face
                .subtypes
                .iter()
                .map(|subtype| subtype.stable_id())
                .collect();
            obj.v4.effective_subtype_ids.sort_unstable();
            obj.v4.effective_subtype_ids.dedup();
            obj.name = face.name.to_string();
            CommittedEvent::Transformed {
                object: t.object,
                face_index: t.face_index,
            }
        }
    };
    Some(MaterializedEventV1 {
        committed,
        alesha_power_departure,
        niv_source_departure,
    })
}
struct MaterializedEventV1 {
    committed: CommittedEvent,
    alesha_power_departure: Option<(ObjectId, u16, u32, i32)>,
    niv_source_departure: Option<crate::state::AbilitySourceContractV4>,
}

/// Runs the replace/prevent pass independently on every event in `events`
/// (each is evaluated against the currently-active replacements as if it
/// were the only proposal in flight -- true simultaneity: none of them can
/// see or react to one another), then commits every survivor back-to-back
/// with no SBA/trigger check interleaved. Used for combat damage (510.2:
/// all of it happens at once); the caller is responsible for running SBAs
/// / trigger collection exactly once after the whole batch (see
/// `engine::deal_combat_damage`), not per event.
pub fn propose_and_commit_batch(state: &mut GameState, events: Vec<ProposedEvent>) {
    if events.iter().any(unsupported_partial_token_v1) {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ObjectId(0),
        ));
        return;
    }
    if crate::linked_aura_operation_v1::needs_event_owner(state)
        && crate::linked_aura_operation_v1::event(
            state,
            crate::linked_aura_operation_v1::OperationV1::Batch(events.clone()),
        )
    {
        return;
    }
    batch_without_linked_aura_v1(state, events);
}

pub(crate) fn batch_without_linked_aura_v1(state: &mut GameState, events: Vec<ProposedEvent>) {
    batch_with_linked_aura_attachments_v1(state, events, Vec::new());
}

/// Already authenticated untargeted attachment choices belong to this entry
/// batch. Install all attachments before publishing post-entry observers.
pub(crate) fn batch_with_linked_aura_attachments_v1(
    state: &mut GameState,
    events: Vec<ProposedEvent>,
    attachments: Vec<(
        crate::effect::EffectObjectBinding,
        crate::effect::EffectObjectBinding,
    )>,
) {
    batch_with_aura_token_hosts_v1(state, events, attachments, None, None, None, None);
}

/// The bound copy continuation supplies one untargeted host per actual expanded
/// Aura. Expansion still records the original single request and replacement
/// world; these entry choices never resubmit each member through replacement.
pub(crate) fn batch_with_copied_aura_hosts_v1(
    state: &mut GameState,
    event: ProposedEvent,
    hosts: Vec<crate::state::ObjectLinkV4>,
) {
    batch_with_aura_token_hosts_v1(
        state,
        vec![event],
        Vec::new(),
        Some(hosts),
        None,
        None,
        None,
    );
}
/// Consumes one private genuine finishing permit. It cannot be reconstructed
/// from serialized frames or reused for a second batch.
pub(crate) fn commit_goblin_excess_tokens_owned(
    state: &mut GameState,
    permit: crate::goblin_negotiation_v1::owned::TokenBatchPermitV1<'_>,
) -> Result<(), String> {
    let events = permit.events()?;
    if !permit.allows(state, &events) {
        return Err("excess token batch lost exact continuation owner".into());
    }
    batch_with_aura_token_hosts_v1(state, events, Vec::new(), None, Some(&permit), None, None);
    if state.engine.halted.is_some() {
        return Err("excess token batch halted".into());
    }
    Ok(())
}
fn batch_with_aura_token_hosts_v1(
    state: &mut GameState,
    events: Vec<ProposedEvent>,
    attachments: Vec<(
        crate::effect::EffectObjectBinding,
        crate::effect::EffectObjectBinding,
    )>,
    aura_hosts: Option<Vec<crate::state::ObjectLinkV4>>,
    excess_permit: Option<&crate::goblin_negotiation_v1::owned::TokenBatchPermitV1<'_>>,
    kykar_permit: Option<&crate::effect::kykar_v1::TokenPermitV1<'_, '_>>,
    spirit_copy_permit: Option<&crate::effect::SpiritCopyPermitV1<'_, '_>>,
) {
    if events.iter().any(unsupported_partial_token_v1)
        && kykar_permit.is_none_or(|p| !p.allows(state, &events))
        && spirit_copy_permit.is_none_or(|p| !p.allows(state, &events))
    {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ObjectId(0),
        ));
        return;
    }
    if excess_permit.is_some_and(|permit| !permit.allows(state, &events)) {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ObjectId(0),
        ));
        return;
    }
    let (mut events, token_requests) =
        match crate::doubling_season_v48::expand_tokens(state, events) {
            Ok(expanded) => expanded,
            Err(_) => {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ObjectId(0),
                ));
                return;
            }
        };
    if let Some(hosts) = aura_hosts {
        let valid = hosts.len() == events.len() && events.iter().all(|event| {
            matches!(event, ProposedEvent::CreateToken(token)
                if crate::card_def::CARD_DEFS[usize::from(token.token_def)].attachment.is_some()
                && token.initialization_v48.as_ref().is_some_and(|init| init.copied && init.attached_to.is_none()))
        });
        if !valid {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ObjectId(0),
            ));
            return;
        }
        for (event, host) in events.iter_mut().zip(hosts) {
            let ProposedEvent::CreateToken(token) = event else {
                unreachable!()
            };
            token.initialization_v48.as_mut().unwrap().attached_to = Some(host);
        }
    }
    if crate::doubling_season_v48::preflight_entry_roster(state, &events).is_err()
        || crate::doubling_season_v48::preflight_token_initialization(state, &events).is_err()
    {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ObjectId(0),
        ));
        return;
    }
    let token_input =
        match crate::doubling_season_v48::TokenInputV48::capture(state, &token_requests) {
            Ok(input) => input,
            Err(_) => {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ObjectId(0),
                ));
                return;
            }
        };
    // Validate the entire replacement roster before any shield or packet mutates.
    if let Some(source) = events.iter().find_map(|event| match event {
        ProposedEvent::Damage(damage)
            if crate::twinflame_tyrant_v1::validate_commuting_damage(state, damage).is_err() =>
        {
            Some(damage.source)
        }
        _ => None,
    }) {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            source,
        ));
        return;
    }
    let garruk_batch = crate::garruks_uprising_v1::EntryBatchV1::capture();
    let arahbo_batch = crate::arahbo_v1::EntryBatchV1::capture();
    let raid_history_start = state.engine.event_history.len();
    if excess_permit.is_none() && crate::effect::crystal_prevention_v1::validate(state).is_err() {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            crate::ids::ObjectId(0),
        ));
        return;
    }
    if let Some(source) = events.iter().find_map(|e| match e {
        ProposedEvent::Damage(d)
            if crate::effect::crystal_prevention_v1::needs_choice(state, d) =>
        {
            Some(d.source)
        }
        _ => None,
    }) {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            source,
        ));
        return;
    }
    let authority_batch = crate::authority_of_the_consuls_v1::EntryBatchV1::capture(state);
    if needs_final_zone_pt_plan_v1(state, &events) {
        let mut replaced = state.clone();
        let survivors = events
            .into_iter()
            .filter_map(|event| apply_replacements(&mut replaced, event))
            .collect::<Vec<_>>();
        if let Some((_, source)) = replaced.engine.halted {
            halt_zone_pt_v1(state, source);
            return;
        }
        match crate::linked_aura_operation_v1::prepare_entry_colors(&mut replaced, &survivors) {
            Ok(true) => {}
            Ok(false) => {
                *state = replaced;
                return;
            }
            Err(_) => {
                halt_zone_pt_v1(state, ObjectId(0));
                return;
            }
        }
        let inputs = survivors
            .iter()
            .map(|event| crate::doubling_season_v48::entry_input(&replaced, event))
            .collect::<Vec<_>>();
        let plan = match FinalZonePtPlanV1::capture(
            &replaced,
            &survivors,
            authority_batch
                .as_ref()
                .map(|batch| batch.observers.as_slice()),
            None,
            &inputs,
            &attachments,
        ) {
            Ok(plan) => plan,
            Err(source) => {
                halt_zone_pt_v1(state, source);
                return;
            }
        };
        *state = replaced;
        commit_batch_survivors(
            state,
            survivors,
            authority_batch,
            EntryBatchCommitContextV1 {
                garruk_batch,
                arahbo_batch,
                attachments,
                raid_history_start,
                token_input,
                token_requests,
            },
            Some(plan),
        );
        return;
    }
    let survivors: Vec<ProposedEvent> = events
        .into_iter()
        .filter_map(|e| apply_replacements(state, e))
        .collect();
    commit_batch_survivors(
        state,
        survivors,
        authority_batch,
        EntryBatchCommitContextV1 {
            garruk_batch,
            arahbo_batch,
            attachments,
            raid_history_start,
            token_input,
            token_requests,
        },
        None,
    );
}
/// The private owning driver has already finished every replacement choice.
/// It cannot submit a foreign root, owner, source or recipient packet.
pub(crate) fn commit_crystal_prepared_batch(
    state: &mut GameState,
    survivors: Vec<ProposedEvent>,
    capture: &crate::effect::crystal_prevention_v1::BatchCaptureV2,
    owner: &crate::effect::crystal_prevention_v1::OwnerV1,
) {
    if !owner.permits_batch(state, capture, &survivors) {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            capture.ctx.source,
        ));
        return;
    }
    let authority_batch = crate::authority_of_the_consuls_v1::EntryBatchV1::capture(state);
    let context = EntryBatchCommitContextV1 {
        garruk_batch: crate::garruks_uprising_v1::EntryBatchV1::capture(),
        arahbo_batch: crate::arahbo_v1::EntryBatchV1::capture(),
        attachments: Vec::new(),
        raid_history_start: state.engine.event_history.len(),
        token_input: None,
        token_requests: Vec::new(),
    };
    commit_batch_survivors(state, survivors, authority_batch, context, None);
}
pub(crate) fn commit_twinflame_prepared_combat(
    state: &mut GameState,
    survivors: Vec<ProposedEvent>,
    owner: &crate::twinflame_tyrant_v1::combat::OwnerV38,
) -> Result<(), String> {
    if !owner.permits(state, &survivors) {
        return Err("combat prepared commit lost private actual wave owner".into());
    }
    let authority = crate::authority_of_the_consuls_v1::EntryBatchV1::capture(state);
    let context = EntryBatchCommitContextV1 {
        garruk_batch: crate::garruks_uprising_v1::EntryBatchV1::capture(),
        arahbo_batch: crate::arahbo_v1::EntryBatchV1::capture(),
        attachments: Vec::new(),
        raid_history_start: state.engine.event_history.len(),
        token_input: None,
        token_requests: Vec::new(),
    };
    commit_batch_survivors(state, survivors, authority, context, None);
    if state.engine.halted.is_some() {
        return Err("actual combat prepared commit halted".into());
    }
    Ok(())
}
struct EntryBatchCommitContextV1 {
    garruk_batch: Option<crate::garruks_uprising_v1::EntryBatchV1>,
    arahbo_batch: Option<crate::arahbo_v1::EntryBatchV1>,
    attachments: Vec<(
        crate::effect::EffectObjectBinding,
        crate::effect::EffectObjectBinding,
    )>,
    raid_history_start: usize,
    token_input: Option<crate::doubling_season_v48::TokenInputV48>,
    token_requests: Vec<crate::doubling_season_v48::TokenRequestV48>,
}
fn commit_batch_survivors(
    state: &mut GameState,
    survivors: Vec<ProposedEvent>,
    mut authority_batch: Option<crate::authority_of_the_consuls_v1::EntryBatchV1>,
    context: EntryBatchCommitContextV1,
    prepared_zone_plan: Option<FinalZonePtPlanV1>,
) {
    if crate::doubling_season_v48::preflight_token_initialization(state, &survivors).is_err()
        || crate::doubling_season_v48::preflight_entry_roster(state, &survivors).is_err()
    {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ObjectId(0),
        ));
        return;
    }
    if prepared_zone_plan.is_none() {
        match crate::linked_aura_operation_v1::prepare_entry_colors(state, &survivors) {
            Ok(true) => {}
            Ok(false) => return,
            Err(_) => {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ObjectId(0),
                ));
                return;
            }
        }
    }
    let planned_counter_inputs = survivors
        .iter()
        .map(|event| crate::doubling_season_v48::entry_input(state, event))
        .collect::<Vec<_>>();
    let zone_plan = if let Some(plan) = prepared_zone_plan {
        Some(plan)
    } else if needs_final_zone_pt_plan_v1(state, &survivors) {
        match FinalZonePtPlanV1::capture(
            state,
            &survivors,
            authority_batch
                .as_ref()
                .map(|batch| batch.observers.as_slice()),
            None,
            &planned_counter_inputs,
            &context.attachments,
        ) {
            Ok(plan) => Some(plan),
            Err(source) => {
                halt_zone_pt_v1(state, source);
                return;
            }
        }
    } else {
        None
    };
    let conqueror_batch = crate::bloodthirsty_conqueror_v49::RecorderV49::capture(state);
    let EntryBatchCommitContextV1 {
        mut garruk_batch,
        mut arahbo_batch,
        attachments,
        raid_history_start,
        token_input,
        token_requests,
    } = context;
    let token_input_index =
        token_input.map(|input| crate::doubling_season_v48::record_token_input(state, input));
    let token_history_start = state.engine.event_history.len();
    #[cfg(feature = "standard-magezero-fixtures")]
    for event in &survivors {
        if let ProposedEvent::ZoneChange(change) = event {
            if state.objects.get(change.object).zone == Zone::Battlefield {
                crate::standard_statics_v1::record_creature_stats_lki(state, change.object);
            }
        }
    }
    crate::gorehorn_damage_v1::before_batch_departures(state, &survivors);
    crate::equipment_damage_source_v4::before_batch_departures(state, &survivors);
    crate::ashroot_animist_v1::before_batch_departures(state, &survivors);
    // Lifelink changes life at the same time as the damage. Capture every
    // source/controller before any member of the simultaneous batch can die.
    // CR 119.9: one source damaging several recipients simultaneously is
    // one life-gain event. Distinct sources remain distinct, in first-event
    // order, even when they share a controller.
    let mut lifelink_gains: Vec<(ObjectId, PlayerId, i32)> = Vec::new();
    for event in &survivors {
        let Some((player, amount)) = lifelink_gain_for(state, event) else {
            continue;
        };
        let ProposedEvent::Damage(damage) = event else {
            unreachable!("only damage causes lifelink");
        };
        if let Some((_, _, total)) = lifelink_gains
            .iter_mut()
            .find(|(source, controller, _)| *source == damage.source && *controller == player)
        {
            let Some(next) = total.checked_add(amount) else {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    damage.source,
                ));
                return;
            };
            *total = next;
        } else {
            lifelink_gains.push((damage.source, player, amount));
        }
    }
    let source_colors = crate::controlled_creature_grants_v1::enabled().then(|| {
        survivors
            .iter()
            .filter_map(|event| {
                crate::controlled_creature_grants_v1::capture_departure_color(state, event)
            })
            .collect::<Vec<_>>()
    });
    let mut death_captures = if crate::creature_death_capture_v1::enabled() {
        let captured = survivors
            .iter()
            .map(|event| crate::creature_death_capture_v1::capture(state, event))
            .collect::<Result<Vec<_>, _>>();
        match captured {
            Ok(captures) => Some(captures.into_iter()),
            Err(_) => {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ObjectId(0),
                ));
                return;
            }
        }
    } else {
        None
    };
    let greatwurm_wave = match crate::effect::greatwurm_v1::capture_wave(state, &survivors) {
        Ok(wave) => wave,
        Err(_) => {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ObjectId(0),
            ));
            return;
        }
    };
    let greatwurm_history_start = state.engine.event_history.len();
    let mut counter_inputs = planned_counter_inputs.into_iter();
    crate::linked_aura_operation_v1::begin_batch(state);
    if cfg!(feature = "limited-fdn-fixtures") {
        let ability_removals = survivors
            .iter()
            .map(|event| match event {
                ProposedEvent::ZoneChange(change) => {
                    !crate::continuous_characteristics_v1::printed_abilities_active(
                        state,
                        change.object,
                    )
                }
                _ => false,
            })
            .collect::<Vec<_>>();
        for (event, removed) in survivors.into_iter().zip(ability_removals) {
            let counter_input = counter_inputs
                .next()
                .expect("one entry counter input per batch member");
            let captures = death_captures
                .as_mut()
                .map(|captures| captures.next().expect("one capture per batch member"));
            let frozen_color = source_colors.as_ref().and_then(|colors| colors.iter().find(|snapshot|
                matches!(&event, ProposedEvent::ZoneChange(change) if change.object == snapshot.source.source)).cloned());
            commit_with_entry_batch_and_source_color(
                state,
                event,
                Some(removed),
                captures,
                authority_batch.as_mut(),
                (garruk_batch.as_mut(), arahbo_batch.as_mut()),
                (
                    (None, true),
                    frozen_color,
                    counter_input,
                    token_input_index,
                    zone_plan.as_ref(),
                ),
            );
        }
    } else {
        for event in survivors {
            let counter_input = counter_inputs
                .next()
                .expect("one entry counter input per batch member");
            let captures = death_captures
                .as_mut()
                .map(|captures| captures.next().expect("one capture per batch member"));
            let frozen_color = source_colors.as_ref().and_then(|colors| colors.iter().find(|snapshot|
                matches!(&event, ProposedEvent::ZoneChange(change) if change.object == snapshot.source.source)).cloned());
            commit_with_entry_batch_and_source_color(
                state,
                event,
                None,
                captures,
                authority_batch.as_mut(),
                (garruk_batch.as_mut(), arahbo_batch.as_mut()),
                (
                    (None, true),
                    frozen_color,
                    counter_input,
                    token_input_index,
                    zone_plan.as_ref(),
                ),
            );
        }
    }

    if greatwurm_wave
        .finish(state, greatwurm_history_start)
        .is_err()
    {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ObjectId(0),
        ));
        return;
    }
    for (aura, host) in attachments {
        if state.objects.get(aura.object).zone == Zone::Battlefield {
            let generation = state.objects.get(aura.object).zone_change_count;
            if state
                .attach_object_exact(
                    aura.object,
                    generation,
                    host.object,
                    host.expected_zone_change_count,
                )
                .is_err()
            {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    aura.object,
                ));
                return;
            }
        }
    }
    if let Some(batch) = authority_batch {
        batch.finish(state);
    }
    if let Some(batch) = garruk_batch {
        batch.finish(state);
    }

    crate::raid_entry_custody_v1::capture_batch_entries(state, raid_history_start);
    if let Some(batch) = arahbo_batch {
        batch.finish(state);
    }
    for (_, player, amount) in lifelink_gains {
        commit(state, ProposedEvent::life_gain(player, amount));
    }
    if state.engine.halted.is_none() {
        if let Some(input) = token_input_index {
            if crate::doubling_season_v48::record_token_creation(
                state,
                token_history_start,
                input,
                token_requests,
            )
            .is_err()
            {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ObjectId(0),
                ));
                return;
            }
        }
    }
    if let Some(batch) = conqueror_batch {
        batch.finish(state);
    }
    crate::linked_aura_operation_v1::end_batch(state);
}

/// The payment caller has already validated one complete sacrifice group.
/// Freeze only new death observers; retain the existing per-member sacrifice,
/// replacement, zone-change and unrelated-event order exactly.
pub(crate) fn commit_sacrifice_death_group_v1(state: &mut GameState, chosen: &[ObjectId]) {
    let planned_events = chosen
        .iter()
        .copied()
        .map(|id| ProposedEvent::zone_change(id, Zone::Graveyard))
        .collect::<Vec<_>>();
    let zone_plan = match direct_zone_pt_plan_v1(state, &planned_events) {
        Ok(plan) => plan,
        Err(source) => {
            halt_zone_pt_v1(state, source);
            return;
        }
    };
    crate::gorehorn_damage_v1::before_batch_departures(
        state,
        &chosen
            .iter()
            .map(|id| ProposedEvent::zone_change(*id, Zone::Graveyard))
            .collect::<Vec<_>>(),
    );

    crate::equipment_damage_source_v4::before_batch_departures(
        state,
        &chosen
            .iter()
            .map(|id| ProposedEvent::zone_change(*id, Zone::Graveyard))
            .collect::<Vec<_>>(),
    );
    let mut source_colors = crate::controlled_creature_grants_v1::enabled().then(|| {
        chosen
            .iter()
            .map(|id| {
                crate::controlled_creature_grants_v1::capture_departure_color(
                    state,
                    &ProposedEvent::zone_change(*id, Zone::Graveyard),
                )
            })
            .collect::<Vec<_>>()
            .into_iter()
    });
    let captures = chosen
        .iter()
        .map(|id| {
            crate::creature_death_capture_v1::capture(
                state,
                &ProposedEvent::zone_change(*id, Zone::Graveyard),
            )
        })
        .collect::<Result<Vec<_>, _>>();
    let captures = match captures {
        Ok(captures) => captures,
        Err(_) => {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ObjectId(0),
            ));
            return;
        }
    };
    for (&id, captures) in chosen.iter().zip(captures) {
        let frozen_color = source_colors.as_mut().and_then(Iterator::next).flatten();
        log_sacrifice(state, id);
        if let Some(event) =
            apply_replacements(state, ProposedEvent::zone_change(id, Zone::Graveyard))
        {
            commit_with_entry_batch_and_source_color(
                state,
                event,
                None,
                Some(captures),
                None,
                (None, None),
                ((None, false), frozen_color, None, None, zone_plan.as_ref()),
            );
        }
    }
}
/// One simultaneous SBA death group. Preserve the existing direct-commit
/// semantics while freezing death observers before any member leaves.
pub(crate) fn commit_sba_death_batch_v1(state: &mut GameState, dying: Vec<ObjectId>) {
    let planned_events = dying
        .iter()
        .copied()
        .map(|id| ProposedEvent::zone_change(id, Zone::Graveyard))
        .collect::<Vec<_>>();
    let zone_plan = match direct_zone_pt_plan_v1(state, &planned_events) {
        Ok(plan) => plan,
        Err(source) => {
            halt_zone_pt_v1(state, source);
            return;
        }
    };
    crate::linked_aura_operation_v1::begin_batch(state);
    crate::gorehorn_damage_v1::before_batch_departures(
        state,
        &dying
            .iter()
            .map(|id| ProposedEvent::zone_change(*id, Zone::Graveyard))
            .collect::<Vec<_>>(),
    );

    crate::equipment_damage_source_v4::before_batch_departures(
        state,
        &dying
            .iter()
            .map(|id| ProposedEvent::zone_change(*id, Zone::Graveyard))
            .collect::<Vec<_>>(),
    );
    let mut source_colors = crate::controlled_creature_grants_v1::enabled().then(|| {
        dying
            .iter()
            .map(|id| {
                crate::controlled_creature_grants_v1::capture_departure_color(
                    state,
                    &ProposedEvent::zone_change(*id, Zone::Graveyard),
                )
            })
            .collect::<Vec<_>>()
            .into_iter()
    });
    if !crate::creature_death_capture_v1::enabled() {
        for id in dying {
            let frozen_color = source_colors.as_mut().and_then(Iterator::next).flatten();
            if frozen_color.is_some() || zone_plan.is_some() {
                commit_with_entry_batch_and_source_color(
                    state,
                    ProposedEvent::zone_change(id, Zone::Graveyard),
                    None,
                    None,
                    None,
                    (None, None),
                    ((None, false), frozen_color, None, None, zone_plan.as_ref()),
                );
            } else {
                commit(state, ProposedEvent::zone_change(id, Zone::Graveyard));
            }
        }
        crate::linked_aura_operation_v1::end_batch(state);
        return;
    }
    let events = dying
        .into_iter()
        .map(|id| ProposedEvent::zone_change(id, Zone::Graveyard))
        .collect::<Vec<_>>();
    let captures = events
        .iter()
        .map(|event| crate::creature_death_capture_v1::capture(state, event))
        .collect::<Result<Vec<_>, _>>();
    let captures = match captures {
        Ok(captures) => captures,
        Err(_) => {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ObjectId(0),
            ));
            return;
        }
    };
    for (event, captures) in events.into_iter().zip(captures) {
        let frozen_color = source_colors.as_mut().and_then(Iterator::next).flatten();
        commit_with_entry_batch_and_source_color(
            state,
            event,
            None,
            Some(captures),
            None,
            (None, None),
            ((None, false), frozen_color, None, None, zone_plan.as_ref()),
        );
    }
    crate::linked_aura_operation_v1::end_batch(state);
}

/// Logs a `SpellCast` marker with no accompanying state mutation (casting
/// itself -- moving hand to stack -- is handled by
/// `engine::move_hand_to_stack`; this is purely a trigger-matching hook).
/// Not named `commit_*` and not routed through `propose_and_commit`
/// because there is no proposed/replaceable form of "a spell was cast" in
/// this increment's scope (countering a spell removes it from the stack
/// later, it doesn't replace the cast event itself).
pub fn log_spell_cast(state: &mut GameState, spell: ObjectId, controller: PlayerId) {
    let committed = CommittedEvent::SpellCast { spell, controller };
    state.engine.event_log.push(committed.clone());
    state.engine.event_history.push(committed);
}

/// Logs a crime committed by one completed targeting action.
#[cfg(feature = "standard-magezero-fixtures")]
pub fn log_crime(state: &mut GameState, player: PlayerId, targeting_stack_item: StackItemId) {
    let committed = CommittedEvent::CrimeCommitted {
        player,
        targeting_stack_item,
    };
    state.engine.event_log.push(committed.clone());
    state.engine.event_history.push(committed);
}

/// Logs one final targeting marker after the enclosing spell or ability has
/// completed announcement. Failed casts never reach this point.
pub fn log_targeted(
    state: &mut GameState,
    target: ObjectId,
    target_zone_change_count: u32,
    targeting_stack_item: StackItemId,
    targeting_controller: PlayerId,
) {
    let committed = CommittedEvent::Targeted {
        target,
        target_zone_change_count,
        targeting_stack_item,
        targeting_controller,
    };
    state.engine.event_log.push(committed.clone());
    state.engine.event_history.push(committed);
}

/// Records the rules action separately from its ensuing replaceable zone
/// change. Sacrifice triggers consume this marker instead of guessing from
/// an ordinary battlefield-to-graveyard move.
pub fn log_sacrifice(state: &mut GameState, object: ObjectId) {
    let live = state.objects.get(object);
    let committed = CommittedEvent::Sacrificed {
        object,
        controller_before: live.controller,
        effective_subtype_ids_before: crate::engine::effective_subtype_ids(state, object),
    };
    state.engine.event_log.push(committed.clone());
    state.engine.event_history.push(committed);
}

/// Records that one exact permanent incarnation dealt combat damage to a
/// player. The underlying damage event remains independently committed;
/// this marker exists only to distinguish combat from noncombat damage for
/// triggered-ability matching.
pub fn log_combat_damage_to_player(
    state: &mut GameState,
    source: ObjectId,
    source_zone_change_count: u32,
    player: PlayerId,
    amount: i32,
) {
    let committed = CommittedEvent::CombatDamageToPlayer {
        source,
        source_zone_change_count,
        player,
        amount,
    };
    state.engine.event_log.push(committed.clone());
    state.engine.event_history.push(committed);
}

/// Logs the nonreplaceable chapter marker immediately after a lore counter
/// is placed by the Saga rules action.
pub fn log_saga_chapter(state: &mut GameState, source: ObjectId, chapter: u8) {
    let object = state.objects.get(source);
    let committed = CommittedEvent::SagaChapter {
        source,
        source_zone_change_count: object.zone_change_count,
        controller: object.controller,
        chapter,
    };
    state.engine.event_log.push(committed.clone());
    state.engine.event_history.push(committed);
}

/// Logs the nonreplaceable marker for the beginning of `player`'s own
/// Upkeep step (Delver of Secrets and any future "at the beginning of ...
/// upkeep" trigger). 505.2: exactly one Upkeep step happens each turn,
/// belonging to the active player, so callers log this once per turn for
/// `player == state.active_player`.
pub fn log_upkeep_began(state: &mut GameState, player: PlayerId) {
    let committed = CommittedEvent::UpkeepBegan { player };
    state.engine.event_log.push(committed.clone());
    state.engine.event_history.push(committed);
}

pub fn log_initiative_trigger(
    state: &mut GameState,
    player: PlayerId,
    mut source: crate::state::AbilitySourceContractV4,
    kind: crate::state::InitiativeTriggerKindV1,
) -> Result<crate::state::InitiativeTriggerBindingV1, String> {
    let live = state
        .objects
        .try_get(source.source)
        .ok_or("Initiative designation source no longer exists")?;
    if live.card_def != source.card_def
        || live.owner != source.owner
        || live.zone_change_count < source.zone_change_count
        || (live.zone_change_count == source.zone_change_count && live.zone != source.zone)
        || crate::card_def::CARD_DEFS
            .get(source.card_def as usize)
            .is_none_or(|definition| definition.name != "Avenging Hunter")
    {
        return Err("Initiative designation source contract is malformed".to_string());
    }
    source.controller = player;
    let history_index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "Initiative event history exceeds u32".to_string())?;
    let binding = crate::state::InitiativeTriggerBindingV1 {
        history_index,
        player,
        source,
        kind,
    };
    let committed = CommittedEvent::InitiativeTrigger { binding };
    state.engine.event_log.push(committed.clone());
    state.engine.event_history.push(committed);
    Ok(binding)
}

/// Logs one engine-owned monarch end-step draw trigger (306.3). Unlike
/// `log_initiative_trigger`, `source` is not validated against one fixed
/// card name: Azure Fleet Admiral's ETB grant and a combat-damage transfer
/// (see `engine::deal_combat_damage`) both freeze whichever object caused
/// the transfer into `EngineState::monarch_source`, and either is a valid
/// source here as long as it is still a real object incarnation.
pub fn log_monarch_trigger(
    state: &mut GameState,
    player: PlayerId,
    mut source: crate::state::AbilitySourceContractV4,
) -> Result<crate::state::MonarchTriggerBindingV1, String> {
    let live = state
        .objects
        .try_get(source.source)
        .ok_or("Monarch designation source no longer exists")?;
    if live.card_def != source.card_def
        || live.owner != source.owner
        || live.zone_change_count < source.zone_change_count
        || (live.zone_change_count == source.zone_change_count && live.zone != source.zone)
    {
        return Err("Monarch designation source contract is malformed".to_string());
    }
    source.controller = player;
    let history_index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "Monarch event history exceeds u32".to_string())?;
    let binding = crate::state::MonarchTriggerBindingV1 {
        history_index,
        player,
        source,
    };
    let committed = CommittedEvent::MonarchTrigger { binding };
    state.engine.event_log.push(committed.clone());
    state.engine.event_history.push(committed);
    Ok(binding)
}

fn permanent_enters_battlefield_tapped(
    state: &GameState,
    object: ObjectId,
    controller: PlayerId,
) -> bool {
    let def = &crate::card_def::CARD_DEFS[state.objects.get(object).card_def as usize];
    if def.enters_battlefield_tapped {
        return true;
    }
    if let Some(rule) = def.enters_tapped_unless_controller {
        use crate::card_def::EntersTappedUnlessControllerDef;
        let other_lands = || {
            state.players[controller.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|&candidate| {
                    candidate != object
                        && state.objects.get(candidate).zone == Zone::Battlefield
                        && crate::engine::object_has_type(
                            state,
                            candidate,
                            crate::card_def::CardType::Land,
                        )
                })
                .count()
        };
        return match rule {
            EntersTappedUnlessControllerDef::ControlsAtMostOtherLands(most) => {
                other_lands() > usize::from(most)
            }
            EntersTappedUnlessControllerDef::ControlsAtLeastOtherLands(least) => {
                other_lands() < usize::from(least)
            }
            EntersTappedUnlessControllerDef::WithinOwnFirstTurns(turns) => {
                !(state.active_player == controller
                    && state.own_turn_count_v1(controller) <= u32::from(turns))
            }
        };
    }
    let Some(rule) = def.enters_battlefield_tapped_unless else {
        return false;
    };
    let controlled_other_count = state.players[controller.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|candidate| *candidate != object)
        .filter(|candidate| {
            let live = state.objects.get(*candidate);
            live.zone == Zone::Battlefield
                && live.controller == controller
                && crate::engine::has_effective_subtype(
                    state,
                    *candidate,
                    rule.controller_controls_other_subtype,
                )
        })
        .count();
    controlled_other_count < usize::from(rule.minimum_count)
}

/// Zone bookkeeping shared by every `MoveObject` effect leaf. "Hand ->
/// Stack" (casting) is deliberately not reachable here: putting a spell on
/// the stack is an engine action (see `engine::begin_cast`), never
/// something a card's own effect program does.
#[allow(clippy::too_many_arguments)]
fn materialize_zone_change_v1(
    state: &mut GameState,
    id: ObjectId,
    to_zone: Zone,
    library_placement: LibraryPlacement,
    preserve_known_identity: bool,
    library_insert_visibility: LibraryInsertVisibility,
    force_battlefield_tapped: bool,
    battlefield_face_index: Option<u8>,
    battlefield_controller: Option<PlayerId>,
    entry_context: (
        Option<&[crate::state::AbilitySourceContractV4]>,
        Option<crate::garruks_uprising_v1::EntryCharacteristicsV1>,
        Option<&crate::doubling_season_v48::ReplacementInputV48>,
        Option<i32>,
    ),
) {
    let (authority_observers, entering_characteristics, counter_input, requested_revival) =
        entry_context;
    let entering_face_down = entering_characteristics.and_then(|entry| entry.face_down);
    #[cfg(feature = "standard-magezero-fixtures")]
    let battlefield_face_index = battlefield_face_index.or_else(|| {
        (to_zone == Zone::Battlefield
            && crate::standard_keywords_v1::enters_transformed_at_night(
                state,
                state.objects.get(id).card_def,
            ))
        .then_some(1)
    });
    let owner = state.objects.get(id).owner;
    let from_zone = state.objects.get(id).zone;
    let nine_lives_cast_entry = to_zone == Zone::Battlefield
        && from_zone == Zone::Stack
        && crate::card_def::CARD_DEFS[usize::from(state.objects.get(id).card_def)].name
            == "Nine-Lives Familiar"
        && state.objects.get(id).spell_copy_origin.is_none()
        && !state.objects.get(id).v4.is_token
        && state
            .objects
            .get(id)
            .v4
            .spell_cast_origin
            .is_some_and(|origin| origin.finalized_method.is_some());
    // A resolving permanent enters under its spell controller (608.3),
    // including a spell cast from another player's exile through Etali.
    let battlefield_controller = battlefield_controller.or_else(|| {
        (from_zone == Zone::Stack && to_zone == Zone::Battlefield)
            .then_some(state.objects.get(id).controller)
    });
    let informed_observer_mask =
        if preserve_known_identity && from_zone == Zone::Library && to_zone == Zone::Hand {
            let position = state.players[owner.index()]
                .library
                .iter()
                .position(|&candidate| candidate == id);
            let generation = state.objects.get(id).zone_change_count;
            position.map_or(0, |position| {
                [PlayerId::P0, PlayerId::P1]
                    .into_iter()
                    .filter(|&observer| {
                        state
                            .known_library_cards(observer, owner)
                            .iter()
                            .any(|entry| {
                                entry.position as usize == position
                                    && entry.object == id
                                    && entry.zone_change_count == generation
                            })
                    })
                    .fold(0_u8, |mask, observer| mask | (1 << observer.index()))
            })
        } else {
            0
        };

    remove_from_zone(state, owner, id, from_zone);
    state.forget_hand_object(id);
    state.clear_object_relations(id);
    if from_zone == Zone::Battlefield {
        // 506.4: leaving the battlefield removes it from combat.
        state.engine.combat.remove_from_combat(id);
    }

    match to_zone {
        Zone::Library => {
            let library_len = state.players[owner.index()].library.len();
            let position = match library_placement {
                LibraryPlacement::Top => 0,
                LibraryPlacement::SecondFromTop => 1.min(library_len),
                LibraryPlacement::Bottom => library_len,
                LibraryPlacement::ThirdFromTop => 2.min(library_len),
            };
            // The insertion position is public even when the inserted
            // identity is not. Preserve every still-valid older fact by
            // shifting it deeper; visibility is installed only after the
            // inserted object's new incarnation exists below.
            state.note_library_insertion(owner, position);
            state.players[owner.index()].library.insert(position, id);
        }
        Zone::Hand => state.players[owner.index()].hand.push(id),
        Zone::Battlefield => state.players[battlefield_controller.unwrap_or(owner).index()]
            .battlefield
            .push(id),
        Zone::Graveyard => state.players[owner.index()].graveyard.push(id),
        Zone::Exile => state.exile.push(id),
        Zone::Command => state.command.push(id),
        Zone::Stack => panic!("MoveObject to Stack is an engine action, not an effect leaf"),
    }

    let turn = state.turn_key_v1();
    let entry_added_subtype = (to_zone == Zone::Battlefield)
        .then(|| entering_characteristics.and_then(|entry| entry.added_subtype_v1))
        .flatten()
        .map(|subtype| (subtype, crate::engine::next_timestamp(state)));
    let entry_flying_grant_timestamp = (to_zone == Zone::Battlefield
        && entering_characteristics.is_some_and(|entry| entry.grant_flying_v1))
    .then(|| crate::engine::next_timestamp(state));
    let enters_battlefield_tapped = to_zone == Zone::Battlefield
        && (force_battlefield_tapped
            || (entering_face_down.is_none()
                && permanent_enters_battlefield_tapped(state, id, owner)));
    {
        let obj = state.objects.get_mut(id);
        obj.zone = to_zone;
        // A zone change creates a new object with no carried-over control
        // effect. A resolving permanent or an explicit controller override
        // chooses the battlefield controller; other destinations use the owner.
        obj.controller = if to_zone == Zone::Battlefield {
            battlefield_controller.unwrap_or(owner)
        } else {
            owner
        };
        // Plot is provenance of one exact exile incarnation, not a durable
        // property of the physical card. `engine::plot_spell` re-stamps the
        // newly-created Exile incarnation immediately after this move; every
        // other ordinary zone change must clear the old marker.
        obj.plotted_turn = None;
        // CR 400.7's zone-change identity: bumped on *every* zone change,
        // regardless of which zones, so `engine::PlayPermission::
        // zone_change_generation` can tell "still sitting where it was granted"
        // apart from "moved since, for any reason" without needing a
        // zone-specific special case.
        obj.zone_change_count += 1;
        if let Some(uses) = &mut state.trigger_uses_v1 {
            uses.retain(|entry| entry.source.object != id);
            if uses.is_empty() {
                state.trigger_uses_v1 = None;
            }
        }
        obj.v4.reset_for_zone_change(obj.card_def, to_zone, turn);
        // CR 611.2e: this resolving entry effect belongs to the new incarnation
        // before entry statics, counter replacements, and committed observers.
        obj.v4.added_subtype_v1 = entry_added_subtype;
        if let Some(timestamp) = entry_flying_grant_timestamp {
            obj.v4
                .creature_upgrade
                .get_or_insert_with(Default::default)
                .keyword_grants
                .push((crate::card_def::Keywords::FLYING, timestamp));
        }
        // 707.10f: a resolving permanent spell copy becomes a token. This
        // is a zone entry, not token creation, so no creation replacement applies.
        if from_zone == Zone::Stack
            && to_zone == Zone::Battlefield
            && obj.spell_copy_origin.is_some()
        {
            obj.v4.is_token = true;
        }
        if to_zone == Zone::Battlefield {
            if let Some(face_down) = entering_face_down {
                obj.v4.face_down_v1 = Some(face_down);
                obj.v4.ward_generic = 2;
            }
        }
        obj.name = crate::card_def::CARD_DEFS[obj.card_def as usize]
            .object_name
            .to_string();
        obj.damage = 0;
        obj.counters = Default::default();
        obj.attachments.clear();
        if to_zone == Zone::Battlefield {
            if let Some(face_index) = battlefield_face_index {
                let def = &crate::card_def::CARD_DEFS[obj.card_def as usize];
                let Some(face) = (face_index == 1)
                    .then_some(def.transform_face.as_ref())
                    .flatten()
                else {
                    panic!("transformed battlefield return requested an undefined face");
                };
                obj.v4.face_index = face_index;
                obj.v4.effective_color_mask = crate::card_def::mana_colors_mask(face.colors);
                obj.v4.effective_subtype_ids = face
                    .subtypes
                    .iter()
                    .map(|subtype| subtype.stable_id())
                    .collect();
                obj.v4.effective_subtype_ids.sort_unstable();
                obj.v4.effective_subtype_ids.dedup();
                obj.name = face.name.to_string();
            }
            obj.tapped = enters_battlefield_tapped;
            obj.summoning_sick = true;
        } else {
            obj.tapped = false;
            obj.summoning_sick = false;
        }
    }
    if to_zone == Zone::Battlefield {
        if let Some(entry) = entering_characteristics {
            if let Some(host) = entry.attached_to {
                if cfg!(feature = "limited-fdn-fixtures") {
                    let timestamp = crate::engine::next_timestamp(state);
                    state.objects.get_mut(id).v4.layer_timestamp = Some(timestamp);
                }
                if entry.bestowed {
                    state.objects.get_mut(id).v4.effective_subtype_ids =
                        vec![crate::card_def::Subtype::Aura.stable_id()];
                }
                let generation = state.objects.get(id).zone_change_count;
                if state
                    .attach_object_exact(id, generation, host.object, host.zone_change_count)
                    .is_err()
                {
                    state.engine.halted = Some((
                        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                        id,
                    ));
                    return;
                }
            }
        }
        // Evaluate this new static rule with the actual entering controller
        // and initialized battlefield characteristics, preserving the earlier
        // legacy entry checks and their order.
        let controller = state.objects.get(id).controller;
        if authority_observers.map_or_else(
            || crate::authority_of_the_consuls_v1::enters_tapped(state, id, controller),
            |observers| {
                crate::authority_of_the_consuls_v1::enters_tapped_from_observers(
                    state, id, controller, observers,
                )
            },
        ) {
            state.objects.get_mut(id).tapped = true;
        }
        if cfg!(feature = "limited-fdn-fixtures")
            && entering_characteristics.is_none_or(|entry| entry.attached_to.is_none())
        {
            let timestamp = crate::engine::next_timestamp(state);
            state.objects.get_mut(id).v4.layer_timestamp = Some(timestamp);
        }
        let kicked = from_zone == Zone::Stack && state.engine.pending_kicked_source == Some(id);
        if entering_face_down.is_none() {
            initialize_entry_counters(state, id, kicked, counter_input);
        }
        if to_zone == Zone::Battlefield && (nine_lives_cast_entry || requested_revival.is_some()) {
            if nine_lives_cast_entry && requested_revival.is_some() {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    id,
                ));
                return;
            }
            let base = requested_revival.unwrap_or(8);
            let controller = state.objects.get(id).controller;
            match crate::doubling_season_v48::entry_amount(state, counter_input, controller, base) {
                Ok(revival) if revival >= 0 => state.objects.get_mut(id).counters.revival = revival,
                _ => {
                    state.engine.halted = Some((
                        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                        id,
                    ));
                    return;
                }
            }
        }
        if initialize_entry_lore_v48(state, id, counter_input).is_err() {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                id,
            ));
            return;
        }
        if crate::doubling_season_v48::requires_successor(state)
            || counter_input.is_some_and(|input| !input.seasons.is_empty())
        {
            crate::standard_cards_v1::initialize_entry_counters(state, id, counter_input);
        }
        if let Some(stun) = entering_characteristics.and_then(|entry| entry.stun_counters_v48) {
            state.objects.get_mut(id).counters.stun = stun;
        }
        if let Some(count) = entering_characteristics.and_then(|entry| entry.time_counters) {
            let controller = state.objects.get(id).controller;
            let placed = crate::doubling_season_v48::entry_amount(
                state,
                counter_input,
                controller,
                i32::from(count),
            )
            .and_then(|n| u8::try_from(n).map_err(|_| "entry time counter overflow".into()));
            match placed {
                Ok(placed) => state.objects.get_mut(id).v4.time_counters_v1 = placed,
                Err(_) => {
                    state.engine.halted = Some((
                        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                        id,
                    ));
                    return;
                }
            }
        }
        if let Some(count) = entering_characteristics.and_then(|entry| entry.x_counters) {
            let controller = state.objects.get(id).controller;
            let count = match crate::doubling_season_v48::entry_amount(
                state,
                counter_input,
                controller,
                count,
            ) {
                Ok(count) => count,
                Err(_) => {
                    state.engine.halted = Some((
                        crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                        id,
                    ));
                    return;
                }
            };
            let total = if crate::doubling_season_v48::requires_successor(state)
                || counter_input.is_some_and(|input| !input.seasons.is_empty())
            {
                match state
                    .objects
                    .get(id)
                    .counters
                    .plus1_plus1
                    .checked_add(count)
                {
                    Some(total) => total,
                    None => {
                        state.engine.halted = Some((
                            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                            id,
                        ));
                        return;
                    }
                }
            } else {
                count
            };
            state.objects.get_mut(id).counters.plus1_plus1 = total;
        }
    }
    crate::planeswalker_v1::after_zone_change(state, id, counter_input);
    if to_zone == Zone::Battlefield {
        crate::ajani_full_v42::lifecycle::initialize_entry(state, id);
    }
    if to_zone == Zone::Library {
        let position = state.players[owner.index()]
            .library
            .iter()
            .position(|&candidate| candidate == id)
            .expect("just-inserted library object remains indexed");
        match library_insert_visibility {
            LibraryInsertVisibility::Hidden => {}
            LibraryInsertVisibility::Owner => {
                state.reveal_library_position(owner, owner, position);
            }
            LibraryInsertVisibility::Public => {
                for observer in [PlayerId::P0, PlayerId::P1] {
                    state.reveal_library_position(observer, owner, position);
                }
            }
        }
    }
    if to_zone == Zone::Hand && from_zone != Zone::Library {
        // Returning a publicly identified card to hand does not make that
        // identity secret again. The owner sees it through `own_hand`; the
        // other observer receives an incarnation-bound known-hand fact.
        for observer in [PlayerId::P0, PlayerId::P1] {
            state
                .reveal_hand_card(observer, owner, id)
                .expect("just moved this live public object into its owner's hand");
        }
    } else if to_zone == Zone::Hand && informed_observer_mask != 0 {
        // A public library reveal followed by a move to hand keeps the
        // revealed identity public. Install the fact only for observers who
        // actually knew this exact position/object/incarnation before the
        // move; the ordinary constructor above remains deliberately hidden.
        for observer in [PlayerId::P0, PlayerId::P1] {
            if informed_observer_mask & (1 << observer.index()) != 0 {
                state
                    .reveal_hand_card(observer, owner, id)
                    .expect("known library object just moved into its owner's hand");
            }
        }
    }
}

/// Refresh the exact battlefield generation's last-known power immediately
/// before it leaves. Paid-cost bindings, Alesha's waiting Raid, and other
/// authenticated consumers retain their own generation-scoped copy.
fn refresh_creature_power_lki(
    state: &mut GameState,
    id: ObjectId,
    from_zone: Zone,
    frozen_power: Option<i32>,
) {
    if from_zone != Zone::Battlefield {
        return;
    }
    let Some(object) = state.objects.try_get(id) else {
        return;
    };
    let generation = object.zone_change_count;
    let card_def = object.card_def;
    let power = frozen_power.unwrap_or_else(|| crate::engine::effective_power(state, id));
    let refresh = |references: &mut Vec<crate::state::PaidCostRefV4>| {
        for reference in references {
            if reference.object == id
                && reference.zone == Zone::Battlefield
                && reference.zone_change_count == generation
                && reference.power_lki.is_some()
            {
                reference.power_lki = Some(power);
            }
        }
    };
    let refresh_binding = |binding: &mut Option<crate::state::FinalizedCastBindingV1>| {
        let Some(reference) = binding
            .as_mut()
            .and_then(|binding| binding.chosen_creature_cost.as_mut())
        else {
            return;
        };
        if reference.object == id
            && reference.zone == Zone::Battlefield
            && reference.zone_change_count == generation
            && reference.power_lki.is_some()
        {
            reference.power_lki = Some(power);
        }
    };
    for item in &mut state.stack {
        refresh(&mut item.v4.paid_cost_refs);
        let source_contract = item.v4.ability_source_contract;
        if let Some(effect) = item.inline_effect.as_mut() {
            refresh_alesha_raid_power_lki(effect, source_contract, id, card_def, generation, power);
        }
        if let Some(contract) = item.v4.source_contract.as_mut() {
            refresh_binding(&mut contract.finalized_cast_binding);
        }
    }
    if let Some(pending) = state.engine.pending_effect.as_mut() {
        refresh(&mut pending.resolving_item.v4.paid_cost_refs);
        let source_contract = pending.resolving_item.v4.ability_source_contract;
        if let Some(effect) = pending.resolving_item.inline_effect.as_mut() {
            refresh_alesha_raid_power_lki(effect, source_contract, id, card_def, generation, power);
        }
        if let Some(contract) = pending.resolving_item.v4.source_contract.as_mut() {
            refresh_binding(&mut contract.finalized_cast_binding);
        }
        refresh(&mut pending.ctx.paid_cost_refs);
    }
    for pending in &mut state.engine.pending_triggers {
        let source_contract = pending.source_contract;
        refresh_alesha_raid_power_lki(
            &mut pending.effect,
            source_contract,
            id,
            card_def,
            generation,
            power,
        );
    }
    if let Some(pending) = state.legend_pending_v1.as_mut() {
        for pending in pending {
            let source_contract = pending.source_contract;
            refresh_alesha_raid_power_lki(
                &mut pending.effect,
                source_contract,
                id,
                card_def,
                generation,
                power,
            );
        }
    }
    for (_, object) in state.objects.iter_mut() {
        refresh_binding(&mut object.v4.finalized_cast_binding);
    }
}

fn refresh_alesha_raid_power_lki(
    effect: &mut crate::effect::EffectOp,
    contract: Option<crate::state::AbilitySourceContractV4>,
    object: ObjectId,
    card_def: u16,
    generation: u32,
    power: i32,
) {
    let (Some(contract), crate::effect::EffectOp::AleshaRaidReturn { source_power_lki }) =
        (contract, effect)
    else {
        return;
    };
    if contract.source == object
        && contract.card_def == card_def
        && contract.zone == Zone::Battlefield
        && contract.zone_change_count == generation
    {
        *source_power_lki = power;
    }
}

/// Removes a virtual game object from whichever live zone indexes it.
/// This covers 111.8/704.5d token cleanup and 707.10a spell copies leaving
/// the stack. Removes `id` from
/// whichever zone list it's currently tracked in (its owner's hand/library/
/// graveyard, `state.exile`/`command`, or the stack) without adding it
/// anywhere -- unlike every other zone transition, a token leaving the
/// battlefield doesn't go *to* another real zone, it just stops being
/// tracked. Token callers run from `trigger::sba_fixed_point`; copy callers
/// run synchronously from spell resolution/countering.
///
/// Returns whether `id` was actually still present (an already-ceased token
/// is a legal, idempotent no-op call) -- `sba_fixed_point`'s fixed-point
/// loop needs this to know whether the sweep made progress; unconditionally
/// reporting "changed" here would loop forever re-"removing" the same
/// already-gone object every pass.
///
/// Deliberately does *not* touch `GameObject::zone` (left as the object's
/// last physical marker). Live membership is authoritative from the zone
/// indexes and `state.stack`, which this function removes it from. Arena ids
/// are never freed, so snapshots and provenance may still refer to the inert
/// historical identity without making it a live target.
///
/// Also drops every `state.engine.linked_exile_records` row whose exiled
/// object is `id`. Because the zone marker and zone-change count stay as
/// recorded, such a row would otherwise still look like a live, uniquely
/// exiled incarnation: observation fails its uniqueness proof and
/// `ReturnObjectsExiledBySource` would put the ceased token back onto the
/// battlefield, which 111.8 forbids. Rows naming `id` only as their source
/// are kept: a token source ceases only after leaving the battlefield, and
/// its leaves-the-battlefield return still resolves from the row by source
/// contract.
pub fn cease_to_exist(state: &mut GameState, id: ObjectId) -> bool {
    let temporal_copy = state
        .stack
        .iter()
        .find(|item| item.source == id && item.is_copy)
        .filter(|item| {
            item.v4
                .source_contract
                .is_some_and(|source| crate::extra_turn_v1::selected_definition(source.card_def))
        })
        .and_then(|item| {
            item.v4
                .source_contract
                .map(|source| (source, item.v4.stack_item_id))
        });
    let owner = state.objects.get(id).owner;
    let zone = state.objects.get(id).zone;
    let banishing_records: Vec<_> = state
        .engine
        .linked_exile_records
        .iter()
        .filter(|record| {
            record.exiled == id
                && crate::banishing_light_v1::owns_definition(record.source.card_def)
        })
        .copied()
        .collect();
    let removed = remove_from_zone(state, owner, id, zone);
    if removed {
        if let Some((source, stack_item_id)) = temporal_copy {
            state
                .engine
                .event_history
                .push(CommittedEvent::ExtraTurnVirtualCeasedV1 {
                    source,
                    stack_item_id,
                });
        }
        crate::banishing_light_v1::record_recipient_ceased(state, &banishing_records);
        state.forget_hand_object(id);
        state.clear_object_relations(id);
        state
            .engine
            .linked_exile_records
            .retain(|record| record.exiled != id);
    }
    removed
}

/// Returns whether `id` was actually present in `zone`'s list before being
/// removed -- see `cease_to_exist`'s doc for why that matters to callers.
fn remove_from_zone(state: &mut GameState, owner: PlayerId, id: ObjectId, zone: Zone) -> bool {
    fn drop_from(v: &mut Vec<ObjectId>, id: ObjectId) -> bool {
        let before = v.len();
        v.retain(|&x| x != id);
        before != v.len()
    }
    match zone {
        Zone::Library => {
            let position = state.players[owner.index()]
                .library
                .iter()
                .position(|&candidate| candidate == id);
            let removed = drop_from(&mut state.players[owner.index()].library, id);
            if let Some(position) = position {
                // At present every generic library departure is from a
                // publicly determined position (draw/top-card exile). A
                // future hidden search must use its own knowledge-aware
                // library operation instead of this generic zone move.
                state.note_library_removal(owner, position);
            }
            removed
        }
        Zone::Hand => drop_from(&mut state.players[owner.index()].hand, id),
        Zone::Battlefield => {
            // Battlefield membership is controller-keyed, unlike every
            // owner-keyed hidden/public card zone. A stolen permanent must
            // leave its current controller's battlefield before the same
            // physical card enters its owner's destination zone.
            let controller = state.objects.get(id).controller;
            drop_from(&mut state.players[controller.index()].battlefield, id)
        }
        Zone::Graveyard => drop_from(&mut state.players[owner.index()].graveyard, id),
        Zone::Exile => drop_from(&mut state.exile, id),
        Zone::Command => drop_from(&mut state.command, id),
        Zone::Stack => {
            let before = state.stack.len();
            let live_generation = state.objects.get(id).zone_change_count;
            state.stack.retain(|item| {
                item.kind != StackItemKind::Spell
                    || item.source != id
                    || item
                        .v4
                        .source_contract
                        .is_none_or(|contract| contract.zone_change_count != live_generation)
            });
            before != state.stack.len()
        }
    }
}

/// Place lore counters together, then trigger every chapter whose threshold was crossed.
pub(crate) fn add_lore_counters(
    state: &mut GameState,
    source: ObjectId,
    amount: i16,
) -> Result<(), String> {
    add_lore_counters_with_origin(
        state,
        source,
        amount,
        crate::doubling_season_v48::CounterOriginV48::Effect,
    )
}
pub(crate) fn add_lore_counters_rule(
    state: &mut GameState,
    source: ObjectId,
    amount: i16,
) -> Result<(), String> {
    add_lore_counters_with_origin(
        state,
        source,
        amount,
        crate::doubling_season_v48::CounterOriginV48::Rule,
    )
}
fn add_lore_counters_with_origin(
    state: &mut GameState,
    source: ObjectId,
    amount: i16,
    origin: crate::doubling_season_v48::CounterOriginV48,
) -> Result<(), String> {
    let live = state.objects.get(source);
    let count = crate::doubling_season_v48::counter_amount(
        state,
        source,
        live.controller,
        origin,
        i32::from(amount),
    )?;
    let before = live.counters.lore;
    let after = before
        .checked_add(i16::try_from(count).map_err(|_| "lore count overflow")?)
        .ok_or("lore counters overflow")?;
    let chapters = crate::card_def::CARD_DEFS[live.card_def as usize]
        .saga
        .as_ref()
        .filter(|_| {
            live.v4.face_index == 0
                && crate::continuous_characteristics_v1::printed_abilities_active(state, source)
        })
        .map_or(0, |saga| saga.chapter_effects.len());
    state.objects.get_mut(source).counters.lore = after;
    for chapter in (before.max(0) + 1)..=after.min(i16::try_from(chapters).unwrap_or(i16::MAX)) {
        log_saga_chapter(state, source, chapter as u8);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PlayerId;

    fn fresh_state() -> GameState {
        GameState::new_from_libraries(&[1, 2, 3], &[4, 5, 6], |c| format!("card-{c}"), 1)
    }

    #[cfg(feature = "standard-magezero-fixtures")]
    #[test]
    fn heartfire_death_respects_the_pre_zone_change_ability_snapshot() {
        for abilities_removed in [false, true] {
            let mut state = fresh_state();
            let hero = push_object_into(&mut state, PlayerId::P0, Zone::Battlefield);
            let definition = crate::card_def::card_id_by_name("Heartfire Hero").unwrap();
            let object = state.objects.get_mut(hero);
            object.card_def = definition;
            object.name = "Heartfire Hero".into();
            object.v4 = crate::state::ObjectStateV4::from_card_def(definition);
            state.players[0].battlefield.push(hero);

            // A simultaneous zone-change batch freezes whether an Aura had
            // removed the printed ability before either permanent leaves.
            commit_with_ability_lki(
                &mut state,
                ProposedEvent::zone_change(hero, Zone::Graveyard),
                Some(abilities_removed),
                None,
            );
            let triggers = crate::trigger::collect_and_process(&mut state);
            assert_eq!(triggers.len(), usize::from(!abilities_removed));
            if let Some(trigger) = triggers.first() {
                assert_eq!(
                    trigger.effect,
                    crate::effect::EffectOp::DealDamage {
                        target: crate::effect::TargetRef::Opponent,
                        amount: 1,
                    }
                );
            }
        }
    }

    fn lifelink_source(state: &mut GameState) -> ObjectId {
        let source = push_object_into(state, PlayerId::P0, Zone::Battlefield);
        let definition = crate::card_def::card_id_by_name("Llanowar Elves").unwrap();
        let object = state.objects.get_mut(source);
        object.card_def = definition;
        object.v4 = crate::state::ObjectStateV4::from_card_def(definition);
        object.v4.lifelink_keyword_counters = 1;
        state.players[0].battlefield.push(source);
        source
    }

    fn life_gain_amounts(state: &GameState) -> Vec<i32> {
        state
            .engine
            .event_log
            .iter()
            .filter_map(|event| match event {
                CommittedEvent::LifeGain {
                    player: PlayerId::P0,
                    amount,
                } => Some(*amount),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn lifelink_split_damage_is_one_life_gain_event() {
        let mut state = fresh_state();
        let source = lifelink_source(&mut state);
        let target = push_object_into(&mut state, PlayerId::P1, Zone::Battlefield);
        propose_and_commit_batch(
            &mut state,
            vec![
                ProposedEvent::damage(source, Target::Object(target), 2),
                ProposedEvent::damage(source, Target::Player(PlayerId::P1), 3),
            ],
        );
        assert_eq!(state.players[0].life, 25);
        assert_eq!(state.players[1].life, 17);
        assert_eq!(state.objects.get(target).damage, 2);
        assert_eq!(life_gain_amounts(&state), vec![5]);
    }

    #[test]
    fn lifelink_distinct_sources_keep_distinct_life_gain_events() {
        let mut state = fresh_state();
        let first = lifelink_source(&mut state);
        let second = lifelink_source(&mut state);
        propose_and_commit_batch(
            &mut state,
            vec![
                ProposedEvent::damage(first, Target::Player(PlayerId::P1), 2),
                ProposedEvent::damage(second, Target::Player(PlayerId::P1), 3),
            ],
        );
        assert_eq!(state.players[0].life, 25);
        assert_eq!(life_gain_amounts(&state), vec![2, 3]);
    }

    #[test]
    fn lifelink_sequential_damage_keeps_separate_life_gain_events() {
        let mut state = fresh_state();
        let source = lifelink_source(&mut state);
        for amount in [2, 3] {
            propose_and_commit(
                &mut state,
                ProposedEvent::damage(source, Target::Player(PlayerId::P1), amount),
            );
        }
        assert_eq!(state.players[0].life, 25);
        assert_eq!(life_gain_amounts(&state), vec![2, 3]);
    }

    #[test]
    fn lifelink_split_damage_counts_only_damage_after_prevention() {
        let mut state = fresh_state();
        let source = lifelink_source(&mut state);
        let target = push_object_into(&mut state, PlayerId::P1, Zone::Battlefield);
        install_combat_damage_prevention(&mut state, target, target);
        let combat = |target, amount| {
            ProposedEvent::Damage(DamageProposed {
                source,
                target,
                amount,
                is_combat: true,
                historical_source: None,
                niv_damage_origin_v1: None,
                touched_by: Vec::new(),
            })
        };
        propose_and_commit_batch(
            &mut state,
            vec![
                combat(Target::Object(target), 2),
                combat(Target::Player(PlayerId::P1), 3),
            ],
        );
        assert_eq!(state.players[0].life, 23);
        assert_eq!(state.objects.get(target).damage, 0);
        assert_eq!(life_gain_amounts(&state), vec![3]);
    }

    #[test]
    fn nonpositive_life_gain_does_not_emit_an_event() {
        let mut state = fresh_state();
        for amount in [0, -1] {
            propose_and_commit(&mut state, ProposedEvent::life_gain(PlayerId::P0, amount));
        }
        assert_eq!(state.players[0].life, 20);
        assert!(state.engine.event_log.is_empty());
    }

    #[test]
    fn commit_damage_to_player_reduces_life() {
        let mut state = fresh_state();
        propose_and_commit(
            &mut state,
            ProposedEvent::damage(ObjectId(0), Target::Player(PlayerId::P1), 3),
        );
        assert_eq!(state.players[1].life, 17);
        assert_eq!(
            state.engine.event_log,
            vec![CommittedEvent::Damage {
                source: ObjectId(0),
                target: Target::Player(PlayerId::P1),
                amount: 3
            }]
        );
    }

    #[test]
    fn zone_change_moves_between_owner_zones_and_updates_object() {
        let mut state = fresh_state();
        let card = state.draw_card(PlayerId::P0).unwrap();
        propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(card, Zone::Battlefield),
        );
        assert_eq!(state.objects.get(card).zone, Zone::Battlefield);
        assert!(state.players[0].battlefield.contains(&card));
        assert!(!state.players[0].hand.contains(&card));
    }

    #[test]
    fn library_to_hand_preserves_identity_only_when_explicit_and_informed() {
        let mut hidden_state = fresh_state();
        let hidden = hidden_state.players[0].library[0];
        propose_and_commit(
            &mut hidden_state,
            ProposedEvent::zone_change(hidden, Zone::Hand),
        );
        assert!(hidden_state
            .known_hand_cards(PlayerId::P1, PlayerId::P0)
            .is_empty());

        let mut revealed_state = fresh_state();
        let revealed = revealed_state.players[0].library[0];
        let old_generation = revealed_state.objects.get(revealed).zone_change_count;
        revealed_state.reveal_library_top(PlayerId::P1, PlayerId::P0, 1);
        propose_and_commit(
            &mut revealed_state,
            ProposedEvent::zone_change_preserving_known_identity(revealed, Zone::Hand),
        );
        assert_eq!(
            revealed_state
                .known_hand_cards(PlayerId::P1, PlayerId::P0)
                .iter()
                .map(|entry| (entry.object, entry.zone_change_count))
                .collect::<Vec<_>>(),
            vec![(revealed, old_generation + 1)]
        );
        assert!(revealed_state
            .known_hand_cards(PlayerId::P0, PlayerId::P0)
            .is_empty());

        let mut stale_state = fresh_state();
        let stale = stale_state.players[0].library[0];
        stale_state.reveal_library_top(PlayerId::P1, PlayerId::P0, 1);
        stale_state.objects.get_mut(stale).zone_change_count += 1;
        propose_and_commit(
            &mut stale_state,
            ProposedEvent::zone_change_preserving_known_identity(stale, Zone::Hand),
        );
        assert!(stale_state
            .known_hand_cards(PlayerId::P1, PlayerId::P0)
            .is_empty());
    }

    #[test]
    fn draw_from_empty_library_sets_drew_from_empty() {
        let mut state = GameState::new_from_libraries(&[], &[1], |c| format!("card-{c}"), 1);
        propose_and_commit(&mut state, ProposedEvent::draw(PlayerId::P0));
        assert!(state.players[0].drew_from_empty);
    }

    /// End-to-end proof of the replacement pipeline shape required by the
    /// design: a prevention shield partially absorbs one hit, then expires
    /// and lets a subsequent hit through in full.
    #[test]
    fn prevention_shield_absorbs_then_expires() {
        let mut state = fresh_state();
        state.engine.active_replacements.push(ActiveReplacement {
            id: 1,
            source: ObjectId(0),
            kind: ReplacementEffectKind::PreventNextDamage {
                target: Target::Player(PlayerId::P1),
                remaining: 2,
            },
        });

        // First hit: 5 damage, shield absorbs 2 -> 3 gets through.
        propose_and_commit(
            &mut state,
            ProposedEvent::damage(ObjectId(0), Target::Player(PlayerId::P1), 5),
        );
        assert_eq!(state.players[1].life, 17);
        assert!(
            state.engine.active_replacements.is_empty(),
            "shield should be fully consumed"
        );

        // Second hit: shield is gone, full damage applies.
        propose_and_commit(
            &mut state,
            ProposedEvent::damage(ObjectId(0), Target::Player(PlayerId::P1), 4),
        );
        assert_eq!(state.players[1].life, 13);
    }

    #[test]
    fn prevention_shield_can_fully_prevent_small_hits() {
        let mut state = fresh_state();
        state.engine.active_replacements.push(ActiveReplacement {
            id: 7,
            source: ObjectId(0),
            kind: ReplacementEffectKind::PreventNextDamage {
                target: Target::Player(PlayerId::P1),
                remaining: 10,
            },
        });
        propose_and_commit(
            &mut state,
            ProposedEvent::damage(ObjectId(0), Target::Player(PlayerId::P1), 3),
        );
        assert_eq!(
            state.players[1].life, 20,
            "fully prevented, no event should mutate life"
        );
        assert_eq!(
            state.engine.active_replacements[0].kind,
            ReplacementEffectKind::PreventNextDamage {
                target: Target::Player(PlayerId::P1),
                remaining: 7,
            }
        );
    }

    #[test]
    fn a_replacement_never_touches_the_same_proposal_twice() {
        // A replacement that always "applies" but rewrites to something it
        // would also match would loop forever without touched_by tracking.
        // PreventNextDamage never re-matches its own rewritten (smaller)
        // event because it fully consumes itself in one hit; this test
        // just pins that a single shield only ever fires once per event.
        let mut state = fresh_state();
        state.engine.active_replacements.push(ActiveReplacement {
            id: 3,
            source: ObjectId(0),
            kind: ReplacementEffectKind::PreventNextDamage {
                target: Target::Player(PlayerId::P1),
                remaining: 100,
            },
        });
        propose_and_commit(
            &mut state,
            ProposedEvent::damage(ObjectId(0), Target::Player(PlayerId::P1), 5),
        );
        assert_eq!(state.players[1].life, 20);
        assert_eq!(
            state.engine.active_replacements[0].kind,
            ReplacementEffectKind::PreventNextDamage {
                target: Target::Player(PlayerId::P1),
                remaining: 95,
            }
        );
    }

    /// Spellbench launch-benchmark reproduction (CawGates): Journey to Nowhere
    /// exiled a token, the 111.8/704.5d sweep ceased it, and the linked-exile
    /// record outlived it, so every later observation failed and Journey
    /// leaving would have returned the ceased token.
    #[test]
    fn cease_to_exist_drops_the_linked_exile_record_of_the_ceased_exiled_object() {
        use crate::state::{AbilitySourceContractV4, LinkedExileRecordV4, ObjectLinkV4};

        let mut state = fresh_state();
        let source = push_object_into(&mut state, PlayerId::P0, Zone::Battlefield);
        let exiled = push_object_into(&mut state, PlayerId::P1, Zone::Exile);
        state.players[0].battlefield.push(source);
        state.exile.push(exiled);
        state.objects.get_mut(exiled).v4.exiled_by = Some(ObjectLinkV4 {
            object: source,
            zone_change_count: 0,
        });
        state.engine.linked_exile_records.push(LinkedExileRecordV4 {
            source: AbilitySourceContractV4 {
                source,
                card_def: 0,
                owner: PlayerId::P0,
                controller: PlayerId::P0,
                zone: Zone::Battlefield,
                zone_change_count: 0,
                attached_to: None,
            },
            exiled,
            exiled_card_def: 0,
            exiled_owner: PlayerId::P1,
            exiled_zone_change_count: 0,
        });

        assert!(cease_to_exist(&mut state, exiled));
        assert!(!state.exile.contains(&exiled));
        assert_eq!(state.objects.get(exiled).v4.exiled_by, None);
        assert!(
            state.engine.linked_exile_records.is_empty(),
            "a ceased exiled object must not leave a linked-exile record: {:?}",
            state.engine.linked_exile_records
        );
    }

    /// A token source that left the battlefield has already triggered its
    /// return, which resolves from the row by source contract, so the row
    /// must survive the source ceasing to exist.
    #[test]
    fn cease_to_exist_keeps_the_linked_exile_record_of_a_ceased_token_source() {
        use crate::state::{AbilitySourceContractV4, LinkedExileRecordV4};

        let mut state = fresh_state();
        let token_source = push_object_into(&mut state, PlayerId::P0, Zone::Graveyard);
        state.players[0].graveyard.push(token_source);
        let still_exiled = push_object_into(&mut state, PlayerId::P1, Zone::Exile);
        state.exile.push(still_exiled);
        let record = LinkedExileRecordV4 {
            source: AbilitySourceContractV4 {
                source: token_source,
                card_def: 0,
                owner: PlayerId::P0,
                controller: PlayerId::P0,
                zone: Zone::Battlefield,
                zone_change_count: 0,
                attached_to: None,
            },
            exiled: still_exiled,
            exiled_card_def: 0,
            exiled_owner: PlayerId::P1,
            exiled_zone_change_count: 0,
        };
        state.objects.get_mut(token_source).zone_change_count = 1;
        state.engine.linked_exile_records.push(record);
        assert!(cease_to_exist(&mut state, token_source));
        assert_eq!(state.engine.linked_exile_records, vec![record]);
    }

    fn push_object_into(state: &mut GameState, owner: PlayerId, zone: Zone) -> ObjectId {
        state.objects.push(crate::state::GameObject {
            card_def: 0,
            name: "fixture".to_string(),
            owner,
            controller: owner,
            zone,
            tapped: false,
            summoning_sick: false,
            damage: 0,
            counters: Default::default(),
            attachments: Vec::new(),
            v4: crate::state::ObjectStateV4::from_card_def(0),
            spell_copy_origin: None,
            plotted_turn: None,
            zone_change_count: 0,
        })
    }
}

#[cfg(test)]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod authority_visibility_tests {
    use super::*;
    use crate::policy_observation_v6::tests::put;

    #[test]
    fn original_entry_visibility_rejects_coherent_public_mask_rewrite_live_and_departed_both_seats()
    {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for disguised in [false, true] {
                let mut state = crate::policy_observation_v6::tests::ready_state_for(actor);
                put(
                    &mut state,
                    actor,
                    "Authority of the Consuls",
                    Zone::Battlefield,
                );
                let entrant = put(&mut state, actor.opponent(), "Llanowar Elves", Zone::Hand);
                let mut batch = crate::authority_of_the_consuls_v1::EntryBatchV1::capture(&state)
                    .expect("actual Authority definition");
                commit_with_ability_lki_and_entry_batch(
                    &mut state,
                    ProposedEvent::zone_change(entrant, Zone::Battlefield),
                    None,
                    None,
                    Some(&mut batch),
                    (None, None),
                    (None, false),
                );
                // A typed physical hidden-characteristic boundary before
                // deferred entry capture, not an admitted FDN disguise card.
                // The ZoneChange, original observer and dispatch journals are
                // all emitted by their genuine engine producers.
                state.objects.get_mut(entrant).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
                    disguised,
                    lookers: 1 << actor.opponent().index(),
                    hidden_by: None,
                    hidden_by_source: None,
                });
                batch.finish(&mut state);
                crate::authority_public_capture_v1::validate_state(&state).unwrap();
                if !disguised {
                    assert!(!crate::engine::object_has_type(
                        &state,
                        entrant,
                        crate::card_def::CardType::Creature,
                    ));
                    assert!(!state.engine.event_history.iter().any(|event| matches!(
                        event,
                        CommittedEvent::AuthorityEntryObserversV1 { .. }
                            | CommittedEvent::AuthorityEntryPublicFactsV1 { .. }
                    )));
                    let restored: GameState =
                        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                    crate::authority_public_capture_v1::validate_state(&restored).unwrap();
                    continue;
                }
                let index = state
                    .engine
                    .event_history
                    .iter()
                    .position(|event| {
                        matches!(event, CommittedEvent::AuthorityEntryPublicFactsV1 { .. })
                    })
                    .unwrap();
                let surface = crate::policy_surface_v5::PolicySurfaceV5::new_for_session();
                let neutral = crate::card_def::card_id_by_name(if disguised {
                    "Face-down creature"
                } else {
                    "Face-down card"
                });
                for departed in [false, true] {
                    let mut original = state.clone();
                    if departed {
                        propose_and_commit(
                            &mut original,
                            ProposedEvent::zone_change(entrant, Zone::Hand),
                        );
                    }
                    let receipt =
                        crate::authority_public_capture_v1::facts_at(&original, (index - 1) as u64)
                            .unwrap();
                    assert_eq!(
                        receipt
                            .entrant
                            .as_ref()
                            .map(|reference| reference.card_db_id),
                        neutral
                    );
                    for viewer in [actor, actor.opponent()] {
                        let public = crate::authority_public_v1::observe_v1(
                            &original, &surface, viewer, 0, 0, 0, 1,
                        );
                        if let Some(neutral) = neutral {
                            assert_eq!(
                                public.unwrap().entry_occurrences[0]
                                    .entrant_at_entry
                                    .card_db_id,
                                neutral
                            );
                        } else {
                            assert!(public.is_err());
                        }
                    }
                    let independent = original.engine.event_history[index - 1].clone();
                    let CommittedEvent::AuthorityEntryPublicFactsV1 { capture } =
                        &mut original.engine.event_history[index]
                    else {
                        unreachable!()
                    };
                    let mut snapshot: crate::state::GameObject =
                        serde_json::from_str(&capture.entrant_snapshot).unwrap();
                    snapshot.v4.face_down_v1 = None;
                    capture.entrant =
                        crate::sower_restriction_capture_v1::masked_ref(entrant, &snapshot).ok();
                    capture.entrant_snapshot = serde_json::to_string(&snapshot).unwrap();
                    assert_eq!(original.engine.event_history[index - 1], independent);
                    let bytes = serde_json::to_vec(&original).unwrap();
                    let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
                    for viewer in [actor, actor.opponent()] {
                        assert!(crate::authority_public_v1::observe_v1(
                            &restored, &surface, viewer, 0, 0, 0, 1
                        )
                        .is_err());
                    }
                    assert!(
                        crate::engine::step(&mut restored, crate::engine::Action::Pass).is_err()
                    );
                    assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
                    let session =
                        crate::rl_session::FastActorSessionV1::from_authority_v17_fixture_state(
                            restored,
                        );
                    assert!(matches!(
                        session.current_response(),
                        crate::rl_session::FastActorResponseV1::Terminal(_)
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod garruk_entry_tests {
    use super::*;
    use crate::policy_observation_v6::tests::put;

    #[test]
    fn event_time_power_precedes_sba_and_distinguishes_plain_from_face_down_entry() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for face_down in [false, true] {
                let mut state = crate::policy_observation_v6::tests::ready_state_for(actor);
                state.step = crate::state::Step::Main1;
                for owner in [actor, actor.opponent()] {
                    for _ in 0..4 {
                        put(&mut state, owner, "Forest", Zone::Library);
                    }
                }
                put(&mut state, actor, "Forest", Zone::Battlefield);
                let source = put(&mut state, actor, "Garruk's Uprising", Zone::Hand);
                let entrant = put(&mut state, actor, "Myr Enforcer", Zone::Hand);
                let mut batch = crate::garruks_uprising_v1::EntryBatchV1::capture().unwrap();
                for id in [source, entrant] {
                    let hidden = (face_down && id == entrant).then_some((
                        id,
                        crate::garruks_uprising_v1::EntryCharacteristicsV1 {
                            face_down: Some(crate::state::FaceDownV1 {
                                disguised: true,
                                lookers: 1 << actor.index(),
                                hidden_by: None,
                                hidden_by_source: None,
                            }),
                            ..Default::default()
                        },
                    ));
                    // Genuine entry commit with explicitly labeled physical
                    // as-entry characteristics, not a printed FDN Disguise claim.
                    commit_with_ability_lki_and_entry_batch(
                        &mut state,
                        ProposedEvent::zone_change(id, Zone::Battlefield),
                        None,
                        None,
                        None,
                        (Some(&mut batch), None),
                        (hidden, false),
                    );
                }
                if !face_down {
                    // Physical post-entry counter boundary before observation:
                    // 4 power / 0 toughness. No fabricated trigger/root/history.
                    state.objects.get_mut(entrant).counters.minus0_minus1 = 4;
                }
                batch.finish(&mut state);
                let captures: Vec<_> = state
                    .engine
                    .event_history
                    .iter()
                    .filter_map(|event| {
                        if let CommittedEvent::GarrukDrawCaptureV1 { capture } = event {
                            Some(capture.root)
                        } else {
                            None
                        }
                    })
                    .collect();
                if face_down {
                    assert_eq!(crate::engine::effective_power(&state, entrant), 2);
                    assert!(captures.is_empty());
                    let observation = state
                        .engine
                        .event_history
                        .iter()
                        .find_map(|event| {
                            if let CommittedEvent::GarrukEntryObservationV1 {
                                entry,
                                effective_power,
                                ..
                            } = event
                            {
                                (entry.object == entrant).then_some((entry, effective_power))
                            } else {
                                None
                            }
                        })
                        .unwrap();
                    assert!(observation.0.snapshot.v4.face_down_v1.is_some());
                    assert_eq!(*observation.1, Some(2));
                } else {
                    assert_eq!(
                        captures,
                        [
                            crate::garruks_uprising_v1::RootV1::OwnEtbFerocious,
                            crate::garruks_uprising_v1::RootV1::ControlledCreaturePowerEntry
                        ]
                    );
                    let before = state.players[actor.index()].hand.len();
                    for _ in 0..40 {
                        match crate::engine::advance_until_decision(&mut state) {
                            crate::engine::Decision::OrderTriggers { pending, .. } => {
                                crate::engine::step(
                                    &mut state,
                                    crate::engine::Action::OrderTriggers(
                                        (0..pending.len()).collect(),
                                    ),
                                )
                                .unwrap();
                            }
                            crate::engine::Decision::CastSpellOrPass { .. }
                                if state.stack.is_empty() =>
                            {
                                break
                            }
                            crate::engine::Decision::CastSpellOrPass { .. } => {
                                crate::engine::step(&mut state, crate::engine::Action::Pass)
                                    .unwrap()
                            }
                            other => panic!("genuine event-time draw traversal {other:?}"),
                        }
                    }
                    assert_eq!(state.objects.get(entrant).zone, Zone::Graveyard);
                    assert_eq!(
                        state.players[actor.index()].hand.len(),
                        before + 1,
                        "entry draw survives zero toughness; own ETB condition becomes false"
                    );
                }
                crate::garruks_uprising_v1::validate_state(&state).unwrap();
            }
        }
    }
}

#[cfg(test)]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod arahbo_entry_tests {
    use super::*;
    use crate::arahbo_v1::future_tests::{cast, ready, settle};
    use crate::arahbo_v1::{EntryBatchV1, RootV1};
    use crate::engine;
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::put;
    #[test]
    fn nontoken_cat_permanent_does_not_require_creature_type_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            cast(&mut state, actor);
            // Labeled physical layer-4 substrate: an accepted Equipment with Cat
            // subtype, not a claim of a natural printed FDN type-grant producer.
            let equipment = put(&mut state, actor, "Goldvein Pick", Zone::Hand);
            crate::event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(equipment, Zone::Battlefield),
            );
            let start = state.engine.event_history.len() - 1;
            // Re-enter through the genuine event pipeline with its retained cached
            // Cat characteristic. The definition remains a noncreature artifact.
            crate::event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(equipment, Zone::Hand),
            );
            state
                .objects
                .get_mut(equipment)
                .v4
                .effective_subtype_ids
                .push(crate::card_def::Subtype::Cat.stable_id());
            let mut batch = EntryBatchV1::capture().unwrap();
            // Cache reset on entry means this boundary belongs immediately before
            // observation, after actual committed movement, as other typed tests do.
            commit_with_ability_lki_and_entry_batch(
                &mut state,
                ProposedEvent::zone_change(equipment, Zone::Battlefield),
                None,
                None,
                None,
                (None, Some(&mut batch)),
                (None, false),
            );
            state
                .objects
                .get_mut(equipment)
                .v4
                .effective_subtype_ids
                .push(crate::card_def::Subtype::Cat.stable_id());
            batch.finish(&mut state);
            assert!(!engine::object_has_type(
                &state,
                equipment,
                crate::card_def::CardType::Creature
            ));
            assert!(state.engine.event_history[start..].iter().any(|event|matches!(event,CommittedEvent::ArahboTokenCaptureV1 {capture} if capture.root==RootV1::OtherNontokenCatEntry)));
            settle(&mut state);
            assert_eq!(state.engine.event_history.iter().filter(|event|matches!(event,CommittedEvent::CreateToken {token_def,..} if Some(*token_def)==crate::card_def::card_id_by_name("Cat Token"))).count(),2);
        }
    }
}

#[cfg(test)]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod season_time_entry_tests_v48 {
    use super::*;
    use crate::policy_observation_v6::tests::{put, ready_state_for};

    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn genuine_time_entry_primitive_preflights_original_board_before_mutation_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for seasons in [1, 6] {
                let mut state = ready_state_for(actor);
                for _ in 0..seasons {
                    let source = put(&mut state, actor, "Doubling Season", Zone::Hand);
                    propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Battlefield),
                    );
                    assert!(state.engine.halted.is_none());
                }
                let entrant = put(&mut state, actor, "Forest", Zone::Hand);
                let mut restored: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                for branch in [&mut state, &mut restored] {
                    let before = branch.clone();
                    // Actual central entry primitive on a physical permanent;
                    // this does not give Forest a fictional Impending ability.
                    commit_with_ability_lki_and_entry_batch(
                        branch,
                        ProposedEvent::zone_change(entrant, Zone::Battlefield),
                        None,
                        None,
                        None,
                        (None, None),
                        (
                            Some((
                                entrant,
                                crate::garruks_uprising_v1::EntryCharacteristicsV1 {
                                    time_counters: Some(4),
                                    ..Default::default()
                                },
                            )),
                            false,
                        ),
                    );
                    if seasons == 1 {
                        assert!(branch.engine.halted.is_none());
                        assert_eq!(branch.objects.get(entrant).zone, Zone::Battlefield);
                        assert_eq!(branch.objects.get(entrant).v4.time_counters_v1, 8);
                    } else {
                        assert_eq!(
                            branch.engine.halted.as_ref().map(|x| x.0),
                            Some(crate::engine::UnsupportedMechanic::InvalidEffectContinuation)
                        );
                        branch.engine.halted = None;
                        assert_eq!(*branch, before);
                    }
                }
                assert_eq!(state, restored);
            }
        }
    }
}

#[cfg(test)]
mod soul_placement_primitive_v1_tests {
    use super::*;
    use crate::effect::{self, EffectOp, ExecCtx, PlayerRef};
    use crate::policy_observation_v6::tests::{put, ready_state_for};
    use crate::state::AbilitySourceContractV4;

    fn context(state: &GameState, source: ObjectId, actor: PlayerId) -> ExecCtx {
        let mut ctx = ExecCtx::no_targets(source, actor);
        ctx.ability_source_contract = Some(AbilitySourceContractV4::capture(state, source));
        ctx
    }

    #[test]
    fn draw_precedes_soul_and_departure_or_reentry_keeps_draw_without_counter_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for departure in 0..3 {
                let mut state = ready_state_for(actor);
                // Typed instruction operand, not a printed Forest activation.
                let source = put(&mut state, actor, "Forest", Zone::Battlefield);
                put(&mut state, actor, "Forest", Zone::Library);
                let ctx = context(&state, source, actor);
                if departure > 0 {
                    propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
                }
                if departure == 2 {
                    propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Battlefield),
                    );
                }
                let hand = state.players[actor.index()].hand.len();
                let at = state.engine.event_history.len();
                let mut restored: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                for branch in [&mut state, &mut restored] {
                    effect::execute(
                        &EffectOp::Sequence(vec![
                            EffectOp::DrawCards {
                                player: PlayerRef::Controller,
                                count: 1,
                            },
                            EffectOp::AddSoulCounterToAbilitySourceV1,
                        ]),
                        &ctx,
                        branch,
                    );
                    assert!(branch.engine.halted.is_none());
                    assert_eq!(branch.players[actor.index()].hand.len(), hand + 1);
                    assert!(branch.players[actor.index()].library.is_empty());
                    assert_eq!(
                        branch.objects.get(source).counters.soul,
                        i32::from(departure == 0)
                    );
                    let events = &branch.engine.event_history[at..];
                    let draw = events.iter().position(|e| matches!(e, CommittedEvent::Draw { player, .. } if *player == actor)).unwrap();
                    let soul = events
                        .iter()
                        .position(|e| matches!(e, CommittedEvent::SoulCounterAddedV1 { .. }));
                    if departure == 0 {
                        assert!(soul.unwrap() > draw);
                    } else {
                        assert!(soul.is_none());
                    }
                }
                assert_eq!(state, restored);
            }
        }
    }

    #[test]
    fn recipient_control_and_original_ability_controller_remain_distinct_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let source = put(&mut state, actor, "Forest", Zone::Battlefield);
            let ctx = context(&state, source, actor);
            crate::standard_cards_v1::gain_control(&mut state, source, actor.opponent());
            add_soul_counter_to_ability_source_v1(&mut state, &ctx).unwrap();
            let Some(CommittedEvent::SoulCounterAddedV1 {
                source: original,
                player,
                replacement,
                before,
                count,
                after,
                ..
            }) = state.engine.event_history.last()
            else {
                panic!("actual soul placement")
            };
            assert_eq!(original.controller, actor);
            assert_eq!(*player, actor);
            assert_eq!(replacement.controller, actor.opponent());
            assert_eq!((*before, *count, *after), (0, 1, 1));
            assert_eq!(state.objects.get(source).controller, actor.opponent());
        }
    }

    #[test]
    fn malformed_source_negative_counter_and_overflow_refuse_before_mutation() {
        let actor = PlayerId::P0;
        for mutation in 0..8 {
            let mut state = ready_state_for(actor);
            let source = put(&mut state, actor, "Forest", Zone::Battlefield);
            let other = put(&mut state, actor, "Island", Zone::Battlefield);
            let mut ctx = context(&state, source, actor);
            match mutation {
                0 => ctx.ability_source_contract = None,
                1 => ctx.ability_source_contract.as_mut().unwrap().source = other,
                2 => ctx.controller = actor.opponent(),
                3 => ctx.ability_source_contract.as_mut().unwrap().zone = Zone::Hand,
                4 => state.objects.get_mut(source).owner = actor.opponent(),
                5 => {
                    ctx.ability_source_contract
                        .as_mut()
                        .unwrap()
                        .zone_change_count += 1
                }
                6 => state.objects.get_mut(source).counters.soul = -1,
                7 => state.objects.get_mut(source).counters.soul = i32::MAX,
                _ => unreachable!(),
            }
            let before = state.clone();
            assert!(add_soul_counter_to_ability_source_v1(&mut state, &ctx).is_err());
            assert_eq!(state, before);
        }
    }

    #[test]
    fn untargeted_soul_loss_uses_original_departure_and_original_controller_after_reentry_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            // Typed source/LKI and control operands, no printed paid activation.
            let source = put(&mut state, actor, "Forest", Zone::Battlefield);
            state.objects.get_mut(source).counters.soul = 5;
            let ctx = context(&state, source, actor);
            propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
            propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
            state.objects.get_mut(source).counters.soul = 9;
            crate::standard_cards_v1::gain_control(&mut state, source, actor.opponent());
            let life = state.players.each_ref().map(|p| p.life);
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut restored] {
                effect::execute(
                    &EffectOp::LoseLifeEachOpponentEqualSourceSoulV1,
                    &ctx,
                    branch,
                );
                assert!(branch.engine.halted.is_none());
                assert_eq!(branch.players[actor.index()].life, life[actor.index()]);
                assert_eq!(
                    branch.players[actor.opponent().index()].life,
                    life[actor.opponent().index()] - 5
                );
                assert_eq!(branch.objects.get(source).counters.soul, 9);
                assert_eq!(branch.objects.get(source).controller, actor.opponent());
            }
            assert_eq!(state, restored);
        }
    }

    #[test]
    fn soul_loss_refuses_underflow_before_mutation_and_keeps_exact_boundary_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for (life, refuses) in [(-2, true), (-1, false)] {
                let mut state = ready_state_for(actor);
                // Arithmetic boundary operands, not a printed protection or paid activation.
                let source = put(&mut state, actor, "Forest", Zone::Battlefield);
                state.objects.get_mut(source).counters.soul = i32::MAX;
                state.players[actor.opponent().index()].life = life;
                let ctx = context(&state, source, actor);
                let before = state.clone();
                effect::execute(
                    &EffectOp::LoseLifeEachOpponentEqualSourceSoulV1,
                    &ctx,
                    &mut state,
                );
                if refuses {
                    assert_eq!(
                        state.engine.halted.take(),
                        Some((
                            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                            source
                        ))
                    );
                    assert_eq!(state, before);
                } else {
                    assert!(state.engine.halted.is_none());
                    assert_eq!(state.players[actor.opponent().index()].life, i32::MIN);
                    assert_eq!(
                        state.players[actor.index()].life,
                        before.players[actor.index()].life
                    );
                    assert!(
                        matches!(state.engine.event_history.last(), Some(CommittedEvent::LifeLoss { player, amount }) if *player == actor.opponent() && *amount == i32::MAX)
                    );
                }
            }
        }
    }

    #[test]
    #[cfg(feature = "standard-magezero-fixtures")]
    fn soul_loss_preserves_real_standard_modifier_and_refuses_inexact_quote_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for count in [3, i32::MAX] {
                let mut state = ready_state_for(actor);
                let source = put(&mut state, actor, "Forest", Zone::Battlefield);
                put(
                    &mut state,
                    actor,
                    "Bloodletter of Aclazotz",
                    Zone::Battlefield,
                );
                state.objects.get_mut(source).counters.soul = count;
                let ctx = context(&state, source, actor);
                let before = state.clone();
                effect::execute(
                    &EffectOp::LoseLifeEachOpponentEqualSourceSoulV1,
                    &ctx,
                    &mut state,
                );
                if count == i32::MAX {
                    assert!(state.engine.halted.take().is_some());
                    assert_eq!(state, before);
                } else {
                    assert!(state.engine.halted.is_none());
                    assert_eq!(
                        state.players[actor.opponent().index()].life,
                        before.players[actor.opponent().index()].life - 6
                    );
                    assert!(
                        matches!(state.engine.event_history.last(), Some(CommittedEvent::LifeLoss { player, amount }) if *player == actor.opponent() && *amount == 6)
                    );
                }
            }
        }
    }
}

#[cfg(test)]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) mod ravenous_amulet_paid_future_tests {
    use super::*;
    use crate::engine::{self, Action, Decision};
    use crate::mana::ManaColor;
    use crate::policy_observation_v6::tests::{advance_to_next_main_for, put, ready_state_for};
    use crate::state::AbilitySourceContractV4;

    fn public_facts(state: &GameState) -> crate::ravenous_public_v51::FactsV51 {
        let facts = crate::ravenous_public_v51::capture(state).unwrap();
        crate::ravenous_public_v51::validate(state, &facts).unwrap();
        let battlefield = crate::ravenous_public_v51::battlefield(state).unwrap();
        assert_eq!(battlefield.facts, facts);
        let restored_battlefield =
            serde_json::from_slice(&serde_json::to_vec(&battlefield).unwrap()).unwrap();
        assert_eq!(battlefield, restored_battlefield);
        crate::ravenous_public_v51::validate_battlefield(state, &restored_battlefield).unwrap();
        for card in battlefield.cards.iter().flatten() {
            let live = state
                .objects
                .get(crate::ids::ObjectId(card.card.stable.arena_id));
            assert_eq!(card.soul, live.counters.soul);
            assert_eq!(card.plus_one_counters, live.counters.plus1_plus1);
            assert_eq!(card.marked_damage, live.damage);
        }
        let mut changed_battlefield = battlefield.clone();
        if let Some(card) = changed_battlefield.cards.iter_mut().flatten().next() {
            card.soul ^= 1;
            assert!(
                crate::ravenous_public_v51::validate_battlefield(state, &changed_battlefield)
                    .is_err()
            );
        }
        let mut missing = state.clone();
        missing.players[0]
            .battlefield
            .push(crate::ids::ObjectId(u32::MAX));
        assert!(crate::ravenous_public_v51::battlefield(&missing).is_err());
        let bytes = serde_json::to_vec(&facts).unwrap();
        let restored = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(facts, restored);
        crate::ravenous_public_v51::validate(state, &restored).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        for forbidden in [
            "paid_cost_refs",
            "payment_history_start",
            "input_history_index",
            "inline_effect",
            "stack_item_id",
            "original_world",
            "event_history",
        ] {
            assert!(!text.contains(forbidden), "private field {forbidden}");
        }
        let mut private_variant = state.clone();
        let hidden = private_variant.players[0].library.last().copied().unwrap();
        private_variant.objects.get_mut(hidden).card_def =
            crate::card_def::card_id_by_name("Lightning Bolt").unwrap();
        // Projection-only private-content perturbation. This is not sampler
        // admission for a retained original-world history.
        assert_eq!(
            crate::ravenous_public_v51::capture(&private_variant).unwrap(),
            facts
        );
        assert_eq!(
            crate::ravenous_public_v51::battlefield(&private_variant).unwrap(),
            battlefield
        );
        let mut changed = facts.clone();
        changed.card_db_hash ^= 1;
        let before = state.clone();
        assert!(crate::ravenous_public_v51::validate(state, &changed).is_err());
        assert_eq!(state, &before);
        facts
    }

    fn completed_refusals(state: &GameState, id: crate::ids::StackItemId) {
        crate::doubling_season_v48::validate_token_journals(state).unwrap();
        let facts = public_facts(state);
        assert!(facts
            .activations
            .iter()
            .any(|activation| activation.outcome
                != crate::ravenous_public_v51::OutcomeV51::Published));
        for case in 0..11 {
            let mut branch: GameState =
                serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
            let (paid_index, item) = branch
                .engine
                .event_history
                .iter()
                .find_map(|event| match event {
                    CommittedEvent::RavenousPaymentV1 { paid }
                        if paid.item.v4.stack_item_id == id =>
                    {
                        Some((paid.index, paid.item.as_ref().clone()))
                    }
                    _ => None,
                })
                .unwrap();
            let removed_at = branch.engine.event_history.iter().position(|event| matches!(event, CommittedEvent::RavenousRemovedV1 { removed } if removed.paid == paid_index)).unwrap();
            let completed_at = branch.engine.event_history.iter().position(|event| matches!(event, CommittedEvent::RavenousCompletedV1 { completed } if completed.removed == removed_at as u64)).unwrap();
            match case {
                0 => branch.stack.push(item),
                1 => branch
                    .engine
                    .event_history
                    .push(branch.engine.event_history[completed_at].clone()),
                2 => {
                    branch.engine.event_history.remove(completed_at);
                }
                3 => {
                    let CommittedEvent::RavenousRemovedV1 { removed } =
                        &mut branch.engine.event_history[removed_at]
                    else {
                        unreachable!()
                    };
                    removed.kind = match removed.kind {
                        crate::ravenous_amulet_v1::RemovalKindV1::Resolving => {
                            crate::ravenous_amulet_v1::RemovalKindV1::Countered
                        }
                        crate::ravenous_amulet_v1::RemovalKindV1::Countered => {
                            crate::ravenous_amulet_v1::RemovalKindV1::Resolving
                        }
                    };
                }
                4 => branch
                    .engine
                    .event_log
                    .push(branch.engine.event_history[removed_at].clone()),
                5 | 6 => {
                    let CommittedEvent::RavenousCompletedV1 { completed } =
                        &mut branch.engine.event_history[completed_at]
                    else {
                        unreachable!()
                    };
                    if case == 5 {
                        completed.operation =
                            crate::ravenous_amulet_v1::OperationV1::Loss { event: paid_index };
                    } else {
                        completed.removed = paid_index;
                    }
                }
                7 | 9 | 10 => {
                    let CommittedEvent::RavenousRemovedV1 { removed } =
                        &mut branch.engine.event_history[removed_at]
                    else {
                        unreachable!()
                    };
                    if case == 7 {
                        removed.original_world.stack.clear();
                    } else if case == 9 {
                        removed
                            .original_world
                            .engine
                            .event_log
                            .push(CommittedEvent::LifeLoss {
                                player: PlayerId::P0,
                                amount: 0,
                            });
                    } else {
                        removed.replay_borrow = if removed.replay_borrow.is_some() {
                            None
                        } else {
                            Some(
                                serde_json::from_value(serde_json::json!({"enclosing": null}))
                                    .unwrap(),
                            )
                        };
                    }
                }
                8 => {
                    let CommittedEvent::RavenousCompletedV1 { completed } =
                        &mut branch.engine.event_history[completed_at]
                    else {
                        unreachable!()
                    };
                    completed.output_world.players[0].life += 1;
                }
                _ => unreachable!(),
            }
            let before = branch.clone();
            assert!(
                crate::doubling_season_v48::validate_token_journals(&branch).is_err(),
                "completion case {case}"
            );
            assert_eq!(branch, before);
        }
    }

    fn cross_family_original_world_refusals(
        state: &GameState,
        actor: PlayerId,
        returned: crate::ids::ObjectId,
    ) {
        for case in 0..5 {
            let mut branch: GameState =
                serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
            let paid_item = branch
                .engine
                .event_history
                .iter()
                .find_map(|event| match event {
                    CommittedEvent::RavenousPaymentV1 { paid } => Some(paid.item.as_ref().clone()),
                    _ => None,
                })
                .unwrap();
            if case < 3 {
                let input = branch
                    .engine
                    .event_history
                    .iter_mut()
                    .find_map(|event| match event {
                        CommittedEvent::SoulCounterInputV1 { input } => Some(input),
                        _ => None,
                    })
                    .unwrap();
                match case {
                    0 => input.world.objects.get_mut(returned).tapped = false,
                    1 => {
                        assert!(input.world.players[actor.index()].hand.len() > 1);
                        input.world.players[actor.index()].hand.reverse();
                    }
                    2 => input.world.stack.push(paid_item),
                    _ => unreachable!(),
                }
            } else if case == 3 {
                let removed = branch
                    .engine
                    .event_history
                    .iter_mut()
                    .find_map(|event| match event {
                        CommittedEvent::RavenousRemovedV1 { removed } => Some(removed),
                        _ => None,
                    })
                    .unwrap();
                removed.original_world.objects.get_mut(returned).tapped = false;
            } else {
                let completed = branch
                    .engine
                    .event_history
                    .iter_mut()
                    .find_map(|event| match event {
                        CommittedEvent::RavenousCompletedV1 { completed } => Some(completed),
                        _ => None,
                    })
                    .unwrap();
                completed.output_world.objects.get_mut(returned).tapped = false;
            }
            // Only nested operation worlds changed. The actual returned Fake
            // creature and the outer paid/capture/return journals remain intact.
            assert!(branch.objects.get(returned).tapped);
            let before = branch.clone();
            assert!(
                crate::doubling_season_v48::validate_token_journals(&branch).is_err(),
                "cross-family operation world case {case}"
            );
            assert_eq!(branch, before);
        }
    }

    fn payment_receipt_refusals(state: &GameState) {
        crate::ravenous_amulet_v1::validate_state(state).unwrap();
        let facts = public_facts(state);
        assert_eq!(
            facts.activations.last().unwrap().outcome,
            crate::ravenous_public_v51::OutcomeV51::Published
        );
        for case in 0..10 {
            let mut branch: GameState =
                serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
            let index = branch
                .engine
                .event_history
                .iter()
                .rposition(|event| matches!(event, CommittedEvent::RavenousPaymentV1 { .. }))
                .unwrap();
            let original = branch.engine.event_history[index].clone();
            if case == 6 {
                branch.engine.event_history.push(original);
            } else if case == 7 {
                branch.engine.event_log.push(original);
            } else if case == 8 {
                let CommittedEvent::RavenousPaymentV1 { paid } =
                    &branch.engine.event_history[index]
                else {
                    unreachable!()
                };
                let id = paid.item.v4.stack_item_id;
                branch
                    .stack
                    .iter_mut()
                    .find(|item| item.v4.stack_item_id == id)
                    .unwrap()
                    .v4
                    .x_value = 1;
            } else {
                let CommittedEvent::RavenousPaymentV1 { paid } =
                    &mut branch.engine.event_history[index]
                else {
                    unreachable!()
                };
                match case {
                    0 => paid.index += 1,
                    1 => paid.tap_history_index = paid.index,
                    2 => {
                        paid.original_source.controller = paid.original_source.controller.opponent()
                    }
                    3 => {
                        if paid.original_sacrifice.source == paid.original_source.source {
                            let owner = paid.original_sacrifice.owner.opponent();
                            let id = paid.item.v4.stack_item_id;
                            paid.original_sacrifice.owner = owner;
                            paid.item.v4.paid_cost_refs[0].owner = owner;
                            branch
                                .stack
                                .iter_mut()
                                .find(|item| item.v4.stack_item_id == id)
                                .unwrap()
                                .v4
                                .paid_cost_refs[0]
                                .owner = owner;
                        } else {
                            paid.original_sacrifice.zone_change_count += 1;
                        }
                    }
                    4 => paid.item.is_copy = true,
                    5 => paid.item.v4.paid_cost_refs.clear(),
                    9 => {
                        paid.original_soul_count = Some(paid.original_soul_count.unwrap_or(0) + 1);
                    }
                    _ => unreachable!(),
                }
            }
            let before = branch.clone();
            assert!(
                crate::ravenous_amulet_v1::validate_state(&branch).is_err(),
                "payment case {case}"
            );
            assert_eq!(branch, before);
        }
    }

    pub(crate) fn paid_priority(state: &mut GameState, actor: PlayerId) {
        assert!(matches!(engine::advance_until_decision(state),
            Decision::CastSpellOrPass { player, .. } if player == actor));
        assert!(state.engine.pending_cast.is_none());
        assert!(state.engine.pending_activation.is_none());
    }

    pub(crate) fn finish(state: &mut GameState) {
        for _ in 0..32 {
            match engine::advance_until_decision(state) {
                Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
                Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
                other => panic!("genuine Amulet resolution: {other:?}"),
            }
        }
        panic!("Amulet stack did not finish");
    }

    pub(crate) fn cast_amulet(actor: PlayerId) -> (GameState, ObjectId) {
        cast_amulet_in(ready_state_for(actor), actor)
    }

    fn cast_amulet_in(mut state: GameState, actor: PlayerId) -> (GameState, ObjectId) {
        for player in [PlayerId::P0, PlayerId::P1] {
            for _ in 0..8 {
                put(&mut state, player, "Forest", Zone::Library);
            }
        }
        let amulet = put(&mut state, actor, "Ravenous Amulet", Zone::Hand);
        let definition = state.objects.get(amulet).card_def;
        assert_eq!(
            crate::card_def::CARD_DEFS[usize::from(definition)].capability,
            crate::card_def::CardCapability::Full
        );
        assert!(crate::card_def::preflight_fully_supported_deck(&[definition]).is_ok());
        for root in [
            crate::rules_meaning_ravenous_amulet_v51::RootV51::DrawAndSoul,
            crate::rules_meaning_ravenous_amulet_v51::RootV51::SoulLoss,
        ] {
            let meaning =
                crate::rules_meaning_ravenous_amulet_v51::printed_meaning(definition, root)
                    .unwrap();
            let restored = serde_json::from_slice(&serde_json::to_vec(&meaning).unwrap()).unwrap();
            assert_eq!(meaning, restored);
            crate::rules_meaning_ravenous_amulet_v51::validate(&restored).unwrap();
            assert!(!meaning.model_admitted);
            let mut changed = meaning.clone();
            changed.sorcery_speed_only = !changed.sorcery_speed_only;
            assert!(crate::rules_meaning_ravenous_amulet_v51::validate(&changed).is_err());
        }
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
        engine::step(&mut state, Action::CastSpell(amulet)).unwrap();
        finish(&mut state);
        assert_eq!(state.objects.get(amulet).zone, Zone::Battlefield);
        assert_eq!(
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
            0
        );
        assert_eq!(state.objects.get(amulet).counters.soul, 0);
        (state, amulet)
    }

    #[test]
    fn actual_paid_draw_soul_and_self_sacrifice_keep_original_count_after_reentry_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, amulet) = cast_amulet(actor);
            let chosen = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let spare = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
            let hand = state.players[actor.index()].hand.len();
            engine::step(&mut state, Action::ActivateAbility(amulet, 0)).unwrap();
            assert!(matches!(engine::advance_until_decision(&mut state),
                Decision::ChooseCostTargets { ref candidates, remaining: 1, .. }
                    if candidates.contains(&chosen) && candidates.contains(&spare)));
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut restored] {
                engine::step(branch, Action::ChooseCostTarget(chosen)).unwrap();
                paid_priority(branch, actor);
                assert_eq!(
                    branch.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
                    0
                );
                assert!(branch.objects.get(amulet).tapped);
                assert_eq!(branch.objects.get(chosen).zone, Zone::Graveyard);
                assert_eq!(branch.objects.get(spare).zone, Zone::Battlefield);
                assert_eq!(branch.objects.get(amulet).counters.soul, 0);
                let payments = branch
                    .engine
                    .event_history
                    .iter()
                    .filter_map(|event| match event {
                        CommittedEvent::RavenousPaymentV1 { paid } => Some(paid.as_ref()),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(payments.len(), 1);
                assert_eq!(payments[0].original_soul_count, None);
                assert_eq!(payments[0].original_sacrifice.source, chosen);
                assert_eq!(payments[0].item.v4.activated_ability_index, Some(0));
                payment_receipt_refusals(branch);
                let id = branch.stack.last().unwrap().v4.stack_item_id;
                // Actual engine counter removal of the genuinely paid row;
                // no printed counterspell producer is claimed by this boundary.
                let mut countered = branch.clone();
                assert!(engine::counter_stack_item_by_id(&mut countered, id)
                    .unwrap()
                    .is_some());
                assert_eq!(countered.players[actor.index()].hand.len(), hand);
                assert_eq!(countered.objects.get(amulet).counters.soul, 0);
                assert_eq!(countered.objects.get(chosen).zone, Zone::Graveyard);
                assert!(countered.objects.get(amulet).tapped);
                completed_refusals(&countered, id);
                finish(branch);
                assert_eq!(branch.players[actor.index()].hand.len(), hand + 1);
                assert_eq!(branch.objects.get(amulet).counters.soul, 1);
                completed_refusals(branch, id);
            }
            assert_eq!(state, restored);

            advance_to_next_main_for(&mut state, actor);
            // Another accepted soul-placement primitive responds while the
            // second genuinely paid draw activation is pending. This does not
            // claim a registered Zimone producer; it checks the resolution
            // boundary that its printed counter-doubling ability will require.
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
            engine::step(&mut state, Action::ActivateAbility(amulet, 0)).unwrap();
            engine::step(&mut state, Action::ChooseCostTarget(spare)).unwrap();
            paid_priority(&mut state, actor);
            let response_source = AbilitySourceContractV4::capture(&state, amulet);
            let mut response = crate::effect::ExecCtx::no_targets(amulet, actor);
            response.ability_source_contract = Some(response_source);
            crate::effect::execute(
                &crate::effect::EffectOp::AddSoulCounterToAbilitySourceV1,
                &response,
                &mut state,
            );
            assert_eq!(state.objects.get(amulet).counters.soul, 2);
            finish(&mut state);
            assert_eq!(state.objects.get(amulet).counters.soul, 3);
            advance_to_next_main_for(&mut state, actor);
            assert!(!state.objects.get(amulet).tapped);
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
            let original = AbilitySourceContractV4::capture(&state, amulet);
            let life = state.players.each_ref().map(|p| p.life);
            engine::step(&mut state, Action::ActivateAbility(amulet, 1)).unwrap();
            paid_priority(&mut state, actor);
            assert_eq!(
                state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
                0
            );
            assert_eq!(state.objects.get(amulet).zone, Zone::Graveyard);
            assert_eq!(state.objects.get(amulet).counters.soul, 0);
            assert_eq!(state.soul_count_for_ability_source_v1(original), Ok(3));
            let payments = state
                .engine
                .event_history
                .iter()
                .filter_map(|event| match event {
                    CommittedEvent::RavenousPaymentV1 { paid } => Some(paid.as_ref()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(payments.len(), 3);
            assert_eq!(payments[1].original_soul_count, None);
            assert_eq!(payments[2].original_source, original);
            assert_eq!(payments[2].original_sacrifice, original);
            assert_eq!(payments[2].original_soul_count, Some(3));
            assert_eq!(payments[2].item.v4.activated_ability_index, Some(1));
            payment_receipt_refusals(&state);
            let loss_id = state.stack.last().unwrap().v4.stack_item_id;
            // Explicit zone-transition boundary after genuine paid self-sacrifice.
            // This is not a claim of a printed artifact-return producer.
            propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(amulet, Zone::Battlefield),
            );
            assert_ne!(
                state.objects.get(amulet).zone_change_count,
                original.zone_change_count
            );
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut restored] {
                finish(branch);
                assert_eq!(branch.players[actor.index()].life, life[actor.index()]);
                assert_eq!(
                    branch.players[actor.opponent().index()].life,
                    life[actor.opponent().index()] - 3
                );
                assert_eq!(branch.objects.get(amulet).zone, Zone::Battlefield);
                assert_eq!(branch.objects.get(amulet).counters.soul, 0);
                completed_refusals(branch, loss_id);
            }
            assert_eq!(state, restored);
        }
    }

    #[test]
    fn actual_paid_amulet_sacrifice_coexists_with_paid_fake_return_and_restore_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, amulet) = cast_amulet(actor);
            put(&mut state, actor, "Island", Zone::Hand);
            let creature = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let spare = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let fake = put(&mut state, actor, "Fake Your Own Death", Zone::Hand);
            for _ in 0..2 {
                let swamp = put(&mut state, actor, "Swamp", Zone::Battlefield);
                paid_priority(&mut state, actor);
                engine::step(&mut state, Action::ActivateManaAbility(swamp)).unwrap();
            }
            engine::step(&mut state, Action::CastSpell(fake)).unwrap();
            assert!(matches!(engine::advance_until_decision(&mut state),
                Decision::ChooseTargets { ref legal_targets, .. }
                    if legal_targets.contains(&Target::Object(creature))));
            engine::step(&mut state, Action::ChooseTarget(Target::Object(creature))).unwrap();
            finish(&mut state);
            assert_eq!(state.objects.get(fake).zone, Zone::Graveyard);
            assert_eq!(engine::effective_power(&state, creature), 3);
            let forest = put(&mut state, actor, "Forest", Zone::Battlefield);
            paid_priority(&mut state, actor);
            engine::step(&mut state, Action::ActivateManaAbility(forest)).unwrap();
            engine::step(&mut state, Action::ActivateAbility(amulet, 0)).unwrap();
            assert!(matches!(engine::advance_until_decision(&mut state),
                Decision::ChooseCostTargets { ref candidates, .. }
                    if candidates.contains(&creature) && candidates.contains(&spare)));
            engine::step(&mut state, Action::ChooseCostTarget(creature)).unwrap();
            paid_priority(&mut state, actor);
            assert_eq!(state.objects.get(creature).zone, Zone::Graveyard);
            crate::ravenous_observation_v51::tests::assert_current(&state);
            let hand = state.players[actor.index()].hand.len();
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            let treasure = crate::card_def::card_id_by_name("Treasure Token").unwrap();
            for branch in [&mut state, &mut restored] {
                finish(branch);
                assert_eq!(branch.objects.get(creature).zone, Zone::Battlefield);
                assert!(branch.objects.get(creature).tapped);
                assert_eq!(engine::effective_power(branch, creature), 1);
                assert_eq!(branch.players[actor.index()].hand.len(), hand + 1);
                assert_eq!(branch.objects.get(amulet).counters.soul, 1);
                assert_eq!(
                    branch.players[actor.index()]
                        .battlefield
                        .iter()
                        .filter(|object| {
                            let live = branch.objects.get(**object);
                            live.card_def == treasure && live.v4.is_token
                        })
                        .count(),
                    1
                );
                crate::doubling_season_v48::validate_token_journals(branch).unwrap();
                crate::fake_your_own_death_v50::spell_custody::validate_retained_grant_history(
                    branch,
                )
                .unwrap();
                crate::ravenous_observation_v51::tests::assert_current(branch);
                cross_family_original_world_refusals(branch, actor, creature);
            }
            assert_eq!(state, restored);
        }
    }

    #[test]
    fn actual_paid_amulet_keeps_genuine_banishing_aura_link_and_enclosing_restore_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            // This existing helper owns the genuine ETB/exile/capture path.
            // Its permanent entry is a fixture operand, not a paid Banishing cast.
            let (linked, banishing, aura, host) =
                crate::banishing_light_v1::tests::linked_aura(actor);
            let (mut state, amulet) = cast_amulet_in(linked, actor);
            let chosen = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let spare = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let forest = put(&mut state, actor, "Forest", Zone::Battlefield);
            paid_priority(&mut state, actor);
            engine::step(&mut state, Action::ActivateManaAbility(forest)).unwrap();
            engine::step(&mut state, Action::ActivateAbility(amulet, 0)).unwrap();
            assert!(matches!(engine::advance_until_decision(&mut state),
                Decision::ChooseCostTargets { ref candidates, .. }
                    if candidates.contains(&chosen) && candidates.contains(&spare)));
            engine::step(&mut state, Action::ChooseCostTarget(chosen)).unwrap();
            paid_priority(&mut state, actor);
            let hand = state.players[actor.index()].hand.len();
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut restored] {
                finish(branch);
                assert_eq!(branch.objects.get(chosen).zone, Zone::Graveyard);
                assert_eq!(branch.objects.get(amulet).counters.soul, 1);
                assert_eq!(branch.players[actor.index()].hand.len(), hand + 1);
                assert_eq!(branch.objects.get(banishing).zone, Zone::Battlefield);
                assert_eq!(branch.objects.get(aura).zone, Zone::Exile);
                assert_eq!(branch.objects.get(host).zone, Zone::Battlefield);
                crate::banishing_light_v1::validate_state(branch).unwrap();
                crate::doubling_season_v48::validate_token_journals(branch).unwrap();
                let removed = branch
                    .engine
                    .event_history
                    .iter()
                    .find_map(|event| match event {
                        CommittedEvent::RavenousRemovedV1 { removed } => Some(removed),
                        _ => None,
                    })
                    .unwrap();
                let borrow = serde_json::to_value(removed.replay_borrow.as_ref().unwrap()).unwrap();
                assert!(borrow["enclosing"].is_object());
                completed_refusals(branch, removed.item.v4.stack_item_id);
            }
            assert_eq!(state, restored);
        }
    }

    #[test]
    fn actual_first_ability_refuses_nonempty_stack_while_zero_soul_second_ability_is_instant_speed_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, amulet) = cast_amulet(actor);
            put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let spell = put(&mut state, actor, "Think Twice", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 5;
            state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
            engine::step(&mut state, Action::CastSpell(spell)).unwrap();
            paid_priority(&mut state, actor);
            assert_eq!(state.stack.len(), 1);
            assert_eq!(
                state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
                4
            );
            let before = state.clone();
            assert!(engine::step(&mut state, Action::ActivateAbility(amulet, 0)).is_err());
            assert_eq!(state, before);
            let original = AbilitySourceContractV4::capture(&state, amulet);
            let life = state.players.each_ref().map(|p| p.life);
            let hand = state.players[actor.index()].hand.len();
            engine::step(&mut state, Action::ActivateAbility(amulet, 1)).unwrap();
            paid_priority(&mut state, actor);
            assert_eq!(state.stack.len(), 2);
            assert_eq!(
                state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
                0
            );
            assert_eq!(state.objects.get(amulet).zone, Zone::Graveyard);
            assert_eq!(state.soul_count_for_ability_source_v1(original), Ok(0));
            let payments = state
                .engine
                .event_history
                .iter()
                .filter_map(|event| match event {
                    CommittedEvent::RavenousPaymentV1 { paid } => Some(paid.as_ref()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(payments.len(), 1);
            assert_eq!(payments[0].original_source, original);
            assert_eq!(payments[0].original_soul_count, Some(0));
            assert_eq!(payments[0].item.v4.activated_ability_index, Some(1));
            finish(&mut state);
            assert_eq!(state.players.each_ref().map(|p| p.life), life);
            assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
            assert_eq!(state.players[actor.index()].hand.len(), hand + 1);
        }
    }
}
pub(crate) fn is_sphinxs_tutelage_v1(card_def: u16) -> bool {
    crate::card_def::CARD_DEFS
        .get(usize::from(card_def))
        .is_some_and(|card| card.is_executable() && card.name == "Sphinx's Tutelage")
}

/// Native mutable-world structural validation, not authority over coordinated
/// raw-history edits. Departed observers retain their event-time controller.
pub(crate) fn tutelage_observers_v1(
    state: &GameState,
    capture_history_index: u64,
) -> Result<&[crate::state::AbilitySourceContractV4], String> {
    let index =
        usize::try_from(capture_history_index).map_err(|_| "Tutelage capture index overflow")?;
    let Some(CommittedEvent::SphinxsTutelageDrawV1 {
        draw_history_index,
        observers,
    }) = state.engine.event_history.get(index)
    else {
        return Err("Tutelage draw capture missing".into());
    };
    let draw_index =
        usize::try_from(*draw_history_index).map_err(|_| "Tutelage draw index overflow")?;
    let Some(CommittedEvent::Draw {
        player,
        object: Some(drawn),
    }) = state.engine.event_history.get(draw_index)
    else {
        return Err("Tutelage indexed draw changed".into());
    };
    if state
        .engine
        .event_history
        .iter()
        .filter(|entry| {
            matches!(entry,
        CommittedEvent::SphinxsTutelageDrawV1 { draw_history_index: found, .. }
        if found == draw_history_index)
        })
        .count()
        != 1
        || draw_index.checked_add(1) != Some(index)
        || observers.is_empty()
        || state
            .objects
            .try_get(*drawn)
            .is_none_or(|object| object.owner != *player)
        || !observers
            .windows(2)
            .all(|pair| pair[0].source < pair[1].source)
    {
        return Err("Tutelage draw roster/index changed".into());
    }
    for source in observers {
        let object = state
            .objects
            .try_get(source.source)
            .ok_or("Tutelage source missing")?;
        if !is_sphinxs_tutelage_v1(source.card_def)
            || source.zone != Zone::Battlefield
            || source.controller != *player
            || source.owner != object.owner
            || !crate::engine::source_card_def_matches(state, *source, object.card_def)
            || object.zone_change_count < source.zone_change_count
            || (object.zone_change_count == source.zone_change_count && object.zone != source.zone)
        {
            return Err("Tutelage captured source changed".into());
        }
    }
    Ok(observers)
}

pub(crate) fn collect_tutelage_v1(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Result<Vec<crate::trigger::PendingTrigger>, String> {
    let mut result = Vec::new();
    let mut consumed = Vec::new();
    for event in events {
        let CommittedEvent::SphinxsTutelageDrawV1 {
            draw_history_index, ..
        } = event
        else {
            continue;
        };
        let index = draw_history_index
            .checked_add(1)
            .ok_or("Tutelage capture overflow")?;
        if state.engine.event_history.get(usize::try_from(index).map_err(|_| "Tutelage index overflow")?) != Some(event)
            || consumed.contains(&index)
            || state.engine.event_history.iter().any(|entry| matches!(entry,
                CommittedEvent::SphinxsTutelageCollectedV1 { capture_history_index } if *capture_history_index == index))
        { return Err("Tutelage capture erased, duplicated or recollected".into()); }
        for source in tutelage_observers_v1(state, index)? {
            result.push(crate::trigger::PendingTrigger {
                controller: source.controller,
                source: source.source,
                effect: crate::effect::EffectOp::SphinxsTutelageMillV1 {
                    capture_history_index: index,
                },
                is_madness_offer: false,
                kicked: false,
                target_spec: crate::card_def::TargetSpec::TargetOpponent,
                targets: Vec::new(),
                target_contracts: Vec::new(),
                placement_ordered: false,
                source_contract: Some(*source),
                granted_by: None,
                optional_additional_cost_paid: None,
                paid_cost_refs: Vec::new(),
            });
        }
        consumed.push(index);
    }
    for capture_history_index in consumed {
        state
            .engine
            .event_history
            .push(CommittedEvent::SphinxsTutelageCollectedV1 {
                capture_history_index,
            });
    }
    Ok(result)
}

pub(crate) fn validate_tutelage_trigger_v1(
    state: &GameState,
    item: &crate::state::StackItem,
) -> Result<u64, String> {
    let Some(crate::effect::EffectOp::SphinxsTutelageMillV1 {
        capture_history_index,
    }) = item.inline_effect.as_ref()
    else {
        return Err("Tutelage trigger changed its captured program".into());
    };
    let source = item
        .v4
        .ability_source_contract
        .ok_or("Tutelage lost original source")?;
    if item.kind != crate::state::StackItemKind::TriggeredAbility
        || source.source != item.source || source.controller != item.controller
        || item.is_copy
        || item.is_flashback
        || item.v4.x_value != 0
        || item.v4.activated_ability_index.is_some() || item.kicked || item.mode_chosen != 0 || item.madness_offer
        || item.v4.granted_by.is_some() || item.v4.cauldron_grant.0.is_some()
        || !item.discarded.is_empty() || !item.v4.paid_cost_refs.is_empty()
        || item.v4.optional_additional_cost_paid.is_some()
        || item.v4.target_spec != Some(crate::card_def::TargetSpec::TargetOpponent)
        || !tutelage_observers_v1(state, *capture_history_index)?.contains(&source)
        || state.engine.event_history.iter().filter(|entry| matches!(entry,
            CommittedEvent::SphinxsTutelageCollectedV1 { capture_history_index: found } if found == capture_history_index)).count() != 1
    { return Err("Tutelage trigger source/controller/recipe changed".into()); }
    Ok(*capture_history_index)
}

pub(crate) fn publish_tutelage_v1(
    state: &mut GameState,
    item: &crate::state::StackItem,
) -> Result<(), String> {
    if !matches!(
        item.inline_effect,
        Some(crate::effect::EffectOp::SphinxsTutelageMillV1 { .. })
    ) {
        return Ok(());
    }
    let capture_history_index = validate_tutelage_trigger_v1(state, item)?;
    if state.engine.event_history.iter().any(|entry| matches!(entry,
        CommittedEvent::SphinxsTutelagePublishedV1 { capture_history_index: capture, item: old }
        if *capture == capture_history_index && old.v4.ability_source_contract == item.v4.ability_source_contract))
    { return Err("Tutelage trigger was already published".into()); }
    state
        .engine
        .event_history
        .push(CommittedEvent::SphinxsTutelagePublishedV1 {
            capture_history_index,
            item: Box::new(item.clone()),
        });
    Ok(())
}

pub(crate) fn validate_published_tutelage_v1(
    state: &GameState,
    item: &crate::state::StackItem,
) -> Result<(), String> {
    let capture = validate_tutelage_trigger_v1(state, item)?;
    let publications = state
        .engine
        .event_history
        .iter()
        .filter_map(|entry| match entry {
            CommittedEvent::SphinxsTutelagePublishedV1 {
                capture_history_index,
                item: published,
            } if published.v4.stack_item_id == item.v4.stack_item_id => {
                Some((*capture_history_index, published.as_ref()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if publications.as_slice() != [(capture, item)] {
        return Err("Tutelage publication/targets changed".into());
    }
    Ok(())
}

pub(crate) fn is_lathril_v1(card_def: u16) -> bool {
    crate::card_def::CARD_DEFS
        .get(usize::from(card_def))
        .is_some_and(|card| card.is_executable() && card.name == "Lathril, Blade of the Elves")
}

// Structural native restore validation. This is not a signature over a caller's
// coordinated edits to an entire raw world/history.
pub(crate) fn lathril_damage_v1(
    state: &GameState,
    capture_history_index: u64,
) -> Result<(crate::state::AbilitySourceContractV4, u32), String> {
    let index =
        usize::try_from(capture_history_index).map_err(|_| "Lathril capture index overflow")?;
    let Some(CommittedEvent::LathrilCombatDamageV1 {
        damage_history_index,
        source,
        player,
        amount: captured_amount,
    }) = state.engine.event_history.get(index)
    else {
        return Err("Lathril capture missing".into());
    };
    let damage_index =
        usize::try_from(*damage_history_index).map_err(|_| "Lathril damage index overflow")?;
    let Some(CommittedEvent::Damage {
        source: actual,
        target: Target::Player(recipient),
        amount,
    }) = state.engine.event_history.get(damage_index)
    else {
        return Err("Lathril original damage changed".into());
    };
    // Only Niv's event-time observers can occur between this Damage and this
    // capture. No unrelated event, second damage or substituted source is lent.
    if damage_index >= index
        || *actual != source.source
        || *amount <= 0
        || amount != captured_amount
        || recipient != player
        || !state.engine.event_history[damage_index + 1..index]
            .iter()
            .all(|entry| {
                matches!(entry,
            CommittedEvent::NivMizzetVisionaryDamageV1 { damage_history_index: other, .. }
            if other == damage_history_index)
            })
        || state
            .engine
            .event_history
            .iter()
            .filter(|entry| {
                matches!(entry,
            CommittedEvent::LathrilCombatDamageV1 { damage_history_index: other, .. }
            if other == damage_history_index)
            })
            .count()
            != 1
        || source.zone != Zone::Battlefield
        || !is_lathril_v1(source.card_def)
        || state.objects.try_get(source.source).is_none_or(|object| {
            object.owner != source.owner
                || !crate::engine::source_card_def_matches(state, *source, object.card_def)
                || object.zone_change_count < source.zone_change_count
                || (object.zone_change_count == source.zone_change_count
                    && object.zone != source.zone)
        })
    {
        return Err("Lathril original combat/source capture changed".into());
    }
    let markers = state
        .engine
        .event_history
        .iter()
        .filter_map(|entry| match entry {
            CommittedEvent::LathrilCombatObservedV1 {
                capture_history_index: capture,
                marker_history_index,
            } if *capture == capture_history_index => Some(*marker_history_index),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [marker] = markers.as_slice() else {
        return Err("Lathril combat handoff missing or duplicated".into());
    };
    let marker_index =
        usize::try_from(*marker).map_err(|_| "Lathril combat marker index overflow")?;
    if marker_index <= index
        || state.engine.event_history.get(marker_index) != Some(&CommittedEvent::CombatDamageToPlayer {
            source: source.source, source_zone_change_count: source.zone_change_count,
            player: *player, amount: *amount,
        })
        || state.engine.event_history.iter().filter(|entry| matches!(entry,
            CommittedEvent::LathrilCombatObservedV1 { marker_history_index, .. } if marker_history_index == marker)).count() != 1 {
        return Err("Lathril exact native combat marker changed".into());
    }
    Ok((
        *source,
        u32::try_from(*amount).map_err(|_| "Lathril damage width overflow")?,
    ))
}

pub(crate) fn collect_lathril_v1(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Result<Vec<crate::trigger::PendingTrigger>, String> {
    let mut captures = Vec::new();
    let mut result = Vec::new();
    for event in events {
        if !matches!(event, CommittedEvent::LathrilCombatDamageV1 { .. }) {
            continue;
        }
        let positions = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(index, actual)| (actual == event).then_some(index as u64))
            .collect::<Vec<_>>();
        let [index] = positions.as_slice() else {
            return Err("Lathril capture erased or duplicated".into());
        };
        if captures.contains(index) || state.engine.event_history.iter().any(|entry| matches!(entry,
            CommittedEvent::LathrilCombatCollectedV1 { capture_history_index } if capture_history_index == index))
        { return Err("Lathril capture recollected".into()); }
        let (source, _) = lathril_damage_v1(state, *index)?;
        result.push(crate::trigger::PendingTrigger {
            controller: source.controller,
            source: source.source,
            effect: crate::effect::EffectOp::LathrilCombatTokensV1 {
                capture_history_index: *index,
            },
            is_madness_offer: false,
            kicked: false,
            target_spec: crate::card_def::TargetSpec::None,
            targets: Vec::new(),
            target_contracts: Vec::new(),
            placement_ordered: false,
            source_contract: Some(source),
            granted_by: None,
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        });
        captures.push(*index);
    }
    for capture_history_index in captures {
        state
            .engine
            .event_history
            .push(CommittedEvent::LathrilCombatCollectedV1 {
                capture_history_index,
            });
    }
    Ok(result)
}

pub(crate) fn validate_lathril_trigger_v1(
    state: &GameState,
    item: &crate::state::StackItem,
) -> Result<u64, String> {
    let Some(crate::effect::EffectOp::LathrilCombatTokensV1 {
        capture_history_index,
    }) = item.inline_effect.as_ref()
    else {
        return Err("Lathril captured program changed".into());
    };
    let (source, _) = lathril_damage_v1(state, *capture_history_index)?;
    if item.kind != crate::state::StackItemKind::TriggeredAbility
        || item.source != source.source
        || item.controller != source.controller
        || item.v4.ability_source_contract != Some(source)
        || item.is_copy
        || item.is_flashback
        || item.kicked
        || item.madness_offer
        || item.mode_chosen != 0
        || item.v4.x_value != 0
        || item.v4.activated_ability_index.is_some()
        || item.v4.granted_by.is_some()
        || item.v4.cauldron_grant.0.is_some()
        || item.v4.hidden_ability_source.is_some()
        || item.v4.source_contract.is_some()
        || item.v4.madness_source_contract.is_some()
        || item.v4.target_spec.is_some()
        || !item.targets.is_empty()
        || !item.v4.target_contracts.is_empty()
        || !item.discarded.is_empty()
        || !item.v4.paid_cost_refs.is_empty()
        || item.v4.optional_additional_cost_paid.is_some()
        || state
            .engine
            .event_history
            .iter()
            .filter(|entry| {
                matches!(entry,
            CommittedEvent::LathrilCombatCollectedV1 { capture_history_index: other }
            if other == capture_history_index)
            })
            .count()
            != 1
        || state.engine.event_history.iter().any(|entry| {
            matches!(entry,
            CommittedEvent::LathrilCombatResolvedV1 { capture_history_index: other }
                | CommittedEvent::LathrilCombatCounteredV1 { capture_history_index: other }
            if other == capture_history_index)
        })
    {
        return Err("Lathril trigger source/controller/recipe changed or consumed".into());
    }
    Ok(*capture_history_index)
}

pub(crate) fn publish_lathril_v1(
    state: &mut GameState,
    item: &crate::state::StackItem,
) -> Result<(), String> {
    if !matches!(
        item.inline_effect,
        Some(crate::effect::EffectOp::LathrilCombatTokensV1 { .. })
    ) {
        return Ok(());
    }
    let capture_history_index = validate_lathril_trigger_v1(state, item)?;
    if state.engine.event_history.iter().any(|entry| matches!(entry,
        CommittedEvent::LathrilCombatPublishedV1 { capture_history_index: other, .. } if *other == capture_history_index))
    { return Err("Lathril damage trigger already published".into()); }
    state
        .engine
        .event_history
        .push(CommittedEvent::LathrilCombatPublishedV1 {
            capture_history_index,
            item: Box::new(item.clone()),
        });
    Ok(())
}

pub(crate) fn validate_published_lathril_v1(
    state: &GameState,
    item: &crate::state::StackItem,
) -> Result<(), String> {
    let capture = validate_lathril_trigger_v1(state, item)?;
    let publications = state
        .engine
        .event_history
        .iter()
        .filter_map(|entry| match entry {
            CommittedEvent::LathrilCombatPublishedV1 {
                capture_history_index,
                item: row,
            } if *capture_history_index == capture
                || row.v4.stack_item_id == item.v4.stack_item_id =>
            {
                Some((*capture_history_index, row.as_ref()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if publications.as_slice() != [(capture, item)] {
        return Err("Lathril original publication changed".into());
    }
    Ok(())
}

pub(crate) fn lathril_counter_capture_v1(
    state: &GameState,
    item: &crate::state::StackItem,
) -> Result<Option<u64>, String> {
    if !lathril_item_selected_v1(state, item) {
        return Ok(None);
    }
    validate_published_lathril_v1(state, item)?;
    validate_lathril_trigger_v1(state, item).map(Some)
}

pub(crate) fn lathril_item_selected_v1(state: &GameState, item: &crate::state::StackItem) -> bool {
    matches!(
        item.inline_effect,
        Some(crate::effect::EffectOp::LathrilCombatTokensV1 { .. })
    ) || state.engine.event_history.iter().any(|entry| {
        matches!(entry,
            CommittedEvent::LathrilCombatPublishedV1 { item: original, .. }
            if original.v4.stack_item_id == item.v4.stack_item_id)
    })
}

/// Only the engine's already-committed combat batch calls this handoff. Equal
/// packets consume matching event-time captures in their original native order.
pub(crate) fn bind_lathril_combat_marker_v1(
    state: &mut GameState,
    captures: &[CommittedEvent],
    marker_history_index: u64,
) -> Result<(), String> {
    let index =
        usize::try_from(marker_history_index).map_err(|_| "Lathril marker index overflow")?;
    let Some(CommittedEvent::CombatDamageToPlayer {
        source,
        source_zone_change_count,
        player,
        amount,
    }) = state.engine.event_history.get(index)
    else {
        return Err("Lathril native combat marker missing".into());
    };
    let mut selected = None;
    for capture in captures {
        let CommittedEvent::LathrilCombatDamageV1 {
            source: observer,
            player: recipient,
            amount: actual,
            ..
        } = capture
        else {
            return Err("Lathril handoff contains a foreign event".into());
        };
        if observer.source != *source
            || observer.zone_change_count != *source_zone_change_count
            || recipient != player
            || actual != amount
        {
            continue;
        }
        let positions = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(i, entry)| (entry == capture).then_some(i as u64))
            .collect::<Vec<_>>();
        let [position] = positions.as_slice() else {
            return Err("Lathril handoff capture erased or duplicated".into());
        };
        if !state.engine.event_history.iter().any(|entry| matches!(entry,
            CommittedEvent::LathrilCombatObservedV1 { capture_history_index, .. } if capture_history_index == position)) {
            selected = Some(*position);
            break;
        }
    }
    if let Some(capture_history_index) = selected {
        state
            .engine
            .event_history
            .push(CommittedEvent::LathrilCombatObservedV1 {
                capture_history_index,
                marker_history_index,
            });
        lathril_damage_v1(state, capture_history_index)?;
    }
    Ok(())
}

// Persistent copiable token abilities. These receipts are native-only; no old
// observer is allowed to discard their ability multiplicity or history.
fn record_copied_token_entry_v1(state: &mut GameState, creation: usize) {
    let Some(CommittedEvent::CreateToken { object, .. }) = state.engine.event_history.get(creation)
    else {
        return;
    };
    let object = *object;
    let instances = state.objects.get(object).v4.copied_end_step_sacrifices_v1;
    if instances == 0 {
        return;
    }
    let source = crate::state::AbilitySourceContractV4::capture(state, object);
    state
        .engine
        .event_history
        .push(CommittedEvent::CopiedTokenAbilitiesEnteredV1 {
            source,
            creation_history_index: creation as u64,
            instances,
        });
}
fn copied_token_entry_v1(
    state: &GameState,
    source: crate::state::AbilitySourceContractV4,
) -> Result<u32, String> {
    let rows = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, e)| match e {
            CommittedEvent::CopiedTokenAbilitiesEnteredV1 {
                source: actual,
                creation_history_index,
                instances,
            } if actual.source == source.source
                && actual.zone_change_count == source.zone_change_count =>
            {
                Some((at, *actual, *creation_history_index, *instances))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(at, original, creation, count)] = rows.as_slice() else {
        return Err("copied token abilities lost unique entry".into());
    };
    crate::engine::validate_historical_ability_source_contract(state, source)?;
    if *count == 0
        || source.zone != Zone::Battlefield
        || source.zone_change_count != 0
        || original.owner != source.owner
        || original.card_def != source.card_def
        || original.zone != Zone::Battlefield
        || creation.checked_add(1) != Some(*at as u64)
        || !matches!(state.engine.event_history.get(*creation as usize),Some(CommittedEvent::CreateToken {object,token_def,controller,..})
            if *object == source.source && *token_def == source.card_def && *controller == original.controller)
    {
        return Err("copied token entry changed its actual creation/incarnation".into());
    }
    Ok(*count)
}
pub(crate) fn record_copied_token_end_step_v1(
    state: &mut GameState,
    end_step: usize,
) -> Result<(), String> {
    if !matches!(
        state.engine.event_history.get(end_step),
        Some(CommittedEvent::BeginningEndStep { .. })
    ) {
        return Err("copied ability requires actual beginning end step".into());
    }
    let mut sources = Vec::new();
    for (id, o) in state.objects.iter() {
        if o.zone != Zone::Battlefield
            || o.v4.copied_end_step_sacrifices_v1 == 0
            || !crate::continuous_characteristics_v1::printed_abilities_active(state, id)
        {
            continue;
        }
        let source = crate::state::AbilitySourceContractV4::capture(state, id);
        let count = copied_token_entry_v1(state, source)?;
        if !o.v4.is_token || count != o.v4.copied_end_step_sacrifices_v1 {
            return Err("copied ability count differs from original entry".into());
        }
        sources.push((source, count));
    }
    if !sources.is_empty() {
        let receipt = CommittedEvent::CopiedTokenEndStepV1 {
            end_step_history_index: end_step as u64,
            sources,
        };
        state.engine.event_log.push(receipt.clone());
        state.engine.event_history.push(receipt);
    }
    Ok(())
}
fn copied_end_sources_v1(
    state: &GameState,
    capture: u64,
) -> Result<&[(crate::state::AbilitySourceContractV4, u32)], String> {
    let Some(CommittedEvent::CopiedTokenEndStepV1 {
        end_step_history_index,
        sources,
    }) = state.engine.event_history.get(capture as usize)
    else {
        return Err("copied end-step capture absent".into());
    };
    if sources.is_empty() || end_step_history_index.checked_add(1) != Some(capture) || !matches!(state.engine.event_history.get(*end_step_history_index as usize),Some(CommittedEvent::BeginningEndStep{..}))
        || state.engine.event_history.iter().filter(|e|matches!(e,CommittedEvent::CopiedTokenEndStepV1{end_step_history_index:other,..} if other==end_step_history_index)).count()!=1
    {return Err("copied end-step capture changed actual event".into());}
    for (i, (source, count)) in sources.iter().enumerate() {
        if *count == 0
            || copied_token_entry_v1(state, *source)? != *count
            || sources[..i].iter().any(|(s, _)| s.source == source.source)
        {
            return Err("copied end-step source roster changed".into());
        }
    }
    Ok(sources)
}
pub(crate) fn collect_copied_token_end_step_v1(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Result<Vec<crate::trigger::PendingTrigger>, String> {
    let mut consumed = Vec::new();
    let mut pending = Vec::new();
    for event in events {
        if !matches!(event, CommittedEvent::CopiedTokenEndStepV1 { .. }) {
            continue;
        }
        let positions = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(i, e)| (e == event).then_some(i as u64))
            .collect::<Vec<_>>();
        let [capture] = positions.as_slice() else {
            return Err("copied end-step receipt erased or duplicated".into());
        };
        if consumed.contains(capture) || state.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::CopiedTokenEndStepCollectedV1{capture_history_index} if capture_history_index==capture)) {return Err("copied end-step receipt replayed".into());}
        for (observer, (source, count)) in
            copied_end_sources_v1(state, *capture)?.iter().enumerate()
        {
            let observer = u32::try_from(observer).map_err(|_| "copied ability observer width")?;
            pending
                .try_reserve(*count as usize)
                .map_err(|_| "copied ability roster allocation")?;
            for ability in 0..*count {
                pending.push(crate::trigger::PendingTrigger {
                    controller: source.controller,
                    source: source.source,
                    effect: crate::effect::EffectOp::CopiedTokenEndStepSacrificeV1 {
                        capture_history_index: *capture,
                        observer,
                        ability,
                    },
                    is_madness_offer: false,
                    kicked: false,
                    target_spec: crate::card_def::TargetSpec::None,
                    targets: Vec::new(),
                    target_contracts: Vec::new(),
                    placement_ordered: false,
                    source_contract: Some(*source),
                    granted_by: None,
                    optional_additional_cost_paid: None,
                    paid_cost_refs: Vec::new(),
                });
            }
        }
        consumed.push(*capture);
    }
    for capture_history_index in consumed {
        state
            .engine
            .event_history
            .push(CommittedEvent::CopiedTokenEndStepCollectedV1 {
                capture_history_index,
            });
    }
    Ok(pending)
}
pub(crate) fn copied_token_trigger_v1(
    state: &GameState,
    item: &crate::state::StackItem,
) -> Result<(u64, u32, u32, crate::state::AbilitySourceContractV4), String> {
    let Some(crate::effect::EffectOp::CopiedTokenEndStepSacrificeV1 {
        capture_history_index,
        observer,
        ability,
    }) = item.inline_effect.as_ref()
    else {
        return Err("copied token trigger root changed".into());
    };
    let (source, count) = *copied_end_sources_v1(state, *capture_history_index)?
        .get(*observer as usize)
        .ok_or("copied ability observer missing")?;
    if *ability>=count || item.kind!=crate::state::StackItemKind::TriggeredAbility || item.source!=source.source || item.controller!=source.controller
        || item.v4.ability_source_contract!=Some(source) || item.v4.source_contract.is_some() || item.v4.granted_by.is_some()
        || item.v4.activated_ability_index.is_some() || item.v4.hidden_ability_source.is_some() || item.v4.madness_source_contract.is_some()
        || item.v4.cauldron_grant.0.is_some() || item.v4.cast_method.is_some() || item.v4.target_spec.is_some() || item.v4.face_index!=0
        || item.mode_chosen!=0 || item.is_copy || item.is_flashback || item.kicked || item.madness_offer || item.v4.x_value!=0
        || !item.targets.is_empty() || !item.v4.target_contracts.is_empty() || !item.discarded.is_empty() || !item.v4.paid_cost_refs.is_empty() || item.v4.optional_additional_cost_paid.is_some()
        || state.engine.event_history.iter().filter(|e|matches!(e,CommittedEvent::CopiedTokenEndStepCollectedV1{capture_history_index:other} if other==capture_history_index)).count()!=1
        || state.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::CopiedTokenSacrificeFinishedV1{stack_item_id} if *stack_item_id==item.v4.stack_item_id && item.v4.stack_item_id!=crate::ids::StackItemId::default()))
    {return Err("copied token trigger source/controller/instance changed or consumed".into());}
    Ok((*capture_history_index, *observer, *ability, source))
}
pub(crate) fn publish_copied_token_trigger_v1(
    state: &mut GameState,
    item: &crate::state::StackItem,
) -> Result<(), String> {
    if !matches!(
        item.inline_effect,
        Some(crate::effect::EffectOp::CopiedTokenEndStepSacrificeV1 { .. })
    ) {
        return Ok(());
    }
    let (capture_history_index, observer, ability, _) = copied_token_trigger_v1(state, item)?;
    if state.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::CopiedTokenSacrificePublishedV1{capture_history_index:c,observer:o,ability:a,item:row} if (*c,*o,*a)==(capture_history_index,observer,ability) || row.v4.stack_item_id==item.v4.stack_item_id)) {return Err("copied token trigger published twice".into());}
    state
        .engine
        .event_history
        .push(CommittedEvent::CopiedTokenSacrificePublishedV1 {
            capture_history_index,
            observer,
            ability,
            item: Box::new(item.clone()),
        });
    Ok(())
}
pub(crate) fn copied_token_item_selected_v1(
    state: &GameState,
    item: &crate::state::StackItem,
) -> bool {
    matches!(item.inline_effect,Some(crate::effect::EffectOp::CopiedTokenEndStepSacrificeV1{..}))
        || state.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::CopiedTokenSacrificePublishedV1{item:row,..} if row.v4.stack_item_id==item.v4.stack_item_id))
}
pub(crate) fn validate_published_copied_token_v1(
    state: &GameState,
    item: &crate::state::StackItem,
) -> Result<(), String> {
    let (capture, observer, ability, _) = copied_token_trigger_v1(state, item)?;
    let rows = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            CommittedEvent::CopiedTokenSacrificePublishedV1 {
                capture_history_index: c,
                observer: o,
                ability: a,
                item: row,
            } if (*c, *o, *a) == (capture, observer, ability)
                || row.v4.stack_item_id == item.v4.stack_item_id =>
            {
                Some(row.as_ref())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if rows.as_slice() != [item] {
        return Err("copied token trigger changed original published row".into());
    }
    Ok(())
}

pub(crate) fn historical_copiable_token_abilities_v1(
    state: &GameState,
    source: crate::state::AbilitySourceContractV4,
) -> Result<u32, String> {
    if state.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::CopiedTokenAbilitiesEnteredV1{source: original,..} if original.source==source.source && original.zone_change_count==source.zone_change_count)) {
        copied_token_entry_v1(state, source)
    } else {
        Ok(0)
    }
}
pub(crate) fn copiable_token_abilities_v1(
    state: &GameState,
    object: ObjectId,
) -> Result<u32, String> {
    let live = state.objects.try_get(object).ok_or("copy source absent")?;
    if live.v4.face_down_v1.is_some() {
        return Ok(0);
    }
    if live.v4.copied_end_step_sacrifices_v1 == 0 {
        if state.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::CopiedTokenAbilitiesEnteredV1{source,..} if source.source==object && source.zone_change_count==live.zone_change_count)) {
            return Err("copy source erased its copiable ability count".into());
        }
        return Ok(0);
    }
    let count = copied_token_entry_v1(
        state,
        crate::state::AbilitySourceContractV4::capture(state, object),
    )?;
    if live.zone != Zone::Battlefield
        || !live.v4.is_token
        || count != live.v4.copied_end_step_sacrifices_v1
    {
        return Err("copy source changed its copiable ability count".into());
    }
    Ok(count)
}

pub(crate) fn validate_copied_token_abilities_v1(state: &GameState) -> Result<(), String> {
    if crate::linked_aura_operation_v1::locally_owned(state) {
        return Ok(());
    }
    for (_, object) in state.objects.iter() {
        if object.zone != Zone::Battlefield && object.v4.copied_end_step_sacrifices_v1 != 0 {
            return Err("copied abilities survived an ordinary zone change".into());
        }
    }
    for event in &state.engine.event_history {
        let CommittedEvent::CopiedTokenAbilitiesEnteredV1 {
            source, instances, ..
        } = event
        else {
            continue;
        };
        if copied_token_entry_v1(state, *source)? != *instances {
            return Err("copied ability entry changed".into());
        }
        if let Some(live) = state.objects.try_get(source.source) {
            if live.zone == Zone::Battlefield
                && live.zone_change_count == source.zone_change_count
                && (live.v4.copied_end_step_sacrifices_v1 != *instances || !live.v4.is_token)
            {
                return Err("live copied token lost its original ability multiplicity".into());
            }
        }
        if matches!(state.engine.event_log.iter().find(|e| *e == event), Some(_)) {
            return Err("private copied entry receipt exposed as an event".into());
        }
    }
    for (id, live) in state.objects.iter() {
        if live.v4.copied_end_step_sacrifices_v1 != 0 {
            if copied_token_entry_v1(
                state,
                crate::state::AbilitySourceContractV4::capture(state, id),
            )? != live.v4.copied_end_step_sacrifices_v1
                || !live.v4.is_token
            {
                return Err("native copied token count lacks its actual entry".into());
            }
        }
    }
    Ok(())
}

fn unsupported_partial_token_v1(event: &ProposedEvent) -> bool {
    matches!(event,ProposedEvent::CreateToken(t) if crate::card_def::CARD_DEFS.get(usize::from(t.token_def)).is_none_or(|d| d.is_token && !d.has_full_support()))
}
pub(crate) fn commit_kykar_spirit_owned_v1(
    state: &mut GameState,
    permit: crate::effect::kykar_v1::TokenPermitV1<'_, '_>,
) -> Result<(), String> {
    let events = permit.events();
    if !permit.allows(state, &events) {
        return Err("Kykar Spirit lost genuine mode owner".into());
    }
    batch_with_aura_token_hosts_v1(state, events, Vec::new(), None, None, Some(&permit), None);
    if state.engine.halted.is_some() {
        return Err("Kykar Spirit batch halted".into());
    }
    Ok(())
}

/// Consumes a distinct lexical spell-copy permit. Raw Partial-token events
/// and Kykar's printed mode retain their original gates.
pub(crate) fn commit_spirit_copy_owned_v1(
    state: &mut GameState,
    permit: crate::effect::SpiritCopyPermitV1<'_, '_>,
) -> Result<(), String> {
    let events = permit.events();
    if !permit.allows(state, &events) {
        return Err("Spirit copy lost its actual spell/target/proposal".into());
    }
    batch_with_aura_token_hosts_v1(state, events, Vec::new(), None, None, None, Some(&permit));
    if state.engine.halted.is_some() {
        return Err("Spirit copy batch halted".into());
    }
    Ok(())
}
