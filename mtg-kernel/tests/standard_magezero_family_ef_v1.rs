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
    let teferi = put(
        &mut state,
        P0,
        "Teferi, Temporal Pilgrim",
        Zone::Battlefield,
    );
    assert_eq!(loyalty(&state, teferi), Some(4));
    let abilities = activatable(&mut state);
    assert!(abilities.contains(&(teferi, 1)));
    // -12 needs twelve loyalty counters.
    assert!(!abilities.contains(&(teferi, 2)));
    act(&mut state, Action::ActivateAbility(teferi, 1));
    next(&mut state);
    assert_eq!(state.stack.len(), 1);
    assert_eq!(
        loyalty(&state, teferi),
        Some(2),
        "loyalty is paid on activation"
    );
    resolve_stack(&mut state);
    let spirits = battlefield_named(&state, P0, "Spirit");
    assert_eq!(spirits.len(), 1);
    let spirit = spirits[0];
    assert_eq!(engine::effective_power(&state, spirit), 2);

    // A later turn's 0 ability draws: Teferi gains loyalty and the Spirit
    // gets a +1/+1 counter.
    state
        .objects
        .get_mut(teferi)
        .v4
        .ability_uses_this_turn
        .clear();
    act(&mut state, Action::ActivateAbility(teferi, 0));
    resolve_stack(&mut state);
    assert_eq!(loyalty(&state, teferi), Some(3));
    assert_eq!(state.objects.get(spirit).counters.plus1_plus1, 1);
    assert_eq!(engine::effective_power(&state, spirit), 3);
}

#[test]
fn teferi_ultimate_bounces_one_and_shuffles_the_rest() {
    let mut state = game();
    let teferi = put(
        &mut state,
        P0,
        "Teferi, Temporal Pilgrim",
        Zone::Battlefield,
    );
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
                act(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(terror)),
                );
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
    let teferi = put(
        &mut state,
        P0,
        "Teferi, Temporal Pilgrim",
        Zone::Battlefield,
    );
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
            assert_eq!(candidates, vec![Target::Player(P0), Target::Object(teferi)]);
        }
        other => panic!("unexpected decision: {other:?}"),
    }
    assert!(!state.engine.combat.attackers_declared);
    assert!(engine::step(&mut state, Action::Pass).is_err());
    assert!(engine::step(&mut state, Action::ChooseAttackTarget(Target::Player(P1))).is_err());
    act(
        &mut state,
        Action::ChooseAttackTarget(Target::Object(teferi)),
    );
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
    act(
        &mut state,
        Action::ChooseAttackTarget(Target::Object(teferi)),
    );
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(teferi, Zone::Hand));
    finish_combat(&mut state);
    assert_eq!(state.players[0].life, 20);
}

#[test]
fn burn_can_target_a_planeswalker() {
    let mut state = game();
    let teferi = put(
        &mut state,
        P1,
        "Teferi, Temporal Pilgrim",
        Zone::Battlefield,
    );
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
            Decision::ChooseAttackTarget { player, .. } => act(
                state,
                Action::ChooseAttackTarget(Target::Player(player.opponent())),
            ),
            Decision::DeclareBlockers { .. } => act(state, Action::DeclareBlockers(Vec::new())),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::CastSpellOrPass { .. }
                if matches!(state.step, Step::EndCombat | Step::Main2)
                    && state.stack.is_empty() =>
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
    assert!(engine::has_effective_keyword(
        &state,
        cecil,
        Keywords::DEATHTOUCH
    ));
    attack_with(&mut state, vec![cecil]);
    assert_eq!(state.players[1].life, 18);
    assert_eq!(
        state.players[0].life, 18,
        "Darkness: you lose that much life"
    );
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
    assert!(engine::has_effective_keyword(
        &state,
        cecil,
        Keywords::LIFELINK
    ));
    assert!(!engine::has_effective_keyword(
        &state,
        cecil,
        Keywords::DEATHTOUCH
    ));
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
    assert!(engine::has_effective_keyword(
        &state,
        elves,
        Keywords::INDESTRUCTIBLE
    ));
    assert!(!engine::has_effective_keyword(
        &state,
        cecil,
        Keywords::INDESTRUCTIBLE
    ));
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
    assert!(engine::has_effective_keyword(
        &state,
        polukranos,
        Keywords::LIFELINK
    ));
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
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(other, Zone::Graveyard),
    );
    resolve_stack(&mut state);
    let tokens = hydra_tokens(&state);
    assert_eq!(tokens.len(), 2);
    assert!(engine::has_effective_keyword(
        &state,
        tokens[0],
        Keywords::REACH
    ));
    assert!(engine::has_effective_keyword(
        &state,
        tokens[1],
        Keywords::LIFELINK
    ));
    // A token Hydra dying does not trigger it.
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(tokens[0], Zone::Graveyard),
    );
    resolve_stack(&mut state);
    assert_eq!(hydra_tokens(&state).len(), 1);
    // Polukranos itself dying does, from the back face it left on.
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(polukranos, Zone::Graveyard),
    );
    resolve_stack(&mut state);
    assert_eq!(hydra_tokens(&state).len(), 3);
}

#[test]
fn polukranos_reborn_front_face_dying_makes_nothing() {
    let mut state = game();
    let polukranos = put(&mut state, P0, "Polukranos Reborn", Zone::Battlefield);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(polukranos, Zone::Graveyard),
    );
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
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
}

