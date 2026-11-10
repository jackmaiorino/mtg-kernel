use super::*;
use crate::card_def::{card_id_by_name, CardCapability};
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{FaceDownV1, GameObject, ObjectStateV4};

#[path = "registered.rs"]
mod registered;

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

fn game() -> GameState {
    GameState::new_from_libraries(
        &[id("Island"); 20],
        &[id("Island"); 20],
        |definition| CARD_DEFS[usize::from(definition)].object_name.into(),
        0x460_6126,
    )
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = id(name);
    let object = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[usize::from(card_def)].object_name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Battlefield => state.players[player.index()].battlefield.push(object),
        Zone::Graveyard => state.players[player.index()].graveyard.push(object),
        Zone::Hand => state.players[player.index()].hand.push(object),
        Zone::Library => state.players[player.index()].library.insert(0, object),
        Zone::Exile => state.exile.push(object),
        _ => panic!("unsupported fixture zone"),
    }
    object
}

// These lower-level tests exercise the layer primitive on an ordinary host.
fn host(state: &mut GameState) -> AbilitySourceContractV4 {
    let object = put(state, PlayerId::P0, "Tireless Tribe", Zone::Battlefield);
    AbilitySourceContractV4::capture(state, object)
}

#[test]
fn legacy_full_text_guard_agrees_with_forced_and_stale_history_scans() {
    use crate::event_history_v1::{guard_tests_v1::with_scans_forced, EventKindV1};

    let mut state = game();
    let land = put(&mut state, PlayerId::P0, "Island", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(land, Zone::Battlefield),
    );
    assert!(!state
        .engine
        .event_history
        .has_any(&[EventKindV1::FullTextBatchEntryV1]));
    assert!(refuse_legacy(&state).is_ok());
    assert!(with_scans_forced(|| refuse_legacy(&state)).is_ok());

    // A mutable history borrow invalidates the index even when the event
    // bytes stay identical. This forces the ordinary fallback scan.
    let _ = state.engine.event_history.as_mut_slice();
    assert!(state
        .engine
        .event_history
        .has_any(&[EventKindV1::FullTextBatchEntryV1]));
    assert!(refuse_legacy(&state).is_ok());
}

#[test]
fn full_text_top_is_the_last_arrival_and_noncreature_does_not_skip_backward() {
    let mut state = game();
    let host = host(&mut state);
    assert_eq!(top_creature_text(&state, host), None);
    let creature = put(&mut state, PlayerId::P0, "Masticore", Zone::Hand);
    event::commit(
        &mut state,
        ProposedEvent::zone_change(creature, Zone::Graveyard),
    );
    assert_eq!(
        top_creature_text(&state, host).unwrap().definition().name,
        "Masticore"
    );
    assert!(active_definition(&state, host.source).is_none());
    let land = put(&mut state, PlayerId::P0, "Island", Zone::Hand);
    event::commit(
        &mut state,
        ProposedEvent::zone_change(land, Zone::Graveyard),
    );
    assert_eq!(top_creature_text(&state, host), None);
    event::commit(&mut state, ProposedEvent::zone_change(land, Zone::Exile));
    assert_eq!(
        top_creature_text(&state, host).unwrap().definition().name,
        "Masticore"
    );
}

#[test]
fn donated_wall_mana_limit_follows_original_donor_incarnation_and_turn() {
    let mut state = game();
    let host = host(&mut state);
    let first = put(&mut state, PlayerId::P0, "Wall of Roots", Zone::Graveyard);
    let text = top_creature_text(&state, host).unwrap();
    assert!(text.is_wall_mana());
    assert!(!text.wall_mana_used(&state));
    text.record_wall_mana_use(&mut state);
    assert!(text.wall_mana_used(&state));

    let second = put(&mut state, PlayerId::P0, "Wall of Roots", Zone::Graveyard);
    assert!(!top_creature_text(&state, host)
        .unwrap()
        .wall_mana_used(&state));
    event::commit(&mut state, ProposedEvent::zone_change(second, Zone::Exile));
    assert!(top_creature_text(&state, host)
        .unwrap()
        .wall_mana_used(&state));
    event::commit(&mut state, ProposedEvent::zone_change(first, Zone::Exile));
    event::commit(
        &mut state,
        ProposedEvent::zone_change(first, Zone::Graveyard),
    );
    let returned = top_creature_text(&state, host).unwrap();
    assert!(!returned.wall_mana_used(&state));
    returned.record_wall_mana_use(&mut state);
    state.active_player = state.active_player.opponent();
    assert!(!returned.wall_mana_used(&state));
}

#[test]
fn full_text_reads_current_controllers_graveyard_and_rejects_old_controller() {
    let mut state = game();
    let host = host(&mut state);
    put(&mut state, PlayerId::P0, "Masticore", Zone::Graveyard);
    put(
        &mut state,
        PlayerId::P1,
        "Birds of Paradise",
        Zone::Graveyard,
    );
    let original = top_creature_text(&state, host).unwrap();
    state.objects.get_mut(host.source).controller = PlayerId::P1;
    assert!(original
        .freeze_activated(&state, FullTextAbilityV1::Printed(0))
        .is_err());
    let current = AbilitySourceContractV4::capture(&state, host.source);
    let changed = top_creature_text(&state, current).unwrap();
    assert_eq!(changed.definition().name, "Birds of Paradise");
    assert_eq!(changed.host().owner, PlayerId::P0);
    assert_eq!(changed.host().controller, PlayerId::P1);
}

#[test]
fn donated_trigger_program_uses_event_time_text_after_host_and_donor_depart() {
    let mut state = game();
    let player = PlayerId::P0;
    let donor = put(&mut state, player, "Palinchron", Zone::Graveyard);
    let object = put(&mut state, player, "Tireless Tribe", Zone::Hand);
    event::commit(
        &mut state,
        ProposedEvent::zone_change(object, Zone::Battlefield),
    );
    let entry = state
        .engine
        .event_history
        .iter()
        .position(|event| {
            matches!(event,
            CommittedEvent::ZoneChange { object: entered, to: Zone::Battlefield, .. }
                if *entered == object)
        })
        .unwrap() as u64;
    let host = AbilitySourceContractV4::capture(&state, object);
    let text = top_creature_text(&state, host).unwrap();
    let capture = text
        .freeze_trigger(&state, DonatedTriggerKindV1::PalinchronEntered { entry })
        .unwrap();
    assert_eq!(capture.host, host);
    assert_eq!(capture.donor.source, donor);
    assert!(text
        .freeze_trigger(
            &state,
            DonatedTriggerKindV1::PalinchronEntered { entry: entry + 1 }
        )
        .is_err());

    event::commit(&mut state, ProposedEvent::zone_change(donor, Zone::Exile));
    event::commit(
        &mut state,
        ProposedEvent::zone_change(object, Zone::Graveyard),
    );
    assert_eq!(capture.program(&state).unwrap(), p2c::palinchron_enters());
    assert!(text
        .freeze_trigger(&state, DonatedTriggerKindV1::PalinchronEntered { entry })
        .is_err());
    let restored: DonatedTriggerCaptureV1 =
        serde_json::from_slice(&serde_json::to_vec(&capture).unwrap()).unwrap();
    assert_eq!(restored, capture);
    assert_eq!(restored.program(&state).unwrap(), p2c::palinchron_enters());
}

