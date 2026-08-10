use mtg_kernel::mana::ManaColor;
use mtg_kernel::rl::PlayerSeatV1;
use mtgo_blackbox_v1::{
    validate_visible_object_action_calibration_trace_v1, MtgoGameplayVisibleChangeV1,
    MtgoVisibleObjectActionCalibrationTraceV1, MtgoVisibleObjectCalibrationActionV1,
};

fn fixture() -> MtgoVisibleObjectActionCalibrationTraceV1 {
    serde_json::from_str(include_str!(
        "../fixtures/solitaire_play_land_transition_v1.json"
    ))
    .expect("checked-in visible object action trace must parse")
}

fn mana_fixture() -> MtgoVisibleObjectActionCalibrationTraceV1 {
    serde_json::from_str(include_str!(
        "../fixtures/solitaire_activate_island_mana_transition_v1.json"
    ))
    .expect("checked-in mana ability trace must parse")
}

#[test]
fn live_solitaire_play_land_transition_is_structurally_checked_only() {
    let checked = validate_visible_object_action_calibration_trace_v1(fixture()).unwrap();
    assert_eq!(
        checked.action(),
        &MtgoVisibleObjectCalibrationActionV1::PlayLand {
            actor: PlayerSeatV1::P0,
            source_adapter_object_id: "before-frame:hand-slot-0".to_string(),
            visible_card_name: "Island".to_string(),
        }
    );
    assert_eq!(checked.transition_commitment_sha256().len(), 64);
}

#[test]
fn non_local_actor_and_invalid_source_labels_fail_closed() {
    let mut actor = fixture();
    actor.action = MtgoVisibleObjectCalibrationActionV1::PlayLand {
        actor: PlayerSeatV1::P1,
        source_adapter_object_id: "before-frame:hand-slot-0".to_string(),
        visible_card_name: "Island".to_string(),
    };
    assert_eq!(
        validate_visible_object_action_calibration_trace_v1(actor)
            .err()
            .expect("non-local action must fail")
            .code(),
        "visible_object_action_trace_action_unsupported"
    );

    let mut object = fixture();
    let MtgoVisibleObjectCalibrationActionV1::PlayLand {
        source_adapter_object_id,
        ..
    } = &mut object.action
    else {
        panic!("fixture is PlayLand")
    };
    *source_adapter_object_id = "../../not-an-object".to_string();
    assert_eq!(
        validate_visible_object_action_calibration_trace_v1(object)
            .err()
            .expect("invalid adapter object ID must fail")
            .code(),
        "visible_object_action_trace_source_object_id_invalid"
    );
}

#[test]
fn every_play_land_visible_postcondition_is_required_and_changed() {
    for required in [
        MtgoGameplayVisibleChangeV1::PromptChanged,
        MtgoGameplayVisibleChangeV1::PlayerCountsChanged,
        MtgoGameplayVisibleChangeV1::BattlefieldChanged,
        MtgoGameplayVisibleChangeV1::HandChanged,
        MtgoGameplayVisibleChangeV1::VisibleGameLogChanged,
    ] {
        let mut missing = fixture();
        let item = missing
            .visible_postconditions
            .iter_mut()
            .find(|item| item.change == required)
            .expect("fixture contains required change");
        item.change = MtgoGameplayVisibleChangeV1::PhaseBarChanged;
        assert_eq!(
            validate_visible_object_action_calibration_trace_v1(missing)
                .err()
                .expect("missing visible postcondition must fail")
                .code(),
            "visible_object_action_trace_required_postcondition_missing"
        );
    }

    let mut unchanged = fixture();
    unchanged.visible_postconditions[2].after_bgra8_sha256 = unchanged.visible_postconditions[2]
        .before_bgra8_sha256
        .clone();
    assert_eq!(
        validate_visible_object_action_calibration_trace_v1(unchanged)
            .err()
            .expect("unchanged battlefield must fail")
            .code(),
        "visible_object_action_trace_postcondition_unchanged"
    );
}

#[test]
fn play_land_action_json_rejects_coordinates_and_authority_claims() {
    let source = include_str!("../fixtures/solitaire_play_land_transition_v1.json");
    let altered = source.replacen(
        "\"visible_card_name\": \"Island\"",
        "\"visible_card_name\": \"Island\", \"x\": 380, \"y\": 790",
        1,
    );
    assert!(serde_json::from_str::<MtgoVisibleObjectActionCalibrationTraceV1>(&altered).is_err());

    let mut authority = fixture();
    authority.after_frame.safe_for_input = true;
    assert_eq!(
        validate_visible_object_action_calibration_trace_v1(authority)
            .err()
            .expect("preview authority claim must fail")
            .code(),
        "visible_object_action_trace_preview_claims_authority"
    );
}

#[test]
fn live_solitaire_island_mana_transition_is_structurally_checked_only() {
    let checked = validate_visible_object_action_calibration_trace_v1(mana_fixture()).unwrap();
    assert_eq!(
        checked.action(),
        &MtgoVisibleObjectCalibrationActionV1::ActivateManaAbility {
            actor: PlayerSeatV1::P0,
            source_adapter_object_id: "before-frame:battlefield-object-0".to_string(),
            visible_card_name: "Island".to_string(),
            mana_choice: None,
            visible_mana_added: ManaColor::U,
        }
    );
    assert_eq!(checked.transition_commitment_sha256().len(), 64);
}

#[test]
fn mana_activation_requires_tapped_object_and_visible_pool_change() {
    for required in [
        MtgoGameplayVisibleChangeV1::BattlefieldChanged,
        MtgoGameplayVisibleChangeV1::ManaPoolChanged,
    ] {
        let mut missing = mana_fixture();
        let item = missing
            .visible_postconditions
            .iter_mut()
            .find(|item| item.change == required)
            .expect("fixture contains required mana change");
        item.change = MtgoGameplayVisibleChangeV1::PhaseBarChanged;
        assert_eq!(
            validate_visible_object_action_calibration_trace_v1(missing)
                .err()
                .expect("missing mana postcondition must fail")
                .code(),
            "visible_object_action_trace_required_postcondition_missing"
        );
    }

    let mut actor = mana_fixture();
    actor.action = MtgoVisibleObjectCalibrationActionV1::ActivateManaAbility {
        actor: PlayerSeatV1::P1,
        source_adapter_object_id: "before-frame:battlefield-object-0".to_string(),
        visible_card_name: "Island".to_string(),
        mana_choice: None,
        visible_mana_added: ManaColor::U,
    };
    assert_eq!(
        validate_visible_object_action_calibration_trace_v1(actor)
            .err()
            .expect("non-local mana activation must fail")
            .code(),
        "visible_object_action_trace_action_unsupported"
    );

    let mut mismatch = mana_fixture();
    let MtgoVisibleObjectCalibrationActionV1::ActivateManaAbility {
        mana_choice,
        visible_mana_added,
        ..
    } = &mut mismatch.action
    else {
        panic!("fixture is ActivateManaAbility")
    };
    *mana_choice = Some(ManaColor::R);
    *visible_mana_added = ManaColor::U;
    assert_eq!(
        validate_visible_object_action_calibration_trace_v1(mismatch)
            .err()
            .expect("explicit mana choice mismatch must fail")
            .code(),
        "visible_object_action_trace_mana_choice_mismatch"
    );
}
