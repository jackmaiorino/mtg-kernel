//! Complete source-only Genesis Wave V69 projection. Distinct compatibility
//! schemas plus the mandatory seventeen-counter roster preserve every native value.
//! This module grants no session, action execution, model or transport admission.
use crate::{
    ids::PlayerId,
    policy_surface_v5::{PolicyDecisionV5, PolicySurfaceV5},
    rl::{CardStableRefV1, RlContractError},
    state::GameState,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/genesis-wave-observation/v69";
pub const COMMON_SCHEMA: &str = "mtg-kernel/genesis-wave-compat-common/v69";
pub const ELENDA_COMPAT_SCHEMA: &str = "mtg-kernel/genesis-wave-compat-elenda/v69";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenesisWaveObservationV69 {
    pub schema: String,
    pub producer_sha256: String,
    /// Inherited layout only. Both nested schemas are distinct V69 schemas.
    /// Compatibility slots never supersede call.battlefield[*].counters.
    pub common: crate::elenda_observation_v52::ElendaObservationV52,
    pub drake: crate::drake_public_v53::FactsV53,
    pub spinner: crate::spinner_public_v54::FactsV54,
    pub quandary: crate::quandary_public_v55::FactsV55,
    pub phoenix: crate::phoenix_public_v56::FactsV56,
    pub next_five: crate::next_five_public_v59::FactsV59,
    pub graveyard_return: crate::graveyard_return_public_v61::FactsV61,
    pub call: crate::valkyries_call_public_v62::FactsV62,
    pub end_turn: crate::time_stop_public_v68::FactsV68,
    pub genesis_wave: crate::genesis_wave_public_v69::FactsV69,
    pub complete_ordinary_observation: bool,
    pub owning_consumer_admitted: bool,
    pub projection_sha256: String,
}
pub(crate) type GenesisWavePublicPartsV69 = (
    GenesisWaveObservationV69,
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
            include_str!("genesis_wave_observation_v69.rs"),
            include_str!("time_stop_public_v68.rs"),
            include_str!("genesis_wave_public_v69.rs"),
            include_str!("rules_meaning_genesis_wave_v69.rs"),
            include_str!("effect/genesis_wave_v1.rs"),
            include_str!("rules_meaning_time_stop_v68.rs"),
            include_str!("ravenous_observation_v51.rs"),
            include_str!("ravenous_public_v51.rs"),
            include_str!("rl.rs"),
            include_str!("rl_session/genesis_wave_v69.rs"),
            include_str!("akroma_family_public_v40.rs"),
            include_str!("optional_trigger_public_v1.rs"),
            include_str!("drake_public_v53.rs"),
            include_str!("spinner_public_v54.rs"),
            include_str!("quandary_public_v55.rs"),
            include_str!("next_five_public_v59.rs"),
            inherited.as_str(),
            native.as_str(),
            include_str!("valkyries_call_public_v62.rs"),
            include_str!("valkyries_call_v1.rs"),
            include_str!("rules_meaning_valkyries_call_v62.rs"),
            include_str!("effect/thousand_year_storm_v1.rs"),
            include_str!("effect/standard_copy.rs"),
            include_str!("standard_spell_copy.rs"),
            include_str!("event_history_v1.rs"),
            include_str!("cast_identity_v1.rs"),
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
pub fn commitment(value: &GenesisWaveObservationV69) -> Result<String, RlContractError> {
    let mut value = value.clone();
    value.projection_sha256.clear();
    let bytes = serde_json::to_vec(&value).map_err(|e| RlContractError(e.to_string()))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    crate::effect::genesis_wave_v1::validate_state(state)?;
    crate::engine::time_stop_v1::validate(state)?;
    crate::engine::ordinary_cleanup_v1::validate(state)?;
    crate::engine::validate_battlefield_pt(state)
        .map_err(|id| format!("native PT envelope exceeded for {id:?}"))?;
    require_supported(state, None)?;
    crate::ravenous_observation_v51::validate_state_complete_prepared_v57(state)?;
    crate::elenda_public_v52::capture(state).map_err(|e| e.0)?;
    crate::drake_public_v53::capture_complete_call_v62(state).map_err(|e| e.0)?;
    crate::spinner_public_v54::capture_complete_call_v62(state)?;
    crate::phoenix_public_v56::capture(state).map_err(|e| e.0)?;
    for viewer in [PlayerId::P0, PlayerId::P1] {
        crate::quandary_public_v55::capture_complete_call_v62(state, viewer)?;
        crate::next_five_public_v59::capture_complete_call_v62(state, viewer)?;
        crate::graveyard_return_public_v61::capture_complete_call_v62(state, viewer)?;
        crate::valkyries_call_public_v62::capture(state, viewer)?;
    }
    Ok(())
}

/// Reject only authorized identities and public producing history. Unrevealed
/// opponent hand/library contents never select a refusal. IDs >=460 are outside
/// this closed Call scope, including a supported return of such a printing.
/// Later-family receipt variants select only originally public mechanics; their
/// private payloads are never scanned or published.
pub(crate) fn require_supported(state: &GameState, viewer: Option<PlayerId>) -> Result<(), String> {
    use crate::{event::CommittedEvent as E, optional_trigger_public_v1 as public};
    let unsupported = |id: u16| {
        id >= 460
            && !crate::engine::time_stop_v1::definition(id)
            && !crate::effect::genesis_wave_v1::definition(id)
    };
    let historical_unsupported =
        crate::cast_identity_v1::unsupported_public_history(state, unsupported)?;
    let visible = state.objects.iter().any(|(id, live)| {
        public::visible_known(state, id, viewer, |card_id| unsupported(card_id))
            || viewer.is_some_and(|viewer| {
                live.v4
                    .face_down_v1
                    .is_some_and(|face| face.lookers & (1 << viewer.index()) != 0)
                    && unsupported(live.card_def)
            })
    });
    let captured_unsupported = |source: crate::state::AbilitySourceContractV4| {
        unsupported(crate::rl::projected_captured_card_def(
            state,
            source.source,
            source.zone_change_count,
            source.zone,
            source.card_def,
        ))
    };
    let item_unsupported = |item: &crate::state::StackItem| {
        item.v4.source_contract.is_some_and(|s| {
            captured_unsupported(crate::state::AbilitySourceContractV4 {
                source: s.source,
                card_def: s.card_def,
                owner: s.owner,
                controller: s.controller,
                zone: s.zone,
                zone_change_count: s.zone_change_count,
                attached_to: None,
            })
        }) || item
            .v4
            .ability_source_contract
            .is_some_and(captured_unsupported)
    };
    let retained = state.stack.iter().any(item_unsupported)
        || state
            .engine
            .pending_effect
            .as_ref()
            .is_some_and(|p| item_unsupported(&p.resolving_item))
        || public::held(state)
            .iter()
            .any(|p| p.source_contract.is_some_and(captured_unsupported))
        || state.players.iter().any(|p| {
            p.restricted_mana_pool
                .0
                .iter()
                .any(|m| unsupported(m.source_card_def))
        })
        || state.engine.event_history.iter().any(|e| match e {
            E::ControlledCreatureDeathCaptureV1 { source, .. } => unsupported(source.card_def),
            E::NivMizzetVisionaryDamageV1 { observer, .. }
            | E::NivMizzetVisionaryTriggerStackedV1 { observer, .. } => {
                unsupported(observer.card_def)
            }
            E::NivMizzetVisionaryDamageSourceDepartureV1 { source, .. } => {
                unsupported(source.card_def)
            }
            E::KinshipEntryV1 { .. }
            | E::SphinxsTutelageDrawV1 { .. }
            | E::SphinxsTutelageCollectedV1 { .. }
            | E::SphinxsTutelagePublishedV1 { .. }
            | E::SphinxsTutelageMillStartedV1 { .. }
            | E::SphinxsTutelageMillBatchV1 { .. }
            | E::LathrilCombatDamageV1 { .. }
            | E::LathrilCombatCollectedV1 { .. }
            | E::LathrilCombatPublishedV1 { .. }
            | E::LathrilCombatResolvedV1 { .. }
            | E::LathrilCombatCounteredV1 { .. }
            | E::LathrilCombatObservedV1 { .. }
            | E::CuratorEnteredV1 { .. }
            | E::CuratorCollectedV1 { .. }
            | E::CuratorPublishedV1 { .. }
            | E::CuratorFinishedV1 { .. }
            | E::CuratorProgressV1 { .. }
            | E::KykarExiledV1 { .. }
            | E::KykarDelayedCollectedV1 { .. } => true,

            _ => false,
        });
    if visible || retained || historical_unsupported {
        Err("V69 has no unsupported later-family semantics".into())
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
                    | "Valkyrie's Call"
                    | "Time Stop"
                    | "Genesis Wave"
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
    crate::engine::time_stop_v1::validate(state).map_err(RlContractError)?;
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
            "V69 source requires unique genuine live or resolving full row".into(),
        ));
    }
    if item.source != source.source || item.controller != source.controller {
        return Err(RlContractError(
            "V69 original source/controller changed".into(),
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
    crate::engine::time_stop_v1::validate(state).map_err(RlContractError)?;
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
            "V69 source requires unique genuine held trigger".into(),
        ));
    }
    Ok(Some(captured_reference(state, source)))
}