#[test]
fn deferred_entry_capture_keeps_its_entry_row_and_selected_arrival() {
    use crate::engine::{self, Action, Decision, RuntimeRulesV1};
    use crate::state::{Step, Target};

    let mut state = game();
    state.step = Step::Main1;
    state.engine.runtime_rules = RuntimeRulesV1::OWNER_ORDERED_GRAVEYARD_V1;
    let player = PlayerId::P0;
    let donor = put(&mut state, player, "Palinchron", Zone::Library);
    let other = put(&mut state, player, "Triskelion", Zone::Library);
    let source = put(&mut state, player, "Tireless Tribe", Zone::Hand);
    event::commit(
        &mut state,
        ProposedEvent::zone_change(source, Zone::Battlefield),
    );
    let entry = state
        .engine
        .event_history
        .iter()
        .position(|event| {
            matches!(event, CommittedEvent::ZoneChange { object, to: Zone::Battlefield, .. }
            if *object == source)
        })
        .unwrap() as u64;
    let host = AbilitySourceContractV4::capture(&state, source);
    let marker = state.engine.event_history.len() as u64;
    // Abstract host for the unregistered card's layer primitive. The marker
    // is pinned before the real simultaneous arrivals and their owner order.
    state
        .engine
        .event_history
        .push(CommittedEvent::FullTextBatchEntryV1 {
            entry_history_index: entry,
            host,
            donor: ObjectLinkV4 {
                object: donor,
                zone_change_count: 1,
            },
            text_definition: id("Palinchron"),
        });
    event::propose_and_commit_batch(
        &mut state,
        vec![
            ProposedEvent::zone_change(donor, Zone::Graveyard),
            ProposedEvent::zone_change(other, Zone::Graveyard),
        ],
    );
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseEffectTargets { .. }
    ));
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(other)),
    )
    .unwrap();
    assert_eq!(state.players[player.index()].graveyard, [other, donor]);

    let text = top_creature_text(&state, host).unwrap();
    let capture = text
        .freeze_trigger(
            &state,
            DonatedTriggerKindV1::PalinchronEnteredAfterBatch { entry, marker },
        )
        .unwrap();
    let capture_at = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(CommittedEvent::FullTextTriggerCapturedV1 {
            capture: Box::new(capture),
        });
    assert_eq!(
        trigger_record(&state, capture_at)
            .unwrap()
            .program(&state)
            .unwrap(),
        p2c::palinchron_enters()
    );
    let CommittedEvent::FullTextBatchEntryV1 { donor, .. } =
        &mut state.engine.event_history[marker as usize]
    else {
        panic!("entry marker missing")
    };
    donor.object = other;
    assert!(trigger_record(&state, capture_at).is_err());
}

#[test]
fn donated_karmic_guide_keeps_its_own_graveyard_target_after_the_top_changes() {
    let mut state = game();
    let player = PlayerId::P0;
    let donor = put(&mut state, player, "Karmic Guide", Zone::Graveyard);
    let source = put(&mut state, player, "Tireless Tribe", Zone::Hand);
    event::commit(
        &mut state,
        ProposedEvent::zone_change(source, Zone::Battlefield),
    );
    let entry = state
        .engine
        .event_history
        .iter()
        .position(|event| {
            matches!(event, CommittedEvent::ZoneChange { object, to: Zone::Battlefield, .. }
            if *object == source)
        })
        .unwrap() as u64;
    let host = AbilitySourceContractV4::capture(&state, source);
    let text = top_creature_text(&state, host).unwrap();
    queue_donated_trigger(
        &mut state,
        text,
        DonatedTriggerKindV1::KarmicGuideEntered { entry },
    )
    .unwrap();
    let index = entry + 1;
    let root = EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P2cV1(
        P2cOpV1::DonatedTriggerV1 { capture: index },
    )));
    event::commit(&mut state, ProposedEvent::zone_change(donor, Zone::Exile));
    event::commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
    assert_eq!(
        trigger_target_spec(&state, &root).unwrap(),
        Some(TargetSpec::CreatureCardInOwnGraveyard)
    );
    assert_eq!(
        trigger_record(&state, index)
            .unwrap()
            .program(&state)
            .unwrap(),
        p2c::guide_enters()
    );
}

