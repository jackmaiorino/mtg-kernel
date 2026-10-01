//! CR 608.2 / 608.2n: a spell stays on the stack until the last step of its
//! resolution. mtg-kernel popped a resolving spell off the stack before a
//! resolution-time player choice (a discard, a "you may pay" cost) was
//! answered, so the observation showed the card in no zone at all (found
//! by shadowing mtg-kernel with the gorge engine: SpellBench Burn-800 steps
//! 29 and 113).

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::rl::observe_v2;
use mtg_kernel::state::{
    Counters, GameObject, GameState, ObjectStateV4, StackItemKind, Step, Zone,
};
use mtg_kernel::surface_v2::HarnessSurfaceV2;

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn ready_main1(seed: u64) -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], card_name, seed);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

fn put_object(state: &mut GameState, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"));
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.to_string(),
        owner: PlayerId::P0,
        controller: PlayerId::P0,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[0].hand.push(id),
        Zone::Battlefield => state.players[0].battlefield.push(id),
        Zone::Graveyard => state.players[0].graveyard.push(id),
        Zone::Exile => state.exile.push(id),
        Zone::Library => state.players[0].library.push(id),
        Zone::Command => state.command.push(id),
        Zone::Stack => panic!("test helper does not fabricate stack items"),
    }
    id
}

/// Passes priority until the first non-priority decision.
fn next_nonpriority(state: &mut GameState) -> Decision {
    for _ in 0..16 {
        match engine::advance_until_decision(state) {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::Halted { mechanic, source } => {
                panic!("unexpected halt {mechanic:?} from {source}")
            }
            other => return other,
        }
    }
    panic!("no non-priority decision within the bounded walk")
}

/// The resolving spell is in exactly one zone: the stack, both in the engine
/// state and in every observer's public projection.
fn assert_resolving_on_stack(state: &GameState, spell: ObjectId) {
    assert_eq!(state.objects.get(spell).zone, Zone::Stack);
    assert!(
        state.stack.iter().any(|item| item.source == spell),
        "the resolving spell stays on the stack"
    );
    for observer in [PlayerId::P0, PlayerId::P1] {
        let observation = observe_v2(state, &HarnessSurfaceV2::new(), observer, 0)
            .expect("observation while the resolution waits");
        assert!(
            observation
                .projection
                .stack
                .iter()
                .any(|item| item.source.arena_id == spell.0 && item.source.zone == Zone::Stack),
            "{observer:?} sees the resolving spell on the stack"
        );
    }
}

fn assert_resolution_finished(state: &mut GameState, spell: ObjectId) {
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
    assert!(state.stack.is_empty());
    assert!(observe_v2(state, &HarnessSurfaceV2::new(), PlayerId::P1, 0)
        .unwrap()
        .projection
        .stack
        .is_empty());
}

/// Burn-800 step 29: Faithless Looting's "then discard two cards".
#[test]
fn faithless_looting_stays_on_the_stack_while_its_discard_is_chosen() {
    let mut state = ready_main1(0x4b53_4841_444f_5708);
    put_object(&mut state, "Island", Zone::Library);
    put_object(&mut state, "Island", Zone::Library);
    put_object(&mut state, "Mountain", Zone::Hand);
    let looting = put_object(&mut state, "Faithless Looting", Zone::Hand);
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
    engine::step(&mut state, Action::CastSpell(looting)).unwrap();

    let Decision::Discard { count, choices, .. } = next_nonpriority(&mut state) else {
        panic!("Faithless Looting's discard")
    };
    assert_eq!(count, 2);
    assert_resolving_on_stack(&state, looting);

    engine::step(&mut state, Action::Discard(choices[..2].to_vec())).unwrap();
    assert_resolution_finished(&mut state, looting);
}

/// Burn-800 step 113: Highway Robbery's "you may discard a card or
/// sacrifice a land", both declining and paying the discard.
#[test]
fn highway_robbery_stays_on_the_stack_while_its_optional_cost_is_chosen() {
    for choice in [OptionalCostChoice::Decline, OptionalCostChoice::Discard] {
        let mut state = ready_main1(0x4b53_4841_444f_5709);
        put_object(&mut state, "Island", Zone::Library);
        put_object(&mut state, "Island", Zone::Library);
        // Two cards in hand, so the discard sub-cost is a real choice
        // (a forced one resolves without asking).
        put_object(&mut state, "Mountain", Zone::Hand);
        put_object(&mut state, "Island", Zone::Hand);
        let robbery = put_object(&mut state, "Highway Robbery", Zone::Hand);
        state.players[0].mana_pool[ManaColor::R.pool_index()] = 2;
        engine::step(&mut state, Action::CastSpell(robbery)).unwrap();

        assert!(matches!(
            next_nonpriority(&mut state),
            Decision::ChooseOptionalCost { .. }
        ));
        assert_resolving_on_stack(&state, robbery);
        engine::step(&mut state, Action::ChooseOptionalCost(choice)).unwrap();
        if choice == OptionalCostChoice::Discard {
            let Decision::Discard { choices, .. } = engine::advance_until_decision(&mut state)
            else {
                panic!("Highway Robbery's discard sub-cost")
            };
            assert_resolving_on_stack(&state, robbery);
            engine::step(&mut state, Action::Discard(choices[..1].to_vec())).unwrap();
        }
        assert_resolution_finished(&mut state, robbery);
    }
}

/// The same holds for an ability whose last instruction is a discard:
/// Harrier Strix's "{2}{U}: Draw a card, then discard a card." stays on the
/// stack until the discard is chosen.
#[test]
fn a_looting_ability_stays_on_the_stack_while_its_discard_is_chosen() {
    let mut state = ready_main1(0x4b53_4841_444f_570e);
    put_object(&mut state, "Island", Zone::Library);
    put_object(&mut state, "Mountain", Zone::Hand);
    put_object(&mut state, "Island", Zone::Hand);
    let strix = put_object(&mut state, "Harrier Strix", Zone::Battlefield);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 3;
    let Decision::CastSpellOrPass {
        activatable_abilities,
        ..
    } = engine::advance_until_decision(&mut state)
    else {
        panic!("priority decision")
    };
    assert!(activatable_abilities.contains(&(strix, 0)));
    engine::step(&mut state, Action::ActivateAbility(strix, 0)).unwrap();

    let Decision::Discard { count, choices, .. } = next_nonpriority(&mut state) else {
        panic!("Harrier Strix's discard")
    };
    assert_eq!(count, 1);
    assert!(
        state
            .stack
            .iter()
            .any(|item| item.source == strix && item.kind == StackItemKind::ActivatedAbility),
        "the resolving ability stays on the stack"
    );
    for observer in [PlayerId::P0, PlayerId::P1] {
        let observation = observe_v2(&state, &HarnessSurfaceV2::new(), observer, 0).unwrap();
        assert!(
            observation
                .projection
                .stack
                .iter()
                .any(|item| item.source.arena_id == strix.0),
            "{observer:?} sees the resolving ability"
        );
    }

    engine::step(&mut state, Action::Discard(choices[..1].to_vec())).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(state.stack.is_empty());
    assert_eq!(state.objects.get(strix).zone, Zone::Battlefield);
}
