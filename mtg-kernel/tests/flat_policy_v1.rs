use mtg_kernel::flat_policy_v1::{
    flat_action_ref_projection_role_id_v1, FlatCompletedDungeonV1, FlatContextPathElementV1,
    FlatDecisionBuffersV1, FlatDecisionEncoderV1, FlatDecisionErrorV1, FlatDecisionV1,
    FlatEffectSubtypeChangeV1, FlatObjectAbilityUseV1, FlatObjectCoreV1, FlatObjectGoadV1,
    FlatObjectGroupV1, FlatObjectSubtypeV1, FlatRelationRoleV1, FlatRelationV1,
    FLAT_ACTION_REF_INTERNAL_ROLE_WIDTH_V1, FLAT_ACTION_REF_INTERNAL_TO_PROJECTION_V1,
    FLAT_ACTION_REF_PROJECTION_ROLE_MAPPING_VERSION_V1, FLAT_ACTION_REF_PROJECTION_ROLE_WIDTH_V1,
    FLAT_POLICY_CONTEXT_SUBROLE_MAPPING_VERSION_V1, FLAT_POLICY_CONTRACT_DIGESTS_V1,
    FLAT_POLICY_ENUM_MAPPING_VERSION_V1, FLAT_POLICY_FEATURE_INVENTORY_VERSION_V1,
    FLAT_POLICY_OBJECT_GROUP_MAPPING_VERSION_V1, FLAT_POLICY_RELATION_ROLE_MAPPING_VERSION_V1,
    FLAT_POLICY_TYPED_LAYOUT_VERSION_V1,
};
use mtg_kernel::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, FlatActionCoreV1,
    FlatActionDecisionSliceBuffersV1, FlatActionKindV1, FlatActionObjectV1, FlatActionRefRoleV1,
    FlatActionRefV1, CANONICAL_BURN_DECK_ID, CANONICAL_RALLY_DECK_ID,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::mem::needs_drop;

#[derive(Clone)]
struct OwnedBuffers {
    objects: Vec<FlatObjectCoreV1>,
    relations: Vec<FlatRelationV1>,
    object_subtypes: Vec<FlatObjectSubtypeV1>,
    ability_uses: Vec<FlatObjectAbilityUseV1>,
    goads: Vec<FlatObjectGoadV1>,
    completed_dungeons: Vec<FlatCompletedDungeonV1>,
    effect_subtype_changes: Vec<FlatEffectSubtypeChangeV1>,
    context_path_elements: Vec<FlatContextPathElementV1>,
    actions: Vec<FlatActionCoreV1>,
    action_refs: Vec<FlatActionRefV1>,
    action_objects: Vec<FlatActionObjectV1>,
}

impl OwnedBuffers {
    fn ample() -> Self {
        Self {
            objects: vec![FlatObjectCoreV1::default(); 512],
            relations: vec![FlatRelationV1::default(); 2_048],
            object_subtypes: vec![FlatObjectSubtypeV1::default(); 2_048],
            ability_uses: vec![FlatObjectAbilityUseV1::default(); 512],
            goads: vec![FlatObjectGoadV1::default(); 512],
            completed_dungeons: vec![FlatCompletedDungeonV1::default(); 128],
            effect_subtype_changes: vec![FlatEffectSubtypeChangeV1::default(); 512],
            context_path_elements: vec![FlatContextPathElementV1::default(); 512],
            actions: vec![FlatActionCoreV1::default(); 128],
            action_refs: vec![FlatActionRefV1::default(); 1_024],
            action_objects: vec![FlatActionObjectV1::default(); 1_024],
        }
    }

    fn view(&mut self) -> FlatDecisionBuffersV1<'_> {
        FlatDecisionBuffersV1 {
            objects: &mut self.objects,
            relations: &mut self.relations,
            object_subtypes: &mut self.object_subtypes,
            ability_uses: &mut self.ability_uses,
            goads: &mut self.goads,
            completed_dungeons: &mut self.completed_dungeons,
            effect_subtype_changes: &mut self.effect_subtype_changes,
            context_path_elements: &mut self.context_path_elements,
            actions: &mut self.actions,
            action_refs: &mut self.action_refs,
            action_objects: &mut self.action_objects,
        }
    }
}

