//! Distinct current-family numeric/scorer prerequisite. Frozen encoders and search are untouched.
//! Public packets contain numeric features only. Bindings and executable candidates remain private.
mod call;
mod end_turn;
mod genesis_wave;
pub use end_turn::{EndTurnDecision, EndTurnSession, EndTurnSnapshot};
pub use genesis_wave::{GenesisWaveDecision, GenesisWaveSession, GenesisWaveSnapshot};
mod canonical_semantic_booleans;
mod elenda;
mod fiery_public_operands;
mod fishing_public_operands;
mod graveyard_return;
pub use call::{CallDecision, CallSession, CallSnapshot};
mod kellan_public_operands;
mod network;
mod next_five;
mod next_five_support;
mod normalize;
mod numeric;
mod prepared;
mod public_projection_operands;
mod roots;
use crate::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, FastActorSessionSnapshotV1, FastActorSessionV1,
};
pub use elenda::{ElendaDecision, ElendaSession, ElendaSnapshot};
pub use graveyard_return::{
    GraveyardReturnDecision, GraveyardReturnSession, GraveyardReturnSnapshot,
};
pub use network::{ModelConfig, NativeModel};
pub use next_five::{NextFiveDecision, NextFiveSession, NextFiveSnapshot};
pub use numeric::NumericTree;
pub use prepared::{PreparedDecision, PreparedSession, PreparedSnapshot};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    NoDecision,
    Range,
    NonIntegralOperand,
    UnsupportedPrintedRoot(u16),
    UnsupportedOperand(String),
    Projection(String),
    StaleBinding,
    ForeignSnapshot,
    ModelContract,
    NonFiniteLogit,
    ActionRange,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "current_mixed_policy_v1:{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    FakeV50,
    RavenousV51,
    ElendaV52,
    PreparedV57,
    NextFiveV59,
    GraveyardReturnV61,
    CallV62,
    EndTurnV68,
    GenesisWaveV69,
    VivienV70,
    MuldrothaV71,
    TinybonesV72,
    LilianaV73,
    ChandraV74,
    KaitoV75,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packet {
    source: Source,
    state: NumericTree,
    actions: Vec<NumericTree>,
}
impl Packet {
    pub fn source(&self) -> Source {
        self.source
    }
    pub fn state(&self) -> &NumericTree {
        &self.state
    }
    /// Rows are in the exact original backend legal-action order, without label sorting.
    pub fn actions(&self) -> &[NumericTree] {
        &self.actions
    }
}

