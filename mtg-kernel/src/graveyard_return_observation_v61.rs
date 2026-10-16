//! Complete source-only GraveyardReturn V61 projection. Distinct compatibility
//! schemas plus the mandatory sixteen-counter roster preserve every native value.
//! This module grants no session, action execution, model or transport admission.
use crate::{
    ids::PlayerId,
    policy_surface_v5::{PolicyDecisionV5, PolicySurfaceV5},
    rl::{CardStableRefV1, RlContractError},
    state::GameState,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/graveyard-return-observation/v61";
pub const COMMON_SCHEMA: &str = "mtg-kernel/graveyard-return-compat-common/v61";
pub const ELENDA_COMPAT_SCHEMA: &str = "mtg-kernel/graveyard-return-compat-elenda/v61";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraveyardReturnObservationV61 {
    pub schema: String,
    pub producer_sha256: String,
    /// Inherited layout only. Both nested schemas are distinct V61 schemas.
    /// Compatibility slots never supersede graveyard_return.battlefield_counters.
    pub common: crate::elenda_observation_v52::ElendaObservationV52,
    pub drake: crate::drake_public_v53::FactsV53,
    pub spinner: crate::spinner_public_v54::FactsV54,
    pub quandary: crate::quandary_public_v55::FactsV55,
    pub phoenix: crate::phoenix_public_v56::FactsV56,
    pub next_five: crate::next_five_public_v59::FactsV59,
    pub graveyard_return: crate::graveyard_return_public_v61::FactsV61,
    pub complete_ordinary_observation: bool,
    pub owning_consumer_admitted: bool,
    pub projection_sha256: String,
}
pub(crate) type GraveyardReturnPublicPartsV61 = (
    GraveyardReturnObservationV61,
    Vec<crate::akroma_family_public_v40::FamilyProjectionV40>,
);
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let inherited = crate::next_five_observation_v59::producer_sha256();
        let native = crate::graveyard_return_public_v61::producer_sha256();
        let mut hash = Sha256::new();
        for part in [
            SCHEMA,
            COMMON_SCHEMA,
            ELENDA_COMPAT_SCHEMA,
            include_str!("graveyard_return_observation_v61.rs"),
            include_str!("ravenous_observation_v51.rs"),
            include_str!("ravenous_public_v51.rs"),
            include_str!("rl.rs"),
            include_str!("rl_session/graveyard_return_v61.rs"),
            include_str!("akroma_family_public_v40.rs"),
            include_str!("optional_trigger_public_v1.rs"),
            include_str!("drake_public_v53.rs"),
            include_str!("spinner_public_v54.rs"),
            include_str!("quandary_public_v55.rs"),
            include_str!("next_five_public_v59.rs"),
            inherited.as_str(),
            native.as_str(),
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
            include_str!("bloodthirsty_conqueror_v49.rs"),
            include_str!("fiendish_panda_v1.rs"),
            include_str!("fiendish_panda_v1/custody.rs"),
            include_str!("life_gain_turn_v1.rs"),
            include_str!("midnight_snack_v1.rs"),
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
pub fn commitment(value: &GraveyardReturnObservationV61) -> Result<String, RlContractError> {
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
    require_supported(state, None)?;
    crate::ravenous_observation_v51::validate_state_complete_prepared_v57(state)?;
    crate::elenda_public_v52::capture(state).map_err(|e| e.0)?;
    crate::drake_public_v53::capture_complete_v61(state).map_err(|e| e.0)?;
    crate::spinner_public_v54::capture_complete_v61(state)?;
    crate::phoenix_public_v56::capture(state).map_err(|e| e.0)?;
    for viewer in [PlayerId::P0, PlayerId::P1] {
        crate::quandary_public_v55::capture_complete_v61(state, viewer)?;
        crate::next_five_public_v59::capture_complete_v61(state, viewer)?;
        crate::graveyard_return_public_v61::capture(state, viewer)?;
    }
    Ok(())
}

/// Reject only authorized identities and public producing history. Unrevealed
/// opponent hand/library contents never select a refusal. IDs >=459 are outside
/// the closed two-family appendix, even when a supported root returns that card.
fn require_supported(state: &GameState, viewer: Option<PlayerId>) -> Result<(), String> {
    use crate::{event::CommittedEvent as E, optional_trigger_public_v1 as public};
    let unsupported = |id: u16| id >= 459;
    let visible = state.objects.iter().any(|(id, live)| {
        public::reference(state, id, viewer).is_some_and(
            |r| matches!(r.identity, public::IdentityV1::Known { card_id } if unsupported(card_id)),
        ) || viewer.is_some_and(|viewer| {
            live.v4
                .face_down_v1
                .is_some_and(|face| face.lookers & (1 << viewer.index()) != 0)
                && unsupported(live.card_def)
        })
    });
    let item_unsupported = |item: &crate::state::StackItem| {
        item.v4
            .source_contract
            .is_some_and(|s| unsupported(s.card_def))
            || item
                .v4
                .ability_source_contract
                .is_some_and(|s| unsupported(s.card_def))
    };
    let retained = state.stack.iter().any(item_unsupported)
        || state
            .engine
            .pending_effect
            .as_ref()
            .is_some_and(|p| item_unsupported(&p.resolving_item))
        || public::held(state)
            .iter()
            .any(|p| p.source_contract.is_some_and(|s| unsupported(s.card_def)))
        || state.players.iter().any(|p| {
            p.restricted_mana_pool
                .0
                .iter()
                .any(|m| unsupported(m.source_card_def))
        })
        || state.engine.event_history.iter().any(|e| match e {
            E::ControlledCreatureDeathCaptureV1 { source, .. } => unsupported(source.card_def),
            // Native SpellCast is logged only after successful finalization,
            // never for an uncast spell copy. Its bare marker does not identify
            // a disguised printing. The native face-down cast route requires
            // this exact definition-owned alternative, so only definitions
            // without that route can supply historical public identity here.
            // A later zone change keeps the physical definition, not its old
            // incarnation metadata; no later hidden reference is published.
            E::SpellCast { spell, .. } => state.objects.try_get(*spell).is_some_and(|o| {
                unsupported(o.card_def)
                    && crate::card_def::CARD_DEFS
                        .get(usize::from(o.card_def))
                        .is_some_and(|d| {
                            !d.alt_cost.is_some_and(|alt| {
                                alt.condition == crate::card_def::AltCostCondition::DisguiseFromHand
                            })
                        })
            }),
            _ => false,
        });
    if visible || retained {
        Err("V61 has no Call, Artist or later-family semantics".into())
    } else {
        Ok(())
    }
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
                    | "Alesha, Who Laughs at Fate"
                    | "Nine-Lives Familiar"
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
            "V61 source requires unique genuine live or resolving full row".into(),
        ));
    }
    if item.source != source.source || item.controller != source.controller {
        return Err(RlContractError(
            "V61 original source/controller changed".into(),
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
            "V61 source requires unique genuine held trigger".into(),
        ));
    }
    Ok(Some(captured_reference(state, source)))
}