#[test]
fn donated_guide_echo_tracks_native_control_and_keeps_its_cost_after_donor_departure() {
    use crate::engine::{self, Action, Decision};
    use crate::mana::ManaColor;
    use crate::state::Step;

    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = game();
        state.engine.runtime_rules.order_graveyard_batches_v1 = true;
        state.active_player = player;
        state.priority_player = player;
        state.step = Step::Upkeep;
        let source = put(&mut state, player, VOLRATH, Zone::Hand);
        event::commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        event::log_upkeep_began(&mut state, player);

        let donor = put(&mut state, player, "Karmic Guide", Zone::Graveyard);
        let upkeep = state.engine.event_history.len() as u64;
        event::log_upkeep_began(&mut state, player);
        let old_host = AbilitySourceContractV4::capture(&state, source);
        assert_eq!(echo_custody(&state, old_host, upkeep), None);

        event::commit(&mut state, ProposedEvent::zone_change(donor, Zone::Exile));
        crate::standard_cards_v1::gain_control(&mut state, source, player.opponent());
        crate::standard_cards_v1::gain_control(&mut state, source, player);
        let control = state.engine.event_history.len() as u64 - 1;
        event::commit(
            &mut state,
            ProposedEvent::zone_change(donor, Zone::Graveyard),
        );
        let upkeep = state.engine.event_history.len() as u64;
        event::log_upkeep_began(&mut state, player);
        assert_eq!(echo_custody(&state, old_host, upkeep), Some(control));
        queue_upkeep_triggers(&mut state, player, upkeep as usize).unwrap();
        let triggers = crate::trigger::collect_and_process(&mut state);
        state.engine.pending_triggers.extend(triggers);
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        assert!(state.stack.last().is_some_and(is_donated_trigger));

        event::commit(&mut state, ProposedEvent::zone_change(donor, Zone::Exile));
        let white = ManaColor::W.pool_index();
        let colorless = ManaColor::C.pool_index();
        state.players[player.index()].mana_pool[white] = 2;
        state.players[player.index()].mana_pool[colorless] = 3;
        let state: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for (option, destination) in [(0, Zone::Battlefield), (1, Zone::Graveyard)] {
            let mut branch = state.clone();
            for _ in 0..2 {
                assert!(matches!(
                    engine::advance_until_decision(&mut branch),
                    Decision::CastSpellOrPass { .. }
                ));
                engine::step(&mut branch, Action::Pass).unwrap();
            }
            assert!(matches!(engine::advance_until_decision(&mut branch),
                Decision::ChooseEffectOption { player: chooser, .. } if chooser == player));
            engine::step(&mut branch, Action::ChooseEffectOption(option)).unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut branch),
                Decision::CastSpellOrPass { .. }
            ));
            assert_eq!(branch.objects.get(source).zone, destination);
            assert_eq!(
                branch.players[player.index()].mana_pool[white],
                if option == 0 { 0 } else { 2 }
            );
            assert_eq!(
                branch.players[player.index()].mana_pool[colorless],
                if option == 0 { 0 } else { 3 }
            );
        }
    }
}

#[test]
fn full_text_preserves_physical_copy_identity_and_all_card_state() {
    let mut state = game();
    let host = host(&mut state);
    put(&mut state, PlayerId::P0, "Masticore", Zone::Graveyard);
    let live = state.objects.get_mut(host.source);
    live.counters.plus1_plus1 = 3;
    live.damage = 1;
    live.tapped = true;
    let before = state.diagnostic_state_hash();
    let text = top_creature_text(&state, host).unwrap();
    assert_eq!(text.definition().name, "Masticore");
    assert!(text.definition().types.contains(&CardType::Artifact));
    assert_eq!(text.definition().power, Some(4));
    assert_eq!(text.definition().toughness, Some(4));
    assert_eq!(text.definition().cost.generic, 4);
    assert_eq!(text.host().card_def, id("Tireless Tribe"));
    assert_eq!(before, state.diagnostic_state_hash());
    // Exercise a real copy constructor: text changes must not mutate the
    // CardDef that the current layer-1 constructor reads.
    let copy =
        crate::standard_cards_v1::create_token_copy(&mut state, host.source, PlayerId::P1).unwrap();
    assert_eq!(state.objects.get(copy).card_def, id("Tireless Tribe"));
    assert_eq!(state.objects.get(copy).counters.plus1_plus1, 0);
}

#[test]
fn full_text_does_not_read_hidden_zones_or_accept_hidden_hosts() {
    let mut state = game();
    let host = host(&mut state);
    let hidden = put(&mut state, PlayerId::P0, "Masticore", Zone::Hand);
    put(
        &mut state,
        PlayerId::P1,
        "Birds of Paradise",
        Zone::Graveyard,
    );
    assert!(top_creature_text(&state, host).is_none());
    put(&mut state, PlayerId::P0, "Glory", Zone::Graveyard);
    let before = top_creature_text(&state, host);
    state.objects.get_mut(hidden).card_def = id("Island");
    assert_eq!(before, top_creature_text(&state, host));
    state.objects.get_mut(host.source).v4.face_down_v1 = Some(FaceDownV1 {
        disguised: true,
        lookers: 1,
        hidden_by: None,
        hidden_by_source: None,
    });
    assert!(top_creature_text(&state, host).is_none());
}

#[test]
fn full_text_rejects_nonbattlefield_hosts_and_stale_incarnations() {
    let mut state = game();
    let host = host(&mut state);
    put(&mut state, PlayerId::P0, "Masticore", Zone::Graveyard);
    event::commit(
        &mut state,
        ProposedEvent::zone_change(host.source, Zone::Hand),
    );
    assert!(top_creature_text(&state, host).is_none());
    let hand = AbilitySourceContractV4::capture(&state, host.source);
    assert!(top_creature_text(&state, hand).is_none());
    event::commit(
        &mut state,
        ProposedEvent::zone_change(host.source, Zone::Battlefield),
    );
    assert!(top_creature_text(&state, host).is_none());
    let current = AbilitySourceContractV4::capture(&state, host.source);
    assert!(top_creature_text(&state, current).is_some());
}

#[test]
fn full_text_tokens_and_malformed_hidden_or_foreign_top_are_not_creature_cards() {
    let mut state = game();
    let host = host(&mut state);
    let donor = put(&mut state, PlayerId::P0, "Masticore", Zone::Graveyard);
    state.objects.get_mut(donor).v4.is_token = true;
    assert!(top_creature_text(&state, host).is_none());
    state.objects.get_mut(donor).v4.is_token = false;
    state.objects.get_mut(donor).owner = PlayerId::P1;
    assert!(top_creature_text(&state, host).is_none());
    state.objects.get_mut(donor).owner = PlayerId::P0;
    state.objects.get_mut(donor).zone = Zone::Library;
    assert!(top_creature_text(&state, host).is_none());
}

#[test]
fn full_text_prepaid_ability_keeps_original_program_host_and_controller() {
    let mut state = game();
    let host = host(&mut state);
    let donor = put(&mut state, PlayerId::P0, "Masticore", Zone::Graveyard);
    let text = top_creature_text(&state, host).unwrap();
    let frozen = text
        .freeze_activated(&state, FullTextAbilityV1::Printed(0))
        .unwrap();
    let effect = (frozen.ability().effect)();
    assert_eq!(frozen.ability().target_spec, TargetSpec::Creature);
    // Costs or responses can remove the donor and source, without changing
    // the already-announced ability's physical source or producing text.
    event::commit(&mut state, ProposedEvent::zone_change(donor, Zone::Exile));
    put(&mut state, PlayerId::P0, "Glory", Zone::Graveyard);
    state.objects.get_mut(host.source).controller = PlayerId::P1;
    event::commit(
        &mut state,
        ProposedEvent::zone_change(host.source, Zone::Hand),
    );
    assert_eq!(frozen.host(), host);
    assert_eq!(frozen.text_definition(), id("Masticore"));
    assert_eq!(frozen.selector(), FullTextAbilityV1::Printed(0));
    assert_eq!((frozen.ability().effect)(), effect);
    assert!(
        crate::engine::validate_historical_ability_source_contract(&state, frozen.host(),).is_ok()
    );
    assert!(text
        .freeze_activated(&state, FullTextAbilityV1::Printed(0))
        .is_err());
}

