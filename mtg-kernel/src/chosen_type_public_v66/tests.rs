use super::*;
use crate::{
    engine::{self, Action, Decision},
    event::{self, ProposedEvent},
    graveyard_return_public_v61::tests::{announce, paid, quiet, settle},
    policy_observation_v6::tests::put,
    state::{FaceDownV1, Step, Target},
};
fn ready(actor: PlayerId) -> GameState {
    crate::graveyard_return_public_v61::tests::ready(actor)
}
fn restore(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let result: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(*state, result);
    for viewer in [PlayerId::P0, PlayerId::P1] {
        let facts = capture(state, viewer).unwrap();
        assert_eq!(facts, capture(&result, viewer).unwrap());
        validate(state, viewer, &facts).unwrap();
        assert_eq!(
            serde_json::from_slice::<FactsV66>(&serde_json::to_vec(&facts).unwrap()).unwrap(),
            facts
        );
        assert!(!facts.complete_ordinary_observation && !facts.owning_consumer_admitted);
    }
    assert_eq!(serde_json::to_vec(state).unwrap(), bytes);
    result
}
fn priority(state: &mut GameState, actor: PlayerId) {
    for _ in 0..16 {
        let decision = engine::advance_until_decision(state);
        if matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor) {
            return;
        }
        quiet(state, decision);
    }
    panic!("actual actor priority absent")
}
fn reach_type(state: &mut GameState, actor: PlayerId) {
    for _ in 0..48 {
        let decision = engine::advance_until_decision(state);
        if let Decision::ChooseEntryCreatureTypeV1 {
            player,
            entry_ordinal,
            types,
        } = &decision
        {
            assert_eq!(*player, actor);
            assert_eq!(types.as_slice(), CreatureTypeV60::ALL);
            for viewer in [actor, actor.opponent()] {
                let facts = capture(state, viewer).unwrap();
                let choice = facts.active_entry_choice.unwrap();
                assert_eq!(choice.controller, actor.into());
                assert_eq!(choice.entry_ordinal, *entry_ordinal);
                assert_eq!(choice.domain, *types);
            }
            return;
        }
        quiet(state, decision);
    }
    panic!("actual complete creature-type choice absent")
}
fn choose(state: &mut GameState, chosen: CreatureTypeV60) {
    engine::step(state, Action::ChooseEffectOption(chosen.choice_index())).unwrap();
    assert!(state.engine.halted.is_none());
}
fn banner(state: &mut GameState, actor: PlayerId, chosen: CreatureTypeV60) -> ObjectId {
    let source = put(state, actor, "Banner of Kinship", Zone::Hand);
    announce(state, actor, source, None);
    let before = capture(state, actor).unwrap();
    assert!(before.casts.iter().any(|row| row.source.object == source.0
        && !row.payment_pending
        && !row.is_uncast_copy
        && row.genuine_cast_finalized));
    reach_type(state, actor);
    choose(state, chosen);
    settle(state);
    source
}
fn courtyard(state: &mut GameState, actor: PlayerId, chosen: CreatureTypeV60) -> ObjectId {
    priority(state, actor);
    let source = put(state, actor, "Secluded Courtyard", Zone::Hand);
    engine::step(state, Action::PlayLand(source)).unwrap();
    reach_type(state, actor);
    let mut replay = restore(state);
    choose(state, chosen);
    choose(&mut replay, chosen);
    assert_eq!(*state, replay);
    priority(state, actor);
    assert_eq!(state.players[actor.index()].lands_played_this_turn, 1);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    source
}
fn refused(state: &GameState) {
    let before = serde_json::to_vec(state).unwrap();
    for viewer in [PlayerId::P0, PlayerId::P1] {
        assert!(capture(state, viewer).is_err());
    }
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
}
#[test]
fn closed_two_printed_programs_complete_domain_unknown_keys_and_no_admission() {
    assert_eq!(CreatureTypeV60::ALL.len(), 324);
    for family in [FamilyV66::BannerOfKinship, FamilyV66::SecludedCourtyard] {
        let meaning = meaning::printed_meaning(family).unwrap();
        meaning::validate(&meaning).unwrap();
        assert!(
            !meaning.full_engine_admitted
                && !meaning.owning_consumer_admitted
                && !meaning.model_admitted
                && !meaning.sampler_admitted
        );
        assert!(crate::card_def::preflight_fully_supported_deck(&[family.card_id()]).is_err());
        let mut changed = meaning.clone();
        changed.ordered_rules.reverse();
        assert!(meaning::validate(&changed).is_err());
        let mut changed = meaning.clone();
        changed.generic_mana += 1;
        assert!(meaning::validate(&changed).is_err());
        let mut raw = serde_json::to_value(&meaning).unwrap();
        raw["ordinary_permission"] = true.into();
        assert!(serde_json::from_value::<meaning::MeaningV66>(raw).is_err());
    }
}
#[test]
fn paid_banner_original_response_choice_scene_fellowship_boost_restore_and_forgery_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let elf = paid(&mut state, actor, "Llanowar Elves", None);
        paid(&mut state, actor, "Doubling Season", None);
        let source = put(&mut state, actor, "Banner of Kinship", Zone::Hand);
        announce(&mut state, actor, source, None);
        // The genuinely paid spell is still counterable before the entry choice.
        assert_eq!(capture(&state, actor).unwrap().casts.len(), 1);
        let mut countered = restore(&state);
        engine::step(&mut countered, Action::Pass).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut countered), Decision::CastSpellOrPass { player, .. } if player == actor.opponent())
        );
        let counter = put(&mut countered, actor.opponent(), "Counterspell", Zone::Hand);
        engine::step(&mut countered, Action::CastSpell(counter)).unwrap();
        let row_id = countered
            .stack
            .iter()
            .find(|row| row.source == source)
            .unwrap()
            .v4
            .stack_item_id;
        assert!(
            matches!(engine::advance_until_decision(&mut countered), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::StackItem(row_id)))
        );
        engine::step(
            &mut countered,
            Action::ChooseTarget(Target::StackItem(row_id)),
        )
        .unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut countered), Decision::CastSpellOrPass { player, .. } if player == actor.opponent())
        );
        assert_eq!(countered.objects.get(counter).zone, Zone::Stack);
        settle(&mut countered);
        assert_eq!(countered.objects.get(source).zone, Zone::Graveyard);
        assert!(capture(&countered, actor)
            .unwrap()
            .completed_entries
            .is_empty());
        reach_type(&mut state, actor);
        let pending = restore(&state);
        for edit in 0..5 {
            let mut bad = pending.clone();
            let p = bad
                .engine
                .linked_aura_operation_v1
                .pending_kinship_v1
                .as_mut()
                .unwrap();
            let GeneralChoiceV36::KinshipV1 { choice, capture } = &mut p.choice else {
                panic!("real type choice")
            };
            match edit {
                0 => choice.controller = actor.opponent(),
                1 => choice.counts[usize::from(CreatureTypeV60::Elf.choice_index())] += 1,
                2 => choice.original.as_mut().unwrap().zone_change_count += 1,
                3 => *capture += 1,
                _ => p.original.objects.get_mut(source).owner = actor.opponent(),
            }
            refused(&bad);
        }
        for index in [324, u16::MAX] {
            let mut bad = pending.clone();
            assert!(engine::step(&mut bad, Action::ChooseEffectOption(index)).is_err());
            assert_eq!(bad, pending);
        }
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            choose(branch, CreatureTypeV60::Elf);
            settle(branch);
        }
        assert_eq!(state, replay);
        let facts = capture(&state, actor).unwrap();
        let entry = facts.completed_entries[0].fellowship.as_ref().unwrap();
        assert_eq!(entry.preentry_chosen_creature_count, 1);
        assert_eq!(entry.entry_fellowship_after_replacements, 2);
        assert_eq!(entry.captured_seasons.len(), 1);
        let permanent = &facts.permanents[0];
        assert_eq!(permanent.current_fellowship, 2);
        assert!(permanent
            .current_controlled_creature_bonuses
            .iter()
            .any(|row| row.recipient.object == elf.0 && row.power == 2 && row.toughness == 2));
        assert_eq!(engine::checked_effective_pt(&state, elf).unwrap(), (3, 3));
        let mut forged = facts.clone();
        forged.completed_entries[0].chosen = CreatureTypeV60::Bear;
        assert!(validate(&state, actor, &forged).is_err());
        let mut forged = facts.clone();
        forged.permanents[0].entry_occurrence = u32::MAX;
        assert!(validate(&state, actor, &forged).is_err());
        let marker = state
            .engine
            .event_history
            .iter()
            .find(|e| {
                matches!(e,
            CommittedEvent::KinshipEntryV1 { journal }
                if matches!(journal.as_ref(), JournalV1::Committed { .. }))
            })
            .unwrap()
            .clone();
        let mut forged = state.clone();
        forged.engine.event_log.push(marker.clone());
        refused(&forged);
        let mut forged = state.clone();
        forged.engine.event_history.push(marker);
        refused(&forged);
        let mut raw = serde_json::to_value(&facts).unwrap();
        raw["active_entry_choice"]["private_answers"] = serde_json::json!([]);
        assert!(serde_json::from_value::<FactsV66>(raw).is_err());
        assert!(crate::drake_public_v53::capture(&state).is_err());
        restore(&state);
    }
}
#[test]
fn paid_banner_current_control_type_suppression_full_counter_width_and_departure_primitives_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        paid(&mut state, actor, "Llanowar Elves", None);
        let source = banner(&mut state, actor, CreatureTypeV60::Elf);
        // Native control, counter-storage, type-setting and suppression scenes.
        // These are not claims of paid artifact-control or suppression printings.
        let other = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        crate::standard_cards_v1::gain_control(&mut state, source, actor.opponent());
        state.objects.get_mut(source).counters.fellowship = 40_000;
        let view = capture(&state, actor).unwrap();
        let row = &view.permanents[0];
        assert_eq!(row.card.controller, actor.opponent().into());
        assert_eq!(
            view.completed_entries[0].source.captured_controller,
            actor.into()
        );
        assert_eq!(row.current_fellowship, 40_000);
        assert!(row
            .current_controlled_creature_bonuses
            .iter()
            .any(|b| b.recipient.object == other.0 && b.power == 40_000));
        let suppressor = ObjectLinkV4 {
            object: other,
            zone_change_count: state.objects.get(other).zone_change_count,
        };
        state.objects.get_mut(source).v4.creature_upgrade =
            Some(crate::standard_creatures_v1::CreatureUpgradeV1 {
                suppressed_by: vec![crate::standard_creatures_v1::TidebinderSuppressionV1 {
                    source: suppressor,
                    timestamp: 8,
                }],
                ..Default::default()
            });
        let view = capture(&state, actor).unwrap();
        assert!(!view.permanents[0].printed_abilities_active);
        assert!(view.permanents[0]
            .current_controlled_creature_bonuses
            .iter()
            .all(|b| b.power == 0 && b.toughness == 0));
        state.objects.get_mut(source).v4.creature_upgrade = None;
        state.objects.get_mut(other).v4.creature_upgrade =
            Some(crate::standard_creatures_v1::CreatureUpgradeV1 {
                creature_types: Some((vec![crate::card_def::Subtype::Human.stable_id()], 9)),
                ..Default::default()
            });
        assert!(capture(&state, actor).unwrap().permanents[0]
            .current_controlled_creature_bonuses
            .iter()
            .all(|b| b.power == 0));
        // Every native counter width is represented on a noncreature storage
        // primitive, avoiding an unrelated checked PT overflow.
        let store = put(&mut state, actor, "Viridian Longbow", Zone::Battlefield);
        {
            let live = state.objects.get_mut(store);
            live.counters = Counters {
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
            };
            live.v4.lifelink_keyword_counters = i16::MAX;
            live.v4.time_counters_v1 = u8::MAX;
            live.v4.creature_upgrade = Some(crate::standard_creatures_v1::CreatureUpgradeV1 {
                finality: u16::MAX,
                ..Default::default()
            });
        }
        let planeswalker = put(
            &mut state,
            actor,
            "Ajani, Caller of the Pride",
            Zone::Battlefield,
        );
        crate::planeswalker_v1::add_loyalty_counters_u32_v48(&mut state, planeswalker, u32::MAX)
            .unwrap();
        let view = capture(&state, actor).unwrap();
        let c = &view.battlefield[actor.index()]
            .iter()
            .find(|r| r.card.object == store.0)
            .unwrap()
            .counters;
        assert_eq!(
            (c.plus_one, c.fellowship, c.time, c.finality),
            (i32::MAX, i32::MAX, u8::MAX, u16::MAX)
        );
        assert_eq!(
            view.battlefield[actor.index()]
                .iter()
                .find(|r| r.card.object == planeswalker.0)
                .unwrap()
                .counters
                .loyalty,
            u32::MAX
        );
        restore(&state);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
        let view = capture(&state, actor).unwrap();
        assert!(view.permanents.is_empty());
        assert!(view.completed_entries[0]
            .source
            .current_original_source
            .is_none());
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        reach_type(&mut state, actor);
        choose(&mut state, CreatureTypeV60::TimeLord);
        let view = capture(&state, actor).unwrap();
        assert_eq!(view.completed_entries.len(), 2);
        assert_eq!(view.permanents[0].entry_occurrence, 1);
        assert_eq!(view.permanents[0].current_fellowship, 0);
        restore(&state);
    }
}
#[test]
fn genuine_courtyard_play_color_alternatives_original_float_reentry_activation_and_tamper_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        // Initial native mana scene intentionally has no spare B source.
        let deck = [crate::card_def::card_id_by_name("Swamp").unwrap(); 40];
        let mut cast_state = GameState::new_from_libraries_with_starting_player_v1(
            &deck,
            &deck,
            crate::rl::card_name,
            110_665,
            actor,
        );
        cast_state.step = Step::Main1;
        let spell_land = courtyard(&mut cast_state, actor, CreatureTypeV60::Elf);
        let elf = put(&mut cast_state, actor, "Llanowar Elves", Zone::Hand);
        let bolt = put(&mut cast_state, actor, "Lightning Bolt", Zone::Hand);
        assert!(
            matches!(engine::advance_until_decision(&mut cast_state), Decision::CastSpellOrPass { castable_spells, .. }
            if castable_spells.contains(&elf) && !castable_spells.contains(&bolt))
        );
        engine::step(&mut cast_state, Action::CastSpell(elf)).unwrap();
        priority(&mut cast_state, actor);
        assert!(cast_state.objects.get(spell_land).tapped);
        assert_eq!(cast_state.players[actor.index()].mana_pool, [0; 6]);
        assert!(cast_state.players[actor.index()]
            .restricted_mana_pool
            .is_empty());
        assert!(cast_state
            .stack
            .iter()
            .any(|row| row.source == elf && row.kind == StackItemKind::Spell));
        let mut cast_replay = restore(&cast_state);
        settle(&mut cast_state);
        settle(&mut cast_replay);
        assert_eq!(cast_state, cast_replay);
        assert_eq!(cast_state.objects.get(elf).zone, Zone::Battlefield);
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &deck,
            &deck,
            crate::rl::card_name,
            110_666,
            actor,
        );
        state.step = Step::Main1;
        let source = courtyard(&mut state, actor, CreatureTypeV60::Skeleton);
        let view = capture(&state, actor).unwrap();
        assert_eq!(view.permanents[0].currently_payable_mana_colors.len(), 6);
        assert_eq!(view.permanents[0].current_fellowship, 0);
        engine::step(
            &mut state,
            Action::ActivateManaAbilityChoice(source, ManaColor::B),
        )
        .unwrap();
        let view = capture(&state, actor).unwrap();
        assert_eq!(view.floating_chosen_type_mana.len(), 1);
        assert_eq!(
            view.floating_chosen_type_mana[0].chosen,
            CreatureTypeV60::Skeleton
        );
        assert_eq!(view.floating_chosen_type_mana[0].holder, actor.into());
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        // Genuine native transitions, not an invented paid blink spell.
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        reach_type(&mut state, actor);
        choose(&mut state, CreatureTypeV60::Elf);
        priority(&mut state, actor);
        let view = capture(&state, actor).unwrap();
        assert_eq!(view.permanents[0].chosen, CreatureTypeV60::Elf);
        assert_eq!(
            view.floating_chosen_type_mana[0].chosen,
            CreatureTypeV60::Skeleton
        );
        assert_eq!(
            view.floating_chosen_type_mana[0].original_entry_occurrence,
            0
        );
        assert!(view.floating_chosen_type_mana[0]
            .current_original_source
            .is_none());
        let mut forged = view.clone();
        forged.floating_chosen_type_mana[0].original_entry_occurrence = 1;
        assert!(validate(&state, actor, &forged).is_err());
        let mut forged = view.clone();
        forged.floating_chosen_type_mana[0].holder = actor.opponent().into();
        assert!(validate(&state, actor, &forged).is_err());
        for edit in 0..4 {
            let mut bad = restore(&state);
            let unit = &mut bad.players[actor.index()].restricted_mana_pool.0[0];
            match edit {
                0 => unit.source.zone_change_count += 1,
                1 => unit.source_card_def = crate::card_def::card_id_by_name("Forest").unwrap(),
                2 => {
                    unit.restriction =
                        ManaSpendRestrictionDef::ChosenCreatureTypeV60(Some(CreatureTypeV60::Elf))
                }
                _ => unit.restriction = ManaSpendRestrictionDef::CreatureSpellOrCreatureAbility,
            }
            refused(&bad);
        }
        let skeleton = put(&mut state, actor, "Reassembling Skeleton", Zone::Graveyard);
        let generic = put(&mut state, actor, "Soulstone Sanctuary", Zone::Battlefield);
        // Tap the new Elf source for C and pay a real Longbow cast so the
        // original Skeleton B is the only legal colored activation payment.
        engine::step(
            &mut state,
            Action::ActivateManaAbilityChoice(source, ManaColor::C),
        )
        .unwrap();
        let bow = put(&mut state, actor, "Viridian Longbow", Zone::Hand);
        engine::step(&mut state, Action::CastSpell(bow)).unwrap();
        priority(&mut state, actor);
        settle(&mut state);
        assert!(!state.objects.get(generic).tapped);
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            engine::step(branch, Action::ActivateAbility(skeleton, 0)).unwrap();
            priority(branch, actor);
            assert!(branch.objects.get(generic).tapped);
            assert!(capture(branch, actor)
                .unwrap()
                .floating_chosen_type_mana
                .is_empty());
            assert!(branch.stack.iter().any(|r| r.source == skeleton
                && r.controller == actor
                && r.kind == StackItemKind::ActivatedAbility));
            settle(branch);
            assert_eq!(branch.objects.get(skeleton).zone, Zone::Battlefield);
        }
        assert_eq!(state, replay);
    }
}
#[test]
fn genuine_float_hidden_departure_and_control_change_hand_activation_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = courtyard(&mut state, actor, CreatureTypeV60::Troll);
        engine::step(
            &mut state,
            Action::ActivateManaAbilityChoice(source, ManaColor::U),
        )
        .unwrap();
        let mut hidden = restore(&state);
        event::propose_and_commit(
            &mut hidden,
            ProposedEvent::zone_change(source, Zone::Library),
        );
        for viewer in [actor, actor.opponent()] {
            let view = capture(&hidden, viewer).unwrap();
            assert!(!view.visible_printings.iter().any(|r| r.object == source.0));
            assert!(view.completed_entries[0]
                .source
                .current_original_source
                .is_none());
            assert!(view.floating_chosen_type_mana[0]
                .current_original_source
                .is_none());
            assert_eq!(
                view.floating_chosen_type_mana[0].chosen,
                CreatureTypeV60::Troll
            );
        }
        crate::standard_cards_v1::gain_control(&mut state, source, actor.opponent());
        let view = capture(&state, actor).unwrap();
        let unit = &view.floating_chosen_type_mana[0];
        assert_eq!(unit.original_entry_controller, actor.into());
        assert_eq!(unit.holder, actor.into());
        assert_eq!(
            unit.current_original_source.as_ref().unwrap().controller,
            actor.opponent().into()
        );
        let troll = put(&mut state, actor, "Troll of Khazad-dum", Zone::Hand);
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            // Genuine Swampcycling{1}, from hand, paid from original U.
            engine::step(branch, Action::ActivateAbility(troll, 0)).unwrap();
            priority(branch, actor);
            assert_eq!(branch.objects.get(troll).zone, Zone::Graveyard);
            assert!(capture(branch, actor)
                .unwrap()
                .floating_chosen_type_mana
                .is_empty());
            assert!(branch
                .stack
                .iter()
                .any(|row| row.source == troll && row.kind == StackItemKind::ActivatedAbility));
        }
        assert_eq!(state, replay);
        settle(&mut state);
        settle(&mut replay);
        assert_eq!(state, replay);
    }
}
#[test]
fn genuine_paid_replication_banner_copy_and_unrevealed_apnap_answers_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        paid(&mut state, actor, "Llanowar Elves", None);
        let source = banner(&mut state, actor, CreatureTypeV60::Elf);
        let replication = paid(&mut state, actor, "Extravagant Replication", None);
        let mut found = false;
        for _ in 0..256 {
            let d = engine::advance_until_decision(&mut state);
            if let Decision::ChooseTargets {
                spell,
                legal_targets,
                ..
            } = &d
            {
                assert_eq!(*spell, replication);
                assert!(legal_targets.contains(&Target::Object(source)));
                found = true;
                break;
            }
            quiet(&mut state, d);
        }
        assert!(found);
        engine::step(&mut state, Action::ChooseTarget(Target::Object(source))).unwrap();
        reach_type(&mut state, actor);
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            choose(branch, CreatureTypeV60::Bear);
            settle(branch);
        }
        assert_eq!(state, replay);
        let view = capture(&state, actor).unwrap();
        assert_eq!(view.permanents.len(), 2);
        assert_eq!(view.completed_entries[1].chosen, CreatureTypeV60::Bear);
        assert!(view.completed_entries[1].created_by_token_event);
        assert_eq!(
            view.completed_entries[1]
                .fellowship
                .as_ref()
                .unwrap()
                .entry_fellowship_after_replacements,
            0
        );
        // Explicit native simultaneous-entry primitive. No paid mass-return claim.
        let first = put(&mut state, actor, "Banner of Kinship", Zone::Hand);
        let second = put(
            &mut state,
            actor.opponent(),
            "Banner of Kinship",
            Zone::Hand,
        );
        event::propose_and_commit_batch(
            &mut state,
            vec![
                ProposedEvent::zone_change(first, Zone::Battlefield),
                ProposedEvent::zone_change(second, Zone::Battlefield),
            ],
        );
        reach_type(&mut state, actor);
        let before = capture(&state, actor.opponent()).unwrap().completed_entries;
        choose(&mut state, CreatureTypeV60::TimeLord);
        reach_type(&mut state, actor.opponent());
        for viewer in [actor, actor.opponent()] {
            let view = capture(&state, viewer).unwrap();
            assert_eq!(view.completed_entries, before);
            let raw = serde_json::to_string(&view.active_entry_choice).unwrap();
            assert!(
                !raw.contains("answers") && !raw.contains("counts") && !raw.contains("original")
            );
        }
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            choose(branch, CreatureTypeV60::Elf);
        }
        assert_eq!(state, replay);
        assert_eq!(capture(&state, actor).unwrap().completed_entries.len(), 4);
    }
}
#[test]
fn independent_hidden_worlds_neutral_face_down_and_readonly_public_forgery_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        fn scene(actor: PlayerId, hidden_name: &str) -> GameState {
            let mut state = ready(actor);
            put(&mut state, actor.opponent(), hidden_name, Zone::Hand);
            let hidden = put(&mut state, actor.opponent(), hidden_name, Zone::Battlefield);
            // Native neutral face-down setup, not a claim of a paid manifest route.
            state.objects.get_mut(hidden).v4.face_down_v1 = Some(FaceDownV1 {
                disguised: false,
                lookers: 0,
                hidden_by: None,
                hidden_by_source: None,
            });
            paid(&mut state, actor, "Llanowar Elves", None);
            banner(&mut state, actor, CreatureTypeV60::Elf);
            state
        }
        let a = scene(actor, "Swamp");
        let b = scene(actor, "Forest");
        assert_ne!(a.state_hash(), b.state_hash());
        assert_eq!(capture(&a, actor).unwrap(), capture(&b, actor).unwrap());
        let facts = capture(&a, actor).unwrap();
        let mut changed = facts.clone();
        changed.battlefield[actor.index()][0].counters.fellowship = i32::MAX;
        assert!(validate(&a, actor, &changed).is_err());
        let mut raw = serde_json::to_value(&facts).unwrap();
        raw["world"] = serde_json::json!({});
        assert!(serde_json::from_value::<FactsV66>(raw).is_err());
        let mut raw = serde_json::to_value(&facts).unwrap();
        raw["battlefield"][actor.index()][0]["counters"]["fellowship"] =
            serde_json::json!(u64::MAX);
        assert!(serde_json::from_value::<FactsV66>(raw).is_err());
        restore(&a);
        restore(&b);
    }
}
