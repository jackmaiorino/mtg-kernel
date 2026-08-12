use mtg_kernel::rl::{
    ActionSemanticV1, CardCharacteristicsV2, CardPublicV2, CardTypeFlagsV2, CountersV1,
    KeywordFlagsV2,
};
use mtg_kernel::rl_session::{RlEpisodeSessionV1, RlSessionResponseV1};
use mtg_kernel::state::Zone;
use mtgo_blackbox_v1::*;
use serde_json::Value;

fn digest(character: char) -> String {
    std::iter::repeat_n(character, 64).collect()
}

fn valid_record() -> MtgoObservedDecisionV1 {
    let decision = (1..=128)
        .find_map(|seed| {
            let session = RlEpisodeSessionV1::reset_with_limits(7, seed, 128, 16_384);
            let RlSessionResponseV1::Decision(decision) = session.current_response() else {
                return None;
            };
            let pass = decision
                .legal_actions
                .iter()
                .find(|action| matches!(action.semantic, ActionSemanticV1::Pass { .. }))?
                .semantic
                .clone();
            let land = decision
                .legal_actions
                .iter()
                .find(|action| matches!(action.semantic, ActionSemanticV1::PlayLand { .. }))?
                .semantic
                .clone();
            Some((decision, pass, land))
        })
        .expect("one deterministic opening exposes Pass and PlayLand");
    let source = match &decision.2 {
        ActionSemanticV1::PlayLand { source, .. } => source.clone(),
        _ => unreachable!(),
    };
    let payload = MtgoSemanticDecisionPayloadV1 {
        observation: (*decision.0.observation).clone(),
        legal_actions: vec![decision.1, decision.2],
        object_bindings: vec![MtgoObjectBindingV1 {
            adapter_object_id: "hand:visible-slot-0".to_owned(),
            kernel_ref: source,
        }],
    };
    let provenance = payload_leaf_inventory_v1(&payload)
        .unwrap()
        .into_iter()
        .filter(|leaf| leaf.requires_visible_evidence)
        .map(|leaf| MtgoLeafProvenanceV1 {
            json_pointer: leaf.json_pointer,
            value_sha256: leaf.value_sha256,
            evidence_ids: vec![20],
            confidence_bps: 10_000,
        })
        .collect();
    MtgoObservedDecisionV1 {
        schema_version: MTGO_OBSERVED_DECISION_SCHEMA_V1,
        decision_id: "visible-duel-input-test".to_owned(),
        frame_id: 1,
        payload: payload.clone(),
        frames: vec![MtgoMockFrameV1 {
            frame_id: 1,
            sequence: 1,
            sha256: digest('a'),
            client_bounds: MtgoRectPxV1 {
                x: 0,
                y: 0,
                width: 1_920,
                height: 1_080,
            },
        }],
        evidence: vec![
            MtgoVisibleEvidenceV1 {
                evidence_id: 10,
                sequence: 1,
                source: MtgoEvidenceSourceV1::FrameRegion {
                    frame_id: 1,
                    rect: MtgoRectPxV1 {
                        x: 0,
                        y: 0,
                        width: 1_920,
                        height: 1_080,
                    },
                    content_sha256: digest('b'),
                },
            },
            MtgoVisibleEvidenceV1 {
                evidence_id: 20,
                sequence: 2,
                source: MtgoEvidenceSourceV1::DerivedPublicFact {
                    parent_evidence_ids: vec![10],
                    derivation: MtgoPublicDerivationV1::PublicStateProjection,
                },
            },
        ],
        provenance,
        local_metadata_sha256: local_metadata_commitment_v1(&payload).unwrap(),
        readiness: MtgoDecisionReadinessV1 {
            observation_complete: true,
            legal_action_set_complete: true,
            client_prompt_reconciled: true,
        },
    }
}

fn refresh_record_commitments(record: &mut MtgoObservedDecisionV1) {
    record.payload.observation.visible_projection_hash =
        compute_visible_projection_hash_v5_v1(&record.payload.observation).unwrap();
    record.provenance = payload_leaf_inventory_v1(&record.payload)
        .unwrap()
        .into_iter()
        .filter(|leaf| leaf.requires_visible_evidence)
        .map(|leaf| MtgoLeafProvenanceV1 {
            json_pointer: leaf.json_pointer,
            value_sha256: leaf.value_sha256,
            evidence_ids: vec![20],
            confidence_bps: 10_000,
        })
        .collect();
    record.local_metadata_sha256 = local_metadata_commitment_v1(&record.payload).unwrap();
}