#[test]
fn full_text_donor_leave_return_invalidates_offer_even_with_the_same_definition() {
    let mut state = game();
    let host = host(&mut state);
    let donor = put(&mut state, PlayerId::P0, "Masticore", Zone::Graveyard);
    let original = top_creature_text(&state, host).unwrap();
    event::commit(&mut state, ProposedEvent::zone_change(donor, Zone::Hand));
    event::commit(
        &mut state,
        ProposedEvent::zone_change(donor, Zone::Graveyard),
    );
    assert!(original
        .freeze_activated(&state, FullTextAbilityV1::Printed(0))
        .is_err());
    let current = top_creature_text(&state, host).unwrap();
    assert!(current
        .freeze_activated(&state, FullTextAbilityV1::Printed(0))
        .is_ok());
}

#[test]
fn full_text_ignores_layer_six_removal_but_activation_does_not() {
    let mut state = game();
    let host = host(&mut state);
    put(&mut state, PlayerId::P0, "Masticore", Zone::Graveyard);
    let dress = put(&mut state, PlayerId::P1, "Dress Down", Zone::Hand);
    event::commit(
        &mut state,
        ProposedEvent::zone_change(dress, Zone::Battlefield),
    );
    // Register Dress Down's native layer-six effect before checking the host.
    let _ = crate::trigger::collect_and_process(&mut state);
    let text = top_creature_text(&state, host).unwrap();
    assert_eq!(text.definition().name, "Masticore");
    for selector in [
        FullTextAbilityV1::Printed(0),
        FullTextAbilityV1::RetainedDiscard,
    ] {
        assert_eq!(
            text.freeze_activated(&state, selector).unwrap_err(),
            "full-text ability was removed in a later layer",
        );
    }
}

#[test]
fn full_text_retained_discard_has_only_a_mana_cost_and_resolves_on_controller() {
    let mut state = game();
    let host = host(&mut state);
    put(&mut state, PlayerId::P0, "Masticore", Zone::Graveyard);
    let frozen = top_creature_text(&state, host)
        .unwrap()
        .freeze_activated(&state, FullTextAbilityV1::RetainedDiscard)
        .unwrap();
    let ability = frozen.ability();
    assert_eq!(
        ability.cost,
        &[CostComponent::Mana(Cost {
            pips: &[],
            generic: 2,
            x_count: 0,
        })]
    );
    assert_eq!(ability.target_spec, TargetSpec::None);
    assert_eq!(
        (ability.effect)(),
        EffectOp::DiscardCards {
            player: PlayerRef::Controller,
            count: 1,
        }
    );
    assert!(!ability.sorcery_speed_only);
    assert_eq!(ability.activation_zone, Zone::Battlefield);
}

#[test]
fn full_text_rejects_graveyard_only_ability_and_out_of_range_selectors() {
    let mut state = game();
    let host = host(&mut state);
    put(&mut state, PlayerId::P0, "Glory", Zone::Graveyard);
    let text = top_creature_text(&state, host).unwrap();
    assert!(text
        .freeze_activated(&state, FullTextAbilityV1::Printed(0))
        .is_err());
    assert!(text
        .freeze_activated(&state, FullTextAbilityV1::Printed(255))
        .is_err());
    assert!(text
        .freeze_activated(&state, FullTextAbilityV1::RetainedDiscard)
        .is_ok());
}

#[test]
fn full_text_registration_is_partial() {
    assert_eq!(
        CARD_DEFS[usize::from(id(VOLRATH))].capability,
        CardCapability::Partial
    );
}

// A resolution fixture starts after payment. It exercises the actual stack
// validator and interpreter; normal announcement/payment integration remains
// separate work and is not claimed by these tests.
fn paid_discard_item(
    state: &mut GameState,
    frozen: FrozenFullTextActivationV1,
) -> crate::state::StackItem {
    use crate::ids::StackItemId;
    use crate::state::{StackItem, StackItemKind, StackStateV4};
    state.engine.next_stack_item_id += 1;
    StackItem {
        kind: StackItemKind::ActivatedAbility,
        source: frozen.host().source,
        controller: frozen.host().controller,
        targets: Vec::new(),
        is_copy: false,
        inline_effect: Some((frozen.ability().effect)()),
        discarded: Vec::new(),
        is_flashback: false,
        mode_chosen: 0,
        madness_offer: false,
        kicked: false,
        v4: StackStateV4 {
            stack_item_id: StackItemId(state.engine.next_stack_item_id),
            ability_source_contract: Some(frozen.host()),
            target_spec: Some(TargetSpec::None),
            activated_ability_index: Some(0),
            full_text_activation_v1: frozen.stack_record(),
            ..StackStateV4::default()
        },
    }
}

#[test]
fn full_text_native_stack_resolves_retained_discard_after_both_sources_leave() {
    use crate::engine::{self, Action, Decision};
    let mut state = game();
    state.step = crate::state::Step::Main1;
    let host = host(&mut state);
    let donor = put(
        &mut state,
        PlayerId::P0,
        "Birds of Paradise",
        Zone::Graveyard,
    );
    let discarded = put(&mut state, PlayerId::P0, "Island", Zone::Hand);
    let text = top_creature_text(&state, host).unwrap();
    let colorless = crate::mana::ManaColor::C.pool_index();
    state.players[0].mana_pool[colorless] = 2;
    engine::activate_full_text_retained_discard(&mut state, text).unwrap();
    assert_eq!(state.players[0].mana_pool[colorless], 0);
    assert_eq!(state.objects.get(discarded).zone, Zone::Hand);
    assert!(state.engine.pending_activation.is_none());
    let item = state.stack.last().unwrap().clone();
    assert!(item.discarded.is_empty());
    assert!(item.v4.paid_cost_refs.is_empty());
    assert_eq!(
        engine::validated_stack_item_target_spec(&item, &state).unwrap(),
        Some(TargetSpec::None)
    );
    // Without its producer record this would be Tireless Tribe's +0/+4
    // activation at index 0, so the ordinary validator must reject it.
    let mut lost = item.clone();
    lost.v4.full_text_activation_v1 = Default::default();
    assert!(engine::validated_stack_item_target_spec(&lost, &state).is_err());
    event::commit(&mut state, ProposedEvent::zone_change(donor, Zone::Exile));
    event::commit(
        &mut state,
        ProposedEvent::zone_change(host.source, Zone::Hand),
    );
    for _ in 0..4 {
        match engine::advance_until_decision(&mut state) {
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            Decision::Discard { player, .. } => {
                assert_eq!(player, PlayerId::P0);
                engine::step(&mut state, Action::Discard(vec![discarded])).unwrap();
                assert_eq!(state.objects.get(discarded).zone, Zone::Graveyard);
                assert!(state.stack.is_empty());
                return;
            }
            other => panic!("unexpected full-text resolution decision: {other:?}"),
        }
    }
    panic!("full-text discard did not resolve");
}