pub fn observe_v69(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<GenesisWaveObservationV69, RlContractError> {
    observe_parts_v69(state, surface, actor_and_origin, step, physical, substep)
        .map(|(value, _)| value)
}
/// Full state validation for one immutable borrow, never a reusable authority
/// based on a hash, DTO or revision number.
pub(crate) struct ValidatedStateV69<'a> {
    state: &'a GameState,
}
pub(crate) fn validate_state_for_observation_v69(
    state: &GameState,
) -> Result<ValidatedStateV69<'_>, String> {
    validate_state(state)?;
    Ok(ValidatedStateV69 { state })
}
pub(crate) fn observe_parts_v69(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<GenesisWavePublicPartsV69, RlContractError> {
    let validated = validate_state_for_observation_v69(state).map_err(RlContractError)?;
    observe_parts_validated_v69(
        validated,
        surface,
        actor_and_origin,
        step,
        physical,
        substep,
    )
}
pub(crate) fn observe_parts_validated_v69(
    validated: ValidatedStateV69<'_>,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<GenesisWavePublicPartsV69, RlContractError> {
    let state = validated.state;
    require_supported(state, Some(actor_and_origin.0)).map_err(RlContractError)?;
    let (inherited, families) =
        crate::ravenous_observation_v51::observe_parts_complete_genesis_wave_v69(
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
    let mut value = GenesisWaveObservationV69 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        common,
        drake: crate::drake_public_v53::capture_complete_call_v62(state)?,
        spinner: crate::spinner_public_v54::capture_complete_call_v62(state)
            .map_err(RlContractError)?,
        quandary: crate::quandary_public_v55::capture_complete_call_v62(state, actor_and_origin.0)
            .map_err(RlContractError)?,
        phoenix: crate::phoenix_public_v56::capture(state)?,
        next_five: crate::next_five_public_v59::capture_complete_call_v62(
            state,
            actor_and_origin.0,
        )
        .map_err(RlContractError)?,
        graveyard_return: crate::graveyard_return_public_v61::capture_complete_call_v62(
            state,
            actor_and_origin.0,
        )
        .map_err(RlContractError)?,
        call: crate::valkyries_call_public_v62::capture(state, actor_and_origin.0)
            .map_err(RlContractError)?,
        end_turn: crate::time_stop_public_v68::capture_genesis_wave_v69(state, actor_and_origin.0)
            .map_err(RlContractError)?,
        genesis_wave: crate::genesis_wave_public_v69::capture(state, actor_and_origin.0)
            .map_err(RlContractError)?,
        complete_ordinary_observation: true,
        owning_consumer_admitted: false,
        projection_sha256: String::new(),
    };
    // Compatibility components are not standalone frozen artifacts.
    value.graveyard_return.schema = format!("{SCHEMA}/compat/graveyard-return-v61");
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
/// supplied native world, including the complete compatibility-terminal roster.
/// Standalone checks do not prove that an untrusted sender supplied every
/// historical row or terminal. Neither function grants action or restore authority.
pub fn validate_artifact(value: &GenesisWaveObservationV69) -> Result<(), RlContractError> {
    crate::genesis_wave_public_v69::validate(&value.genesis_wave).map_err(RlContractError)?;
    crate::time_stop_public_v68::validate(&value.end_turn).map_err(RlContractError)?;
    let inherited = &value.common.common;
    validate_compatibility_terminals_v68(value)?;
    if value.end_turn.viewer != inherited.acting_player
        || value.genesis_wave.viewer != inherited.acting_player
    {
        return Err(RlContractError(
            "V69 Time Stop viewer differs from current actor".into(),
        ));
    }
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
    inherited.validate_genesis_wave_compatibility_v69()?;
    crate::rules_meaning_elenda_v52::validate(&value.common.elenda.meaning)
        .map_err(|e| RlContractError(format!("V69 Elenda meaning:{e:?}")))?;
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
            "V69 inherited family artifact identity/root order changed".into(),
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
        return Err(RlContractError("V69 artifact identity changed".into()));
    }
    let meanings = crate::rules_meaning_graveyard_return_v61::FamilyV61::ALL
        .into_iter()
        .map(crate::rules_meaning_graveyard_return_v61::printed_meaning)
        .collect::<Result<Vec<_>, _>>()
        .map_err(RlContractError)?;
    if value.graveyard_return.schema != format!("{SCHEMA}/compat/graveyard-return-v61")
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
            "V69 closed source identity/permission changed".into(),
        ));
    }
    crate::rules_meaning_valkyries_call_v62::validate(&value.call.meaning)
        .map_err(RlContractError)?;
    if value.call.schema != crate::valkyries_call_public_v62::SCHEMA
        || value.call.producer_sha256 != crate::valkyries_call_public_v62::producer_sha256()
        || value.call.viewer != inherited.acting_player
        || value.call.complete_ordinary_observation
        || value.call.owning_consumer_admitted
    {
        return Err(RlContractError(
            "Call V62 complete source identity changed".into(),
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
            || exact.len() != value.call.battlefield[seat].len()
        {
            return Err(RlContractError(
                "V69 complete battlefield roster changed".into(),
            ));
        }
        for (ordinal, (((card, compat), base), complete)) in exact
            .iter()
            .zip(narrow)
            .zip(base)
            .zip(&value.graveyard_return.battlefield_counters[seat])
            .enumerate()
        {
            let call = &value.call.battlefield[seat][ordinal];
            if call.card != complete.card {
                return Err(RlContractError(
                    "Call V62 full counter roster identity changed".into(),
                ));
            }
            let mut prefix =
                serde_json::to_value(&call.counters).map_err(|e| RlContractError(e.to_string()))?;
            if prefix
                .as_object()
                .unwrap()
                .values()
                .any(|n| n.as_i64().is_none_or(|v| v < 0))
            {
                return Err(RlContractError("Call V62 negative full counter".into()));
            }
            prefix.as_object_mut().unwrap().remove("fellowship");
            if prefix
                != serde_json::to_value(&complete.counters)
                    .map_err(|e| RlContractError(e.to_string()))?
            {
                return Err(RlContractError(
                    "Call V62 full counter prefix changed".into(),
                ));
            }
            let full = &complete.counters;
            validate_counter_prefix(full, &card.counters)?;
            if complete.card.object != base.stable.arena_id
                || complete.card.generation != base.stable.zone_change_count
                || complete.card.owner != base.stable.owner
                || complete.card.controller != base.stable.controller
                || complete.card.zone != base.stable.zone
                || serde_json::to_value(&complete.card.identity)
                    .map_err(|e| RlContractError(e.to_string()))?
                    != serde_json::to_value(card.reference.identity)
                        .map_err(|e| RlContractError(e.to_string()))?
            {
                return Err(RlContractError(
                    "V69 mandatory counter incarnation/order changed".into(),
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
                return Err(RlContractError("V69 public counter underflow".into()));
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
                    "V69 exact card roster/value mismatch".into(),
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
    value: &GenesisWaveObservationV69,
) -> Result<(), RlContractError> {
    if observe_v69(state, surface, actor_and_origin, step, physical, substep)? != *value {
        return Err(RlContractError(
            "V69 observation differs from current actual decision".into(),
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
        .ok_or_else(|| RlContractError("V69 counter object absent".into()))?
        .values()
        .any(|n| n.as_i64().is_none_or(|n| n < 0))
    {
        return Err(RlContractError(
            "V69 counter underflow/width changed".into(),
        ));
    }
    encoded
        .as_object_mut()
        .ok_or_else(|| RlContractError("V69 counter object absent".into()))?
        .remove("revival");
    if encoded != serde_json::to_value(prefix).map_err(|e| RlContractError(e.to_string()))? {
        return Err(RlContractError(
            "V69 complete counters lost compatibility prefix meaning".into(),
        ));
    }
    Ok(())
}

/// A compatibility appendix's historical publication label cannot override its
/// exact V68 terminal result. No old enum or serialized row is reinterpreted
/// as an executable current stack membership.
fn validate_compatibility_terminals_v68(
    v: &GenesisWaveObservationV69,
) -> Result<(), RlContractError> {
    use crate::time_stop_public_v68::CompatibilityTargetV68 as T;
    let old = &v.common.common;
    for terminal in &v.end_turn.compatibility_terminals {
        let original = &v.end_turn.disposals[terminal.disposal_occurrence as usize].disposed_rows
            [terminal.original_stack_position as usize]
            .original;
        let source = match terminal.target {
            T::DrakeCombat { occurrence } => {
                let row = v
                    .drake
                    .combat
                    .get(occurrence as usize)
                    .ok_or_else(|| RlContractError("V69 Drake combat role absent".into()))?;
                if row.outcome != crate::drake_public_v53::CombatOutcomeV53::Published {
                    return Err(RlContractError(
                        "V69 disposed combat acquired another terminal outcome".into(),
                    ));
                }
                drake_compatibility_reference_v68(&row.original_source)?
            }
            T::DrakeActivation { occurrence } => {
                let row = v
                    .drake
                    .activations
                    .get(occurrence as usize)
                    .ok_or_else(|| RlContractError("V69 Drake activation role absent".into()))?;
                if row.outcome != crate::drake_public_v53::ActivationOutcomeV53::Published {
                    return Err(RlContractError(
                        "V69 disposed activation acquired another terminal outcome".into(),
                    ));
                }
                drake_compatibility_reference_v68(&row.original_source)?
            }
            T::RavenousActivation { occurrence } => {
                let row = old
                    .ravenous
                    .activations
                    .get(occurrence as usize)
                    .ok_or_else(|| RlContractError("V69 Amulet role absent".into()))?;
                if row.outcome != crate::ravenous_public_v51::OutcomeV51::Published {
                    return Err(RlContractError(
                        "V69 disposed Amulet acquired another terminal outcome".into(),
                    ));
                }
                row.source.clone()
            }
            T::AjaniActivation { position } => {
                let row = old
                    .ajani
                    .activations
                    .get(position as usize)
                    .ok_or_else(|| RlContractError("V69 Ajani role absent".into()))?;
                if row.stage != crate::ajani_public_v42::StageV42::OnStack {
                    return Err(RlContractError(
                        "V69 disposed Ajani acquired another terminal outcome".into(),
                    ));
                }
                row.source_at_payment.clone()
            }
            T::FishingOperation { position } => {
                let mut families = old.families.iter().filter_map(|f| {
                    if let crate::akroma_family_public_v40::FamilyFactsV40::Fishing(f) = f {
                        Some(f)
                    } else {
                        None
                    }
                });
                let family = families
                    .next()
                    .ok_or_else(|| RlContractError("V69 Fishing family absent".into()))?;
                if families.next().is_some() {
                    return Err(RlContractError("V69 duplicate Fishing family".into()));
                }
                let row = family
                    .operations
                    .get(position as usize)
                    .ok_or_else(|| RlContractError("V69 Fishing role absent".into()))?;
                if row.stage != crate::fishing_pole_public_v39::StageV39::OnStack
                    || row.stack_ordinal.is_some()
                {
                    return Err(RlContractError(
                        "V69 disposed Fishing acquired another live/terminal outcome".into(),
                    ));
                }
                row.original_pole.clone()
            }
            T::FaebloomSpell { ordinal } => {
                let row = old
                    .faebloom
                    .spells
                    .get(ordinal as usize)
                    .ok_or_else(|| RlContractError("V69 Faebloom spell role absent".into()))?;
                if row.stage != crate::faebloom_public_v43::StageV43::OnStack
                    || row.stack_ordinal.is_some()
                {
                    return Err(RlContractError(
                        "V69 disposed Faebloom acquired another live/terminal outcome".into(),
                    ));
                }
                row.original_source.clone()
            }
            T::FaebloomReflexive { ordinal } => {
                let row = old
                    .faebloom
                    .reflexives
                    .get(ordinal as usize)
                    .ok_or_else(|| RlContractError("V69 Faebloom reflexive role absent".into()))?;
                if row.stage != crate::faebloom_public_v43::StageV43::OnStack
                    || row.stack_ordinal.is_some()
                {
                    return Err(RlContractError(
                        "V69 disposed reflexive acquired another live/terminal outcome".into(),
                    ));
                }
                row.original_source.clone()
            }
        };
        if source != original.source || source.controller != original.controller {
            return Err(RlContractError("V69 compatibility terminal lost exact original public source/controller/incarnation".into()));
        }
    }
    Ok(())
}
fn drake_compatibility_reference_v68(
    v: &crate::drake_public_v53::ReferenceV53,
) -> Result<CardStableRefV1, RlContractError> {
    let card_db_id = match v.identity {
        crate::drake_public_v53::IdentityV53::Known { card_id } => card_id,
        crate::drake_public_v53::IdentityV53::FaceDown { disguised } => {
            crate::card_def::card_id_by_name(if disguised {
                "Face-down creature"
            } else {
                "Face-down card"
            })
            .ok_or_else(|| RlContractError("V69 neutral source sentinel absent".into()))?
        }
    };
    Ok(CardStableRefV1 {
        arena_id: v.incarnation.object,
        card_db_id,
        owner: v.owner,
        controller: v.controller,
        zone: v.zone,
        zone_change_count: v.incarnation.generation,
    })
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) mod tests;