fn decision(session: &FastActorSessionV1) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(decision) = session.current_response() else {
        panic!("expected a live decision");
    };
    decision
}

fn encode(
    session: &FastActorSessionV1,
    expected: FastActorDecisionV1,
    encoder: &mut FlatDecisionEncoderV1,
    buffers: &mut OwnedBuffers,
) -> FlatDecisionV1 {
    session
        .encode_current_flat_decision_v1(expected, encoder, &mut buffers.view())
        .unwrap()
}

#[test]
fn complete_binding_and_actions_are_exactly_the_existing_action_slice() {
    let session = FastActorSessionV1::reset_with_limits(90_001, 11, 128, 16_384);
    let expected = decision(&session);
    let mut encoder = FlatDecisionEncoderV1::default();
    let mut full_buffers = OwnedBuffers::ample();
    let full = encode(&session, expected, &mut encoder, &mut full_buffers);

    assert_eq!(
        full.binding.typed_layout_version,
        FLAT_POLICY_TYPED_LAYOUT_VERSION_V1
    );
    assert_eq!(
        full.binding.feature_inventory_version,
        FLAT_POLICY_FEATURE_INVENTORY_VERSION_V1
    );
    assert_eq!(
        full.binding.enum_mapping_version,
        FLAT_POLICY_ENUM_MAPPING_VERSION_V1
    );
    assert_eq!(
        full.binding.object_group_mapping_version,
        FLAT_POLICY_OBJECT_GROUP_MAPPING_VERSION_V1
    );
    assert_eq!(
        full.binding.relation_role_mapping_version,
        FLAT_POLICY_RELATION_ROLE_MAPPING_VERSION_V1
    );
    assert_eq!(
        full.binding.context_subrole_mapping_version,
        FLAT_POLICY_CONTEXT_SUBROLE_MAPPING_VERSION_V1
    );
    assert_eq!(
        full.binding.action_ref_projection_role_mapping_version,
        FLAT_ACTION_REF_PROJECTION_ROLE_MAPPING_VERSION_V1
    );
    assert_eq!(
        full.binding.contract_digests,
        FLAT_POLICY_CONTRACT_DIGESTS_V1
    );
    assert!(full.active_object_count > 0);
    assert_eq!(full.active_action_count, expected.legal_action_count);

    let mut actions = vec![FlatActionCoreV1::default(); 128];
    let mut refs = vec![FlatActionRefV1::default(); 1_024];
    let mut action_objects = vec![FlatActionObjectV1::default(); 1_024];
    let action_slice = session
        .encode_current_flat_action_slice_v1(
            expected,
            &mut FlatActionDecisionSliceBuffersV1 {
                actions: &mut actions,
                refs: &mut refs,
                objects: &mut action_objects,
            },
        )
        .unwrap();
    assert_eq!(full.binding.action_binding, action_slice.binding);
    assert_eq!(
        &full_buffers.actions[..usize::try_from(full.active_action_count).unwrap()],
        &actions[..usize::try_from(action_slice.active_action_count).unwrap()]
    );
    assert_eq!(
        &full_buffers.action_refs[..usize::try_from(full.active_action_ref_count).unwrap()],
        &refs[..usize::try_from(action_slice.active_ref_count).unwrap()]
    );
    assert_eq!(
        &full_buffers.action_objects[..usize::try_from(full.active_action_object_count).unwrap()],
        &action_objects[..usize::from(action_slice.active_object_count)]
    );
}

