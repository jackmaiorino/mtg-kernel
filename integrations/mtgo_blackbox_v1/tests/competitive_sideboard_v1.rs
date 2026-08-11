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

fn starting_configuration() -> MtgoCompetitiveDeckConfigurationV1 {
    MtgoCompetitiveDeckConfigurationV1 {
        mainboard: sorted(vec![card("Mountain", 3), card("Lightning Bolt", 2)]),
        sideboard: sorted(vec![card("Searing Blaze", 2)]),
    }
}

fn swapped_configuration() -> MtgoCompetitiveDeckConfigurationV1 {
    MtgoCompetitiveDeckConfigurationV1 {
        mainboard: sorted(vec![
            card("Mountain", 3),
            card("Lightning Bolt", 1),
            card("Searing Blaze", 1),
        ]),
        sideboard: sorted(vec![card("Lightning Bolt", 1), card("Searing Blaze", 1)]),
    }
}

fn manifest_raw() -> MtgoCompetitiveDeckManifestV1 {
    MtgoCompetitiveDeckManifestV1 {
        schema_version: MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
        deck_list_sha256: digest('0'),
        format_sha256: digest('1'),
        starting_mainboard_count: 5,
        starting_sideboard_count: 2,
        configuration: starting_configuration(),
    }
}

fn lifecycle_raw(sequence: u64) -> MtgoVisibleCompetitiveLifecycleSnapshotV1 {
    let facts = [
        MtgoLifecycleVisibleFactKindV1::SideboardSurfaceVisible,
        MtgoLifecycleVisibleFactKindV1::SideboardTimerVisible,
        MtgoLifecycleVisibleFactKindV1::SideboardConfigurationVisible,
        MtgoLifecycleVisibleFactKindV1::SideboardSubmitControlEnabled,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, kind)| MtgoLifecycleVisibleFactV1 {
        kind,
        rect_client_px: MtgoRectPxV1 {
            x: 10 + (index as u32 * 25),
            y: 10,
            width: 20,
            height: 20,
        },
        content_sha256: digest('2'),
        confidence_bps: 10_000,
    })
    .collect();
    MtgoVisibleCompetitiveLifecycleSnapshotV1 {
        schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
        snapshot_id: format!("sideboard-lifecycle-{sequence}"),
        event_kind: MtgoCompetitiveEventKindV1::League,
        phase: MtgoCompetitiveLifecyclePhaseV1::Sideboarding,
        frame_id: sequence,
        frame_sequence: sequence,
        frame_sha256: format!("{sequence:064x}"),
        client_bounds: MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: 1_240,
            height: 740,
        },
        event_identity_sha256: Some(digest('3')),
        match_identity_sha256: Some(digest('4')),
        game_number: Some(1),
        entry_terms: None,
        visible_state_complete: true,
        facts,
    }
}

fn visible_cards(
    configuration: &MtgoCompetitiveDeckConfigurationV1,
) -> Vec<MtgoVisibleCompetitiveSideboardCardV1> {
    configuration
        .mainboard
        .iter()
        .map(|card| (MtgoCompetitiveDeckPartitionV1::Mainboard, card))
        .chain(
            configuration
                .sideboard
                .iter()
                .map(|card| (MtgoCompetitiveDeckPartitionV1::Sideboard, card)),
        )
        .enumerate()
        .map(
            |(index, (partition, card))| MtgoVisibleCompetitiveSideboardCardV1 {
                partition,
                card_db_id: card.card_db_id,
                card_name: card.card_name.clone(),
                count: card.count,
                rect_client_px: MtgoRectPxV1 {
                    x: 20 + (index as u32 * 80),
                    y: match partition {
                        MtgoCompetitiveDeckPartitionV1::Mainboard => 100,
                        MtgoCompetitiveDeckPartitionV1::Sideboard => 420,
                    },
                    width: 60,
                    height: 80,
                },
                content_sha256: format!("{:064x}", index + 20),
                confidence_bps: 10_000,
            },
        )
        .collect()
}

