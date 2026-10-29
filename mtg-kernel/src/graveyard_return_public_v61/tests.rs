use super::*;
use crate::{
    card_def::CARD_DEFS,
    engine::{self, Action, Decision},
    event::{self, ProposedEvent},
    policy_observation_v6::tests::put,
};

pub(crate) fn ready(actor: PlayerId) -> GameState {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
    for seat in [PlayerId::P0, PlayerId::P1] {
        for _ in 0..8 {
            for name in ["Swamp", "Mountain", "Island", "Forest"] {
                put(&mut state, seat, name, Zone::Battlefield);
            }
        }
    }
    state
}
pub(crate) fn quiet(state: &mut GameState, decision: Decision) {
    let action = match decision {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::ChooseEffectBoolean {
            purpose: crate::effect::EffectBooleanChoicePurpose::AleshaRaidReturnV1,
            ..
        } => Action::ChooseEffectBoolean(true),
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
        Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        other => panic!("unexpected V61 fixture decision {other:?}"),
    };
    engine::step(state, action).unwrap();
    assert!(state.engine.halted.is_none());
}
pub(crate) fn settle(state: &mut GameState) {
    for _ in 0..160 {
        let d = engine::advance_until_decision(state);
        if state.stack.is_empty()
            && state.engine.pending_cast.is_none()
            && state.engine.pending_effect.is_none()
            && state.engine.pending_triggers.is_empty()
        {
            assert!(matches!(d, Decision::CastSpellOrPass { .. }));
            return;
        }
        quiet(state, d);
    }
    panic!("V61 native fixture did not settle");
}
pub(crate) fn announce(
    state: &mut GameState,
    actor: PlayerId,
    spell: ObjectId,
    target: Option<ObjectId>,
) {
    let decision = (0..12)
        .find_map(|_| {
            let d = engine::advance_until_decision(state);
            if matches!(d,Decision::CastSpellOrPass{player,..}if player==actor) {
                Some(d)
            } else {
                quiet(state, d);
                None
            }
        })
        .unwrap();
    assert!(
        matches!(decision,Decision::CastSpellOrPass{castable_spells,..}if castable_spells.contains(&spell))
    );
    let before = state.players[actor.index()]
        .battlefield
        .iter()
        .filter(|&&id| state.objects.get(id).tapped)
        .count();
    engine::step(state, Action::CastSpell(spell)).unwrap();
    if let Some(target) = target {
        let mut choice = engine::advance_until_decision(state);
        if let Decision::ChooseSpellMode {
            player,
            spell: pending,
            mode_count,
            legal_modes,
        } = &choice
        {
            // The paid Employment scene includes its real Treasure artifact,
            // so Abrade must choose creature damage before choosing a target.
            assert_eq!(
                CARD_DEFS[state.objects.get(spell).card_def as usize].name,
                "Abrade"
            );
            assert_eq!((*player, *pending, *mode_count), (actor, spell, 2));
            assert_eq!(legal_modes.as_slice(), &[0, 1]);
            engine::step(state, Action::ChooseSpellMode(0)).unwrap();
            choice = engine::advance_until_decision(state);
        }
        assert!(
            matches!(&choice,Decision::ChooseTargets{player,spell:pending,legal_targets,..}if *player==actor && *pending==spell && legal_targets.contains(&Target::Object(target))),
            "expected the paid spell's original target choice: {choice:?}"
        );
        engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    }
    assert!(
        matches!(engine::advance_until_decision(state),Decision::CastSpellOrPass{player,..}if player==actor)
    );
    let after = state.players[actor.index()]
        .battlefield
        .iter()
        .filter(|&&id| state.objects.get(id).tapped)
        .count();
    assert_eq!(
        after - before,
        usize::from(CARD_DEFS[usize::from(state.objects.get(spell).card_def)].mana_value)
    );
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
}
pub(crate) fn paid(
    state: &mut GameState,
    actor: PlayerId,
    name: &str,
    target: Option<ObjectId>,
) -> ObjectId {
    let spell = put(state, actor, name, Zone::Hand);
    announce(state, actor, spell, target);
    settle(state);
    spell
}
fn assert_restore(state: &GameState) -> GameState {
    let before = serde_json::to_vec(state).unwrap();
    let restored: GameState = serde_json::from_slice(&before).unwrap();
    for viewer in [PlayerId::P0, PlayerId::P1] {
        let facts = capture(state, viewer).unwrap();
        assert_eq!(facts, capture(&restored, viewer).unwrap());
        validate(state, viewer, &facts).unwrap();
        let roundtrip: FactsV61 =
            serde_json::from_slice(&serde_json::to_vec(&facts).unwrap()).unwrap();
        assert_eq!(roundtrip, facts);
        assert!(!facts.complete_ordinary_observation && !facts.owning_consumer_admitted);
    }
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
    restored
}

