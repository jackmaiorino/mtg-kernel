use mtgo_blackbox_v1::*;

fn digest(byte: char) -> String {
    byte.to_string().repeat(64)
}

fn required_fact_kinds(
    phase: MtgoCompetitiveLifecyclePhaseV1,
) -> Vec<MtgoLifecycleVisibleFactKindV1> {
    use MtgoLifecycleVisibleFactKindV1::*;
    match phase {
        MtgoCompetitiveLifecyclePhaseV1::EventBrowser => vec![EventBrowserVisible],
        MtgoCompetitiveLifecyclePhaseV1::EntryReview => {
            vec![EntryReviewVisible, EntryTermsVisible]
        }
        MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing => vec![EnteredEventVisible],
        MtgoCompetitiveLifecyclePhaseV1::PairingReady => {
            vec![PairingVisible, PairingAcceptControlEnabled]
        }
        MtgoCompetitiveLifecyclePhaseV1::MatchInProgress => {
            vec![MatchSurfaceVisible, LocalClockVisible, OpponentClockVisible]
        }
        MtgoCompetitiveLifecyclePhaseV1::Sideboarding => vec![
            SideboardSurfaceVisible,
            SideboardTimerVisible,
            SideboardNoChangesConfirmed,
            SideboardSubmitControlEnabled,
        ],
        MtgoCompetitiveLifecyclePhaseV1::MatchComplete => {
            vec![MatchResultVisible, MatchContinueControlEnabled]
        }
        MtgoCompetitiveLifecyclePhaseV1::EventComplete => {
            vec![EventResultVisible, EventCloseControlEnabled]
        }
        MtgoCompetitiveLifecyclePhaseV1::Reconnect => {
            vec![ReconnectVisible, ReconnectResumeControlEnabled]
        }
    }
}

fn snapshot(
    phase: MtgoCompetitiveLifecyclePhaseV1,
    sequence: u64,
) -> MtgoVisibleCompetitiveLifecycleSnapshotV1 {
    let has_event = phase != MtgoCompetitiveLifecyclePhaseV1::EventBrowser;
    let has_match = matches!(
        phase,
        MtgoCompetitiveLifecyclePhaseV1::PairingReady
            | MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
            | MtgoCompetitiveLifecyclePhaseV1::Sideboarding
            | MtgoCompetitiveLifecyclePhaseV1::MatchComplete
            | MtgoCompetitiveLifecyclePhaseV1::Reconnect
    );
    let has_game = matches!(
        phase,
        MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
            | MtgoCompetitiveLifecyclePhaseV1::Sideboarding
            | MtgoCompetitiveLifecyclePhaseV1::Reconnect
    );
    let entry_terms = (phase == MtgoCompetitiveLifecyclePhaseV1::EntryReview).then(|| {
        MtgoCompetitiveEntryTermsV1 {
            terms_sha256: digest('4'),
            resource: MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
            amount: 100,
        }
    });
    let facts = required_fact_kinds(phase)
        .into_iter()
        .enumerate()
        .map(|(index, kind)| MtgoLifecycleVisibleFactV1 {
            kind,
            rect_client_px: MtgoRectPxV1 {
                x: 10 + (index as u32 * 30),
                y: 10,
                width: 20,
                height: 20,
            },
            content_sha256: digest('5'),
            confidence_bps: 10_000,
        })
        .collect();
    MtgoVisibleCompetitiveLifecycleSnapshotV1 {
        schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
        snapshot_id: format!("lifecycle-{sequence}"),
        event_kind: MtgoCompetitiveEventKindV1::League,
        phase,
        frame_id: sequence,
        frame_sequence: sequence,
        frame_sha256: format!("{sequence:064x}"),
        client_bounds: MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: 1_240,
            height: 740,
        },
        event_identity_sha256: has_event.then(|| digest('6')),
        match_identity_sha256: has_match.then(|| digest('7')),
        game_number: has_game.then_some(1),
        entry_terms,
        visible_state_complete: true,
        facts,
    }
}

fn mode_authorization() -> MtgoAuthorizationScopeV1 {
    MtgoAuthorizationScopeV1 {
        schema_version: MTGO_AUTHORIZATION_SCHEMA_V1,
        account_alias_sha256: digest('1'),
        written_permission_sha256: digest('2'),
        visible_channels_only: true,
        league_input: true,
        challenge_input: false,
        ..MtgoAuthorizationScopeV1::default()
    }
}