fn snapshot_raw(
    lifecycle: &CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    manifest: &ValidatedMtgoCompetitiveDeckManifestV1,
    configuration: &MtgoCompetitiveDeckConfigurationV1,
) -> MtgoVisibleCompetitiveSideboardSnapshotV1 {
    MtgoVisibleCompetitiveSideboardSnapshotV1 {
        schema_version: MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
        snapshot_id: format!("sideboard-{}", lifecycle.frame_sequence()),
        event_kind: lifecycle.event_kind(),
        event_identity_sha256: lifecycle.event_identity_sha256_v1().unwrap().to_owned(),
        match_identity_sha256: lifecycle.match_identity_sha256_v1().unwrap().to_owned(),
        game_number: lifecycle.game_number_v1().unwrap(),
        frame_id: lifecycle.frame_id_v1(),
        frame_sequence: lifecycle.frame_sequence(),
        frame_sha256: lifecycle.frame_sha256_v1().to_owned(),
        lifecycle_snapshot_commitment_sha256: lifecycle.snapshot_commitment_sha256().to_owned(),
        deck_manifest_commitment_sha256: manifest.manifest_commitment_sha256().to_owned(),
        policy_deployment_commitment_sha256: digest('5'),
        visible_configuration_complete: true,
        mainboard_zone: MtgoVisibleCompetitiveSideboardZoneV1 {
            rect_client_px: MtgoRectPxV1 {
                x: 0,
                y: 80,
                width: 1_000,
                height: 250,
            },
            content_sha256: digest('6'),
            empty_drop_rect_client_px: MtgoRectPxV1 {
                x: 900,
                y: 100,
                width: 60,
                height: 80,
            },
            empty_drop_content_sha256: digest('7'),
            confidence_bps: 10_000,
        },
        sideboard_zone: MtgoVisibleCompetitiveSideboardZoneV1 {
            rect_client_px: MtgoRectPxV1 {
                x: 0,
                y: 400,
                width: 1_000,
                height: 250,
            },
            content_sha256: digest('8'),
            empty_drop_rect_client_px: MtgoRectPxV1 {
                x: 900,
                y: 420,
                width: 60,
                height: 80,
            },
            empty_drop_content_sha256: digest('9'),
            confidence_bps: 10_000,
        },
        cards: visible_cards(configuration),
    }
}

fn checked_snapshot(
    manifest: &ValidatedMtgoCompetitiveDeckManifestV1,
    configuration: &MtgoCompetitiveDeckConfigurationV1,
    sequence: u64,
) -> CheckedUntrustedMtgoCompetitiveSideboardSnapshotV1 {
    let lifecycle =
        validate_visible_competitive_lifecycle_snapshot_v1(lifecycle_raw(sequence)).unwrap();
    let snapshot = snapshot_raw(&lifecycle, manifest, configuration);
    validate_visible_competitive_sideboard_snapshot_v1(lifecycle, manifest, snapshot).unwrap()
}

fn selection(
    source: &CheckedUntrustedMtgoCompetitiveSideboardSnapshotV1,
    target_configuration: MtgoCompetitiveDeckConfigurationV1,
) -> MtgoCompetitiveSideboardSelectionV1 {
    MtgoCompetitiveSideboardSelectionV1 {
        schema_version: MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
        source_snapshot_commitment_sha256: source.snapshot_commitment_sha256().to_owned(),
        deck_manifest_commitment_sha256: source.deck_manifest_commitment_sha256().to_owned(),
        policy_deployment_commitment_sha256: source
            .policy_deployment_commitment_sha256()
            .to_owned(),
        target_configuration,
    }
}

#[test]
fn manifest_is_canonical_exact_and_non_authorizing() {
    let manifest = validate_competitive_deck_manifest_v1(manifest_raw()).unwrap();
    assert_eq!(manifest.starting_mainboard_count(), 5);
    assert_eq!(manifest.starting_sideboard_count(), 2);
    assert_eq!(manifest.deck_list_sha256(), digest('0'));
    assert_eq!(manifest.manifest_commitment_sha256().len(), 64);
    assert!(!manifest.claims_format_legality_v1());
    assert!(!manifest.permits_live_input_v1());

    let mut duplicate = manifest_raw();
    duplicate.configuration.mainboard.push(card("Mountain", 1));
    duplicate.starting_mainboard_count += 1;
    assert_eq!(
        validate_competitive_deck_manifest_v1(duplicate)
            .err()
            .unwrap()
            .code(),
        "sideboard_partition_order"
    );

    let mut wrong_count = manifest_raw();
    wrong_count.starting_mainboard_count = 6;
    assert_eq!(
        validate_competitive_deck_manifest_v1(wrong_count)
            .err()
            .unwrap()
            .code(),
        "sideboard_manifest_count"
    );

    let mut crossed_identity = manifest_raw();
    crossed_identity.deck_list_sha256 = crossed_identity.format_sha256.clone();
    assert_eq!(
        validate_competitive_deck_manifest_v1(crossed_identity)
            .err()
            .unwrap()
            .code(),
        "sideboard_manifest_format"
    );
}

