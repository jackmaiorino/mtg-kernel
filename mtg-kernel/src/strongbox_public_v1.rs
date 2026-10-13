//! Separate public Strongbox transport. This is not V8/V9 or tensor/model admission.
use crate::ids::PlayerId;
use crate::policy_observation_v8::PolicyObservationExtensionsV8;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::{
    CardPrivateV1, CardStableRefV1, KnownLibraryCardV4, PlayerSeatV1,
    PublicObservationProjectionV5, RlContractError,
};
use crate::state::{GameState, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/strongbox-public/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootV1 {
    pub card_id: u16,
    pub trigger_index: u16,
    pub root_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrongboxObservationV1 {
    pub schema: String,
    pub producer_sha256: String,
    pub kernel_version: String,
    pub surface_version: u32,
    pub policy_surface_version: u32,
    pub card_db_hash: u64,
    pub acting_player: PlayerSeatV1,
    pub step_index: u64,
    pub physical_decision_id: u64,
    pub substep_index: u32,
    pub substep_count: u32,
    pub projection: PublicObservationProjectionV5,
    pub own_hand: Vec<CardPrivateV1>,
    pub known_library_cards: [Vec<KnownLibraryCardV4>; 2],
    pub known_hand_cards: [Vec<CardPrivateV1>; 2],
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known_face_down_cards: Vec<crate::rl::FaceDownCardKnowledgeV1>,
    pub extensions: PolicyObservationExtensionsV8,
    pub attacked_this_turn: [bool; 2],
    pub entries: Vec<EntryV1>,
    pub selection_menu: Option<SelectionMenuV1>,
    pub roots: Vec<RootV1>,
    pub projection_sha256: String,
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn producer_sha256_v1() -> String {
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("strongbox_public_v1.rs"),
        include_str!("rules_meaning_strongbox_v1.rs"),
        include_str!("effect/exile_top_choose_v1.rs"),
        include_str!("effect/strongbox_permission_v1.rs"),
        include_str!("source_power_activation_v1.rs"),
        include_str!("gorehorn_damage_v1.rs"),
        include_str!("effect/squad_rallier_resolution_v1.rs"),
        include_str!("raid_entry_v1.rs"),
        include_str!("raid_entry_custody_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("../build.rs"),
        include_str!("trigger.rs"),
        include_str!("effect.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("state.rs"),
        include_str!("extra_turn_v1.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("rl.rs"),
        include_str!("rl_session.rs"),
        include_str!("rl_session/strongbox_v32.rs"),
        include_str!("human_bo3_v1/strongbox_v32.rs"),
        include_str!("human_bo3_v1/labels.rs"),
        include_str!("human_bo3_v1/visible.rs"),
        include_str!("human_bo3_v1/life_ward_v8.rs"),
        include_str!("policy_observation_v8.rs"),
        include_str!("policy_observation_v7.rs"),
        include_str!("rl_session/flat_action_v3.rs"),
        include_str!("rl_session/flat_action_v4.rs"),
        include_str!("policy_surface_v5.rs"),
        include_str!("surface_v2.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("standard_statics_v1.rs"),
        include_str!("standard_keywords_v1.rs"),
        include_str!("standard_legends_v1.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn commitment(observation: &StrongboxObservationV1) -> Result<String, RlContractError> {
    let mut value = serde_json::to_value(observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("Strongbox projection is not an object".into()))?
        .remove("projection_sha256");
    Ok(digest(&serde_json::to_vec(&value)?))
}
impl StrongboxObservationV1 {
    /// Checks transport identity only. Execution always rederives the entire
    /// observation and private ordered candidate binding from the owning session.
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.producer_sha256 != producer_sha256_v1()
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError("wrong Strongbox artifact identity".into()));
        }
        let expected = roots()?;
        if self.roots != expected {
            return Err(RlContractError("wrong Strongbox printed root".into()));
        }
        Ok(())
    }
}
fn roots() -> Result<Vec<RootV1>, RlContractError> {
    // Ordinary Default/Standard sessions still have a real successor identity,
    // but never fabricate the missing Limited definition or claim its meaning.
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Ok(Vec::new());
    }
    let Some(id) = crate::card_def::card_id_by_name("Strongbox Raider") else {
        return Ok(Vec::new());
    };
    let selector = crate::rules_meaning_strongbox_v1::live_selector()
        .map_err(|_| RlContractError("Strongbox catalog not supported".into()))?;
    let meaning = crate::rules_meaning_strongbox_v1::meaning(&selector, id, 0)
        .map_err(|_| RlContractError("Strongbox root not supported".into()))?;
    Ok(vec![RootV1 {
        card_id: id,
        trigger_index: 0,
        root_sha256: meaning.root_sha256,
    }])
}
// V40 consumes these private typed parts; standalone predecessors retain their refusal path.
pub(crate) fn parts_for_akroma_v40(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
) -> Result<StrongboxObservationV1, RlContractError> {
    parts_for_common_scope(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        substep,
        crate::rl::CommonProjectionScopeV42::ExistingV40,
    )
}
pub(crate) fn parts_for_ajani_v42(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
) -> Result<StrongboxObservationV1, RlContractError> {
    parts_for_common_scope(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        substep,
        crate::rl::CommonProjectionScopeV42::FullAjaniV42,
    )
}
pub(crate) fn parts_for_faebloom_v43(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
) -> Result<StrongboxObservationV1, RlContractError> {
    parts_for_common_scope(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        substep,
        crate::rl::CommonProjectionScopeV42::FullFaebloomV43,
    )
}
fn parts_for_common_scope(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
    scope: crate::rl::CommonProjectionScopeV42,
) -> Result<StrongboxObservationV1, RlContractError> {
    let (substep_index, substep_count) = substep;

    validate_projection_authority(state)?;
    let base = crate::rl::common_family_observation_storage_scoped(
        (scope, state),
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let base = storage.base;
    let observation = StrongboxObservationV1 {
        schema: String::new(),
        producer_sha256: String::new(),
        kernel_version: base.kernel_version,
        surface_version: base.surface_version,
        policy_surface_version: base.policy_surface_version,
        card_db_hash: base.card_db_hash,
        acting_player: base.acting_player,
        step_index: base.step_index,
        physical_decision_id: base.physical_decision_id,
        substep_index: base.substep_index,
        substep_count: base.substep_count,
        projection: base.projection,
        own_hand: base.own_hand,
        known_library_cards: base.known_library_cards,
        known_hand_cards: base.known_hand_cards,
        known_face_down_cards: base.known_face_down_cards,
        extensions: storage.extensions,
        attacked_this_turn: [
            state.player_attacked_this_turn_v1(PlayerId::P0),
            state.player_attacked_this_turn_v1(PlayerId::P1),
        ],
        entries: entries(state)?,
        selection_menu: selection_menu(state)?,
        roots: roots()?,
        projection_sha256: String::new(),
    };
    Ok(observation)
}
#[allow(clippy::too_many_arguments)]
pub fn observe_v1(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
) -> Result<StrongboxObservationV1, RlContractError> {
    validate_projection_authority(state)?;
    let base = crate::rl::strongbox_observation_storage_v1(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let base = storage.base;
    let mut observation = StrongboxObservationV1 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256_v1(),
        kernel_version: base.kernel_version,
        surface_version: base.surface_version,
        policy_surface_version: base.policy_surface_version,
        card_db_hash: base.card_db_hash,
        acting_player: base.acting_player,
        step_index: base.step_index,
        physical_decision_id: base.physical_decision_id,
        substep_index: base.substep_index,
        substep_count: base.substep_count,
        projection: base.projection,
        own_hand: base.own_hand,
        known_library_cards: base.known_library_cards,
        known_hand_cards: base.known_hand_cards,
        known_face_down_cards: base.known_face_down_cards,
        extensions: storage.extensions,
        attacked_this_turn: [
            state.player_attacked_this_turn_v1(PlayerId::P0),
            state.player_attacked_this_turn_v1(PlayerId::P1),
        ],
        entries: entries(state)?,
        selection_menu: selection_menu(state)?,
        roots: roots()?,
        projection_sha256: String::new(),
    };
    observation.projection_sha256 = commitment(&observation)?;
    Ok(observation)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryV1 {
    pub entry_order: u32,
    pub source_at_entry: CardStableRefV1,
    pub captured_controller: PlayerSeatV1,
    pub pending_trigger_ordinal: Option<u32>,
    pub live_stack_index: Option<u32>,
    pub phase: String,
    pub exiled: Vec<CardStableRefV1>,
    pub selected: Option<CardStableRefV1>,
    pub ordinary_permission: Option<PermissionV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionV1 {
    pub card: CardStableRefV1,
    pub holder: PlayerSeatV1,
    pub play_land: bool,
    pub cast_spell: bool,
    pub next_holder_turn_started: bool,
    pub expired: bool,
    pub exact_current_exile: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionMenuV1 {
    pub source_at_entry: CardStableRefV1,
    pub chooser: PlayerSeatV1,
    pub minimum: u16,
    pub maximum: u16,
    pub can_finish: bool,
    pub legal: Vec<CardStableRefV1>,
    pub selected: Vec<CardStableRefV1>,
}
fn source_ref(source: crate::state::AbilitySourceContractV4) -> CardStableRefV1 {
    CardStableRefV1 {
        arena_id: source.source.0,
        card_db_id: source.card_def,
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: source.zone,
        zone_change_count: source.zone_change_count,
    }
}
pub(crate) fn validate_projection_authority(state: &GameState) -> Result<(), RlContractError> {
    crate::effect::validate_pending_effect_choice(state).map_err(RlContractError)?;
    crate::raid_entry_v1::validate_state(state).map_err(RlContractError)?;
    Ok(())
}
fn selected_capture(c: &crate::raid_entry_custody_v1::CaptureV1) -> bool {
    c.pending.is_some()
        && crate::card_def::CARD_DEFS[usize::from(c.source.card_def)].name == "Strongbox Raider"
}
fn entries(state: &GameState) -> Result<Vec<EntryV1>, RlContractError> {
    use crate::event::CommittedEvent as E;
    let mut rows = Vec::new();
    for event in &state.engine.event_history {
        let E::RaidEntryCaptureV1 { capture } = event else {
            continue;
        };
        // Only eligible captures authenticate a genuinely face-up printed source.
        // Hidden/ineligible card identity never becomes a dynamic manifest row.
        if !selected_capture(capture) {
            continue;
        }
        let source = source_ref(capture.source);
        let publication = state
            .engine
            .event_history
            .iter()
            .find_map(|event| match event {
                E::RaidEntryPublishedV1 { publication } if publication.capture == capture.index => {
                    Some(publication.as_ref())
                }
                _ => None,
            });
        let pending_trigger_ordinal = state
            .engine
            .pending_triggers
            .iter()
            .position(|pending| pending.source_contract == Some(capture.source))
            .and_then(|index| u32::try_from(index).ok());
        let live_stack_index = publication
            .and_then(|p| {
                state
                    .stack
                    .iter()
                    .position(|item| item.v4.stack_item_id == p.item.v4.stack_item_id)
            })
            .and_then(|index| u32::try_from(index).ok());
        let stack = publication.map(|p| p.item.v4.stack_item_id);
        let binding = stack.and_then(|stack| {
            state
                .engine
                .event_history
                .iter()
                .find_map(|event| match event {
                    E::ExilePlaySelectionCapturedV1 {
                        resolving_stack_item,
                        binding,
                    } if *resolving_stack_item == stack => Some(binding.as_ref()),
                    _ => None,
                })
        });
        let consumed = stack.and_then(|stack| {
            state
                .engine
                .event_history
                .iter()
                .find_map(|event| match event {
                    E::ExilePlaySelectionConsumedV1 {
                        resolving_stack_item,
                        selected,
                    } if *resolving_stack_item == stack => Some(*selected),
                    _ => None,
                })
        });
        let installation = stack.and_then(|stack| {
            state
                .engine
                .event_history
                .iter()
                .find_map(|event| match event {
                    E::StrongboxPermissionGrantedV1 { installation }
                        if installation.stack == stack =>
                    {
                        Some(installation.as_ref())
                    }
                    _ => None,
                })
        });
        let exiled = binding.map_or_else(Vec::new, |binding| binding.exiled_public.clone());
        let selected = consumed.flatten().and_then(|chosen| {
            exiled
                .iter()
                .find(|r| {
                    r.arena_id == chosen.object.0
                        && r.zone_change_count == chosen.expected_zone_change_count
                })
                .cloned()
        });
        let permission=installation.map(|g| {
            let card=selected.clone().ok_or_else(||RlContractError("Strongbox selected public card missing".into()))?;
            let started=state.engine.event_history.iter().any(|event|matches!(event,E::StrongboxPermissionTurnStartedV1 {installation,..} if *installation==g.index));
            let expired=state.engine.event_history.iter().any(|event|matches!(event,E::StrongboxPermissionExpiredV1 {installation,..} if *installation==g.index));
            let current=state.objects.try_get(g.chosen.object).is_some_and(|object|object.zone==Zone::Exile && object.zone_change_count==g.chosen.expected_zone_change_count && object.owner==capture.source.controller);
            Ok::<_,RlContractError>(PermissionV1 {card,holder:g.permission.holder.into(),play_land:g.permission.play_or_cast==crate::engine::PlayOrCast::Play,cast_spell:g.permission.play_or_cast==crate::engine::PlayOrCast::Cast,next_holder_turn_started:started,expired,exact_current_exile:current})
        }).transpose()?;
        let phase = if installation.is_some() {
            "permission"
        } else if consumed.is_some() {
            "closed"
        } else if binding.is_some() {
            "choosing"
        } else if live_stack_index.is_some() {
            "published"
        } else if publication.is_some() {
            "closed"
        } else {
            "waiting"
        };
        rows.push(EntryV1 {
            entry_order: u32::try_from(rows.len())
                .map_err(|_| RlContractError("Strongbox public entry count overflow".into()))?,
            source_at_entry: source,
            captured_controller: capture.source.controller.into(),
            pending_trigger_ordinal,
            live_stack_index,
            phase: phase.into(),
            exiled,
            selected,
            ordinary_permission: permission,
        });
    }
    Ok(rows)
}
fn selection_menu(state: &GameState) -> Result<Option<SelectionMenuV1>, RlContractError> {
    use crate::effect::{EffectTargetSelectionPurpose, PendingEffectChoice};
    let Some(pending) = state.engine.pending_effect.as_ref() else {
        return Ok(None);
    };
    let Some(PendingEffectChoice::SelectTargets {
        player,
        selected,
        min_targets,
        max_targets,
        purpose: EffectTargetSelectionPurpose::ChooseNewlyExiledCard { binding },
        ..
    }) = pending.choice.as_ref()
    else {
        return Ok(None);
    };
    let source = pending
        .ctx
        .ability_source_contract
        .ok_or_else(|| RlContractError("Strongbox chooser lost original source".into()))?;
    let selected = selected
        .iter()
        .map(|candidate| {
            binding
                .exiled_public
                .iter()
                .find(|r| {
                    candidate.expected_object.is_some_and(|b| {
                        r.arena_id == b.object.0
                            && r.zone_change_count == b.expected_zone_change_count
                    })
                })
                .cloned()
                .ok_or_else(|| RlContractError("Strongbox selected foreign exile".into()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(SelectionMenuV1 {
        source_at_entry: source_ref(source),
        chooser: (*player).into(),
        minimum: *min_targets,
        maximum: *max_targets,
        can_finish: selected.len() >= usize::from(*min_targets),
        legal: binding.exiled_public.clone(),
        selected,
    }))
}
