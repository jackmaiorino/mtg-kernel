use mtgo_blackbox_v1::*;

fn digest(byte: char) -> String {
    byte.to_string().repeat(64)
}

fn lifecycle_facts(phase: MtgoCompetitiveLifecyclePhaseV1) -> Vec<MtgoLifecycleVisibleFactV1> {
    use MtgoLifecycleVisibleFactKindV1::*;
    let kinds: &[MtgoLifecycleVisibleFactKindV1] = match phase {
        MtgoCompetitiveLifecyclePhaseV1::EventBrowser => &[EventBrowserVisible],
        MtgoCompetitiveLifecyclePhaseV1::EntryReview => &[EntryReviewVisible, EntryTermsVisible],
        MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing => &[EnteredEventVisible],
        MtgoCompetitiveLifecyclePhaseV1::PairingReady => {
            &[PairingVisible, PairingAcceptControlEnabled]
        }
        MtgoCompetitiveLifecyclePhaseV1::MatchInProgress => {
            &[MatchSurfaceVisible, LocalClockVisible, OpponentClockVisible]
        }
        MtgoCompetitiveLifecyclePhaseV1::Sideboarding => &[
            SideboardSurfaceVisible,
            SideboardTimerVisible,
            SideboardConfigurationVisible,
            SideboardNoChangesConfirmed,
            SideboardSubmitControlEnabled,
        ],
        MtgoCompetitiveLifecyclePhaseV1::MatchComplete => {
            &[MatchResultVisible, MatchContinueControlEnabled]
        }
        MtgoCompetitiveLifecyclePhaseV1::EventComplete => {
            &[EventResultVisible, EventCloseControlEnabled]
        }
        MtgoCompetitiveLifecyclePhaseV1::Reconnect => {
            &[ReconnectVisible, ReconnectResumeControlEnabled]
        }
    };
    kinds
        .iter()
        .copied()
        .enumerate()
        .map(|(index, kind)| MtgoLifecycleVisibleFactV1 {
            kind,
            rect_client_px: MtgoRectPxV1 {
                x: 10 + (index as u32 * 30),
                y: 10,
                width: 20,
                height: 20,
            },
            content_sha256: digest('a'),
            confidence_bps: 10_000,
        })
        .collect()
}

fn lifecycle(
    event_kind: MtgoCompetitiveEventKindV1,
    phase: MtgoCompetitiveLifecyclePhaseV1,
    sequence: u64,
) -> CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1 {
    let match_related = matches!(
        phase,
        MtgoCompetitiveLifecyclePhaseV1::PairingReady
            | MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
            | MtgoCompetitiveLifecyclePhaseV1::Sideboarding
            | MtgoCompetitiveLifecyclePhaseV1::MatchComplete
            | MtgoCompetitiveLifecyclePhaseV1::Reconnect
    );
    validate_visible_competitive_lifecycle_snapshot_v1(MtgoVisibleCompetitiveLifecycleSnapshotV1 {
        schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
        snapshot_id: format!("event-record-source-{sequence}"),
        event_kind,
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
        event_identity_sha256: Some(digest('b')),
        match_identity_sha256: match_related.then(|| digest('c')),
        game_number: matches!(
            phase,
            MtgoCompetitiveLifecyclePhaseV1::MatchInProgress
                | MtgoCompetitiveLifecyclePhaseV1::Sideboarding
                | MtgoCompetitiveLifecyclePhaseV1::Reconnect
        )
        .then_some(1),
        entry_terms: None,
        visible_state_complete: true,
        facts: lifecycle_facts(phase),
    })
    .unwrap()
}

fn event_fact(
    kind: MtgoCompetitiveEventRecordVisibleFactKindV1,
    index: u32,
) -> MtgoCompetitiveEventRecordVisibleFactV1 {
    MtgoCompetitiveEventRecordVisibleFactV1 {
        kind,
        rect_client_px: MtgoRectPxV1 {
            x: 100 + index * 120,
            y: 100,
            width: 100,
            height: 30,
        },
        content_sha256: digest('d'),
        confidence_bps: 10_000,
    }
}

