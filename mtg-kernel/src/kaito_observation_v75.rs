//! Complete Kaito V75 projection. Distinct compatibility schemas retain
//! every inherited prefix; the mandatory inherited and Kaito rosters carries all eighteen counters.
//! This module grants no session, action execution, model or transport admission.
use crate::{
    ids::PlayerId,
    policy_surface_v5::{LimitedAppendixV1, PolicyDecisionV5, PolicySurfaceV5},
    rl::{CardStableRefV1, RlContractError},
    state::GameState,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/kaito-observation/v75";
pub const COMMON_SCHEMA: &str = "mtg-kernel/kaito-compat-common/v75";
pub const ELENDA_COMPAT_SCHEMA: &str = "mtg-kernel/kaito-compat-elenda/v75";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KaitoObservationV75 {
    pub schema: String,
    pub producer_sha256: String,
    /// Inherited layout only. Both nested schemas are distinct V75 schemas.
    /// Compatibility slots never supersede tinybones.battlefield[*].counters.
    #[serde(deserialize_with = "strict_value")]
    pub common: crate::elenda_observation_v52::ElendaObservationV52,
    #[serde(deserialize_with = "strict_value")]
    pub drake: crate::drake_public_v53::FactsV53,
    #[serde(deserialize_with = "strict_value")]
    pub spinner: crate::spinner_public_v54::FactsV54,
    #[serde(deserialize_with = "strict_value")]
    pub quandary: crate::quandary_public_v55::FactsV55,
    #[serde(deserialize_with = "strict_value")]
    pub phoenix: crate::phoenix_public_v56::FactsV56,
    #[serde(deserialize_with = "strict_value")]
    pub next_five: crate::next_five_public_v59::FactsV59,
    #[serde(deserialize_with = "strict_value")]
    pub graveyard_return: crate::graveyard_return_public_v61::FactsV61,
    #[serde(deserialize_with = "strict_value")]
    pub call: crate::valkyries_call_public_v62::FactsV62,
    #[serde(deserialize_with = "strict_value")]
    pub end_turn: crate::time_stop_public_v68::FactsV68,
    #[serde(deserialize_with = "strict_value")]
    pub genesis_wave: crate::genesis_wave_public_v69::FactsV69,
    #[serde(deserialize_with = "strict_value")]
    pub vivien: crate::vivien_public_v70::FactsV70,
    #[serde(deserialize_with = "strict_value")]
    pub muldrotha: crate::muldrotha_public_v71::FactsV71,
    #[serde(deserialize_with = "strict_value")]
    pub tinybones: crate::tinybones_public_v72::FactsV72,
    #[serde(deserialize_with = "strict_value")]
    pub liliana: crate::liliana_public_v73::FactsV73,
    #[serde(deserialize_with = "strict_value")]
    pub chandra: crate::chandra_public_v74::FactsV74,
    #[serde(deserialize_with = "strict_value")]
    pub kaito: crate::kaito_public_v75::FactsV75,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "strict_value"
    )]
    pub artist: Option<crate::artist_public_v64::FactsV64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "strict_value"
    )]
    pub chosen_type: Option<crate::chosen_type_consumer_v1::FactsV1>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "strict_value"
    )]
    pub niv: Option<crate::niv_mizzet_public_v63::ConsumerFactsV1>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "strict_value"
    )]
    pub drakuseth: Option<crate::drakuseth_consumer_v1::FactsV1>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "strict_value"
    )]
    pub embercleave: Option<crate::embercleave_public_v65::FactsV65>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zimone: Option<crate::zimone_public_v1::FactsV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub omniscience: Option<crate::omniscience_public_v1::FactsV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tutelage: Option<crate::tutelage_public_v1::FactsV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moon: Option<crate::moon_public_v1::FactsV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leyline_axe: Option<crate::leyline_axe_public_v1::FactsV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progenitus: Option<crate::progenitus_public_v1::FactsV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aberration: Option<crate::aberration_public_v1::FactsV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lathril: Option<crate::lathril_public_v1::FactsV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub electroduplicate: Option<crate::electroduplicate_public_v1::FactsV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edict: Option<crate::edict_public_v1::FactsV1>,
    pub complete_ordinary_observation: bool,
    pub owning_consumer_admitted: bool,
    pub projection_sha256: String,
}
pub(crate) type KaitoPublicPartsV75 = (
    KaitoObservationV75,
    Vec<crate::akroma_family_public_v40::FamilyProjectionV40>,
);
fn strict_value<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned + Serialize,
{
    let raw = serde_json::Value::deserialize(deserializer)?;
    let value: T = serde_json::from_value(raw.clone()).map_err(serde::de::Error::custom)?;
    if serde_json::to_value(&value).map_err(serde::de::Error::custom)? != raw {
        return Err(serde::de::Error::custom(
            "V75 noncanonical or unknown nested fields",
        ));
    }
    Ok(value)
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let inherited = crate::next_five_observation_v59::producer_sha256();
        let native = crate::graveyard_return_public_v61::producer_sha256();
        let mut hash = Sha256::new();
        for part in [
            SCHEMA,
            include_str!("chandra_observation_v74.rs"),
            include_str!("chandra_candidates_v74.rs"),
            include_str!("kaito_public_v75.rs"),
            include_str!("effect/kaito_cunning_v75.rs"),
            include_str!("rules_meaning_kaito_v75.rs"),
            include_str!("liliana_observation_v73.rs"),
            include_str!("liliana_candidates_v73.rs"),
            include_str!("chandra_public_v74.rs"),
            include_str!("effect/chandra_flameshaper_public_v74.rs"),
            include_str!("effect/chandra_flameshaper_v1.rs"),
            include_str!("rules_meaning_chandra_v74.rs"),
            COMMON_SCHEMA,
            ELENDA_COMPAT_SCHEMA,
            include_str!("tinybones_observation_v72.rs"),
            include_str!("kaito_observation_v75.rs"),
            include_str!("limited_mixed_consumer_v1.rs"),
            include_str!("leyline_axe_public_v1.rs"),
            include_str!("leyline_axe_v1.rs"),
            include_str!("lathril_public_v1.rs"),
            include_str!("lathril_receipts_v1.rs"),
            include_str!("edict_public_v1.rs"),
            include_str!("edict_receipts_v1.rs"),
            include_str!("drakuseth_consumer_v1.rs"),
            include_str!("drakuseth_consumer_v1/damage.rs"),
            include_str!("human_bo3_v1/drakuseth_consumer_v1.rs"),
            include_str!("fdn_family_source_v76.rs"),
            include_str!("drakuseth_damage_source_v76.rs"),
            include_str!("kaito_candidates_v75.rs"),
            include_str!("rl_session/kaito_v75.rs"),
            include_str!("current_mixed_policy_v1/kaito.rs"),
            include_str!("liliana_public_v73.rs"),
            include_str!("effect/liliana_dreadhorde_public_v73.rs"),
            include_str!("effect/liliana_dreadhorde_v1.rs"),
            include_str!("rules_meaning_liliana_v73.rs"),
            include_str!("tinybones_candidates_v72.rs"),
            include_str!("time_stop_public_v68.rs"),
            include_str!("genesis_wave_public_v69.rs"),
            include_str!("rules_meaning_genesis_wave_v69.rs"),
            include_str!("effect/genesis_wave_v1.rs"),
            include_str!("rules_meaning_time_stop_v68.rs"),
            include_str!("ravenous_observation_v51.rs"),
            include_str!("ravenous_public_v51.rs"),
            include_str!("rl.rs"),
            include_str!("rl_session/muldrotha_v71.rs"),
            include_str!("akroma_family_public_v40.rs"),
            include_str!("crystal_public_v1.rs"),
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
            include_str!("vivien_public_v70.rs"),
            include_str!("rules_meaning_vivien_v70.rs"),
            include_str!("effect/vivien_reid_v1.rs"),
            include_str!("cast_identity_v1.rs"),
            include_str!("muldrotha_candidates_v71.rs"),
            include_str!("current_mixed_policy_v1/muldrotha.rs"),
            include_str!("human_bo3_v1/muldrotha_v71.rs"),
            include_str!("human_bo3_v1/limited_muldrotha_v12.rs"),
            include_str!("limited_muldrotha_session_v12.rs"),
            include_str!("bin/kernel_limited_muldrotha_env_v12.rs"),
            include_str!("effect/squad_rallier_resolution_v1.rs"),
            include_str!("muldrotha_public_v71.rs"),
            include_str!("rules_meaning_muldrotha_v71.rs"),
            include_str!("engine/muldrotha_v1.rs"),
            include_str!("engine/tinybones_v72.rs"),
            include_str!("tinybones_public_v72.rs"),
            include_str!("rl_session/tinybones_v72.rs"),
            include_str!("rules_meaning_tinybones_v72.rs"),
            include_str!("human_bo3_v1/tinybones_v72.rs"),
            include_str!("human_bo3_v1/limited_tinybones_v13.rs"),
            include_str!("limited_tinybones_session_v13.rs"),
            include_str!("bin/kernel_limited_tinybones_env_v13.rs"),
            include_str!("../../python/tools/limited_tinybones_session_v13.py"),
            include_str!("human_bo3_v1/liliana_v73.rs"),
            include_str!("human_bo3_v1/limited_liliana_v14.rs"),
            include_str!("limited_liliana_session_v14.rs"),
            include_str!("bin/kernel_limited_liliana_env_v14.rs"),
            include_str!("../../python/tools/limited_liliana_session_v14.py"),
            include_str!("human_bo3_v1/chandra_v74.rs"),
            include_str!("human_bo3_v1/limited_chandra_v15.rs"),
            include_str!("limited_chandra_session_v15.rs"),
            include_str!("bin/kernel_limited_chandra_env_v15.rs"),
            include_str!("../../python/tools/limited_chandra_session_v15.py"),
            include_str!("human_bo3_v1/kaito_v75.rs"),
            include_str!("human_bo3_v1/limited_kaito_v16.rs"),
            include_str!("limited_kaito_session_v16.rs"),
            include_str!("bin/kernel_limited_kaito_env_v16.rs"),
            include_str!("../../python/tools/limited_kaito_session_v16.py"),
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
pub fn commitment(value: &KaitoObservationV75) -> Result<String, RlContractError> {
    let mut value = value.clone();
    value.projection_sha256.clear();
    let bytes = serde_json::to_vec(&value).map_err(|e| RlContractError(e.to_string()))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    validate_state_scoped(state, LimitedAppendixV1::Legacy)
}
pub(crate) fn validate_state_scoped(
    state: &GameState,
    appendix: LimitedAppendixV1,
) -> Result<(), String> {
    crate::effect::kaito_cunning_v75::validate_state(state)?;
    crate::effect::kaito_cunning_v75::validate_pending(state)?;
    crate::effect::chandra_flameshaper_v1::validate_state(state)?;
    crate::effect::chandra_flameshaper_v1::validate_pending(state)?;
    crate::effect::liliana_dreadhorde_v1::validate_state(state)?;
    crate::engine::tinybones_v72::validate_state(state)?;
    crate::engine::muldrotha_v1::validate_state(state)?;
    crate::effect::vivien_reid_v1::validate_state(state)?;
    crate::effect::vivien_reid_v1::validate_pending(state)?;
    crate::effect::genesis_wave_v1::validate_state(state)?;
    crate::engine::time_stop_v1::validate(state)?;
    crate::engine::ordinary_cleanup_v1::validate(state)?;
    crate::engine::validate_battlefield_pt(state)
        .map_err(|id| format!("native PT envelope exceeded for {id:?}"))?;
    require_supported(state, None, appendix)?;
    if appendix.includes(LimitedAppendixV1::ChosenType) {
        crate::banner_of_kinship_v1::entry::validate_state(state)?;
    }
    if appendix.includes(LimitedAppendixV1::Drakuseth) {
        crate::drakuseth_damage_source_v76::validate_state(state)?;
    }
    if appendix.includes(LimitedAppendixV1::Tutelage) {
        crate::tutelage_public_v1::require_receipts(state)?;
    }
    if appendix == LimitedAppendixV1::Moon {
        crate::moon_public_v1::validate_state(state)?;
    }
    if appendix == LimitedAppendixV1::LeylineAxe {
        crate::leyline_axe_v1::validate(state)?;
    }
    if appendix == LimitedAppendixV1::Lathril {
        crate::lathril_public_v1::validate_native_receipts(state)?;
    }
    if appendix == LimitedAppendixV1::Edict {
        crate::edict_public_v1::validate_native(state)?;
    }
    crate::ravenous_observation_v51::validate_state_complete_recipient_scope(
        state,
        crate::planeswalker_v1::RecipientScopeV1::KaitoV75,
    )?;
    crate::elenda_public_v52::capture(state).map_err(|e| e.0)?;
    crate::drake_public_v53::capture_kaito_compat_v75(state, appendix).map_err(|e| e.0)?;
    crate::spinner_public_v54::capture_kaito_compat_v75(state, appendix)?;
    crate::phoenix_public_v56::capture(state).map_err(|e| e.0)?;
    for viewer in [PlayerId::P0, PlayerId::P1] {
        crate::quandary_public_v55::capture_kaito_compat_v75(state, viewer, appendix)?;
        crate::next_five_public_v59::capture_kaito_compat_v75(state, viewer, appendix)?;
        crate::graveyard_return_public_v61::capture_kaito_compat_v75(state, viewer, appendix)?;
        crate::valkyries_call_public_v62::capture_kaito_compat_v75(state, viewer, appendix)?;
    }
    Ok(())
}

/// Reject only authorized identities and public producing history. Unrevealed
/// opponent hand/library contents never select a refusal. IDs >=460 are outside
/// this closed Call scope, including a supported return of such a printing.
/// Later-family receipt variants select only originally public mechanics; their
/// private payloads are never scanned or published.
fn require_supported(
    state: &GameState,
    viewer: Option<PlayerId>,
    appendix: LimitedAppendixV1,
) -> Result<(), String> {
    use crate::{event::CommittedEvent as E, optional_trigger_public_v1 as public};
    let selected = match appendix {
        LimitedAppendixV1::Legacy | LimitedAppendixV1::ChosenType | LimitedAppendixV1::Mixed => {
            None
        }
        LimitedAppendixV1::Artist => Some(crate::rules_meaning_artist_v64::selected_id()?),
        LimitedAppendixV1::Niv => Some(crate::rules_meaning_niv_mizzet_v63::selected_id()?),
        LimitedAppendixV1::Drakuseth => Some(crate::rules_meaning_drakuseth_v67::selected_id()?),
        LimitedAppendixV1::Embercleave => {
            Some(crate::rules_meaning_embercleave_v65::selected_id()?)
        }
        LimitedAppendixV1::Zimone => Some(crate::zimone_public_v1::selected_id()?),
        LimitedAppendixV1::Omniscience => Some(crate::omniscience_public_v1::selected_id()?),
        LimitedAppendixV1::Tutelage => Some(crate::tutelage_public_v1::selected_id()?),
        LimitedAppendixV1::Moon => Some(crate::moon_public_v1::selected_id()?),
        LimitedAppendixV1::LeylineAxe => Some(crate::leyline_axe_public_v1::selected_id()?),
        LimitedAppendixV1::Progenitus => Some(crate::progenitus_public_v1::selected_id()?),
        LimitedAppendixV1::Aberration => Some(crate::aberration_public_v1::selected_id()?),
        LimitedAppendixV1::Lathril => Some(crate::lathril_public_v1::selected_id()?),
        LimitedAppendixV1::Electroduplicate => Some(crate::electroduplicate_public_v1::selected_id()?),
        LimitedAppendixV1::Edict => Some(crate::edict_public_v1::selected_id()?),
    };
    let unsupported = |id: u16| {
        id >= 460
            && !(appendix == LimitedAppendixV1::Mixed
                && crate::limited_mixed_consumer_v1::PARTIAL_PRINTINGS_V1.contains(&id))
            && selected != Some(id)
            && !(appendix.includes(LimitedAppendixV1::ChosenType)
                && crate::banner_of_kinship_v1::entry::owns_entry_definition(id))
            && !crate::engine::time_stop_v1::definition(id)
            && !crate::effect::genesis_wave_v1::definition(id)
            && !crate::effect::vivien_reid_v1::definition(id)
            && !crate::engine::muldrotha_v1::definition(id)
            && !crate::engine::tinybones_v72::definition(id)
            && !crate::effect::liliana_dreadhorde_v1::definition(id)
            && !crate::effect::liliana_dreadhorde_v1::token_definition(id)
            && !crate::effect::chandra_flameshaper_v1::definition(id)
            && !crate::effect::kaito_cunning_v75::definition(id)
            && !crate::effect::kaito_cunning_v75::token_definition(id)
    };
    let historical_unsupported =
        crate::cast_identity_v1::unsupported_public_history(state, unsupported)?;
    let visible = state.objects.iter().any(|(id, live)| {
        public::visible_known(state, id, viewer, unsupported)
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
            E::NivMizzetVisionaryDrawEndedV1 { .. } => !appendix.includes(LimitedAppendixV1::Niv),
            E::KinshipEntryV1 { .. } => !appendix.includes(LimitedAppendixV1::ChosenType),
            E::SphinxsTutelageDrawV1 { .. }
            | E::SphinxsTutelageCollectedV1 { .. }
            | E::SphinxsTutelagePublishedV1 { .. }
            | E::SphinxsTutelageMillStartedV1 { .. }
            | E::SphinxsTutelageMillBatchV1 { .. } => {
                !appendix.includes(LimitedAppendixV1::Tutelage)
            }
            E::LathrilCombatDamageV1 { .. }
            | E::LathrilCombatCollectedV1 { .. }
            | E::LathrilCombatPublishedV1 { .. }
            | E::LathrilCombatResolvedV1 { .. }
            | E::LathrilCombatCounteredV1 { .. }
            | E::LathrilCombatObservedV1 { .. }
            | E::LathrilReceiptV1 { .. } => appendix != LimitedAppendixV1::Lathril,
            E::BlasphemousEdictReceiptV1 { .. } => appendix != LimitedAppendixV1::Edict,
            E::CuratorEnteredV1 { .. }
            | E::CuratorCollectedV1 { .. }
            | E::CuratorPublishedV1 { .. }
            | E::CuratorFinishedV1 { .. }
            | E::CuratorProgressV1 { .. }
            | E::KykarExiledV1 { .. }
            | E::KykarDelayedCollectedV1 { .. } => true,

            _ => false,
        });
    if visible || retained || historical_unsupported {
        Err("V75 has no unsupported later-family semantics".into())
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
                    | "Vivien Reid"
                    | "Muldrotha, the Gravetide"
                    | "Tinybones, Bauble Burglar"
                    | "Liliana, Dreadhorde General"
                    | "Chandra, Flameshaper"
                    | "Kaito, Cunning Infiltrator"
            )
        })
}
fn captured_reference(
    state: &GameState,
    source: crate::state::AbilitySourceContractV4,
) -> CardStableRefV1 {
    CardStableRefV1 {
        arena_id: source.source.0,
        card_db_id: if crate::effect::liliana_dreadhorde_v1::definition(source.card_def)
            || crate::effect::chandra_flameshaper_v1::definition(source.card_def)
            || crate::effect::kaito_cunning_v75::definition(source.card_def)
            || source.card_def == crate::drakuseth_consumer_v1::PRINTING_V1
        {
            source.card_def
        } else {
            crate::rl::projected_captured_card_def(
                state,
                source.source,
                source.zone_change_count,
                source.zone,
                source.card_def,
            )
        },
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
    let artist_trigger = item.kind == crate::state::StackItemKind::TriggeredAbility
        && source.card_def == crate::artist_consumer_v1::PRINTING_V1
        && item.inline_effect.as_ref() == Some(&crate::effect::perforating_artist_v1::raid());
    let niv_trigger = item.kind == crate::state::StackItemKind::TriggeredAbility
        && source.card_def == crate::niv_consumer_v1::PRINTING_V1
        && matches!(
            item.inline_effect,
            Some(crate::effect::EffectOp::NivMizzetVisionaryDrawV1 { .. })
        );
    let drakuseth_trigger = source.card_def == crate::drakuseth_consumer_v1::PRINTING_V1
        && item.kind == crate::state::StackItemKind::TriggeredAbility
        && item.inline_effect.as_ref() == Some(&crate::standard_cards_v1::drakuseth_attack_v1());
    let embercleave_attachment = source.card_def == crate::embercleave_consumer_v1::PRINTING_V1
        && item.v4.granted_by.is_none()
        && matches!(
            item.kind,
            crate::state::StackItemKind::ActivatedAbility
                | crate::state::StackItemKind::TriggeredAbility
        )
        && item
            .inline_effect
            .as_ref()
            .is_some_and(crate::embercleave_consumer_v1::is_attachment_program);
    let zimone_counters = source.card_def == crate::zimone_public_v1::PRINTING
        && item.v4.granted_by.is_none()
        && match item.kind {
            crate::state::StackItemKind::TriggeredAbility => {
                item.inline_effect.as_ref()
                    == Some(&crate::zimone_public_v1::AbilityKindV1::CombatPutPlusOne.program())
            }
            crate::state::StackItemKind::ActivatedAbility => {
                item.v4.activated_ability_index == Some(0)
                    && item.inline_effect.as_ref()
                        == Some(
                            &crate::zimone_public_v1::AbilityKindV1::ActivateDoubleEveryKind
                                .program(),
                        )
            }
            _ => false,
        };
    let tutelage = source.card_def == crate::tutelage_public_v1::PRINTING
        && match item.kind {
            crate::state::StackItemKind::TriggeredAbility => matches!(
                item.inline_effect,
                Some(crate::effect::EffectOp::SphinxsTutelageMillV1 { .. })
            ),
            crate::state::StackItemKind::ActivatedAbility => {
                item.v4.activated_ability_index == Some(0)
                    && item
                        .inline_effect
                        .as_ref()
                        .is_some_and(crate::tutelage_public_v1::activation_program)
            }
            _ => false,
        };
    if !selected_source(source)
        && !artist_trigger
        && !drakuseth_trigger
        && !niv_trigger
        && !embercleave_attachment
        && !zimone_counters
        && !tutelage
    {
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
            "V75 source requires unique genuine live or resolving full row".into(),
        ));
    }
    if item.source != source.source || item.controller != source.controller {
        return Err(RlContractError(
            "V75 original source/controller changed".into(),
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
    let artist_trigger = source.card_def == crate::artist_consumer_v1::PRINTING_V1
        && pending.effect == crate::effect::perforating_artist_v1::raid();
    let niv_trigger = source.card_def == crate::niv_consumer_v1::PRINTING_V1
        && matches!(
            pending.effect,
            crate::effect::EffectOp::NivMizzetVisionaryDrawV1 { .. }
        );
    let drakuseth_trigger = source.card_def == crate::drakuseth_consumer_v1::PRINTING_V1
        && pending.granted_by.is_none()
        && pending.effect == crate::standard_cards_v1::drakuseth_attack_v1();
    let embercleave_attachment = source.card_def == crate::embercleave_consumer_v1::PRINTING_V1
        && pending.granted_by.is_none()
        && crate::embercleave_consumer_v1::is_attachment_program(&pending.effect);
    let zimone_trigger = source.card_def == crate::zimone_public_v1::PRINTING
        && pending.granted_by.is_none()
        && pending.effect == crate::zimone_public_v1::AbilityKindV1::CombatPutPlusOne.program();
    let tutelage = source.card_def == crate::tutelage_public_v1::PRINTING
        && matches!(
            pending.effect,
            crate::effect::EffectOp::SphinxsTutelageMillV1 { .. }
        );
    if !selected_source(source)
        && !artist_trigger
        && !drakuseth_trigger
        && !niv_trigger
        && !embercleave_attachment
        && !zimone_trigger
        && !tutelage
    {
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
            "V75 source requires unique genuine held trigger".into(),
        ));
    }
    Ok(Some(captured_reference(state, source)))
}