#[test]
fn full_text_retained_activation_requires_mana_but_not_hand_or_supported_donor() {
    use crate::engine::{self, Action, Decision};
    let mut state = game();
    state.step = crate::state::Step::Main1;
    let host = host(&mut state);
    put(&mut state, PlayerId::P0, VOLRATH, Zone::Graveyard);
    let text = top_creature_text(&state, host).unwrap();
    assert_eq!(text.definition().capability, CardCapability::Partial);
    assert_eq!(
        text.freeze_activated(&state, FullTextAbilityV1::Printed(0))
            .unwrap_err(),
        "full-text definition is not fully supported"
    );
    let before = state.diagnostic_state_hash();
    assert!(engine::activate_full_text_retained_discard(&mut state, text).is_err());
    assert_eq!(state.diagnostic_state_hash(), before);
    let colorless = crate::mana::ManaColor::C.pool_index();
    state.players[0].mana_pool[colorless] = 2;
    assert!(state.players[0].hand.is_empty());
    engine::activate_full_text_retained_discard(&mut state, text).unwrap();
    assert_eq!(state.players[0].mana_pool[colorless], 0);
    assert_eq!(state.stack.len(), 1);
    for _ in 0..4 {
        match engine::advance_until_decision(&mut state) {
            Decision::CastSpellOrPass { .. } => {
                if state.stack.is_empty() {
                    assert!(state.players[0].hand.is_empty());
                    return;
                }
                engine::step(&mut state, Action::Pass).unwrap();
            }
            other => panic!("unexpected empty-hand resolution: {other:?}"),
        }
    }
    panic!("empty-hand discard did not finish");
}

#[test]
fn full_text_retained_activation_rejects_changed_top_before_spending_mana() {
    let mut state = game();
    state.step = crate::state::Step::Main1;
    let host = host(&mut state);
    put(
        &mut state,
        PlayerId::P0,
        "Birds of Paradise",
        Zone::Graveyard,
    );
    let stale = top_creature_text(&state, host).unwrap();
    state.players[0].mana_pool[crate::mana::ManaColor::C.pool_index()] = 2;
    put(&mut state, PlayerId::P0, "Island", Zone::Graveyard);
    let before = state.diagnostic_state_hash();
    let error = crate::engine::activate_full_text_retained_discard(&mut state, stale).unwrap_err();
    assert!(error.contains("stale host or graveyard top"));
    assert_eq!(state.diagnostic_state_hash(), before);
}

#[test]
fn donated_triskelion_spends_the_hosts_counter_and_keeps_the_original_program() {
    use crate::engine::{self, Action, Decision};
    use crate::state::{Step, Target};

    let mut state = game();
    state.step = Step::Main1;
    let host = host(&mut state);
    let donor = put(&mut state, PlayerId::P0, "Triskelion", Zone::Graveyard);
    state.objects.get_mut(host.source).counters.plus1_plus1 = 2;
    let bow = put(
        &mut state,
        PlayerId::P0,
        "Viridian Longbow",
        Zone::Battlefield,
    );
    state.attach_object_exact(bow, 0, host.source, 0).unwrap();
    let cauldron = put(
        &mut state,
        PlayerId::P0,
        "Agatha's Soul Cauldron",
        Zone::Battlefield,
    );
    let grant = put(&mut state, PlayerId::P0, "Psychatog", Zone::Exile);
    state.objects.get_mut(grant).v4.exiled_by = Some(ObjectLinkV4 {
        object: cauldron,
        zone_change_count: 0,
    });
    assert!(engine::resolved_activated_ability(host.card_def, 1, &state, host.source).is_some());
    let grant_slots = crate::standard_cards_v1::cauldron_granted_abilities(&state, host.source)
        .into_iter()
        .map(|(index, _, _, _)| index)
        .collect::<Vec<_>>();
    assert_eq!(grant_slots, [2, 3]);
    let text = top_creature_text(&state, host).unwrap();
    assert_eq!(text.first_donated_slot(&state), 4);
    engine::stage_full_text_activation(&mut state, text, FullTextAbilityV1::Printed(0)).unwrap();

    let target = Target::Player(PlayerId::P1);
    assert!(matches!(engine::advance_until_decision(&mut state),
        Decision::ChooseTargets { spell, legal_targets, .. }
        if spell == host.source && legal_targets.contains(&target)));
    engine::step(&mut state, Action::ChooseTarget(target)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.objects.get(host.source).counters.plus1_plus1, 1);
    assert_eq!(state.objects.get(donor).counters.plus1_plus1, 0);
    let item = state.stack.last().unwrap();
    assert_eq!(item.v4.ability_source_contract, Some(host));
    assert_eq!(item.v4.activated_ability_index, Some(4));
    assert_eq!(
        engine::validated_stack_item_target_spec(item, &state).unwrap(),
        Some(TargetSpec::AnyTarget)
    );

    event::commit(&mut state, ProposedEvent::zone_change(donor, Zone::Exile));
    event::commit(
        &mut state,
        ProposedEvent::zone_change(cauldron, Zone::Graveyard),
    );
    for _ in 0..2 {
        engine::step(&mut state, Action::Pass).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
    }
    assert!(state.stack.is_empty());
    assert_eq!(state.players[PlayerId::P1.index()].life, 19);
}

