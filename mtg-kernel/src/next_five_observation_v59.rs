//! One current next-five-family envelope. Compatibility layouts have distinct
//! schemas and never authorize frozen V51/V52 artifacts or executable actions.
use crate::{
    ids::PlayerId,
    policy_surface_v5::{PolicyDecisionV5, PolicySurfaceV5},
    rl::{ActionSemanticV1, CardStableRefV1, RlContractError},
    state::GameState,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/next-five-observation/v59";
pub const COMMON_SCHEMA: &str = "mtg-kernel/next-five-compat-common/v59";
pub const ELENDA_COMPAT_SCHEMA: &str = "mtg-kernel/next-five-compat-elenda/v59";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NextFiveActivationV59 {
    pub source: CardStableRefV1,
    pub ability_index: u8,
    pub kind: crate::rules_meaning_drake_hatcher_v53::CounterV53,
    pub count: u16,
    pub before: i32,
    pub after: i32,
    pub any_priority: bool,
    pub costs_paid_before_stack: bool,
    pub countering_refunds_costs: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NextFiveObservationV59 {
    pub schema: String,
    pub producer_sha256: String,
    /// Inherited layout only. Both nested schemas are distinct Next-five schemas.
    /// Narrow card slots never supersede the mandatory drake.battlefield roster.
    pub common: crate::elenda_observation_v52::ElendaObservationV52,
    pub drake: crate::drake_public_v53::FactsV53,
    pub spinner: crate::spinner_public_v54::FactsV54,
    pub quandary: crate::quandary_public_v55::FactsV55,
    pub phoenix: crate::phoenix_public_v56::FactsV56,
    /// Exact same order/cardinality as the genuine current action vector.
    pub action_meanings: Vec<Option<NextFiveActivationV59>>,
    pub next_five: crate::next_five_public_v59::FactsV59,
    pub projection_sha256: String,
}
pub(crate) type NextFivePublicPartsV59 = (
    NextFiveObservationV59,
    Vec<crate::akroma_family_public_v40::FamilyProjectionV40>,
);
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for part in [
            SCHEMA,
            COMMON_SCHEMA,
            ELENDA_COMPAT_SCHEMA,
            include_str!("next_five_observation_v59.rs"),
            include_str!("next_five_public_v59.rs"),
            include_str!("snack_wide_public_v59.rs"),
            include_str!("panda_public_v47.rs"),
            include_str!("conqueror_public_v49.rs"),
            include_str!("rules_meaning_next_five_v59.rs"),
            include_str!("goblin_negotiation_v1/owned.rs"),
            include_str!("prepared_observation_v57.rs"),
            include_str!("rl_session/next_five_v59.rs"),
            include_str!("rl_session.rs"),
            include_str!("divine_actions_v44.rs"),
            include_str!("rl_variable_cast_v1.rs"),
            include_str!("rl.rs"),
            include_str!("ravenous_observation_v51.rs"),
            include_str!("ravenous_public_v51.rs"),
            include_str!("elenda_public_v52.rs"),
            include_str!("drake_public_v53.rs"),
            include_str!("drake_hatcher_v1/custody.rs"),
            include_str!("drake_hatcher_v1.rs"),
            include_str!("spinner_public_v54.rs"),
            include_str!("quandary_public_v55.rs"),
            include_str!("optional_trigger_public_v1.rs"),
            include_str!("optional_trigger_source_v1.rs"),
            include_str!("phoenix_public_v56.rs"),
            include_str!("rules_meaning_drake_hatcher_v53.rs"),
            include_str!("rules_meaning_spinner_v54.rs"),
            include_str!("rules_meaning_quandary_v55.rs"),
            include_str!("rules_meaning_phoenix_v56.rs"),
            include_str!("akroma_family_public_v40.rs"),
            include_str!("crystal_public_v1.rs"),
            include_str!("twinflame_observation_v1.rs"),
            include_str!("engine.rs"),
            include_str!("event.rs"),
            include_str!("effect.rs"),
            include_str!("state.rs"),
            include_str!("trigger.rs"),
            include_str!("policy_surface_v5.rs"),
            include_str!("counter_target_effects_v1.rs"),
            include_str!("zimone_paradox_sculptor_v1.rs"),
            include_str!("standard_creatures_v1.rs"),
            include_str!("planeswalker_v1.rs"),
            include_str!("goblin_negotiation_v1.rs"),
            include_str!("engine/omniscience_cast_v1.rs"),
            include_str!("resolution_cast_v1.rs"),
            include_str!("effect/harvester_v1.rs"),
            include_str!("leyline_axe_v1.rs"),
            include_str!("london_mulligan_v1.rs"),
            include_str!("life_loss_turn_v1.rs"),
            include_str!("soul_shackled_zombie_v1.rs"),
            include_str!("progenitus_v1.rs"),
            include_str!("controlled_creature_grants_v1.rs"),
            include_str!("source_power_activation_v1.rs"),
            include_str!("effect/nonland_copy_v1.rs"),
            include_str!("rl_session/flat_action_v4/search_state/effect_refs.rs"),
            include_str!("effect/inspiration_resolution_v1.rs"),
            include_str!("effect/standard_exile.rs"),
            include_str!("snack_wide_life_v59.rs"),
            include_str!("snack_wide_life_v59/custody.rs"),
            include_str!("linked_aura_operation_v1.rs"),
            include_str!("bloodthirsty_conqueror_v49.rs"),
            include_str!("fiendish_panda_v1.rs"),
            include_str!("fiendish_panda_v1/custody.rs"),
            include_str!("life_gain_turn_v1.rs"),
            include_str!("midnight_snack_v1.rs"),
            include_str!("continuous_characteristics_v1.rs"),
            include_str!("standard_cards_v1.rs"),
            include_str!("mana.rs"),
            include_str!("legend_rule_v1.rs"),
            include_str!("doubling_season_v48.rs"),
            include_str!("standard_cast_convoke.rs"),
            include_str!("engine/mandatory_cast_cost_v1.rs"),
        ] {
            hash.update((part.len() as u64).to_le_bytes());
            hash.update(part.as_bytes());
        }
        hash.update(b"legacy-graveyard-return-guard/v61\0");
        hash.update(crate::graveyard_return_public_v61::producer_sha256().as_bytes());
        hash.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    })
    .clone()
}
pub fn commitment(value: &NextFiveObservationV59) -> Result<String, RlContractError> {
    let mut value = value.clone();
    value.projection_sha256.clear();
    let bytes = serde_json::to_vec(&value).map_err(|e| RlContractError(e.to_string()))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    crate::drake_hatcher_v1::custody::with_validation_replay_keys(state, validate_state_in_frame)
}
fn validate_state_in_frame(state: &GameState) -> Result<(), String> {
    // Reuse every inherited authority validator, without treating its old DTO
    // as a current artifact or inheriting executable authority from that DTO.
    crate::engine::validate_battlefield_pt(state)
        .map_err(|id| format!("native PT envelope exceeded for {id:?}"))?;
    crate::ravenous_observation_v51::validate_state_complete_next_five_v59(state)?;
    crate::graveyard_return_public_v61::require_legacy_visible(state, None)?;
    crate::elenda_public_v52::capture(state).map_err(|e| e.0)?;
    crate::drake_public_v53::capture(state).map_err(|e| e.0)?;
    crate::spinner_public_v54::capture(state)?;
    for viewer in [PlayerId::P0, PlayerId::P1] {
        crate::quandary_public_v55::capture(state, viewer)?;
    }
    crate::phoenix_public_v56::capture(state).map_err(|e| e.0)?;
    for viewer in [PlayerId::P0, PlayerId::P1] {
        crate::next_five_public_v59::capture(state, viewer)?;
    }
    Ok(())
}

