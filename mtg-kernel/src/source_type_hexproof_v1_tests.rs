use super::*;
use crate::card_def::card_id_by_name;
use crate::engine::{self, Action, Decision};
use crate::ids::StackItemId;
use crate::state::{GameObject, ObjectStateV4, SpellCopyOriginV4, Step};

#[test]
fn instant_rule_reads_effective_type_set_instead_of_name_color_or_other_types() {
    for (types, restricted) in [
        (&[CardType::Instant][..], true),
        (&[CardType::Instant, CardType::Creature][..], true),
        (&[CardType::Sorcery][..], false),
        (&[CardType::Creature][..], false),
        (&[][..], false),
    ] {
        let facts = SourceTypesV1::from_types(types);
        assert!(facts.valid());
        assert_eq!(
            protects(SourceTypeHexproofDefV1::Instants, facts),
            restricted
        );
        assert_eq!(facts.card_types().len(), types.len());
        assert_eq!(
            facts,
            serde_json::from_slice(&serde_json::to_vec(&facts).unwrap()).unwrap()
        );
    }
    assert!(!SourceTypesV1(0x80).valid());
}

#[test]
fn empty_projection_preserves_legacy_copy_origin_bytes_and_hash() {
    #[derive(Serialize, Hash)]
    struct LegacyOrigin {
        parent: ObjectId,
        parent_card_def: u16,
        parent_owner: PlayerId,
        parent_controller: PlayerId,
        parent_stack_zone_change_count: u32,
        parent_was_copy: bool,
    }
    let legacy = LegacyOrigin {
        parent: ObjectId(1),
        parent_card_def: 2,
        parent_owner: PlayerId::P0,
        parent_controller: PlayerId::P1,
        parent_stack_zone_change_count: 3,
        parent_was_copy: false,
    };
    let current = SpellCopyOriginV4 {
        parent: legacy.parent,
        parent_card_def: legacy.parent_card_def,
        parent_owner: legacy.parent_owner,
        parent_controller: legacy.parent_controller,
        parent_stack_zone_change_count: legacy.parent_stack_zone_change_count,
        parent_was_copy: legacy.parent_was_copy,
        source_types_v1: Default::default(),
    };
    assert_eq!(
        serde_json::to_vec(&legacy).unwrap(),
        serde_json::to_vec(&current).unwrap()
    );
    let hash = |value: &dyn Fn(&mut std::collections::hash_map::DefaultHasher)| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        value(&mut hasher);
        hasher.finish()
    };
    assert_eq!(hash(&|h| legacy.hash(h)), hash(&|h| current.hash(h)));
    let restored: SpellCopyOriginV4 =
        serde_json::from_slice(&serde_json::to_vec(&legacy).unwrap()).unwrap();
    assert_eq!(current, restored);
    let projected = SpellCopyOriginV4 {
        source_types_v1: SourceTypeProjectionV1(Some(SourceTypesV1::from_types(&[
            CardType::Instant,
        ]))),
        ..current
    };
    assert_ne!(hash(&|h| projected.hash(h)), hash(&|h| current.hash(h)));
    assert_eq!(
        projected,
        serde_json::from_slice(&serde_json::to_vec(&projected).unwrap()).unwrap()
    );
}

fn ready() -> GameState {
    let island = card_id_by_name("Island").unwrap();
    let mut state =
        GameState::new_from_libraries(&[island; 40], &[island; 40], |_| "Island".into(), 110119);
    state.step = Step::Main1;
    state
}
fn put(state: &mut GameState, name: &str) -> ObjectId {
    let card_def = card_id_by_name(name).expect("actual registered printing required");
    let owner = PlayerId::P0;
    let source = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[usize::from(card_def)].object_name.into(),
        owner,
        controller: owner,
        zone: Zone::Hand,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    state.players[0].hand.push(source);
    source
}
fn paid_form(state: &mut GameState, name: &str, form: u8) -> (ObjectId, StackItemId) {
    let source = put(state, name);
    state.players[0].mana_pool = [5; 6];
    assert!(
        matches!(engine::advance_until_decision(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&source))
    );
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(
        matches!(engine::advance_until_decision(state), Decision::ChooseSpellMode { spell, .. } if spell == source)
    );
    engine::step(state, Action::ChooseSpellMode(form)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass { .. }
    ));
    let item = state
        .stack
        .iter()
        .find(|item| item.source == source)
        .unwrap();
    engine::validate_spell_stack_source(state, item).unwrap();
    assert!(
        state.players[0].mana_pool.iter().copied().sum::<u8>() < 30,
        "real mana payment required"
    );
    (source, item.v4.stack_item_id)
}