fn record(
    source: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    status: MtgoCompetitiveEventVisibleStatusV1,
    progress: MtgoCompetitiveEventProgressV1,
) -> MtgoVisibleCompetitiveEventRecordV1 {
    let mut facts = vec![
        event_fact(
            MtgoCompetitiveEventRecordVisibleFactKindV1::EventStatusVisible,
            0,
        ),
        event_fact(
            MtgoCompetitiveEventRecordVisibleFactKindV1::EventProgressVisible,
            1,
        ),
    ];
    if matches!(
        &progress,
        MtgoCompetitiveEventProgressV1::Challenge {
            standing_rank: Some(_),
            field_size: Some(_),
            ..
        }
    ) {
        facts.push(event_fact(
            MtgoCompetitiveEventRecordVisibleFactKindV1::EventStandingVisible,
            2,
        ));
    }
    let completion = (status == MtgoCompetitiveEventVisibleStatusV1::EventComplete)
        .then_some(MtgoCompetitiveEventCompletionV1::Completed);
    if completion.is_some() {
        facts.push(event_fact(
            MtgoCompetitiveEventRecordVisibleFactKindV1::EventResultVisible,
            3,
        ));
    }
    MtgoVisibleCompetitiveEventRecordV1 {
        schema_version: MTGO_COMPETITIVE_EVENT_RECORD_SCHEMA_V1,
        record_id: format!("record-{}", source.frame_sequence()),
        source_lifecycle_snapshot_commitment_sha256: source.snapshot_commitment_sha256().to_owned(),
        approved_account_alias_sha256: digest('e'),
        event_kind: source.event_kind(),
        lifecycle_phase: source.phase(),
        frame_id: source.frame_id_v1(),
        frame_sequence: source.frame_sequence(),
        frame_sha256: source.frame_sha256_v1().to_owned(),
        client_bounds: source.client_bounds_v1().clone(),
        event_identity_sha256: source.event_identity_sha256_v1().unwrap().to_owned(),
        status,
        progress,
        completion,
        visible_record_complete: true,
        facts,
    }
}

fn league_progress(completed: u16) -> MtgoCompetitiveEventProgressV1 {
    MtgoCompetitiveEventProgressV1::League {
        match_record: MtgoCompetitiveMatchRecordV1 {
            wins: completed,
            losses: 0,
            draws: 0,
            matches_completed: completed,
        },
        matches_total: Some(5),
    }
}

fn challenge_progress(rounds_completed: u16) -> MtgoCompetitiveEventProgressV1 {
    MtgoCompetitiveEventProgressV1::Challenge {
        match_record: MtgoCompetitiveMatchRecordV1 {
            wins: rounds_completed,
            losses: 0,
            draws: 0,
            matches_completed: rounds_completed,
        },
        rounds_completed,
        rounds_total: 8,
        match_points: rounds_completed * 3,
        standing_rank: Some(12),
        field_size: Some(128),
    }
}

#[test]
fn league_waiting_record_binds_exact_lifecycle_and_account() {
    let source = lifecycle(
        MtgoCompetitiveEventKindV1::League,
        MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing,
        10,
    );
    let checked = validate_visible_competitive_event_record_v1(
        &source,
        &digest('e'),
        record(
            &source,
            MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing,
            league_progress(2),
        ),
    )
    .unwrap();
    assert_eq!(checked.event_kind_v1(), MtgoCompetitiveEventKindV1::League);
    assert_eq!(
        checked.status_v1(),
        MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing
    );
    assert_eq!(checked.record_commitment_sha256_v1().len(), 64);
    assert_eq!(checked.visible_facts_v1().len(), 2);
    assert!(!checked.safe_for_live_input_v1());
    assert!(!checked.permits_event_entry_v1());
    assert!(!checked.permits_spending_v1());
    assert!(!checked.permits_gameplay_v1());
}

#[test]
fn challenge_standing_is_source_bound_and_coordinate_free_after_validation() {
    let source = lifecycle(
        MtgoCompetitiveEventKindV1::Challenge,
        MtgoCompetitiveLifecyclePhaseV1::MatchComplete,
        20,
    );
    let checked = validate_visible_competitive_event_record_v1(
        &source,
        &digest('e'),
        record(
            &source,
            MtgoCompetitiveEventVisibleStatusV1::BetweenMatches,
            challenge_progress(3),
        ),
    )
    .unwrap();
    assert!(matches!(
        checked.progress_v1(),
        MtgoCompetitiveEventProgressV1::Challenge {
            standing_rank: Some(12),
            field_size: Some(128),
            ..
        }
    ));
    assert_eq!(checked.completion_v1(), None);
}