pub fn feature_identity() -> String {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(|| {
        let mut hash = Sha256::new();
        for part in [
            "current-mixed-policy/numeric-tree/v1",
            include_str!("current_mixed_policy_v1.rs"),
            include_str!("current_mixed_policy_v1/numeric.rs"),
            include_str!("current_mixed_policy_v1/normalize.rs"),
            include_str!("current_mixed_policy_v1/public_projection_operands.rs"),
            include_str!("current_mixed_policy_v1/canonical_semantic_booleans.rs"),
            include_str!("current_mixed_policy_v1/divine_printing.rs"),
            include_str!("current_mixed_policy_v1/roots.rs"),
            include_str!("current_mixed_policy_v1/ordinary_removal_printing.rs"),
            include_str!("current_mixed_policy_v1/targeted_spell_printing.rs"),
            include_str!("current_mixed_policy_v1/simple_trigger_printing.rs"),
            include_str!("current_mixed_policy_v1/observer_trigger_printing.rs"),
            include_str!("current_mixed_policy_v1/ordinary_equipment_printing.rs"),
            include_str!("current_mixed_policy_v1/eaten_printing.rs"),
            include_str!("current_mixed_policy_v1/enchantment_hunter_printing.rs"),
            include_str!("current_mixed_policy_v1/faebloom_printing.rs"),
            include_str!("current_mixed_policy_v1/fiery_printing.rs"),
            include_str!("current_mixed_policy_v1/fiery_public_operands.rs"),
            include_str!("current_mixed_policy_v1/fishing_public_operands.rs"),
            include_str!("fishing_pole_public_v39.rs"),
            include_str!("fishing_pole_observation_v39.rs"),
            include_str!("rules_meaning_fishing_pole_v39.rs"),
            include_str!("fishing_pole_v39.rs"),
            include_str!("fishing_pole_v39/lifecycle.rs"),
            include_str!("rl_session/fishing_pole_v39.rs"),
            include_str!("fiery_public_v1.rs"),
            include_str!("fiery_observation_v1.rs"),
            include_str!("fiery_annihilation_v1.rs"),
            include_str!("current_mixed_policy_v1/crystal_printing.rs"),
            include_str!("current_mixed_policy_v1/tyrant_herald_printing.rs"),
            include_str!("current_mixed_policy_v1/soulstone_printing.rs"),
            include_str!("twinflame_tyrant_v1.rs"),
            include_str!("current_mixed_policy_v1/legendary_printing.rs"),
            include_str!("current_mixed_policy_v1/early_activation_printing.rs"),
            include_str!("current_mixed_policy_v1/rallier_bloom_printing.rs"),
            include_str!("current_mixed_policy_v1/kellan_printing.rs"),
            include_str!("current_mixed_policy_v1/kellan_public_operands.rs"),
            include_str!("arahbo_v1.rs"),
            include_str!("zul_ashur_v1.rs"),
            include_str!("current_mixed_policy_v1/network.rs"),
            include_str!("current_mixed_policy_v1/elenda.rs"),
            include_str!("current_mixed_policy_v1/prepared.rs"),
            include_str!("prepared_observation_v57.rs"),
            include_str!("rl_session/prepared_v57.rs"),
            include_str!("drake_public_v53.rs"),
            include_str!("drake_hatcher_v1/custody.rs"),
            include_str!("rules_meaning_drake_hatcher_v53.rs"),
            include_str!("optional_trigger_source_v1.rs"),
            include_str!("optional_trigger_public_v1.rs"),
            include_str!("rules_meaning_spinner_v54.rs"),
            include_str!("spinner_public_v54.rs"),
            include_str!("rules_meaning_quandary_v55.rs"),
            include_str!("quandary_public_v55.rs"),
            include_str!("rules_meaning_phoenix_v56.rs"),
            include_str!("phoenix_public_v56.rs"),
            include_str!("elenda_observation_v52.rs"),
            include_str!("elenda_public_v52.rs"),
            include_str!("rules_meaning_elenda_v52.rs"),
            include_str!("starting_life_statics_v1.rs"),
            include_str!("source_type_hexproof_v1.rs"),
            include_str!("rl_session/elenda_v52.rs"),
            include_str!("rl_session.rs"),
            include_str!("fake_observation_v50.rs"),
            include_str!("ravenous_observation_v51.rs"),
            include_str!("rl_session/fake_v50.rs"),
            include_str!("rl_session/ravenous_v51.rs"),
            include_str!("rules_meaning_ajani_v42.rs"),
            include_str!("rules_meaning_akroma_v1.rs"),
            include_str!("rules_meaning_arahbo_v1.rs"),
            include_str!("rules_meaning_ashroot_v1.rs"),
            include_str!("rules_meaning_authority_v1.rs"),
            include_str!("rules_meaning_banishing_v1.rs"),
            include_str!("rules_meaning_bloodthirsty_conqueror_v49.rs"),
            include_str!("bloodthirsty_conqueror_v49.rs"),
            include_str!("rules_meaning_bloom_tender_v1.rs"),
            include_str!("rules_meaning_cerberus_v1.rs"),
            include_str!("rules_meaning_crystal_barricade_v1.rs"),
            include_str!("rules_meaning_crystal_ordering_v1.rs"),
            include_str!("rules_meaning_divine_v44.rs"),
            include_str!("divine_public_v44.rs"),
            include_str!("policy_observation_variable_cast_v1.rs"),
            include_str!("rl_variable_cast_v1.rs"),
            include_str!("rl_session/variable_cast_v1.rs"),
            include_str!("rules_meaning_doubling_season_v48.rs"),
            include_str!("doubling_season_v48.rs"),
            include_str!("rules_meaning_eaten_alive_v1.rs"),
            include_str!("rules_meaning_employment_v1.rs"),
            include_str!("rules_meaning_extra_turn_v1.rs"),
            include_str!("rules_meaning_faebloom_v43.rs"),
            include_str!("rules_meaning_fake_your_own_death_v50.rs"),
            include_str!("rules_meaning_fiendish_panda_v1.rs"),
            include_str!("rules_meaning_fiery_v1.rs"),
            include_str!("rules_meaning_fishing_pole_v39.rs"),
            include_str!("rules_meaning_frenzied_goblin_v1.rs"),
            include_str!("rules_meaning_game_outcome_protection_v1.rs"),
            include_str!("rules_meaning_garruk_v1.rs"),
            include_str!("rules_meaning_gourmand_v1.rs"),
            include_str!("rules_meaning_heartfire_v1.rs"),
            include_str!("rules_meaning_heraldic_banner_v1.rs"),
            include_str!("rules_meaning_high_society_hunter_v1.rs"),
            include_str!("rules_meaning_inspiration_v1.rs"),
            include_str!("rules_meaning_juggernaut_v1.rs"),
            include_str!("juggernaut_public_v1.rs"),
            include_str!("rules_meaning_krenko_v1.rs"),
            include_str!("rules_meaning_librarian_v1.rs"),
            include_str!("rules_meaning_ooze_v1.rs"),
            include_str!("rules_meaning_raid_entry_v1.rs"),
            include_str!("rules_meaning_ravenous_amulet_v51.rs"),
            include_str!("rules_meaning_searslicer_v1.rs"),
            include_str!("rules_meaning_self_reflection_v1.rs"),
            include_str!("rules_meaning_soul_shackled_zombie_v1.rs"),
            include_str!("rules_meaning_sower_v1.rs"),
            include_str!("rules_meaning_squad_rallier_v1.rs"),
            include_str!("rules_meaning_stromkirk_v1.rs"),
            include_str!("rules_meaning_strongbox_v1.rs"),
            include_str!("rules_meaning_v2.rs"),
            include_str!("rules_meaning_wildwood_v1.rs"),
            include_str!("rules_meaning_wildwood_v1_tests.rs"),
            include_str!("rules_meaning_zul_ashur_v1.rs"),
            include_str!("card_def.rs"),
            include_str!("engine.rs"),
            include_str!("spell_costs_v1.rs"),
            include_str!("mana.rs"),
            include_str!("rl.rs"),
            include_str!("akroma_family_public_v40.rs"),
            include_str!("frenzied_goblin_v1.rs"),
            include_str!("effect/frenzied_goblin_resolution_v1.rs"),
            include_str!("sower_restriction_capture_v1.rs"),
            include_str!("frenzied_public_v1.rs"),
            include_str!("sower_public_v1.rs"),
            include_str!("policy_observation_v8.rs"),
            include_str!("policy_surface_v5.rs"),
            include_str!("policy_surface_v5/hunter_order_v45.rs"),
            include_str!("ravenous_public_v51.rs"),
            include_str!("fake_your_own_death_v50/public_v50.rs"),
            include_str!("standard_creatures_v1.rs"),
            include_str!("continuous_characteristics_v1.rs"),
            include_str!("trigger.rs"),
            include_str!("effect.rs"),
            include_str!("state.rs"),
            include_str!("event.rs"),
            include_str!("arahbo_public_v1.rs"),
            include_str!("ashroot_public_v1.rs"),
            include_str!("authority_public_v1.rs"),
            include_str!("banishing_public_v1.rs"),
            include_str!("banner_observation_v1.rs"),
            include_str!("cerberus_public_v1.rs"),
            include_str!("crystal_public_v1.rs"),
            include_str!("employment_public_v1.rs"),
            include_str!("extra_turn_public_v1.rs"),
            include_str!("garruk_public_v1.rs"),
            include_str!("gourmand_public_v1.rs"),
            include_str!("inspiration_observation_v1.rs"),
            include_str!("krenko_observation_v1.rs"),
            include_str!("librarian_public_v1.rs"),
            include_str!("stromkirk_public_v1.rs"),
            include_str!("strongbox_public_v1.rs"),
            include_str!("twinflame_observation_v1.rs"),
            include_str!("wildwood_public_v1.rs"),
            include_str!("zul_ashur_observation_v1.rs"),
            include_str!("bloodthirsty_conqueror_v49/custody.rs"),
            include_str!("linked_aura_operation_v1.rs"),
            include_str!("current_mixed_policy_v1/next_five.rs"),
            include_str!("current_mixed_policy_v1/next_five_support.rs"),
            include_str!("next_five_observation_v59.rs"),
            include_str!("next_five_public_v59.rs"),
            include_str!("snack_wide_public_v59.rs"),
            include_str!("panda_public_v47.rs"),
            include_str!("conqueror_public_v49.rs"),
            include_str!("rules_meaning_next_five_v59.rs"),
            include_str!("rules_meaning_midnight_snack_v58.rs"),
            include_str!("midnight_snack_public_v58.rs"),
            include_str!("midnight_snack_v1.rs"),
            include_str!("extravagant_replication_v1.rs"),
            include_str!("infernal_vessel_v1.rs"),
            include_str!("giada_v1.rs"),
            include_str!("goblin_negotiation_v1.rs"),
            include_str!("goblin_negotiation_v1/owned.rs"),
            include_str!("effect/nonland_copy_v1.rs"),
            include_str!("effect/curator_v1.rs"),
            include_str!("effect/loot_v1.rs"),
            include_str!("effect/kykar_v1.rs"),
            include_str!("effect/greatwurm_v1.rs"),
            include_str!("effect/source_token_copy_v1.rs"),
            include_str!("engine/greatwurm_cast_v1.rs"),
            include_str!("effect/sphinx_lore_v1.rs"),
            include_str!("effect/harvester_v1.rs"),
            include_str!("leyline_axe_v1.rs"),
            include_str!("london_mulligan_v1.rs"),
            include_str!("soul_shackled_zombie_v1.rs"),
            include_str!("engine/sphinx_lore_cast_v1.rs"),
            include_str!("effect/squad_rallier_resolution_v1.rs"),
            include_str!("effect/crystal_prevention_v1.rs"),
            include_str!("creature_death_capture_v1.rs"),
            include_str!("rl_session/next_five_v59.rs"),
            include_str!("controlled_creature_grants_v1.rs"),
            include_str!("standard_statics_v1.rs"),
            include_str!("standard_legends_v1.rs"),
            include_str!("standard_cards_v1.rs"),
            include_str!("self_reflection_v1.rs"),
            include_str!("life_loss_turn_v1.rs"),
            include_str!("../build.rs"),
            include_str!("standard_cast_convoke.rs"),
            include_str!("banner_of_kinship_v1.rs"),
            include_str!("banner_of_kinship_v1/entry.rs"),
            include_str!("current_mixed_policy_v1/graveyard_return.rs"),
            include_str!("graveyard_return_candidates_v61.rs"),
            include_str!("graveyard_return_observation_v61.rs"),
            include_str!("graveyard_return_public_v61.rs"),
            include_str!("rules_meaning_graveyard_return_v61.rs"),
            include_str!("alesha_v1.rs"),
            include_str!("nine_lives_familiar_v1.rs"),
            include_str!("rl_session/graveyard_return_v61.rs"),
            include_str!("heraldic_banner_v1.rs"),
            include_str!("creature_types_v60.rs"),
            include_str!("../../data/rules/creature_types_v60.json"),
            include_str!("counter_target_effects_v1.rs"),
            include_str!("zimone_paradox_sculptor_v1.rs"),
            include_str!("planeswalker_v1.rs"),
            include_str!("current_mixed_policy_v1/call.rs"),
            include_str!("human_bo3_v1/call_v62.rs"),
            include_str!("limited_call_session_v8.rs"),
            include_str!("human_bo3_v1/limited_call_v8.rs"),
            include_str!("call_observation_v62.rs"),
            include_str!("call_candidates_v62.rs"),
            include_str!("rl_session/call_v62.rs"),
            include_str!("rules_meaning_valkyries_call_v62.rs"),
            include_str!("valkyries_call_public_v62.rs"),
            include_str!("valkyries_call_v1.rs"),
            include_str!("engine/omniscience_cast_v1.rs"),
            include_str!("resolution_cast_v1.rs"),
            include_str!("progenitus_v1.rs"),
            include_str!("source_power_activation_v1.rs"),
            include_str!("rl_session/flat_action_v4/search_state/effect_refs.rs"),
            include_str!("effect/inspiration_resolution_v1.rs"),
            include_str!("effect/standard_exile.rs"),
            include_str!("snack_wide_life_v59.rs"),
            include_str!("snack_wide_life_v59/custody.rs"),
            include_str!("fiendish_panda_v1.rs"),
            include_str!("fiendish_panda_v1/custody.rs"),
            include_str!("life_gain_turn_v1.rs"),
            include_str!("legend_rule_v1.rs"),
            include_str!("engine/mandatory_cast_cost_v1.rs"),
            include_str!("effect/thousand_year_storm_v1.rs"),
            include_str!("effect/standard_copy.rs"),
            include_str!("standard_spell_copy.rs"),
            include_str!("event_history_v1.rs"),
            include_str!("rules_meaning_vivien_v70.rs"),
            include_str!("vivien_public_v70.rs"),
            include_str!("vivien_observation_v70.rs"),
            include_str!("vivien_candidates_v70.rs"),
            include_str!("rl_session/vivien_v70.rs"),
            include_str!("current_mixed_policy_v1/vivien.rs"),
            include_str!("human_bo3_v1/vivien_v70.rs"),
            include_str!("human_bo3_v1/limited_vivien_v11.rs"),
            include_str!("limited_vivien_session_v11.rs"),
            include_str!("bin/kernel_limited_vivien_env_v11.rs"),
            include_str!("effect/vivien_reid_v1.rs"),
            include_str!("effect/squad_rallier_resolution_v1.rs"),
            crate::engine::time_stop_v1::source_sha256_v1(),
            crate::effect::genesis_wave_v1::source_sha256_v1(),
            include_str!("engine/tinybones_v72.rs"),
            include_str!("muldrotha_public_v71.rs"),
            include_str!("rules_meaning_muldrotha_v71.rs"),
            include_str!("muldrotha_observation_v71.rs"),
            include_str!("muldrotha_candidates_v71.rs"),
            include_str!("rl_session/muldrotha_v71.rs"),
            include_str!("current_mixed_policy_v1/muldrotha.rs"),
            include_str!("human_bo3_v1/muldrotha_v71.rs"),
            include_str!("human_bo3_v1/limited_muldrotha_v12.rs"),
            include_str!("limited_muldrotha_session_v12.rs"),
            include_str!("bin/kernel_limited_muldrotha_env_v12.rs"),
            include_str!("rules_meaning_tinybones_v72.rs"),
            include_str!("tinybones_public_v72.rs"),
            include_str!("tinybones_observation_v72.rs"),
            include_str!("tinybones_candidates_v72.rs"),
            include_str!("rl_session/tinybones_v72.rs"),
            include_str!("current_mixed_policy_v1/tinybones.rs"),
            include_str!("current_mixed_policy_v1/liliana.rs"),
            include_str!("rules_meaning_liliana_v73.rs"),
            include_str!("liliana_public_v73.rs"),
            include_str!("liliana_observation_v73.rs"),
            include_str!("liliana_candidates_v73.rs"),
            include_str!("rl_session/liliana_v73.rs"),
            include_str!("human_bo3_v1/tinybones_v72.rs"),
            include_str!("human_bo3_v1/limited_tinybones_v13.rs"),
            include_str!("limited_tinybones_session_v13.rs"),
            include_str!("bin/kernel_limited_tinybones_env_v13.rs"),
            include_str!("../../python/tools/limited_tinybones_session_v13.py"),
            include_str!("chandra_observation_v74.rs"),
            include_str!("chandra_candidates_v74.rs"),
            include_str!("rl_session/chandra_v74.rs"),
            include_str!("current_mixed_policy_v1/chandra.rs"),
            include_str!("chandra_public_v74.rs"),
            include_str!("effect/chandra_flameshaper_public_v74.rs"),
            include_str!("effect/chandra_flameshaper_v1.rs"),
            include_str!("rules_meaning_chandra_v74.rs"),
            include_str!("rl_session/flat_action_v4.rs"),
            include_str!("rl_session/flat_action_v4/search_state.rs"),
            include_str!("rl_session/flat_action_v4/search_state/sampler.rs"),
            include_str!("kaito_observation_v75.rs"),
            include_str!("kaito_candidates_v75.rs"),
            include_str!("rl_session/kaito_v75.rs"),
            include_str!("current_mixed_policy_v1/kaito.rs"),
            include_str!("kaito_public_v75.rs"),
            include_str!("effect/kaito_cunning_v75.rs"),
            include_str!("rules_meaning_kaito_v75.rs"),
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
            include_str!("effect/liliana_dreadhorde_v1.rs"),
            include_str!("human_bo3_v1/kaito_v75.rs"),
            include_str!("human_bo3_v1/limited_kaito_v16.rs"),
            include_str!("limited_kaito_session_v16.rs"),
            include_str!("bin/kernel_limited_kaito_env_v16.rs"),
            include_str!("../../python/tools/limited_kaito_session_v16.py"),
        ] {
            hash.update((part.len() as u64).to_le_bytes());
            hash.update(part.as_bytes());
        }
        hash.update(crate::card_def::KERNEL_CARDDB_HASH.to_le_bytes());
        hash.update(b"legacy-graveyard-return-guard/v61\0");
        hash.update(crate::graveyard_return_public_v61::producer_sha256().as_bytes());
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}

// Pure normalization of a fresh public input, without recapturing any decision
// or acquiring authority. Encode and apply share this identical packet grammar.
fn packet_from_current(
    source: Source,
    observation: &impl serde::Serialize,
    actions: &[crate::rl::ActionSemanticV1],
) -> Result<Packet> {
    let state = serde_json::to_value(observation).map_err(|e| Error::Projection(e.to_string()))?;
    let actions = actions
        .iter()
        .map(serde_json::to_value)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Projection(e.to_string()))?;
    normalize::encode(source, state, actions)
}

macro_rules! session {
    ($session:ident,$decision:ident,$snapshot:ident,$binding:ty,$into:ident,$input:ident,$validated_input:ident,$restore:ident,$source:ident) => {
        /// Owning source-specific session. No conversion to a predecessor model contract.
        pub struct $session {
            inner: FastActorSessionV1,
            owner: Arc<()>,
            generation: u64,
        }
        #[derive(Clone)]
        pub struct $snapshot {
            inner: FastActorSessionSnapshotV1,
            owner: Arc<()>,
        }
        /// Private authority is deliberately neither Debug nor Serialize.
        #[derive(Clone)]
        pub struct $decision {
            expected: FastActorDecisionV1,
            binding: $binding,
            packet: Packet,
            owner: Arc<()>,
            generation: u64,
        }
        impl $decision {
            pub fn packet(&self) -> &Packet {
                &self.packet
            }
        }
        impl $session {
            pub fn from_session(session: FastActorSessionV1) -> Result<Self> {
                Ok(Self {
                    inner: session
                        .$into()
                        .map_err(|e| Error::Projection(e.to_string()))?,
                    owner: Arc::new(()),
                    generation: 0,
                })
            }
            pub fn current_response(&self) -> FastActorResponseV1 {
                self.inner.current_response()
            }
            pub fn snapshot(&self) -> $snapshot {
                $snapshot {
                    inner: self.inner.snapshot_v1(),
                    owner: self.owner.clone(),
                }
            }
            pub fn restore(&mut self, snapshot: &$snapshot) -> Result<()> {
                if !Arc::ptr_eq(&self.owner, &snapshot.owner) {
                    return Err(Error::ForeignSnapshot);
                }
                let next = self.generation.checked_add(1).ok_or(Error::Range)?;
                self.inner
                    .$restore(&snapshot.inner)
                    .map_err(|_| Error::StaleBinding)?;
                self.generation = next;
                Ok(())
            }
            pub fn encode(&self, expected: FastActorDecisionV1) -> Result<$decision> {
                let (observation, actions, binding) = self
                    .inner
                    .$input(expected, expected.acting_player)
                    .map_err(|_| Error::StaleBinding)?;
                let packet = packet_from_current(Source::$source, &observation, &actions)?;
                Ok($decision {
                    expected,
                    binding,
                    packet,
                    owner: self.owner.clone(),
                    generation: self.generation,
                })
            }
            pub fn apply(
                &mut self,
                decision: &$decision,
                index: u32,
            ) -> Result<FastActorResponseV1> {
                if !Arc::ptr_eq(&self.owner, &decision.owner)
                    || self.generation != decision.generation
                {
                    return Err(Error::StaleBinding);
                }
                if index as usize >= decision.packet.actions.len() {
                    return Err(Error::ActionRange);
                }
                let (observation, actions) = self
                    .inner
                    .$validated_input(
                        decision.expected,
                        decision.expected.acting_player,
                        &decision.binding,
                    )
                    .map_err(|_| Error::StaleBinding)?;
                // Numeric packet equality is additional to, never a substitute for, full private authority.
                if packet_from_current(Source::$source, &observation, &actions)? != decision.packet
                {
                    return Err(Error::StaleBinding);
                }
                self.inner
                    .step(decision.expected.episode_id, decision.expected.step, index)
                    .map_err(|e| Error::Projection(format!("execution:{e:?}")))
            }
            pub fn score_and_apply(
                &mut self,
                decision: &$decision,
                model: &NativeModel,
            ) -> Result<(Vec<f32>, u32, FastActorResponseV1)> {
                let logits = model.forward(&decision.packet)?;
                if logits.len() != decision.packet.actions.len() {
                    return Err(Error::ModelContract);
                }
                let mut selected = 0;
                for index in 1..logits.len() {
                    if logits[index] > logits[selected] {
                        selected = index;
                    }
                }
                let selected = u32::try_from(selected).map_err(|_| Error::Range)?;
                let response = self.apply(decision, selected)?;
                Ok((logits, selected, response))
            }
        }
    };
}
session!(
    FakeSession,
    FakeDecision,
    FakeSnapshot,
    crate::rl_session::fake_v50::FakeDecisionBindingV50,
    into_fake_v50,
    fake_current_decision_input_v50,
    fake_validated_current_input_v50,
    fake_restore_v50,
    FakeV50
);
session!(
    RavenousSession,
    RavenousDecision,
    RavenousSnapshot,
    crate::rl_session::ravenous_v51::RavenousDecisionBindingV51,
    into_ravenous_v51,
    ravenous_current_decision_input_v51,
    ravenous_validated_current_input_v51,
    ravenous_restore_v51,
    RavenousV51
);

#[cfg(test)]
mod tests;

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod legendary_tests;

mod vivien;
pub use vivien::{VivienDecision, VivienSession, VivienSnapshot};

mod muldrotha;
pub use muldrotha::{MuldrothaDecision, MuldrothaSession, MuldrothaSnapshot};
mod tinybones;
pub use tinybones::{TinybonesDecision, TinybonesSession, TinybonesSnapshot};

mod liliana;
pub use liliana::{LilianaDecision, LilianaSession, LilianaSnapshot};

mod chandra;
pub use chandra::{ChandraDecision, ChandraSession, ChandraSnapshot};

mod kaito;
pub use kaito::{KaitoDecision, KaitoSession, KaitoSnapshot};