#[test]
fn insufficient_first_table_capacity_publishes_nothing() {
    let session = FastActorSessionV1::reset_with_limits(90_002, 12, 128, 16_384);
    let expected = decision(&session);
    let mut encoder = FlatDecisionEncoderV1::default();
    let mut ample = OwnedBuffers::ample();
    let complete = encode(&session, expected, &mut encoder, &mut ample);
    let required = usize::try_from(complete.active_object_count).unwrap();
    assert!(required > 0);

    let mut short = OwnedBuffers::ample();
    short.objects.truncate(required - 1);
    let before = short.clone();
    let error = session
        .encode_current_flat_decision_v1(expected, &mut encoder, &mut short.view())
        .unwrap_err();
    assert_eq!(
        error,
        FlatDecisionErrorV1::InsufficientObjectCapacity {
            required,
            available: required - 1,
        }
    );
    assert_eq!(short.objects, before.objects);
    assert_eq!(short.relations, before.relations);
    assert_eq!(short.object_subtypes, before.object_subtypes);
    assert_eq!(short.ability_uses, before.ability_uses);
    assert_eq!(short.goads, before.goads);
    assert_eq!(short.completed_dungeons, before.completed_dungeons);
    assert_eq!(short.effect_subtype_changes, before.effect_subtype_changes);
    assert_eq!(short.context_path_elements, before.context_path_elements);
    assert_eq!(short.actions, before.actions);
    assert_eq!(short.action_refs, before.action_refs);
    assert_eq!(short.action_objects, before.action_objects);
}

#[test]
fn exact_snapshot_and_clone_reuse_are_identical_and_stale_expected_fails_closed() {
    let mut session = FastActorSessionV1::reset_with_limits(90_003, 13, 128, 16_384);
    let expected = decision(&session);
    let snapshot = session.snapshot_v1();
    let cloned = session.clone();
    let mut encoder = FlatDecisionEncoderV1::default();
    let mut first_buffers = OwnedBuffers::ample();
    let first = encode(&session, expected, &mut encoder, &mut first_buffers);

    let mut clone_buffers = OwnedBuffers::ample();
    let cloned_decision = encode(&cloned, expected, &mut encoder, &mut clone_buffers);
    assert_eq!(first, cloned_decision);
    assert_eq!(
        &first_buffers.objects[..usize::try_from(first.active_object_count).unwrap()],
        &clone_buffers.objects[..usize::try_from(first.active_object_count).unwrap()]
    );
    assert_eq!(
        &first_buffers.relations[..usize::try_from(first.active_relation_count).unwrap()],
        &clone_buffers.relations[..usize::try_from(first.active_relation_count).unwrap()]
    );

    session
        .consume_current_flat_action_slice_v1(first.binding.action_binding, 0)
        .unwrap();
    let mut poisoned = OwnedBuffers::ample();
    let before = poisoned.clone();
    let stale = session
        .encode_current_flat_decision_v1(expected, &mut encoder, &mut poisoned.view())
        .unwrap_err();
    assert!(matches!(stale, FlatDecisionErrorV1::Action(_)));
    assert_eq!(poisoned.objects, before.objects);
    assert_eq!(poisoned.actions, before.actions);

    session.restore_v1(&snapshot);
    let mut restored_buffers = OwnedBuffers::ample();
    let restored = encode(&session, expected, &mut encoder, &mut restored_buffers);
    assert_eq!(first, restored);
}

