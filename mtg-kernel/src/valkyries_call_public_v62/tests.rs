use super::*;
use crate::{
    card_def::{self, CardCapability, CardType, Keywords, Subtype},
    engine::{self, Action, Decision},
    graveyard_return_public_v61::tests::{announce, paid, quiet, settle},
    policy_observation_v6::tests::put,
    state::{FaceDownV1, Target},
};

fn ready(actor: PlayerId) -> GameState {
    let mut state = crate::graveyard_return_public_v61::tests::ready(actor);
    for seat in [actor, actor.opponent()] {
        for _ in 0..8 {
            put(&mut state, seat, "Plains", Zone::Battlefield);
        }
    }
    state
}
fn restore(state: &GameState) -> GameState {
    let before = serde_json::to_vec(state).unwrap();
    let restored: GameState = serde_json::from_slice(&before).unwrap();
    for viewer in [PlayerId::P0, PlayerId::P1] {
        let facts = capture(state, viewer).unwrap();
        assert_eq!(facts, capture(&restored, viewer).unwrap());
        validate(state, viewer, &facts).unwrap();
        let roundtrip: FactsV62 =
            serde_json::from_slice(&serde_json::to_vec(&facts).unwrap()).unwrap();
        assert_eq!(roundtrip, facts);
        assert!(!facts.complete_ordinary_observation && !facts.owning_consumer_admitted);
    }
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
    assert_eq!(state.state_hash(), restored.state_hash());
    restored
}
fn published(state: &mut GameState, count: usize) {
    for _ in 0..100 {
        let d = engine::advance_until_decision(state);
        let facts = capture(state, state.active_player).unwrap();
        if facts.returns.len() == count
            && facts.returns.iter().all(|r| r.stage == StageV62::Published)
        {
            assert!(matches!(d, Decision::CastSpellOrPass { .. }));
            return;
        }
        quiet(state, d);
    }
    panic!("genuine Call publication absent");
}
fn assert_readonly_tamper(state: &GameState, viewer: PlayerId) {
    let bytes = serde_json::to_vec(state).unwrap();
    let facts = capture(state, viewer).unwrap();
    let mut wrong = facts.clone();
    wrong.deaths[0].original_graveyard.generation += 1;
    assert!(validate(state, viewer, &wrong).is_err());
    let mut wrong = facts.clone();
    wrong.returns[0].under_physical_owner = viewer.opponent().into();
    // The selected fixture has its own physical owner.
    assert!(validate(state, viewer, &wrong).is_err());
    let mut wrong = facts.clone();
    wrong.returns[0].death_occurrence = u32::MAX;
    assert!(validate(state, viewer, &wrong).is_err());
    let mut raw = serde_json::to_value(&facts).unwrap();
    raw["invented_target"] = true.into();
    assert!(serde_json::from_value::<FactsV62>(raw).is_err());
    let mut raw = serde_json::to_value(&facts).unwrap();
    raw["deaths"][0]["departure"]["card_id"] = 459.into();
    assert!(serde_json::from_value::<FactsV62>(raw).is_err());
    let selected = state
        .stack
        .iter()
        .position(|row| {
            matches!(
                row.inline_effect,
                Some(EffectOp::ReturnValkyriesCallCreatureV1 { .. })
            )
        })
        .unwrap();
    for edit in 0..4 {
        let mut bad = state.clone();
        let EffectOp::ReturnValkyriesCallCreatureV1 {
            source,
            capture_history_index,
            graveyard,
            owner,
        } = bad.stack[selected].inline_effect.as_mut().unwrap()
        else {
            unreachable!()
        };
        match edit {
            0 => source.controller = source.controller.opponent(),
            1 => *capture_history_index = u64::MAX,
            2 => graveyard.zone_change_count += 1,
            _ => *owner = owner.opponent(),
        }
        let before = serde_json::to_vec(&bad).unwrap();
        assert!(engine::validated_stack_item_target_spec(&bad.stack[selected], &bad).is_err());
        assert!(capture(&bad, viewer).is_err());
        assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
    }
    let mut bad = state.clone();
    let marker = bad.engine.event_history.iter().find(|event| matches!(event, E::ControlledCreatureDeathCaptureV1 { source, .. } if source.card_def == 459)).unwrap().clone();
    bad.engine.event_history.push(marker);
    let before = serde_json::to_vec(&bad).unwrap();
    assert!(capture(&bad, viewer).is_err());
    assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
    let mut bad = state.clone();
    bad.stack.push(bad.stack[selected].clone());
    let before = serde_json::to_vec(&bad).unwrap();
    assert!(capture(&bad, viewer).is_err());
    assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
    assert_eq!(serde_json::to_vec(state).unwrap(), bytes);
}

