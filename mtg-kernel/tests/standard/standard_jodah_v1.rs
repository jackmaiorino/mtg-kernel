//! MageZero Standard family D: newer set keywords (prowess, valiant, speed,
//! warp, offspring, plot, crime and friends) in the Standard catalog.
#![cfg(feature = "standard-magezero-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

/// A main-phase state with `library` cards in both libraries, priority to P0.
fn ready_with(library: &str) -> GameState {
    let card = card_id_by_name(library).unwrap();
    let name = library.to_owned();
    let mut state =
        GameState::new_from_libraries(&[card; 30], &[card; 30], move |_| name.clone(), 0x4644);
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state.step = Step::Main1;
    state
}

fn ready() -> GameState {
    ready_with("Mountain")
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
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
    let seat = &mut state.players[player.index()];
    match zone {
        Zone::Hand => seat.hand.push(id),
        Zone::Battlefield => seat.battlefield.push(id),
        Zone::Graveyard => seat.graveyard.push(id),
        _ => panic!("helper zone"),
    }
    id
}

fn add_mana(state: &mut GameState, player: PlayerId, colored: &[ManaColor], generic: u8) {
    let pool = &mut state.players[player.index()].mana_pool;
    for color in colored {
        pool[color.pool_index()] += 1;
    }
    pool[5] += generic;
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

/// Casts `spell`, answering each target prompt from `targets` in order.
fn cast(state: &mut GameState, spell: ObjectId, targets: &[Target]) {
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell)),
        "{} not castable",
        state.objects.get(spell).name
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    for target in targets {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(
                    legal_targets.contains(target),
                    "{target:?} not in {legal_targets:?}"
                );
                engine::step(state, Action::ChooseTarget(*target)).unwrap();
            }
            other => panic!("expected targets, got {other:?}"),
        }
    }
}

fn set_library_card(state: &mut GameState, position: usize, name: &str) -> ObjectId {
    let id = state.players[0].library[position];
    let def = card_id_by_name(name).unwrap();
    let object = state.objects.get_mut(id);
    object.card_def = def;
    object.name = CARD_DEFS[def as usize].object_name.into();
    object.v4 = ObjectStateV4::from_card_def(def);
    id
}

#[test]
fn jodah_uses_bound_mana_value_after_original_is_countered_and_can_cast_or_decline() {
    for should_cast in [false, true] {
        let mut state = ready();
        let too_large = set_library_card(&mut state, 0, "Katilda, Dawnhart Prime");
        let hit = set_library_card(&mut state, 1, "Skrelv, Defector Mite");
        put(
            &mut state,
            PlayerId::P0,
            "Jodah, the Unifier",
            Zone::Battlefield,
        );
        let katilda = put(
            &mut state,
            PlayerId::P0,
            "Katilda, Dawnhart Prime",
            Zone::Hand,
        );
        add_mana(&mut state, PlayerId::P0, &[ManaColor::W, ManaColor::G], 0);
        cast(&mut state, katilda, &[]);
        next(&mut state);
        assert_eq!(state.stack.len(), 2);
        state.priority_player = PlayerId::P1;
        let counter = put(&mut state, PlayerId::P1, "Counterspell", Zone::Hand);
        add_mana(&mut state, PlayerId::P1, &[ManaColor::U, ManaColor::U], 0);
        cast(&mut state, counter, &[Target::Object(katilda)]);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectTargets { .. })
        ));
        assert_eq!(state.objects.get(katilda).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(too_large).zone, Zone::Exile);
        assert_eq!(state.objects.get(hit).zone, Zone::Exile);
        let mut restored: GameState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        for state in [&mut state, &mut restored] {
            engine::step(
                state,
                if should_cast {
                    Action::ChooseEffectTarget(Target::Object(hit))
                } else {
                    Action::FinishEffectSelection
                },
            )
            .unwrap();
            settled(state);
            assert_eq!(state.objects.get(too_large).zone, Zone::Library);
            assert_eq!(
                state.objects.get(hit).zone,
                if should_cast {
                    Zone::Battlefield
                } else {
                    Zone::Library
                }
            );
            assert!(state.exile.is_empty());
            // The hit cast from exile cannot recursively trigger Jodah.
            let exiles = state
                .engine
                .event_history
                .iter()
                .filter(|event| {
                    matches!(
                        event,
                        mtg_kernel::event::CommittedEvent::ZoneChange {
                            from: Zone::Library,
                            to: Zone::Exile,
                            ..
                        }
                    )
                })
                .count();
            assert_eq!(exiles, 2);
        }
        assert_eq!(state, restored);
    }
}

#[test]
fn jodah_does_not_trigger_on_a_nonlegendary_creature() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Jodah, the Unifier",
        Zone::Battlefield,
    );
    let creature = put(&mut state, PlayerId::P0, "Llanowar Elves", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::G], 0);
    cast(&mut state, creature, &[]);
    next(&mut state);
    assert_eq!(state.stack.len(), 1);
    settled(&mut state);
    assert!(state.exile.is_empty());
}

#[test]
fn jodah_rejects_a_changed_bound_mana_value_after_snapshot_restore() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Jodah, the Unifier",
        Zone::Battlefield,
    );
    let katilda = put(
        &mut state,
        PlayerId::P0,
        "Katilda, Dawnhart Prime",
        Zone::Hand,
    );
    add_mana(&mut state, PlayerId::P0, &[ManaColor::W, ManaColor::G], 0);
    cast(&mut state, katilda, &[]);
    next(&mut state);
    let effect = state
        .stack
        .last_mut()
        .unwrap()
        .inline_effect
        .as_mut()
        .unwrap();
    let mtg_kernel::effect::EffectOp::Sequence(ops) = effect else {
        panic!("bound Jodah program");
    };
    let mtg_kernel::effect::EffectOp::StandardLegendV1(
        mtg_kernel::standard_legends_v1::LegendEffectV1::JodahCastSnapshot(binding),
    ) = &mut ops[0]
    else {
        panic!("cast marker");
    };
    binding.mana_value = 12;
    let surface = mtg_kernel::surface_v2::HarnessSurfaceV2::new();
    assert!(mtg_kernel::rl::observe_v2(&state, &surface, PlayerId::P0, 0).is_err());
}
