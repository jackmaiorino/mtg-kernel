//! MageZero Standard families E and F: planeswalkers, transforming legends,
//! big spells and non-creature permanent frameworks.
#![cfg(feature = "standard-magezero-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardType, Keywords, CARD_DEFS};
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

// ---- Transforming legends ----------------------------------------------

fn untapped(state: &mut GameState, object: ObjectId) {
    state.objects.get_mut(object).tapped = false;
}

/// From P0's precombat main phase, attacks with `attackers` (each attacking
/// the player), passes on blocks, and stops after combat damage.
fn attack_with(state: &mut GameState, attackers: Vec<ObjectId>) {
    loop {
        match next(state) {
            Decision::DeclareAttackers { .. } => {
                act(state, Action::DeclareAttackers(attackers.clone()))
            }
            Decision::ChooseAttackTarget { player, .. } => {
                act(state, Action::ChooseAttackTarget(Target::Player(player.opponent())))
            }
            Decision::DeclareBlockers { .. } => act(state, Action::DeclareBlockers(Vec::new())),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::CastSpellOrPass { .. }
                if matches!(state.step, Step::EndCombat | Step::Main2) && state.stack.is_empty() =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
}

#[test]
fn cecil_loses_life_equal_to_the_damage_it_deals() {
    let mut state = game();
    let cecil = put(&mut state, P0, "Cecil, Dark Knight", Zone::Battlefield);
    assert!(engine::has_effective_keyword(&state, cecil, Keywords::DEATHTOUCH));
    attack_with(&mut state, vec![cecil]);
    assert_eq!(state.players[1].life, 18);
    assert_eq!(state.players[0].life, 18, "Darkness: you lose that much life");
    assert_eq!(state.objects.get(cecil).v4.face_index, 0);
    assert!(state.objects.get(cecil).tapped);

}

#[test]
fn cecil_untaps_and_transforms_at_half_life() {
    let mut state = game();
    let cecil = put(&mut state, P0, "Cecil, Dark Knight", Zone::Battlefield);
    state.players[0].life = 12;
    attack_with(&mut state, vec![cecil]);
    assert_eq!(state.players[0].life, 10);
    let paladin = state.objects.get(cecil);
    assert_eq!(paladin.v4.face_index, 1);
    assert_eq!(paladin.name, "Cecil, Redeemed Paladin");
    assert!(!paladin.tapped, "Cecil untaps as it transforms");
    assert_eq!(engine::effective_power(&state, cecil), 4);
    assert!(engine::has_effective_keyword(&state, cecil, Keywords::LIFELINK));
    assert!(!engine::has_effective_keyword(&state, cecil, Keywords::DEATHTOUCH));
}

#[test]
fn cecil_redeemed_paladin_protects_other_attackers() {
    let mut state = game();
    let cecil = put(&mut state, P0, "Cecil, Dark Knight", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    event::propose_and_commit(&mut state, ProposedEvent::transform_in_place(cecil, 1));
    loop {
        match next(&mut state) {
            Decision::DeclareAttackers { .. } => {
                act(&mut state, Action::DeclareAttackers(vec![cecil, elves]))
            }
            Decision::CastSpellOrPass { .. } if state.step == Step::DeclareAttackers => {
                if state.stack.is_empty() {
                    break;
                }
                act(&mut state, Action::Pass)
            }
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert!(engine::has_effective_keyword(&state, elves, Keywords::INDESTRUCTIBLE));
    assert!(!engine::has_effective_keyword(&state, cecil, Keywords::INDESTRUCTIBLE));
    // Lifelink on the back face: four damage gains four life.
    attack_with(&mut state, Vec::new());
    assert_eq!(state.players[0].life, 24);
    assert_eq!(state.players[1].life, 15);
}

#[test]
fn polukranos_transforms_at_sorcery_speed_with_phyrexian_mana() {
    let mut state = game();
    let polukranos = put(&mut state, P0, "Polukranos Reborn", Zone::Battlefield);
    state.players[0].mana_pool[ManaColor::G.pool_index()] = 6;
    // {W/P} paid with 2 life when no white mana is available.
    assert!(activatable(&mut state).contains(&(polukranos, 0)));
    act(&mut state, Action::ActivateAbility(polukranos, 0));
    resolve_stack(&mut state);
    assert_eq!(state.players[0].life, 18);
    let engine_of_ruin = state.objects.get(polukranos);
    assert_eq!(engine_of_ruin.v4.face_index, 1);
    assert_eq!(engine_of_ruin.name, "Polukranos, Engine of Ruin");
    assert_eq!(engine::effective_power(&state, polukranos), 6);
    assert!(engine::has_effective_keyword(&state, polukranos, Keywords::LIFELINK));
    // The front face's transform ability is gone.
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 7;
    assert!(!activatable(&mut state).contains(&(polukranos, 0)));
}

fn hydra_tokens(state: &GameState) -> Vec<ObjectId> {
    battlefield_named(state, P0, "Phyrexian Hydra")
}

#[test]
fn polukranos_engine_of_ruin_makes_hydras_when_hydras_die() {
    let mut state = game();
    let polukranos = put(&mut state, P0, "Polukranos Reborn", Zone::Battlefield);
    let other = put(&mut state, P0, "Polukranos Reborn", Zone::Battlefield);
    event::propose_and_commit(&mut state, ProposedEvent::transform_in_place(polukranos, 1));
    // Another nontoken Hydra dies (the legend rule is not exercised here).
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(other, Zone::Graveyard));
    resolve_stack(&mut state);
    let tokens = hydra_tokens(&state);
    assert_eq!(tokens.len(), 2);
    assert!(engine::has_effective_keyword(&state, tokens[0], Keywords::REACH));
    assert!(engine::has_effective_keyword(&state, tokens[1], Keywords::LIFELINK));
    // A token Hydra dying does not trigger it.
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(tokens[0], Zone::Graveyard));
    resolve_stack(&mut state);
    assert_eq!(hydra_tokens(&state).len(), 1);
    // Polukranos itself dying does, from the back face it left on.
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(polukranos, Zone::Graveyard));
    resolve_stack(&mut state);
    assert_eq!(hydra_tokens(&state).len(), 3);
}

#[test]
fn polukranos_reborn_front_face_dying_makes_nothing() {
    let mut state = game();
    let polukranos = put(&mut state, P0, "Polukranos Reborn", Zone::Battlefield);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(polukranos, Zone::Graveyard));
    resolve_stack(&mut state);
    assert!(hydra_tokens(&state).is_empty());
}

fn cast_burst_at_opponent(state: &mut GameState) {
    let burst = put(state, P0, "Burst Lightning", Zone::Hand);
    state.players[0].mana_pool[ManaColor::R.pool_index()] += 1;
    next(state);
    act(state, Action::CastSpell(burst));
    loop {
        match next(state) {
            Decision::ChooseTargets { .. } => act(state, Action::ChooseTarget(Target::Player(P1))),
            Decision::ChooseKicker { .. } => act(state, Action::ChooseKicker(false)),
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
}

#[test]
fn ojer_axonil_raises_red_noncombat_damage_to_its_power() {
    let mut state = game();
    put(&mut state, P0, "Ojer Axonil, Deepest Might", Zone::Battlefield);
    cast_burst_at_opponent(&mut state);
    assert_eq!(state.players[1].life, 16, "two damage became four");
}

#[test]
fn ojer_axonil_returns_as_temple_of_power_and_transforms_back() {
    let mut state = game();
    let ojer = put(&mut state, P0, "Ojer Axonil, Deepest Might", Zone::Battlefield);
    // A creature, so its {T}: Add {R} is not on this face.
    assert!(!matches!(next(&mut state), Decision::CastSpellOrPass { ref mana_abilities, .. } if mana_abilities.contains(&ojer)));
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(ojer, Zone::Graveyard));
    resolve_stack(&mut state);
    let temple = state.objects.get(ojer);
    assert_eq!(temple.zone, Zone::Battlefield);
    assert_eq!(temple.v4.face_index, 1);
    assert_eq!(temple.name, "Temple of Power");
    assert!(temple.tapped);
    assert!(engine::object_has_type(&state, ojer, CardType::Land));
    assert!(!engine::object_has_type(&state, ojer, CardType::Creature));

    // Untapped, the Temple taps for {R} even though it entered this turn.
    untapped(&mut state, ojer);
    match next(&mut state) {
        Decision::CastSpellOrPass { mana_abilities, .. } => assert!(mana_abilities.contains(&ojer)),
        other => panic!("unexpected decision: {other:?}"),
    }
    // Its transform ability needs four noncombat damage from red sources.
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 3;
    assert!(!activatable(&mut state).contains(&(ojer, 0)));
    cast_burst_at_opponent(&mut state);
    assert_eq!(state.players[1].life, 18, "no Ojer on the battlefield to raise it");
    assert!(!activatable(&mut state).contains(&(ojer, 0)));
    cast_burst_at_opponent(&mut state);
    assert!(activatable(&mut state).contains(&(ojer, 0)));
    act(&mut state, Action::ActivateAbility(ojer, 0));
    resolve_stack(&mut state);
    let ojer_again = state.objects.get(ojer);
    assert_eq!(ojer_again.v4.face_index, 0);
    assert_eq!(ojer_again.name, "Ojer Axonil, Deepest Might");
    assert!(engine::object_has_type(&state, ojer, CardType::Creature));
    assert_eq!(engine::effective_power(&state, ojer), 4);
}

#[test]
fn temple_of_power_cannot_attack() {
    let mut state = game();
    let ojer = put(&mut state, P0, "Ojer Axonil, Deepest Might", Zone::Battlefield);
    event::propose_and_commit(&mut state, ProposedEvent::transform_in_place(ojer, 1));
    loop {
        match next(&mut state) {
            Decision::DeclareAttackers { eligible, .. } => {
                assert!(!eligible.contains(&ojer));
                break;
            }
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
}