#[test]
fn donated_hermit_druid_pays_the_host_and_uses_the_selected_graveyard_top() {
    use crate::engine::{self, Action, Decision};
    use crate::premodern_ordered_public_v1::{
        self as public, DonatedActivationV1, FullTextProgramV1, ProducerStageV1,
    };
    use crate::state::{Step, Target};

    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = GameState::new_from_libraries_with_graveyard_order_v1(
            &[id("Island"); 20],
            &[id("Island"); 20],
            crate::rl::card_name,
            0x460_6127,
            player,
        );
        state.step = Step::Main1;
        // Exercise the native donor/payment path with a public fixture host
        // while Volrath remains unregistered.
        let source = put(&mut state, player, "Tireless Tribe", Zone::Battlefield);
        let host = AbilitySourceContractV4::capture(&state, source);
        let druid = put(&mut state, player, "Hermit Druid", Zone::Graveyard);
        let forest = put(&mut state, player, "Forest", Zone::Battlefield);
        let basic = state.players[player.index()].library[0];
        let triskelion = put(&mut state, player, "Triskelion", Zone::Library);
        let devourer = put(&mut state, player, "Phyrexian Devourer", Zone::Library);
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { player: chooser, .. } if chooser == player
        ));
        engine::step(&mut state, Action::ActivateManaAbility(forest)).unwrap();
        let text = top_creature_text(&state, host).unwrap();
        assert_eq!(text.definition().name, "Hermit Druid");
        engine::stage_full_text_activation(&mut state, text, FullTextAbilityV1::Printed(0))
            .unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        assert!(state.objects.get(source).tapped);
        assert!(state.objects.get(forest).tapped);
        assert!(!state.objects.get(druid).tapped);
        assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
        assert_eq!(
            state.stack.last().unwrap().v4.ability_source_contract,
            Some(host)
        );
        let surface = crate::surface_v2::HarnessSurfaceV2::new();
        for viewer in [player, player.opponent()] {
            let numeric =
                crate::rl::observe_premodern_ordered_v1(&state, &surface, viewer, 0).unwrap();
            let [producer] = numeric.ordered.producers.as_slice() else {
                panic!("expected the actual donated activation");
            };
            assert_eq!(producer.stage, ProducerStageV1::Stack { position: 0 });
            assert_eq!(
                producer.program,
                FullTextProgramV1::DonatedActivation {
                    printed_index: 0,
                    effect: DonatedActivationV1::HermitDruidRevealThroughBasic,
                }
            );
            assert_eq!(producer.physical.arena_id, source.0);
            assert_eq!(producer.text_donor.arena_id, druid.0);
            assert_eq!(
                producer.native_ability_index,
                state.stack[0].v4.activated_ability_index
            );
            let stable = numeric.stable.unwrap();
            let [stack_item] = stable.projection.stack.as_slice() else {
                panic!("expected one native stack item in the numeric scene");
            };
            assert_eq!(stack_item.stack_index, 0);
            assert_eq!(stack_item.source, producer.physical);
            assert_eq!(stack_item.controller, player.into());
            assert_eq!(
                stack_item.stack_item_kind,
                crate::rl::StackItemKindV2::ActivatedAbility
            );
            let human = public::observe_human(&state, viewer).unwrap();
            let [human_producer] = human.producers.as_slice() else {
                panic!("expected the donated activation in the Human view");
            };
            assert_eq!(human_producer.stage, producer.stage);
            assert_eq!(human_producer.program, producer.program);
            assert_eq!(human_producer.controller, player.into());
            assert_eq!(human_producer.physical_name, "Tireless Tribe");
            assert_eq!(human_producer.text_donor_name, "Hermit Druid");
            assert!(!serde_json::to_string(&human).unwrap().contains("arena_id"));
        }

        let mut ordering = false;
        for _ in 0..2 {
            engine::step(&mut state, Action::Pass).unwrap();
            match engine::advance_until_decision(&mut state) {
                Decision::ChooseEffectTargets {
                    player: chooser,
                    legal_targets,
                    min_targets: 2,
                    max_targets: 2,
                    can_finish: false,
                    ..
                } => {
                    assert_eq!(chooser, player);
                    assert_eq!(
                        legal_targets,
                        [Target::Object(devourer), Target::Object(triskelion)]
                    );
                    ordering = true;
                    break;
                }
                Decision::CastSpellOrPass { .. } => {}
                other => panic!("unexpected Hermit Druid resolution: {other:?}"),
            }
        }
        assert!(ordering);
        assert!(crate::full_text_v1::refuse_legacy(&state).is_err());
        assert_eq!(state.objects.get(basic).zone, Zone::Hand);
        let chooser = public::observe(&state, player).unwrap();
        let opponent = public::observe(&state, player.opponent()).unwrap();
        assert!(opponent.order_choice.is_none());
        assert_eq!(chooser.graveyards, opponent.graveyards);
        assert!(chooser.full_text.is_none());
        let graveyard = &chooser.graveyards[player.index()];
        assert_eq!(
            graveyard
                .bottom_to_top
                .iter()
                .map(|card| card.arena_id)
                .collect::<Vec<_>>(),
            [druid.0]
        );
        assert_eq!(graveyard.unplaced.len(), 2);
        let choice = chooser.order_choice.unwrap();
        assert_eq!(
            choice
                .native_candidates
                .iter()
                .map(|card| card.arena_id)
                .collect::<Vec<_>>(),
            [devourer.0, triskelion.0]
        );
        let saved = serde_json::to_vec(&state).unwrap();
        let mut altered: serde_json::Value = serde_json::from_slice(&saved).unwrap();
        let printing = altered
            .pointer_mut("/engine/graveyard_order_v1/pending/choice/remaining/0/printing")
            .unwrap();
        *printing = serde_json::json!(id("Forest"));
        let altered: GameState = serde_json::from_value(altered).unwrap();
        assert!(public::observe(&altered, player).is_err());
        assert!(public::observe(&altered, player.opponent()).is_err());
        for (ordinal, (first, top)) in [(devourer, triskelion), (triskelion, devourer)]
            .into_iter()
            .enumerate()
        {
            let mut restored: GameState = serde_json::from_slice(&saved).unwrap();
            assert_eq!(restored.state_hash(), state.state_hash());
            assert!(
                public::choose_order(&mut restored, player.opponent(), &choice, ordinal).is_err()
            );
            let mut forged = choice.clone();
            forged.native_candidates[ordinal].zone_change_count += 1;
            assert!(public::choose_order(&mut restored, player, &forged, ordinal).is_err());
            assert_eq!(restored.state_hash(), state.state_hash());
            public::choose_order(&mut restored, player, &choice, ordinal).unwrap();
            assert!(public::choose_order(&mut restored, player, &choice, ordinal).is_err());
            assert!(matches!(
                engine::advance_until_decision(&mut restored),
                Decision::CastSpellOrPass { .. }
            ));
            assert_eq!(
                restored.players[player.index()].graveyard,
                [druid, first, top]
            );
            assert_eq!(
                top_creature_text(&restored, host).unwrap().donor.object,
                top
            );
            assert_eq!(restored.objects.get(basic).zone, Zone::Hand);
            assert!(restored.stack.is_empty());
            assert!(restored.objects.get(source).tapped);
            let public_order = public::observe(&restored, player.opponent()).unwrap();
            let graveyard = &public_order.graveyards[player.index()];
            assert!(graveyard.unplaced.is_empty());
            assert_eq!(
                graveyard
                    .bottom_to_top
                    .iter()
                    .map(|card| card.arena_id)
                    .collect::<Vec<_>>(),
                [druid.0, first.0, top.0]
            );
        }
    }
}

