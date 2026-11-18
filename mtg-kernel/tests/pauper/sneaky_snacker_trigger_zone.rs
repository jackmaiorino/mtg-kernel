//! CR 603.2 and 113.6 (issue 771): Sneaky Snacker's "Whenever you draw your
//! third card each turn, return Sneaky Snacker from your graveyard to the
//! battlefield tapped" functions only from the graveyard, so it triggers only
//! if Snacker is already in its owner's graveyard when the third card is
//! drawn. Faithless Looting draws two cards and then discards two as one
//! resolution, so its draws and its discard are matched in one event batch;
//! a Snacker that was still in hand at the third draw must not trigger even
//! though it is in the graveyard once the batch is matched.

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{
    Counters, GameObject, GameState, ObjectStateV4, StackItemKind, Step, Zone,
};

const SEATS: [PlayerId; 2] = [PlayerId::P0, PlayerId::P1];

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

/// Main phase 1 of `player`'s turn, with that player's first draw of the
/// turn already made.
fn ready_main1(player: PlayerId, seed: u64) -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], card_name, seed);
    state.step = Step::Main1;
    state.active_player = player;
    state.priority_player = player;
    state.players[player.index()].draws_this_turn = 1;
    state
}

fn put_object(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"));
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.to_string(),
        owner: player,
        controller: player,
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
    let seat = &mut state.players[player.index()];
    match zone {
        Zone::Hand => seat.hand.push(id),
        Zone::Graveyard => seat.graveyard.push(id),
        Zone::Library => seat.library.push(id),
        other => panic!("test helper does not place cards in {other:?}"),
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

/// Advances to the next priority decision once the resolving spell is done.
fn finish_resolution(state: &mut GameState) {
    let decision = engine::advance_until_decision(state);
    assert!(
        matches!(decision, Decision::CastSpellOrPass { .. }),
        "got {decision:?}"
    );
}

fn snacker_triggers_on_stack(state: &GameState, snacker: ObjectId) -> usize {
    state
        .stack
        .iter()
        .filter(|item| item.source == snacker && item.kind == StackItemKind::TriggeredAbility)
        .count()
}

/// Both players pass until the stack is empty.
fn resolve_stack(state: &mut GameState) {
    for _ in 0..16 {
        if state.stack.is_empty() {
            return;
        }
        match engine::advance_until_decision(state) {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision while the stack resolves: {other:?}"),
        }
    }
    panic!("the stack did not resolve within the bounded walk")
}

fn assert_stays_in_graveyard(state: &GameState, player: PlayerId, snacker: ObjectId) {
    assert_eq!(snacker_triggers_on_stack(state, snacker), 0);
    assert_eq!(state.objects.get(snacker).zone, Zone::Graveyard);
    assert!(state.players[player.index()].graveyard.contains(&snacker));
}

fn assert_returned_tapped(state: &mut GameState, player: PlayerId, snacker: ObjectId) {
    assert_eq!(
        snacker_triggers_on_stack(state, snacker),
        1,
        "Sneaky Snacker's trigger waits on the stack"
    );
    resolve_stack(state);
    assert!(state.players[player.index()].battlefield.contains(&snacker));
    assert!(state.objects.get(snacker).tapped);
}

/// Casts Faithless Looting from `player`'s hand with two cards in the
/// library, then discards the cards `pick` selects from the offered choices.
fn cast_looting_and_discard(
    state: &mut GameState,
    player: PlayerId,
    pick: impl Fn(&[ObjectId]) -> Vec<ObjectId>,
) -> ObjectId {
    put_object(state, player, "Island", Zone::Library);
    put_object(state, player, "Island", Zone::Library);
    let looting = put_object(state, player, "Faithless Looting", Zone::Hand);
    state.players[player.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    engine::step(state, Action::CastSpell(looting)).unwrap();

    let Decision::Discard { count, choices, .. } = next_nonpriority(state) else {
        panic!("Faithless Looting's discard")
    };
    assert_eq!(count, 2);
    assert_eq!(state.players[player.index()].draws_this_turn, 3);
    engine::step(state, Action::Discard(pick(&choices))).unwrap();
    finish_resolution(state);
    assert_eq!(state.objects.get(looting).zone, Zone::Graveyard);
    looting
}

/// The issue's line: Snacker is in hand when Faithless Looting's second draw
/// (the third card this turn) happens, and the same Looting discards it.
#[test]
fn snacker_discarded_by_the_looting_that_draws_the_third_card_does_not_trigger() {
    for (seat, player) in SEATS.into_iter().enumerate() {
        let mut state = ready_main1(player, 0x0771_0001 + seat as u64);
        let snacker = put_object(&mut state, player, "Sneaky Snacker", Zone::Hand);
        cast_looting_and_discard(&mut state, player, |choices| {
            let other = choices
                .iter()
                .copied()
                .find(|&id| id != snacker)
                .expect("a drawn card to discard beside Snacker");
            vec![snacker, other]
        });
        assert_stays_in_graveyard(&state, player, snacker);
    }
}

/// Control: Snacker is already in the graveyard when Looting draws the third
/// card, so it returns tapped.
#[test]
fn snacker_in_the_graveyard_at_the_third_draw_returns_tapped() {
    for (seat, player) in SEATS.into_iter().enumerate() {
        let mut state = ready_main1(player, 0x0771_0011 + seat as u64);
        let snacker = put_object(&mut state, player, "Sneaky Snacker", Zone::Graveyard);
        put_object(&mut state, player, "Mountain", Zone::Hand);
        cast_looting_and_discard(&mut state, player, |choices| choices[..2].to_vec());
        assert_returned_tapped(&mut state, player, snacker);
    }
}

/// Control: Highway Robbery's discard sub-cost puts Snacker into the
/// graveyard before the same resolution draws the third card, so it returns
/// tapped even though the discard and the draws are matched in one batch.
#[test]
fn snacker_discarded_before_the_third_draw_returns_tapped() {
    for (seat, player) in SEATS.into_iter().enumerate() {
        let mut state = ready_main1(player, 0x0771_0021 + seat as u64);
        put_object(&mut state, player, "Island", Zone::Library);
        put_object(&mut state, player, "Island", Zone::Library);
        let snacker = put_object(&mut state, player, "Sneaky Snacker", Zone::Hand);
        put_object(&mut state, player, "Mountain", Zone::Hand);
        let robbery = put_object(&mut state, player, "Highway Robbery", Zone::Hand);
        state.players[player.index()].mana_pool[ManaColor::R.pool_index()] = 2;
        engine::step(&mut state, Action::CastSpell(robbery)).unwrap();

        assert!(matches!(
            next_nonpriority(&mut state),
            Decision::ChooseOptionalCost { .. }
        ));
        engine::step(
            &mut state,
            Action::ChooseOptionalCost(OptionalCostChoice::Discard),
        )
        .unwrap();
        let Decision::Discard { choices, .. } = engine::advance_until_decision(&mut state) else {
            panic!("Highway Robbery's discard sub-cost")
        };
        assert!(choices.contains(&snacker));
        engine::step(&mut state, Action::Discard(vec![snacker])).unwrap();
        finish_resolution(&mut state);
        assert_eq!(state.players[player.index()].draws_this_turn, 3);
        assert_returned_tapped(&mut state, player, snacker);
    }
}

/// Control: the third draw with no Snacker anywhere puts nothing on the
/// stack.
#[test]
fn the_third_draw_without_snacker_triggers_nothing() {
    for (seat, player) in SEATS.into_iter().enumerate() {
        let mut state = ready_main1(player, 0x0771_0031 + seat as u64);
        put_object(&mut state, player, "Mountain", Zone::Hand);
        cast_looting_and_discard(&mut state, player, |choices| choices[..2].to_vec());
        assert!(state.stack.is_empty());
    }
}