#[test]
fn generated_enum_inventory_and_privacy_shape_are_exact() {
    let golden: Value =
        serde_json::from_str(include_str!("../../data/flat_policy_v1/goldens_v1.json")).unwrap();
    let groups = golden["enum_maps"]["object_group"].as_object().unwrap();
    let expected_groups = [
        ("self_hand", FlatObjectGroupV1::SelfHand),
        ("self_battlefield", FlatObjectGroupV1::SelfBattlefield),
        (
            "opponent_battlefield",
            FlatObjectGroupV1::OpponentBattlefield,
        ),
        ("self_graveyard", FlatObjectGroupV1::SelfGraveyard),
        ("opponent_graveyard", FlatObjectGroupV1::OpponentGraveyard),
        ("exile", FlatObjectGroupV1::Exile),
        ("stack", FlatObjectGroupV1::Stack),
        ("combat", FlatObjectGroupV1::Combat),
        ("effect", FlatObjectGroupV1::ContinuousEffect),
        ("permission", FlatObjectGroupV1::Permission),
        ("attachment", FlatObjectGroupV1::Attachment),
        ("stack_target", FlatObjectGroupV1::HistoricalStackTarget),
        ("combat_block", FlatObjectGroupV1::CombatBlock),
        ("pending_context", FlatObjectGroupV1::PendingContext),
        ("private_context", FlatObjectGroupV1::PrivateContext),
        ("known_library_self", FlatObjectGroupV1::KnownSelfLibrary),
        (
            "known_library_opponent",
            FlatObjectGroupV1::KnownOpponentLibrary,
        ),
        ("known_hand_self", FlatObjectGroupV1::KnownSelfHand),
        ("known_hand_opponent", FlatObjectGroupV1::KnownOpponentHand),
        ("paid_cost", FlatObjectGroupV1::HistoricalPaidCost),
    ];
    for (name, variant) in expected_groups {
        assert_eq!(groups[name].as_u64(), Some(u64::from(variant as u8)));
    }
    let roles = golden["enum_maps"]["relation_role"].as_object().unwrap();
    let expected_roles = [
        ("attachment", FlatRelationRoleV1::Attachment),
        ("stack_target", FlatRelationRoleV1::StackTarget),
        ("combat_attacker", FlatRelationRoleV1::CombatAttacker),
        ("combat_blocker", FlatRelationRoleV1::CombatBlocker),
        ("effect_affected", FlatRelationRoleV1::EffectAffected),
        ("effect_source", FlatRelationRoleV1::EffectSource),
        ("permission", FlatRelationRoleV1::Permission),
        ("pending_context", FlatRelationRoleV1::PendingContext),
        ("private_context", FlatRelationRoleV1::PrivateContext),
        ("known_library", FlatRelationRoleV1::KnownLibrary),
        ("known_hand", FlatRelationRoleV1::KnownHand),
        ("attached_to", FlatRelationRoleV1::AttachedTo),
        ("exiled_by", FlatRelationRoleV1::ExiledBy),
        ("paid_cost", FlatRelationRoleV1::PaidCost),
    ];
    for (name, variant) in expected_roles {
        assert_eq!(roles[name].as_u64(), Some(u64::from(variant as u8)));
    }

    // Public model rows own no heap-backed string or raw-name storage.  The
    // independent PR27 action-object table is operational and intentionally
    // outside this assertion.
    assert!(!needs_drop::<FlatObjectCoreV1>());
    assert!(!needs_drop::<FlatRelationV1>());
    assert!(!needs_drop::<FlatDecisionV1>());
    let inventory: Value = serde_json::from_str(include_str!(
        "../../data/flat_policy_v1/feature_inventory_v1.json"
    ))
    .unwrap();
    for entry in inventory["entries"].as_array().unwrap() {
        if entry["classification"] == "forbidden" {
            assert_eq!(entry["destination"], "absent");
        }
    }
}

fn digest_from_json(value: &Value) -> [u8; 32] {
    let text = value.as_str().expect("digest is a JSON string");
    assert_eq!(text.len(), 64);
    let mut digest = [0_u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).unwrap();
    }
    digest
}