fn conceal_returned_card_native(state: &mut GameState, card: ObjectId) {
    let owner = state.objects.get(card).owner;
    let generation = state.objects.get(card).zone_change_count;
    // Snap's real public return remains known at its new hand incarnation.
    let remembered = public::reference(state, card, Some(owner.opponent())).unwrap();
    assert_eq!(remembered.zone, Zone::Hand);
    assert_eq!(remembered.generation, generation);
    assert_eq!(
        remembered.identity,
        public::IdentityV1::Known {
            card_id: state.objects.get(card).card_def,
        }
    );
    // Explicit native concealment setup, not a claimed printed shuffle ability:
    // move to library, shuffle away exact knowledge, then privately move to hand.
    // The independently paid original death/departure history stays intact.
    event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Library));
    state.shuffle_library(owner).unwrap();
    event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Hand));
    assert_eq!(state.objects.get(card).zone, Zone::Hand);
    assert_eq!(state.objects.get(card).zone_change_count, generation + 2);
    assert!(public::reference(state, card, Some(owner.opponent())).is_none());
    assert!(public::reference(state, card, Some(owner)).is_some());
}
#[test]
fn closed_two_printing_programs_preserve_order_and_refuse_forged_meanings() {
    for (family, id) in [(FamilyV61::Alesha, 457), (FamilyV61::NineLives, 458)] {
        let meaning = meaning::printed_meaning(family).unwrap();
        assert_eq!(meaning.card_id, id);
        meaning::validate(&meaning).unwrap();
        assert!(
            !meaning.full_engine_admitted
                && !meaning.owning_consumer_admitted
                && !meaning.model_admitted
                && !meaning.sampler_admitted
        );
        let mut changed = meaning.clone();
        changed.ordered_rules.reverse();
        assert!(meaning::validate(&changed).is_err());
        let mut changed = meaning.clone();
        changed.printed.generic_mana += 1;
        assert!(meaning::validate(&changed).is_err());
        let mut raw = serde_json::to_value(&meaning).unwrap();
        raw["foreign_permission"] = true.into();
        assert!(serde_json::from_value::<meaning::MeaningV61>(raw).is_err());
    }
    assert_eq!(FamilyV61::ALL.len(), 2);
}
#[test]
fn paid_two_familiars_expose_exact_death_schedule_and_delayed_native_stages_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let one = paid(&mut state, actor, "Nine-Lives Familiar", None);
        let two = paid(&mut state, actor, "Nine-Lives Familiar", None);
        let spell = put(&mut state, actor, "Breath Weapon", Zone::Hand);
        announce(&mut state, actor, spell, None);
        for _ in 0..40 {
            let d = engine::advance_until_decision(&mut state);
            if matches!(&d,Decision::OrderTriggers{pending,..}if pending.len()==2) {
                break;
            }
            quiet(&mut state, d);
        }
        let facts = capture(&state, actor).unwrap();
        assert_eq!(facts.nine_lives_deaths.len(), 2);
        assert_eq!(
            facts
                .triggers
                .iter()
                .filter(|r| r.stage == StageV61::Held)
                .count(),
            2
        );
        assert!(facts.nine_lives_schedules.is_empty());
        let mut restored = assert_restore(&state);
        for branch in [&mut state, &mut restored] {
            engine::step(branch, Action::OrderTriggers(vec![1, 0])).unwrap();
            let facts = capture(branch, actor).unwrap();
            assert_eq!(
                facts
                    .triggers
                    .iter()
                    .filter(|r| r.stage == StageV61::Published)
                    .count(),
                2
            );
            settle(branch);
            let facts = capture(branch, actor).unwrap();
            assert_eq!(facts.nine_lives_schedules.len(), 2);
            assert!(facts
                .nine_lives_schedules
                .iter()
                .all(|r| r.departure_revival == 8 && r.return_base_before_replacements == 7));
        }
        assert_eq!(state, restored);
        assert_restore(&state);
        let mut changed = state.clone();
        changed.nine_lives_delayed_v1.as_mut().unwrap()[0].revival_counters += 1;
        let unchanged = serde_json::to_vec(&changed).unwrap();
        assert!(capture(&changed, actor).is_err());
        assert_eq!(serde_json::to_vec(&changed).unwrap(), unchanged);
        for _ in 0..300 {
            let d = engine::advance_until_decision(&mut state);
            if matches!(&d,Decision::OrderTriggers{pending,..}if pending.len()==2) {
                break;
            }
            quiet(&mut state, d);
        }
        let facts = capture(&state, actor).unwrap();
        assert!(facts.nine_lives_schedules.is_empty());
        assert_eq!(
            facts
                .triggers
                .iter()
                .filter(|r| r.stage == StageV61::Held
                    && matches!(r.program, ProgramV61::NineLivesDelayedReturn { .. }))
                .count(),
            2
        );
        let mut bad = facts.clone();
        bad.triggers[0].position += 1;
        assert!(validate(&state, actor, &bad).is_err());
        let mut replay = assert_restore(&state);
        for branch in [&mut state, &mut replay] {
            engine::step(branch, Action::OrderTriggers(vec![0, 1])).unwrap();
            settle(branch);
            assert_eq!(branch.objects.get(one).counters.revival, 7);
            assert_eq!(branch.objects.get(two).counters.revival, 7);
        }
        assert_eq!(state, replay);
        assert_restore(&state);
    }
}
pub(crate) fn raid_ready(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
    raid_ready_with_kill(actor, "Abrade")
}
pub(crate) fn raid_ready_with_kill(
    actor: PlayerId,
    kill_spell: &str,
) -> (GameState, ObjectId, ObjectId) {
    let mut state = ready(actor);
    let alesha = paid(&mut state, actor, "Alesha, Who Laughs at Fate", None);
    let target = paid(&mut state, actor, "Vampire Nighthawk", None);
    paid(&mut state, actor, kill_spell, Some(target));
    let turn = state.turn;
    for _ in 0..300 {
        let d = engine::advance_until_decision(&mut state);
        if matches!(d,Decision::DeclareAttackers{player,..}if player==actor && state.turn>turn) {
            break;
        }
        quiet(&mut state, d);
    }
    engine::step(&mut state, Action::DeclareAttackers(vec![alesha])).unwrap();
    settle(&mut state);
    assert_eq!(engine::effective_power(&state, alesha), 3);
    for _ in 0..240 {
        let d = engine::advance_until_decision(&mut state);
        if matches!(&d,Decision::ChooseTargets{player,legal_targets,..}if *player==actor && legal_targets.contains(&Target::Object(target)))
        {
            return (state, alesha, target);
        }
        quiet(&mut state, d);
    }
    panic!("V61 genuine Raid target prompt absent");
}
#[test]
fn paid_alesha_raid_binds_actual_current_then_departure_power_and_public_target_restore_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, alesha, target) = raid_ready(actor);
        let facts = capture(&state, actor).unwrap();
        assert!(facts.triggers.iter().any(|r|matches!(&r.program,ProgramV61::AleshaRaid{power,..}if power.amount==3 && power.source==PowerSourceV61::CurrentOriginalIncarnation)));
        assert_restore(&state);
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        let bolt = put(&mut state, actor.opponent(), "Abrade", Zone::Hand);
        announce(&mut state, actor.opponent(), bolt, Some(alesha));
        let first = engine::advance_until_decision(&mut state);
        assert!(matches!(first,Decision::CastSpellOrPass{player,..}if player==actor.opponent()));
        engine::step(&mut state, Action::Pass).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state),Decision::CastSpellOrPass{player,..}if player==actor)
        );
        engine::step(&mut state, Action::Pass).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        let facts = capture(&state, actor).unwrap();
        assert_eq!(facts.alesha_departures.len(), 1);
        assert_eq!(facts.alesha_departures[0].power, 3);
        let contract = state
            .stack
            .iter()
            .find(|row| row.source == alesha)
            .unwrap()
            .v4
            .ability_source_contract
            .unwrap();
        assert_eq!(engine::alesha_raid_source_power(&state, contract), Some(3));
        let receipt = state
            .engine
            .event_history
            .iter()
            .position(|event| matches!(event,E::AleshaPowerLkiV1{source,..}if *source==alesha))
            .unwrap();
        let E::AleshaPowerLkiV1 {
            departure_history_index,
            ..
        } = &state.engine.event_history[receipt]
        else {
            unreachable!()
        };
        let departure = usize::try_from(*departure_history_index).unwrap();
        assert_eq!(
            engine::alesha_power_receipt_index_v1(&state, departure),
            Some(receipt)
        );
        assert_eq!(receipt, departure + 2);
        assert!(
            matches!(&state.engine.event_history[departure+1],E::NivMizzetVisionaryDamageSourceDepartureV1{source,..}if source.source==alesha)
        );
        // Actual native receipt composition is accepted; edits below are only
        // negative fixtures and never replace an engine-produced departure.
        for mode in 0..9 {
            let mut changed = state.clone();
            match mode {
                3 => {
                    changed.engine.event_history.remove(departure + 1);
                }
                4 => {
                    let unrelated = changed.engine.event_history[departure].clone();
                    changed.engine.event_history.insert(receipt, unrelated);
                }
                5 => {
                    let duplicate = changed.engine.event_history[receipt].clone();
                    changed.engine.event_history.push(duplicate);
                }
                6 => changed.engine.event_history.swap(departure + 1, receipt),
                _ => {
                    let E::NivMizzetVisionaryDamageSourceDepartureV1 {
                        source,
                        departure_history_index,
                    } = &mut changed.engine.event_history[departure + 1]
                    else {
                        unreachable!()
                    };
                    match mode {
                        0 => source.zone_change_count += 1,
                        1 => source.controller = actor.opponent(),
                        2 => *departure_history_index += 1,
                        7 => source.card_def = crate::card_def::card_id_by_name("Forest").unwrap(),
                        _ => source.owner = actor.opponent(),
                    }
                }
            }
            let before = serde_json::to_vec(&changed).unwrap();
            assert_eq!(
                engine::alesha_raid_source_power(&changed, contract),
                None,
                "field mode {mode}"
            );
            assert!(capture(&changed, actor).is_err(), "field mode {mode}");
            assert_eq!(serde_json::to_vec(&changed).unwrap(), before);
        }
        assert!(facts.triggers.iter().any(|r|matches!(&r.program,ProgramV61::AleshaRaid{power,..}if power.amount==3 && power.source==PowerSourceV61::ExactDepartureLki)
            && r.targets.len()==1 && r.targets[0].legal_now));
        let mut bad = facts.clone();
        bad.alesha_departures[0].power += 1;
        assert!(validate(&state, actor, &bad).is_err());
        let mut replay = assert_restore(&state);
        settle(&mut state);
        settle(&mut replay);
        assert_eq!(state, replay);
        assert_eq!(state.objects.get(target).zone, Zone::Battlefield);
    }
}
#[test]
fn paid_nine_lives_old_schedule_does_not_expose_new_hidden_hand_incarnation_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let familiar = paid(&mut state, actor, "Nine-Lives Familiar", None);
        paid(&mut state, actor, "Abrade", Some(familiar));
        paid(&mut state, actor, "Zombify", Some(familiar));
        paid(&mut state, actor, "Snap", Some(familiar));
        assert_eq!(state.objects.get(familiar).zone, Zone::Hand);
        let remembered = capture(&state, actor.opponent()).unwrap();
        assert!(remembered
            .authorized
            .iter()
            .any(|r| r.card.object == familiar.0
                && r.card.zone == Zone::Hand
                && r.card.generation == state.objects.get(familiar).zone_change_count));
        conceal_returned_card_native(&mut state, familiar);
        let facts = capture(&state, actor.opponent()).unwrap();
        assert_eq!(facts.nine_lives_schedules, remembered.nine_lives_schedules);
        assert_eq!(facts.nine_lives_schedules.len(), 1);
        assert!(facts.nine_lives_schedules[0]
            .source
            .current_original_source
            .is_none());
        assert!(facts.nine_lives_schedules[0]
            .exact_graveyard_card_now
            .is_none());
        assert!(!facts.authorized.iter().any(|r| r.card.object == familiar.0));
        assert_restore(&state);
        let secret = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        let before = capture(&state, actor.opponent()).unwrap();
        let mut alternate = state.clone();
        let id = card_def::card_id_by_name("Counterspell").unwrap();
        alternate.objects.get_mut(secret).card_def = id;
        alternate.objects.get_mut(secret).name = "Counterspell".into();
        alternate.objects.get_mut(secret).v4 = crate::state::ObjectStateV4::from_card_def(id);
        alternate.players[actor.index()].library.reverse();
        assert_eq!(before, capture(&alternate, actor.opponent()).unwrap());
    }
}
#[test]
fn paid_roots_have_complete_all_permanent_counter_roster_and_explicit_wide_negative_primitive_checks(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let familiar = paid(&mut state, actor, "Nine-Lives Familiar", None);
        let baseline = capture(&state, actor).unwrap();
        assert_eq!(
            baseline
                .battlefield_counters
                .iter()
                .map(Vec::len)
                .sum::<usize>(),
            state
                .players
                .iter()
                .map(|p| p.battlefield.len())
                .sum::<usize>()
        );
        let land = state.players[actor.opponent().index()].battlefield[0];
        // Explicit native counter-storage primitives, not invented printed land abilities.
        state.objects.get_mut(land).counters.revival = i32::MAX;
        state.objects.get_mut(land).counters.plus1_plus1 = i32::MAX;
        let facts = capture(&state, actor).unwrap();
        let row = facts.battlefield_counters[actor.opponent().index()]
            .iter()
            .find(|r| r.card.object == land.0)
            .unwrap();
        assert_eq!(row.counters.revival, i32::MAX);
        assert_eq!(row.counters.plus_one, i32::MAX);
        assert_restore(&state);
        assert!(crate::rl::observe_v1(&state, actor, 0).is_err());
        state.objects.get_mut(familiar).counters.revival = -1;
        let before = serde_json::to_vec(&state).unwrap();
        assert!(capture(&state, actor).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
}

#[test]
fn paid_stolen_familiar_keeps_physical_owner_and_captured_controller_season_entry_world_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        paid(&mut state, actor, "Doubling Season", None);
        let familiar = paid(&mut state, actor, "Nine-Lives Familiar", None);
        assert_eq!(state.objects.get(familiar).counters.revival, 16);
        let opponent = actor.opponent();
        crate::policy_observation_v6::tests::advance_to_next_main_for(&mut state, opponent);
        paid(
            &mut state,
            opponent,
            "Involuntary Employment",
            Some(familiar),
        );
        paid(&mut state, actor, "Abrade", Some(familiar));
        let facts = capture(&state, actor).unwrap();
        let row = &facts.nine_lives_schedules[0];
        assert_eq!(row.source.captured_controller, opponent.into());
        assert_eq!(row.source.original_source.owner, actor.into());
        assert_eq!(row.departure_revival, 16);
        assert_eq!(row.return_base_before_replacements, 15);
        let mut restored = assert_restore(&state);
        for branch in [&mut state, &mut restored] {
            for _ in 0..300 {
                if branch.objects.get(familiar).zone == Zone::Battlefield {
                    break;
                }
                let d = engine::advance_until_decision(branch);
                quiet(branch, d);
            }
            assert_eq!(branch.objects.get(familiar).zone, Zone::Battlefield);
            assert_eq!(branch.objects.get(familiar).owner, actor);
            assert_eq!(branch.objects.get(familiar).controller, opponent);
            // Actor's Season doubled the actual cast entry, but does not apply
            // to a later entry under the captured opposing controller.
            assert_eq!(branch.objects.get(familiar).counters.revival, 15);
            let facts = capture(branch, actor).unwrap();
            assert!(facts.battlefield_counters[opponent.index()]
                .iter()
                .any(|r| r.card.object == familiar.0 && r.counters.revival == 15));
        }
        assert_eq!(state, restored);
    }
}

