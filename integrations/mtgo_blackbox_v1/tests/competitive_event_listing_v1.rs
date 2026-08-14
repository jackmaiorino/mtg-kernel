use mtg_kernel::card_def::card_id_by_name;
use mtgo_blackbox_v1::*;

fn digest(byte: char) -> String {
    byte.to_string().repeat(64)
}

fn card(name: &str, count: u16) -> MtgoCompetitiveDeckCardCountV1 {
    MtgoCompetitiveDeckCardCountV1 {
        card_db_id: card_id_by_name(name).unwrap(),
        card_name: name.to_owned(),
        count,
    }
}

fn sorted(mut cards: Vec<MtgoCompetitiveDeckCardCountV1>) -> Vec<MtgoCompetitiveDeckCardCountV1> {
    cards.sort_by_key(|card| card.card_db_id);
    cards
}

fn deck() -> ValidatedMtgoCompetitiveDeckManifestV1 {
    validate_competitive_deck_manifest_v1(MtgoCompetitiveDeckManifestV1 {
        schema_version: MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
        deck_list_sha256: digest('0'),
        format_sha256: digest('1'),
        starting_mainboard_count: 5,
        starting_sideboard_count: 2,
        configuration: MtgoCompetitiveDeckConfigurationV1 {
            mainboard: sorted(vec![card("Mountain", 3), card("Lightning Bolt", 2)]),
            sideboard: sorted(vec![card("Searing Blaze", 2)]),
        },
    })
    .unwrap()
}

fn lifecycle_fact(kind: MtgoLifecycleVisibleFactKindV1, x: u32) -> MtgoLifecycleVisibleFactV1 {
    MtgoLifecycleVisibleFactV1 {
        kind,
        rect_client_px: MtgoRectPxV1 {
            x,
            y: 40,
            width: 120,
            height: 40,
        },
        content_sha256: format!("{:064x}", x + 20),
        confidence_bps: 10_000,
    }
}

fn event_browser(
    event_kind: MtgoCompetitiveEventKindV1,
) -> CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1 {
    validate_visible_competitive_lifecycle_snapshot_v1(MtgoVisibleCompetitiveLifecycleSnapshotV1 {
        schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
        snapshot_id: "event-listing-browser-v1".to_owned(),
        event_kind,
        phase: MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
        frame_id: 1,
        frame_sequence: 1,
        frame_sha256: digest('3'),
        client_bounds: MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: 1_550,
            height: 925,
        },
        event_identity_sha256: None,
        match_identity_sha256: None,
        game_number: None,
        entry_terms: None,
        visible_state_complete: true,
        facts: vec![lifecycle_fact(
            MtgoLifecycleVisibleFactKindV1::EventBrowserVisible,
            20,
        )],
    })
    .unwrap()
}

fn entry_review(
    event_kind: MtgoCompetitiveEventKindV1,
    event_identity_sha256: String,
    frame_id: u64,
) -> MtgoVisibleCompetitiveLifecycleSnapshotV1 {
    MtgoVisibleCompetitiveLifecycleSnapshotV1 {
        schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
        snapshot_id: format!("event-listing-entry-review-{frame_id}"),
        event_kind,
        phase: MtgoCompetitiveLifecyclePhaseV1::EntryReview,
        frame_id,
        frame_sequence: frame_id,
        frame_sha256: format!("{frame_id:064x}"),
        client_bounds: MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: 1_550,
            height: 925,
        },
        event_identity_sha256: Some(event_identity_sha256),
        match_identity_sha256: None,
        game_number: None,
        entry_terms: Some(MtgoCompetitiveEntryTermsV1 {
            terms_sha256: digest('c'),
            resource: MtgoCompetitiveEntryResourceV1::ExistingPlayPoints,
            amount: 100,
        }),
        visible_state_complete: true,
        facts: vec![
            lifecycle_fact(MtgoLifecycleVisibleFactKindV1::EntryReviewVisible, 20),
            lifecycle_fact(MtgoLifecycleVisibleFactKindV1::EntryTermsVisible, 180),
        ],
    }
}