fn entry_authorization() -> MtgoCompetitiveEntryAuthorizationV1 {
    MtgoCompetitiveEntryAuthorizationV1 {
        schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
        account_alias_sha256: digest('1'),
        written_permission_sha256: digest('2'),
        event_kind: MtgoCompetitiveEventKindV1::League,
        event_identity_sha256: digest('6'),
        entry_terms: MtgoCompetitiveEntryTermsV1 {
            terms_sha256: digest('4'),
            resource: MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
            amount: 100,
        },
        exact_entry_authorized: true,
        existing_account_resources_only: true,
    }
}

fn action_transition(
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    action: MtgoCompetitiveLifecycleActionV1,
    next: MtgoVisibleCompetitiveLifecycleSnapshotV1,
    entry: Option<&MtgoCompetitiveEntryAuthorizationV1>,
) -> CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1 {
    let intent =
        make_offline_competitive_lifecycle_intent_v1(source, action, &mode_authorization(), entry)
            .unwrap();
    let next_commitment = validate_visible_competitive_lifecycle_snapshot_v1(next.clone())
        .unwrap()
        .snapshot_commitment_sha256()
        .to_owned();
    let transition = validate_competitive_lifecycle_action_transition_v1(
        source,
        &intent,
        &mode_authorization(),
        entry,
        next.clone(),
    )
    .unwrap();
    assert_eq!(
        transition.source_snapshot_commitment_sha256(),
        source.snapshot_commitment_sha256()
    );
    assert_eq!(
        transition.next_snapshot_commitment_sha256(),
        next_commitment
    );
    assert!(!transition.safe_for_live_input());
    validate_visible_competitive_lifecycle_snapshot_v1(next).unwrap()
}

fn observed_transition(
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    event: MtgoObservedCompetitiveLifecycleAdvanceV1,
    next: MtgoVisibleCompetitiveLifecycleSnapshotV1,
) -> CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1 {
    validate_observed_competitive_lifecycle_advance_v1(source, event, next.clone()).unwrap();
    validate_visible_competitive_lifecycle_snapshot_v1(next).unwrap()
}

#[test]
fn complete_league_lifecycle_is_structurally_composable() {
    let browser = validate_visible_competitive_lifecycle_snapshot_v1(snapshot(
        MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
        1,
    ))
    .unwrap();
    let entry = action_transition(
        &browser,
        MtgoCompetitiveLifecycleActionV1::OpenEntryReview,
        snapshot(MtgoCompetitiveLifecyclePhaseV1::EntryReview, 2),
        None,
    );
    let entry_auth = entry_authorization();
    let waiting = action_transition(
        &entry,
        MtgoCompetitiveLifecycleActionV1::ConfirmEntry,
        snapshot(MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing, 3),
        Some(&entry_auth),
    );
    let pairing = observed_transition(
        &waiting,
        MtgoObservedCompetitiveLifecycleAdvanceV1::PairingPosted,
        snapshot(MtgoCompetitiveLifecyclePhaseV1::PairingReady, 4),
    );
    let game_one = action_transition(
        &pairing,
        MtgoCompetitiveLifecycleActionV1::AcceptPairing,
        snapshot(MtgoCompetitiveLifecyclePhaseV1::MatchInProgress, 5),
        None,
    );
    let sideboard = observed_transition(
        &game_one,
        MtgoObservedCompetitiveLifecycleAdvanceV1::GameEndedForSideboarding,
        snapshot(MtgoCompetitiveLifecyclePhaseV1::Sideboarding, 6),
    );
    let mut game_two_snapshot = snapshot(MtgoCompetitiveLifecyclePhaseV1::MatchInProgress, 7);
    game_two_snapshot.game_number = Some(2);
    let game_two = action_transition(
        &sideboard,
        MtgoCompetitiveLifecycleActionV1::SubmitSideboard,
        game_two_snapshot,
        None,
    );
    let match_complete = observed_transition(
        &game_two,
        MtgoObservedCompetitiveLifecycleAdvanceV1::MatchEnded,
        snapshot(MtgoCompetitiveLifecyclePhaseV1::MatchComplete, 8),
    );
    let waiting_again = action_transition(
        &match_complete,
        MtgoCompetitiveLifecycleActionV1::ContinueAfterMatch,
        snapshot(MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing, 9),
        None,
    );
    let event_complete = observed_transition(
        &waiting_again,
        MtgoObservedCompetitiveLifecycleAdvanceV1::EventEnded,
        snapshot(MtgoCompetitiveLifecyclePhaseV1::EventComplete, 10),
    );
    let browser_again = action_transition(
        &event_complete,
        MtgoCompetitiveLifecycleActionV1::CloseCompletedEvent,
        snapshot(MtgoCompetitiveLifecyclePhaseV1::EventBrowser, 11),
        None,
    );
    assert_eq!(
        browser_again.phase(),
        MtgoCompetitiveLifecyclePhaseV1::EventBrowser
    );
}