#[test]
fn paid_sorcery_forms_preserve_catalog_selected_copy_contract_through_departure() {
    for name in ["Fang Dragon", "Sagu Wildling"] {
        for form in [0, 1] {
            let mut state = ready();
            let (source, parent_id) = paid_form(&mut state, name, form);
            let types = current_types(&state, source).unwrap();
            assert!(!types.contains(CardType::Instant));
            assert_eq!(types.contains(CardType::Sorcery), form == 1);
            assert_eq!(types.contains(CardType::Creature), form == 0);
            let (copy_source, copy_id) =
                engine::copy_spell_for(&mut state, source, PlayerId::P1).unwrap();
            let copy_types = if enabled() {
                types
            } else {
                spell_types(
                    &CARD_DEFS[usize::from(state.objects.get(source).card_def)],
                    CastMethodV4::Normal,
                )
                .unwrap()
            };
            assert_eq!(current_types(&state, copy_source).unwrap(), copy_types);
            engine::counter_stack_item_by_id(&mut state, parent_id)
                .unwrap()
                .unwrap();
            assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
            let copy = state
                .stack
                .iter()
                .find(|item| item.v4.stack_item_id == copy_id)
                .unwrap();
            engine::validate_spell_stack_source(&state, copy).unwrap();
            assert_eq!(current_types(&state, copy_source).unwrap(), copy_types);
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(current_types(&restored, copy_source).unwrap(), copy_types);
            assert_eq!(state.state_hash(), restored.state_hash());
        }
    }
}

#[test]
#[cfg(feature = "standard-magezero-fixtures")]
fn paid_instant_adventure_is_distinct_while_absent_catalog_copy_remains_normal() {
    for form in [0, 1] {
        let mut state = ready();
        let (source, parent_id) = paid_form(&mut state, "Questing Druid", form);
        let types = current_types(&state, source).unwrap();
        assert_eq!(types.contains(CardType::Instant), form == 1);
        assert_eq!(types.contains(CardType::Creature), form == 0);
        assert_eq!(
            protects(SourceTypeHexproofDefV1::Instants, types),
            form == 1
        );
        let (copy_source, copy_id) =
            engine::copy_spell_for(&mut state, source, PlayerId::P1).unwrap();
        let copy_types = if enabled() {
            types
        } else {
            spell_types(
                &CARD_DEFS[usize::from(state.objects.get(source).card_def)],
                CastMethodV4::Normal,
            )
            .unwrap()
        };
        engine::counter_stack_item_by_id(&mut state, parent_id)
            .unwrap()
            .unwrap();
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for state in [&state, &restored] {
            let copy = state
                .stack
                .iter()
                .find(|item| item.v4.stack_item_id == copy_id)
                .unwrap();
            engine::validate_spell_stack_source(state, copy).unwrap();
            assert_eq!(current_types(state, copy_source).unwrap(), copy_types);
        }
    }
}

#[test]
fn every_cast_method_preserves_absent_mode_and_selected_alternative_semantics() {
    for method in [
        CastMethodV4::Normal,
        CastMethodV4::Alternative,
        CastMethodV4::Flashback,
        CastMethodV4::Madness,
        CastMethodV4::Plotted,
        CastMethodV4::Escape,
        CastMethodV4::Bestow,
        CastMethodV4::Omen,
    ] {
        assert_eq!(
            copy_method_for_selection(method, false),
            CastMethodV4::Normal
        );
        let selected = match method {
            CastMethodV4::Omen | CastMethodV4::Bestow => method,
            _ => CastMethodV4::Normal,
        };
        assert_eq!(copy_method_for_selection(method, true), selected);
        assert_eq!(
            copy_method(method),
            if enabled() {
                selected
            } else {
                CastMethodV4::Normal
            }
        );
    }
}