fn authorization(event_kind: MtgoCompetitiveEventKindV1) -> MtgoAuthorizationScopeV1 {
    MtgoAuthorizationScopeV1 {
        schema_version: MTGO_AUTHORIZATION_SCHEMA_V1,
        account_alias_sha256: digest('a'),
        written_permission_sha256: digest('b'),
        visible_channels_only: true,
        league_input: event_kind == MtgoCompetitiveEventKindV1::League,
        challenge_input: event_kind == MtgoCompetitiveEventKindV1::Challenge,
        ..MtgoAuthorizationScopeV1::default()
    }
}

struct Fixture {
    deck: ValidatedMtgoCompetitiveDeckManifestV1,
    target: MtgoCompetitiveEventListingTargetV1,
    lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    raw: MtgoVisibleCompetitiveEventListingSelectionV1,
}

fn fixture(event_kind: MtgoCompetitiveEventKindV1) -> Fixture {
    let deck = deck();
    let lifecycle = event_browser(event_kind);
    let target = MtgoCompetitiveEventListingTargetV1 {
        schema_version: MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1,
        target_id: match event_kind {
            MtgoCompetitiveEventKindV1::League => "modern-league-v1",
            MtgoCompetitiveEventKindV1::Challenge => "modern-challenge-v1",
        }
        .to_owned(),
        event_kind,
        approved_account_alias_sha256: digest('a'),
        event_identity_sha256: digest('4'),
        event_display_label_sha256: digest('5'),
        deck_display_label_sha256: digest('6'),
        deck_list_sha256: deck.deck_list_sha256().to_owned(),
        deck_manifest_commitment_sha256: deck.manifest_commitment_sha256().to_owned(),
        deck_format_sha256: deck.format_sha256().to_owned(),
        policy_deployment_commitment_sha256: digest('7'),
    };
    let target_commitment_sha256 =
        competitive_event_listing_target_commitment_v1(&target, &deck).unwrap();
    let raw = MtgoVisibleCompetitiveEventListingSelectionV1 {
        schema_version: MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1,
        selection_id: "selected-event-listing-v1".to_owned(),
        target_commitment_sha256,
        event_kind,
        event_identity_sha256: target.event_identity_sha256.clone(),
        event_display_label_sha256: target.event_display_label_sha256.clone(),
        source_lifecycle_snapshot_commitment_sha256: lifecycle
            .snapshot_commitment_sha256()
            .to_owned(),
        frame_id: lifecycle.frame_id_v1(),
        frame_sequence: lifecycle.frame_sequence(),
        frame_sha256: lifecycle.frame_sha256_v1().to_owned(),
        client_bounds: lifecycle.client_bounds_v1().clone(),
        event_label_rect_client_px: MtgoRectPxV1 {
            x: 100,
            y: 200,
            width: 400,
            height: 80,
        },
        event_label_region_sha256: digest('8'),
        open_entry_review_control_rect_client_px: MtgoRectPxV1 {
            x: 1_200,
            y: 200,
            width: 180,
            height: 80,
        },
        open_entry_review_control_region_sha256: digest('9'),
        open_entry_review_control_enabled: true,
        confidence_bps: 10_000,
    };
    Fixture {
        deck,
        target,
        lifecycle,
        raw,
    }
}

fn checked_selection(
    event_kind: MtgoCompetitiveEventKindV1,
) -> (
    ValidatedMtgoCompetitiveDeckManifestV1,
    CheckedUntrustedMtgoCompetitiveEventListingSelectionV1,
) {
    let fixture = fixture(event_kind);
    let selection = validate_visible_competitive_event_listing_selection_v1(
        fixture.lifecycle,
        &fixture.deck,
        fixture.target,
        fixture.raw,
    )
    .unwrap();
    (fixture.deck, selection)
}

