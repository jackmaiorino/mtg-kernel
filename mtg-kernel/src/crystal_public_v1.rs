//! Scoped Crystal static facts and affected-controller prevention choices.
//! No frozen observation or model schema is relabeled.
use crate::ids::PlayerId;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::{
    CardPrivateV1, CardStableRefV1, KnownLibraryCardV4, PlayerSeatV1,
    PublicObservationProjectionV5, RlContractError,
};
use crate::state::{GameState, Target, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/crystal-public/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticSourceV1 {
    pub source: CardStableRefV1,
    pub controller: PlayerSeatV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionV1 {
    pub kind: String,
    pub source: CardStableRefV1,
    pub shield_remaining: Option<i32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MenuV1 {
    pub chooser: PlayerSeatV1,
    /// Present only for publicly announced, receipt-authenticated sacrifice
    /// damage. In that case source describes the captured incarnation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_history: Option<SavedDamageSourceV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed_source_history: Option<SavedFixedDamageSourceV3>,
    /// Includes the current affected recipient. No hidden roster or raw IDs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub simultaneous_choices_remaining: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creature_power_at_resolution: Option<i32>,
    pub source: CardStableRefV1,
    pub resolving_source: CardStableRefV1,
    pub recipient: CardStableRefV1,
    pub remaining_damage: i32,
    pub options: Vec<OptionV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscardMenuV2 {
    pub chooser: PlayerSeatV1,
    pub source: CardStableRefV1,
    pub maximum_cards: u16,
    pub draws_if_discarded: u16,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramMenuV3 {
    pub chooser: PlayerSeatV1,
    pub source: CardStableRefV1,
    pub decline_index: u16,
    pub fight_index: u16,
    pub requires_original_source_creature: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedDamageSourceV1 {
    pub signed_power: i32,
    pub captured_controller: PlayerSeatV1,
    pub color_mask: u8,
    pub lifelink: bool,
    pub deathtouch: bool,
    pub werewolf: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedFixedDamageSourceV3 {
    pub printed_damage: i32,
    pub captured_controller: PlayerSeatV1,
    pub color_mask: u8,
    pub lifelink: bool,
    pub deathtouch: bool,
    pub werewolf: bool,
}
type DamageOriginV3 = (
    CardStableRefV1,
    Option<SavedDamageSourceV1>,
    Option<SavedFixedDamageSourceV3>,
);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrystalObservationV1 {
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
    pub extensions: crate::policy_observation_v8::PolicyObservationExtensionsV8,
    pub attacked_this_turn: [bool; 2],
    pub static_sources: Vec<StaticSourceV1>,
    pub prevention_menu: Option<MenuV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optional_discard_menu: Option<DiscardMenuV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optional_fight_menu: Option<ProgramMenuV3>,
    pub projection_sha256: String,
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn producer_sha256_v1() -> String {
    let mut h = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("crystal_public_v1.rs"),
        include_str!("crystal_barricade_v1.rs"),
        include_str!("effect/crystal_prevention_v1.rs"),
        include_str!("effect.rs"),
        include_str!("event.rs"),
        include_str!("engine.rs"),
        include_str!("state.rs"),
        include_str!("rl.rs"),
        include_str!("rl_session.rs"),
        include_str!("rl_session/crystal_v22.rs"),
        include_str!("human_bo3_v1/crystal_v22.rs"),
        include_str!("human_bo3_v1/visible.rs"),
        include_str!("human_bo3_v1/labels.rs"),
        include_str!("human_bo3_v1/life_ward_v8.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("sower_restriction_capture_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("source_power_activation_v1.rs"),
        include_str!("fixed_source_damage_v3.rs"),
        include_str!("rules_meaning_heartfire_v1.rs"),
        include_str!("creature_stats_lki_v1.rs"),
        include_str!("standard_legends_v1.rs"),
        include_str!("rules_meaning_crystal_ordering_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("../build.rs"),
        include_str!("policy_observation_v8.rs"),
        include_str!("policy_observation_v7.rs"),
        include_str!("rl_session/flat_action_v3.rs"),
        include_str!("rl_session/flat_action_v4.rs"),
    ] {
        h.update([0]);
        h.update(source.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
fn commitment(o: &CrystalObservationV1) -> Result<String, RlContractError> {
    let mut v = serde_json::to_value(o)?;
    v.as_object_mut()
        .ok_or_else(|| RlContractError("Crystal observation is not an object".into()))?
        .remove("projection_sha256");
    Ok(digest(&serde_json::to_vec(&v)?))
}
impl CrystalObservationV1 {
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.producer_sha256 != producer_sha256_v1()
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError("wrong Crystal artifact identity".into()));
        }
        Ok(())
    }
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    crate::effect::crystal_prevention_v1::validate(state)?;
    crate::effect::validate_pending_effect_choice(state)?;
    if crate::crystal_barricade_v1::active(state) {
        crate::rules_meaning_crystal_ordering_v1::selected()?;
    }
    Ok(())
}
fn public_ref(
    state: &GameState,
    id: crate::ids::ObjectId,
) -> Result<CardStableRefV1, RlContractError> {
    let o = state
        .objects
        .try_get(id)
        .ok_or_else(|| RlContractError("Crystal public operand absent".into()))?;
    if !matches!(
        o.zone,
        Zone::Battlefield | Zone::Stack | Zone::Graveyard | Zone::Exile
    ) {
        return Err(RlContractError(
            "hidden prevention operand needs a captured public-history successor".into(),
        ));
    }
    crate::sower_restriction_capture_v1::masked_ref(id, o)
}
fn damage_origin(
    state: &GameState,
    choice: &crate::effect::crystal_prevention_v1::ChoiceV1,
) -> Result<DamageOriginV3, RlContractError> {
    let Some(id) = choice.historical_source else {
        return Ok((public_ref(state, choice.source)?, None, None));
    };
    let item = state
        .stack
        .iter()
        .find(|item| item.v4.stack_item_id == id)
        .ok_or_else(|| RlContractError("Crystal historical source lost live owner".into()))?;
    crate::source_power_activation_v1::validate_item(state, item).map_err(RlContractError)?;
    let source = item.v4.ability_source_contract.ok_or_else(|| {
        RlContractError("Crystal historical source lost captured identity".into())
    })?;
    let local = item
        .v4
        .activated_ability_index
        .ok_or_else(|| RlContractError("Crystal historical source lost printed selector".into()))?;
    if crate::fixed_source_damage_v3::has_payment(state, id) {
        if !crate::fixed_source_damage_v3::selected(item) || local != 0 {
            return Err(RlContractError(
                "Crystal fixed source lost its printed selector".into(),
            ));
        }
        let facts = crate::fixed_source_damage_v3::damage_source(state, choice.source, id)
            .map_err(RlContractError)?;
        let reference = CardStableRefV1 {
            arena_id: source.source.0,
            card_db_id: source.card_def,
            owner: source.owner.into(),
            controller: source.controller.into(),
            zone: source.zone,
            zone_change_count: source.zone_change_count,
        };
        return Ok((
            reference,
            None,
            Some(SavedFixedDamageSourceV3 {
                printed_damage: 1,
                captured_controller: facts.source.controller.into(),
                color_mask: facts.color_mask,
                lifelink: facts.lifelink,
                deathtouch: facts.deathtouch,
                werewolf: facts.werewolf,
            }),
        ));
    }
    crate::rules_meaning_heartfire_v1::root_meaning(source.card_def, u32::from(local)).map_err(
        |_| RlContractError("Crystal historical source is not the printed power root".into()),
    )?;
    let facts = crate::source_power_activation_v1::damage_source(state, choice.source, id)
        .map_err(RlContractError)?;
    let power = state
        .engine
        .event_history
        .iter()
        .find_map(|e| match e {
            crate::event::CommittedEvent::SourcePowerActivationCommittedV1 {
                item, power, ..
            } if item.v4.stack_item_id == id => Some(*power),
            _ => None,
        })
        .ok_or_else(|| RlContractError("Crystal historical source lost power".into()))?;
    // These are the publicly visible source facts at the genuine activation,
    // not an inspection of a later hidden/reentered object's current identity.
    let reference = CardStableRefV1 {
        arena_id: source.source.0,
        card_db_id: source.card_def,
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: source.zone,
        zone_change_count: source.zone_change_count,
    };
    Ok((
        reference,
        Some(SavedDamageSourceV1 {
            signed_power: power,
            captured_controller: facts.source.controller.into(),
            color_mask: facts.color_mask,
            lifelink: facts.lifelink,
            deathtouch: facts.deathtouch,
            werewolf: facts.werewolf,
        }),
        None,
    ))
}
fn discard_menu(
    state: &GameState,
    actor: PlayerId,
) -> Result<Option<DiscardMenuV2>, RlContractError> {
    if !crate::effect::crystal_prevention_v1::is_discard_choice(state) {
        return Ok(None);
    }
    let pending = state.engine.pending_effect.as_ref().unwrap();
    if actor != pending.ctx.controller {
        return Err(RlContractError(
            "Crystal discard input is private to its captured controller".into(),
        ));
    }
    Ok(Some(DiscardMenuV2 {
        chooser: actor.into(),
        source: crate::rl::crystal_resolving_source_ref(state, &pending.resolving_item)?,
        maximum_cards: 1,
        draws_if_discarded: 1,
    }))
}
fn menu(state: &GameState) -> Result<Option<MenuV1>, RlContractError> {
    use crate::effect::crystal_prevention_v1::PreventionV1;
    use crate::effect::{EffectOptionChoicePurpose, PendingEffectChoice};
    let Some(PendingEffectChoice::ChooseOption {
        purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
        ..
    }) = state
        .engine
        .pending_effect
        .as_ref()
        .and_then(|p| p.choice.as_ref())
    else {
        return Ok(None);
    };
    let Target::Object(recipient) = choice.target else {
        return Err(RlContractError("Crystal menu lost recipient".into()));
    };
    let options = choice.options.iter().map(|option| {
        let (kind, id, amount) = match option {
            PreventionV1::Barricade(source) => ("barricade", source.source, None),
            PreventionV1::Protection(target) => ("protection", target.object, None),
            PreventionV1::Replacement(r) => match r.kind {
                crate::event::ReplacementEffectKind::PreventNextDamage { remaining, .. } =>
                    ("shield", r.source, Some(remaining)),
                crate::event::ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn { .. } =>
                    ("color", r.source, None),
                crate::event::ReplacementEffectKind::PreventCombatDamageToObjectUntilEndOfTurn { .. } =>
                    return Err(RlContractError("noncombat Crystal menu contains combat-only prevention".into())),
            },
        };
        Ok(OptionV1 { kind: kind.into(), source: public_ref(state, id)?, shield_remaining: amount })
    }).collect::<Result<Vec<_>, RlContractError>>()?;
    let (source, source_history, fixed_source_history) = damage_origin(state, choice)?;
    Ok(Some(MenuV1 {
        chooser: choice.chooser.into(),
        source,
        resolving_source: crate::rl::crystal_resolving_source_ref(
            state,
            &state.engine.pending_effect.as_ref().unwrap().resolving_item,
        )?,
        source_history,
        fixed_source_history,
        simultaneous_choices_remaining: choice.batch.as_ref().map(|progress| {
            progress
                .outcomes
                .iter()
                .filter(|o| {
                    matches!(
                        o,
                        crate::effect::crystal_prevention_v1::BatchOutcomeV2::Waiting
                    )
                })
                .count()
        }),
        creature_power_at_resolution: crate::effect::crystal_prevention_v1::creature_power_signed(
            state, choice,
        )
        .map_err(RlContractError)?,
        recipient: public_ref(state, recipient)?,
        remaining_damage: choice.remaining_amount,
        options,
    }))
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
) -> Result<CrystalObservationV1, RlContractError> {
    validate_state(state).map_err(RlContractError)?;
    let optional_discard_menu = discard_menu(state, actor)?;
    let optional_fight_menu = if crate::effect::crystal_prevention_v1::is_program_choice(state) {
        let pending = state
            .engine
            .pending_effect
            .as_ref()
            .ok_or_else(|| RlContractError("Crystal optional fight absent".into()))?;
        Some(ProgramMenuV3 {
            chooser: pending.ctx.controller.into(),
            source: crate::rl::crystal_resolving_source_ref(state, &pending.resolving_item)?,
            decline_index: 0,
            fight_index: 1,
            requires_original_source_creature: true,
        })
    } else {
        None
    };
    let mut static_sources = Vec::new();
    for (id, o) in state.objects.iter() {
        if crate::crystal_barricade_v1::is_active_source(state, id) {
            static_sources.push(StaticSourceV1 {
                source: public_ref(state, id)?,
                controller: o.controller.into(),
            });
        }
    }
    let base = crate::rl::crystal_observation_storage_v1(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let base = storage.base;
    let mut o = CrystalObservationV1 {
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
        static_sources,
        prevention_menu: menu(state)?,
        optional_discard_menu,
        optional_fight_menu,
        projection_sha256: String::new(),
    };
    o.projection_sha256 = commitment(&o)?;
    Ok(o)
}

#[cfg(test)]
#[path = "crystal_public_v1_tests.rs"]
mod tests;