#[test]
fn donated_devourer_uses_the_exiled_cost_value_after_the_graveyard_top_changes() {
    use crate::engine::{self, Action, Decision};
    use crate::state::Step;

    let mut state = game();
    state.step = Step::Main1;
    let host = host(&mut state);
    let donor = put(
        &mut state,
        PlayerId::P0,
        "Phyrexian Devourer",
        Zone::Graveyard,
    );
    let paid = put(&mut state, PlayerId::P0, "Triskelion", Zone::Library);
    let text = top_creature_text(&state, host).unwrap();
    engine::stage_full_text_activation(&mut state, text, FullTextAbilityV1::Printed(0)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.objects.get(paid).zone, Zone::Exile);
    let item = state.stack.last().unwrap();
    assert_eq!(item.v4.ability_source_contract, Some(host));
    assert_eq!(item.v4.paid_cost_refs.len(), 1);
    assert_eq!(item.v4.paid_cost_refs[0].object, paid);
    put(&mut state, PlayerId::P0, "Island", Zone::Graveyard);
    assert!(top_creature_text(&state, host).is_none());

    for _ in 0..2 {
        engine::step(&mut state, Action::Pass).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
    }
    assert!(state.stack.is_empty());
    assert_eq!(state.objects.get(host.source).counters.plus1_plus1, 6);
    assert_eq!(state.objects.get(donor).counters.plus1_plus1, 0);
}

#[test]
fn donated_psychatog_can_discard_above_or_exile_its_own_donor() {
    use crate::engine::{self, Action, CostKind, Decision};
    use crate::premodern_ordered_public_v1::{DonatedActivationV1, FullTextProgramV1};
    use crate::state::Step;

    for (index, effect) in [
        (0, DonatedActivationV1::PsychatogDiscardPump),
        (1, DonatedActivationV1::PsychatogExilePump),
    ] {
        let mut state = game();
        state.step = Step::Main1;
        let host = host(&mut state);
        let other_cost = put(&mut state, PlayerId::P0, "Island", Zone::Graveyard);
        put(&mut state, PlayerId::P0, "Forest", Zone::Graveyard);
        let donor = put(&mut state, PlayerId::P0, "Psychatog", Zone::Graveyard);
        let discard = put(&mut state, PlayerId::P0, "Island", Zone::Hand);
        let kept = put(&mut state, PlayerId::P0, "Forest", Zone::Hand);
        let before = engine::effective_power(&state, host.source);
        let text = top_creature_text(&state, host).unwrap();
        engine::stage_full_text_activation(&mut state, text, FullTextAbilityV1::Printed(index))
            .unwrap();
        if index == 0 {
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::Discard { count: 1, choices, .. }
                    if choices.contains(&discard) && choices.contains(&kept)
            ));
            engine::step(&mut state, Action::Discard(vec![discard])).unwrap();
            assert_eq!(state.objects.get(discard).zone, Zone::Graveyard);
            assert_eq!(state.objects.get(kept).zone, Zone::Hand);
        } else {
            for cost in [donor, other_cost] {
                assert!(matches!(engine::advance_until_decision(&mut state),
                    Decision::ChooseCostTargets { cost_kind: CostKind::ExileFromGraveyard, candidates, .. }
                        if candidates.contains(&cost)));
                engine::step(&mut state, Action::ChooseCostTarget(cost)).unwrap();
            }
        }
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        assert!(top_creature_text(&state, host).is_none());
        let item = state.stack.last().unwrap();
        assert_eq!(item.v4.ability_source_contract, Some(host));
        assert_eq!(item.v4.activated_ability_index, Some(index + 3));
        assert_eq!(item.v4.paid_cost_refs.len(), if index == 0 { 1 } else { 2 });
        let rows = public_producers(&state).unwrap();
        let [producer] = rows.as_slice() else {
            panic!("expected the paid Psychatog ability");
        };
        assert_eq!(
            producer.program,
            FullTextProgramV1::DonatedActivation {
                printed_index: index,
                effect,
            }
        );
        assert_eq!(producer.text_donor.arena_id, donor.0);
        assert_eq!(producer.text_donor.zone, Zone::Graveyard);
        if index == 1 {
            assert_eq!(state.objects.get(donor).zone, Zone::Exile);
            assert!(item
                .v4
                .paid_cost_refs
                .iter()
                .any(|paid| paid.object == donor));
        }
        for _ in 0..2 {
            engine::step(&mut state, Action::Pass).unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
        }
        assert!(state.stack.is_empty());
        assert_eq!(engine::effective_power(&state, host.source), before + 1);
    }
}

#[test]
fn donated_scrounger_requires_threshold_and_counts_the_original_donors_uses() {
    use crate::engine::{self, Action, CostKind, Decision};
    use crate::state::Step;

    let player = PlayerId::P0;
    let selector = FullTextAbilityV1::Printed(0);
    let mut state = game();
    state.step = Step::Main1;
    let host = host(&mut state);
    let mut costs = (0..5)
        .map(|_| put(&mut state, player, "Island", Zone::Graveyard))
        .collect::<Vec<_>>();
    put(&mut state, player, "Battlefield Scrounger", Zone::Graveyard);
    let first = top_creature_text(&state, host).unwrap();
    assert!(engine::stage_full_text_activation(&mut state, first, selector).is_err());
    assert!(state.engine.pending_activation.is_none());

    for _ in 0..4 {
        costs.push(put(&mut state, player, "Forest", Zone::Graveyard));
    }
    put(&mut state, player, "Battlefield Scrounger", Zone::Graveyard);
    let paid_donor = top_creature_text(&state, host).unwrap();
    engine::stage_full_text_activation(&mut state, paid_donor, selector).unwrap();
    for &cost in &costs[..3] {
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::ChooseCostTargets { cost_kind: CostKind::ExileFromGraveyard, candidates, .. }
                if candidates.contains(&cost)));
        engine::step(&mut state, Action::ChooseCostTarget(cost)).unwrap();
    }
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(costs[..3]
        .iter()
        .all(|&cost| state.objects.get(cost).zone == Zone::Library));
    assert!(state.players[player.index()].graveyard.len() >= 7);
    assert!(engine::stage_full_text_activation(&mut state, paid_donor, selector).is_err());
    put(&mut state, player, "Battlefield Scrounger", Zone::Graveyard);
    let another = top_creature_text(&state, host).unwrap();
    engine::stage_full_text_activation(&mut state, another, selector).unwrap();
}

