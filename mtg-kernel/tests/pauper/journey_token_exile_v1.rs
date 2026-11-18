//! Journey to Nowhere exiling a creature token made in the game by a card
//! of the pool (Experimental Synthesizer's Samurai). CR 111.7, 111.8 and
//! 704.5d: the token ceases to exist in exile, so when Journey leaves the
//! battlefield nothing returns, nothing halts and no linked-exile record is
//! left behind. A creature card exiled the same way still returns.

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{Counters, GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::trigger;

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

fn collect_triggers(state: &mut GameState) {
    let pending = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(pending);
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

fn idle(state: &GameState) -> bool {
    state.stack.is_empty()
        && state.engine.pending_triggers.is_empty()
        && state.engine.pending_effect.is_none()
}

/// The maker activates Experimental Synthesizer ({2}{R}, sacrifice it) on
/// its own turn and gets a real Samurai token.
fn make_samurai(state: &mut GameState, maker: PlayerId) -> ObjectId {
    let synthesizer = put_object(state, maker, "Experimental Synthesizer", Zone::Battlefield);
    put_object(state, maker, "Mountain", Zone::Library);
    state.players[maker.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    state.players[maker.index()].mana_pool[ManaColor::C.pool_index()] = 2;
    engine::step(state, Action::ActivateAbility(synthesizer, 0)).unwrap();
    pass_until(state, idle);
    assert_eq!(state.objects.get(synthesizer).zone, Zone::Graveyard);
    let samurai = card_id("Samurai Token");
    let token = *state.players[maker.index()]
        .battlefield
        .iter()
        .find(|&&id| state.objects.get(id).card_def == samurai)
        .expect("Experimental Synthesizer made a Samurai token");
    assert!(state.objects.get(token).v4.is_token);
    token
}

/// `caster` casts Journey to Nowhere and its trigger exiles `target`.
fn journey_exiles(state: &mut GameState, caster: PlayerId, target: ObjectId) -> ObjectId {
    let journey = put_object(state, caster, "Journey to Nowhere", Zone::Hand);
    state.players[caster.index()].mana_pool[ManaColor::W.pool_index()] = 2;
    engine::step(state, Action::CastSpell(journey)).unwrap();
    let mut targeted = false;
    for _ in 0..24 {
        match engine::advance_until_decision(state) {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::ChooseTargets {
                player,
                legal_targets,
                ..
            } => {
                assert_eq!(player, caster);
                assert!(legal_targets.contains(&Target::Object(target)));
                engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
                targeted = true;
                break;
            }
            other => panic!("unexpected decision before Journey's target: {other:?}"),
        }
    }
    assert!(targeted, "Journey's trigger asked for its target");
    pass_until(state, |state| {
        idle(state) && state.objects.get(target).zone == Zone::Exile
    });
    assert_eq!(state.objects.get(journey).zone, Zone::Battlefield);
    journey
}

fn journey_leaves(state: &mut GameState, journey: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(journey, Zone::Graveyard));
    collect_triggers(state);
    pass_until(state, idle);
    assert!(state.engine.halted.is_none());
    assert_eq!(state.objects.get(journey).zone, Zone::Graveyard);
}

#[test]
fn journey_leaving_does_not_return_an_exiled_samurai_token() {
    for (maker, seed) in [
        (PlayerId::P0, 0x4a54_4f4b_454e_0001),
        (PlayerId::P1, 0x4a54_4f4b_454e_0002),
    ] {
        let caster = maker.opponent();
        let mut state = ready_main(seed, maker);
        let token = make_samurai(&mut state, maker);
        // The caster's next main phase.
        state.active_player = caster;
        state.priority_player = caster;
        let journey = journey_exiles(&mut state, caster, token);
        assert!(
            !state.exile.contains(&token),
            "the exiled token ceased to exist"
        );
        assert!(state.engine.linked_exile_records.is_empty());

        journey_leaves(&mut state, journey);
        for player in [PlayerId::P0, PlayerId::P1] {
            assert!(!state.players[player.index()].battlefield.contains(&token));
        }
        assert!(!state.exile.contains(&token));
        assert_ne!(state.objects.get(token).zone, Zone::Battlefield);
        assert!(state.engine.linked_exile_records.is_empty());
    }
}

/// Control: a creature card exiled by Journey returns under its owner's
/// control when Journey leaves.
#[test]
fn journey_leaving_returns_an_exiled_creature_card() {
    for (owner, seed) in [
        (PlayerId::P0, 0x4a54_4f4b_454e_0003),
        (PlayerId::P1, 0x4a54_4f4b_454e_0004),
    ] {
        let caster = owner.opponent();
        let mut state = ready_main(seed, caster);
        let creature = put_object(&mut state, owner, "Voldaren Epicure", Zone::Battlefield);
        let journey = journey_exiles(&mut state, caster, creature);
        assert!(state.exile.contains(&creature));
        assert_eq!(state.engine.linked_exile_records.len(), 1);

        journey_leaves(&mut state, journey);
        assert_eq!(state.objects.get(creature).zone, Zone::Battlefield);
        assert!(state.players[owner.index()].battlefield.contains(&creature));
        assert_eq!(state.objects.get(creature).controller, owner);
        assert!(state.engine.linked_exile_records.is_empty());
    }
}