fn annotation(
    state: &GameState,
    action: &ActionSemanticV1,
) -> Result<Option<NextFiveActivationV59>, RlContractError> {
    let ActionSemanticV1::ActivateAbility {
        actor,
        source,
        ability_index: 0,
        granted_ability,
    } = action
    else {
        return Ok(None);
    };
    if !granted_ability.is_none() {
        return Ok(None);
    }
    let Some(live) = state.objects.try_get(crate::ids::ObjectId(source.arena_id)) else {
        return Err(RlContractError("prepared activation source missing".into()));
    };
    // Concealed/suppressed underlying printings cannot select a public rule.
    if live.v4.face_down_v1.is_some()
        || !crate::continuous_characteristics_v1::printed_abilities_active(
            state,
            crate::ids::ObjectId(source.arena_id),
        )
    {
        return Ok(None);
    }
    if crate::card_def::CARD_DEFS[usize::from(live.card_def)].name != "Drake Hatcher" {
        return Ok(None);
    }
    crate::rules_meaning_drake_hatcher_v53::printed_meaning(
        live.card_def,
        crate::rules_meaning_drake_hatcher_v53::RootV53::RemoveIncubationForDrake,
    )
    .map_err(RlContractError)?;
    if live.zone != crate::state::Zone::Battlefield
        || source.zone != live.zone
        || source.zone_change_count != live.zone_change_count
        || source.owner != live.owner.into()
        || source.controller != live.controller.into()
        || *actor != source.controller
        || source.card_db_id != live.card_def
        || live.v4.face_down_v1.is_some()
        || !crate::continuous_characteristics_v1::printed_abilities_active(
            state,
            crate::ids::ObjectId(source.arena_id),
        )
        || live.counters.incubation < 3
    {
        return Err(RlContractError(
            "prepared Hatcher activation relation changed".into(),
        ));
    }
    Ok(Some(NextFiveActivationV59 {
        source: source.clone(),
        ability_index: 0,
        kind: crate::rules_meaning_drake_hatcher_v53::CounterV53::Incubation,
        count: 3,
        before: live.counters.incubation,
        after: live
            .counters
            .incubation
            .checked_sub(3)
            .ok_or_else(|| RlContractError("prepared counter payment underflow".into()))?,
        any_priority: true,
        costs_paid_before_stack: true,
        countering_refunds_costs: false,
    }))
}
fn selected_source(source: crate::state::AbilitySourceContractV4) -> bool {
    crate::card_def::CARD_DEFS
        .get(usize::from(source.card_def))
        .is_some_and(|d| {
            matches!(
                d.name,
                "Drake Hatcher"
                    | "Spinner of Souls"
                    | "Painful Quandary"
                    | "Flamewake Phoenix"
                    | "Midnight Snack"
                    | "Extravagant Replication"
                    | "Infernal Vessel"
                    | "Giada, Font of Hope"
                    | "Goblin Negotiation"
            )
        })
}
fn captured_reference(
    state: &GameState,
    source: crate::state::AbilitySourceContractV4,
) -> CardStableRefV1 {
    CardStableRefV1 {
        arena_id: source.source.0,
        card_db_id: crate::rl::projected_captured_card_def(
            state,
            source.source,
            source.zone_change_count,
            source.zone,
            source.card_def,
        ),
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: source.zone,
        zone_change_count: source.zone_change_count,
    }
}
pub(crate) fn stack_source_reference(
    state: &GameState,
    item: &crate::state::StackItem,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    if let Some(source) = crate::snack_wide_public_v59::stack_source_reference(state, item)? {
        return Ok(Some(source));
    }
    let source = if let Some(source) = item.v4.ability_source_contract {
        source
    } else if let Some(source) = item.v4.source_contract {
        crate::state::AbilitySourceContractV4 {
            source: source.source,
            card_def: source.card_def,
            owner: source.owner,
            controller: source.controller,
            zone: source.zone,
            zone_change_count: source.zone_change_count,
            attached_to: None,
        }
    } else {
        return Ok(None);
    };
    if !selected_source(source) {
        return Ok(None);
    }
    validate_state(state).map_err(RlContractError)?;
    crate::engine::validated_stack_item_target_spec(item, state).map_err(RlContractError)?;
    let live = state
        .stack
        .iter()
        .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
        .collect::<Vec<_>>();
    if !(matches!(live.as_slice(),[row] if *row==item)
        || live.is_empty()
            && state
                .engine
                .pending_effect
                .as_ref()
                .is_some_and(|pending| pending.resolving_item == *item))
    {
        return Err(RlContractError(
            "prepared source requires unique genuine live or resolving full row".into(),
        ));
    }
    if item.source != source.source || item.controller != source.controller {
        return Err(RlContractError(
            "prepared original source/controller changed".into(),
        ));
    }
    Ok(Some(captured_reference(state, source)))
}
pub(crate) fn pending_source_reference(
    state: &GameState,
    pending: &crate::trigger::PendingTrigger,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    if let Some(source) = crate::snack_wide_public_v59::pending_source_reference(state, pending)? {
        return Ok(Some(source));
    }
    let Some(source) = pending.source_contract else {
        return Ok(None);
    };
    if !selected_source(source) {
        return Ok(None);
    }
    validate_state(state).map_err(RlContractError)?;
    crate::engine::validate_pending_trigger_identity(state, pending).map_err(RlContractError)?;
    let held = crate::optional_trigger_public_v1::held(state);
    let actual = held
        .iter()
        .filter(|row| std::ptr::eq(**row, pending))
        .count();
    let matching = held.iter().filter(|row| **row == pending).count();
    // Distinct simultaneous occurrences may have identical values. A direct
    // held-row borrow is unambiguous; a detached value needs a unique match.
    if !(actual == 1 || (actual == 0 && matching == 1))
        || pending.source != source.source
        || pending.controller != source.controller
    {
        return Err(RlContractError(
            "prepared source requires unique genuine held trigger".into(),
        ));
    }
    Ok(Some(captured_reference(state, source)))
}