#[test]
fn closed_printed_call_program_and_unknown_fields_refuse_without_consumer_admission() {
    let meaning = meaning::printed_meaning().unwrap();
    assert_eq!(meaning.card_id, 459);
    assert_eq!(meaning.printed.generic_mana, 3);
    assert_eq!(
        meaning.printed.colored_mana,
        vec![crate::mana::ManaColor::W; 2]
    );
    assert_eq!(meaning.printed.types, vec![CardType::Enchantment]);
    assert_eq!(card_def::CARD_DEFS[459].capability, CardCapability::Partial);
    assert!(card_def::preflight_fully_supported_deck(&[459]).is_err());
    assert!(
        !meaning.full_engine_admitted
            && !meaning.owning_consumer_admitted
            && !meaning.model_admitted
            && !meaning.sampler_admitted
    );
    meaning::validate(&meaning).unwrap();
    let mut bad = meaning.clone();
    bad.ordered_rules.reverse();
    assert!(meaning::validate(&bad).is_err());
    let mut bad = meaning.clone();
    bad.printed.generic_mana += 1;
    assert!(meaning::validate(&bad).is_err());
    let mut raw = serde_json::to_value(&meaning).unwrap();
    raw["permission"] = true.into();
    assert!(serde_json::from_value::<meaning::MeaningV62>(raw).is_err());
}
#[test]
fn paid_simultaneous_deaths_native_order_source_departure_response_restore_and_readonly_forgery_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let call = put(&mut state, actor, "Valkyrie's Call", Zone::Hand);
        announce(&mut state, actor, call, None);
        let facts = capture(&state, actor).unwrap();
        assert_eq!(facts.casts.len(), 1);
        assert_eq!(facts.casts[0].source.object, call.0);
        assert_eq!(facts.casts[0].original_controller, actor.into());
        assert!(facts.casts[0].genuine_cast_finalized);
        assert!(!facts.casts[0].is_uncast_copy && !facts.casts[0].payment_pending);
        restore(&state);
        settle(&mut state);
        assert!(crate::graveyard_return_observation_v61::validate_state(&state).is_err());
        assert!(crate::next_five_observation_v59::validate_state(&state).is_err());
        let one = paid(&mut state, actor, "Llanowar Elves", None);
        let two = paid(&mut state, actor, "Llanowar Elves", None);
        let breath = put(&mut state, actor.opponent(), "Breath Weapon", Zone::Hand);
        announce(&mut state, actor.opponent(), breath, None);
        for _ in 0..80 {
            let d = engine::advance_until_decision(&mut state);
            if let Decision::OrderTriggers { ref pending, .. } = d {
                assert_eq!(pending.len(), 2);
                let facts = capture(&state, actor).unwrap();
                assert_eq!(facts.deaths.len(), 2);
                assert!(facts.returns.iter().all(|r| r.stage == StageV62::Held));
                restore(&state);
                engine::step(&mut state, Action::OrderTriggers(vec![1, 0])).unwrap();
                break;
            }
            quiet(&mut state, d);
        }
        published(&mut state, 2);
        let facts = capture(&state, actor).unwrap();
        assert_eq!(facts.deaths.len(), 2);
        for row in &facts.returns {
            let native = &state.stack[row.position as usize];
            assert_eq!(native.source, call);
            assert_eq!(native.controller, actor);
            assert!(native.targets.is_empty());
            assert!(row.return_eligible_now);
        }
        assert_readonly_tamper(&state, actor);
        let removal = put(&mut state, actor.opponent(), "Broken Wings", Zone::Hand);
        announce(&mut state, actor.opponent(), removal, Some(call));
        for _ in 0..40 {
            let d = engine::advance_until_decision(&mut state);
            if state.objects.get(call).zone == Zone::Graveyard {
                break;
            }
            quiet(&mut state, d);
        }
        assert_eq!(state.objects.get(call).zone, Zone::Graveyard);
        let facts = capture(&state, actor).unwrap();
        assert_eq!(facts.returns.len(), 2);
        assert!(facts
            .returns
            .iter()
            .all(|row| row.source.current_original_source.is_none()));
        let mut replay = restore(&state);
        settle(&mut state);
        settle(&mut replay);
        assert_eq!(state, replay);
        for id in [one, two] {
            let current = capture(&state, actor).unwrap().battlefield[actor.index()]
                .iter()
                .find(|row| row.card.object == id.0)
                .unwrap()
                .clone();
            assert_eq!(current.counters.plus_one, 1);
            assert!(current.form.subtype_ids.contains(&(Subtype::Angel as u16)));
            assert!(current.form.subtype_ids.contains(&(Subtype::Elf as u16)));
            assert_ne!(current.form.keyword_bits & Keywords::FLYING.0, 0);
        }
    }
}
#[test]
fn paid_foreign_physical_owner_season_return_and_explicit_stale_zone_primitive_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let owner = actor.opponent();
        let mut state = ready(actor);
        crate::policy_observation_v6::tests::advance_to_next_main_for(&mut state, owner);
        paid(&mut state, owner, "Doubling Season", None);
        let elf = paid(&mut state, owner, "Llanowar Elves", None);
        crate::policy_observation_v6::tests::advance_to_next_main_for(&mut state, actor);
        let call = paid(&mut state, actor, "Valkyrie's Call", None);
        paid(&mut state, actor, "Involuntary Employment", Some(elf));
        assert_eq!(state.objects.get(elf).controller, actor);
        // Explicit native death/collection primitive on this genuinely paid
        // creature and control history, before any printed removal is cast.
        let mut uncollected = state.clone();
        crate::event::propose_and_commit(
            &mut uncollected,
            crate::event::ProposedEvent::zone_change(elf, Zone::Graveyard),
        );
        let facts = capture(&uncollected, actor).unwrap();
        assert_eq!(facts.deaths.len(), 1);
        assert!(!facts.deaths[0].collection_completed && facts.returns.is_empty());
        let mut bad = uncollected.clone();
        bad.engine.event_log.retain(|row| !matches!(row, E::ControlledCreatureDeathCaptureV1 { source, .. } if source.card_def == 459));
        let before = serde_json::to_vec(&bad).unwrap();
        assert!(capture(&bad, actor).is_err());
        assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
        // Preserve the genuine marker and all independent history while an
        // extra current-log row attempts to invent another source or death.
        let marker = uncollected.engine.event_log.iter().find(|row|
            matches!(row, E::ControlledCreatureDeathCaptureV1 { source, .. } if source.card_def == 459)
        ).unwrap().clone();
        for mutation in 0..3 {
            let mut bad = uncollected.clone();
            let mut extra = marker.clone();
            let E::ControlledCreatureDeathCaptureV1 { source, died, .. } = &mut extra else {
                unreachable!()
            };
            match mutation {
                0 => source.controller = source.controller.opponent(),
                1 => died.zone_change_count += 1,
                _ => {}
            }
            bad.engine.event_log.push(extra);
            assert_eq!(bad.engine.event_history, uncollected.engine.event_history);
            let before = serde_json::to_vec(&bad).unwrap();
            assert!(crate::creature_death_capture_v1::pending(
                &bad,
                &bad.engine.event_log,
                bad.engine.event_log.len() - 1
            )
            .is_err());
            for viewer in [actor, owner] {
                assert!(capture(&bad, viewer).is_err());
            }
            assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
        }
        let triggers = crate::trigger::collect_and_process(&mut uncollected);
        uncollected.engine.pending_triggers.extend(triggers);
        let facts = capture(&uncollected, actor).unwrap();
        assert!(facts.deaths[0].collection_completed);
        assert_eq!(facts.returns[0].stage, StageV62::Held);
        restore(&uncollected);
        let removal = put(&mut state, owner, "Hero's Downfall", Zone::Hand);
        announce(&mut state, owner, removal, Some(elf));
        published(&mut state, 1);
        let facts = capture(&state, actor).unwrap();
        assert_eq!(facts.deaths[0].source.original_source.object, call.0);
        assert_eq!(facts.deaths[0].source.captured_controller, actor.into());
        assert_eq!(facts.deaths[0].departure.controller, actor.into());
        assert_eq!(facts.deaths[0].departure.owner, owner.into());
        assert_eq!(facts.returns[0].under_physical_owner, owner.into());
        let mut stale = restore(&state);
        // Explicit native zone-change primitive, not a claimed printed instant.
        crate::event::propose_and_commit(
            &mut stale,
            crate::event::ProposedEvent::zone_change(elf, Zone::Exile),
        );
        let stale_facts = capture(&stale, actor).unwrap();
        assert!(!stale_facts.returns[0].return_eligible_now);
        assert!(stale_facts.deaths[0].current_exact_graveyard_card.is_none());
        settle(&mut stale);
        assert_eq!(stale.objects.get(elf).zone, Zone::Exile);
        let mut replay = restore(&state);
        settle(&mut state);
        settle(&mut replay);
        assert_eq!(state, replay);
        let facts = capture(&state, actor).unwrap();
        let row = facts.battlefield[owner.index()]
            .iter()
            .find(|row| row.card.object == elf.0)
            .unwrap();
        assert_eq!(row.counters.plus_one, 2);
        assert_eq!(row.card.controller, owner.into());
        assert!(row.form.subtype_ids.contains(&(Subtype::Angel as u16)));
        // Now an Angel, its next genuine death must not create a new Call row.
        paid(&mut state, actor, "Hero's Downfall", Some(elf));
        let facts = capture(&state, actor).unwrap();
        assert_eq!(facts.deaths.len(), 1);
        assert!(facts.returns.is_empty());
    }
}
#[test]
fn paid_replication_creates_real_call_copy_and_distinct_original_sources_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let call = paid(&mut state, actor, "Valkyrie's Call", None);
        let elf = paid(&mut state, actor, "Llanowar Elves", None);
        let replication = paid(&mut state, actor, "Extravagant Replication", None);
        let mut selected = false;
        for _ in 0..256 {
            let d = engine::advance_until_decision(&mut state);
            if let Decision::ChooseTargets {
                spell,
                ref legal_targets,
                ..
            } = d
            {
                assert_eq!(spell, replication);
                assert!(legal_targets.contains(&Target::Object(call)));
                engine::step(&mut state, Action::ChooseTarget(Target::Object(call))).unwrap();
                selected = true;
                break;
            }
            quiet(&mut state, d);
        }
        assert!(selected);
        settle(&mut state);
        let facts = capture(&state, actor).unwrap();
        let copies = facts.battlefield[actor.index()]
            .iter()
            .filter(|row| {
                row.is_token
                    && matches!(
                        row.card.identity,
                        public::IdentityV1::Known { card_id: 459 }
                    )
            })
            .collect::<Vec<_>>();
        assert_eq!(copies.len(), 1);
        let copied = copies[0].card.object;
        assert_ne!(copied, call.0);
        assert!(copies[0].form.printed_abilities_active);
        let removal = put(&mut state, actor.opponent(), "Hero's Downfall", Zone::Hand);
        announce(&mut state, actor.opponent(), removal, Some(elf));
        published(&mut state, 2);
        let facts = capture(&state, actor).unwrap();
        let sources = facts
            .returns
            .iter()
            .map(|r| r.source.original_source.object)
            .collect::<BTreeSet<_>>();
        assert_eq!(sources, BTreeSet::from([call.0, copied]));
        assert_eq!(facts.deaths.len(), 2);
        let mut replay = restore(&state);
        settle(&mut state);
        settle(&mut replay);
        assert_eq!(state, replay);
        assert_eq!(state.objects.get(elf).counters.plus1_plus1, 1);
    }
}
#[test]
fn paid_original_call_spell_countered_before_entry_has_no_death_observer_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let call = put(&mut state, actor, "Valkyrie's Call", Zone::Hand);
        announce(&mut state, actor, call, None);
        assert_eq!(capture(&state, actor).unwrap().casts.len(), 1);
        restore(&state);
        paid(&mut state, actor.opponent(), "Counterspell", Some(call));
        assert_eq!(state.objects.get(call).zone, Zone::Graveyard);
        assert!(capture(&state, actor).unwrap().casts.is_empty());
        let elf = paid(&mut state, actor, "Llanowar Elves", None);
        paid(&mut state, actor.opponent(), "Hero's Downfall", Some(elf));
        let facts = capture(&state, actor).unwrap();
        assert!(facts.deaths.is_empty() && facts.returns.is_empty());
        restore(&state);
    }
}
#[test]
fn independent_hidden_worlds_neutral_face_down_and_all17_counter_storage_primitives_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let build = |hidden_hand: &str, concealed_battlefield: &str| {
            let mut state = ready(actor);
            put(&mut state, actor.opponent(), hidden_hand, Zone::Hand);
            put(&mut state, actor.opponent(), hidden_hand, Zone::Library);
            // Explicit initial face-down native primitive, no paid manifest claim.
            let concealed = put(
                &mut state,
                actor.opponent(),
                concealed_battlefield,
                Zone::Battlefield,
            );
            state.objects.get_mut(concealed).v4.face_down_v1 = Some(FaceDownV1 {
                disguised: false,
                lookers: 0,
                hidden_by: None,
                hidden_by_source: None,
            });
            let call = paid(&mut state, actor, "Valkyrie's Call", None);
            (state, call, concealed)
        };
        let (mut state, call, concealed) = build("Valkyrie's Call", "Valkyrie's Call");
        let (other, _, _) = build("Lightning Bolt", "Nine-Lives Familiar");
        assert_eq!(
            capture(&state, actor).unwrap(),
            capture(&other, actor).unwrap()
        );
        let facts = capture(&state, actor).unwrap();
        assert!(!facts
            .visible_printings
            .iter()
            .any(|row| row.object == concealed.0));
        let face = facts.battlefield[actor.opponent().index()]
            .iter()
            .find(|row| row.card.object == concealed.0)
            .unwrap();
        assert!(matches!(
            face.card.identity,
            public::IdentityV1::FaceDown { .. }
        ));
        // This scene uses the neutral face-down card form (disguised=false),
        // not the distinct anonymous 2/2 creature form. Neither hidden
        // printing contributes power, toughness or a public card type.
        assert_eq!(face.form.base_power, None);
        assert_eq!(face.form.base_toughness, None);
        assert_eq!(face.form.power, None);
        assert_eq!(face.form.toughness, None);
        assert!(face.form.types.is_empty());
        assert!(face.form.names.is_empty());
        let land = state.players[actor.index()]
            .battlefield
            .iter()
            .copied()
            .find(|id| {
                state.objects.get(*id).card_def == card_def::card_id_by_name("Forest").unwrap()
            })
            .unwrap();
        // Explicit native counter-storage primitives on a noncreature. No land ability is invented.
        state.objects.get_mut(land).counters = Counters {
            plus1_plus1: i32::MAX,
            minus1_minus1: i16::MAX,
            minus0_minus1: i16::MAX,
            stun: i16::MAX,
            lore: i16::MAX,
            oil: i16::MAX,
            charge: i32::MAX,
            net: i32::MAX,
            bait: i32::MAX,
            soul: i32::MAX,
            incubation: i32::MAX,
            revival: i32::MAX,
            fellowship: i32::MAX,
            stash: 0,
        };
        {
            let live = state.objects.get_mut(land);
            live.damage = u32::MAX;
            live.v4.lifelink_keyword_counters = i16::MAX;
            live.v4.time_counters_v1 = u8::MAX;
            live.v4.creature_upgrade = Some(crate::standard_creatures_v1::CreatureUpgradeV1 {
                finality: u16::MAX,
                ..Default::default()
            });
        }
        crate::planeswalker_v1::add_loyalty_counters_checked_v1(&mut state, land, u32::MAX)
            .unwrap();
        let facts = capture(&state, actor).unwrap();
        let row = facts.battlefield[actor.index()]
            .iter()
            .find(|row| row.card.object == land.0)
            .unwrap();
        assert_eq!(row.counters.fellowship, i32::MAX);
        assert_eq!(row.counters.revival, i32::MAX);
        assert_eq!(row.counters.loyalty, u32::MAX);
        assert_eq!(
            row.counters,
            CountersV62 {
                plus_one: i32::MAX,
                minus_one: i16::MAX,
                minus_toughness: i16::MAX,
                stun: i16::MAX,
                lore: i16::MAX,
                oil: i16::MAX,
                charge: i32::MAX,
                net: i32::MAX,
                bait: i32::MAX,
                soul: i32::MAX,
                incubation: i32::MAX,
                revival: i32::MAX,
                fellowship: i32::MAX,
                lifelink: i16::MAX,
                time: u8::MAX,
                finality: u16::MAX,
                loyalty: u32::MAX,
            }
        );
        assert_eq!(
            serde_json::to_value(&row.counters)
                .unwrap()
                .as_object()
                .unwrap()
                .len(),
            17
        );
        restore(&state);
        let mut bad = state.clone();
        bad.objects.get_mut(land).counters.fellowship = -1;
        let before = serde_json::to_vec(&bad).unwrap();
        assert!(capture(&bad, actor).is_err());
        assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
        // Explicit native suppression record, not a claimed printed land action.
        let link = ObjectLinkV4 {
            object: land,
            zone_change_count: state.objects.get(land).zone_change_count,
        };
        state
            .objects
            .get_mut(call)
            .v4
            .creature_upgrade
            .get_or_insert_with(Default::default)
            .suppressed_by
            .push(crate::standard_creatures_v1::TidebinderSuppressionV1 {
                source: link,
                timestamp: u64::MAX,
            });
        let facts = capture(&state, actor).unwrap();
        assert!(
            !facts.battlefield[actor.index()]
                .iter()
                .find(|row| row.card.object == call.0)
                .unwrap()
                .form
                .printed_abilities_active
        );
        restore(&state);
    }
}