pub fn observe_v61(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<GraveyardReturnObservationV61, RlContractError> {
    observe_parts_v61(state, surface, actor_and_origin, step, physical, substep)
        .map(|(value, _)| value)
}
pub(crate) fn observe_parts_v61(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<GraveyardReturnPublicPartsV61, RlContractError> {
    validate_state(state).map_err(RlContractError)?;
    require_supported(state, Some(actor_and_origin.0)).map_err(RlContractError)?;
    let (inherited, families) =
        crate::ravenous_observation_v51::observe_parts_complete_graveyard_return_v61(
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
    let mut value = GraveyardReturnObservationV61 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        common,
        drake: crate::drake_public_v53::capture_complete_v61(state)?,
        spinner: crate::spinner_public_v54::capture_complete_v61(state).map_err(RlContractError)?,
        quandary: crate::quandary_public_v55::capture_complete_v61(state, actor_and_origin.0)
            .map_err(RlContractError)?,
        phoenix: crate::phoenix_public_v56::capture(state)?,
        next_five: crate::next_five_public_v59::capture_complete_v61(state, actor_and_origin.0)
            .map_err(RlContractError)?,
        graveyard_return: crate::graveyard_return_public_v61::capture(state, actor_and_origin.0)
            .map_err(RlContractError)?,
        complete_ordinary_observation: true,
        owning_consumer_admitted: false,
        projection_sha256: String::new(),
    };
    // Compatibility components are not standalone frozen artifacts.
    value.drake.schema = format!("{SCHEMA}/compat/drake-v53");
    value.drake.producer_sha256 = producer_sha256();
    value.spinner.schema = format!("{SCHEMA}/compat/spinner-v54");
    value.quandary.schema = format!("{SCHEMA}/compat/quandary-v55");
    value.next_five.schema = format!("{SCHEMA}/compat/next-five-v59");
    value.projection_sha256 = commitment(&value)?;
    validate_artifact(&value)?;
    Ok((value, families))
}
/// Closed value checks are supplemental. validate_current recaptures from the
/// supplied native world. Neither function grants action or restore authority.
pub fn validate_artifact(value: &GraveyardReturnObservationV61) -> Result<(), RlContractError> {
    let inherited = &value.common.common;
    let meanings = crate::rules_meaning_next_five_v59::FamilyV59::ALL
        .into_iter()
        .map(crate::rules_meaning_next_five_v59::printed_meaning)
        .collect::<Result<Vec<_>, _>>()
        .map_err(RlContractError)?;
    if value.next_five.schema != format!("{SCHEMA}/compat/next-five-v59")
        || value.next_five.producer_sha256 != crate::next_five_public_v59::producer_sha256()
        || value.next_five.viewer != inherited.acting_player
        || value.next_five.meanings != meanings
    {
        return Err(RlContractError(
            "next-five public identity or complete meaning order changed".into(),
        ));
    }
    inherited.validate_graveyard_return_compatibility_v61()?;
    crate::rules_meaning_elenda_v52::validate(&value.common.elenda.meaning)
        .map_err(|e| RlContractError(format!("V61 Elenda meaning:{e:?}")))?;
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
        || value.drake.schema != format!("{SCHEMA}/compat/drake-v53")
        || value.drake.producer_sha256 != producer_sha256()
        || value.common.elenda.schema != crate::elenda_public_v52::SCHEMA
        || value.common.elenda.producer_sha256 != crate::elenda_public_v52::producer_sha256()
        || value.common.elenda.card_db_hash != inherited.card_db_hash
        || value.spinner.schema != format!("{SCHEMA}/compat/spinner-v54")
        || value.spinner.producer_sha256 != crate::rules_meaning_spinner_v54::producer_sha256()
        || value.quandary.schema != format!("{SCHEMA}/compat/quandary-v55")
        || value.quandary.producer_sha256 != crate::rules_meaning_quandary_v55::producer_sha256()
        || value.quandary.viewer != inherited.acting_player
        || value.phoenix.schema != crate::phoenix_public_v56::SCHEMA
        || value.phoenix.producer_sha256 != crate::phoenix_public_v56::producer_sha256()
        || value.phoenix.card_db_hash != inherited.card_db_hash
    {
        return Err(RlContractError(
            "V61 inherited family artifact identity/root order changed".into(),
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
        return Err(RlContractError("V61 artifact identity changed".into()));
    }
    let meanings = crate::rules_meaning_graveyard_return_v61::FamilyV61::ALL
        .into_iter()
        .map(crate::rules_meaning_graveyard_return_v61::printed_meaning)
        .collect::<Result<Vec<_>, _>>()
        .map_err(RlContractError)?;
    if value.graveyard_return.schema != crate::graveyard_return_public_v61::SCHEMA
        || value.graveyard_return.producer_sha256
            != crate::graveyard_return_public_v61::producer_sha256()
        || value.graveyard_return.viewer != inherited.acting_player
        || value.graveyard_return.meanings != meanings
        || !value.complete_ordinary_observation
        || value.owning_consumer_admitted
        || value.graveyard_return.complete_ordinary_observation
        || value.graveyard_return.owning_consumer_admitted
    {
        return Err(RlContractError(
            "V61 closed source identity/permission changed".into(),
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for seat in 0..2 {
        let exact = &value.drake.battlefield[seat];
        let narrow = &inherited.battlefield[seat];
        let base = &inherited.projection.surface.battlefield[seat];
        if exact.len() != narrow.len()
            || exact.len() != base.len()
            || exact.len() != value.graveyard_return.battlefield_counters[seat].len()
        {
            return Err(RlContractError(
                "V61 complete battlefield roster changed".into(),
            ));
        }
        for (((card, compat), base), complete) in exact
            .iter()
            .zip(narrow)
            .zip(base)
            .zip(&value.graveyard_return.battlefield_counters[seat])
        {
            let full = &complete.counters;
            validate_counter_prefix(full, &card.counters)?;
            if complete.card.object != base.stable.arena_id
                || complete.card.generation != base.stable.zone_change_count
                || complete.card.owner != base.stable.owner
                || complete.card.controller != base.stable.controller
                || complete.card.zone != base.stable.zone
                || serde_json::to_value(&complete.card.identity)
                    .map_err(|e| RlContractError(e.to_string()))?
                    != serde_json::to_value(&card.reference.identity)
                        .map_err(|e| RlContractError(e.to_string()))?
            {
                return Err(RlContractError(
                    "V61 mandatory counter incarnation/order changed".into(),
                ));
            }
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
                return Err(RlContractError("V61 public counter underflow".into()));
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
                    "V61 exact card roster/value mismatch".into(),
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
    value: &GraveyardReturnObservationV61,
) -> Result<(), RlContractError> {
    if observe_v61(state, surface, actor_and_origin, step, physical, substep)? != *value {
        return Err(RlContractError(
            "V61 observation differs from current actual decision".into(),
        ));
    }
    Ok(())
}

fn validate_counter_prefix(
    full: &crate::graveyard_return_public_v61::CountersV61,
    prefix: &crate::drake_public_v53::CountersV53,
) -> Result<(), RlContractError> {
    let mut encoded = serde_json::to_value(full).map_err(|e| RlContractError(e.to_string()))?;
    if encoded
        .as_object()
        .ok_or_else(|| RlContractError("V61 counter object absent".into()))?
        .values()
        .any(|n| n.as_i64().is_none_or(|n| n < 0))
    {
        return Err(RlContractError(
            "V61 counter underflow/width changed".into(),
        ));
    }
    encoded
        .as_object_mut()
        .ok_or_else(|| RlContractError("V61 counter object absent".into()))?
        .remove("revival");
    if encoded != serde_json::to_value(prefix).map_err(|e| RlContractError(e.to_string()))? {
        return Err(RlContractError(
            "V61 complete counters lost compatibility prefix meaning".into(),
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
