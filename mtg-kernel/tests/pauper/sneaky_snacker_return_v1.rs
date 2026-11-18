//! Sneaky Snacker's "whenever you draw your third card each turn, return
//! this card from your graveyard to the battlefield tapped" against
//! graveyard hate. CR 400.7 and 608.2b: once the card leaves the graveyard
//! it is a new object the resolving ability cannot find, so exiling the
//! graveyard in response (Nihil Spellbomb, Relic of Progenitus) leaves it in
//! exile, and a card that left the graveyard and came back is not returned.

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{Counters, GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn ready_main(seed: u64, active: PlayerId) -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], card_name, seed);
    state.step = Step::Main1;
    state.active_player = active;
    state.priority_player = active;
    state
}

fn put_object(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id(name);
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.to_string(),
        owner,
        controller: owner,
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
        Zone::Hand => state.players[owner.index()].hand.push(id),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
        Zone::Library => state.players[owner.index()].library.push(id),
        Zone::Graveyard => state.players[owner.index()].graveyard.push(id),
        Zone::Exile => state.exile.push(id),
        Zone::Command => state.command.push(id),
        Zone::Stack => panic!("test helper does not fabricate stack objects"),
    }
    id
}

/// Passes priority until `done` holds, failing on a halt or on any
/// non-priority decision.
fn pass_until(state: &mut GameState, done: impl Fn(&GameState) -> bool) -> Decision {
    for _ in 0..64 {
        let decision = engine::advance_until_decision(state);
        if let Decision::Halted { mechanic, source } = decision {
            panic!("unexpected halt {mechanic:?} from {source}");
        }
        if done(state) {
            return decision;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision while passing: {other:?}"),
        }
    }
    panic!("condition did not become true within bounded priority walk")
}

/// Passes priority until the stack is empty and returns whether any stack
/// item from `watched` was seen on the way.
fn pass_until_idle_watching(state: &mut GameState, watched: ObjectId) -> bool {
    let mut seen = false;
    for _ in 0..96 {
        let decision = engine::advance_until_decision(state);
        seen |= state.stack.iter().any(|item| item.source == watched);
        if let Decision::Halted { mechanic, source } = decision {
            panic!("unexpected halt {mechanic:?} from {source}");
        }
        if state.stack.is_empty()
            && state.engine.pending_effect.is_none()
            && matches!(decision, Decision::CastSpellOrPass { .. })
        {
            return seen;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision while resolving to idle: {other:?}"),
        }
    }
    panic!("bounded resolution did not become idle")
}

struct SnackerGame {
    state: GameState,
    me: PlayerId,
    snacker: ObjectId,
    deep_analysis: ObjectId,
}

/// `me` is the active player with Sneaky Snacker in the graveyard, one card
/// already drawn this turn and Deep Analysis in hand: casting it on itself
/// draws the second and third cards.
fn snacker_game(seed: u64, me: PlayerId) -> SnackerGame {
    let mut state = ready_main(seed, me);
    let snacker = put_object(&mut state, me, "Sneaky Snacker", Zone::Graveyard);
    put_object(&mut state, me, "Mountain", Zone::Library);
    put_object(&mut state, me, "Mountain", Zone::Library);
    put_object(&mut state, me.opponent(), "Island", Zone::Library);
    let deep_analysis = put_object(&mut state, me, "Deep Analysis", Zone::Hand);
    state.players[me.index()].draws_this_turn = 1;
    state.players[me.index()].mana_pool[ManaColor::U.pool_index()] = 1;
    state.players[me.index()].mana_pool[ManaColor::C.pool_index()] = 3;
    SnackerGame {
        state,
        me,
        snacker,
        deep_analysis,
    }
}

/// Casts Deep Analysis targeting its caster and resolves it, leaving Sneaky
/// Snacker's trigger alone on the stack with the active player holding
/// priority.
fn snacker_trigger_on_the_stack(game: &mut SnackerGame) {
    engine::step(&mut game.state, Action::CastSpell(game.deep_analysis)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut game.state),
        Decision::ChooseTargets { .. }
    ));
    let me = game.me;
    engine::step(&mut game.state, Action::ChooseTarget(Target::Player(me))).unwrap();
    let snacker = game.snacker;
    let decision = pass_until(&mut game.state, |state| {
        state.stack.len() == 1 && state.stack[0].source == snacker
    });
    assert!(
        matches!(decision, Decision::CastSpellOrPass { player, .. } if player == me),
        "the active player has priority over the trigger: {decision:?}"
    );
    assert_eq!(game.state.objects.get(snacker).zone, Zone::Graveyard);
}

/// The active player passes with the trigger on the stack, so the opponent
/// receives priority.
fn opponent_gets_priority(game: &mut SnackerGame) {
    engine::step(&mut game.state, Action::Pass).unwrap();
    let opponent = game.me.opponent();
    let decision = engine::advance_until_decision(&mut game.state);
    assert!(
        matches!(decision, Decision::CastSpellOrPass { player, .. } if player == opponent),
        "the opponent may respond to the trigger: {decision:?}"
    );
}

fn assert_snacker_stayed_in_exile(game: &SnackerGame) {
    let state = &game.state;
    assert!(state.engine.halted.is_none());
    assert_eq!(state.objects.get(game.snacker).zone, Zone::Exile);
    assert!(state.exile.contains(&game.snacker));
    for player in [PlayerId::P0, PlayerId::P1] {
        assert!(
            !state.players[player.index()]
                .battlefield
                .contains(&game.snacker),
            "the exiled card is a new object and does not return"
        );
    }
    assert!(!state.objects.get(game.snacker).tapped);
}