#[test]
fn full_text_stack_rejects_changed_producer_slot_and_mixed_grant_provenance() {
    let mut state = game();
    let host = host(&mut state);
    put(
        &mut state,
        PlayerId::P0,
        "Birds of Paradise",
        Zone::Graveyard,
    );
    let frozen = top_creature_text(&state, host)
        .unwrap()
        .freeze_activated(&state, FullTextAbilityV1::RetainedDiscard)
        .unwrap();
    let item = paid_discard_item(&mut state, frozen);
    let validate = |item: &crate::state::StackItem| {
        crate::engine::validated_stack_item_target_spec(item, &state)
    };
    let mut changed = item.clone();
    changed.v4.activated_ability_index = Some(1);
    assert!(validate(&changed).is_err());
    changed.v4.activated_ability_index = Some(0);
    // An archived layout cannot overlap an existing physical slot.
    changed
        .v4
        .full_text_activation_v1
        .0
        .as_mut()
        .unwrap()
        .first_donated_slot = 0;
    assert!(validate(&changed).is_err());
    let mut changed = item.clone();
    changed
        .v4
        .full_text_activation_v1
        .0
        .as_mut()
        .unwrap()
        .host
        .controller = PlayerId::P1;
    assert!(validate(&changed).is_err());
    let mut changed = item.clone();
    changed
        .v4
        .full_text_activation_v1
        .0
        .as_mut()
        .unwrap()
        .donor
        .card_def = id("Island");
    assert!(validate(&changed).is_err());
    let mut changed = item.clone();
    changed.v4.granted_by = Some(host);
    assert!(validate(&changed).is_err());
    let mut changed = item;
    changed.kind = crate::state::StackItemKind::TriggeredAbility;
    assert!(validate(&changed).is_err());
}

#[test]
fn full_text_stack_roundtrip_preserves_producer_and_legacy_observation_refuses_it() {
    let mut state = game();
    let host = host(&mut state);
    put(
        &mut state,
        PlayerId::P0,
        "Birds of Paradise",
        Zone::Graveyard,
    );
    let frozen = top_creature_text(&state, host)
        .unwrap()
        .freeze_activated(&state, FullTextAbilityV1::RetainedDiscard)
        .unwrap();
    let item = paid_discard_item(&mut state, frozen);
    let json = serde_json::to_value(&item).unwrap();
    let restored: crate::state::StackItem = serde_json::from_value(json).unwrap();
    assert_eq!(item, restored);
    assert!(crate::engine::validated_stack_item_target_spec(&restored, &state).is_ok());
    state.stack.push(restored);
    for observer in [PlayerId::P0, PlayerId::P1] {
        let error = crate::rl::observe_v1(&state, observer, 0).unwrap_err();
        assert!(error.0.contains("full-text ability producers"));
    }
}

#[test]
fn full_text_batch_keywords_follow_paid_host_donor_and_layer_six_both_seats() {
    use crate::card_def::Keywords;
    use crate::engine::{self, Action, Decision, RuntimeRulesV1};
    use crate::mana::ManaColor;
    use crate::state::Step;

    fn check(state: &GameState, source: ObjectId, expected: Keywords) {
        let wanted = Keywords::DEFENDER | Keywords::FLYING;
        assert_eq!(engine::effective_keywords(state, source, wanted), expected);
        for keyword in [Keywords::DEFENDER, Keywords::FLYING] {
            assert_eq!(engine::has_effective_keyword(state, source, keyword), expected.has(keyword));
        }
        assert_eq!(state.objects.get(source).card_def, id(VOLRATH));
        let restored: GameState = serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
        assert_eq!(engine::effective_keywords(&restored, source, wanted), expected);
        assert_eq!(state.state_hash(), restored.state_hash());
    }

    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = game();
        state.step = Step::Main1;
        state.active_player = player;
        state.priority_player = player;
        state.engine.runtime_rules = RuntimeRulesV1::OWNER_ORDERED_GRAVEYARD_V1;
        put(&mut state, player, "Wall of Roots", Zone::Graveyard);
        let source = put(&mut state, player, VOLRATH, Zone::Hand);
        state.players[player.index()].mana_pool[ManaColor::U.pool_index()] = 2;
        state.players[player.index()].mana_pool[ManaColor::C.pool_index()] = 1;
        assert!(matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&source)));
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        for _ in 0..8 {
            let decision = engine::advance_until_decision(&mut state);
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }), "{decision:?}");
            if state.stack.is_empty() { break; }
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert!(state.stack.is_empty());
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
        assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
        check(&state, source, Keywords::DEFENDER);

        let bird = put(&mut state, player, "Birds of Paradise", Zone::Hand);
        event::commit(&mut state, ProposedEvent::zone_change(bird, Zone::Graveyard));
        check(&state, source, Keywords::FLYING);
        let land = put(&mut state, player, "Island", Zone::Hand);
        event::commit(&mut state, ProposedEvent::zone_change(land, Zone::Graveyard));
        assert!(active_definition(&state, source).is_none());
        check(&state, source, Keywords::NONE);
        event::commit(&mut state, ProposedEvent::zone_change(land, Zone::Exile));
        check(&state, source, Keywords::FLYING);

        let dress = put(&mut state, player.opponent(), "Dress Down", Zone::Hand);
        event::commit(&mut state, ProposedEvent::zone_change(dress, Zone::Battlefield));
        // Install the native layer-six removal as in the full-text layer fixture.
        let _ = crate::trigger::collect_and_process(&mut state);
        assert_eq!(active_definition(&state, source).unwrap().name, "Birds of Paradise");
        check(&state, source, Keywords::NONE);
    }
}