#[test]
fn only_the_genuine_partial_limited_printing_selects_source_type_captures() {
    let selected = cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ));
    assert_eq!(enabled(), selected);
    for def in CARD_DEFS {
        assert_eq!(
            def.source_type_hexproof_v1.is_some(),
            selected && def.name == "Elenda, Saint of Dusk"
        );
    }
    let mut state = ready();
    let object = put(&mut state, "Lightning Bolt");
    assert!(characteristics(&state, object).is_none());
    assert_eq!(
        copy_projection(state.objects.get(object).card_def, CastMethodV4::Normal)
            .0
            .is_some(),
        selected
    );
}

#[test]
fn explicit_rule_primitive_uses_current_target_controller_and_printed_ability_suppression() {
    // These are typed primitive inputs, never installed on a CARD_DEFS entry.
    let mut state = ready();
    let target = put(&mut state, "Voldaren Epicure");
    state.objects.get_mut(target).zone = Zone::Battlefield;
    let rule = Some(SourceTypeHexproofDefV1::Instants);
    assert_eq!(
        active_rule_for_input(&state, target, PlayerId::P0, rule),
        None
    );
    assert_eq!(
        active_rule_for_input(&state, target, PlayerId::P1, rule),
        rule
    );
    state.objects.get_mut(target).controller = PlayerId::P1;
    let restored: GameState = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(
        active_rule_for_input(&restored, target, PlayerId::P0, rule),
        rule
    );
    assert_eq!(
        active_rule_for_input(&restored, target, PlayerId::P1, rule),
        None
    );
    state.objects.get_mut(target).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
        disguised: true,
        lookers: 3,
        hidden_by: None,
        hidden_by_source: None,
    });
    assert_eq!(
        active_rule_for_input(&state, target, PlayerId::P0, rule),
        None
    );
    assert!(
        characteristics(&state, target).is_none(),
        "actual Epicure has no such printed rule"
    );
}

#[test]
fn wrong_copy_type_projection_cannot_replace_selected_form_or_extend_absent_mode() {
    let mut state = ready();
    let (source, _) = paid_form(&mut state, "Fang Dragon", 1);
    let (copy, id) = engine::copy_spell_for(&mut state, source, PlayerId::P1).unwrap();
    let mut origin = state.objects.get(copy).spell_copy_origin.unwrap();
    origin.source_types_v1 =
        SourceTypeProjectionV1(Some(SourceTypesV1::from_types(&[CardType::Instant])));
    state.objects.get_mut(copy).spell_copy_origin = Some(origin);
    let row = state
        .stack
        .iter_mut()
        .find(|row| row.v4.stack_item_id == id)
        .unwrap();
    row.v4.source_contract.as_mut().unwrap().spell_copy_origin = Some(origin);
    let before = serde_json::to_vec(&state).unwrap();
    let row = state
        .stack
        .iter()
        .find(|row| row.v4.stack_item_id == id)
        .unwrap();
    assert!(engine::validate_spell_stack_source(&state, row).is_err());
    assert_eq!(before, serde_json::to_vec(&state).unwrap());
}

fn explicit_snapshot_input(
    state: &GameState,
    source: ObjectId,
) -> crate::controlled_creature_grants_v1::DepartureColorV1 {
    // A primitive input built from the actual current board. This is not
    // inserted as a fabricated journal occurrence; real opt-in history has its own fixture.
    crate::controlled_creature_grants_v1::DepartureColorV1 {
        source: crate::state::AbilitySourceContractV4::capture(state, source),
        color_mask: engine::object_color_mask(state, source),
        private_snapshot_json: serde_json::to_string(state.objects.get(source)).unwrap(),
        source_types_v1: SourceTypeProjectionV1(Some(current_types(state, source).unwrap())),
        source_type_input_v1: SourceTypeInputProjectionV1(Some(Box::new(
            capture_type_input(state, source).unwrap(),
        ))),
    }
}