#[test]
fn exact_league_and_challenge_listing_reach_only_matching_entry_review() {
    for event_kind in [
        MtgoCompetitiveEventKindV1::League,
        MtgoCompetitiveEventKindV1::Challenge,
    ] {
        let (deck, selection) = checked_selection(event_kind);
        assert_eq!(selection.event_kind_v1(), event_kind);
        assert_eq!(selection.deck_list_sha256_v1(), deck.deck_list_sha256());
        assert_eq!(
            selection.deck_manifest_commitment_sha256_v1(),
            deck.manifest_commitment_sha256()
        );
        assert!(!selection.safe_for_input_v1());
        assert!(!selection.permits_event_entry_v1());
        assert!(!selection.permits_spending_v1());
        let event_identity = selection.event_identity_sha256_v1().to_owned();
        let intent = make_offline_competitive_event_listing_open_intent_v1(
            selection,
            &authorization(event_kind),
        )
        .unwrap();
        assert_eq!(intent.event_kind_v1(), event_kind);
        assert!(!intent.safe_for_input_v1());
        assert!(!intent.permits_entry_confirmation_v1());
        let arrival = confirm_competitive_event_listing_opened_v1(
            intent,
            &authorization(event_kind),
            entry_review(event_kind, event_identity.clone(), 2),
        )
        .unwrap();
        assert_eq!(arrival.event_identity_sha256_v1(), event_identity);
        assert_eq!(arrival.deck_list_sha256_v1(), deck.deck_list_sha256());
        assert_eq!(arrival.arrival_commitment_sha256_v1().len(), 64);
        assert!(!arrival.safe_for_input_v1());
        assert!(!arrival.permits_entry_confirmation_v1());
        assert!(!arrival.permits_spending_v1());
    }
}

#[test]
fn source_frame_phase_and_mode_substitution_fail_closed() {
    let mut frame_fixture = fixture(MtgoCompetitiveEventKindV1::League);
    frame_fixture.raw.frame_id += 1;
    assert_eq!(
        validate_visible_competitive_event_listing_selection_v1(
            frame_fixture.lifecycle,
            &frame_fixture.deck,
            frame_fixture.target,
            frame_fixture.raw,
        )
        .err()
        .unwrap()
        .code(),
        "event_listing_source"
    );

    let mut mode_fixture = fixture(MtgoCompetitiveEventKindV1::League);
    mode_fixture.raw.event_kind = MtgoCompetitiveEventKindV1::Challenge;
    assert_eq!(
        validate_visible_competitive_event_listing_selection_v1(
            mode_fixture.lifecycle,
            &mode_fixture.deck,
            mode_fixture.target,
            mode_fixture.raw,
        )
        .err()
        .unwrap()
        .code(),
        "event_listing_target_binding"
    );
}

#[test]
fn target_deck_format_policy_and_visible_identity_are_exact() {
    let mut deck_fixture = fixture(MtgoCompetitiveEventKindV1::League);
    deck_fixture.target.deck_list_sha256 = digest('d');
    assert_eq!(
        validate_visible_competitive_event_listing_selection_v1(
            deck_fixture.lifecycle,
            &deck_fixture.deck,
            deck_fixture.target,
            deck_fixture.raw,
        )
        .err()
        .unwrap()
        .code(),
        "event_listing_deployment_identity"
    );

    let mut identity_fixture = fixture(MtgoCompetitiveEventKindV1::League);
    identity_fixture.raw.event_display_label_sha256 = digest('d');
    assert_eq!(
        validate_visible_competitive_event_listing_selection_v1(
            identity_fixture.lifecycle,
            &identity_fixture.deck,
            identity_fixture.target,
            identity_fixture.raw,
        )
        .err()
        .unwrap()
        .code(),
        "event_listing_target_binding"
    );

    let mut deck_label_fixture = fixture(MtgoCompetitiveEventKindV1::League);
    deck_label_fixture.target.deck_display_label_sha256 = "not-a-digest".to_owned();
    assert_eq!(
        validate_visible_competitive_event_listing_selection_v1(
            deck_label_fixture.lifecycle,
            &deck_label_fixture.deck,
            deck_label_fixture.target,
            deck_label_fixture.raw,
        )
        .err()
        .unwrap()
        .code(),
        "event_listing_deck_label"
    );
}