#[test]
fn compiled_contract_digests_and_action_ref_projection_crosswalk_are_exact() {
    let golden: Value =
        serde_json::from_str(include_str!("../../data/flat_policy_v1/goldens_v1.json")).unwrap();
    let inventory: Value = serde_json::from_str(include_str!(
        "../../data/flat_policy_v1/feature_inventory_v1.json"
    ))
    .unwrap();
    assert_eq!(
        FLAT_POLICY_CONTRACT_DIGESTS_V1.mapping_sha256,
        digest_from_json(&golden["mapping_sha256"])
    );
    assert_eq!(
        FLAT_POLICY_CONTRACT_DIGESTS_V1.feature_inventory_sha256,
        digest_from_json(&golden["inventory_sha256"])
    );
    assert_eq!(
        FLAT_POLICY_CONTRACT_DIGESTS_V1.typed_layout_sha256,
        digest_from_json(&inventory["rust_typed_layout_sha256"])
    );

    let roles = [
        (FlatActionRefRoleV1::Source, "source", 0_u8),
        (FlatActionRefRoleV1::Candidate, "candidate", 1),
        (FlatActionRefRoleV1::Card, "card", 2),
        (FlatActionRefRoleV1::Attacker, "attacker", 3),
        (FlatActionRefRoleV1::Blocker, "blocker", 4),
        (FlatActionRefRoleV1::TargetObject, "target_object", 5),
        (FlatActionRefRoleV1::Cards, "cards", 6),
        (FlatActionRefRoleV1::PendingSources, "pending_sources", 9),
    ];
    assert_eq!(
        usize::from(FLAT_ACTION_REF_INTERNAL_ROLE_WIDTH_V1),
        roles.len()
    );
    assert_eq!(FLAT_ACTION_REF_PROJECTION_ROLE_WIDTH_V1, 10);
    assert_eq!(
        FLAT_ACTION_REF_INTERNAL_TO_PROJECTION_V1,
        [0, 1, 2, 3, 4, 5, 6, 9]
    );
    let entries = golden["action_ref_role_crosswalk"]["entries"]
        .as_array()
        .unwrap();
    assert_eq!(entries.len(), roles.len());
    let projection_map = golden["enum_maps"]["action_ref_role"].as_object().unwrap();
    let mut occupied_projection_ids = [false; 10];
    for (internal_id, ((role, name, expected_projection_id), entry)) in
        roles.into_iter().zip(entries).enumerate()
    {
        assert_eq!(u8::try_from(internal_id).unwrap(), role as u8);
        assert_eq!(entry["role"], name);
        assert_eq!(entry["rust_internal_id"], internal_id);
        assert_eq!(
            entry["python_projection_id"].as_u64(),
            Some(u64::from(expected_projection_id))
        );
        assert_eq!(
            projection_map[name].as_u64(),
            Some(u64::from(expected_projection_id))
        );
        assert_eq!(
            flat_action_ref_projection_role_id_v1(role),
            expected_projection_id
        );
        assert!(!occupied_projection_ids[usize::from(expected_projection_id)]);
        occupied_projection_ids[usize::from(expected_projection_id)] = true;
    }
    assert!(!occupied_projection_ids[7]);
    assert!(!occupied_projection_ids[8]);
    assert_eq!(
        golden["action_ref_role_crosswalk"]["projection_only"],
        serde_json::json!([
            {"role": "attackers", "python_projection_id": 7},
            {"role": "blockers", "python_projection_id": 8},
        ])
    );
}

fn next_splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[test]
fn burn_and_rally_randomized_rollouts_encode_every_reached_decision() {
    for (case, deck_id) in [CANONICAL_BURN_DECK_ID, CANONICAL_RALLY_DECK_ID]
        .into_iter()
        .enumerate()
    {
        let episode_id = 90_100 + u64::try_from(case).unwrap();
        let mut session = FastActorSessionV1::reset_with_decks_and_limits(
            episode_id,
            81_701 + u64::try_from(case).unwrap(),
            4_096,
            524_288,
            [deck_id.to_string(), deck_id.to_string()],
        )
        .unwrap();
        let mut encoder = FlatDecisionEncoderV1::default();
        let mut buffers = OwnedBuffers::ample();
        let mut rng = 0xA66A_E6A7_E000_0005_u64 ^ episode_id;
        let mut encoded_count = 0_usize;
        let mut saw_relation = false;
        for _ in 0..16_384 {
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                break;
            };
            let encoded = encode(&session, expected, &mut encoder, &mut buffers);
            encoded_count += 1;
            saw_relation |= encoded.active_relation_count > 0;
            let object_count = encoded.active_object_count;
            for relation in
                &buffers.relations[..usize::try_from(encoded.active_relation_count).unwrap()]
            {
                assert!(relation
                    .source_object
                    .is_none_or(|index| index < object_count));
                assert!(relation
                    .target_object
                    .is_none_or(|index| index < object_count));
            }
            let action_count = usize::try_from(encoded.active_action_count).unwrap();
            let include_true = action_count >= 2
                && matches!(
                    buffers.actions[1].kind,
                    FlatActionKindV1::ChooseAttackerInclusion
                        | FlatActionKindV1::ChooseBlockerInclusion
                );
            let selected = if include_true {
                1
            } else {
                usize::try_from(next_splitmix(&mut rng) % u64::try_from(action_count).unwrap())
                    .unwrap()
            };
            if matches!(
                session
                    .consume_current_flat_action_slice_v1(
                        encoded.binding.action_binding,
                        u32::try_from(selected).unwrap(),
                    )
                    .unwrap(),
                FastActorResponseV1::Terminal(_)
            ) {
                break;
            }
        }
        assert!(encoded_count >= 32, "{deck_id}: {encoded_count}");
        assert!(
            saw_relation,
            "{deck_id} rollout never reached a relation row"
        );
    }
}