#[test]
fn ojer_axonil_raises_red_noncombat_damage_to_its_power() {
    let mut state = game();
    put(
        &mut state,
        P0,
        "Ojer Axonil, Deepest Might",
        Zone::Battlefield,
    );
    cast_burst_at_opponent(&mut state);
    assert_eq!(state.players[1].life, 16, "two damage became four");
}

#[test]
fn ojer_axonil_returns_as_temple_of_power_and_transforms_back() {
    let mut state = game();
    let ojer = put(
        &mut state,
        P0,
        "Ojer Axonil, Deepest Might",
        Zone::Battlefield,
    );
    // A creature, so its {T}: Add {R} is not on this face.
    assert!(
        !matches!(next(&mut state), Decision::CastSpellOrPass { ref mana_abilities, .. } if mana_abilities.contains(&ojer))
    );
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(ojer, Zone::Graveyard),
    );
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
    assert_eq!(
        state.players[1].life, 18,
        "no Ojer on the battlefield to raise it"
    );
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
    let ojer = put(
        &mut state,
        P0,
        "Ojer Axonil, Deepest Might",
        Zone::Battlefield,
    );
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

// ---- Blue Sun's Twilight -----------------------------------------------

/// Casts Blue Sun's Twilight at `target`, announcing `x`.
fn cast_twilight(state: &mut GameState, target: ObjectId, x: u8) {
    let twilight = put(state, P0, "Blue Sun's Twilight", Zone::Hand);
    next(state);
    act(state, Action::CastSpell(twilight));
    loop {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(target)));
                act(state, Action::ChooseTarget(Target::Object(target)));
            }
            Decision::ChooseEffectOption { option_count, .. } => {
                // Options start at the target's mana value.
                let minimum = CARD_DEFS[state.objects.get(target).card_def as usize].mana_value;
                let option = u16::from(x) - minimum;
                assert!(option < option_count);
                act(state, Action::ChooseEffectOption(option));
            }
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
}

#[test]
fn blue_suns_twilight_steals_a_creature_with_mana_value_up_to_x() {
    let mut state = game();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 4;
    cast_twilight(&mut state, elves, 2);
    let stolen = state.objects.get(elves);
    assert_eq!(stolen.controller, P0);
    assert!(state.players[0].battlefield.contains(&elves));
    assert!(!state.players[1].battlefield.contains(&elves));
    assert!(stolen.summoning_sick);
    assert!(
        battlefield_named(&state, P0, "Llanowar Elves").len() == 1,
        "X below 5 makes no copy"
    );
}

#[test]
fn blue_suns_twilight_copies_at_x_five_or_more() {
    let mut state = game();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 7;
    cast_twilight(&mut state, elves, 5);
    assert_eq!(state.objects.get(elves).controller, P0);
    let all_elves = battlefield_named(&state, P0, "Llanowar Elves");
    assert_eq!(all_elves.len(), 2);
    let copy = *all_elves.iter().find(|&&id| id != elves).unwrap();
    assert!(state.objects.get(copy).v4.is_token);
    // The copy is a token: leaving the battlefield, it ceases to exist.
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(copy, Zone::Graveyard),
    );
    resolve_stack(&mut state);
    assert!(!state.players[0].graveyard.contains(&copy));
}

#[test]
fn blue_suns_twilight_needs_a_creature_it_can_afford() {
    let mut state = game();
    let terror = put(&mut state, P1, "Tolarian Terror", Zone::Battlefield);
    let twilight = put(&mut state, P0, "Blue Sun's Twilight", Zone::Hand);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 4;
    match next(&mut state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => assert!(!castable_spells.contains(&twilight)),
        other => panic!("unexpected decision: {other:?}"),
    }
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::CastSpell(twilight));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(elves)));
            assert!(!legal_targets.contains(&Target::Object(terror)));
        }
        other => panic!("unexpected decision: {other:?}"),
    }
}

// ---- Rooms: Unholy Annex // Ritual Chamber ---------------------------------

const ROOM: &str = "Unholy Annex // Ritual Chamber";

