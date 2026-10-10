//! Fresh FDN cast observers; catalog v66 / IDs338-339 pending serial integration.
#![cfg(feature = "limited-fdn-fixtures")]
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::effect::{EffectTargetSelectionPurpose, PendingEffectChoice};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::trigger;

fn ready() -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state =
        GameState::new_from_libraries(&[forest; 40], &[forest; 40], |_| "Forest".into(), 123);
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: zone == Zone::Hand,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        _ => panic!("helper zone"),
    }
    id
}

/// Moves an object between zones outside the stack and queues its triggers.
fn move_to(state: &mut GameState, id: ObjectId, zone: Zone) {
    event::propose_and_commit(state, ProposedEvent::zone_change(id, zone));
    let triggers = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(triggers);
}

fn mana(cost: &[(ManaColor, u8)], generic: u8) -> [u8; 6] {
    let mut pool = [0; 6];
    for &(color, count) in cost {
        pool[color.pool_index()] += count;
    }
    pool[5] += generic;
    pool
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

/// Passes priority and orders triggers until the stack and pending triggers
/// are empty, returning the first other decision if one interrupts.
fn settle(state: &mut GameState) -> Option<Decision> {
    for _ in 0..100 {
        match next(state) {
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return None
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::ChooseEffectTargets {
                player,
                source,
                selected_count: 0,
                min_targets: 2,
                max_targets: 2,
                legal_targets,
                can_finish: false,
            } if state.objects.get(source).card_def == card_id_by_name("Mental Note").unwrap() => {
                assert_eq!(player, PlayerId::P0);
                assert_eq!(legal_targets.len(), 2);
                let continuation = state.engine.pending_effect.as_ref().unwrap();
                let Some(PendingEffectChoice::SelectTargets {
                    player: chooser,
                    selected,
                    legal,
                    ordered: true,
                    purpose: EffectTargetSelectionPurpose::OrderMilledIntoGraveyard,
                    ..
                }) = &continuation.choice
                else {
                    panic!("unexpected Mental Note choice");
                };
                assert_eq!(*chooser, player);
                assert!(selected.is_empty());
                assert_eq!(legal.len(), 2);
                for ((candidate, target), object) in legal
                    .iter()
                    .zip(&legal_targets)
                    .zip(&state.players[player.index()].library[..2])
                {
                    assert_eq!(candidate.target, *target);
                    assert_eq!(*target, Target::Object(*object));
                    let binding = candidate.expected_object.unwrap();
                    assert_eq!(binding.object, *object);
                    assert_eq!(binding.expected_zone, Zone::Library);
                    assert_eq!(
                        binding.expected_zone_change_count,
                        state.objects.get(*object).zone_change_count
                    );
                }
                engine::step(state, Action::ChooseEffectTarget(legal_targets[0])).unwrap();
            }
            other => return Some(other),
        }
    }
    panic!("did not settle");
}

fn settled(state: &mut GameState) {
    if let Some(other) = settle(state) {
        panic!("unexpected choice: {other:?}");
    }
}

fn cast(state: &mut GameState, spell: ObjectId, target: Option<ObjectId>) {
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell)),
        "{} not castable",
        state.objects.get(spell).name
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    if let Some(target) = target {
        assert!(
            matches!(next(state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(target)))
        );
        engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    }
    priority(state);
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

fn priority(state: &mut GameState) -> Decision {
    for _ in 0..8 {
        match next(state) {
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap();
            }
            choice @ Decision::CastSpellOrPass { .. } => return choice,
            choice => panic!("unexpected decision: {choice:?}"),
        }
    }
    panic!("trigger ordering did not finish")
}