#[test]
fn sneaky_snacker_stays_exiled_when_nihil_spellbomb_answers_its_trigger() {
    for (me, seed) in [
        (PlayerId::P0, 0x534e_4143_4b00_0001),
        (PlayerId::P1, 0x534e_4143_4b00_0002),
    ] {
        let mut game = snacker_game(seed, me);
        let spellbomb = put_object(
            &mut game.state,
            me.opponent(),
            "Nihil Spellbomb",
            Zone::Battlefield,
        );
        snacker_trigger_on_the_stack(&mut game);
        opponent_gets_priority(&mut game);
        engine::step(&mut game.state, Action::ActivateAbility(spellbomb, 0)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut game.state),
            Decision::ChooseTargets { .. }
        ));
        engine::step(&mut game.state, Action::ChooseTarget(Target::Player(me))).unwrap();
        pass_until_idle_watching(&mut game.state, game.snacker);
        assert_eq!(game.state.objects.get(spellbomb).zone, Zone::Graveyard);
        assert_snacker_stayed_in_exile(&game);
    }
}

#[test]
fn sneaky_snacker_stays_exiled_when_relic_of_progenitus_answers_its_trigger() {
    for (me, seed) in [
        (PlayerId::P0, 0x534e_4143_4b00_0003),
        (PlayerId::P1, 0x534e_4143_4b00_0004),
    ] {
        let mut game = snacker_game(seed, me);
        let relic = put_object(
            &mut game.state,
            me.opponent(),
            "Relic of Progenitus",
            Zone::Battlefield,
        );
        game.state.players[me.opponent().index()].mana_pool[ManaColor::C.pool_index()] = 1;
        snacker_trigger_on_the_stack(&mut game);
        opponent_gets_priority(&mut game);
        engine::step(&mut game.state, Action::ActivateAbility(relic, 1)).unwrap();
        pass_until_idle_watching(&mut game.state, game.snacker);
        assert_eq!(game.state.objects.get(relic).zone, Zone::Exile);
        assert_snacker_stayed_in_exile(&game);
    }
}

/// Control: with no response the trigger returns Sneaky Snacker tapped.
#[test]
fn sneaky_snacker_returns_tapped_when_nobody_responds() {
    for (me, seed) in [
        (PlayerId::P0, 0x534e_4143_4b00_0005),
        (PlayerId::P1, 0x534e_4143_4b00_0006),
    ] {
        let mut game = snacker_game(seed, me);
        snacker_trigger_on_the_stack(&mut game);
        pass_until_idle_watching(&mut game.state, game.snacker);
        let state = &game.state;
        assert!(state.engine.halted.is_none());
        assert_eq!(state.objects.get(game.snacker).zone, Zone::Battlefield);
        assert!(state.players[me.index()]
            .battlefield
            .contains(&game.snacker));
        assert!(state.objects.get(game.snacker).tapped);
    }
}

/// Control: Relic of Progenitus exiles every graveyard and then draws the
/// third card. Sneaky Snacker is in exile when the card is drawn, so its
/// ability, which functions only from the graveyard, does not trigger.
#[test]
fn sneaky_snacker_exiled_before_the_third_draw_never_triggers() {
    for (me, seed) in [
        (PlayerId::P0, 0x534e_4143_4b00_0007),
        (PlayerId::P1, 0x534e_4143_4b00_0008),
    ] {
        let mut game = snacker_game(seed, me);
        game.state.players[me.index()].draws_this_turn = 2;
        let relic = put_object(
            &mut game.state,
            me,
            "Relic of Progenitus",
            Zone::Battlefield,
        );
        engine::step(&mut game.state, Action::ActivateAbility(relic, 1)).unwrap();
        let triggered = pass_until_idle_watching(&mut game.state, game.snacker);
        assert!(!triggered, "no Sneaky Snacker trigger was put on the stack");
        assert_eq!(game.state.players[me.index()].draws_this_turn, 3);
        assert_snacker_stayed_in_exile(&game);
    }
}

/// Sneaky Snacker leaves the graveyard and is put back into it while its
/// trigger waits. The card in the graveyard is a new object, so the trigger
/// does nothing and the card stays where it is, untapped.
#[test]
fn sneaky_snacker_back_in_the_graveyard_as_a_new_object_is_not_returned() {
    for (me, seed) in [
        (PlayerId::P0, 0x534e_4143_4b00_0009),
        (PlayerId::P1, 0x534e_4143_4b00_000a),
    ] {
        let mut game = snacker_game(seed, me);
        snacker_trigger_on_the_stack(&mut game);
        let before = game.state.objects.get(game.snacker).zone_change_count;
        event::propose_and_commit(
            &mut game.state,
            ProposedEvent::zone_change(game.snacker, Zone::Hand),
        );
        event::propose_and_commit(
            &mut game.state,
            ProposedEvent::zone_change(game.snacker, Zone::Graveyard),
        );
        assert!(game.state.objects.get(game.snacker).zone_change_count > before);
        pass_until_idle_watching(&mut game.state, game.snacker);
        let state = &game.state;
        assert!(state.engine.halted.is_none());
        assert_eq!(state.objects.get(game.snacker).zone, Zone::Graveyard);
        assert!(state.players[me.index()].graveyard.contains(&game.snacker));
        assert!(!state.players[me.index()]
            .battlefield
            .contains(&game.snacker));
        assert!(!state.objects.get(game.snacker).tapped);
    }
}