#[test]
fn source_frame_event_and_account_cannot_be_substituted() {
    let source = lifecycle(
        MtgoCompetitiveEventKindV1::League,
        MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing,
        30,
    );
    let baseline = record(
        &source,
        MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing,
        league_progress(0),
    );
    for mutate in 0..4 {
        let mut changed = baseline.clone();
        match mutate {
            0 => changed.frame_id += 1,
            1 => changed.event_identity_sha256 = digest('f'),
            2 => changed.source_lifecycle_snapshot_commitment_sha256 = digest('1'),
            _ => changed.approved_account_alias_sha256 = digest('2'),
        }
        assert_eq!(
            validate_visible_competitive_event_record_v1(&source, &digest('e'), changed)
                .err()
                .unwrap()
                .code(),
            "event_record_source_binding"
        );
    }
}

#[test]
fn lifecycle_phase_and_visible_status_must_match() {
    let source = lifecycle(
        MtgoCompetitiveEventKindV1::League,
        MtgoCompetitiveLifecyclePhaseV1::PairingReady,
        40,
    );
    let wrong = record(
        &source,
        MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing,
        league_progress(1),
    );
    assert_eq!(
        validate_visible_competitive_event_record_v1(&source, &digest('e'), wrong)
            .err()
            .unwrap()
            .code(),
        "event_record_status"
    );
}

#[test]
fn league_and_challenge_progress_schemas_are_not_interchangeable() {
    let league = lifecycle(
        MtgoCompetitiveEventKindV1::League,
        MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing,
        50,
    );
    let wrong = record(
        &league,
        MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing,
        challenge_progress(0),
    );
    assert_eq!(
        validate_visible_competitive_event_record_v1(&league, &digest('e'), wrong)
            .err()
            .unwrap()
            .code(),
        "event_record_mode"
    );
}

#[test]
fn match_record_and_declared_total_are_internally_consistent() {
    let source = lifecycle(
        MtgoCompetitiveEventKindV1::League,
        MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing,
        60,
    );
    let mut wrong_sum = record(
        &source,
        MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing,
        league_progress(2),
    );
    if let MtgoCompetitiveEventProgressV1::League { match_record, .. } = &mut wrong_sum.progress {
        match_record.matches_completed = 1;
    }
    assert_eq!(
        validate_visible_competitive_event_record_v1(&source, &digest('e'), wrong_sum)
            .err()
            .unwrap()
            .code(),
        "event_record_match_record"
    );

    let mut wrong_total = record(
        &source,
        MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing,
        league_progress(2),
    );
    if let MtgoCompetitiveEventProgressV1::League { matches_total, .. } = &mut wrong_total.progress
    {
        *matches_total = Some(1);
    }
    assert_eq!(
        validate_visible_competitive_event_record_v1(&source, &digest('e'), wrong_total)
            .err()
            .unwrap()
            .code(),
        "event_record_league_total"
    );
}

#[test]
fn challenge_rounds_and_standing_must_be_consistent() {
    let source = lifecycle(
        MtgoCompetitiveEventKindV1::Challenge,
        MtgoCompetitiveLifecyclePhaseV1::MatchComplete,
        70,
    );
    let mut wrong_round = record(
        &source,
        MtgoCompetitiveEventVisibleStatusV1::BetweenMatches,
        challenge_progress(3),
    );
    if let MtgoCompetitiveEventProgressV1::Challenge {
        rounds_completed, ..
    } = &mut wrong_round.progress
    {
        *rounds_completed = 9;
    }
    assert_eq!(
        validate_visible_competitive_event_record_v1(&source, &digest('e'), wrong_round)
            .err()
            .unwrap()
            .code(),
        "event_record_challenge_rounds"
    );

    let mut too_many_matches = record(
        &source,
        MtgoCompetitiveEventVisibleStatusV1::BetweenMatches,
        challenge_progress(3),
    );
    if let MtgoCompetitiveEventProgressV1::Challenge { match_record, .. } =
        &mut too_many_matches.progress
    {
        match_record.wins = 4;
        match_record.matches_completed = 4;
    }
    assert_eq!(
        validate_visible_competitive_event_record_v1(&source, &digest('e'), too_many_matches)
            .err()
            .unwrap()
            .code(),
        "event_record_challenge_rounds"
    );

    let mut missing_field = record(
        &source,
        MtgoCompetitiveEventVisibleStatusV1::BetweenMatches,
        challenge_progress(3),
    );
    if let MtgoCompetitiveEventProgressV1::Challenge { field_size, .. } =
        &mut missing_field.progress
    {
        *field_size = None;
    }
    assert_eq!(
        validate_visible_competitive_event_record_v1(&source, &digest('e'), missing_field)
            .err()
            .unwrap()
            .code(),
        "event_record_challenge_standing"
    );
}