// These boundaries do not use CardCapability as a proxy for representation.
fn legacy_refuses_readonly(state: &GameState) {
    let before = serde_json::to_vec(state).unwrap();
    let restored: GameState = serde_json::from_slice(&before).unwrap();
    for branch in [state, &restored] {
        for viewer in [PlayerId::P0, PlayerId::P1] {
            assert!(require_legacy_visible(branch, Some(viewer)).is_err());
            assert!(crate::rl::observe_v1(branch, viewer, 0).is_err());
        }
        assert!(require_legacy_visible(branch, None).is_err());
        assert!(crate::prepared_observation_v57::validate_state(branch).is_err());
        assert!(crate::next_five_observation_v59::validate_state(branch).is_err());
    }
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
}

#[test]
fn paid_alesha_and_zero_uncast_familiar_legacy_boundaries_preserve_complete_v61_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        paid(&mut state, actor, "Alesha, Who Laughs at Fate", None);
        legacy_refuses_readonly(&state);
        // The supported complete consumer still uses its inherited builders.
        let mut surface = crate::policy_surface_v5::PolicySurfaceV5::new_with_engine_priority_v1();
        let origin = surface.next_decision(&mut state).unwrap();
        let viewer = origin.actor(&state).unwrap();
        let before = state.clone();
        crate::graveyard_return_candidates_v61::project_v61(
            &state,
            &surface,
            (viewer, &origin),
            0,
            0,
            origin.substep(),
        )
        .unwrap();
        assert!(crate::rl::core_policy_action_candidates_next_five_v59(&origin, &state).is_err());
        assert!(crate::rl::legal_action_candidates_v5(&origin, &state).is_err());
        assert_eq!(state, before);

        let mut state = ready(actor);
        // Native object setup only: no cast event or entry replacement is claimed.
        let familiar = put(&mut state, actor, "Nine-Lives Familiar", Zone::Battlefield);
        assert_eq!(state.objects.get(familiar).counters.revival, 0);
        assert!(!state
            .engine
            .event_history
            .iter()
            .any(|e| matches!(e, E::SpellCast { .. })));
        legacy_refuses_readonly(&state);
        assert_restore(&state);
    }
}

