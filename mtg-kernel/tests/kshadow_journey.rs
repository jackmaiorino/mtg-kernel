//! Journey to Nowhere cast when no creature it could target is on the
//! battlefield (found by shadowing mtg-kernel with the gorge engine in
//! SpellBench Caw-Gates games: arena heldout m0003p0014g0, where the only
//! creature was Guardian of the Guildpact and the bridge failed closed on a
//! decision with zero legal actions).

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{Counters, GameObject, GameState, ObjectStateV4, Step, Zone};
use mtg_kernel::trigger;

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn ready_game(seed: u64) -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], card_name, seed);
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state.step = Step::Main1;
    state
}

fn put_object(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id(name);
    let object = state.objects.push(GameObject {
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
        Zone::Hand => state.players[owner.index()].hand.push(object),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(object),
        Zone::Library => state.players[owner.index()].library.push(object),
        Zone::Graveyard => state.players[owner.index()].graveyard.push(object),
        Zone::Exile => state.exile.push(object),
        Zone::Command => state.command.push(object),
        Zone::Stack => panic!("test helper does not fabricate stack-zone objects"),
    }
    object
}

/// Passes priority until `done` holds, failing loudly on a halt or on any
/// non-priority decision.
fn pass_until(state: &mut GameState, done: impl Fn(&GameState) -> bool) -> Decision {
    for _ in 0..32 {
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

/// Casts Journey to Nowhere with `creatures` (owner, name) on the
/// battlefield, none of which Journey's ETB trigger can legally target, and
/// checks that the game continues: CR 603.3d, a triggered ability with no
/// legal target is removed from the stack (here, never put on it), so
/// Journey enters, nothing is exiled, and when Journey later leaves nothing
/// returns.
fn journey_with_no_legal_target_continues(seed: u64, creatures: &[(PlayerId, &str)]) {
    let mut state = ready_game(seed);
    let creatures: Vec<ObjectId> = creatures
        .iter()
        .map(|&(owner, name)| put_object(&mut state, owner, name, Zone::Battlefield))
        .collect();
    let journey = put_object(&mut state, PlayerId::P0, "Journey to Nowhere", Zone::Hand);
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 2;

    let Decision::CastSpellOrPass {
        castable_spells, ..
    } = engine::advance_until_decision(&mut state)
    else {
        panic!("priority decision before casting Journey")
    };
    assert!(
        castable_spells.contains(&journey),
        "Journey has no cast-time target"
    );
    engine::step(&mut state, Action::CastSpell(journey)).unwrap();

    let decision = pass_until(&mut state, |state| {
        state.stack.is_empty() && state.objects.get(journey).zone == Zone::Battlefield
    });
    assert!(
        matches!(decision, Decision::CastSpellOrPass { .. }),
        "the game continues after Journey enters: {decision:?}"
    );
    assert!(state.engine.halted.is_none());
    assert!(state.engine.pending_triggers.is_empty());
    assert!(state.engine.linked_exile_records.is_empty());
    for &creature in &creatures {
        assert_eq!(state.objects.get(creature).zone, Zone::Battlefield);
    }
    engine::step(&mut state, Action::Pass).unwrap();
    assert!(!matches!(
        engine::advance_until_decision(&mut state),
        Decision::Halted { .. }
    ));

    let mut left = state.clone();
    event::propose_and_commit(
        &mut left,
        ProposedEvent::zone_change(journey, Zone::Graveyard),
    );
    let pending = trigger::collect_and_process(&mut left);
    left.engine.pending_triggers.extend(pending);
    pass_until(&mut left, |state| {
        state.stack.is_empty() && state.engine.pending_triggers.is_empty()
    });
    assert!(left.engine.halted.is_none());
    assert_eq!(left.objects.get(journey).zone, Zone::Graveyard);
    for &creature in &creatures {
        assert_eq!(left.objects.get(creature).zone, Zone::Battlefield);
    }
}

/// Arena heldout m0003p0014g0: the only creature was Guardian of the
/// Guildpact (protection from monocolored), which the white Journey's
/// trigger cannot target (CR 702.16b).
#[test]
fn journey_to_nowhere_facing_only_protection_from_monocolored_continues() {
    for owner in [PlayerId::P0, PlayerId::P1] {
        journey_with_no_legal_target_continues(
            0x4b53_4841_444f_570b,
            &[(owner, "Guardian of the Guildpact")],
        );
    }
}