pub fn observe_v59(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<NextFiveObservationV59, RlContractError> {
    observe_parts_v59(state, surface, actor_and_origin, step, physical, substep)
        .map(|(value, _)| value)
}
pub(crate) fn observe_parts_v59(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<NextFivePublicPartsV59, RlContractError> {
    validate_state(state).map_err(RlContractError)?;
    let (inherited, families) =
        crate::ravenous_observation_v51::observe_parts_complete_next_five_v59(
            state,
            surface,
            actor_and_origin,
            step,
            physical,
            substep,
        )?;
    let mut common = crate::elenda_observation_v52::ElendaObservationV52 {
        schema: ELENDA_COMPAT_SCHEMA.into(),
        producer_sha256: producer_sha256(),
        common: inherited,
        elenda: crate::elenda_public_v52::capture(state)?,
        projection_sha256: String::new(),
    };
    common.projection_sha256 = crate::elenda_observation_v52::commitment(&common)?;
    let candidates =
        crate::rl_session::next_five_candidates_v59(actor_and_origin.1, state, surface)?;
    let action_meanings = candidates
        .iter()
        .map(|candidate| annotation(state, &candidate.semantic))
        .collect::<Result<Vec<_>, _>>()?;
    let mut value = NextFiveObservationV59 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        common,
        drake: crate::drake_public_v53::capture(state)?,
        spinner: crate::spinner_public_v54::capture(state).map_err(RlContractError)?,
        quandary: crate::quandary_public_v55::capture(state, actor_and_origin.0)
            .map_err(RlContractError)?,
        phoenix: crate::phoenix_public_v56::capture(state)?,
        action_meanings,
        next_five: crate::next_five_public_v59::capture(state, actor_and_origin.0)
            .map_err(RlContractError)?,
        projection_sha256: String::new(),
    };
    value.projection_sha256 = commitment(&value)?;
    validate_artifact(&value)?;
    Ok((value, families))
}
/// Closed value checks are supplemental. Only validate_current plus the sealed
/// complete private binding authorizes an action or snapshot restore.
pub fn validate_artifact(value: &NextFiveObservationV59) -> Result<(), RlContractError> {
    let inherited = &value.common.common;
    crate::snack_wide_public_v59::validate_artifact(&value.next_five.wide_life)?;
    let meanings = crate::rules_meaning_next_five_v59::FamilyV59::ALL
        .into_iter()
        .map(crate::rules_meaning_next_five_v59::printed_meaning)
        .collect::<Result<Vec<_>, _>>()
        .map_err(RlContractError)?;
    if value.next_five.schema != crate::next_five_public_v59::SCHEMA
        || value.next_five.producer_sha256 != crate::next_five_public_v59::producer_sha256()
        || value.next_five.viewer != inherited.acting_player
        || value.next_five.meanings != meanings
    {
        return Err(RlContractError(
            "next-five public identity or complete meaning order changed".into(),
        ));
    }
    inherited.validate_next_five_compatibility_v59()?;
    crate::rules_meaning_elenda_v52::validate(&value.common.elenda.meaning)
        .map_err(|e| RlContractError(format!("prepared Elenda meaning:{e:?}")))?;
    crate::rules_meaning_spinner_v54::validate(&value.spinner.meaning).map_err(RlContractError)?;
    crate::rules_meaning_quandary_v55::validate(&value.quandary.meaning)
        .map_err(RlContractError)?;
    crate::rules_meaning_phoenix_v56::validate(&value.phoenix.meaning).map_err(RlContractError)?;
    for root in &value.drake.meanings {
        crate::rules_meaning_drake_hatcher_v53::validate(root).map_err(RlContractError)?;
    }
    use crate::rules_meaning_drake_hatcher_v53::RootV53;
    if value
        .drake
        .meanings
        .iter()
        .map(|root| root.root)
        .collect::<Vec<_>>()
        != vec![
            RootV53::Prowess,
            RootV53::CombatIncubation,
            RootV53::RemoveIncubationForDrake,
        ]
        || value.drake.schema != crate::drake_public_v53::SCHEMA
        || value.drake.producer_sha256 != crate::drake_public_v53::producer_sha256()
        || value.common.elenda.schema != crate::elenda_public_v52::SCHEMA
        || value.common.elenda.producer_sha256 != crate::elenda_public_v52::producer_sha256()
        || value.common.elenda.card_db_hash != inherited.card_db_hash
        || value.spinner.schema != crate::spinner_public_v54::SCHEMA
        || value.spinner.producer_sha256 != crate::rules_meaning_spinner_v54::producer_sha256()
        || value.quandary.schema != crate::quandary_public_v55::SCHEMA
        || value.quandary.producer_sha256 != crate::rules_meaning_quandary_v55::producer_sha256()
        || value.quandary.viewer != inherited.acting_player
        || value.phoenix.schema != crate::phoenix_public_v56::SCHEMA
        || value.phoenix.producer_sha256 != crate::phoenix_public_v56::producer_sha256()
        || value.phoenix.card_db_hash != inherited.card_db_hash
    {
        return Err(RlContractError(
            "next-five family artifact identity/root order changed".into(),
        ));
    }
    if value.schema != SCHEMA
        || value.producer_sha256 != producer_sha256()
        || value.projection_sha256 != commitment(value)?
        || value.common.schema != ELENDA_COMPAT_SCHEMA
        || value.common.producer_sha256 != producer_sha256()
        || value.common.projection_sha256
            != crate::elenda_observation_v52::commitment(&value.common)?
        || inherited.schema != COMMON_SCHEMA
        || inherited.producer_sha256 != producer_sha256()
        || inherited.projection_sha256 != crate::ravenous_observation_v51::commitment(inherited)?
        || inherited.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
        || inherited.substep_count == 0
        || inherited.substep_index >= inherited.substep_count
    {
        return Err(RlContractError(
            "next-five artifact identity changed".into(),
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for seat in 0..2 {
        let exact = &value.drake.battlefield[seat];
        let narrow = &inherited.battlefield[seat];
        let base = &inherited.projection.surface.battlefield[seat];
        if exact.len() != narrow.len() || exact.len() != base.len() {
            return Err(RlContractError(
                "prepared complete battlefield roster changed".into(),
            ));
        }
        for ((card, compat), base) in exact.iter().zip(narrow).zip(base) {
            let source = &card.reference;
            let reference = &base.stable;
            let c = &card.counters;
            if [
                c.plus_one,
                i32::from(c.minus_one),
                i32::from(c.minus_toughness),
                i32::from(c.stun),
                i32::from(c.lore),
                i32::from(c.oil),
                c.charge,
                c.net,
                c.bait,
                c.soul,
                c.incubation,
                i32::from(c.lifelink),
            ]
            .iter()
            .any(|n| *n < 0)
            {
                return Err(RlContractError("prepared public counter underflow".into()));
            }
            if !seen.insert(source.incarnation.object)
                || source.incarnation.object != reference.arena_id
                || source.incarnation.generation != reference.zone_change_count
                || source.owner != reference.owner
                || source.controller != reference.controller
                || source.controller
                    != crate::rl::PlayerSeatV1::from(if seat == 0 {
                        PlayerId::P0
                    } else {
                        PlayerId::P1
                    })
                || source.zone != crate::state::Zone::Battlefield
                || source.zone != reference.zone
                || compat.card != *base
                || card.counters.plus_one != compat.plus_one_counters
                || card.counters.soul != compat.soul
                || card.marked_damage != compat.marked_damage
                || card.counters.plus_one < 0
                || card.counters.soul < 0
                || card.counters.incubation < 0
            {
                return Err(RlContractError(
                    "prepared exact card roster/value mismatch".into(),
                ));
            }
        }
    }
    Ok(())
}
pub fn validate_current(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
    value: &NextFiveObservationV59,
) -> Result<(), RlContractError> {
    if observe_v59(state, surface, actor_and_origin, step, physical, substep)? != *value {
        return Err(RlContractError(
            "prepared observation differs from current actual decision".into(),
        ));
    }
    Ok(())
}