fn assert_no_forbidden_keys(value: &Value) {
    match value {
        Value::Object(fields) => {
            for (key, child) in fields {
                assert!(
                    !matches!(
                        key.as_str(),
                        "arena_id"
                            | "card_db_id"
                            | "zone_change_count"
                            | "adapter_object_id"
                            | "engine_context"
                            | "surface_context"
                            | "policy_surface_context"
                    ),
                    "forbidden field leaked: {key}"
                );
                assert_no_forbidden_keys(child);
            }
        }
        Value::Array(values) => {
            for child in values {
                assert_no_forbidden_keys(child);
            }
        }
        _ => {}
    }
}

fn other_player(player: PlayerSeatV1) -> PlayerSeatV1 {
    match player {
        PlayerSeatV1::P0 => PlayerSeatV1::P1,
        PlayerSeatV1::P1 => PlayerSeatV1::P0,
    }
}

fn plotted_exile_card(
    owner: PlayerSeatV1,
    arena_id: u32,
    card_db_id: u16,
    card_name: &str,
) -> CardPublicV2 {
    CardPublicV2 {
        stable: CardStableRefV1 {
            arena_id,
            card_db_id,
            owner,
            controller: owner,
            zone: Zone::Exile,
            zone_change_count: 1,
        },
        card_name: card_name.to_owned(),
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: CountersV1 {
            plus1_plus1: 0,
            minus1_minus1: 0,
            minus0_minus1: 0,
            stun: 0,
            lore: 0,
        },
        attachments: Vec::new(),
        plotted_turn: Some(1),
        is_token: false,
        face_index: 0,
        chosen_color: None,
        entered_battlefield_turn: None,
        ability_uses_this_turn: Vec::new(),
        skip_next_untap: false,
        goaded_by: Vec::new(),
        characteristics: CardCharacteristicsV2 {
            type_flags: CardTypeFlagsV2 {
                land: false,
                creature: false,
                instant: false,
                sorcery: true,
                artifact: false,
                enchantment: false,
            },
            base_power: None,
            base_toughness: None,
            effective_power: None,
            effective_toughness: None,
            effective_color_mask: 0,
            effective_subtype_ids: Vec::new(),
            effective_keywords: KeywordFlagsV2 {
                flying: false,
                reach: false,
                haste: false,
                vigilance: false,
                trample: false,
                first_strike: false,
                double_strike: false,
                deathtouch: false,
                menace: false,
                defender: false,
                lifelink: false,
                hexproof: false,
                indestructible: false,
                protection_from_monocolored: false,
                ward_generic: 0,
                minimum_blockers: 0,
                landwalk_mask: 0,
            },
        },
    }
}

#[test]
fn validated_decision_projects_to_owned_visible_state_and_ordered_actions() {
    let decision = validate_observed_decision_v1(valid_record()).unwrap();
    let input = build_player_visible_duel_decision_input_v1(&decision).unwrap();

    assert_eq!(
        input.current_state.acting_player,
        decision.observation().acting_player
    );
    assert_eq!(
        input.current_state.turn,
        decision.observation().projection.surface.turn
    );
    assert_eq!(
        input.ordered_legal_actions.len(),
        decision.legal_actions().len()
    );
    assert_eq!(input.commitment_sha256_v1().unwrap().len(), 64);
    assert!(!input.safe_for_live_input_v1());
    assert!(!input.permits_event_entry_v1());
    assert!(!input.permits_spending_v1());

    let json = serde_json::to_value(&input).unwrap();
    assert_no_forbidden_keys(&json);
    for forbidden in [
        "schema_version",
        "kernel_version",
        "card_db_hash",
        "step_index",
        "physical_decision_id",
        "visible_projection_hash",
        "source_frame",
        "decision_commitment",
    ] {
        assert!(
            !json.to_string().contains(forbidden),
            "forbidden field leaked: {forbidden}"
        );
    }
}