#[test]
fn paid_familiar_copy_and_retained_death_schedule_refuse_legacy_readonly_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let familiar = paid(&mut state, actor, "Nine-Lives Familiar", None);
        paid(&mut state, actor, "Self-Reflection", Some(familiar));
        let copies: Vec<_> = state.players[actor.index()]
            .battlefield
            .iter()
            .copied()
            .filter(|&id| {
                id != familiar
                    && state.objects.get(id).card_def == state.objects.get(familiar).card_def
            })
            .collect();
        assert_eq!(copies.len(), 1);
        assert!(state.objects.get(copies[0]).v4.is_token);
        assert_eq!(state.objects.get(copies[0]).counters.revival, 0);
        legacy_refuses_readonly(&state);
        // A second, independently paid world isolates the original public source.
        // A labelled native concealment setup below then hides its newer incarnation.
        let mut state = ready(actor);
        let familiar = paid(&mut state, actor, "Nine-Lives Familiar", None);
        paid(&mut state, actor, "Abrade", Some(familiar));
        assert!(!capture(&state, actor).unwrap().nine_lives_deaths.is_empty());
        assert_eq!(
            capture(&state, actor).unwrap().nine_lives_schedules.len(),
            1
        );
        paid(&mut state, actor, "Zombify", Some(familiar));
        paid(&mut state, actor, "Snap", Some(familiar));
        assert_eq!(state.objects.get(familiar).zone, Zone::Hand);
        conceal_returned_card_native(&mut state, familiar);
        assert!(public::reference(&state, familiar, Some(actor.opponent())).is_none());
        assert!(require_legacy_visible(&state, Some(actor.opponent())).is_err());
        legacy_refuses_readonly(&state);
        assert_restore(&state);
        let mut changed = state.clone();
        changed.nine_lives_delayed_v1.as_mut().unwrap()[0].revival_counters += 1;
        let before = changed.clone();
        assert!(require_legacy_visible(&changed, Some(actor.opponent())).is_err());
        assert_eq!(changed, before);
    }
}