pub fn observe_v75(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<KaitoObservationV75, RlContractError> {
    if surface.limited_appendix_v1() != crate::policy_surface_v5::LimitedAppendixV1::Legacy {
        return Err(RlContractError(
            "Appendix decisions require their owning profile".into(),
        ));
    }
    observe_parts_v75(state, surface, actor_and_origin, step, physical, substep)
        .map(|(value, _)| value)
}
pub(crate) fn observe_parts_v75(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<KaitoPublicPartsV75, RlContractError> {
    let appendix = surface.limited_appendix_v1();
    validate_state_scoped(state, appendix).map_err(RlContractError)?;
    require_supported(state, Some(actor_and_origin.0), appendix).map_err(RlContractError)?;
    let appendix_consumer = if appendix == LimitedAppendixV1::Mixed {
        crate::optional_trigger_public_v1::ConsumerV1::KaitoMixedV1
    } else {
        crate::optional_trigger_public_v1::ConsumerV1::Legacy
    };
    let (inherited, families) = crate::ravenous_observation_v51::observe_parts_complete_kaito_v75(
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
    let mut value = KaitoObservationV75 {
        schema: match appendix {
            LimitedAppendixV1::Legacy => SCHEMA,
            LimitedAppendixV1::Artist => crate::artist_consumer_v1::OBSERVATION_SCHEMA,
            LimitedAppendixV1::Niv => crate::niv_consumer_v1::OBSERVATION_SCHEMA,
            LimitedAppendixV1::ChosenType => crate::chosen_type_consumer_v1::OBSERVATION_SCHEMA,
            LimitedAppendixV1::Drakuseth => crate::drakuseth_consumer_v1::OBSERVATION_SCHEMA,
            LimitedAppendixV1::Embercleave => crate::embercleave_consumer_v1::OBSERVATION_SCHEMA,
            LimitedAppendixV1::Zimone => crate::zimone_consumer_v1::OBSERVATION_SCHEMA,
            LimitedAppendixV1::Omniscience => crate::omniscience_consumer_v1::OBSERVATION_SCHEMA,
            LimitedAppendixV1::Tutelage => crate::tutelage_consumer_v1::OBSERVATION_SCHEMA,
            LimitedAppendixV1::Mixed => crate::limited_mixed_consumer_v1::OBSERVATION_SCHEMA,
            LimitedAppendixV1::Moon => crate::moon_consumer_v1::OBSERVATION_SCHEMA,
            LimitedAppendixV1::LeylineAxe => crate::leyline_axe_consumer_v1::OBSERVATION_SCHEMA,
            LimitedAppendixV1::Progenitus => crate::progenitus_consumer_v1::OBSERVATION_SCHEMA,
            LimitedAppendixV1::Aberration => crate::aberration_consumer_v1::OBSERVATION_SCHEMA,
            LimitedAppendixV1::Lathril => crate::lathril_consumer_v1::OBSERVATION_SCHEMA,
            LimitedAppendixV1::Electroduplicate => crate::electroduplicate_consumer_v1::OBSERVATION_SCHEMA,
            LimitedAppendixV1::Edict => crate::edict_consumer_v1::OBSERVATION_SCHEMA,
        }
        .into(),
        producer_sha256: producer_sha256(),
        common,
        drake: crate::drake_public_v53::capture_kaito_compat_v75(state, appendix)?,
        spinner: crate::spinner_public_v54::capture_kaito_compat_v75(state, appendix)
            .map_err(RlContractError)?,
        quandary: crate::quandary_public_v55::capture_kaito_compat_v75(
            state,
            actor_and_origin.0,
            appendix,
        )
        .map_err(RlContractError)?,
        phoenix: crate::phoenix_public_v56::capture(state)?,
        next_five: crate::next_five_public_v59::capture_kaito_compat_v75(
            state,
            actor_and_origin.0,
            appendix,
        )
        .map_err(RlContractError)?,
        graveyard_return: crate::graveyard_return_public_v61::capture_kaito_compat_v75(
            state,
            actor_and_origin.0,
            appendix,
        )
        .map_err(RlContractError)?,
        call: crate::valkyries_call_public_v62::capture_kaito_compat_v75(
            state,
            actor_and_origin.0,
            appendix,
        )
        .map_err(RlContractError)?,
        end_turn: crate::time_stop_public_v68::capture_tinybones_v72(state, actor_and_origin.0)
            .map_err(RlContractError)?,
        genesis_wave: crate::genesis_wave_public_v69::capture(state, actor_and_origin.0)
            .map_err(RlContractError)?,
        vivien: crate::vivien_public_v70::capture_tinybones_for_consumer_v72(
            state,
            actor_and_origin.0,
            crate::optional_trigger_public_v1::ConsumerV1::for_kaito(appendix),
        )
        .map_err(RlContractError)?,
        muldrotha: crate::muldrotha_public_v71::capture_tinybones_for_consumer_v72(
            state,
            actor_and_origin.0,
            crate::optional_trigger_public_v1::ConsumerV1::for_kaito(appendix),
        )
        .map_err(RlContractError)?,
        tinybones: crate::tinybones_public_v72::capture(state, actor_and_origin.0)
            .map_err(RlContractError)?,
        liliana: crate::liliana_public_v73::capture(state, actor_and_origin.0)
            .map_err(RlContractError)?,
        chandra: crate::chandra_public_v74::capture(state, actor_and_origin.0)
            .map_err(RlContractError)?,
        kaito: crate::kaito_public_v75::capture(state, actor_and_origin.0)
            .map_err(RlContractError)?,
        artist: (appendix.includes(LimitedAppendixV1::Artist))
            .then(|| {
                crate::artist_consumer_v1::capture_scoped(
                    state,
                    actor_and_origin.0,
                    appendix_consumer,
                )
            })
            .transpose()
            .map_err(RlContractError)?,
        niv: (appendix.includes(LimitedAppendixV1::Niv))
            .then(|| {
                crate::niv_mizzet_public_v63::capture_consumer_scoped(
                    state,
                    actor_and_origin.0,
                    appendix_consumer,
                )
            })
            .transpose()
            .map_err(RlContractError)?,
        chosen_type: (appendix.includes(LimitedAppendixV1::ChosenType))
            .then(|| {
                crate::chosen_type_consumer_v1::capture_scoped(
                    state,
                    actor_and_origin.0,
                    appendix_consumer,
                )
            })
            .transpose()
            .map_err(RlContractError)?,
        drakuseth: (appendix.includes(LimitedAppendixV1::Drakuseth))
            .then(|| {
                crate::drakuseth_consumer_v1::capture_scoped(
                    state,
                    actor_and_origin.0,
                    appendix_consumer,
                )
            })
            .transpose()
            .map_err(RlContractError)?,
        embercleave: (appendix.includes(LimitedAppendixV1::Embercleave))
            .then(|| {
                crate::embercleave_consumer_v1::capture_scoped(
                    state,
                    actor_and_origin.0,
                    appendix_consumer,
                )
            })
            .transpose()
            .map_err(RlContractError)?,
        zimone: (appendix.includes(LimitedAppendixV1::Zimone))
            .then(|| {
                crate::zimone_public_v1::capture_scoped(
                    state,
                    actor_and_origin.0,
                    appendix_consumer,
                )
            })
            .transpose()
            .map_err(RlContractError)?,
        omniscience: (appendix.includes(LimitedAppendixV1::Omniscience))
            .then(|| {
                crate::omniscience_public_v1::capture(state, actor_and_origin.0, actor_and_origin.1)
            })
            .transpose()
            .map_err(RlContractError)?,
        tutelage: (appendix.includes(LimitedAppendixV1::Tutelage))
            .then(|| crate::tutelage_public_v1::capture(state, actor_and_origin.0))
            .transpose()
            .map_err(RlContractError)?,
        moon: (appendix == LimitedAppendixV1::Moon)
            .then(|| crate::moon_public_v1::capture(state, actor_and_origin.0))
            .transpose()
            .map_err(RlContractError)?,
        leyline_axe: (appendix == LimitedAppendixV1::LeylineAxe)
            .then(|| crate::leyline_axe_public_v1::capture(state, actor_and_origin.0))
            .transpose()
            .map_err(RlContractError)?,
        progenitus: (appendix == LimitedAppendixV1::Progenitus)
            .then(|| crate::progenitus_public_v1::capture(state, actor_and_origin.0))
            .transpose()
            .map_err(RlContractError)?,
        aberration: (appendix == LimitedAppendixV1::Aberration)
            .then(|| crate::aberration_public_v1::capture(state, actor_and_origin.0))
            .transpose()
            .map_err(RlContractError)?,
        lathril: (appendix == LimitedAppendixV1::Lathril)
            .then(|| crate::lathril_public_v1::capture(state, actor_and_origin.0))
            .transpose()
            .map_err(RlContractError)?,
        electroduplicate: (appendix == LimitedAppendixV1::Electroduplicate)
            .then(|| crate::electroduplicate_public_v1::capture(state, actor_and_origin.0))
            .transpose()
            .map_err(RlContractError)?,
        edict: (appendix == LimitedAppendixV1::Edict)
            .then(|| crate::edict_public_v1::capture(state, actor_and_origin.0, actor_and_origin.1))
            .transpose()
            .map_err(RlContractError)?,
        complete_ordinary_observation: true,
        owning_consumer_admitted: false,
        projection_sha256: String::new(),
    };
    // Compatibility components are not standalone frozen artifacts.
    value.call.schema = format!("{SCHEMA}/compat/call-v62");
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
pub fn validate_artifact(value: &KaitoObservationV75) -> Result<(), RlContractError> {
    let actor = value.common.common.acting_player;
    let schema = match (
        &value.artist,
        &value.niv,
        &value.chosen_type,
        &value.drakuseth,
        &value.embercleave,
        &value.zimone,
        &value.omniscience,
        &value.tutelage,
        &value.moon,
        &value.leyline_axe,
        &value.progenitus,
        &value.aberration,
        &value.lathril,
        &value.electroduplicate,
        &value.edict,
    ) {
        (Some(artist), Some(niv), Some(chosen), Some(drakuseth), Some(embercleave), Some(zimone), Some(omniscience), Some(tutelage), None, None, None, None, None, None, None) if value.schema == crate::limited_mixed_consumer_v1::OBSERVATION_SCHEMA => {
            crate::artist_consumer_v1::validate_facts(artist, actor).map_err(RlContractError)?;
            crate::niv_consumer_v1::validate_facts(niv, actor).map_err(RlContractError)?;
            crate::chosen_type_consumer_v1::validate_facts(chosen, actor)
                .map_err(RlContractError)?;
            crate::drakuseth_consumer_v1::validate_facts(drakuseth, actor)
                .map_err(RlContractError)?;
            crate::embercleave_consumer_v1::validate_facts(embercleave, actor)
                .map_err(RlContractError)?;
            crate::zimone_consumer_v1::validate_facts(zimone, actor).map_err(RlContractError)?;
            crate::omniscience_consumer_v1::validate_facts(omniscience, actor)
                .map_err(RlContractError)?;
            crate::tutelage_consumer_v1::validate_facts(tutelage, actor)
                .map_err(RlContractError)?;
            crate::limited_mixed_consumer_v1::OBSERVATION_SCHEMA
        }
        (Some(artist), None, None, None, None, None, None, None, None, None, None, None, None, None, None) => {
            crate::artist_consumer_v1::validate_facts(artist, actor).map_err(RlContractError)?;
            crate::artist_consumer_v1::OBSERVATION_SCHEMA
        }
        (None, Some(niv), None, None, None, None, None, None, None, None, None, None, None, None, None) => {
            crate::niv_consumer_v1::validate_facts(niv, actor).map_err(RlContractError)?;
            crate::niv_consumer_v1::OBSERVATION_SCHEMA
        }
        (None, None, Some(chosen_type), None, None, None, None, None, None, None, None, None, None, None, None) => {
            crate::chosen_type_consumer_v1::validate_facts(chosen_type, actor)
                .map_err(RlContractError)?;
            crate::chosen_type_consumer_v1::OBSERVATION_SCHEMA
        }
        (None, None, None, Some(facts), None, None, None, None, None, None, None, None, None, None, None) => {
            crate::drakuseth_consumer_v1::validate_facts(facts, actor).map_err(RlContractError)?;
            crate::drakuseth_consumer_v1::OBSERVATION_SCHEMA
        }
        (None, None, None, None, Some(facts), None, None, None, None, None, None, None, None, None, None) => {
            crate::embercleave_consumer_v1::validate_facts(facts, actor)
                .map_err(RlContractError)?;
            crate::embercleave_consumer_v1::OBSERVATION_SCHEMA
        }
        (None, None, None, None, None, Some(facts), None, None, None, None, None, None, None, None, None) => {
            crate::zimone_consumer_v1::validate_facts(facts, actor).map_err(RlContractError)?;
            crate::zimone_consumer_v1::OBSERVATION_SCHEMA
        }
        (None, None, None, None, None, None, Some(facts), None, None, None, None, None, None, None, None) => {
            crate::omniscience_consumer_v1::validate_facts(facts, actor)
                .map_err(RlContractError)?;
            crate::omniscience_consumer_v1::OBSERVATION_SCHEMA
        }
        (None, None, None, None, None, None, None, Some(facts), None, None, None, None, None, None, None) => {
            crate::tutelage_consumer_v1::validate_facts(facts, actor).map_err(RlContractError)?;
            crate::tutelage_consumer_v1::OBSERVATION_SCHEMA
        }
        (None, None, None, None, None, None, None, None, Some(facts), None, None, None, None, None, None) => {
            crate::moon_consumer_v1::validate_facts(facts, actor).map_err(RlContractError)?;
            crate::moon_consumer_v1::OBSERVATION_SCHEMA
        }
        (None, None, None, None, None, None, None, None, None, Some(facts), None, None, None, None, None) => {
            crate::leyline_axe_consumer_v1::validate_facts(facts, actor)
                .map_err(RlContractError)?;
            crate::leyline_axe_consumer_v1::OBSERVATION_SCHEMA
        }
        (None, None, None, None, None, None, None, None, None, None, None, None, None, None, None) => SCHEMA,
        (None, None, None, None, None, None, None, None, None, None, Some(facts), None, None, None, None) => {
            crate::progenitus_consumer_v1::validate_facts(facts, actor).map_err(RlContractError)?;
            crate::progenitus_consumer_v1::OBSERVATION_SCHEMA
        }
        (None, None, None, None, None, None, None, None, None, None, None, Some(facts), None, None, None) => {
            crate::aberration_consumer_v1::validate_facts(facts, actor).map_err(RlContractError)?;
            crate::aberration_consumer_v1::OBSERVATION_SCHEMA
        }
        (None, None, None, None, None, None, None, None, None, None, None, None, Some(facts), None, None) => {
            crate::lathril_consumer_v1::validate_facts(facts, actor).map_err(RlContractError)?;
            crate::lathril_consumer_v1::OBSERVATION_SCHEMA
        }
        (None, None, None, None, None, None, None, None, None, None, None, None, None, Some(facts), None) => {
            crate::electroduplicate_consumer_v1::validate_facts(facts, actor).map_err(RlContractError)?;
            crate::electroduplicate_consumer_v1::validate_inherited(facts, value).map_err(RlContractError)?;
            crate::electroduplicate_consumer_v1::OBSERVATION_SCHEMA
        }
        (None, None, None, None, None, None, None, None, None, None, None, None, None, None, Some(facts)) => {
            crate::edict_consumer_v1::validate_facts(facts, actor).map_err(RlContractError)?;
            crate::edict_consumer_v1::OBSERVATION_SCHEMA
        }
        _ => return Err(RlContractError("mixed appendix profiles".into())),
    };
    crate::muldrotha_public_v71::validate_tinybones_compat_v72(&value.muldrotha)
        .map_err(RlContractError)?;
    crate::vivien_public_v70::validate_tinybones_compat_v72(&value.vivien)
        .map_err(RlContractError)?;
    crate::genesis_wave_public_v69::validate(&value.genesis_wave).map_err(RlContractError)?;
    crate::time_stop_public_v68::validate(&value.end_turn).map_err(RlContractError)?;
    crate::tinybones_public_v72::validate(&value.tinybones).map_err(RlContractError)?;
    crate::liliana_public_v73::validate(&value.liliana).map_err(RlContractError)?;
    crate::chandra_public_v74::validate(&value.chandra).map_err(RlContractError)?;
    crate::kaito_public_v75::validate(&value.kaito).map_err(RlContractError)?;
    if value.kaito.battlefield != value.tinybones.battlefield
        || value.chandra.battlefield != value.tinybones.battlefield
        || value.liliana.battlefield != value.tinybones.battlefield
    {
        return Err(RlContractError(
            "V75 authoritative current rosters differ".into(),
        ));
    }
    let inherited = &value.common.common;
    validate_compatibility_terminals_v68(value)?;
    if value.kaito.viewer != inherited.acting_player
        || value.chandra.viewer != inherited.acting_player
        || value.liliana.viewer != inherited.acting_player
        || value.tinybones.viewer != inherited.acting_player
        || value.muldrotha.viewer != inherited.acting_player
        || value.vivien.viewer != inherited.acting_player
        || value.end_turn.viewer != inherited.acting_player
        || value.genesis_wave.viewer != inherited.acting_player
    {
        return Err(RlContractError(
            "V75 Time Stop viewer differs from current actor".into(),
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
    inherited.validate_kaito_compatibility_v75()?;
    crate::rules_meaning_elenda_v52::validate(&value.common.elenda.meaning)
        .map_err(|e| RlContractError(format!("V75 Elenda meaning:{e:?}")))?;
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
            "V75 inherited family artifact identity/root order changed".into(),
        ));
    }
    if value.schema != schema
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
        return Err(RlContractError("V75 artifact identity changed".into()));
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
            "V75 closed source identity/permission changed".into(),
        ));
    }
    crate::rules_meaning_valkyries_call_v62::validate(&value.call.meaning)
        .map_err(RlContractError)?;
    if value.call.schema != format!("{SCHEMA}/compat/call-v62")
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
            || exact.len() != value.tinybones.battlefield[seat].len()
        {
            return Err(RlContractError(
                "V75 complete battlefield roster changed".into(),
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
            let full18 = &value.tinybones.battlefield[seat][ordinal];
            let mut prefix18 =
                serde_json::to_value(full18).map_err(|e| RlContractError(e.to_string()))?;
            prefix18
                .get_mut("counters")
                .and_then(serde_json::Value::as_object_mut)
                .ok_or_else(|| RlContractError("V75 full counter roster absent".into()))?
                .remove("stash");
            if prefix18 != serde_json::to_value(call).map_err(|e| RlContractError(e.to_string()))? {
                return Err(RlContractError(
                    "V75 mandatory eighteen-counter/form/incarnation prefix changed".into(),
                ));
            }
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
                    "V75 mandatory counter incarnation/order changed".into(),
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
                return Err(RlContractError("V75 public counter underflow".into()));
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
                    "V75 exact card roster/value mismatch".into(),
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
    value: &KaitoObservationV75,
) -> Result<(), RlContractError> {
    if observe_v75(state, surface, actor_and_origin, step, physical, substep)? != *value {
        return Err(RlContractError(
            "V75 observation differs from current actual decision".into(),
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
        .ok_or_else(|| RlContractError("V75 counter object absent".into()))?
        .values()
        .any(|n| n.as_i64().is_none_or(|n| n < 0))
    {
        return Err(RlContractError(
            "V75 counter underflow/width changed".into(),
        ));
    }
    encoded
        .as_object_mut()
        .ok_or_else(|| RlContractError("V75 counter object absent".into()))?
        .remove("revival");
    if encoded != serde_json::to_value(prefix).map_err(|e| RlContractError(e.to_string()))? {
        return Err(RlContractError(
            "V75 complete counters lost compatibility prefix meaning".into(),
        ));
    }
    Ok(())
}

/// A compatibility appendix's historical publication label cannot override its
/// exact V68 terminal result. No old enum or serialized row is reinterpreted
/// as an executable current stack membership.
fn validate_compatibility_terminals_v68(v: &KaitoObservationV75) -> Result<(), RlContractError> {
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
                    .ok_or_else(|| RlContractError("V75 Drake combat role absent".into()))?;
                if row.outcome != crate::drake_public_v53::CombatOutcomeV53::Published {
                    return Err(RlContractError(
                        "V75 disposed combat acquired another terminal outcome".into(),
                    ));
                }
                drake_compatibility_reference_v68(&row.original_source)?
            }
            T::DrakeActivation { occurrence } => {
                let row = v
                    .drake
                    .activations
                    .get(occurrence as usize)
                    .ok_or_else(|| RlContractError("V75 Drake activation role absent".into()))?;
                if row.outcome != crate::drake_public_v53::ActivationOutcomeV53::Published {
                    return Err(RlContractError(
                        "V75 disposed activation acquired another terminal outcome".into(),
                    ));
                }
                drake_compatibility_reference_v68(&row.original_source)?
            }
            T::RavenousActivation { occurrence } => {
                let row = old
                    .ravenous
                    .activations
                    .get(occurrence as usize)
                    .ok_or_else(|| RlContractError("V75 Amulet role absent".into()))?;
                if row.outcome != crate::ravenous_public_v51::OutcomeV51::Published {
                    return Err(RlContractError(
                        "V75 disposed Amulet acquired another terminal outcome".into(),
                    ));
                }
                row.source.clone()
            }
            T::AjaniActivation { position } => {
                let row = old
                    .ajani
                    .activations
                    .get(position as usize)
                    .ok_or_else(|| RlContractError("V75 Ajani role absent".into()))?;
                if row.stage != crate::ajani_public_v42::StageV42::OnStack {
                    return Err(RlContractError(
                        "V75 disposed Ajani acquired another terminal outcome".into(),
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
                    .ok_or_else(|| RlContractError("V75 Fishing family absent".into()))?;
                if families.next().is_some() {
                    return Err(RlContractError("V75 duplicate Fishing family".into()));
                }
                let row = family
                    .operations
                    .get(position as usize)
                    .ok_or_else(|| RlContractError("V75 Fishing role absent".into()))?;
                if row.stage != crate::fishing_pole_public_v39::StageV39::OnStack
                    || row.stack_ordinal.is_some()
                {
                    return Err(RlContractError(
                        "V75 disposed Fishing acquired another live/terminal outcome".into(),
                    ));
                }
                row.original_pole.clone()
            }
            T::FaebloomSpell { ordinal } => {
                let row = old
                    .faebloom
                    .spells
                    .get(ordinal as usize)
                    .ok_or_else(|| RlContractError("V75 Faebloom spell role absent".into()))?;
                if row.stage != crate::faebloom_public_v43::StageV43::OnStack
                    || row.stack_ordinal.is_some()
                {
                    return Err(RlContractError(
                        "V75 disposed Faebloom acquired another live/terminal outcome".into(),
                    ));
                }
                row.original_source.clone()
            }
            T::FaebloomReflexive { ordinal } => {
                let row = old
                    .faebloom
                    .reflexives
                    .get(ordinal as usize)
                    .ok_or_else(|| RlContractError("V75 Faebloom reflexive role absent".into()))?;
                if row.stage != crate::faebloom_public_v43::StageV43::OnStack
                    || row.stack_ordinal.is_some()
                {
                    return Err(RlContractError(
                        "V75 disposed reflexive acquired another live/terminal outcome".into(),
                    ));
                }
                row.original_source.clone()
            }
        };
        if source != original.source || source.controller != original.controller {
            return Err(RlContractError("V75 compatibility terminal lost exact original public source/controller/incarnation".into()));
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
            .ok_or_else(|| RlContractError("V75 neutral source sentinel absent".into()))?
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