fn restore(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn change_controller(state: &mut GameState, object: ObjectId, controller: PlayerId) {
    let old = state.objects.get(object).controller;
    state.players[old.index()]
        .battlefield
        .retain(|&id| id != object);
    state.players[controller.index()].battlefield.push(object);
    state.objects.get_mut(object).controller = controller;
}

fn cast_note(state: &mut GameState) -> ObjectId {
    state.players[0].mana_pool = mana(&[(ManaColor::U, 1)], 0);
    let spell = put(state, PlayerId::P0, "Mental Note", Zone::Hand);
    cast(state, spell, None);
    spell
}

#[test]
fn printed_metadata_and_exact_cast_costs() {
    for (offset, name, pips, generic, subs, stats) in [
        (
            0,
            "Balmor, Battlemage Captain",
            vec![ManaColor::U, ManaColor::R],
            0,
            vec![Subtype::Bird, Subtype::Wizard],
            (1, 3),
        ),
        (
            1,
            "Firespitter Whelp",
            vec![ManaColor::R],
            2,
            vec![Subtype::Dragon],
            (2, 2),
        ),
    ] {
        let id = card_id_by_name(name).unwrap();
        assert_eq!(id as usize, 338 + offset);
        let def = &CARD_DEFS[id as usize];
        assert_eq!(def.capability, CardCapability::Full);
        assert_eq!(def.cost.generic, generic);
        assert_eq!(def.cost.pips.len(), pips.len());
        assert_eq!(def.colors, pips);
        assert_eq!(def.mana_value, u16::from(generic) + pips.len() as u16);
        assert_eq!(def.types, &[CardType::Creature]);
        assert_eq!(def.subtypes, subs);
        assert_eq!((def.power, def.toughness), (Some(stats.0), Some(stats.1)));
        assert!(def.keywords.has(Keywords::FLYING));
        assert_eq!(trigger::triggers_for(id).len(), 1);
        let mut state = ready();
        state.players[0].mana_pool = mana(
            &pips.into_iter().map(|color| (color, 1)).collect::<Vec<_>>(),
            generic,
        );
        let creature = put(&mut state, PlayerId::P0, name, Zone::Hand);
        cast(&mut state, creature, None);
        assert_eq!(
            state.stack.len(),
            1,
            "own battlefield trigger cannot observe its own cast"
        );
        settled(&mut state);
        assert_eq!(state.objects.get(creature).zone, Zone::Battlefield);
    }
}

#[test]
fn spell_filters_apply_to_actual_casts_and_dragon_triggers_once() {
    for (name, pips, generic, balmor_count, whelp_count) in [
        ("Mental Note", vec![ManaColor::U], 0, 1, 1),
        ("Golden Egg", vec![], 2, 0, 1),
        ("Llanowar Elves", vec![ManaColor::G], 0, 0, 0),
        ("Firespitter Whelp", vec![ManaColor::R], 2, 0, 1),
    ] {
        let mut state = ready();
        let balmor = put(
            &mut state,
            PlayerId::P0,
            "Balmor, Battlemage Captain",
            Zone::Battlefield,
        );
        let whelp = put(
            &mut state,
            PlayerId::P0,
            "Firespitter Whelp",
            Zone::Battlefield,
        );
        let spell = put(&mut state, PlayerId::P0, name, Zone::Hand);
        state.players[0].mana_pool = mana(
            &pips.into_iter().map(|color| (color, 1)).collect::<Vec<_>>(),
            generic,
        );
        cast(&mut state, spell, None);
        assert_eq!(
            state
                .stack
                .iter()
                .filter(|item| item.source == balmor)
                .count(),
            balmor_count
        );
        assert_eq!(
            state
                .stack
                .iter()
                .filter(|item| item.source == whelp)
                .count(),
            whelp_count
        );
        let mut replay = restore(&state);
        settled(&mut state);
        settled(&mut replay);
        assert_eq!(state, replay);
        assert_eq!(state.players[1].life, 20 - whelp_count as i32);
    }
}

#[test]
fn balmors_effect_samples_resolution_creatures_and_preserves_incarnations() {
    let mut state = ready();
    let balmor = put(
        &mut state,
        PlayerId::P0,
        "Balmor, Battlemage Captain",
        Zone::Battlefield,
    );
    let departed = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let taken = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let incoming = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    cast_note(&mut state);
    change_controller(&mut state, taken, PlayerId::P1);
    change_controller(&mut state, incoming, PlayerId::P0);
    move_to(&mut state, departed, Zone::Hand);
    move_to(&mut state, departed, Zone::Battlefield);
    let before_resolution = put(
        &mut state,
        PlayerId::P0,
        "Firebrand Archer",
        Zone::Battlefield,
    );
    let mut replay = restore(&state);
    settled(&mut state);
    settled(&mut replay);
    assert_eq!(state, replay);
    for object in [balmor, departed, incoming, before_resolution] {
        assert!(engine::has_effective_keyword(
            &state,
            object,
            Keywords::TRAMPLE
        ));
        assert_eq!(
            engine::effective_power(&state, object),
            CARD_DEFS[state.objects.get(object).card_def as usize]
                .power
                .unwrap() as i32
                + 1
        );
    }
    assert!(!engine::has_effective_keyword(
        &state,
        taken,
        Keywords::TRAMPLE
    ));
    change_controller(&mut state, incoming, PlayerId::P1);
    assert!(engine::has_effective_keyword(
        &state,
        incoming,
        Keywords::TRAMPLE
    ));
    let later = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    assert!(!engine::has_effective_keyword(
        &state,
        later,
        Keywords::TRAMPLE
    ));
    move_to(&mut state, departed, Zone::Hand);
    move_to(&mut state, departed, Zone::Battlefield);
    assert!(!engine::has_effective_keyword(
        &state,
        departed,
        Keywords::TRAMPLE
    ));
    state.step = Step::End;
    state.engine.priority_passes = [false, false];
    priority(&mut state);
    engine::step(&mut state, Action::Pass).unwrap();
    priority(&mut state);
    engine::step(&mut state, Action::Pass).unwrap();
    next(&mut state);
    assert!(state.engine.until_end_of_turn.is_empty());
    assert!(!engine::has_effective_keyword(
        &state,
        incoming,
        Keywords::TRAMPLE
    ));
}

#[test]
fn trigger_controller_is_frozen_after_source_leaves_or_changes_control() {
    for leaves in [false, true] {
        let mut state = ready();
        let balmor = put(
            &mut state,
            PlayerId::P0,
            "Balmor, Battlemage Captain",
            Zone::Battlefield,
        );
        let whelp = put(
            &mut state,
            PlayerId::P0,
            "Firespitter Whelp",
            Zone::Battlefield,
        );
        let elf = put(
            &mut state,
            PlayerId::P0,
            "Llanowar Elves",
            Zone::Battlefield,
        );
        cast_note(&mut state);
        assert_eq!(state.stack.len(), 3);
        assert!(state
            .stack
            .iter()
            .all(|item| item.controller == PlayerId::P0));
        for source in [balmor, whelp] {
            if leaves {
                move_to(&mut state, source, Zone::Graveyard);
            } else {
                change_controller(&mut state, source, PlayerId::P1);
            }
        }
        let mut replay = restore(&state);
        settled(&mut state);
        settled(&mut replay);
        assert_eq!(state, replay);
        assert_eq!((state.players[0].life, state.players[1].life), (20, 19));
        assert_eq!(engine::effective_power(&state, elf), 2);
        assert!(engine::has_effective_keyword(
            &state,
            elf,
            Keywords::TRAMPLE
        ));
    }
}

#[test]
fn stacked_casts_accumulate_and_countering_spell_preserves_both_observers() {
    let mut state = ready();
    let balmor = put(
        &mut state,
        PlayerId::P0,
        "Balmor, Battlemage Captain",
        Zone::Battlefield,
    );
    put(
        &mut state,
        PlayerId::P0,
        "Firespitter Whelp",
        Zone::Battlefield,
    );
    let spell = cast_note(&mut state);
    cast_note(&mut state);
    assert_eq!(state.stack.len(), 6);
    assert!(matches!(
        priority(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    engine::step(&mut state, Action::Pass).unwrap();
    assert!(matches!(
        priority(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P1,
            ..
        }
    ));
    state.players[1].mana_pool = mana(&[(ManaColor::U, 2)], 0);
    let counter = put(&mut state, PlayerId::P1, "Counterspell", Zone::Hand);
    engine::step(&mut state, Action::CastSpell(counter)).unwrap();
    assert!(
        matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(spell)))
    );
    engine::step(&mut state, Action::ChooseTarget(Target::Object(spell))).unwrap();
    priority(&mut state);
    assert_eq!(state.stack.len(), 7);
    let mut replay = restore(&state);
    settled(&mut state);
    settled(&mut replay);
    assert_eq!(state, replay);
    assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
    assert_eq!(state.players[1].life, 18);
    assert_eq!(engine::effective_power(&state, balmor), 3);
    assert_eq!(engine::effective_toughness(&state, balmor), 3);
}

#[test]
fn adventure_face_is_one_noncreature_cast_and_triggers_both_observers() {
    let mut state = ready();
    let balmor = put(
        &mut state,
        PlayerId::P0,
        "Balmor, Battlemage Captain",
        Zone::Battlefield,
    );
    let whelp = put(
        &mut state,
        PlayerId::P0,
        "Firespitter Whelp",
        Zone::Battlefield,
    );
    let dragon = put(&mut state, PlayerId::P0, "Fang Dragon", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::R, 1)], 1);
    cast(&mut state, dragon, None);
    assert_eq!(state.stack.len(), 3);
    assert_eq!(
        state
            .stack
            .iter()
            .filter(|item| item.source == balmor)
            .count(),
        1
    );
    assert_eq!(
        state
            .stack
            .iter()
            .filter(|item| item.source == whelp)
            .count(),
        1
    );
    let mut replay = restore(&state);
    settled(&mut state);
    settled(&mut replay);
    assert_eq!(state, replay);
    assert_eq!(state.players[1].life, 19);
    assert_eq!(state.objects.get(dragon).zone, Zone::Exile);
}

#[test]
fn observer_uses_current_controller_when_cast_is_committed() {
    let mut state = ready();
    let balmor = put(
        &mut state,
        PlayerId::P0,
        "Balmor, Battlemage Captain",
        Zone::Battlefield,
    );
    let whelp = put(
        &mut state,
        PlayerId::P0,
        "Firespitter Whelp",
        Zone::Battlefield,
    );
    change_controller(&mut state, balmor, PlayerId::P1);
    change_controller(&mut state, whelp, PlayerId::P1);
    cast_note(&mut state);
    assert_eq!(state.stack.len(), 1);
    settled(&mut state);
    assert_eq!((state.players[0].life, state.players[1].life), (20, 20));
    assert!(!engine::has_effective_keyword(
        &state,
        balmor,
        Keywords::TRAMPLE
    ));
}