#[test]
fn disabled_low_confidence_overlapping_and_out_of_bounds_controls_reject() {
    for mutation in 0..4 {
        let mut fixture = fixture(MtgoCompetitiveEventKindV1::League);
        match mutation {
            0 => fixture.raw.open_entry_review_control_enabled = false,
            1 => fixture.raw.confidence_bps = 9_499,
            2 => fixture.raw.open_entry_review_control_rect_client_px.x = 300,
            3 => fixture.raw.open_entry_review_control_rect_client_px.x = 1_500,
            _ => unreachable!(),
        }
        assert!(validate_visible_competitive_event_listing_selection_v1(
            fixture.lifecycle,
            &fixture.deck,
            fixture.target,
            fixture.raw,
        )
        .is_err());
    }
}

#[test]
fn open_intent_requires_exact_account_and_one_selected_mode() {
    let (_, selection) = checked_selection(MtgoCompetitiveEventKindV1::League);
    let mut wrong_account = authorization(MtgoCompetitiveEventKindV1::League);
    wrong_account.account_alias_sha256 = digest('d');
    assert_eq!(
        make_offline_competitive_event_listing_open_intent_v1(selection, &wrong_account)
            .err()
            .unwrap()
            .code(),
        "event_listing_account"
    );

    let (_, selection) = checked_selection(MtgoCompetitiveEventKindV1::League);
    let mut combined = authorization(MtgoCompetitiveEventKindV1::League);
    combined.challenge_input = true;
    assert_eq!(
        make_offline_competitive_event_listing_open_intent_v1(selection, &combined)
            .err()
            .unwrap()
            .code(),
        "event_listing_mode_scope"
    );
}

#[test]
fn arrival_rejects_wrong_event_stale_frame_and_authorization_drift() {
    let (_, selection) = checked_selection(MtgoCompetitiveEventKindV1::League);
    let auth = authorization(MtgoCompetitiveEventKindV1::League);
    let intent = make_offline_competitive_event_listing_open_intent_v1(selection, &auth).unwrap();
    assert_eq!(
        confirm_competitive_event_listing_opened_v1(
            intent,
            &auth,
            entry_review(MtgoCompetitiveEventKindV1::League, digest('d'), 2),
        )
        .err()
        .unwrap()
        .code(),
        "event_listing_arrival_identity"
    );

    let (_, selection) = checked_selection(MtgoCompetitiveEventKindV1::League);
    let intent = make_offline_competitive_event_listing_open_intent_v1(selection, &auth).unwrap();
    assert_eq!(
        confirm_competitive_event_listing_opened_v1(
            intent,
            &auth,
            entry_review(MtgoCompetitiveEventKindV1::League, digest('4'), 1),
        )
        .err()
        .unwrap()
        .code(),
        "lifecycle_transition_freshness"
    );

    let (_, selection) = checked_selection(MtgoCompetitiveEventKindV1::League);
    let intent = make_offline_competitive_event_listing_open_intent_v1(selection, &auth).unwrap();
    let mut broad = auth.clone();
    broad.challenge_input = true;
    assert_eq!(
        confirm_competitive_event_listing_opened_v1(
            intent,
            &broad,
            entry_review(MtgoCompetitiveEventKindV1::League, digest('4'), 2),
        )
        .err()
        .unwrap()
        .code(),
        "event_listing_arrival_authorization"
    );
}

#[test]
fn target_and_visible_selection_json_reject_unknown_fields() {
    let fixture = fixture(MtgoCompetitiveEventKindV1::League);
    let mut target = serde_json::to_value(&fixture.target).unwrap();
    target
        .as_object_mut()
        .unwrap()
        .insert("process_memory".to_owned(), serde_json::json!(true));
    assert!(serde_json::from_value::<MtgoCompetitiveEventListingTargetV1>(target).is_err());

    let mut raw = serde_json::to_value(&fixture.raw).unwrap();
    raw.as_object_mut()
        .unwrap()
        .insert("window_handle".to_owned(), serde_json::json!(42));
    assert!(serde_json::from_value::<MtgoVisibleCompetitiveEventListingSelectionV1>(raw).is_err());
}