#[test]
fn paid_instant_type_only_rewrites_refuse_with_original_cast_and_input_unchanged() {
    let mut state = ready();
    let source = put(&mut state, "Lightning Bolt");
    state.players[0].mana_pool = [5; 6];
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    engine::step(&mut state, Action::CastSpell(source)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseTargets { .. }
    ));
    engine::step(
        &mut state,
        Action::ChooseTarget(crate::state::Target::Player(PlayerId::P1)),
    )
    .unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    let input = explicit_snapshot_input(&state, source);
    assert!(input.source_types_v1.0.unwrap().contains(CardType::Instant));
    let id = state
        .stack
        .iter()
        .find(|row| row.source == source)
        .unwrap()
        .v4
        .stack_item_id;
    engine::counter_stack_item_by_id(&mut state, id)
        .unwrap()
        .unwrap();
    assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
    validate_snapshot_types(&input, state.objects.len()).unwrap();
    let original = serde_json::to_vec(&state).unwrap();
    for types in [
        SourceTypesV1::from_types(&[CardType::Sorcery]),
        SourceTypesV1::from_types(&[]),
    ] {
        let mut rewritten = input.clone();
        rewritten.source_types_v1 = SourceTypeProjectionV1(Some(types));
        assert_eq!(input.private_snapshot_json, rewritten.private_snapshot_json);
        assert_eq!(input.source_type_input_v1, rewritten.source_type_input_v1);
        assert!(validate_snapshot_types(&rewritten, state.objects.len()).is_err());
        assert_eq!(original, serde_json::to_vec(&state).unwrap());
    }
}

#[test]
#[cfg(feature = "standard-magezero-fixtures")]
fn paid_instant_adventure_type_projection_recomputes_original_selected_form() {
    let mut state = ready();
    let (source, id) = paid_form(&mut state, "Questing Druid", 1);
    let input = explicit_snapshot_input(&state, source);
    engine::counter_stack_item_by_id(&mut state, id)
        .unwrap()
        .unwrap();
    validate_snapshot_types(&input, state.objects.len()).unwrap();
    let mut rewritten = input.clone();
    rewritten.source_types_v1 =
        SourceTypeProjectionV1(Some(SourceTypesV1::from_types(&[CardType::Creature])));
    assert!(validate_snapshot_types(&rewritten, state.objects.len()).is_err());
    assert_eq!(input.private_snapshot_json, rewritten.private_snapshot_json);
}

#[test]
#[cfg(feature = "standard-magezero-fixtures")]
fn paid_aura_animation_retains_original_prebatch_dependencies_after_aura_departure() {
    let mut state = ready();
    let source = put(&mut state, "Great Furnace");
    crate::event::propose_and_commit(
        &mut state,
        crate::event::ProposedEvent::zone_change(source, Zone::Battlefield),
    );
    let aura = put(&mut state, "Zoetic Glyph");
    state.players[0].mana_pool = [5; 6];
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    engine::step(&mut state, Action::CastSpell(aura)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseTargets { .. }
    ));
    engine::step(
        &mut state,
        Action::ChooseTarget(crate::state::Target::Object(source)),
    )
    .unwrap();
    for _ in 0..30 {
        match engine::advance_until_decision(&mut state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected Glyph decision: {other:?}"),
        }
    }
    assert_eq!(state.objects.get(aura).zone, Zone::Battlefield);
    let input = explicit_snapshot_input(&state, source);
    assert!(input
        .source_types_v1
        .0
        .unwrap()
        .contains(CardType::Creature));
    assert!(input
        .source_type_input_v1
        .0
        .as_ref()
        .unwrap()
        .dependencies
        .iter()
        .any(|(id, _)| *id == aura));
    crate::event::propose_and_commit(
        &mut state,
        crate::event::ProposedEvent::zone_change(aura, Zone::Graveyard),
    );
    let after = current_types(&state, source).unwrap();
    assert!(!after.contains(CardType::Creature));
    crate::event::propose_and_commit(
        &mut state,
        crate::event::ProposedEvent::zone_change(source, Zone::Graveyard),
    );
    validate_snapshot_types(&input, state.objects.len()).unwrap();
    let mut rewritten = input.clone();
    rewritten.source_types_v1 = SourceTypeProjectionV1(Some(after));
    assert!(validate_snapshot_types(&rewritten, state.objects.len()).is_err());
    let mut erased = input;
    erased
        .source_type_input_v1
        .0
        .as_mut()
        .unwrap()
        .dependencies
        .clear();
    assert!(validate_snapshot_types(&erased, state.objects.len()).is_err());
}
