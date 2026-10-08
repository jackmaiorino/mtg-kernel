//! MageZero Standard families E and F: planeswalkers, transforming legends,
//! big spells and non-creature permanent frameworks.
#![cfg(feature = "standard-magezero-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::planeswalker_v1::{change_loyalty, loyalty};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

fn game() -> GameState {
    let island = card_id_by_name("Island").unwrap();
    let mut state = GameState::new_from_libraries(
        &[island; 20],
        &[island; 20],
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x4546_7631,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    state
}

/// Creates `name` in `zone`; battlefield arrivals go through a real zone
/// change so entering-the-battlefield state (loyalty) is initialized.
fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap_or_else(|| panic!("{name}"));
    let object = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner: player,
        controller: player,
        zone: Zone::Hand,
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
    state.players[player.index()].hand.push(object);
    if zone == Zone::Battlefield {
        event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Battlefield));
        state.objects.get_mut(object).summoning_sick = false;
    } else {
        assert_eq!(zone, Zone::Hand);
    }
    object
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn act(state: &mut GameState, action: Action) {
    engine::step(state, action.clone()).unwrap_or_else(|err| panic!("{action:?}: {err}"));
}

/// Passes priority until the stack is empty and the active player has
/// priority again in the same step.
fn resolve_stack(state: &mut GameState) {
    loop {
        match next(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
}

fn activatable(state: &mut GameState) -> Vec<(ObjectId, u8)> {
    match next(state) {
        Decision::CastSpellOrPass {
            activatable_abilities,
            ..
        } => activatable_abilities,
        other => panic!("unexpected decision: {other:?}"),
    }
}

fn battlefield_named(state: &GameState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| state.objects.get(id).name == name)
        .collect()
}

// ---- Teferi, Temporal Pilgrim -------------------------------------------

#[test]
fn teferi_enters_with_four_loyalty_and_draws_add_loyalty() {
    let mut state = game();
    let teferi = put(&mut state, P0, "Teferi, Temporal Pilgrim", Zone::Hand);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 5;
    next(&mut state);
    act(&mut state, Action::CastSpell(teferi));
    resolve_stack(&mut state);
    assert_eq!(state.objects.get(teferi).zone, Zone::Battlefield);
    assert_eq!(loyalty(&state, teferi), Some(4));

    // 0: Draw a card. The draw trigger then adds a loyalty counter.
    let hand = state.players[0].hand.len();
    assert!(activatable(&mut state).contains(&(teferi, 0)));
    act(&mut state, Action::ActivateAbility(teferi, 0));
    resolve_stack(&mut state);
    assert_eq!(state.players[0].hand.len(), hand + 1);
    assert_eq!(loyalty(&state, teferi), Some(5));

    // 606.3: one loyalty ability per planeswalker per turn.
    assert!(activatable(&mut state)
        .iter()
        .all(|&(source, _)| source != teferi));
}

#[test]
fn teferi_minus_two_makes_a_spirit_that_grows_on_draws() {
    let mut state = game();
    let teferi = put(&mut state, P0, "Teferi, Temporal Pilgrim", Zone::Battlefield);
    assert_eq!(loyalty(&state, teferi), Some(4));
    let abilities = activatable(&mut state);
    assert!(abilities.contains(&(teferi, 1)));
    // -12 needs twelve loyalty counters.
    assert!(!abilities.contains(&(teferi, 2)));
    act(&mut state, Action::ActivateAbility(teferi, 1));
    next(&mut state);
    assert_eq!(state.stack.len(), 1);
    assert_eq!(loyalty(&state, teferi), Some(2), "loyalty is paid on activation");
    resolve_stack(&mut state);
    let spirits = battlefield_named(&state, P0, "Spirit");
    assert_eq!(spirits.len(), 1);
    let spirit = spirits[0];
    assert_eq!(engine::effective_power(&state, spirit), 2);

    // A later turn's 0 ability draws: Teferi gains loyalty and the Spirit
    // gets a +1/+1 counter.
    state.objects.get_mut(teferi).v4.ability_uses_this_turn.clear();
    act(&mut state, Action::ActivateAbility(teferi, 0));
    resolve_stack(&mut state);
    assert_eq!(loyalty(&state, teferi), Some(3));
    assert_eq!(state.objects.get(spirit).counters.plus1_plus1, 1);
    assert_eq!(engine::effective_power(&state, spirit), 3);
}

#[test]
fn teferi_ultimate_bounces_one_and_shuffles_the_rest() {
    let mut state = game();
    let teferi = put(&mut state, P0, "Teferi, Temporal Pilgrim", Zone::Battlefield);
    change_loyalty(&mut state, teferi, 8);
    assert_eq!(loyalty(&state, teferi), Some(12));
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let terror = put(&mut state, P1, "Tolarian Terror", Zone::Battlefield);
    let land = put(&mut state, P1, "Forest", Zone::Battlefield);
    let library = state.players[1].library.len();
    assert!(activatable(&mut state).contains(&(teferi, 2)));
    act(&mut state, Action::ActivateAbility(teferi, 2));
    loop {
        match next(&mut state) {
            Decision::ChooseTargets { .. } => {
                act(&mut state, Action::ChooseTarget(Target::Player(P1)))
            }
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            Decision::ChooseEffectTargets {
                player,
                legal_targets,
                ..
            } => {
                assert_eq!(player, P1, "the opponent chooses");
                assert!(legal_targets.contains(&Target::Object(terror)));
                assert!(legal_targets.contains(&Target::Object(land)));
                act(&mut state, Action::ChooseEffectTarget(Target::Object(terror)));
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    // Teferi hit zero loyalty and died.
    assert_eq!(state.objects.get(teferi).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(terror).zone, Zone::Hand);
    assert_eq!(state.objects.get(elves).zone, Zone::Library);
    assert_eq!(state.objects.get(land).zone, Zone::Battlefield);
    assert_eq!(state.players[1].library.len(), library + 1);
}

// ---- Attacking planeswalkers -------------------------------------------

/// Moves from P1's precombat main phase to its declare-attackers decision.
fn to_declare_attackers(state: &mut GameState) -> Vec<ObjectId> {
    loop {
        match next(state) {
            Decision::DeclareAttackers { eligible, .. } => return eligible,
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
}

fn attacking_game() -> (GameState, ObjectId, ObjectId) {
    let mut state = game();
    state.active_player = P1;
    state.priority_player = P1;
    let teferi = put(&mut state, P0, "Teferi, Temporal Pilgrim", Zone::Battlefield);
    let terror = put(&mut state, P1, "Tolarian Terror", Zone::Battlefield);
    assert_eq!(to_declare_attackers(&mut state), vec![terror]);
    act(&mut state, Action::DeclareAttackers(vec![terror]));
    (state, teferi, terror)
}

/// Passes through blocks (none) to the end of combat damage.
fn finish_combat(state: &mut GameState) {
    loop {
        match next(state) {
            Decision::DeclareBlockers { .. } => act(state, Action::DeclareBlockers(Vec::new())),
            Decision::CastSpellOrPass { .. }
                if matches!(state.step, Step::EndCombat | Step::Main2) =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
}

#[test]
fn creatures_can_attack_a_planeswalker() {
    let (mut state, teferi, terror) = attacking_game();
    match next(&mut state) {
        Decision::ChooseAttackTarget {
            player,
            attacker,
            candidates,
        } => {
            assert_eq!(player, P1);
            assert_eq!(attacker, terror);
            assert_eq!(
                candidates,
                vec![Target::Player(P0), Target::Object(teferi)]
            );
        }
        other => panic!("unexpected decision: {other:?}"),
    }
    assert!(!state.engine.combat.attackers_declared);
    assert!(engine::step(&mut state, Action::Pass).is_err());
    assert!(engine::step(&mut state, Action::ChooseAttackTarget(Target::Player(P1))).is_err());
    act(&mut state, Action::ChooseAttackTarget(Target::Object(teferi)));
    assert!(state.engine.combat.attackers_declared);
    assert!(state.objects.get(terror).tapped);
    finish_combat(&mut state);
    // Five damage removes four loyalty; Teferi dies and the player is untouched.
    assert_eq!(state.players[0].life, 20);
    assert_eq!(state.objects.get(teferi).zone, Zone::Graveyard);
}

#[test]
fn attacking_the_player_is_still_offered() {
    let (mut state, teferi, _) = attacking_game();
    act(&mut state, Action::ChooseAttackTarget(Target::Player(P0)));
    finish_combat(&mut state);
    assert_eq!(state.players[0].life, 15);
    assert_eq!(loyalty(&state, teferi), Some(4));
}

#[test]
fn an_attacked_planeswalker_that_leaves_takes_no_damage_and_redirects_none() {
    let (mut state, teferi, _) = attacking_game();
    act(&mut state, Action::ChooseAttackTarget(Target::Object(teferi)));
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(teferi, Zone::Hand));
    finish_combat(&mut state);
    assert_eq!(state.players[0].life, 20);
}

#[test]
fn burn_can_target_a_planeswalker() {
    let mut state = game();
    let teferi = put(&mut state, P1, "Teferi, Temporal Pilgrim", Zone::Battlefield);
    let burst = put(&mut state, P0, "Burst Lightning", Zone::Hand);
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
    next(&mut state);
    act(&mut state, Action::CastSpell(burst));
    loop {
        match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(teferi)));
                act(&mut state, Action::ChooseTarget(Target::Object(teferi)));
            }
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert_eq!(loyalty(&state, teferi), Some(2));
}
