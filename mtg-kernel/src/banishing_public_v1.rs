//! Explicit linked-exile and immediate owner-attachment transport. Frozen V8/V9 remain refused.
use crate::event::CommittedEvent;
use crate::ids::PlayerId;
use crate::policy_observation_v8::PolicyObservationExtensionsV8;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::{
    CardPrivateV1, CardStableRefV1, KnownLibraryCardV4, PlayerSeatV1,
    PublicObservationProjectionV5, RlContractError,
};
use crate::state::{AbilitySourceContractV4, GameState, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/banishing-public/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerV1 {
    pub source_at_entry: CardStableRefV1,
    pub captured_controller: PlayerSeatV1,
    pub original_source_present: bool,
    pub phase: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkV1 {
    pub source_at_entry: CardStableRefV1,
    pub exiled_recipient: CardStableRefV1,
    pub recipient_owner: PlayerSeatV1,
    pub recipient_was_token: bool,
    pub expired: bool,
    pub duration_ended: bool,
    pub return_pending: bool,
    pub recipient_ceased: bool,
    pub outcome: Option<crate::banishing_light_v1::NoReturnV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentChoiceV1 {
    pub aura: CardStableRefV1,
    pub owner: PlayerSeatV1,
    pub legal_hosts: Vec<CardStableRefV1>,
    pub untargeted: bool,
    pub mandatory: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmedAttachmentV1 {
    pub aura: CardStableRefV1,
    pub owner: PlayerSeatV1,
    /// The chosen exact host is included only while still publicly on BF.
    pub current_chosen_host: Option<CardStableRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootV1 {
    pub card_id: u16,
    pub root: crate::rules_meaning_banishing_v1::RootV1,
    pub root_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BanishingObservationV1 {
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
    pub triggers: Vec<TriggerV1>,
    pub links: Vec<LinkV1>,
    pub attachment_choice: Option<AttachmentChoiceV1>,
    pub confirmed_attachments: Vec<ConfirmedAttachmentV1>,
    pub roots: Vec<RootV1>,
    pub projection_sha256: String,
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn producer_sha256_v1() -> String {
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("banishing_public_v1.rs"),
        include_str!("banishing_light_v1.rs"),
        include_str!("linked_aura_operation_v1.rs"),
        include_str!("linked_aura_return_v1.rs"),
        include_str!("rules_meaning_banishing_v1.rs"),
        include_str!("rl.rs"),
        include_str!("rl_session/banishing_v26.rs"),
        include_str!("human_bo3_v1/banishing_v26.rs"),
        include_str!("human_bo3_v1/visible.rs"),
        include_str!("human_bo3_v1/labels.rs"),
        include_str!("human_bo3_v1/life_ward_v8.rs"),
        include_str!("rl_session.rs"),
        include_str!("rl_session/flat_action_v3.rs"),
        include_str!("rl_session/flat_action_v4.rs"),
        include_str!("policy_surface_v5.rs"),
        include_str!("policy_observation_v8.rs"),
        include_str!("policy_observation_v7.rs"),
        include_str!("engine.rs"),
        include_str!("state.rs"),
        include_str!("event.rs"),
        include_str!("effect.rs"),
        include_str!("card_def.rs"),
        include_str!("../build.rs"),
        include_str!("trigger.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("standard_statics_v1.rs"),
        include_str!("sower_restriction_capture_v1.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
pub(crate) fn roots() -> Result<Vec<RootV1>, RlContractError> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Ok(Vec::new());
    }
    let Some(id) = crate::card_def::card_id_by_name("Banishing Light") else {
        return Ok(Vec::new());
    };
    let selector = crate::rules_meaning_banishing_v1::live_selector_v1()
        .map_err(|_| RlContractError("Banishing catalog unsupported".into()))?;
    [
        crate::rules_meaning_banishing_v1::RootV1::OrdinaryPermanentSpell,
        crate::rules_meaning_banishing_v1::RootV1::PrintedEtb { trigger_index: 0 },
        crate::rules_meaning_banishing_v1::RootV1::ImmediateUntilSourceLeavesReturn,
    ]
    .into_iter()
    .map(|root| {
        let meaning = crate::rules_meaning_banishing_v1::meaning_v1(&selector, id, root)
            .map_err(|_| RlContractError("Banishing exact root unsupported".into()))?;
        Ok(RootV1 {
            card_id: id,
            root,
            root_sha256: meaning.root_sha256,
        })
    })
    .collect()
}
fn commitment(observation: &BanishingObservationV1) -> Result<String, RlContractError> {
    let mut value = serde_json::to_value(observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("Banishing object absent".into()))?
        .remove("projection_sha256");
    Ok(digest(&serde_json::to_vec(&value)?))
}
impl BanishingObservationV1 {
    /// Artifact identity never authorizes execution; the owner rederives all facts.
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.producer_sha256 != producer_sha256_v1()
            || self.roots != roots()?
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError("wrong Banishing artifact identity".into()));
        }
        Ok(())
    }
}
fn source_ref(source: AbilitySourceContractV4) -> CardStableRefV1 {
    CardStableRefV1 {
        arena_id: source.source.0,
        card_db_id: source.card_def,
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: source.zone,
        zone_change_count: source.zone_change_count,
    }
}
fn aura_ref(choice: &crate::linked_aura_return_v1::HostChoiceV1) -> CardStableRefV1 {
    CardStableRefV1 {
        arena_id: choice.aura.object.0,
        card_db_id: choice.link.exiled_card_def,
        owner: choice.owner.into(),
        controller: choice.owner.into(),
        zone: Zone::Exile,
        zone_change_count: choice.aura.expected_zone_change_count,
    }
}
fn current_host(
    state: &GameState,
    binding: crate::effect::EffectObjectBinding,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    state
        .objects
        .try_get(binding.object)
        .filter(|live| {
            live.zone == Zone::Battlefield
                && live.zone_change_count == binding.expected_zone_change_count
        })
        .map(|live| crate::sower_restriction_capture_v1::masked_ref(binding.object, live))
        .transpose()
}
pub(crate) type FactsV1 = (
    Vec<TriggerV1>,
    Vec<LinkV1>,
    Option<AttachmentChoiceV1>,
    Vec<ConfirmedAttachmentV1>,
);
pub(crate) fn facts(state: &GameState) -> Result<FactsV1, RlContractError> {
    crate::banishing_light_v1::validate_state(state).map_err(RlContractError)?;
    let mut triggers = Vec::new();
    let mut links = Vec::new();
    for event in &state.engine.event_history {
        match event {
            CommittedEvent::BanishingEntryV1 { source, .. } => {
                let published = state
                    .engine
                    .event_history
                    .iter()
                    .find_map(|event| match event {
                        CommittedEvent::BanishingPublishedV1 { item }
                            if item.v4.ability_source_contract == Some(*source) =>
                        {
                            Some(item)
                        }
                        _ => None,
                    });
                let phase = if let Some(item) = published {
                    if state.engine.event_history.iter().any(|event|matches!(event,
                        CommittedEvent::BanishingFinishedV1 {stack_item_id} if *stack_item_id==item.v4.stack_item_id)) {
                        if state.engine.event_history.iter().any(|event|matches!(event,
                            CommittedEvent::BanishingRemovedV1 {item:original,outcome:crate::banishing_light_v1::RemovalV1::Countered}
                                if original==item)){"countered"}else{"resolved"}
                    }else{"published"}
                } else if state.engine.event_history.iter().any(|event| {
                    matches!(event,
                    CommittedEvent::BanishingNoTargetsV1 {source:original} if original==source)
                }) {
                    "no_legal_target"
                } else {
                    "pending"
                };
                triggers.push(TriggerV1 {
                    source_at_entry: source_ref(*source),
                    captured_controller: source.controller.into(),
                    original_source_present: crate::banishing_light_v1::source_present(
                        state, *source,
                    ),
                    phase: phase.into(),
                });
            }
            CommittedEvent::BanishingLinkInstalledV1 {
                record,
                recipient_was_token,
                ..
            } => {
                let expiry = state
                    .engine
                    .event_history
                    .iter()
                    .find_map(|event| match event {
                        CommittedEvent::BanishingLinkExpiredV1 {
                            record: original,
                            no_return_reason,
                            ..
                        } if original == record => Some(no_return_reason.clone()),
                        _ => None,
                    });
                links.push(LinkV1 {
                    source_at_entry: source_ref(record.source),
                    exiled_recipient: CardStableRefV1 {
                        arena_id: record.exiled.0,
                        card_db_id: record.exiled_card_def,
                        owner: record.exiled_owner.into(),
                        controller: record.exiled_owner.into(),
                        zone: Zone::Exile,
                        zone_change_count: record.exiled_zone_change_count,
                    },
                    recipient_owner: record.exiled_owner.into(),
                    recipient_was_token: *recipient_was_token,
                    expired: expiry.is_some(),
                    duration_ended: !crate::banishing_light_v1::source_present(state,record.source),
                    return_pending: expiry.is_none() && crate::linked_aura_operation_v1::has_pending(state)
                        && !crate::banishing_light_v1::source_present(state,record.source),
                    recipient_ceased: state.engine.event_history.iter().any(|event|matches!(event,
                        CommittedEvent::BanishingRecipientCeasedV1 {record:original} if original==record)),
                    outcome: expiry.flatten(),
                });
            }
            _ => {}
        }
    }
    let mut confirmed = Vec::new();
    let attachment = if let Some(pending) = state.engine.linked_aura_operation_v1.pending.as_ref() {
        for answer in &pending.answers {
            confirmed.push(ConfirmedAttachmentV1 {
                aura: aura_ref(&answer.choice),
                owner: answer.choice.owner.into(),
                current_chosen_host: current_host(state, answer.host)?,
            });
        }
        let legal_hosts = pending
            .choice
            .original_candidates
            .iter()
            .map(|binding| {
                current_host(state, *binding)?.ok_or_else(|| {
                    RlContractError(
                        "Banishing attachment host lost exact public incarnation".into(),
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Some(AttachmentChoiceV1 {
            aura: aura_ref(&pending.choice),
            owner: pending.choice.owner.into(),
            legal_hosts,
            untargeted: true,
            mandatory: true,
        })
    } else {
        None
    };
    Ok((triggers, links, attachment, confirmed))
}
// V40 consumes these private typed parts; standalone predecessors retain their refusal path.
pub(crate) fn parts_for_akroma_v40(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
) -> Result<BanishingObservationV1, RlContractError> {
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
) -> Result<BanishingObservationV1, RlContractError> {
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
) -> Result<BanishingObservationV1, RlContractError> {
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
) -> Result<BanishingObservationV1, RlContractError> {
    let (substep_index, substep_count) = substep;

    let (triggers, links, attachment_choice, confirmed_attachments) = facts(state)?;
    let base = crate::rl::common_family_observation_storage_scoped(
        (scope, state),
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let mut base = storage.base;
    if attachment_choice.is_some() {
        // New producer storage describes the actual effect owner, without
        // manufacturing a predecessor observation or canonical effect root.
        base.projection.surface.engine_context.current_stage =
            crate::rl::EngineDecisionStageV2::PendingEffect;
    }
    let observation = BanishingObservationV1 {
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
        triggers,
        links,
        attachment_choice,
        confirmed_attachments,
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
) -> Result<BanishingObservationV1, RlContractError> {
    let (triggers, links, attachment_choice, confirmed_attachments) = facts(state)?;
    let base = crate::rl::banishing_observation_storage_v1(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let mut base = storage.base;
    if attachment_choice.is_some() {
        // New producer storage describes the actual effect owner, without
        // manufacturing a predecessor observation or canonical effect root.
        base.projection.surface.engine_context.current_stage =
            crate::rl::EngineDecisionStageV2::PendingEffect;
    }
    let mut observation = BanishingObservationV1 {
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
        triggers,
        links,
        attachment_choice,
        confirmed_attachments,
        roots: roots()?,
        projection_sha256: String::new(),
    };
    observation.projection_sha256 = commitment(&observation)?;
    Ok(observation)
}