#[test]
fn league_and_challenge_authorization_are_not_interchangeable() {
    let source = validate_visible_competitive_lifecycle_snapshot_v1(snapshot(
        MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
        1,
    ))
    .unwrap();
    let mut scope = mode_authorization();
    scope.league_input = false;
    scope.challenge_input = true;
    assert_eq!(
        make_offline_competitive_lifecycle_intent_v1(
            &source,
            MtgoCompetitiveLifecycleActionV1::OpenEntryReview,
            &scope,
            None,
        )
        .unwrap_err()
        .code(),
        "mode_not_authorized"
    );

    let mut challenge = snapshot(MtgoCompetitiveLifecyclePhaseV1::EventBrowser, 2);
    challenge.event_kind = MtgoCompetitiveEventKindV1::Challenge;
    let challenge = validate_visible_competitive_lifecycle_snapshot_v1(challenge).unwrap();
    assert!(make_offline_competitive_lifecycle_intent_v1(
        &challenge,
        MtgoCompetitiveLifecycleActionV1::OpenEntryReview,
        &scope,
        None,
    )
    .is_ok());
}

#[test]
fn entry_confirmation_requires_exact_separate_resource_authorization() {
    let entry = validate_visible_competitive_lifecycle_snapshot_v1(snapshot(
        MtgoCompetitiveLifecyclePhaseV1::EntryReview,
        1,
    ))
    .unwrap();
    assert_eq!(
        make_offline_competitive_lifecycle_intent_v1(
            &entry,
            MtgoCompetitiveLifecycleActionV1::ConfirmEntry,
            &mode_authorization(),
            None,
        )
        .unwrap_err()
        .code(),
        "entry_authorization_missing"
    );
    for mutation in 0..5 {
        let mut authorization = entry_authorization();
        match mutation {
            0 => authorization.exact_entry_authorized = false,
            1 => authorization.existing_account_resources_only = false,
            2 => authorization.account_alias_sha256 = digest('8'),
            3 => authorization.entry_terms.amount = 120,
            4 => authorization.event_identity_sha256 = digest('9'),
            _ => unreachable!(),
        }
        assert!(make_offline_competitive_lifecycle_intent_v1(
            &entry,
            MtgoCompetitiveLifecycleActionV1::ConfirmEntry,
            &mode_authorization(),
            Some(&authorization),
        )
        .is_err());
    }
    assert!(make_offline_competitive_lifecycle_intent_v1(
        &entry,
        MtgoCompetitiveLifecycleActionV1::ConfirmEntry,
        &mode_authorization(),
        Some(&entry_authorization()),
    )
    .is_ok());

    let exact_entry = entry_authorization();
    let mut forged_intent = make_offline_competitive_lifecycle_intent_v1(
        &entry,
        MtgoCompetitiveLifecycleActionV1::ConfirmEntry,
        &mode_authorization(),
        Some(&exact_entry),
    )
    .unwrap();
    forged_intent.entry_authorization_sha256 = Some(digest('8'));
    assert!(validate_competitive_lifecycle_action_transition_v1(
        &entry,
        &forged_intent,
        &mode_authorization(),
        Some(&exact_entry),
        snapshot(MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing, 2,),
    )
    .is_err());
}

#[test]
fn visible_state_completeness_facts_confidence_and_geometry_fail_closed() {
    let base = snapshot(MtgoCompetitiveLifecyclePhaseV1::MatchInProgress, 1);
    for mutation in 0..6 {
        let mut candidate = base.clone();
        match mutation {
            0 => candidate.visible_state_complete = false,
            1 => {
                candidate.facts.pop();
            }
            2 => candidate.facts[0].confidence_bps = 9_499,
            3 => candidate.facts[0].rect_client_px.width = 0,
            4 => candidate.facts[0].rect_client_px.x = 1_230,
            5 => candidate.facts.push(candidate.facts[0].clone()),
            _ => unreachable!(),
        }
        assert!(validate_visible_competitive_lifecycle_snapshot_v1(candidate).is_err());
    }
}

