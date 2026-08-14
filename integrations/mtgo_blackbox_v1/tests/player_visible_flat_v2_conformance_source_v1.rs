use mtg_kernel::card_def::card_id_by_name;
use mtgo_blackbox_v1::{
    MtgoPlayerRelativeRoleV1, MtgoPlayerVisibleDuelActionV1, MtgoPlayerVisibleDuelDecisionInputV1,
};
use serde::Deserialize;
use serde_json::Value;

const FIXTURE_JSON: &str =
    include_str!("../fixtures/player_visible_flat_v2_conformance_source_v1.json");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConformanceFixtureV1 {
    schema_version: u32,
    fixture_id: String,
    model_input: MtgoPlayerVisibleDuelDecisionInputV1,
    expected_input_commitment_sha256: String,
    synthetic_simulator_setup: SyntheticSimulatorSetupV1,
    expected_flat_v2: ExpectedFlatV2,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SyntheticSimulatorSetupV1 {
    control_changed_card_name: String,
    physical_owner: String,
    rendered_controller_partition: String,
    model_input_contains_physical_owner: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedFlatV2 {
    globals: ExpectedGlobalsV2,
    object_rows: Vec<ExpectedObjectRowV2>,
    action_rows: Vec<ExpectedActionRowV2>,
    expected_empty_slices: Vec<String>,
    control_changed_battlefield_owner_difference_class: String,
    allowed_parity_difference_classes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedGlobalsV2 {
    acting_player: String,
    phase: String,
    active_player: String,
    priority_player: String,
    initiative: String,
    stack_nonempty: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedObjectRowV2 {
    model_object_index: u32,
    visible_ordinal: u32,
    card_name: String,
    group: String,
    owner: String,
    controller: String,
    zone: String,
    card_token_source: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedActionRowV2 {
    action_index: u32,
    kind: String,
    source_visible_ordinal: Option<u32>,
    source_model_object_index: Option<u32>,
}

fn assert_no_forbidden_model_keys(value: &Value) {
    match value {
        Value::Object(fields) => {
            for (key, child) in fields {
                assert!(
                    !matches!(
                        key.as_str(),
                        "physical_owner"
                            | "arena_id"
                            | "card_db_id"
                            | "zone_change_count"
                            | "adapter_object_id"
                            | "client_object_id"
                            | "engine_context"
                            | "surface_context"
                            | "policy_surface_context"
                    ),
                    "forbidden model-input field leaked: {key}"
                );
                assert_no_forbidden_model_keys(child);
            }
        }
        Value::Array(values) => values.iter().for_each(assert_no_forbidden_model_keys),
        _ => {}
    }
}

#[test]
fn canonical_source_fixture_is_exact_sanitized_model_input() {
    let fixture: ConformanceFixtureV1 = serde_json::from_str(FIXTURE_JSON).unwrap();
    assert_eq!(fixture.schema_version, 1);
    assert_eq!(
        fixture.fixture_id,
        "player-visible-flat-v2-common-facts-control-change-v1"
    );
    assert_eq!(
        fixture.model_input.commitment_sha256_v1().unwrap(),
        fixture.expected_input_commitment_sha256
    );
    assert_eq!(
        fixture.model_input.current_state.acting_player,
        MtgoPlayerRelativeRoleV1::SeatedPlayer
    );
    assert_eq!(fixture.model_input.ordered_legal_actions.len(), 2);
    assert!(matches!(
        fixture.model_input.ordered_legal_actions[0],
        MtgoPlayerVisibleDuelActionV1::Pass {
            actor: MtgoPlayerRelativeRoleV1::SeatedPlayer
        }
    ));
    assert!(matches!(
        fixture.model_input.ordered_legal_actions[1],
        MtgoPlayerVisibleDuelActionV1::CastSpell {
            actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            source
        } if source.visible_ordinal == 1
    ));

    let model_input_json = serde_json::to_value(&fixture.model_input).unwrap();
    assert_no_forbidden_model_keys(&model_input_json);
    assert!(card_id_by_name("Mountain").is_some());
    assert!(card_id_by_name("Lightning Bolt").is_some());
}

#[test]
fn object_and_action_order_are_pinned_for_future_flat_v2_encoder() {
    let fixture: ConformanceFixtureV1 = serde_json::from_str(FIXTURE_JSON).unwrap();
    let state = &fixture.model_input.current_state;
    assert_eq!(state.own_hand.len(), 1);
    assert_eq!(state.own_hand[0].card_name, "Lightning Bolt");
    assert_eq!(state.own_hand[0].object_ref.visible_ordinal, 1);
    assert_eq!(state.battlefield[0].len(), 1);
    assert_eq!(state.battlefield[0][0].card_name, "Mountain");
    assert_eq!(state.battlefield[0][0].object_ref.visible_ordinal, 0);
    assert!(state.battlefield[1].is_empty());
    assert!(state.graveyards.iter().all(Vec::is_empty));
    assert!(state.exile.is_empty());
    assert!(state.stack.is_empty());
    assert!(state.visible_object_relations.is_empty());
    assert!(state.known_library_cards.iter().all(Vec::is_empty));
    assert!(state.known_hand_cards.iter().all(Vec::is_empty));

    let rows = &fixture.expected_flat_v2.object_rows;
    assert_eq!(rows.len(), 2);
    assert_eq!(
        (
            rows[0].model_object_index,
            rows[0].visible_ordinal,
            rows[0].card_name.as_str(),
            rows[0].group.as_str(),
            rows[0].owner.as_str(),
            rows[0].controller.as_str(),
            rows[0].zone.as_str(),
            rows[0].card_token_source.as_str(),
        ),
        (
            0,
            1,
            "Lightning Bolt",
            "self_hand",
            "self_player",
            "none",
            "hand",
            "exact_case_sensitive_card_name_catalog_lookup",
        )
    );
    assert_eq!(
        (
            rows[1].model_object_index,
            rows[1].visible_ordinal,
            rows[1].card_name.as_str(),
            rows[1].group.as_str(),
            rows[1].owner.as_str(),
            rows[1].controller.as_str(),
            rows[1].zone.as_str(),
            rows[1].card_token_source.as_str(),
        ),
        (
            1,
            0,
            "Mountain",
            "self_battlefield",
            "none",
            "self_player",
            "battlefield",
            "exact_case_sensitive_card_name_catalog_lookup",
        )
    );

    let actions = &fixture.expected_flat_v2.action_rows;
    assert_eq!(actions.len(), 2);
    assert_eq!(
        (
            actions[0].action_index,
            actions[0].kind.as_str(),
            actions[0].source_visible_ordinal,
            actions[0].source_model_object_index,
        ),
        (0, "pass", None, None)
    );
    assert_eq!(
        (
            actions[1].action_index,
            actions[1].kind.as_str(),
            actions[1].source_visible_ordinal,
            actions[1].source_model_object_index,
        ),
        (1, "cast_spell", Some(1), Some(0))
    );
}

#[test]
fn control_changed_permanent_uses_rendered_partition_and_neutral_owner() {
    let fixture: ConformanceFixtureV1 = serde_json::from_str(FIXTURE_JSON).unwrap();
    let setup = &fixture.synthetic_simulator_setup;
    assert_eq!(setup.control_changed_card_name, "Mountain");
    assert_eq!(setup.physical_owner, "opponent");
    assert_eq!(setup.rendered_controller_partition, "seated_player");
    assert!(!setup.model_input_contains_physical_owner);

    let model_input_json = serde_json::to_value(&fixture.model_input).unwrap();
    let visible_battlefield_card = model_input_json
        .pointer("/current_state/battlefield/0/0")
        .and_then(Value::as_object)
        .unwrap();
    assert!(!visible_battlefield_card.contains_key("owner"));
    assert!(!visible_battlefield_card.contains_key("controller"));

    let battlefield_row = &fixture.expected_flat_v2.object_rows[1];
    assert_eq!(battlefield_row.card_name, setup.control_changed_card_name);
    assert_eq!(battlefield_row.owner, "none");
    assert_eq!(battlefield_row.controller, "self_player");
    assert_eq!(
        fixture
            .expected_flat_v2
            .control_changed_battlefield_owner_difference_class,
        "fixed_neutral_unavailable_value"
    );
}

#[test]
fn parity_classification_vocabulary_and_neutral_slices_are_frozen() {
    let fixture: ConformanceFixtureV1 = serde_json::from_str(FIXTURE_JSON).unwrap();
    assert_eq!(
        fixture.expected_flat_v2.allowed_parity_difference_classes,
        [
            "player_visible_value_preserved",
            "printed_catalog_compatibility_value",
            "fixed_neutral_unavailable_value",
            "deliberate_decision_local_identity_replacement",
        ]
    );
    assert_eq!(
        fixture.expected_flat_v2.expected_empty_slices,
        [
            "relations",
            "ability_uses",
            "goads",
            "completed_dungeons",
            "effect_subtype_changes",
            "context_path_elements",
        ]
    );

    let globals = &fixture.expected_flat_v2.globals;
    assert_eq!(globals.acting_player, "self_player");
    assert_eq!(globals.phase, "main1");
    assert_eq!(globals.active_player, "self_player");
    assert_eq!(globals.priority_player, "self_player");
    assert_eq!(globals.initiative, "none");
    assert!(!globals.stack_nonempty);
}
