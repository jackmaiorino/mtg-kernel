//! The pregame announcement step scale
//! (`loss_selection.pregame_announcement_step_scale`): the key, its bytes
//! when absent, its validation and the readers that refuse it. The step
//! itself is tested in `native_policy_train_step_v1::gae_v1::tests`, the run
//! in `native_expanded_training_run_v1`.

use super::*;

fn selection(pregame_advantage: PregameAdvantageV1, scale: Option<f32>) -> ExpandedLossSelectionV1 {
    ExpandedLossSelectionV1::GaeAdvantageValueV1 {
        gamma: 1.0,
        lambda: 0.9,
        entropy_coefficient: 0.0,
        pregame_advantage,
        pregame_announcement_step_scale: scale,
    }
}

#[test]
fn an_absent_scale_writes_no_byte_and_a_set_scale_is_part_of_the_selection() {
    let absent = selection(PregameAdvantageV1::TerminalV1, None);
    assert_eq!(
        serde_json::to_string(&absent).unwrap(),
        r#"{"kind":"gae_advantage_value_v1","gamma":1.0,"lambda":0.9,"entropy_coefficient":0.0,"pregame_advantage":"terminal_v1"}"#
    );
    assert_eq!(
        serde_json::to_string(&selection(PregameAdvantageV1::Gae, None)).unwrap(),
        r#"{"kind":"gae_advantage_value_v1","gamma":1.0,"lambda":0.9,"entropy_coefficient":0.0}"#
    );
    assert_eq!(absent.pregame_announcement_step_scale_v1(), None);
    // A file without the key reads as absent.
    let read: ExpandedLossSelectionV1 = serde_json::from_str(
        r#"{"kind":"gae_advantage_value_v1","gamma":1.0,"lambda":0.9,"entropy_coefficient":0.0,"pregame_advantage":"terminal_v1"}"#,
    )
    .unwrap();
    assert_eq!(read, absent);
    // Set: written, read back, and another selection than absent.
    let set = selection(PregameAdvantageV1::TerminalV1, Some(0.03));
    set.validate_v1().unwrap();
    assert_ne!(set, absent);
    assert_eq!(set.pregame_announcement_step_scale_v1(), Some(0.03));
    let written = serde_json::to_string(&set).unwrap();
    assert_eq!(
        written,
        r#"{"kind":"gae_advantage_value_v1","gamma":1.0,"lambda":0.9,"entropy_coefficient":0.0,"pregame_advantage":"terminal_v1","pregame_announcement_step_scale":0.03}"#
    );
    let read: ExpandedLossSelectionV1 = serde_json::from_str(&written).unwrap();
    assert_eq!(read, set);
    assert_ne!(read, selection(PregameAdvantageV1::TerminalV1, Some(0.003)));
    // The loss identity string is the GAE one either way.
    assert_eq!(set.loss_identity_v1(), absent.loss_identity_v1());
    // The other losses name no scale.
    assert_eq!(
        ExpandedLossSelectionV1::TerminalReinforceValueV3.pregame_announcement_step_scale_v1(),
        None
    );
}

#[test]
fn the_scale_is_refused_outside_the_open_unit_interval_and_without_the_terminal_mode() {
    for scale in [
        0.003f32,
        0.03,
        0.1,
        0.3,
        f32::MIN_POSITIVE,
        1.0 - f32::EPSILON / 2.0,
    ] {
        selection(PregameAdvantageV1::TerminalV1, Some(scale))
            .validate_v1()
            .unwrap_or_else(|error| panic!("{scale}: {error}"));
    }
    for scale in [
        0.0f32,
        -0.0,
        -0.1,
        1.0,
        1.5,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        let error = selection(PregameAdvantageV1::TerminalV1, Some(scale))
            .validate_v1()
            .unwrap_err();
        assert!(error.contains("(0, 1) exclusive"), "{scale}: {error}");
    }
    // Decimals that round to f32 zero or to f32 one are refused the same way.
    for text in ["1e-50", "0.99999999999"] {
        let read: ExpandedLossSelectionV1 = serde_json::from_str(&format!(
            r#"{{"kind":"gae_advantage_value_v1","gamma":1.0,"lambda":0.9,"entropy_coefficient":0.0,"pregame_advantage":"terminal_v1","pregame_announcement_step_scale":{text}}}"#
        ))
        .unwrap();
        assert!(
            read.pregame_announcement_step_scale_v1().is_some(),
            "{text}"
        );
        assert!(read.validate_v1().is_err(), "{text}");
    }
    // Only with the terminal pregame advantage.
    let error = selection(PregameAdvantageV1::Gae, Some(0.03))
        .validate_v1()
        .unwrap_err();
    assert!(
        error.contains("requires pregame_advantage terminal_v1"),
        "{error}"
    );
    selection(PregameAdvantageV1::Gae, None)
        .validate_v1()
        .unwrap();
}

/// The earlier, pre-normalisation key name is not a key of this trainer: a
/// selection that carries it (alone or beside the new key) does not parse,
/// so the two can never be combined or confused. The key on another struct
/// loss is refused as well.
#[test]
fn the_pre_normalisation_scale_key_and_the_key_on_another_loss_do_not_parse() {
    for extra in [
        r#""pregame_advantage_scale":0.003"#,
        r#""pregame_advantage_scale":0.003,"pregame_announcement_step_scale":0.03"#,
    ] {
        let error = serde_json::from_str::<ExpandedLossSelectionV1>(&format!(
            r#"{{"kind":"gae_advantage_value_v1","gamma":1.0,"lambda":0.9,"entropy_coefficient":0.0,"pregame_advantage":"terminal_v1",{extra}}}"#
        ))
        .unwrap_err()
        .to_string();
        assert!(error.contains("pregame_advantage_scale"), "{error}");
    }
    assert!(serde_json::from_str::<ExpandedLossSelectionV1>(
        r#"{"kind":"gae_uniform_kl_recovery_v1","gamma":1.0,"lambda":0.9,"initial_adam_step":0,"recovery":true,"diagnostics":false,"pregame_announcement_step_scale":0.03}"#
    )
    .is_err());
}