#[test]
fn independent_hidden_worlds_do_not_select_a_graveyard_return_legacy_refusal_both_seats() {
    for viewer in [PlayerId::P0, PlayerId::P1] {
        let worlds: Vec<_> = [
            "Alesha, Who Laughs at Fate",
            "Nine-Lives Familiar",
            "Island",
        ]
        .into_iter()
        .map(|secret| {
            let mut state = ready(viewer);
            put(&mut state, viewer.opponent(), secret, Zone::Hand);
            put(&mut state, viewer.opponent(), secret, Zone::Library);
            let concealed = put(&mut state, viewer.opponent(), secret, Zone::Battlefield);
            // Explicit anonymous native face-down primitive, no manifest claim.
            state.objects.get_mut(concealed).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
                disguised: false,
                lookers: 0,
                hidden_by: None,
                hidden_by_source: None,
            });
            paid(&mut state, viewer, "Llanowar Elves", None);
            state
        })
        .collect();
        let mut projected = vec![];
        for state in &worlds {
            let before = state.clone();
            require_legacy_visible(state, Some(viewer)).unwrap();
            require_legacy_visible(state, None).unwrap();
            projected.push(crate::rl::observe_v1(state, viewer, 0).unwrap());
            assert_eq!(*state, before);
        }
        assert_eq!(projected[0], projected[1]);
        assert_eq!(projected[0], projected[2]);
        // A viewer's actual private knowledge may select the refusal; the
        // opponent's concealed identity alone could not do so above.
        assert!(require_legacy_visible(&worlds[0], Some(viewer.opponent())).is_err());
        assert!(require_legacy_visible(&worlds[1], Some(viewer.opponent())).is_err());
    }
}