#[test]
fn kernel_bookkeeping_and_transport_metadata_do_not_change_visible_input() {
    let original = validate_observed_decision_v1(valid_record()).unwrap();
    let original_input = build_player_visible_duel_decision_input_v1(&original).unwrap();

    let mut changed = valid_record();
    changed.payload.observation.step_index += 100;
    changed.payload.observation.physical_decision_id += 100;
    changed
        .payload
        .observation
        .projection
        .surface
        .engine_context
        .priority_passes = [true, true];
    changed
        .payload
        .observation
        .projection
        .surface
        .surface_context
        .combat_priority_spent = [true, true];
    refresh_record_commitments(&mut changed);
    let changed = validate_observed_decision_v1(changed).unwrap();
    let changed_input = build_player_visible_duel_decision_input_v1(&changed).unwrap();

    assert_eq!(changed_input, original_input);
    assert_eq!(
        changed_input.commitment_sha256_v1().unwrap(),
        original_input.commitment_sha256_v1().unwrap()
    );
}

#[test]
fn player_visible_fact_or_legal_action_change_changes_commitment() {
    let original = validate_observed_decision_v1(valid_record()).unwrap();
    let original_input = build_player_visible_duel_decision_input_v1(&original).unwrap();

    let mut changed_fact = original_input.clone();
    changed_fact.current_state.life_totals[0] -= 1;
    assert_ne!(
        changed_fact.commitment_sha256_v1().unwrap(),
        original_input.commitment_sha256_v1().unwrap()
    );

    let mut changed_actions = original_input.clone();
    changed_actions.ordered_legal_actions.reverse();
    assert_ne!(
        changed_actions.commitment_sha256_v1().unwrap(),
        original_input.commitment_sha256_v1().unwrap()
    );
}

#[test]
fn opponent_plotted_card_name_is_concealed_while_own_name_remains_visible() {
    let mut record = valid_record();
    let actor = record.payload.observation.acting_player;
    record.payload.observation.projection.surface.exile.extend([
        plotted_exile_card(actor, 9_001, 601, "Own Plotted Card"),
        plotted_exile_card(other_player(actor), 9_002, 602, "Opponent Secret Card"),
    ]);
    refresh_record_commitments(&mut record);

    let decision = validate_observed_decision_v1(record).unwrap();
    let input = build_player_visible_duel_decision_input_v1(&decision).unwrap();
    let own = input.current_state.exile.len() - 2;
    let opponent = own + 1;

    assert_eq!(
        input.current_state.exile[own].visible_card_name.as_deref(),
        Some("Own Plotted Card")
    );
    assert_eq!(input.current_state.exile[opponent].visible_card_name, None);
    assert!(!serde_json::to_string(&input)
        .unwrap()
        .contains("Opponent Secret Card"));
}

#[test]
fn kernel_choice_indices_do_not_cross_the_player_visible_action_boundary() {
    let mut first = valid_record();
    let actor = first.payload.observation.acting_player;
    let source = first.payload.object_bindings[0].kernel_ref.clone();
    first.payload.legal_actions = vec![
        ActionSemanticV1::ActivateAbility {
            actor,
            source: source.clone(),
            ability_index: 7,
        },
        ActionSemanticV1::ActivateAbility {
            actor,
            source,
            ability_index: 9,
        },
    ];
    refresh_record_commitments(&mut first);

    let mut second = first.clone();
    let ActionSemanticV1::ActivateAbility { ability_index, .. } =
        &mut second.payload.legal_actions[0]
    else {
        unreachable!()
    };
    *ability_index = 42;
    let ActionSemanticV1::ActivateAbility { ability_index, .. } =
        &mut second.payload.legal_actions[1]
    else {
        unreachable!()
    };
    *ability_index = 43;
    refresh_record_commitments(&mut second);

    let first =
        build_player_visible_duel_decision_input_v1(&validate_observed_decision_v1(first).unwrap())
            .unwrap();
    let second = build_player_visible_duel_decision_input_v1(
        &validate_observed_decision_v1(second).unwrap(),
    )
    .unwrap();

    assert_eq!(first, second);
    assert_eq!(
        serde_json::to_value(&first).unwrap()["ordered_legal_actions"][0]["visible_choice_ordinal"],
        0
    );
    assert_eq!(
        serde_json::to_value(&first).unwrap()["ordered_legal_actions"][1]["visible_choice_ordinal"],
        1
    );
    assert!(!serde_json::to_string(&first)
        .unwrap()
        .contains("ability_index"));
}