#[test]
fn manifest_rejects_unknown_mismatched_and_unsupported_cards() {
    let mut unknown = manifest_raw();
    unknown.configuration.mainboard[0].card_db_id = u16::MAX;
    assert!(validate_competitive_deck_manifest_v1(unknown).is_err());

    let mut renamed = manifest_raw();
    renamed.configuration.mainboard[0].card_name = "Not the kernel card".to_owned();
    assert!(validate_competitive_deck_manifest_v1(renamed).is_err());

    let mut token = manifest_raw();
    let token_id = mtg_kernel::card_def::CARD_DEFS
        .iter()
        .position(|definition| definition.is_token)
        .unwrap() as u16;
    token.configuration.sideboard = vec![MtgoCompetitiveDeckCardCountV1 {
        card_db_id: token_id,
        card_name: mtg_kernel::card_def::CARD_DEFS[usize::from(token_id)]
            .name
            .to_owned(),
        count: 2,
    }];
    assert!(validate_competitive_deck_manifest_v1(token).is_err());
}

#[test]
fn visible_snapshot_binds_exact_lifecycle_manifest_policy_and_inventory() {
    let manifest = validate_competitive_deck_manifest_v1(manifest_raw()).unwrap();
    let snapshot = checked_snapshot(&manifest, &starting_configuration(), 10);
    assert_eq!(snapshot.game_number(), 1);
    assert_eq!(snapshot.frame_sequence(), 10);
    assert_eq!(snapshot.snapshot_commitment_sha256().len(), 64);
    assert_eq!(snapshot.configuration_v1(), &starting_configuration());
    assert!(!snapshot.safe_for_live_input_v1());
    assert!(!snapshot.permits_sideboard_submission_v1());
}

#[test]
fn visible_snapshot_rejects_incomplete_stale_or_ambiguous_evidence() {
    let manifest = validate_competitive_deck_manifest_v1(manifest_raw()).unwrap();
    let lifecycle = validate_visible_competitive_lifecycle_snapshot_v1(lifecycle_raw(10)).unwrap();
    let baseline = snapshot_raw(&lifecycle, &manifest, &starting_configuration());

    let mut incomplete = baseline.clone();
    incomplete.visible_configuration_complete = false;
    assert!(validate_visible_competitive_sideboard_snapshot_v1(
        validate_visible_competitive_lifecycle_snapshot_v1(lifecycle_raw(10)).unwrap(),
        &manifest,
        incomplete,
    )
    .is_err());

    let mut wrong_policy = baseline.clone();
    wrong_policy.policy_deployment_commitment_sha256 = manifest.format_sha256().to_owned();
    assert!(validate_visible_competitive_sideboard_snapshot_v1(
        validate_visible_competitive_lifecycle_snapshot_v1(lifecycle_raw(10)).unwrap(),
        &manifest,
        wrong_policy,
    )
    .is_err());

    let mut missing = baseline.clone();
    missing.cards.pop();
    assert!(validate_visible_competitive_sideboard_snapshot_v1(
        validate_visible_competitive_lifecycle_snapshot_v1(lifecycle_raw(10)).unwrap(),
        &manifest,
        missing,
    )
    .is_err());

    let mut overlap = baseline;
    overlap.cards[1].rect_client_px = overlap.cards[0].rect_client_px.clone();
    assert!(validate_visible_competitive_sideboard_snapshot_v1(
        validate_visible_competitive_lifecycle_snapshot_v1(lifecycle_raw(10)).unwrap(),
        &manifest,
        overlap,
    )
    .is_err());
}

#[test]
fn visible_snapshot_rejects_crossed_zones_or_nonempty_drop_targets() {
    let manifest = validate_competitive_deck_manifest_v1(manifest_raw()).unwrap();

    for mutation in 0..4 {
        let lifecycle =
            validate_visible_competitive_lifecycle_snapshot_v1(lifecycle_raw(10)).unwrap();
        let mut candidate = snapshot_raw(&lifecycle, &manifest, &starting_configuration());
        match mutation {
            0 => candidate.sideboard_zone.rect_client_px.y = 300,
            1 => candidate.mainboard_zone.empty_drop_rect_client_px.x = 1_100,
            2 => {
                candidate.mainboard_zone.empty_drop_rect_client_px =
                    candidate.cards[0].rect_client_px.clone()
            }
            3 => candidate.cards[0].rect_client_px.y = 420,
            _ => unreachable!(),
        }
        assert!(validate_visible_competitive_sideboard_snapshot_v1(
            lifecycle, &manifest, candidate,
        )
        .is_err());
    }
}