#[test]
fn paid_alesha_departure_lki_refuses_old_viewer_after_source_becomes_hidden_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = paid(&mut state, actor, "Alesha, Who Laughs at Fate", None);
        paid(&mut state, actor, "Snap", Some(source));
        assert_eq!(state.objects.get(source).zone, Zone::Hand);
        let remembered = capture(&state, actor.opponent()).unwrap();
        assert_eq!(remembered.alesha_departures.len(), 1);
        assert_eq!(remembered.alesha_departures[0].power, 2);
        conceal_returned_card_native(&mut state, source);
        assert!(public::reference(&state, source, Some(actor.opponent())).is_none());
        let facts = capture(&state, actor.opponent()).unwrap();
        assert_eq!(facts.alesha_departures, remembered.alesha_departures);
        assert_eq!(facts.alesha_departures.len(), 1);
        assert_eq!(facts.alesha_departures[0].power, 2);
        legacy_refuses_readonly(&state);
        assert_restore(&state);
        let mut changed = state.clone();
        let marker = changed
            .engine
            .event_history
            .iter_mut()
            .find(|event| matches!(event, E::AleshaPowerLkiV1 { .. }))
            .unwrap();
        if let E::AleshaPowerLkiV1 { power, .. } = marker {
            *power += 1;
        }
        let before = changed.clone();
        assert!(require_legacy_visible(&changed, Some(actor.opponent())).is_err());
        assert_eq!(changed, before);
    }
}
