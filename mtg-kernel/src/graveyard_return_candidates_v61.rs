//! Read-only candidates for an authenticated, already-current exact-bound V5
//! surface. Original executable pairs and indices stay private. Public rows
//! preserve native visible order, except positionless library normalization,
//! and complete ordinary semantics, with mandatory typed
//! V61 context. This is not a session, restore capability or action dispatcher.
use crate::{
    graveyard_return_observation_v61::{self as observation, GraveyardReturnObservationV61},
    graveyard_return_public_v61::{
        DeathV61, ProgramV61, ScheduleV61, StageV61, TargetV61, TriggerV61,
    },
    ids::PlayerId,
    optional_trigger_public_v1::{IdentityV1, ReferenceV1},
    policy_surface_v5::{PolicyDecisionV5, PolicySurfaceV5},
    rl::{ActionSemanticV1 as A, CardStableRefV1, RlContractError, TargetRefV1},
    state::{GameState, Zone},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/graveyard-return-candidates/v61";
pub const ANNOTATION_SCHEMA: &str = "mtg-kernel/graveyard-return-candidate-meaning/v61";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateMeaningV61 {
    pub schema: String,
    /// Public ordinal only. Original native indices remain private, including
    /// library positions and indices of infeasible omitted combat answers.
    pub ordinal: u32,
    /// Full ordinary semantics, with source roles frozen to the authenticated
    /// public incarnation. Private PolicyActionV5 operands are never serialized.
    #[serde(deserialize_with = "strict_action")]
    pub action: A,
    #[serde(deserialize_with = "required_optional")]
    pub drake_activation: Option<crate::next_five_observation_v59::NextFiveActivationV59>,
    /// Complete current programs at this genuine decision, in native stage and
    /// position order. An ordinary pass does not become a delayed-return choice.
    pub programs: Vec<TriggerV61>,
    #[serde(deserialize_with = "required_optional")]
    pub selected_graveyard_target: Option<TargetV61>,
    pub nine_lives_deaths: Vec<DeathV61>,
    pub nine_lives_schedules: Vec<ScheduleV61>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraveyardReturnCandidatesV61 {
    pub schema: String,
    pub producer_sha256: String,
    /// Carries all inherited ordinary and family meaning, plus all sixteen
    /// native counters. Narrow special annotations never replace this context.
    pub observation: GraveyardReturnObservationV61,
    pub candidates: Vec<CandidateMeaningV61>,
    /// The existing native Alesha root currently forces its legal return. This
    /// source projection does not invent the missing printed resolution-time may.
    pub alesha_optional_resolution_supported: bool,
    pub owning_consumer_admitted: bool,
    pub projection_sha256: String,
}

pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let inherited = observation::producer_sha256();
        let mut hash = Sha256::new();
        for part in [
            SCHEMA,
            ANNOTATION_SCHEMA,
            inherited.as_str(),
            include_str!("graveyard_return_candidates_v61.rs"),
            include_str!("lib.rs"),
            include_str!("rl.rs"),
            include_str!("rl_session.rs"),
            include_str!("divine_actions_v44.rs"),
            include_str!("rl_variable_cast_v1.rs"),
            include_str!("next_five_observation_v59.rs"),
            include_str!("rl_session/banner_v36.rs"),
            include_str!("rl_session/inspiration_v37.rs"),
            include_str!("rl_session/flat_action_v3.rs"),
            include_str!("policy_surface_v5.rs"),
            include_str!("policy_surface_v5/hunter_order_v45.rs"),
        ] {
            hash.update((part.len() as u64).to_le_bytes());
            hash.update(part.as_bytes());
        }
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}

pub fn commitment(value: &GraveyardReturnCandidatesV61) -> Result<String, RlContractError> {
    let mut value = value.clone();
    value.projection_sha256.clear();
    let bytes = serde_json::to_vec(&value).map_err(error)?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
fn error(e: impl std::fmt::Display) -> RlContractError {
    RlContractError(e.to_string())
}

fn required_optional<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

// The frozen action enum deliberately retains its old tolerant deserializer.
// V61 requires the exact canonical typed shape, without changing old contracts.
fn strict_action<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<A, D::Error> {
    let value = Value::deserialize(deserializer)?;
    let action: A = serde_json::from_value(value.clone()).map_err(serde::de::Error::custom)?;
    if serde_json::to_value(&action).map_err(serde::de::Error::custom)? != value {
        return Err(serde::de::Error::custom(
            "V61 action has noncanonical or unknown fields",
        ));
    }
    Ok(action)
}

/// Collect only exact typed reference shapes from an already-public artifact.
/// This is a visibility check, not native decision authentication. Native pairs
/// are reconstructed separately and are never accepted from this JSON.
fn references(value: &Value, output: &mut Vec<CardStableRefV1>) -> Result<(), RlContractError> {
    match value {
        Value::Object(fields) if fields.contains_key("arena_id") => {
            let reference: CardStableRefV1 =
                serde_json::from_value(value.clone()).map_err(error)?;
            if fields.len() != 6 {
                return Err(error("V61 stable reference shape changed"));
            }
            output.push(reference);
        }
        Value::Object(fields) => {
            if fields.len() == 6
                && fields.contains_key("identity")
                && fields.contains_key("generation")
            {
                let r: ReferenceV1 = serde_json::from_value(value.clone()).map_err(error)?;
                let card_db_id = match r.identity {
                    IdentityV1::Known { card_id } => card_id,
                    IdentityV1::FaceDown { disguised } => {
                        crate::card_def::card_id_by_name(if disguised {
                            "Face-down creature"
                        } else {
                            "Face-down card"
                        })
                        .ok_or_else(|| error("V61 face-down sentinel missing"))?
                    }
                };
                output.push(CardStableRefV1 {
                    arena_id: r.object,
                    card_db_id,
                    owner: r.owner,
                    controller: r.controller,
                    zone: r.zone,
                    zone_change_count: r.generation,
                });
            } else {
                for field in fields.values() {
                    references(field, output)?;
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                references(value, output)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn authorized_references(
    value: &GraveyardReturnObservationV61,
) -> Result<Vec<CardStableRefV1>, RlContractError> {
    let mut authority = serde_json::to_value(value).map_err(error)?;
    mask_incarnation_relations(value, &mut authority)?;
    let mut refs = Vec::new();
    references(&authority, &mut refs)?;
    Ok(refs)
}

/// These closed typed parent routes carry incarnation relations, never a
/// printing identity. Mask only those fields in the temporary authority scan;
/// the actual complete public observation and its commitments remain intact.
/// The generic action-reference scan still rejects every incarnation-only or
/// unknown `arena_id` shape, including one outside these authenticated routes.
fn mask_incarnation_relations(
    value: &GraveyardReturnObservationV61,
    authority: &mut Value,
) -> Result<(), RlContractError> {
    use crate::akroma_family_public_v40::FamilyFactsV40 as F;
    for (index, row) in value.common.common.ajani.activations.iter().enumerate() {
        let base = format!("/common/common/ajani/activations/{index}");
        mask_incarnation_relation(authority, &format!("{base}/target"), &row.target)?;
        mask_incarnation_relation(
            authority,
            &format!("{base}/created_tokens"),
            &row.created_tokens,
        )?;
    }
    for (family_index, family) in value.common.common.families.iter().enumerate() {
        let base = format!("/common/common/families/{family_index}/facts");
        match family {
            F::Fishing(facts) => {
                for (index, row) in facts.poles.iter().enumerate() {
                    mask_incarnation_relation(
                        authority,
                        &format!("{base}/poles/{index}/attached_host"),
                        &row.attached_host,
                    )?;
                }
                for (index, row) in facts.operations.iter().enumerate() {
                    let operation = format!("{base}/operations/{index}");
                    mask_incarnation_relation(
                        authority,
                        &format!("{operation}/original_host"),
                        &row.original_host,
                    )?;
                    mask_incarnation_relation(
                        authority,
                        &format!("{operation}/created_fish"),
                        &row.created_fish,
                    )?;
                }
            }
            F::Fiery(facts) => {
                if let Some(row) = &facts.facts.pending_cast_targets {
                    mask_incarnation_relation(
                        authority,
                        &format!("{base}/facts/pending_cast_targets/selected_targets"),
                        &row.selected_targets,
                    )?;
                }
                for (index, row) in facts.facts.death_replacements.iter().enumerate() {
                    mask_incarnation_relation(
                        authority,
                        &format!("{base}/facts/death_replacements/{index}/original_target"),
                        &row.original_target,
                    )?;
                }
            }
            F::Krenko(facts) => {
                for (index, row) in facts.facts.activations.iter().enumerate() {
                    mask_incarnation_relation(
                        authority,
                        &format!("{base}/facts/activations/{index}/created_tokens"),
                        &row.created_tokens,
                    )?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn mask_incarnation_relation(
    authority: &mut Value,
    path: &str,
    typed: &impl Serialize,
) -> Result<(), RlContractError> {
    let expected = serde_json::to_value(typed).map_err(error)?;
    fn pair(value: &Value) -> bool {
        match value {
            Value::Null => true,
            Value::Array(rows) => rows.iter().all(pair),
            Value::Object(fields) => {
                fields.len() == 2
                    && fields
                        .get("arena_id")
                        .and_then(Value::as_u64)
                        .is_some_and(|v| u32::try_from(v).is_ok())
                    && fields
                        .get("zone_change_count")
                        .and_then(Value::as_u64)
                        .is_some_and(|v| u32::try_from(v).is_ok())
            }
            _ => false,
        }
    }
    let actual = authority
        .pointer_mut(path)
        .ok_or_else(|| error("V61 typed incarnation parent route changed"))?;
    if !pair(&expected) || *actual != expected {
        return Err(error(
            "V61 incarnation relation differs from its exact typed parent field",
        ));
    }
    *actual = Value::Null;
    Ok(())
}
fn validate_references(action: &A, authorized: &[CardStableRefV1]) -> Result<(), RlContractError> {
    let mut refs = Vec::new();
    references(&serde_json::to_value(action).map_err(error)?, &mut refs)?;
    if refs.iter().any(|r| !authorized.contains(r)) {
        return Err(error(
            "V61 candidate reference is outside the actor's complete public projection",
        ));
    }
    Ok(())
}

fn visible_action(
    value: &GraveyardReturnObservationV61,
    mut action: A,
) -> Result<A, RlContractError> {
    let context = &value.common.common.projection.surface.engine_context;
    let pending = |position: usize, source: &mut CardStableRefV1| -> Result<(), RlContractError> {
        if let Some(captured) = context
            .pending_triggers
            .get(position)
            .and_then(|p| p.source.as_ref())
        {
            if source.arena_id == captured.arena_id {
                *source = captured.clone();
            }
        }
        Ok(())
    };
    match &mut action {
        #[cfg(feature = "limited-fdn-fixtures")]
        A::ChooseTriggerOrderNext {
            source,
            trigger_index,
            ..
        } => pending(*trigger_index, source)?,
        A::OrderTriggers {
            pending_sources, ..
        } => {
            for (position, source) in pending_sources.iter_mut().enumerate() {
                pending(position, source)?;
            }
        }
        A::ChooseTarget { source, .. }
        | A::FinishTargetSelection { source, .. }
        | A::ChooseSpellMode { source, .. } => pending(0, source)?,
        A::ChooseEffectOption { source, .. }
        | A::ChooseEffectTarget { source, .. }
        | A::FinishEffectSelection { source, .. }
        | A::ChooseEffectColor { source, .. }
        | A::ChooseEffectNumber { source, .. }
        | A::ChooseEffectBoolean { source, .. } => {
            if let Some(captured) = context
                .pending_effect
                .as_ref()
                .and_then(|p| p.source.as_ref())
            {
                if source.arena_id != captured.arena_id {
                    return Err(error("V61 candidate resolving source changed"));
                }
                *source = captured.clone();
            }
        }
        // Every other original semantic field, including target incarnation,
        // native choice index, selected count and trigger permutation, survives.
        _ => {}
    }
    Ok(action)
}

fn selected_graveyard_target(
    value: &GraveyardReturnObservationV61,
    action: &A,
) -> Result<Option<TargetV61>, RlContractError> {
    let A::ChooseTarget {
        source,
        target: TargetRefV1::Object { object },
        ..
    } = action
    else {
        return Ok(None);
    };
    let Some(program) = value.graveyard_return.triggers.iter().find(|p| {
        p.stage == StageV61::Held
            && p.position == 0
            && p.source.original_source.object == source.arena_id
            && p.source.original_source.generation == source.zone_change_count
            && matches!(p.program, ProgramV61::AleshaRaid { .. })
    }) else {
        return Ok(None);
    };
    if object.zone != Zone::Graveyard || object.owner != program.source.captured_controller {
        return Err(error("V61 Raid target lost its exact graveyard relation"));
    }
    let original = ReferenceV1 {
        object: object.arena_id,
        generation: object.zone_change_count,
        owner: object.owner,
        controller: object.controller,
        zone: object.zone,
        identity: IdentityV1::Known {
            card_id: object.card_db_id,
        },
    };
    Ok(Some(TargetV61 {
        current_exact_graveyard_card: Some(original.clone()),
        original,
        legal_now: true,
    }))
}

fn drake_activation(
    value: &GraveyardReturnObservationV61,
    action: &A,
) -> Result<Option<crate::next_five_observation_v59::NextFiveActivationV59>, RlContractError> {
    let A::ActivateAbility {
        actor,
        source,
        ability_index: 0,
        granted_ability,
    } = action
    else {
        return Ok(None);
    };
    if !granted_ability.is_none()
        || crate::card_def::CARD_DEFS
            .get(usize::from(source.card_db_id))
            .is_none_or(|d| d.name != "Drake Hatcher")
    {
        return Ok(None);
    }
    let row = value
        .drake
        .battlefield
        .iter()
        .flatten()
        .find(|row| {
            row.reference.incarnation.object == source.arena_id
                && row.reference.incarnation.generation == source.zone_change_count
        })
        .ok_or_else(|| error("V61 Drake candidate has no exact public permanent"))?;
    if !row.form.printed_abilities_active
        || matches!(
            row.reference.identity,
            crate::drake_public_v53::IdentityV53::FaceDown { .. }
        )
    {
        return Ok(None);
    }
    let before = row.counters.incubation;
    if *actor != source.controller || source.zone != Zone::Battlefield || before < 3 {
        return Err(error(
            "V61 Drake candidate lost its exact public counter payment",
        ));
    }
    Ok(Some(
        crate::next_five_observation_v59::NextFiveActivationV59 {
            source: source.clone(),
            ability_index: 0,
            kind: crate::rules_meaning_drake_hatcher_v53::CounterV53::Incubation,
            count: 3,
            before,
            after: before
                .checked_sub(3)
                .ok_or_else(|| error("V61 Drake payment underflow"))?,
            any_priority: true,
            costs_paid_before_stack: true,
            countering_refunds_costs: false,
        },
    ))
}

/// Private exact original pairs and public-to-native mapping. No Debug or
/// serialization, no supplied binding accepted, and no action application API.
pub(crate) struct NativeCandidateProjectionV61 {
    pub(crate) original: Vec<crate::rl::CorePolicyActionCandidateV1>,
    pub(crate) public_to_original: Vec<usize>,
}

pub fn project_v61(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<GraveyardReturnCandidatesV61, RlContractError> {
    project_parts_v61(state, surface, actor_and_origin, step, physical, substep)
        .map(|(value, _)| value)
}

pub(crate) fn project_parts_v61(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<(GraveyardReturnCandidatesV61, NativeCandidateProjectionV61), RlContractError> {
    observation::validate_state(state).map_err(error)?;
    // Exact-bound source surfaces authenticate their own private scan. Owned
    // revision surfaces fail here until a separately assigned owner exists.
    // Nonscan re-entry must also be genuinely idempotent. A native Drake
    // Enclosing receipt is a real world change, even if its menu is unchanged;
    // this guard refuses it until an actual read-only issued-origin API exists.
    let mut probe = state.clone();
    let mut probe_surface = surface.clone();
    let actual = probe_surface.next_decision(&mut probe).map_err(error)?;
    if actual != *actor_and_origin.1
        || probe != *state
        || actual.actor(state) != Some(actor_and_origin.0)
    {
        return Err(error(
            "V61 candidates require the actual already-current exact-bound surface",
        ));
    }
    let observation =
        observation::observe_v61(state, surface, actor_and_origin, step, physical, substep)?;
    let authorized = authorized_references(&observation)?;
    // No native action or original semantic enters the public value. Keep the
    // original vector intact so index i continues to bind its exact native pair.
    let original =
        crate::rl_session::graveyard_return_candidates_v61(actor_and_origin.1, state, surface)?;
    let feasible = if matches!(
        actor_and_origin.1,
        PolicyDecisionV5::AttackerInclusion { .. } | PolicyDecisionV5::BlockerInclusion { .. }
    ) {
        Some(
            surface
                .feasible_scan_answers(state, actor_and_origin.1)
                .map_err(error)?,
        )
    } else {
        None
    };
    let mut projected = Vec::new();
    for (index, candidate) in original.iter().enumerate() {
        if let Some(feasible) = feasible {
            let include = match candidate.semantic {
                A::ChooseAttackerInclusion { include, .. }
                | A::ChooseBlockerInclusion { include, .. } => include,
                _ => return Err(error("V61 scan candidate changed semantic family")),
            };
            if !feasible[usize::from(include)] {
                continue;
            }
        }
        let action = visible_action(&observation, candidate.semantic.clone())?;
        validate_references(&action, &authorized)?;
        projected.push((
            index,
            CandidateMeaningV61 {
                schema: ANNOTATION_SCHEMA.into(),
                ordinal: 0,
                selected_graveyard_target: selected_graveyard_target(&observation, &action)?,
                drake_activation: drake_activation(&observation, &action)?,
                action,
                programs: observation.graveyard_return.triggers.clone(),
                nine_lives_deaths: observation.graveyard_return.nine_lives_deaths.clone(),
                nine_lives_schedules: observation.graveyard_return.nine_lives_schedules.clone(),
            },
        ));
    }
    if let Some(search) = &observation.common.common.extensions.decision_local_library {
        let mut ordered = projected
            .into_iter()
            .map(|(index, row)| {
                crate::rl::positionless_library_candidate_order(&row.action, search)
                    .map(|order| (order, index, row))
            })
            .collect::<Result<Vec<_>, _>>()?;
        ordered.sort_by_key(|(order, _, _)| *order);
        projected = ordered
            .into_iter()
            .map(|(_, index, row)| (index, row))
            .collect();
    }
    let public_to_original = projected.iter().map(|(index, _)| *index).collect();
    let candidates = projected
        .into_iter()
        .enumerate()
        .map(|(ordinal, (_, mut row))| {
            row.ordinal = u32::try_from(ordinal).map_err(error)?;
            Ok(row)
        })
        .collect::<Result<Vec<_>, RlContractError>>()?;
    let mut value = GraveyardReturnCandidatesV61 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        observation,
        candidates,
        alesha_optional_resolution_supported: false,
        owning_consumer_admitted: false,
        projection_sha256: String::new(),
    };
    value.projection_sha256 = commitment(&value)?;
    validate_artifact(&value)?;
    Ok((
        value,
        NativeCandidateProjectionV61 {
            original,
            public_to_original,
        },
    ))
}

/// Supplemental closed-shape checks. Only validate_current authenticates this
/// artifact against the native world; neither function grants execution/restore.
pub fn validate_artifact(value: &GraveyardReturnCandidatesV61) -> Result<(), RlContractError> {
    observation::validate_artifact(&value.observation)?;
    if value.schema != SCHEMA
        || value.producer_sha256 != producer_sha256()
        || value.owning_consumer_admitted
        || value.alesha_optional_resolution_supported
        || value.candidates.is_empty()
        || value.projection_sha256 != commitment(value)?
    {
        return Err(error("V61 candidate artifact identity changed"));
    }
    let authorized = authorized_references(&value.observation)?;
    let facts = &value.observation.graveyard_return;
    for (ordinal, row) in value.candidates.iter().enumerate() {
        validate_references(&row.action, &authorized)?;
        let action = serde_json::to_value(&row.action).map_err(error)?;
        if action.get("actor")
            != Some(
                &serde_json::to_value(value.observation.common.common.acting_player)
                    .map_err(error)?,
            )
            || matches!(
                row.action,
                A::Ambiguous { .. }
                    | A::DeclareAttackers { .. }
                    | A::DeclareBlockersForAttacker { .. }
            )
        {
            return Err(error(
                "V61 candidate changed its actual policy actor or action family",
            ));
        }
        if row.schema != ANNOTATION_SCHEMA
            || usize::try_from(row.ordinal).ok() != Some(ordinal)
            || row.programs != facts.triggers
            || row.nine_lives_deaths != facts.nine_lives_deaths
            || row.nine_lives_schedules != facts.nine_lives_schedules
            || row.drake_activation != drake_activation(&value.observation, &row.action)?
            || row.selected_graveyard_target
                != selected_graveyard_target(&value.observation, &row.action)?
        {
            return Err(error("V61 mandatory candidate annotation changed"));
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
    value: &GraveyardReturnCandidatesV61,
) -> Result<(), RlContractError> {
    if project_v61(state, surface, actor_and_origin, step, physical, substep)? != *value {
        return Err(error(
            "V61 candidate artifact differs from current native pairs",
        ));
    }
    Ok(())
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