#[test]
fn phase_specific_identity_terms_and_game_fields_fail_closed() {
    for mutation in 0..6 {
        let mut candidate = snapshot(MtgoCompetitiveLifecyclePhaseV1::EntryReview, 1);
        match mutation {
            0 => candidate.event_identity_sha256 = None,
            1 => candidate.match_identity_sha256 = Some(digest('7')),
            2 => candidate.game_number = Some(1),
            3 => candidate.entry_terms = None,
            4 => candidate.entry_terms.as_mut().unwrap().amount = 0,
            5 => candidate.frame_sha256 = "not-a-digest".to_owned(),
            _ => unreachable!(),
        }
        assert!(validate_visible_competitive_lifecycle_snapshot_v1(candidate).is_err());
    }
}

#[test]
fn stale_wrong_phase_and_cross_identity_transitions_fail_closed() {
    let pairing = validate_visible_competitive_lifecycle_snapshot_v1(snapshot(
        MtgoCompetitiveLifecyclePhaseV1::PairingReady,
        5,
    ))
    .unwrap();
    let intent = make_offline_competitive_lifecycle_intent_v1(
        &pairing,
        MtgoCompetitiveLifecycleActionV1::AcceptPairing,
        &mode_authorization(),
        None,
    )
    .unwrap();

    let stale = snapshot(MtgoCompetitiveLifecyclePhaseV1::MatchInProgress, 5);
    assert!(validate_competitive_lifecycle_action_transition_v1(
        &pairing,
        &intent,
        &mode_authorization(),
        None,
        stale,
    )
    .is_err());
    let wrong = snapshot(MtgoCompetitiveLifecyclePhaseV1::MatchComplete, 6);
    assert!(validate_competitive_lifecycle_action_transition_v1(
        &pairing,
        &intent,
        &mode_authorization(),
        None,
        wrong,
    )
    .is_err());
    let mut crossed = snapshot(MtgoCompetitiveLifecyclePhaseV1::MatchInProgress, 7);
    crossed.match_identity_sha256 = Some(digest('8'));
    assert!(validate_competitive_lifecycle_action_transition_v1(
        &pairing,
        &intent,
        &mode_authorization(),
        None,
        crossed,
    )
    .is_err());
}

#[test]
fn reconnect_and_server_advances_are_explicit_and_observation_only() {
    let game = validate_visible_competitive_lifecycle_snapshot_v1(snapshot(
        MtgoCompetitiveLifecyclePhaseV1::MatchInProgress,
        1,
    ))
    .unwrap();
    let reconnect = snapshot(MtgoCompetitiveLifecyclePhaseV1::Reconnect, 2);
    let checked_reconnect =
        validate_visible_competitive_lifecycle_snapshot_v1(reconnect.clone()).unwrap();
    let checked = validate_checked_observed_competitive_lifecycle_advance_v1(
        &game,
        MtgoObservedCompetitiveLifecycleAdvanceV1::ConnectionInterrupted,
        &checked_reconnect,
    )
    .unwrap();
    assert!(!checked.safe_for_live_input());
    let checked = validate_observed_competitive_lifecycle_advance_v1(
        &game,
        MtgoObservedCompetitiveLifecycleAdvanceV1::ConnectionInterrupted,
        reconnect.clone(),
    )
    .unwrap();
    assert!(!checked.safe_for_live_input());
    assert!(validate_observed_competitive_lifecycle_advance_v1(
        &game,
        MtgoObservedCompetitiveLifecycleAdvanceV1::PairingPosted,
        reconnect,
    )
    .is_err());
}

#[test]
fn lifecycle_json_rejects_coordinates_purchase_resources_and_unknown_fields() {
    let source = validate_visible_competitive_lifecycle_snapshot_v1(snapshot(
        MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
        1,
    ))
    .unwrap();
    let intent = make_offline_competitive_lifecycle_intent_v1(
        &source,
        MtgoCompetitiveLifecycleActionV1::OpenEntryReview,
        &mode_authorization(),
        None,
    )
    .unwrap();
    let json = serde_json::to_string(&intent).unwrap();
    let with_coordinates = json.replacen('{', r#"{"x":10,"y":20,"#, 1);
    assert!(
        serde_json::from_str::<MtgoOfflineCompetitiveLifecycleIntentV1>(&with_coordinates).is_err()
    );
    assert!(serde_json::from_str::<MtgoCompetitiveEntryTermsV1>(
        r#"{"terms_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","resource":"purchase_tickets","amount":10}"#
    )
    .is_err());
}
