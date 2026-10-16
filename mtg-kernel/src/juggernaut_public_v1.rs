//! Selected live combat-static transport. Frozen V8/V9 and models are unchanged.
use crate::ids::PlayerId;
use crate::policy_observation_v8::PolicyObservationExtensionsV8;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::{
    CardPrivateV1, CardStableRefV1, KnownLibraryCardV4, PlayerSeatV1,
    PublicObservationProjectionV5, RlContractError,
};
use crate::state::GameState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/juggernaut-public/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatRuleV1 {
    pub source: CardStableRefV1,
    pub current_controller: PlayerSeatV1,
    pub front_face_printed_must_attack_each_combat: bool,
    pub front_face_printed_blocked_subtype: crate::card_def::Subtype,
    pub printed_rules_active: bool,
    pub required_in_current_declaration: bool,
    pub wall_restriction_active: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootV1 {
    pub card_id: u16,
    pub ability_index: u16,
    pub root_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JuggernautObservationV1 {
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
    pub combat_rules: Vec<CombatRuleV1>,
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
    let phoenix_producer = crate::phoenix_public_v56::producer_sha256();
    for source in [
        SCHEMA,
        include_str!("juggernaut_public_v1.rs"),
        phoenix_producer.as_str(),
        include_str!("combat_static_rules_v1.rs"),
        include_str!("rules_meaning_juggernaut_v1.rs"),
        include_str!("engine.rs"),
        include_str!("policy_surface_v5.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("standard_statics_v1.rs"),
        include_str!("standard_keywords_v1.rs"),
        include_str!("standard_legends_v1.rs"),
        include_str!("creature_stats_lki_v1.rs"),
        include_str!("sower_of_chaos_v1.rs"),
        include_str!("sower_restriction_capture_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("state.rs"),
        include_str!("../build.rs"),
        include_str!("rl.rs"),
        include_str!("policy_observation_v8.rs"),
        include_str!("policy_observation_v7.rs"),
        include_str!("rl_session.rs"),
        include_str!("rl_session/juggernaut_v25.rs"),
        include_str!("rl_session/flat_action_v3.rs"),
        include_str!("rl_session/flat_action_v4.rs"),
        include_str!("rl_session/human_v8.rs"),
        include_str!("human_bo3_v1/juggernaut_v25.rs"),
        include_str!("human_bo3_v1/visible.rs"),
        include_str!("human_bo3_v1/labels.rs"),
        include_str!("human_bo3_v1/life_ward_v8.rs"),
        include_str!("bloodthirsty_conqueror_v49/custody.rs"),
        include_str!("linked_aura_operation_v1.rs"),
        include_str!("counter_target_effects_v1.rs"),
        include_str!("zimone_paradox_sculptor_v1.rs"),
        include_str!("planeswalker_v1.rs"),
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
        include_str!("mana.rs"),
        include_str!("legend_rule_v1.rs"),
        include_str!("doubling_season_v48.rs"),
        include_str!("event.rs"),
        include_str!("effect/thousand_year_storm_v1.rs"),
        include_str!("effect/standard_copy.rs"),
        include_str!("standard_spell_copy.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.update(b"legacy-graveyard-return-guard/v61\0");
    hash.update(crate::graveyard_return_public_v61::producer_sha256().as_bytes());
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn commitment(observation: &JuggernautObservationV1) -> Result<String, RlContractError> {
    let mut value = serde_json::to_value(observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("Juggernaut projection is not an object".into()))?
        .remove("projection_sha256");
    Ok(digest(&serde_json::to_vec(&value)?))
}
impl JuggernautObservationV1 {
    /// Checks transport identity only. Execution always rederives the entire
    /// observation and private ordered candidate binding from the owning session.
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.producer_sha256 != producer_sha256_v1()
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError("wrong Juggernaut artifact identity".into()));
        }
        let expected = roots()?;
        if self.roots != expected {
            return Err(RlContractError("wrong Juggernaut printed root".into()));
        }
        Ok(())
    }
}
fn roots() -> Result<Vec<RootV1>, RlContractError> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Ok(Vec::new());
    }
    let Some(id) = crate::card_def::card_id_by_name("Juggernaut") else {
        return Ok(Vec::new());
    };
    let selector = crate::rules_meaning_juggernaut_v1::live_selector()
        .map_err(|_| RlContractError("Juggernaut catalog not supported".into()))?;
    [
        crate::rules_meaning_juggernaut_v1::RootV1::MustAttackEachCombat,
        crate::rules_meaning_juggernaut_v1::RootV1::CannotBeBlockedByWalls,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, root)| {
        let meaning = crate::rules_meaning_juggernaut_v1::meaning(&selector, id, root)
            .map_err(|_| RlContractError("Juggernaut root not supported".into()))?;
        Ok(RootV1 {
            card_id: id,
            ability_index: index as u16,
            root_sha256: meaning.root_sha256,
        })
    })
    .collect()
}
/// Authenticates selected roots and the independent event-time capture lifecycle.
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    roots().map_err(|error| error.0)?;
    crate::effect::validate_pending_effect_choice(state)?;
    for pending in &state.engine.pending_triggers {
        crate::engine::validate_pending_trigger_identity(state, pending)?;
        if pending.placement_ordered {
            crate::engine::validate_pending_trigger(state, pending)?;
        }
    }
    for item in &state.stack {
        crate::engine::validated_stack_item_target_spec(item, state)?;
    }
    Ok(())
}
fn combat_rows(state: &GameState) -> Result<Vec<CombatRuleV1>, RlContractError> {
    combat_rows_for_scope(state, crate::rl::CommonProjectionScopeV42::ExistingV40)
}
fn combat_rows_for_scope(
    state: &GameState,
    scope: crate::rl::CommonProjectionScopeV42,
) -> Result<Vec<CombatRuleV1>, RlContractError> {
    let mut rows = Vec::new();
    // This fresh value is local to the same immutable state. Only Prepared's
    // mandatory precise Phoenix family can represent a must-attack-only rule;
    // the frozen Juggernaut row always requires its blocked subtype as well.
    let mut phoenix = None;
    for player in &state.players {
        for source in &player.battlefield {
            // The existing public projector determines definition visibility.
            // A plain/disguised face-down source reveals only its neutral sentinel.
            let reference = crate::rl::card_ref(state, *source)?;
            let def = &crate::card_def::CARD_DEFS[usize::from(reference.card_db_id)];
            if def.combat_static_rules == crate::card_def::CombatStaticRulesDef::NONE {
                continue;
            }
            if scope == crate::rl::CommonProjectionScopeV42::PreparedV57
                && crate::rules_meaning_phoenix_v56::selected_definition(def, reference.card_db_id)
            {
                if phoenix.is_none() {
                    phoenix = Some(crate::phoenix_public_v56::capture(state)?);
                }
                let represented = phoenix.as_ref().is_some_and(|facts| {
                    facts.meaning.card_id == reference.card_db_id
                        && facts.current.iter().any(|row| {
                            row.source.object == reference.arena_id
                                && row.source.generation == reference.zone_change_count
                                && row.source.owner == reference.owner
                                && row.source.controller == reference.controller
                                && row.source.zone == reference.zone
                                && row.source.identity
                                    == crate::phoenix_public_v56::IdentityV56::Known {
                                        card_id: reference.card_db_id,
                                    }
                        })
                });
                if !represented {
                    return Err(RlContractError(
                        "Prepared Phoenix combat rule lacks its exact precise current row".into(),
                    ));
                }
                continue;
            }
            let subtype = def
                .combat_static_rules
                .cant_be_blocked_by_subtype
                .ok_or_else(|| RlContractError("unsupported combat descriptor".into()))?;
            let selector = crate::rules_meaning_juggernaut_v1::live_selector()
                .map_err(|_| RlContractError("unsupported combat catalog".into()))?;
            crate::rules_meaning_juggernaut_v1::meaning(
                &selector,
                reference.card_db_id,
                crate::rules_meaning_juggernaut_v1::RootV1::MustAttackEachCombat,
            )
            .map_err(|_| RlContractError("unsupported combat root".into()))?;
            let active = crate::combat_static_rules_v1::active(state, *source);
            rows.push(CombatRuleV1 {
                current_controller: reference.controller,
                source: reference,
                front_face_printed_must_attack_each_combat: def
                    .combat_static_rules
                    .must_attack_each_combat,
                front_face_printed_blocked_subtype: subtype,
                printed_rules_active: active != crate::card_def::CombatStaticRulesDef::NONE,
                required_in_current_declaration: state.step == crate::state::Step::DeclareAttackers
                    && !state.engine.combat.attackers_declared
                    && state.objects.get(*source).controller == state.active_player
                    && crate::engine::can_attack(state, *source)
                    && active.must_attack_each_combat,
                wall_restriction_active: active.cant_be_blocked_by_subtype.is_some(),
            });
        }
    }
    Ok(rows)
}
// V40 consumes these private typed parts; standalone predecessors retain their refusal path.
pub(crate) fn parts_for_akroma_v40(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
) -> Result<JuggernautObservationV1, RlContractError> {
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
) -> Result<JuggernautObservationV1, RlContractError> {
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
) -> Result<JuggernautObservationV1, RlContractError> {
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
pub(crate) fn parts_for_common_scope(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
    scope: crate::rl::CommonProjectionScopeV42,
) -> Result<JuggernautObservationV1, RlContractError> {
    parts_for_common_scope_with_common(
        (scope, state),
        surface,
        actor,
        step_index,
        physical_decision_id,
        substep,
        None,
    )
}
pub(crate) fn parts_for_common_scope_with_common(
    request: (crate::rl::CommonProjectionScopeV42, &GameState),
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
    common: Option<&crate::rl::BorrowedCommonObservationV42<'_>>,
) -> Result<JuggernautObservationV1, RlContractError> {
    let (scope, state) = request;
    let (substep_index, substep_count) = substep;

    validate_state(state).map_err(RlContractError)?;
    let combat_rules = combat_rows_for_scope(state, scope)?;
    let base = crate::rl::common_family_observation_storage_from_borrowed(
        (scope, state),
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
        common,
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let base = storage.base;
    let observation = JuggernautObservationV1 {
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
        combat_rules,
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
) -> Result<JuggernautObservationV1, RlContractError> {
    validate_state(state).map_err(RlContractError)?;
    let combat_rules = combat_rows(state)?;
    let base = crate::rl::juggernaut_observation_storage_v1(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let base = storage.base;
    let mut observation = JuggernautObservationV1 {
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
        combat_rules,
        roots: roots()?,
        projection_sha256: String::new(),
    };
    observation.projection_sha256 = commitment(&observation)?;
    Ok(observation)
}