/// Casts the Room's left half (normal cost) or right half (alternative
/// cost) from a pool of black mana and resolves it.
fn cast_room(state: &mut GameState, right_half: bool) -> ObjectId {
    let room = put(state, P0, ROOM, Zone::Hand);
    state.players[0].mana_pool[ManaColor::B.pool_index()] = 5;
    next(state);
    act(state, Action::CastSpell(room));
    loop {
        match next(state) {
            Decision::ChooseCastMode { options, .. } => {
                let mode = if right_half {
                    engine::CastMode::Alternative
                } else {
                    engine::CastMode::Normal
                };
                assert!(options.contains(&mode));
                act(state, Action::ChooseCastMode(mode));
            }
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    state.players[0].mana_pool = Default::default();
    room
}

fn demons(state: &GameState) -> Vec<ObjectId> {
    battlefield_named(state, P0, "Demon")
}

/// Passes to P0's end step and resolves whatever triggers there.
fn through_end_step(state: &mut GameState) {
    loop {
        let decision = next(state);
        if state.step == Step::End {
            break;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::DeclareAttackers { .. } => act(state, Action::DeclareAttackers(Vec::new())),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    resolve_stack(state);
}

#[test]
fn casting_unholy_annex_unlocks_only_its_door() {
    let mut state = game();
    let room = cast_room(&mut state, false);
    assert_eq!(state.objects.get(room).zone, Zone::Battlefield);
    assert!(demons(&state).is_empty());
    let abilities = activatable(&mut state);
    assert!(
        !abilities.contains(&(room, 0)),
        "the left door is already unlocked"
    );
    // Ritual Chamber's door costs {3}{B}{B}; nothing to pay it with yet.
    assert!(!abilities.contains(&(room, 1)));

    // No Demon: draw a card and lose 2 life.
    let hand = state.players[0].hand.len();
    through_end_step(&mut state);
    assert_eq!(state.players[0].hand.len(), hand + 1);
    assert_eq!(state.players[0].life, 18);
    assert_eq!(state.players[1].life, 20);
}

#[test]
fn casting_ritual_chamber_unlocks_its_door_and_makes_a_demon() {
    let mut state = game();
    let room = cast_room(&mut state, true);
    let demon = demons(&state);
    assert_eq!(demon.len(), 1);
    let demon = demon[0];
    assert_eq!(engine::effective_power(&state, demon), 6);
    assert!(engine::has_effective_keyword(
        &state,
        demon,
        Keywords::FLYING
    ));
    // The Annex's door is still locked, so its end-step ability does nothing.
    let hand = state.players[0].hand.len();
    through_end_step(&mut state);
    assert_eq!(state.players[0].hand.len(), hand);
    assert_eq!(state.players[0].life, 20);
    assert_eq!(state.objects.get(room).zone, Zone::Battlefield);
}

#[test]
fn unlocking_ritual_chamber_is_a_special_action_that_triggers() {
    let mut state = game();
    let room = cast_room(&mut state, false);
    state.players[0].mana_pool[ManaColor::B.pool_index()] = 5;
    assert!(activatable(&mut state).contains(&(room, 1)));
    act(&mut state, Action::ActivateAbility(room, 1));
    // The door unlocks at once; only its triggered ability uses the stack.
    loop {
        match next(&mut state) {
            Decision::CastSpellOrPass { .. } => break,
            Decision::OrderTriggers { pending, .. } => act(
                &mut state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert_eq!(state.stack.len(), 1);
    assert!(state
        .stack
        .iter()
        .all(|item| item.kind != mtg_kernel::state::StackItemKind::ActivatedAbility));
    resolve_stack(&mut state);
    assert_eq!(demons(&state).len(), 1);
    let abilities = activatable(&mut state);
    assert!(!abilities.contains(&(room, 0)) && !abilities.contains(&(room, 1)));

    // With a Demon: draw, the opponent loses 2 and P0 gains 2.
    let hand = state.players[0].hand.len();
    through_end_step(&mut state);
    assert_eq!(state.players[0].hand.len(), hand + 1);
    assert_eq!(state.players[0].life, 22);
    assert_eq!(state.players[1].life, 18);
}

#[test]
fn a_room_put_onto_the_battlefield_has_both_doors_locked() {
    let mut state = game();
    let room = put(&mut state, P0, ROOM, Zone::Battlefield);
    state.players[0].mana_pool[ManaColor::B.pool_index()] = 5;
    let abilities = activatable(&mut state);
    assert!(abilities.contains(&(room, 0)) && abilities.contains(&(room, 1)));
    let hand = state.players[0].hand.len();
    state.players[0].mana_pool = Default::default();
    through_end_step(&mut state);
    assert_eq!(state.players[0].hand.len(), hand);
    assert_eq!(state.players[0].life, 20);
}

// ---- Exile until this leaves, Auras and Equipment (GW) ----------------------

/// Answers decisions until the stack is empty and the active player has
/// priority, taking targets and cost choices from `picks` in order.
fn drive(state: &mut GameState, picks: &[Target]) {
    let mut picks = picks.iter().copied();
    loop {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                let pick = picks.next().expect("a target to choose");
                assert!(
                    legal_targets.contains(&pick),
                    "{pick:?} not in {legal_targets:?}"
                );
                act(state, Action::ChooseTarget(pick));
            }
            Decision::ChooseCostTargets { candidates, .. } => {
                let Some(Target::Object(pick)) = picks.next() else {
                    panic!("a cost object to choose")
                };
                assert!(candidates.contains(&pick));
                act(state, Action::ChooseCostTarget(pick));
            }
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
}

fn cast(state: &mut GameState, name: &str, white: u8, picks: &[Target]) -> ObjectId {
    let card = put(state, P0, name, Zone::Hand);
    state.players[0].mana_pool[ManaColor::W.pool_index()] = white;
    next(state);
    act(state, Action::CastSpell(card));
    drive(state, picks);
    state.players[0].mana_pool = Default::default();
    card
}

/// The legal targets offered for the first target of `name`'s ETB trigger
/// once it is cast, without choosing one.
fn etb_trigger_targets(
    state: &mut GameState,
    name: &str,
    white: u8,
    picks: &[Target],
) -> Vec<Target> {
    let card = put(state, P0, name, Zone::Hand);
    state.players[0].mana_pool[ManaColor::W.pool_index()] = white;
    next(state);
    act(state, Action::CastSpell(card));
    let mut picks = picks.iter().copied();
    loop {
        match next(state) {
            Decision::ChooseTargets {
                spell,
                legal_targets,
                ..
            } if spell == card && state.objects.get(card).zone == Zone::Battlefield => {
                return legal_targets
            }
            Decision::ChooseTargets { .. } => {
                act(state, Action::ChooseTarget(picks.next().unwrap()))
            }
            Decision::ChooseCostTargets { .. } => {
                let Some(Target::Object(pick)) = picks.next() else {
                    panic!()
                };
                act(state, Action::ChooseCostTarget(pick));
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
}

#[test]
fn seam_rip_exiles_a_cheap_nonland_permanent_until_it_leaves() {
    let mut state = game();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let terror = put(&mut state, P1, "Tolarian Terror", Zone::Battlefield);
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let legal = etb_trigger_targets(&mut state.clone(), "Seam Rip", 1, &[]);
    assert!(legal.contains(&Target::Object(elves)));
    for excluded in [terror, island, mine] {
        assert!(!legal.contains(&Target::Object(excluded)), "{excluded:?}");
    }

    let seam_rip = cast(&mut state, "Seam Rip", 1, &[Target::Object(elves)]);
    assert_eq!(state.objects.get(elves).zone, Zone::Exile);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(seam_rip, Zone::Graveyard),
    );
    drive(&mut state, &[]);
    assert_eq!(state.objects.get(elves).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(elves).controller, P1);
}

#[test]
fn dusk_rose_reliquary_sacrifices_for_its_cost_and_exiles_an_artifact_or_creature() {
    let mut state = game();
    let fodder = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let terror = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let reliquary = cast(
        &mut state,
        "Dusk Rose Reliquary",
        1,
        // The only artifact or creature P0 controls pays the sacrifice
        // without a choice.
        &[Target::Object(terror)],
    );
    assert_eq!(state.objects.get(fodder).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(reliquary).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(terror).zone, Zone::Exile);
    assert_eq!(
        CARD_DEFS[state.objects.get(reliquary).card_def as usize].ward_cost,
        Some(mtg_kernel::card_def::WardCostDef::Generic(2))
    );
}

#[test]
fn sheltered_by_ghosts_enchants_your_creature_and_exiles_until_it_leaves() {
    let mut state = game();
    let host = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let epicure = put(&mut state, P1, "Basilisk Collar", Zone::Battlefield);
    let aura = cast(
        &mut state,
        "Sheltered by Ghosts",
        2,
        &[Target::Object(host), Target::Object(epicure)],
    );
    assert_eq!(
        state
            .objects
            .get(aura)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(host)
    );
    assert_eq!(state.objects.get(epicure).zone, Zone::Exile);
    assert_eq!(engine::effective_power(&state, host), 2);
    assert!(engine::has_effective_keyword(
        &state,
        host,
        Keywords::LIFELINK
    ));

    // Ward {2}: an opponent's spell targeting the creature is countered
    // unless they pay {2}.
    state.active_player = P1;
    state.priority_player = P1;
    let burst = put(&mut state, P1, "Burst Lightning", Zone::Hand);
    state.players[1].mana_pool[ManaColor::R.pool_index()] = 1;
    next(&mut state);
    act(&mut state, Action::CastSpell(burst));
    loop {
        match next(&mut state) {
            Decision::ChooseTargets { .. } => {
                act(&mut state, Action::ChooseTarget(Target::Object(host)))
            }
            Decision::ChooseKicker { .. } => act(&mut state, Action::ChooseKicker(false)),
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert_eq!(state.objects.get(host).damage, 0, "countered by ward");
    assert_eq!(state.objects.get(host).zone, Zone::Battlefield);

    // The creature leaves; the Aura goes to the graveyard and the Epicure
    // returns.
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(host, Zone::Graveyard),
    );
    drive(&mut state, &[]);
    assert_eq!(state.objects.get(aura).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(epicure).zone, Zone::Battlefield);
}

#[test]
fn sheltered_by_ghosts_cannot_enchant_an_opponents_creature() {
    let mut state = game();
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let aura = put(&mut state, P0, "Sheltered by Ghosts", Zone::Hand);
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 2;
    next(&mut state);
    act(&mut state, Action::CastSpell(aura));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(mine)));
            assert!(!legal_targets.contains(&Target::Object(theirs)));
        }
        other => panic!("unexpected decision: {other:?}"),
    }
}

#[test]
fn hardlight_containment_enchants_your_artifact_and_exiles_a_creature() {
    let mut state = game();
    let collar = put(&mut state, P0, "Basilisk Collar", Zone::Battlefield);
    // Not Tolarian Terror: its own ward would counter the trigger.
    let terror = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let aura = cast(
        &mut state,
        "Hardlight Containment",
        1,
        &[Target::Object(collar), Target::Object(terror)],
    );
    assert_eq!(
        state
            .objects
            .get(aura)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(collar)
    );
    assert_eq!(state.objects.get(terror).zone, Zone::Exile);
    // The artifact leaves; the Aura follows and the Terror returns.
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(collar, Zone::Graveyard),
    );
    drive(&mut state, &[]);
    assert_eq!(state.objects.get(aura).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(terror).zone, Zone::Battlefield);
}

#[test]
fn basilisk_collar_equips_for_two_and_grants_deathtouch_and_lifelink() {
    let mut state = game();
    let collar = put(&mut state, P0, "Basilisk Collar", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 2;
    assert!(activatable(&mut state).contains(&(collar, 0)));
    act(&mut state, Action::ActivateAbility(collar, 0));
    drive(&mut state, &[Target::Object(elves)]);
    assert_eq!(
        state
            .objects
            .get(collar)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(elves)
    );
    assert!(engine::has_effective_keyword(
        &state,
        elves,
        Keywords::DEATHTOUCH
    ));
    assert!(engine::has_effective_keyword(
        &state,
        elves,
        Keywords::LIFELINK
    ));
}

// ---- Candy Trail, Warleader's Call, Lunar Convocation, Simulacrum, Cases ----

/// Resolves the stack like `resolve_stack`, keeping scried cards on top.
fn settle(state: &mut GameState) {
    loop {
        match next(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::ChooseEffectTargets {
                can_finish: true, ..
            } => act(state, Action::FinishEffectSelection),
            // Ordering the cards kept on top: take them in offered order.
            Decision::ChooseEffectTargets { legal_targets, .. } => {
                act(state, Action::ChooseEffectTarget(legal_targets[0]))
            }
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
}

/// Passes to the end step like `through_end_step`, settling any scry.
fn to_end_step(state: &mut GameState) {
    loop {
        let decision = next(state);
        if state.step == Step::End {
            break;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::DeclareAttackers { .. } => act(state, Action::DeclareAttackers(Vec::new())),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    settle(state);
}

#[test]
fn candy_trail_scries_then_sacrifices_for_life_and_a_card() {
    let mut state = game();
    let trail = put(&mut state, P0, "Candy Trail", Zone::Hand);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 1;
    next(&mut state);
    act(&mut state, Action::CastSpell(trail));
    settle(&mut state);
    assert_eq!(state.objects.get(trail).zone, Zone::Battlefield);

    // {2}, {T}, Sacrifice: gain 3 life and draw a card.
    assert!(
        !activatable(&mut state).contains(&(trail, 0)),
        "needs {{2}}"
    );
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 2;
    assert!(activatable(&mut state).contains(&(trail, 0)));
    let hand = state.players[0].hand.len();
    act(&mut state, Action::ActivateAbility(trail, 0));
    settle(&mut state);
    assert_eq!(state.objects.get(trail).zone, Zone::Graveyard);
    assert_eq!(state.players[0].life, 23);
    assert_eq!(state.players[0].hand.len(), hand + 1);
}

#[test]
fn warleaders_call_pumps_your_creatures_and_pings_on_entry() {
    let mut state = game();
    put(&mut state, P0, "Warleader's Call", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    settle(&mut state);
    assert_eq!(
        state.players[1].life, 19,
        "only your creature entering pings"
    );
    assert_eq!(
        (
            engine::effective_power(&state, elves),
            engine::effective_toughness(&state, elves)
        ),
        (2, 2)
    );
    assert_eq!(
        (
            engine::effective_power(&state, theirs),
            engine::effective_toughness(&state, theirs)
        ),
        (1, 1)
    );
}

fn lunar_end_step(gained: i32, lost: i32) -> GameState {
    let mut state = game();
    put(&mut state, P0, "Lunar Convocation", Zone::Battlefield);
    if gained > 0 {
        event::propose_and_commit(&mut state, ProposedEvent::life_gain(P0, gained));
    }
    if lost > 0 {
        event::propose_and_commit(&mut state, ProposedEvent::life_loss(P0, lost));
    }
    to_end_step(&mut state);
    state
}

#[test]
fn lunar_convocation_drains_after_you_gain_life() {
    let state = lunar_end_step(1, 0);
    assert_eq!(state.players[1].life, 19);
    assert!(battlefield_named(&state, P0, "Bat").is_empty());

    let state = lunar_end_step(0, 0);
    assert_eq!(state.players[1].life, 20);
    let state = lunar_end_step(0, 3);
    assert_eq!(state.players[1].life, 20);
    assert!(battlefield_named(&state, P0, "Bat").is_empty());
}

#[test]
fn lunar_convocation_makes_a_bat_after_you_gain_and_lose_life() {
    let state = lunar_end_step(2, 1);
    assert_eq!(state.players[1].life, 19);
    let bats = battlefield_named(&state, P0, "Bat");
    assert_eq!(bats.len(), 1);
    assert!(engine::has_effective_keyword(
        &state,
        bats[0],
        Keywords::FLYING
    ));
    assert_eq!(engine::effective_power(&state, bats[0]), 1);
}

#[test]
fn lunar_convocation_pays_two_life_to_draw_and_that_counts_as_losing_life() {
    let mut state = game();
    let convocation = put(&mut state, P0, "Lunar Convocation", Zone::Battlefield);
    state.players[0].mana_pool[ManaColor::B.pool_index()] = 2;
    let hand = state.players[0].hand.len();
    act(&mut state, Action::ActivateAbility(convocation, 0));
    settle(&mut state);
    assert_eq!(state.players[0].life, 18);
    assert_eq!(state.players[0].hand.len(), hand + 1);
    event::propose_and_commit(&mut state, ProposedEvent::life_gain(P0, 1));
    to_end_step(&mut state);
    assert_eq!(state.players[1].life, 19);
    assert_eq!(battlefield_named(&state, P0, "Bat").len(), 1);
}

#[test]
fn simulacrum_synthesizer_makes_constructs_for_big_artifacts() {
    let mut state = game();
    put(&mut state, P0, "Simulacrum Synthesizer", Zone::Battlefield);
    settle(&mut state);
    assert!(battlefield_named(&state, P0, "Construct").is_empty());

    // Mana value 1: no Construct.
    put(&mut state, P0, "Candy Trail", Zone::Battlefield);
    settle(&mut state);
    assert!(battlefield_named(&state, P0, "Construct").is_empty());

    // Another Synthesizer (mana value 3) makes one from the first.
    put(&mut state, P0, "Simulacrum Synthesizer", Zone::Battlefield);
    settle(&mut state);
    let constructs = battlefield_named(&state, P0, "Construct");
    assert_eq!(constructs.len(), 1);
    // Two Synthesizers, Candy Trail and the Construct itself.
    let construct = constructs[0];
    assert_eq!(engine::effective_power(&state, construct), 4);
    assert_eq!(engine::effective_toughness(&state, construct), 4);

    // An opponent's artifact entering makes nothing.
    put(&mut state, P1, "Simulacrum Synthesizer", Zone::Battlefield);
    settle(&mut state);
    assert_eq!(battlefield_named(&state, P0, "Construct").len(), 1);
}

#[test]
fn case_of_the_gateway_express_damages_with_each_creature_and_solves_after_three_attack() {
    let mut state = game();
    let mine = [
        put(&mut state, P0, "Llanowar Elves", Zone::Battlefield),
        put(&mut state, P0, "Llanowar Elves", Zone::Battlefield),
        put(&mut state, P0, "Llanowar Elves", Zone::Battlefield),
    ];
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let targets = etb_trigger_targets(&mut state, "Case of the Gateway Express", 2, &[]);
    assert_eq!(targets, vec![Target::Object(theirs)]);
    drive(&mut state, &[Target::Object(theirs)]);
    assert_eq!(state.objects.get(theirs).zone, Zone::Graveyard);

    // Unsolved: no anthem yet.
    assert_eq!(engine::effective_power(&state, mine[0]), 1);
    attack_with(&mut state, mine.to_vec());
    assert_eq!(state.players[1].life, 17);
    assert_eq!(engine::effective_power(&state, mine[0]), 1);
    to_end_step(&mut state);
    for creature in mine {
        assert_eq!(engine::effective_power(&state, creature), 2);
        assert_eq!(engine::effective_toughness(&state, creature), 1);
    }
}

#[test]
fn case_of_the_gateway_express_stays_unsolved_after_two_attackers() {
    let mut state = game();
    let mine = [
        put(&mut state, P0, "Llanowar Elves", Zone::Battlefield),
        put(&mut state, P0, "Llanowar Elves", Zone::Battlefield),
    ];
    let case = put(
        &mut state,
        P0,
        "Case of the Gateway Express",
        Zone::Battlefield,
    );
    settle_case_trigger(&mut state, case);
    attack_with(&mut state, mine.to_vec());
    to_end_step(&mut state);
    assert_eq!(engine::effective_power(&state, mine[0]), 1);
}

/// A Case put onto the battlefield with no creature to target: its entering
/// trigger is removed for lack of a target.
fn settle_case_trigger(state: &mut GameState, case: ObjectId) {
    settle(state);
    assert_eq!(state.objects.get(case).zone, Zone::Battlefield);
}

// ---- Classes -------------------------------------------------------------------

/// Passes from the precombat main phase to the beginning of combat and
/// returns the legal targets of the trigger waiting there.
fn begin_combat_targets(state: &mut GameState) -> Vec<Target> {
    loop {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } if state.step == Step::BeginCombat => {
                return legal_targets
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
}

fn level_up(state: &mut GameState, class: ObjectId, ability: u8, color: ManaColor, mana: u8) {
    state.players[0].mana_pool[color.pool_index()] = mana;
    assert!(activatable(state).contains(&(class, ability)));
    act(state, Action::ActivateAbility(class, ability));
    settle(state);
    state.players[0].mana_pool = Default::default();
}

#[test]
fn innkeepers_talent_levels_in_order_and_grants_ward_to_countered_permanents() {
    let mut state = game();
    let class = put(&mut state, P0, "Innkeeper's Talent", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);

    // Level 3 needs level 2 first (716.2a).
    state.players[0].mana_pool[ManaColor::G.pool_index()] = 5;
    let abilities = activatable(&mut state);
    assert!(abilities.contains(&(class, 0)));
    assert!(!abilities.contains(&(class, 1)));
    level_up(&mut state, class, 0, ManaColor::G, 1);
    assert!(
        !activatable(&mut state).contains(&(class, 0)),
        "already level 2"
    );

    // Level 1: a +1/+1 counter on target creature you control at the
    // beginning of combat on your turn.
    assert_eq!(
        begin_combat_targets(&mut state),
        vec![Target::Object(elves)]
    );
    act(&mut state, Action::ChooseTarget(Target::Object(elves)));
    settle(&mut state);
    assert_eq!(state.objects.get(elves).counters.plus1_plus1, 1);

    // Level 2: it has a counter, so it has ward {1}.
    state.active_player = P1;
    state.priority_player = P1;
    let burst = put(&mut state, P1, "Burst Lightning", Zone::Hand);
    state.players[1].mana_pool[ManaColor::R.pool_index()] = 1;
    next(&mut state);
    act(&mut state, Action::CastSpell(burst));
    loop {
        match next(&mut state) {
            Decision::ChooseTargets { .. } => {
                act(&mut state, Action::ChooseTarget(Target::Object(elves)))
            }
            Decision::ChooseKicker { .. } => act(&mut state, Action::ChooseKicker(false)),
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert_eq!(state.objects.get(elves).damage, 0, "countered by ward");
}

#[test]
fn innkeepers_talent_level_three_doubles_your_counters() {
    let mut state = game();
    let class = put(&mut state, P0, "Innkeeper's Talent", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    level_up(&mut state, class, 0, ManaColor::G, 1);
    level_up(&mut state, class, 1, ManaColor::G, 4);
    assert!(activatable(&mut state)
        .iter()
        .all(|&(source, _)| source != class));
    begin_combat_targets(&mut state);
    act(&mut state, Action::ChooseTarget(Target::Object(elves)));
    settle(&mut state);
    assert_eq!(state.objects.get(elves).counters.plus1_plus1, 2);
}

#[test]
fn stormchasers_talent_makes_otters_and_regrows_a_spell_at_level_two() {
    let mut state = game();
    let class = put(&mut state, P0, "Stormchaser's Talent", Zone::Hand);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 1;
    next(&mut state);
    act(&mut state, Action::CastSpell(class));
    settle(&mut state);
    let otters = battlefield_named(&state, P0, "Otter");
    assert_eq!(otters.len(), 1);
    let otter = otters[0];
    assert_eq!(engine::effective_power(&state, otter), 1);

    let burst = put(&mut state, P0, "Burst Lightning", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(burst, Zone::Graveyard),
    );
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 4;
    act(&mut state, Action::ActivateAbility(class, 0));
    drive(&mut state, &[Target::Object(burst)]);
    assert_eq!(state.objects.get(burst).zone, Zone::Hand);

    // Level 3: casting an instant makes an Otter, and the first Otter's
    // prowess triggers.
    level_up(&mut state, class, 1, ManaColor::U, 6);
    cast_burst_at_opponent(&mut state);
    assert_eq!(battlefield_named(&state, P0, "Otter").len(), 2);
    assert_eq!(engine::effective_power(&state, otter), 2);
    assert_eq!(state.players[1].life, 18);
}

// ---- Case of the Uneaten Feast -----------------------------------------------

/// Passes priority through the rest of this turn and the opponent's turn to
/// P0's next precombat main phase.
fn to_next_own_main(state: &mut GameState) {
    let start = state.turn;
    loop {
        let decision = next(state);
        if state.turn > start && state.active_player == P0 && state.step == Step::Main1 {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::DeclareAttackers { .. } => act(state, Action::DeclareAttackers(Vec::new())),
            Decision::DeclareBlockers { .. } => act(state, Action::DeclareBlockers(Vec::new())),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
}

#[test]
fn case_of_the_uneaten_feast_gains_life_solves_and_lets_creatures_be_cast_from_the_graveyard() {
    let mut state = game();
    let case = put(
        &mut state,
        P0,
        "Case of the Uneaten Feast",
        Zone::Battlefield,
    );
    for _ in 0..4 {
        put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    }
    settle(&mut state);
    assert_eq!(state.players[0].life, 24);
    assert!(!activatable(&mut state).contains(&(case, 0)), "unsolved");
    // Four life is not enough to solve it.
    to_end_step(&mut state);
    to_next_own_main(&mut state);
    assert!(!activatable(&mut state).contains(&(case, 0)));

    for _ in 0..5 {
        put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    }
    settle(&mut state);
    to_end_step(&mut state);
    to_next_own_main(&mut state);
    assert!(activatable(&mut state).contains(&(case, 0)), "solved");

    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(elves, Zone::Graveyard),
    );
    state.players[0].mana_pool[ManaColor::G.pool_index()] = 1;
    match next(&mut state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => assert!(!castable_spells.contains(&elves)),
        other => panic!("unexpected decision: {other:?}"),
    }
    act(&mut state, Action::ActivateAbility(case, 0));
    settle(&mut state);
    assert_eq!(state.objects.get(case).zone, Zone::Graveyard);
    match next(&mut state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => assert!(castable_spells.contains(&elves)),
        other => panic!("unexpected decision: {other:?}"),
    }
    act(&mut state, Action::CastSpell(elves));
    settle(&mut state);
    assert_eq!(state.objects.get(elves).zone, Zone::Battlefield);
}

// ---- Liliana of the Veil -----------------------------------------------

#[test]
fn liliana_plus_one_makes_each_player_discard_a_card() {
    let mut state = game();
    let liliana = put(&mut state, P0, "Liliana of the Veil", Zone::Battlefield);
    assert_eq!(loyalty(&state, liliana), Some(3));
    let kept = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    let pitched = put(&mut state, P0, "Forest", Zone::Hand);
    let theirs = put(&mut state, P1, "Tolarian Terror", Zone::Hand);
    let their_other = put(&mut state, P1, "Forest", Zone::Hand);
    act(&mut state, Action::ActivateAbility(liliana, 0));
    let mut discarders = Vec::new();
    loop {
        match next(&mut state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            Decision::Discard {
                player, choices, ..
            } => {
                discarders.push(player);
                let card = if player == P0 { pitched } else { theirs };
                assert!(choices.contains(&card));
                act(&mut state, Action::Discard(vec![card]));
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert_eq!(discarders, vec![P0, P1]);
    assert_eq!(loyalty(&state, liliana), Some(4));
    assert_eq!(state.objects.get(pitched).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(theirs).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(kept).zone, Zone::Hand);
    assert_eq!(state.objects.get(their_other).zone, Zone::Hand);
}

#[test]
fn liliana_minus_two_makes_target_player_sacrifice_a_creature() {
    let mut state = game();
    let liliana = put(&mut state, P0, "Liliana of the Veil", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let terror = put(&mut state, P1, "Tolarian Terror", Zone::Battlefield);
    act(&mut state, Action::ActivateAbility(liliana, 1));
    loop {
        match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Player(P0)));
                act(&mut state, Action::ChooseTarget(Target::Player(P1)))
            }
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            Decision::ChooseEffectTargets { player, .. } => {
                assert_eq!(player, P1, "the targeted player chooses");
                act(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(elves)),
                );
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert_eq!(loyalty(&state, liliana), Some(1));
    assert_eq!(state.objects.get(elves).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(terror).zone, Zone::Battlefield);
}

#[test]
fn liliana_minus_six_separates_piles_and_the_target_sacrifices_one() {
    let mut state = game();
    let liliana = put(&mut state, P0, "Liliana of the Veil", Zone::Battlefield);
    change_loyalty(&mut state, liliana, 3);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let terror = put(&mut state, P1, "Tolarian Terror", Zone::Battlefield);
    let forest = put(&mut state, P1, "Forest", Zone::Battlefield);
    let mine = put(&mut state, P0, "Forest", Zone::Battlefield);
    assert!(activatable(&mut state).contains(&(liliana, 2)));
    act(&mut state, Action::ActivateAbility(liliana, 2));
    let mut separated = false;
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
                can_finish,
                ..
            } => {
                assert_eq!(player, P0, "Liliana's controller separates");
                assert!(can_finish);
                assert!(!legal_targets.contains(&Target::Object(mine)));
                if legal_targets.contains(&Target::Object(elves)) {
                    act(
                        &mut state,
                        Action::ChooseEffectTarget(Target::Object(elves)),
                    );
                } else if legal_targets.contains(&Target::Object(forest)) {
                    act(
                        &mut state,
                        Action::ChooseEffectTarget(Target::Object(forest)),
                    );
                } else {
                    separated = true;
                    act(&mut state, Action::FinishEffectSelection);
                }
            }
            Decision::ChooseEffectBoolean { player, .. } => {
                assert_eq!(player, P1, "the target chooses the pile");
                // false: the second pile, Tolarian Terror alone.
                act(&mut state, Action::ChooseEffectBoolean(false));
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert!(separated);
    assert_eq!(state.objects.get(liliana).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(terror).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(elves).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(forest).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(mine).zone, Zone::Battlefield);
}

// ---- Breach the Multiverse ---------------------------------------------

fn to_graveyard(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let card = put(state, player, name, Zone::Hand);
    event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Graveyard));
    card
}

#[test]
fn breach_the_multiverse_mills_and_takes_a_card_from_each_graveyard() {
    let mut state = game();
    let elves = to_graveyard(&mut state, P0, "Llanowar Elves");
    let terror = to_graveyard(&mut state, P1, "Tolarian Terror");
    let teferi = to_graveyard(&mut state, P1, "Teferi, Temporal Pilgrim");
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let breach = put(&mut state, P0, "Breach the Multiverse", Zone::Hand);
    state.players[0].mana_pool[ManaColor::B.pool_index()] = 7;
    let libraries = [
        state.players[0].library.len(),
        state.players[1].library.len(),
    ];
    next(&mut state);
    act(&mut state, Action::CastSpell(breach));
    let mut picks = Vec::new();
    loop {
        match next(&mut state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            Decision::ChooseEffectTargets {
                player,
                legal_targets,
                ..
            } => {
                let pick = [elves, teferi]
                    .into_iter()
                    .map(Target::Object)
                    .find(|target| legal_targets.contains(target));
                if let Some(pick) = pick {
                    assert_eq!(player, P0, "Breach's controller chooses");
                    assert!(!legal_targets.contains(&Target::Object(mine)));
                    picks.push(pick);
                    act(&mut state, Action::ChooseEffectTarget(pick));
                } else {
                    // A milled batch's owner orders it into the graveyard.
                    act(&mut state, Action::ChooseEffectTarget(legal_targets[0]));
                }
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert_eq!(picks, vec![Target::Object(elves), Target::Object(teferi)]);
    assert_eq!(state.players[0].library.len(), libraries[0] - 10);
    assert_eq!(state.players[1].library.len(), libraries[1] - 10);
    for card in [elves, teferi] {
        assert_eq!(state.objects.get(card).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(card).controller, P0);
    }
    assert_eq!(loyalty(&state, teferi), Some(4));
    assert_eq!(state.objects.get(terror).zone, Zone::Graveyard);
    let phyrexian = mtg_kernel::card_def::Subtype::Phyrexian;
    assert!(engine::has_effective_subtype(&state, elves, phyrexian));
    assert!(engine::has_effective_subtype(&state, mine, phyrexian));
    assert!(!engine::has_effective_subtype(&state, teferi, phyrexian));
    assert!(engine::effective_subtype_ids(&state, mine).contains(&phyrexian.stable_id()));
}
