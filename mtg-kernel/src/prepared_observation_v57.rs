//! One current prepared-family envelope. Compatibility layouts have distinct
//! schemas and never authorize frozen V51/V52 artifacts or executable actions.
use crate::akroma_family_public_v40::diagnose_observation_error as diagnostic;
use crate::{
    ids::PlayerId,
    policy_surface_v5::{PolicyDecisionV5, PolicySurfaceV5},
    rl::{ActionSemanticV1, CardStableRefV1, RlContractError},
    state::GameState,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/prepared-observation/v57";
pub const COMMON_SCHEMA: &str = "mtg-kernel/prepared-compat-common/v57";
pub const ELENDA_COMPAT_SCHEMA: &str = "mtg-kernel/prepared-compat-elenda/v57";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedActivationV57 {
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
pub struct PreparedObservationV57 {
    pub schema: String,
    pub producer_sha256: String,
    /// Inherited layout only. Both nested schemas are distinct Prepared schemas.
    /// Narrow card slots never supersede the mandatory drake.battlefield roster.
    pub common: crate::elenda_observation_v52::ElendaObservationV52,
    pub drake: crate::drake_public_v53::FactsV53,
    pub spinner: crate::spinner_public_v54::FactsV54,
    pub quandary: crate::quandary_public_v55::FactsV55,
    pub phoenix: crate::phoenix_public_v56::FactsV56,
    /// Exact same order/cardinality as the genuine current action vector.
    pub action_meanings: Vec<Option<PreparedActivationV57>>,
    pub projection_sha256: String,
}
pub(crate) type PreparedPublicPartsV57 = (
    PreparedObservationV57,
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
            include_str!("prepared_observation_v57.rs"),
            include_str!("rl_session/prepared_v57.rs"),
            include_str!("rl_session.rs"),
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
            include_str!("frenzied_goblin_v1.rs"),
            include_str!("rules_meaning_frenzied_goblin_v1.rs"),
            include_str!("rules_meaning_sower_v1.rs"),
            include_str!("effect/frenzied_goblin_resolution_v1.rs"),
            include_str!("sower_restriction_capture_v1.rs"),
            include_str!("frenzied_public_v1.rs"),
            include_str!("sower_public_v1.rs"),
            include_str!("juggernaut_public_v1.rs"),
            include_str!("crystal_public_v1.rs"),
            include_str!("twinflame_observation_v1.rs"),
            include_str!("engine.rs"),
            include_str!("event.rs"),
            include_str!("effect.rs"),
            include_str!("state.rs"),
            include_str!("trigger.rs"),
            include_str!("policy_surface_v5.rs"),
            include_str!("wildwood_public_v1.rs"),
            include_str!("authority_public_v1.rs"),
            include_str!("employment_public_v1.rs"),
            include_str!("cerberus_public_v1.rs"),
            include_str!("librarian_public_v1.rs"),
            include_str!("extra_turn_public_v1.rs"),
            include_str!("ashroot_public_v1.rs"),
            include_str!("banishing_public_v1.rs"),
            include_str!("stromkirk_public_v1.rs"),
            include_str!("gourmand_public_v1.rs"),
            include_str!("garruk_public_v1.rs"),
            include_str!("arahbo_public_v1.rs"),
            include_str!("strongbox_public_v1.rs"),
            include_str!("fiery_observation_v1.rs"),
            include_str!("krenko_observation_v1.rs"),
            include_str!("zul_ashur_observation_v1.rs"),
            include_str!("banner_observation_v1.rs"),
            include_str!("inspiration_observation_v1.rs"),
            include_str!("fishing_pole_observation_v39.rs"),
        ] {
            hash.update((part.len() as u64).to_le_bytes());
            hash.update(part.as_bytes());
        }
        hash.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    })
    .clone()
}
pub fn commitment(value: &PreparedObservationV57) -> Result<String, RlContractError> {
    let mut value = value.clone();
    value.projection_sha256.clear();
    let bytes = serde_json::to_vec(&value).map_err(|e| RlContractError(e.to_string()))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    crate::engine::validate_battlefield_pt(state)
        .map_err(|id| format!("native PT envelope exceeded for {id:?}"))?;
    crate::ravenous_observation_v51::validate_state_complete_prepared_v57(state)?;
    crate::elenda_public_v52::capture(state).map_err(|e| e.0)?;
    crate::drake_public_v53::capture(state).map_err(|e| e.0)?;
    crate::spinner_public_v54::capture(state)?;
    // Validate every choice owner, not whichever seat currently has priority.
    for viewer in [PlayerId::P0, PlayerId::P1] {
        crate::quandary_public_v55::capture(state, viewer)?;
    }
    crate::phoenix_public_v56::capture(state).map_err(|e| e.0)?;
    Ok(())
}
fn annotation(
    state: &GameState,
    action: &ActionSemanticV1,
) -> Result<Option<PreparedActivationV57>, RlContractError> {
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
    Ok(Some(PreparedActivationV57 {
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
                "Drake Hatcher" | "Spinner of Souls" | "Painful Quandary" | "Flamewake Phoenix"
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

pub fn observe_v57(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<PreparedObservationV57, RlContractError> {
    observe_parts_v57(state, surface, actor_and_origin, step, physical, substep)
        .map(|(value, _)| value)
}
pub(crate) fn observe_parts_v57(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<PreparedPublicPartsV57, RlContractError> {
    observe_parts_v57_using_inherited(
        state,
        surface,
        actor_and_origin,
        step,
        physical,
        substep,
        crate::ravenous_observation_v51::observe_parts_complete_prepared_v57,
    )
}

/// Keep a surrounding immutable request's optional history-key frame. This is
/// the same complete observation body: without a matching frame, key derivation
/// takes its ordinary uncached path. No validator or binding check is skipped.
pub(crate) fn observe_parts_v57_in_request(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<PreparedPublicPartsV57, RlContractError> {
    #[cfg(test)]
    let _timing = crate::rl_session::test_timing_v1::span(
        crate::rl_session::test_timing_v1::Phase::PreparedObservePartsInclusive,
    );
    observe_parts_v57_in_frame(
        state,
        surface,
        actor_and_origin,
        step,
        physical,
        substep,
        crate::ravenous_observation_v51::observe_parts_complete_prepared_v57,
    )
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) fn observe_parts_v57_fully_rebuilt(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<PreparedPublicPartsV57, RlContractError> {
    observe_parts_v57_using_inherited(
        state,
        surface,
        actor_and_origin,
        step,
        physical,
        substep,
        crate::ravenous_observation_v51::observe_parts_complete_prepared_v57_fully_rebuilt,
    )
}

type InheritedPartsBuilderV57 =
    fn(
        &GameState,
        &PolicySurfaceV5,
        (PlayerId, &PolicyDecisionV5),
        u64,
        u64,
        (u32, u32),
    ) -> Result<crate::ravenous_observation_v51::RavenousPublicPartsV51, RlContractError>;

fn observe_parts_v57_using_inherited(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
    build_inherited: InheritedPartsBuilderV57,
) -> Result<PreparedPublicPartsV57, RlContractError> {
    #[cfg(test)]
    let _timing = crate::rl_session::test_timing_v1::span(
        crate::rl_session::test_timing_v1::Phase::PreparedObservePartsInclusive,
    );
    crate::drake_hatcher_v1::custody::with_observation_replay_keys(state, |state| {
        observe_parts_v57_in_frame(
            state,
            surface,
            actor_and_origin,
            step,
            physical,
            substep,
            build_inherited,
        )
    })
}
fn observe_parts_v57_in_frame(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
    build_inherited: InheritedPartsBuilderV57,
) -> Result<PreparedPublicPartsV57, RlContractError> {
    validate_state(state)
        .map_err(|error| diagnostic("prepared.state", error))
        .map_err(RlContractError)?;
    let (inherited, families) =
        build_inherited(state, surface, actor_and_origin, step, physical, substep)
            .map_err(|error| diagnostic("prepared.inherited", error))?;
    let mut common = crate::elenda_observation_v52::ElendaObservationV52 {
        schema: ELENDA_COMPAT_SCHEMA.into(),
        producer_sha256: producer_sha256(),
        common: inherited,
        elenda: crate::elenda_public_v52::capture(state)
            .map_err(|error| diagnostic("prepared.elenda", error))?,
        projection_sha256: String::new(),
    };
    common.projection_sha256 = crate::elenda_observation_v52::commitment(&common)
        .map_err(|error| diagnostic("prepared.elenda_commitment", error))?;
    let candidates = crate::rl_session::prepared_candidates_v57(actor_and_origin.1, state, surface)
        .map_err(|error| diagnostic("prepared.candidates", error))?;
    let action_meanings = candidates
        .iter()
        .map(|candidate| {
            annotation(state, &candidate.semantic)
                .map_err(|error| diagnostic("prepared.annotation", error))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut value = PreparedObservationV57 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        common,
        drake: crate::drake_public_v53::capture(state)
            .map_err(|error| diagnostic("prepared.drake", error))?,
        spinner: crate::spinner_public_v54::capture(state)
            .map_err(|error| diagnostic("prepared.spinner", error))
            .map_err(RlContractError)?,
        quandary: crate::quandary_public_v55::capture(state, actor_and_origin.0)
            .map_err(|error| diagnostic("prepared.quandary", error))
            .map_err(RlContractError)?,
        phoenix: crate::phoenix_public_v56::capture(state)
            .map_err(|error| diagnostic("prepared.phoenix", error))?,
        action_meanings,
        projection_sha256: String::new(),
    };
    value.projection_sha256 =
        commitment(&value).map_err(|error| diagnostic("prepared.commitment", error))?;
    validate_artifact(&value).map_err(|error| diagnostic("prepared.artifact", error))?;
    Ok((value, families))
}
/// Closed value checks are supplemental. Only validate_current plus the sealed
/// complete private binding authorizes an action or snapshot restore.
pub fn validate_artifact(value: &PreparedObservationV57) -> Result<(), RlContractError> {
    let inherited = &value.common.common;
    inherited.validate_prepared_compatibility_v57()?;
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
        return Err(diagnostic(
            "prepared_artifact.prepared_family_artifact_identity_root_order_changed",
            RlContractError("prepared family artifact identity/root order changed".into()),
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
        return Err(diagnostic(
            "prepared_artifact.prepared_artifact_identity_changed",
            RlContractError("prepared artifact identity changed".into()),
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for seat in 0..2 {
        let exact = &value.drake.battlefield[seat];
        let narrow = &inherited.battlefield[seat];
        let base = &inherited.projection.surface.battlefield[seat];
        if exact.len() != narrow.len() || exact.len() != base.len() {
            return Err(diagnostic(
                "prepared_artifact.prepared_complete_battlefield_roster_changed",
                RlContractError("prepared complete battlefield roster changed".into()),
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
                return Err(diagnostic(
                    "prepared_artifact.prepared_public_counter_underflow",
                    RlContractError("prepared public counter underflow".into()),
                ));
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
                return Err(diagnostic(
                    "prepared_artifact.prepared_exact_card_roster_value_mismatch",
                    RlContractError("prepared exact card roster/value mismatch".into()),
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
    value: &PreparedObservationV57,
) -> Result<(), RlContractError> {
    if observe_v57(state, surface, actor_and_origin, step, physical, substep)? != *value {
        return Err(RlContractError(
            "prepared observation differs from current actual decision".into(),
        ));
    }
    Ok(())
}