#[test]
fn no_change_selection_is_valid_and_coordinate_free() {
    let manifest = validate_competitive_deck_manifest_v1(manifest_raw()).unwrap();
    let source = checked_snapshot(&manifest, &starting_configuration(), 10);
    let request = selection(&source, starting_configuration());
    let plan = validate_competitive_sideboard_selection_v1(source, request).unwrap();
    assert!(plan.no_changes_selected_v1());
    assert!(plan.transfers_v1().is_empty());
    assert!(!plan.safe_for_live_input_v1());
    assert!(!plan.permits_sideboard_submission_v1());
}

#[test]
fn changed_selection_derives_deterministic_inventory_conserving_transfers() {
    let manifest = validate_competitive_deck_manifest_v1(manifest_raw()).unwrap();
    let source = checked_snapshot(&manifest, &starting_configuration(), 10);
    let request = selection(&source, swapped_configuration());
    let plan = validate_competitive_sideboard_selection_v1(source, request).unwrap();
    assert!(!plan.no_changes_selected_v1());
    assert_eq!(plan.transfers_v1().len(), 2);
    assert_eq!(
        plan.transfers_v1()
            .iter()
            .map(|transfer| transfer.count)
            .sum::<u16>(),
        2
    );
    assert!(plan.transfers_v1().iter().any(|transfer| transfer.direction
        == MtgoCompetitiveSideboardTransferDirectionV1::MainboardToSideboard));
    assert!(plan.transfers_v1().iter().any(|transfer| transfer.direction
        == MtgoCompetitiveSideboardTransferDirectionV1::SideboardToMainboard));
    assert_eq!(plan.plan_commitment_sha256().len(), 64);
}

#[test]
fn selection_rejects_policy_source_and_inventory_substitution() {
    let manifest = validate_competitive_deck_manifest_v1(manifest_raw()).unwrap();
    let source = checked_snapshot(&manifest, &starting_configuration(), 10);
    let mut wrong_policy = selection(&source, swapped_configuration());
    wrong_policy.policy_deployment_commitment_sha256 = digest('6');
    assert!(validate_competitive_sideboard_selection_v1(source, wrong_policy).is_err());

    let source = checked_snapshot(&manifest, &starting_configuration(), 10);
    let mut wrong_source = selection(&source, swapped_configuration());
    wrong_source.source_snapshot_commitment_sha256 = digest('6');
    assert!(validate_competitive_sideboard_selection_v1(source, wrong_source).is_err());

    let source = checked_snapshot(&manifest, &starting_configuration(), 10);
    let mut gained = swapped_configuration();
    gained.mainboard[0].count += 1;
    let gained = selection(&source, gained);
    assert!(validate_competitive_sideboard_selection_v1(source, gained).is_err());
}

#[test]
fn ready_confirmation_requires_exact_target_on_changed_newer_frame() {
    let manifest = validate_competitive_deck_manifest_v1(manifest_raw()).unwrap();
    let source = checked_snapshot(&manifest, &starting_configuration(), 10);
    let request = selection(&source, swapped_configuration());
    let plan = validate_competitive_sideboard_selection_v1(source, request).unwrap();
    let confirmed = checked_snapshot(&manifest, &swapped_configuration(), 11);
    let ready = confirm_competitive_sideboard_target_visible_v1(plan, confirmed).unwrap();
    assert_eq!(ready.game_number(), 1);
    assert_eq!(ready.event_identity_sha256(), digest('3'));
    assert_eq!(ready.match_identity_sha256(), digest('4'));
    assert_eq!(
        ready.deck_manifest_commitment_sha256(),
        manifest.manifest_commitment_sha256()
    );
    assert_eq!(ready.policy_deployment_commitment_sha256(), digest('5'));
    assert_eq!(ready.after_frame_id(), 11);
    assert_eq!(ready.after_frame_sequence(), 11);
    assert_eq!(ready.after_frame_sha256(), format!("{:064x}", 11));
    assert_eq!(ready.ready_commitment_sha256().len(), 64);
    assert!(!ready.safe_for_live_input_v1());
    assert!(!ready.permits_sideboard_submission_v1());

    let source = checked_snapshot(&manifest, &starting_configuration(), 10);
    let request = selection(&source, swapped_configuration());
    let plan = validate_competitive_sideboard_selection_v1(source, request).unwrap();
    let wrong_target = checked_snapshot(&manifest, &starting_configuration(), 11);
    assert!(confirm_competitive_sideboard_target_visible_v1(plan, wrong_target).is_err());

    let source = checked_snapshot(&manifest, &starting_configuration(), 10);
    let request = selection(&source, swapped_configuration());
    let plan = validate_competitive_sideboard_selection_v1(source, request).unwrap();
    let stale = checked_snapshot(&manifest, &swapped_configuration(), 10);
    assert!(confirm_competitive_sideboard_target_visible_v1(plan, stale).is_err());
}