fn sha256_debug<T: std::fmt::Debug + ?Sized>(value: &T) -> String {
    format!("{:x}", Sha256::digest(format!("{value:?}").as_bytes()))
}

#[test]
fn runtime_typed_row_count_and_digest_goldens_are_exact() {
    let golden: Value =
        serde_json::from_str(include_str!("../../data/flat_policy_v1/goldens_v1.json")).unwrap();
    let fixture_case = |name: &str| {
        golden["runtime_fixture_cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap()
    };
    let mut digest_mismatches = Vec::new();
    for (name, decks, seed, episode_id) in [
        ("burn_seed_11_initial", ["Burn", "Burn"], 11, 90_001),
        ("rally_seed_23_initial", ["Rally", "Rally"], 23, 90_002),
        ("burn_rally_seed_37_initial", ["Burn", "Rally"], 37, 90_003),
    ] {
        let session = FastActorSessionV1::reset_with_decks_and_limits(
            episode_id,
            seed,
            128,
            16_384,
            decks.map(str::to_string),
        )
        .unwrap();
        let expected = decision(&session);
        let mut encoder = FlatDecisionEncoderV1::default();
        let mut buffers = OwnedBuffers::ample();
        let encoded = encode(&session, expected, &mut encoder, &mut buffers);
        let model_digest = sha256_debug(&(
            encoded,
            &buffers.objects[..usize::try_from(encoded.active_object_count).unwrap()],
            &buffers.relations[..usize::try_from(encoded.active_relation_count).unwrap()],
            &buffers.object_subtypes
                [..usize::try_from(encoded.active_object_subtype_count).unwrap()],
            &buffers.ability_uses[..usize::try_from(encoded.active_ability_use_count).unwrap()],
            &buffers.goads[..usize::try_from(encoded.active_goad_count).unwrap()],
            &buffers.completed_dungeons
                [..usize::try_from(encoded.active_completed_dungeon_count).unwrap()],
            &buffers.effect_subtype_changes
                [..usize::try_from(encoded.active_effect_subtype_change_count).unwrap()],
            &buffers.context_path_elements
                [..usize::try_from(encoded.active_context_path_element_count).unwrap()],
            &buffers.actions[..usize::try_from(encoded.active_action_count).unwrap()],
            &buffers.action_refs[..usize::try_from(encoded.active_action_ref_count).unwrap()],
        ));
        let operational_digest = sha256_debug(
            &buffers.action_objects[..usize::try_from(encoded.active_action_object_count).unwrap()],
        );
        let counts = [
            encoded.active_object_count,
            encoded.active_relation_count,
            encoded.active_object_subtype_count,
            encoded.active_ability_use_count,
            encoded.active_goad_count,
            encoded.active_completed_dungeon_count,
            encoded.active_effect_subtype_change_count,
            encoded.active_context_path_element_count,
            encoded.active_action_count,
            encoded.active_action_ref_count,
            encoded.active_action_object_count,
        ];
        let fixture = fixture_case(name);
        assert_eq!(fixture["counts"], serde_json::json!(counts));
        let expected_model_digest = fixture["model_typed_debug_sha256"].as_str().unwrap();
        if expected_model_digest != model_digest {
            digest_mismatches.push(format!(
                "{name}: expected {expected_model_digest}, actual {model_digest}"
            ));
        }
        assert_eq!(
            fixture["action_objects_operational_debug_sha256"],
            operational_digest
        );
    }
    for (case_index, deck_id) in ["Burn", "Rally"].into_iter().enumerate() {
        let episode_id = 90_101 + u64::try_from(case_index).unwrap();
        let mut session = FastActorSessionV1::reset_with_decks_and_limits(
            episode_id,
            81_701 + u64::try_from(case_index).unwrap(),
            4_096,
            524_288,
            [deck_id.to_string(), deck_id.to_string()],
        )
        .unwrap();
        let mut encoder = FlatDecisionEncoderV1::default();
        let mut buffers = OwnedBuffers::ample();
        let mut rng = 0xA66A_E6A7_E000_0005_u64 ^ episode_id;
        for decision_index in 0..16_384_u32 {
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("{deck_id} terminated before a relation");
            };
            let encoded = encode(&session, expected, &mut encoder, &mut buffers);
            if encoded.active_relation_count > 0 {
                let model_digest = sha256_debug(&(
                    encoded,
                    &buffers.objects[..usize::try_from(encoded.active_object_count).unwrap()],
                    &buffers.relations[..usize::try_from(encoded.active_relation_count).unwrap()],
                    &buffers.object_subtypes
                        [..usize::try_from(encoded.active_object_subtype_count).unwrap()],
                    &buffers.ability_uses
                        [..usize::try_from(encoded.active_ability_use_count).unwrap()],
                    &buffers.goads[..usize::try_from(encoded.active_goad_count).unwrap()],
                    &buffers.completed_dungeons
                        [..usize::try_from(encoded.active_completed_dungeon_count).unwrap()],
                    &buffers.effect_subtype_changes
                        [..usize::try_from(encoded.active_effect_subtype_change_count).unwrap()],
                    &buffers.context_path_elements
                        [..usize::try_from(encoded.active_context_path_element_count).unwrap()],
                    &buffers.actions[..usize::try_from(encoded.active_action_count).unwrap()],
                    &buffers.action_refs
                        [..usize::try_from(encoded.active_action_ref_count).unwrap()],
                ));
                let name = format!(
                    "{}_seed_{}_first_relation",
                    deck_id.to_lowercase(),
                    81_701 + u64::try_from(case_index).unwrap()
                );
                let counts = [
                    encoded.active_object_count,
                    encoded.active_relation_count,
                    encoded.active_object_subtype_count,
                    encoded.active_ability_use_count,
                    encoded.active_goad_count,
                    encoded.active_completed_dungeon_count,
                    encoded.active_effect_subtype_change_count,
                    encoded.active_context_path_element_count,
                    encoded.active_action_count,
                    encoded.active_action_ref_count,
                    encoded.active_action_object_count,
                ];
                let fixture = fixture_case(&name);
                assert_eq!(fixture["decision_index"], decision_index);
                assert_eq!(fixture["counts"], serde_json::json!(counts));
                let expected_model_digest = fixture["model_typed_debug_sha256"].as_str().unwrap();
                if expected_model_digest != model_digest {
                    digest_mismatches.push(format!(
                        "{name}: expected {expected_model_digest}, actual {model_digest}"
                    ));
                }
                break;
            }
            let action_count = usize::try_from(encoded.active_action_count).unwrap();
            let include_true = action_count >= 2
                && matches!(
                    buffers.actions[1].kind,
                    FlatActionKindV1::ChooseAttackerInclusion
                        | FlatActionKindV1::ChooseBlockerInclusion
                );
            let selected = if include_true {
                1
            } else {
                usize::try_from(next_splitmix(&mut rng) % u64::try_from(action_count).unwrap())
                    .unwrap()
            };
            session
                .consume_current_flat_action_slice_v1(
                    encoded.binding.action_binding,
                    u32::try_from(selected).unwrap(),
                )
                .unwrap();
        }
    }
    assert!(
        digest_mismatches.is_empty(),
        "runtime typed digest mismatches:\n{}",
        digest_mismatches.join("\n")
    );
}