#[test]
fn completion_reason_is_exactly_bound_to_completed_state_and_progress() {
    let source = lifecycle(
        MtgoCompetitiveEventKindV1::Challenge,
        MtgoCompetitiveLifecyclePhaseV1::EventComplete,
        80,
    );
    let mut early = record(
        &source,
        MtgoCompetitiveEventVisibleStatusV1::EventComplete,
        challenge_progress(3),
    );
    assert_eq!(
        validate_visible_competitive_event_record_v1(&source, &digest('e'), early.clone())
            .err()
            .unwrap()
            .code(),
        "event_record_completion_progress"
    );
    early.completion = Some(MtgoCompetitiveEventCompletionV1::Eliminated);
    validate_visible_competitive_event_record_v1(&source, &digest('e'), early).unwrap();

    let waiting = lifecycle(
        MtgoCompetitiveEventKindV1::League,
        MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing,
        81,
    );
    let mut premature = record(
        &waiting,
        MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing,
        league_progress(1),
    );
    premature.completion = Some(MtgoCompetitiveEventCompletionV1::Dropped);
    assert_eq!(
        validate_visible_competitive_event_record_v1(&waiting, &digest('e'), premature)
            .err()
            .unwrap()
            .code(),
        "event_record_completion"
    );
}

#[test]
fn fact_set_is_complete_unique_high_confidence_and_in_bounds() {
    let source = lifecycle(
        MtgoCompetitiveEventKindV1::League,
        MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing,
        90,
    );
    let baseline = record(
        &source,
        MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing,
        league_progress(1),
    );

    let mut missing = baseline.clone();
    missing.facts.pop();
    assert_eq!(
        validate_visible_competitive_event_record_v1(&source, &digest('e'), missing)
            .err()
            .unwrap()
            .code(),
        "event_record_fact_count"
    );

    let mut duplicate = baseline.clone();
    duplicate.facts[1].kind = duplicate.facts[0].kind;
    assert_eq!(
        validate_visible_competitive_event_record_v1(&source, &digest('e'), duplicate)
            .err()
            .unwrap()
            .code(),
        "event_record_fact_duplicate"
    );

    let mut low_confidence = baseline.clone();
    low_confidence.facts[0].confidence_bps = 9_499;
    assert_eq!(
        validate_visible_competitive_event_record_v1(&source, &digest('e'), low_confidence)
            .err()
            .unwrap()
            .code(),
        "event_record_fact_confidence"
    );

    let mut out_of_bounds = baseline;
    out_of_bounds.facts[0].rect_client_px.x = 1_200;
    out_of_bounds.facts[0].rect_client_px.width = 100;
    assert_eq!(
        validate_visible_competitive_event_record_v1(&source, &digest('e'), out_of_bounds)
            .err()
            .unwrap()
            .code(),
        "event_record_fact_bounds"
    );
}

#[test]
fn serde_rejects_unknown_event_record_fields() {
    let source = lifecycle(
        MtgoCompetitiveEventKindV1::League,
        MtgoCompetitiveLifecyclePhaseV1::EnteredWaitingForPairing,
        100,
    );
    let mut value = serde_json::to_value(record(
        &source,
        MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing,
        league_progress(0),
    ))
    .unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("hidden_memory".to_owned(), serde_json::json!(true));
    assert!(serde_json::from_value::<MtgoVisibleCompetitiveEventRecordV1>(value).is_err());
}
