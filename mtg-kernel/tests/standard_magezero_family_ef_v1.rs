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

#[test]
fn kaito_changes_type_each_turn_retains_loyalty_and_stacks_emblems() {
    let mut state = game();
    let kaito = put(
        &mut state,
        P0,
        "Kaito, Bane of Nightmares",
        Zone::Battlefield,
    );
    assert_eq!(loyalty(&state, kaito), Some(4));
    assert!(engine::object_has_type(&state, kaito, CardType::Creature));
    assert!(!engine::object_has_type(
        &state,
        kaito,
        CardType::Planeswalker
    ));
    assert_eq!(engine::effective_power(&state, kaito), 3);
    assert_eq!(engine::effective_toughness(&state, kaito), 4);
    assert!(engine::has_effective_keyword(
        &state,
        kaito,
        Keywords::HEXPROOF
    ));
    act(&mut state, Action::ActivateAbility(kaito, 0));
    resolve_stack(&mut state);
    assert_eq!(loyalty(&state, kaito), Some(5));
    assert_eq!(engine::effective_power(&state, kaito), 4);
    state.active_player = P1;
    assert!(!engine::object_has_type(&state, kaito, CardType::Creature));
    assert!(engine::object_has_type(
        &state,
        kaito,
        CardType::Planeswalker
    ));
    assert!(!engine::has_effective_keyword(
        &state,
        kaito,
        Keywords::HEXPROOF
    ));
    assert_eq!(loyalty(&state, kaito), Some(5));
    state.active_player = P0;
    let ctx = mtg_kernel::effect::ExecCtx::no_targets(kaito, P0);
    mtg_kernel::effect::execute(
        &mtg_kernel::standard_cards_v1::kaito_emblem(),
        &ctx,
        &mut state,
    );
    assert_eq!(engine::effective_power(&state, kaito), 5);
    let ninja = put(&mut state, P0, "Kaito, Bane of Nightmares", Zone::Hand);
    assert_ne!(ninja, kaito);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(kaito, Zone::Graveyard),
    );
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(ninja, Zone::Battlefield),
    );
    assert_eq!(
        engine::effective_power(&state, ninja),
        5,
        "emblems survive their source"
    );
}

#[test]
fn kaito_minus_two_taps_and_places_two_stun_counters() {
    let mut state = game();
    let kaito = put(
        &mut state,
        P0,
        "Kaito, Bane of Nightmares",
        Zone::Battlefield,
    );
    let target = put(&mut state, P1, "Quirion Beastcaller", Zone::Battlefield);
    act(&mut state, Action::ActivateAbility(kaito, 2));
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    act(&mut state, Action::ChooseTarget(Target::Object(target)));
    resolve_stack(&mut state);
    assert_eq!(loyalty(&state, kaito), Some(2));
    assert!(state.objects.get(target).tapped);
    assert_eq!(state.objects.get(target).counters.stun, 2);
}

#[test]
fn tidebinder_removes_kaitos_abilities_but_preserves_his_creature_layers() {
    let mut state = game();
    let kaito = put(
        &mut state,
        P0,
        "Kaito, Bane of Nightmares",
        Zone::Battlefield,
    );
    next(&mut state);
    act(&mut state, Action::ActivateAbility(kaito, 0));
    next(&mut state);
    let ability = state.stack.last().unwrap().v4.stack_item_id;
    let tidebinder = put(&mut state, P0, "Tishana's Tidebinder", Zone::Hand);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 3;
    act(&mut state, Action::CastSpell(tidebinder));
    drive(&mut state, &[Target::StackItem(ability)]);
    assert!(!state
        .stack
        .iter()
        .any(|item| item.v4.stack_item_id == ability));
    assert_eq!(loyalty(&state, kaito), Some(5));
    assert_eq!(
        engine::effective_power(&state, kaito),
        3,
        "the emblem ability was countered"
    );
    assert_eq!(engine::effective_toughness(&state, kaito), 4);
    assert!(engine::object_has_type(&state, kaito, CardType::Creature));
    assert!(!engine::object_has_type(
        &state,
        kaito,
        CardType::Planeswalker
    ));
    assert_eq!(
        engine::effective_subtype_ids(&state, kaito),
        vec![mtg_kernel::card_def::Subtype::Ninja.stable_id()]
    );
    assert!(!engine::has_effective_keyword(
        &state,
        kaito,
        Keywords::HEXPROOF
    ));
    state = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    state.active_player = P1;
    assert!(engine::object_has_type(
        &state,
        kaito,
        CardType::Planeswalker
    ));
    assert!(!engine::object_has_type(&state, kaito, CardType::Creature));
    state.active_player = P0;
    assert!(engine::object_has_type(&state, kaito, CardType::Creature));
    assert_eq!(engine::effective_power(&state, kaito), 3);
    assert!(!engine::has_effective_keyword(
        &state,
        kaito,
        Keywords::HEXPROOF
    ));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(tidebinder, Zone::Graveyard),
    );
    assert!(engine::has_effective_keyword(
        &state,
        kaito,
        Keywords::HEXPROOF
    ));
}

#[test]
fn kaitos_earlier_creature_layers_do_not_overwrite_witness_protection() {
    let mut state = game();
    let kaito = put(
        &mut state,
        P0,
        "Kaito, Bane of Nightmares",
        Zone::Battlefield,
    );
    let aura = put(&mut state, P0, "Witness Protection", Zone::Hand);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 1;
    next(&mut state);
    act(&mut state, Action::CastSpell(aura));
    drive(&mut state, &[Target::Object(kaito)]);
    assert_eq!(engine::effective_power(&state, kaito), 1);
    assert_eq!(engine::effective_toughness(&state, kaito), 1);
    assert_eq!(
        engine::effective_subtype_ids(&state, kaito),
        vec![mtg_kernel::card_def::Subtype::Citizen.stable_id()]
    );
    assert!(!engine::has_effective_keyword(
        &state,
        kaito,
        Keywords::HEXPROOF
    ));
    assert!(engine::object_has_type(&state, kaito, CardType::Creature));
    state.active_player = P1;
    assert!(engine::object_has_type(&state, kaito, CardType::Creature));
    assert!(!engine::object_has_type(
        &state,
        kaito,
        CardType::Planeswalker
    ));
}

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
    let flash = put(&mut state, P0, "Resolute Reinforcements", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(flash, Zone::Graveyard),
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

    state.players[0].mana_pool[ManaColor::W.pool_index()] = 2;
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&flash)
    ));
    let round = state.turn;
    // Enter cleanup through the engine so its end-of-turn effects actually run.
    state.step = Step::End;
    loop {
        let decision = next(&mut state);
        if state.active_player == P1 && state.priority_player == P0 {
            break;
        }
        assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
        act(&mut state, Action::Pass);
    }
    assert_eq!(state.turn, round, "cleanup occurs within the same round");
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 2;
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { castable_spells, .. } if !castable_spells.contains(&flash)
    ));
    assert!(engine::step(&mut state, Action::CastSpell(flash)).is_err());
}

#[test]
fn case_and_mosswood_graveyard_permissions_offer_only_their_authorized_forms() {
    use mtg_kernel::state::{CastMethodV4, SpellCastRouteV4};

    for (case_permission, adventure_permission) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        let mut original = game();
        let knight = put(
            &mut original,
            P0,
            "Mosswood Dreadknight",
            if adventure_permission {
                Zone::Battlefield
            } else {
                Zone::Hand
            },
        );
        event::propose_and_commit(
            &mut original,
            ProposedEvent::zone_change(knight, Zone::Graveyard),
        );
        settle(&mut original);
        if case_permission {
            // Isolate the resolved Case grant; solving and paying its sacrifice
            // are exercised by the preceding Case integration regression.
            let case = put(
                &mut original,
                P0,
                "Case of the Uneaten Feast",
                Zone::Battlefield,
            );
            mtg_kernel::effect::execute(
                &mtg_kernel::standard_cards_v1::uneaten_feast_grant(),
                &mtg_kernel::effect::ExecCtx::no_targets(case, P0),
                &mut original,
            );
            settle(&mut original);
        }

        for (form, allowed, color) in [
            (0, case_permission, ManaColor::G),
            (1, adventure_permission, ManaColor::B),
        ] {
            let mut state = original.clone();
            state.players[0].mana_pool[color.pool_index()] = 1;
            state.players[0].mana_pool[5] = 1;
            let Decision::CastSpellOrPass {
                castable_spells, ..
            } = next(&mut state)
            else {
                panic!("expected a cast offer");
            };
            assert_eq!(
                castable_spells.iter().filter(|&&id| id == knight).count(),
                usize::from(allowed)
            );
            if !allowed {
                assert!(engine::step(&mut state, Action::CastSpell(knight)).is_err());
                continue;
            }

            // Payable costs for both forms expose the real choice when both
            // independent permissions are present.
            state.players[0].mana_pool[ManaColor::G.pool_index()] = 1;
            state.players[0].mana_pool[ManaColor::B.pool_index()] = 1;
            act(&mut state, Action::CastSpell(knight));
            let mut restored: GameState =
                serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
            for state in [&mut state, &mut restored] {
                let decision = next(state);
                if case_permission && adventure_permission {
                    assert!(
                        matches!(decision, Decision::ChooseSpellMode { legal_modes, .. } if legal_modes == vec![0, 1])
                    );
                    act(state, Action::ChooseSpellMode(form));
                    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
                } else {
                    assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
                }
                let spell = state.stack.last().unwrap();
                assert_eq!(spell.source, knight);
                assert_eq!(
                    spell.v4.cast_method,
                    Some(if form == 0 {
                        CastMethodV4::Normal
                    } else {
                        CastMethodV4::Omen
                    })
                );
                let route = spell
                    .v4
                    .source_contract
                    .unwrap()
                    .spell_cast_origin
                    .unwrap()
                    .route;
                assert!(if form == 0 {
                    matches!(route, SpellCastRouteV4::GraveyardPermissionV1 { .. })
                } else {
                    matches!(route, SpellCastRouteV4::GraveyardAdventure { .. })
                });
                let before_hand = state.players[0].hand.len();
                settle(state);
                assert_eq!(
                    state.objects.get(knight).zone,
                    if form == 0 {
                        Zone::Battlefield
                    } else {
                        Zone::Exile
                    }
                );
                assert_eq!(
                    state.players[0].hand.len(),
                    before_hand + usize::from(form == 1)
                );
            }
            assert_eq!(state, restored);
        }
    }
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

// ---- Fable of the Mirror-Breaker -----------------------------------------

/// Resolves the stack, answering a chapter II discard with `discard`.
fn resolve_with_discards(state: &mut GameState, discard: &[ObjectId]) {
    loop {
        match next(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::ChooseEffectTargets {
                player,
                legal_targets,
                can_finish,
                ..
            } => {
                assert_eq!(player, P0);
                assert!(can_finish, "discarding is optional");
                match discard
                    .iter()
                    .map(|&card| Target::Object(card))
                    .find(|target| legal_targets.contains(target))
                {
                    Some(card) => act(state, Action::ChooseEffectTarget(card)),
                    None => act(state, Action::FinishEffectSelection),
                }
            }
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
}

#[test]
fn fable_of_the_mirror_breaker_makes_a_goblin_rummages_and_transforms() {
    let mut state = game();
    let fable = put(&mut state, P0, "Fable of the Mirror-Breaker", Zone::Hand);
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 3;
    next(&mut state);
    act(&mut state, Action::CastSpell(fable));
    resolve_with_discards(&mut state, &[]);
    assert_eq!(battlefield_named(&state, P0, "Goblin Shaman").len(), 1);

    // Chapter II: discard two, draw two.
    let first = put(&mut state, P0, "Forest", Zone::Hand);
    let second = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    to_next_own_main(&mut state);
    let hand = state.players[0].hand.len();
    resolve_with_discards(&mut state, &[first, second]);
    assert_eq!(state.objects.get(first).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(second).zone, Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), hand);

    // Chapter III: exiled and returned as Reflection of Kiki-Jiki.
    to_next_own_main(&mut state);
    resolve_with_discards(&mut state, &[]);
    let reflection = state.objects.get(fable);
    assert_eq!(reflection.zone, Zone::Battlefield);
    assert_eq!(reflection.v4.face_index, 1);
    assert!(engine::object_has_type(&state, fable, CardType::Creature));
}

#[test]
fn fable_chapter_two_may_discard_nothing() {
    let mut state = game();
    let fable = put(&mut state, P0, "Fable of the Mirror-Breaker", Zone::Hand);
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 3;
    next(&mut state);
    act(&mut state, Action::CastSpell(fable));
    resolve_with_discards(&mut state, &[]);
    let kept = put(&mut state, P0, "Forest", Zone::Hand);
    to_next_own_main(&mut state);
    let hand = state.players[0].hand.len();
    resolve_with_discards(&mut state, &[]);
    assert_eq!(state.objects.get(kept).zone, Zone::Hand);
    assert_eq!(state.players[0].hand.len(), hand);
}

#[test]
fn fable_goblin_shaman_makes_a_treasure_when_it_attacks() {
    let mut state = game();
    let goblin = put(
        &mut state,
        P0,
        "Fable Goblin Shaman Token",
        Zone::Battlefield,
    );
    attack_with(&mut state, vec![goblin]);
    let treasure = card_id_by_name("Treasure Token").unwrap();
    assert_eq!(
        state.players[0]
            .battlefield
            .iter()
            .filter(|&&id| state.objects.get(id).card_def == treasure)
            .count(),
        1
    );
    assert_eq!(state.players[1].life, 18);
}

#[test]
fn reflection_of_kiki_jiki_copies_with_haste_and_sacrifices_at_end_step() {
    let mut state = game();
    let fable = put(
        &mut state,
        P0,
        "Fable of the Mirror-Breaker",
        Zone::Battlefield,
    );
    resolve_with_discards(&mut state, &[]);
    event::propose_and_commit(&mut state, ProposedEvent::transform_in_place(fable, 1));
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let cecil = put(&mut state, P0, "Cecil, Dark Knight", Zone::Battlefield);
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
    assert!(activatable(&mut state).contains(&(fable, 0)));
    act(&mut state, Action::ActivateAbility(fable, 0));
    loop {
        match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(elves)));
                assert!(!legal_targets.contains(&Target::Object(fable)), "another");
                assert!(
                    !legal_targets.contains(&Target::Object(cecil)),
                    "nonlegendary"
                );
                act(&mut state, Action::ChooseTarget(Target::Object(elves)));
            }
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    let copies = battlefield_named(&state, P0, "Llanowar Elves");
    assert_eq!(copies.len(), 2);
    let copy = copies.into_iter().find(|&id| id != elves).unwrap();
    assert!(state.objects.get(copy).v4.is_token);
    assert!(engine::has_effective_keyword(&state, copy, Keywords::HASTE));
    assert!(!engine::has_effective_keyword(
        &state,
        elves,
        Keywords::HASTE
    ));

    to_end_step(&mut state);
    assert_eq!(battlefield_named(&state, P0, "Llanowar Elves"), vec![elves]);
}

// ---- Repurposing Bay ---------------------------------------------------

fn to_library(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let card = put(state, player, name, Zone::Hand);
    event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Library));
    card
}

#[test]
fn repurposing_bay_trades_an_artifact_for_one_with_one_more_mana_value() {
    let mut state = game();
    let bay = put(&mut state, P0, "Repurposing Bay", Zone::Battlefield);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 2;
    // "Another artifact": the Bay cannot pay with itself.
    assert!(!activatable(&mut state).contains(&(bay, 0)));

    let wellspring = to_library(&mut state, P0, "Ichor Wellspring");
    let spellbomb = to_library(&mut state, P0, "Nihil Spellbomb");
    let trail = put(&mut state, P0, "Candy Trail", Zone::Battlefield);
    settle(&mut state);
    assert!(activatable(&mut state).contains(&(bay, 0)));
    act(&mut state, Action::ActivateAbility(bay, 0));
    let mut searched = false;
    loop {
        match next(&mut state) {
            Decision::ChooseCostTargets { candidates, .. } => {
                assert_eq!(candidates, vec![trail]);
                act(&mut state, Action::ChooseCostTarget(trail));
            }
            Decision::ChooseEffectTargets {
                player,
                legal_targets,
                ..
            } => {
                assert_eq!(player, P0);
                assert_eq!(legal_targets, vec![Target::Object(wellspring)]);
                searched = true;
                act(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(wellspring)),
                );
            }
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => act(
                &mut state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert!(searched);
    assert_eq!(state.objects.get(trail).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(wellspring).zone, Zone::Battlefield);
    assert!(!state.objects.get(wellspring).tapped);
    assert_eq!(state.objects.get(spellbomb).zone, Zone::Library);
    assert!(state.objects.get(bay).tapped);
}

// ---- The Irencrag ------------------------------------------------------

fn mana_sources(state: &mut GameState) -> Vec<ObjectId> {
    match next(state) {
        Decision::CastSpellOrPass { mana_abilities, .. } => mana_abilities,
        other => panic!("unexpected decision: {other:?}"),
    }
}

/// Resolves the stack, answering the Everflame choice with `accept`.
fn resolve_everflame(state: &mut GameState, accept: bool) -> bool {
    let mut asked = false;
    loop {
        match next(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return asked,
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::ChooseEffectBoolean { player, .. } => {
                assert_eq!(player, P0);
                asked = true;
                act(state, Action::ChooseEffectBoolean(accept));
            }
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
}

#[test]
fn the_irencrag_becomes_everflame_when_a_legend_enters() {
    let mut state = game();
    let irencrag = put(&mut state, P0, "The Irencrag", Zone::Battlefield);
    assert!(mana_sources(&mut state).contains(&irencrag));
    assert!(
        !activatable(&mut state).contains(&(irencrag, 0)),
        "no equip yet"
    );

    // A nonlegendary creature does nothing.
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    assert!(!resolve_everflame(&mut state, true));

    let cecil = put(&mut state, P0, "Cecil, Dark Knight", Zone::Battlefield);
    assert!(resolve_everflame(&mut state, true));
    assert_eq!(
        state.objects.get(irencrag).name,
        "Everflame, Heroes' Legacy"
    );
    assert!(engine::has_effective_subtype(
        &state,
        irencrag,
        mtg_kernel::card_def::Subtype::Equipment
    ));
    assert!(
        !mana_sources(&mut state).contains(&irencrag),
        "loses its mana ability"
    );

    // Equip {3}: equipped creature gets +3/+3.
    let power = engine::effective_power(&state, cecil);
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 3;
    assert!(activatable(&mut state).contains(&(irencrag, 0)));
    act(&mut state, Action::ActivateAbility(irencrag, 0));
    drive(&mut state, &[Target::Object(cecil)]);
    assert_eq!(engine::effective_power(&state, cecil), power + 3);
    assert_eq!(engine::effective_power(&state, elves), 1);

    // Everflame lost the trigger: another legend asks nothing.
    put(&mut state, P0, "Cecil, Dark Knight", Zone::Battlefield);
    assert!(!resolve_everflame(&mut state, true));
}

#[test]
fn the_irencrag_may_stay_a_mana_rock() {
    let mut state = game();
    let irencrag = put(&mut state, P0, "The Irencrag", Zone::Battlefield);
    put(&mut state, P0, "Cecil, Dark Knight", Zone::Battlefield);
    assert!(resolve_everflame(&mut state, false));
    assert_eq!(state.objects.get(irencrag).name, "The Irencrag");
    assert!(mana_sources(&mut state).contains(&irencrag));
    assert!(!activatable(&mut state).contains(&(irencrag, 0)));
}

// ---- Craft: Clay-Fired Bricks // Cosmium Kiln ---------------------------

#[test]
fn clay_fired_bricks_finds_a_basic_plains_and_gains_two() {
    let mut state = game();
    let plains = to_library(&mut state, P0, "Plains");
    let bricks = put(&mut state, P0, "Clay-Fired Bricks", Zone::Hand);
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 2;
    next(&mut state);
    act(&mut state, Action::CastSpell(bricks));
    let mut offered = Vec::new();
    loop {
        match next(&mut state) {
            Decision::ChooseEffectTargets { legal_targets, .. } => {
                offered = legal_targets;
                act(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(plains)),
                );
            }
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    // Only the basic Plains, not the Islands.
    assert_eq!(offered, vec![Target::Object(plains)]);
    assert_eq!(state.objects.get(plains).zone, Zone::Hand);
    assert_eq!(state.players[0].life, 22);
    assert_eq!(state.objects.get(bricks).zone, Zone::Battlefield);
}

/// Activates Clay-Fired Bricks' craft ability with seven white mana,
/// exiling `material`, and resolves it.
fn craft_bricks(state: &mut GameState, bricks: ObjectId, material: ObjectId) -> Vec<ObjectId> {
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 7;
    assert!(activatable(state).contains(&(bricks, 0)));
    act(state, Action::ActivateAbility(bricks, 0));
    let mut candidates = Vec::new();
    loop {
        match next(state) {
            Decision::ChooseCostTargets {
                candidates: offered,
                ..
            } => {
                candidates = offered;
                act(state, Action::ChooseCostTarget(material));
            }
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return candidates,
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
}

#[test]
fn crafting_clay_fired_bricks_exiles_an_artifact_and_returns_cosmium_kiln() {
    let mut state = game();
    let bricks = put(&mut state, P0, "Clay-Fired Bricks", Zone::Battlefield);
    settle(&mut state);
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 7;
    // Craft needs another artifact: Bricks cannot exile itself twice.
    assert!(!activatable(&mut state).contains(&(bricks, 0)));

    let trail = put(&mut state, P0, "Candy Trail", Zone::Battlefield);
    settle(&mut state);
    let wellspring = to_graveyard(&mut state, P0, "Ichor Wellspring");
    let opponents = to_graveyard(&mut state, P1, "Nihil Spellbomb");
    let life = state.players[0].life;
    let candidates = craft_bricks(&mut state, bricks, wellspring);
    // Another artifact you control or an artifact card in your graveyard.
    assert_eq!(candidates, vec![trail, wellspring]);
    assert!(!candidates.contains(&opponents));

    assert_eq!(state.objects.get(wellspring).zone, Zone::Exile);
    assert_eq!(state.objects.get(trail).zone, Zone::Battlefield);
    let kiln = state.objects.get(bricks);
    assert_eq!(kiln.zone, Zone::Battlefield);
    assert_eq!(kiln.v4.face_index, 1);
    // Cosmium Kiln's entry makes two Gnomes; its anthem makes them 2/2.
    let gnomes = battlefield_named(&state, P0, "Gnome");
    assert_eq!(gnomes.len(), 2);
    for gnome in gnomes {
        assert!(engine::object_has_type(&state, gnome, CardType::Artifact));
        assert_eq!(engine::effective_power(&state, gnome), 2);
        assert_eq!(engine::effective_toughness(&state, gnome), 2);
    }
    // The front face's entry trigger does not fire for the back face.
    assert_eq!(state.players[0].life, life);
}

#[test]
fn crafting_is_sorcery_speed_and_can_exile_a_battlefield_artifact() {
    let mut state = game();
    let bricks = put(&mut state, P0, "Clay-Fired Bricks", Zone::Battlefield);
    let trail = put(&mut state, P0, "Candy Trail", Zone::Battlefield);
    settle(&mut state);
    state.step = Step::Upkeep;
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 7;
    assert!(!activatable(&mut state).contains(&(bricks, 0)));
    state.step = Step::Main1;
    // The only material is chosen automatically.
    assert!(craft_bricks(&mut state, bricks, trail).is_empty());
    assert_eq!(state.objects.get(trail).zone, Zone::Exile);
    assert_eq!(state.objects.get(bricks).v4.face_index, 1);
    // Cosmium Kiln has no craft ability.
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 7;
    assert!(!activatable(&mut state).contains(&(bricks, 0)));
}

// ---- Craft: Braided Net // Braided Quipu ------------------------------

/// Activates Braided Net's tap ability on `target` and resolves it.
fn net(state: &mut GameState, braided: ObjectId, target: ObjectId) {
    assert!(activatable(state).contains(&(braided, 0)));
    act(state, Action::ActivateAbility(braided, 0));
    drive(state, &[Target::Object(target)]);
}

/// Passes priority until `player`'s next main phase.
fn to_main_of(state: &mut GameState, player: PlayerId) {
    let turn = state.turn;
    loop {
        let decision = next(state);
        if state.turn != turn && state.active_player == player && state.step == Step::Main1 {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::DeclareAttackers { .. } => act(state, Action::DeclareAttackers(Vec::new())),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
}

#[test]
fn braided_net_taps_and_locks_abilities_until_the_permanent_untaps() {
    let mut state = game();
    let braided = put(&mut state, P0, "Braided Net", Zone::Battlefield);
    let convocation = put(&mut state, P0, "Lunar Convocation", Zone::Battlefield);
    settle(&mut state);
    state.players[0].mana_pool[ManaColor::B.pool_index()] = 2;
    assert!(activatable(&mut state).contains(&(convocation, 0)));

    // Net can't target itself or a land.
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    act(&mut state, Action::ActivateAbility(braided, 0));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(convocation)));
            assert!(!legal_targets.contains(&Target::Object(braided)));
            assert!(!legal_targets.contains(&Target::Object(island)));
            act(
                &mut state,
                Action::ChooseTarget(Target::Object(convocation)),
            );
        }
        other => panic!("unexpected decision: {other:?}"),
    }
    resolve_stack(&mut state);
    assert!(state.objects.get(convocation).tapped);
    // Lunar Convocation's draw ability has no {T}, but it is locked.
    state.players[0].mana_pool[ManaColor::B.pool_index()] = 2;
    assert!(!activatable(&mut state).contains(&(convocation, 0)));

    // Replacing an effect's untap attempt removes one stun counter but does
    // not end Net's lock. The following natural untap ends it normally.
    state.objects.get_mut(convocation).counters.stun = 1;
    mtg_kernel::effect::execute(
        &mtg_kernel::effect::EffectOp::UntapObject {
            object: mtg_kernel::effect::ObjectRef::ThisSource,
        },
        &mtg_kernel::effect::ExecCtx::no_targets(convocation, P0),
        &mut state,
    );
    assert!(state.objects.get(convocation).tapped);
    assert_eq!(state.objects.get(convocation).counters.stun, 0);
    assert!(!activatable(&mut state).contains(&(convocation, 0)));

    // The lock ends when it untaps, and a later tap does not renew it.
    to_main_of(&mut state, P1);
    to_main_of(&mut state, P0);
    assert!(!state.objects.get(convocation).tapped);
    state.objects.get_mut(convocation).tapped = true;
    state.players[0].mana_pool[ManaColor::B.pool_index()] = 2;
    assert!(activatable(&mut state).contains(&(convocation, 0)));
}

#[test]
fn braided_net_has_three_net_counters() {
    let mut state = game();
    let braided = put(&mut state, P0, "Braided Net", Zone::Battlefield);
    let trail = put(&mut state, P1, "Candy Trail", Zone::Battlefield);
    settle(&mut state);
    for _ in 0..3 {
        state.objects.get_mut(braided).tapped = false;
        state.objects.get_mut(trail).tapped = false;
        net(&mut state, braided, trail);
        assert!(state.objects.get(trail).tapped);
    }
    state.objects.get_mut(braided).tapped = false;
    state.objects.get_mut(trail).tapped = false;
    assert!(!activatable(&mut state).contains(&(braided, 0)));
}

#[test]
fn braided_quipu_draws_per_artifact_then_goes_third_from_top() {
    let mut state = game();
    let braided = put(&mut state, P0, "Braided Net", Zone::Battlefield);
    let trail = put(&mut state, P0, "Candy Trail", Zone::Battlefield);
    let bricks = put(&mut state, P0, "Clay-Fired Bricks", Zone::Battlefield);
    settle(&mut state);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 2;
    assert!(activatable(&mut state).contains(&(braided, 1)));
    act(&mut state, Action::ActivateAbility(braided, 1));
    drive(&mut state, &[Target::Object(trail)]);
    assert_eq!(state.objects.get(trail).zone, Zone::Exile);
    assert_eq!(state.objects.get(braided).v4.face_index, 1);
    // Braided Quipu has no net ability, only its draw.
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 4;
    let offered = activatable(&mut state);
    assert!(!offered.contains(&(braided, 0)));
    assert!(offered.contains(&(braided, 2)));

    let hand = state.players[0].hand.len();
    act(&mut state, Action::ActivateAbility(braided, 2));
    resolve_stack(&mut state);
    // Two artifacts: Braided Quipu and Clay-Fired Bricks.
    assert_eq!(state.players[0].hand.len(), hand + 2);
    assert_eq!(state.objects.get(braided).zone, Zone::Library);
    assert_eq!(state.players[0].library[2], braided);
    assert_eq!(state.objects.get(bricks).zone, Zone::Battlefield);
}

// ---- Chandra, Hope's Beacon ----------------------------------------------

const CHANDRA: &str = "Chandra, Hope's Beacon";

/// Resolves everything, answering spell targets with `spell_target`, the
/// copy's new-target choice with `copy_target` and any variable-count
/// ability targets with `ability_targets`.
fn chandra_drive(
    state: &mut GameState,
    spell_target: Target,
    copy_target: Option<Target>,
    ability_targets: &[Target],
) {
    let mut ability_targets = ability_targets.iter().copied();
    loop {
        match next(state) {
            Decision::ChooseTargets { .. } => act(state, Action::ChooseTarget(spell_target)),
            Decision::ChooseKicker { .. } => act(state, Action::ChooseKicker(false)),
            Decision::ChooseEffectTargets {
                legal_targets,
                can_finish,
                ..
            } => {
                let pick = if can_finish {
                    ability_targets.next()
                } else {
                    copy_target
                };
                match pick {
                    Some(pick) => {
                        assert!(
                            legal_targets.contains(&pick),
                            "{pick:?} not in {legal_targets:?}"
                        );
                        act(state, Action::ChooseEffectTarget(pick));
                    }
                    None => act(state, Action::FinishEffectSelection),
                }
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

fn castable(state: &mut GameState) -> Vec<ObjectId> {
    match next(state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => castable_spells,
        other => panic!("unexpected decision: {other:?}"),
    }
}

#[test]
fn chandra_plus_two_adds_two_mana_in_any_combination() {
    let mut state = game();
    let chandra = put(&mut state, P0, CHANDRA, Zone::Battlefield);
    assert_eq!(loyalty(&state, chandra), Some(5));
    assert!(activatable(&mut state).contains(&(chandra, 0)));
    act(&mut state, Action::ActivateAbility(chandra, 0));
    loop {
        match next(&mut state) {
            Decision::ChooseEffectOption { option_count, .. } => {
                assert_eq!(option_count, 15);
                // WW WU WB WR WG UU UB UR UG BB BR BG RR ...
                act(&mut state, Action::ChooseEffectOption(12));
            }
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert_eq!(state.players[0].mana_pool[ManaColor::R.pool_index()], 2);
    assert_eq!(loyalty(&state, chandra), Some(7));
}

#[test]
fn chandra_plus_one_lets_you_cast_only_one_exiled_instant_or_sorcery() {
    let mut state = game();
    let chandra = put(&mut state, P0, CHANDRA, Zone::Battlefield);
    let first = to_library(&mut state, P0, "Lightning Bolt");
    let second = to_library(&mut state, P0, "Lightning Bolt");
    let island = state.players[0].library[2];
    act_and_settle(&mut state, chandra, 1);
    for card in [first, second, island] {
        assert_eq!(state.objects.get(card).zone, Zone::Exile);
    }
    assert_eq!(loyalty(&state, chandra), Some(6));
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
    let offered = castable(&mut state);
    assert!(offered.contains(&first) && offered.contains(&second));
    assert!(!offered.contains(&island));

    // The copy trigger also fires: the copy keeps the opponent as its target.
    act(&mut state, Action::CastSpell(first));
    chandra_drive(
        &mut state,
        Target::Player(P1),
        Some(Target::Player(P1)),
        &[],
    );
    assert_eq!(state.players[1].life, 14);
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
    assert!(!castable(&mut state).contains(&second));
}

fn act_and_settle(state: &mut GameState, source: ObjectId, ability: u8) {
    next(state);
    act(state, Action::ActivateAbility(source, ability));
    resolve_stack(state);
}

#[test]
fn chandra_minus_x_deals_x_to_each_of_up_to_two_targets() {
    let mut state = game();
    let chandra = put(&mut state, P0, CHANDRA, Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let offered = activatable(&mut state);
    assert!(offered.contains(&(chandra, 2)));
    assert!(!offered.contains(&(chandra, 3)));
    act(&mut state, Action::ActivateAbility(chandra, 2));
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption {
            option_count: 6,
            ..
        }
    ));
    act(&mut state, Action::ChooseEffectOption(3));
    chandra_drive(
        &mut state,
        Target::Player(P1),
        None,
        &[Target::Player(P1), Target::Object(elves)],
    );
    assert_eq!(state.players[1].life, 17);
    assert_eq!(state.objects.get(elves).zone, Zone::Graveyard);
    assert_eq!(loyalty(&state, chandra), Some(2));
}

#[test]
fn chandra_minus_x_may_choose_one_target() {
    let mut state = game();
    let chandra = put(&mut state, P0, CHANDRA, Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateAbility(chandra, 2));
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption {
            option_count: 6,
            ..
        }
    ));
    act(&mut state, Action::ChooseEffectOption(1));
    chandra_drive(&mut state, Target::Player(P1), None, &[Target::Player(P1)]);
    assert_eq!(state.players[1].life, 19);
    assert_eq!(loyalty(&state, chandra), Some(4));
}

#[test]
fn chandra_copies_the_first_instant_each_turn_with_a_new_target() {
    let mut state = game();
    put(&mut state, P0, CHANDRA, Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let burst = put(&mut state, P0, "Burst Lightning", Zone::Hand);
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
    next(&mut state);
    act(&mut state, Action::CastSpell(burst));
    chandra_drive(
        &mut state,
        Target::Player(P1),
        Some(Target::Object(elves)),
        &[],
    );
    assert_eq!(state.players[1].life, 18);
    assert_eq!(state.objects.get(elves).zone, Zone::Graveyard);

    // Only once each turn.
    let second = put(&mut state, P0, "Burst Lightning", Zone::Hand);
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
    next(&mut state);
    act(&mut state, Action::CastSpell(second));
    chandra_drive(&mut state, Target::Player(P1), None, &[]);
    assert_eq!(state.players[1].life, 16);
}

// ---- Assimilation Aegis ----------------------------------------------------

/// Resolves everything, choosing `picks` for targets in order and stopping
/// a variable-count target choice once `picks` runs out.
fn drive_up_to(state: &mut GameState, picks: &[Target]) {
    let mut picks = picks.iter().copied();
    loop {
        match next(state) {
            Decision::ChooseTargets {
                legal_targets,
                can_finish,
                ..
            } => match picks.next() {
                Some(pick) => {
                    assert!(
                        legal_targets.contains(&pick),
                        "{pick:?} not in {legal_targets:?}"
                    );
                    act(state, Action::ChooseTarget(pick));
                }
                None => {
                    assert!(can_finish);
                    act(state, Action::FinishEffectSelection);
                }
            },
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
}

fn cast_aegis(state: &mut GameState, picks: &[Target]) -> ObjectId {
    let aegis = put(state, P0, "Assimilation Aegis", Zone::Hand);
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 1;
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 2;
    next(state);
    act(state, Action::CastSpell(aegis));
    drive_up_to(state, picks);
    state.players[0].mana_pool = Default::default();
    aegis
}

fn equip_aegis(state: &mut GameState, aegis: ObjectId, creature: ObjectId) {
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 2;
    assert!(activatable(state).contains(&(aegis, 0)));
    act(state, Action::ActivateAbility(aegis, 0));
    drive_up_to(state, &[Target::Object(creature)]);
}

#[test]
fn assimilation_aegis_makes_the_equipped_creature_a_copy_of_the_exiled_card() {
    let mut state = game();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let terror = put(&mut state, P0, "Tolarian Terror", Zone::Battlefield);
    let aegis = cast_aegis(&mut state, &[Target::Object(elves)]);
    assert_eq!(state.objects.get(elves).zone, Zone::Exile);

    equip_aegis(&mut state, aegis, terror);
    let live = state.objects.get(terror);
    assert_eq!(live.name, "Llanowar Elves");
    assert_eq!(live.card_def, state.objects.get(elves).card_def);
    assert_eq!(engine::effective_power(&state, terror), 1);
    assert_eq!(engine::effective_toughness(&state, terror), 1);
    match next(&mut state) {
        Decision::CastSpellOrPass { mana_abilities, .. } => {
            assert!(mana_abilities.contains(&terror))
        }
        other => panic!("unexpected decision: {other:?}"),
    }

    // The Aegis leaving ends the copy and returns the exiled card.
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(aegis, Zone::Graveyard),
    );
    assert_eq!(state.objects.get(terror).name, "Tolarian Terror");
    assert_eq!(
        state.objects.get(elves).zone,
        Zone::Battlefield,
        "return duration ends immediately"
    );
    resolve_stack(&mut state);
    assert_eq!(state.objects.get(terror).name, "Tolarian Terror");
    assert_eq!(engine::effective_power(&state, terror), 5);
    assert_eq!(state.objects.get(elves).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(elves).controller, P1);
}

#[test]
fn assimilation_aegis_copy_ends_when_it_moves_to_another_creature() {
    let mut state = game();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let terror = put(&mut state, P0, "Tolarian Terror", Zone::Battlefield);
    let aegis = cast_aegis(&mut state, &[Target::Object(elves)]);
    equip_aegis(&mut state, aegis, terror);
    let second = put(&mut state, P0, "Tolarian Terror", Zone::Battlefield);
    equip_aegis(&mut state, aegis, second);
    assert_eq!(state.objects.get(terror).name, "Tolarian Terror");
    assert_eq!(engine::effective_toughness(&state, terror), 5);
    assert_eq!(state.objects.get(second).name, "Llanowar Elves");
}

#[test]
fn assimilation_aegis_may_exile_nothing_and_then_copies_nothing() {
    let mut state = game();
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let terror = put(&mut state, P0, "Tolarian Terror", Zone::Battlefield);
    let aegis = cast_aegis(&mut state, &[]);
    assert!(state.players[1].battlefield.len() == 1);
    equip_aegis(&mut state, aegis, terror);
    assert_eq!(state.objects.get(terror).name, "Tolarian Terror");
}

#[test]
fn kellan_granted_trigger_survives_aegis_reversion_and_source_departure_with_frozen_proof() {
    for depart_after_trigger in [false, true] {
        let mut state = game();
        let kellan = put(
            &mut state,
            P1,
            "Kellan, Planar Trailblazer",
            Zone::Battlefield,
        );
        let host = put(&mut state, P0, "Tolarian Terror", Zone::Battlefield);
        let host_definition = state.objects.get(host).card_def;
        let aegis = cast_aegis(&mut state, &[Target::Object(kellan)]);
        equip_aegis(&mut state, aegis, host);
        state.players[0].mana_pool[ManaColor::R.pool_index()] = 2;
        act(&mut state, Action::ActivateAbility(host, 0));
        resolve_stack(&mut state);
        assert!(state
            .objects
            .get(host)
            .v4
            .creature_upgrade
            .as_ref()
            .unwrap()
            .combat_impulse
            .is_some());
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(aegis, Zone::Graveyard),
        );
        resolve_stack(&mut state);
        assert_eq!(state.objects.get(host).card_def, host_definition);

        let generation = state.objects.get(host).zone_change_count;
        event::log_combat_damage_to_player(&mut state, host, generation, P1, 1);
        let pending = mtg_kernel::trigger::collect_and_process(&mut state);
        assert_eq!(
            pending.len(),
            1,
            "the persisted grant must trigger exactly once"
        );
        assert_eq!(
            pending[0].source_contract.unwrap().card_def,
            host_definition
        );
        let grant = pending[0].granted_by.unwrap();
        assert_eq!(grant.source, host);
        assert_eq!(grant.zone_change_count, generation);
        assert_eq!(
            CARD_DEFS[usize::from(grant.card_def)].name,
            "Kellan, Planar Trailblazer"
        );
        state.engine.pending_triggers.extend(pending);
        state = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();

        for wrong_definition in [false, true] {
            let mut altered = state.clone();
            if wrong_definition {
                altered.engine.pending_triggers[0]
                    .granted_by
                    .as_mut()
                    .unwrap()
                    .card_def = host_definition;
            } else {
                altered.engine.pending_triggers[0].granted_by = None;
            }
            assert!(matches!(
                engine::advance_until_decision(&mut altered),
                Decision::Halted { .. }
            ));
        }
        let top = state.players[0].library[0];
        if depart_after_trigger {
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(host, Zone::Hand));
            assert!(state.objects.get(host).v4.creature_upgrade.is_none());
        }
        next(&mut state);
        let item = state.stack.last().unwrap();
        assert_eq!(item.source, host);
        assert_eq!(item.v4.granted_by, Some(grant));
        let mut restored: GameState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        resolve_stack(&mut state);
        resolve_stack(&mut restored);
        assert_eq!(state, restored);
        assert_eq!(state.objects.get(top).zone, Zone::Exile);
        assert_eq!(
            state
                .engine
                .exile_play_permissions
                .iter()
                .filter(|p| p.object == top && p.holder == P0)
                .count(),
            1
        );
        if depart_after_trigger {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(host, Zone::Battlefield),
            );
            let generation = state.objects.get(host).zone_change_count;
            event::log_combat_damage_to_player(&mut state, host, generation, P1, 1);
            assert!(mtg_kernel::trigger::collect_and_process(&mut state).is_empty());
        }
    }
}

#[test]
fn an_aegis_copy_that_dies_reaches_the_graveyard_as_its_own_card() {
    let mut state = game();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let terror = put(&mut state, P0, "Tolarian Terror", Zone::Battlefield);
    let terror_def = state.objects.get(terror).card_def;
    let aegis = cast_aegis(&mut state, &[Target::Object(elves)]);
    equip_aegis(&mut state, aegis, terror);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(aegis, Target::Object(terror), 1),
    );
    resolve_stack(&mut state);
    let live = state.objects.get(terror);
    assert_eq!(live.zone, Zone::Graveyard);
    assert_eq!(
        (live.card_def, live.name.as_str()),
        (terror_def, "Tolarian Terror")
    );
    assert_eq!(state.objects.get(aegis).v4.attached_to, None);
}

#[test]
fn an_aegis_copys_trigger_still_resolves_after_the_copy_ends() {
    let mut state = game();
    let cecil = put(&mut state, P1, "Cecil, Dark Knight", Zone::Battlefield);
    let terror = put(&mut state, P0, "Tolarian Terror", Zone::Battlefield);
    let aegis = cast_aegis(&mut state, &[Target::Object(cecil)]);
    equip_aegis(&mut state, aegis, terror);
    assert_eq!(state.objects.get(terror).name, "Cecil, Dark Knight");

    // Cecil's "you lose that much life" trigger waits on the stack while
    // the Aegis leaves and the copy ends.
    let mut removed = false;
    loop {
        match next(&mut state) {
            Decision::DeclareAttackers { .. } => {
                act(&mut state, Action::DeclareAttackers(vec![terror]))
            }
            Decision::ChooseAttackTarget { .. } => {
                act(&mut state, Action::ChooseAttackTarget(Target::Player(P1)))
            }
            Decision::DeclareBlockers { .. } => {
                act(&mut state, Action::DeclareBlockers(Vec::new()))
            }
            Decision::OrderTriggers { pending, .. } => act(
                &mut state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            Decision::CastSpellOrPass { .. } if !state.stack.is_empty() && !removed => {
                removed = true;
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(aegis, Zone::Graveyard),
                );
            }
            Decision::CastSpellOrPass { .. }
                if matches!(state.step, Step::EndCombat | Step::Main2)
                    && state.stack.is_empty() =>
            {
                break
            }
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert!(removed);
    assert_eq!(state.objects.get(terror).name, "Tolarian Terror");
    assert_eq!(state.players[1].life, 18);
    assert_eq!(state.players[0].life, 18);
    assert_eq!(state.objects.get(cecil).zone, Zone::Battlefield);
}

// ---- Agatha's Soul Cauldron --------------------------------------------------

#[test]
fn agathas_soul_cauldron_grants_an_exiled_creatures_ability_to_countered_creatures() {
    let mut state = game();
    let cauldron = put(&mut state, P0, "Agatha's Soul Cauldron", Zone::Battlefield);
    let terror = put(&mut state, P0, "Tolarian Terror", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let strix = to_graveyard(&mut state, P1, "Harrier Strix");
    assert!(!activatable(&mut state)
        .iter()
        .any(|&(source, _)| source == terror));

    // {T}: exile Harrier Strix; its creature card triggers the counter.
    act(&mut state, Action::ActivateAbility(cauldron, 0));
    drive(&mut state, &[Target::Object(strix), Target::Object(terror)]);
    assert_eq!(state.objects.get(strix).zone, Zone::Exile);
    assert_eq!(state.objects.get(terror).counters.plus1_plus1, 1);
    assert_eq!(state.objects.get(elves).counters.plus1_plus1, 0);

    // "{2}{U}: Draw a card, then discard a card", paid with green mana as
    // though it were blue.
    state.players[0].mana_pool[ManaColor::G.pool_index()] = 3;
    let offered = activatable(&mut state);
    assert!(offered.contains(&(terror, 1)), "{offered:?}");
    assert!(!offered.iter().any(|&(source, _)| source == elves));
    let library = state.players[0].library.len();
    act(&mut state, Action::ActivateAbility(terror, 1));
    loop {
        match next(&mut state) {
            Decision::Discard { choices, .. } => act(&mut state, Action::Discard(vec![choices[0]])),
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert_eq!(state.players[0].library.len(), library - 1);
    assert_eq!(state.players[0].mana_pool[ManaColor::G.pool_index()], 0);

    // Without the Cauldron the grant ends.
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(cauldron, Zone::Graveyard),
    );
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 4;
    assert!(!activatable(&mut state)
        .iter()
        .any(|&(source, _)| source == terror));
}

#[test]
fn agathas_soul_cauldron_exiling_a_noncreature_card_adds_no_counter() {
    let mut state = game();
    let cauldron = put(&mut state, P0, "Agatha's Soul Cauldron", Zone::Battlefield);
    let terror = put(&mut state, P0, "Tolarian Terror", Zone::Battlefield);
    let bolt = to_graveyard(&mut state, P1, "Lightning Bolt");
    act(&mut state, Action::ActivateAbility(cauldron, 0));
    drive(&mut state, &[Target::Object(bolt)]);
    assert_eq!(state.objects.get(bolt).zone, Zone::Exile);
    assert_eq!(state.objects.get(terror).counters.plus1_plus1, 0);
}

#[test]
fn chandra_variable_loyalty_includes_zero_and_values_above_twenty() {
    for x in [0, 21] {
        let mut state = game();
        let chandra = put(&mut state, P0, CHANDRA, Zone::Battlefield);
        change_loyalty(&mut state, chandra, 20);
        next(&mut state);
        act(&mut state, Action::ActivateAbility(chandra, 2));
        assert!(matches!(
            next(&mut state),
            Decision::ChooseEffectOption {
                option_count: 26,
                ..
            }
        ));
        act(&mut state, Action::ChooseEffectOption(x));
        chandra_drive(&mut state, Target::Player(P1), None, &[]);
        assert_eq!(loyalty(&state, chandra), Some(25 - u32::from(x)));
    }
}

#[test]
fn bankbuster_crew_accepts_summoning_sick_pilot_and_ends_at_cleanup() {
    let mut state = game();
    let vehicle = put(&mut state, P0, "Reckoner Bankbuster", Zone::Battlefield);
    let pilot = put(&mut state, P0, "Pilot Token", Zone::Battlefield);
    state.objects.get_mut(pilot).summoning_sick = true;
    assert_eq!(state.objects.get(vehicle).counters.charge, 3);
    assert!(!engine::object_has_type(
        &state,
        vehicle,
        CardType::Creature
    ));
    assert!(activatable(&mut state).contains(&(vehicle, 1)));
    act(&mut state, Action::ActivateAbility(vehicle, 1));
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectTargets {
            can_finish: false,
            ..
        }
    ));
    act(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(pilot)),
    );
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectTargets {
            can_finish: true,
            ..
        }
    ));
    act(&mut state, Action::FinishEffectSelection);
    resolve_stack(&mut state);
    assert!(state.objects.get(pilot).tapped);
    assert!(engine::object_has_type(&state, vehicle, CardType::Creature));
    assert_eq!(engine::effective_power(&state, vehicle), 4);
    for _ in 0..100 {
        if state.step == Step::Untap || state.active_player == P1 {
            break;
        }
        match next(&mut state) {
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            Decision::DeclareAttackers { .. } => act(&mut state, Action::DeclareAttackers(vec![])),
            other => panic!("{other:?}"),
        }
    }
    assert!(!engine::object_has_type(
        &state,
        vehicle,
        CardType::Creature
    ));
}

#[test]
fn bankbuster_last_charge_creates_tokens_even_if_source_leaves_in_response() {
    let mut state = game();
    let vehicle = put(&mut state, P0, "Reckoner Bankbuster", Zone::Battlefield);
    state.objects.get_mut(vehicle).counters.charge = 1;
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 2;
    next(&mut state);
    act(&mut state, Action::ActivateAbility(vehicle, 0));
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(state.objects.get(vehicle).counters.charge, 0);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(vehicle, Zone::Graveyard),
    );
    resolve_stack(&mut state);
    assert_eq!(battlefield_named(&state, P0, "Pilot").len(), 1);
    let treasure_name = CARD_DEFS[card_id_by_name("Treasure Token").unwrap() as usize].object_name;
    assert_eq!(battlefield_named(&state, P0, treasure_name).len(), 1);
}

#[test]
fn a_pilot_without_abilities_cannot_supply_the_extra_crew_power() {
    let mut state = game();
    let vehicle = put(&mut state, P0, "Reckoner Bankbuster", Zone::Battlefield);
    let pilot = put(&mut state, P0, "Pilot Token", Zone::Battlefield);
    let cauldron = put(&mut state, P0, "Agatha's Soul Cauldron", Zone::Battlefield);
    let donor = to_graveyard(&mut state, P0, "Gingerbrute");
    act(&mut state, Action::ActivateAbility(cauldron, 0));
    drive(&mut state, &[Target::Object(donor), Target::Object(pilot)]);
    assert_eq!(engine::effective_power(&state, pilot), 2);
    assert!(activatable(&mut state).contains(&(vehicle, 1)));
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 1;
    let granted = activatable(&mut state)
        .into_iter()
        .find(|(source, _)| *source == pilot)
        .expect("Pilot has Gingerbrute's nonmana activated ability");
    act(&mut state, Action::ActivateAbility(granted.0, granted.1));
    next(&mut state);
    let ability = state.stack.last().unwrap().v4.stack_item_id;
    let tidebinder = put(&mut state, P0, "Tishana's Tidebinder", Zone::Hand);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 3;
    act(&mut state, Action::CastSpell(tidebinder));
    drive(&mut state, &[Target::StackItem(ability)]);
    event::propose_and_commit(&mut state, ProposedEvent::tap(tidebinder));
    assert_eq!(engine::effective_power(&state, pilot), 2);
    assert!(!state.objects.get(pilot).tapped);
    assert!(!activatable(&mut state).contains(&(vehicle, 1)));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(tidebinder, Zone::Graveyard),
    );
    assert!(activatable(&mut state).contains(&(vehicle, 1)));
}

#[test]
fn etali_captures_each_casts_triggers_but_defers_placement_and_sbas() {
    // An Etali entering without a cast and an Etali following one prior cast
    // exercise both duplicate-trigger and missed-trigger failure modes.
    for prior_casts in [0, 1] {
        let mut state = game();
        let cutter = put(&mut state, P0, "Cori-Steel Cutter", Zone::Battlefield);
        let jodah = put(&mut state, P0, "Jodah, the Unifier", Zone::Battlefield);
        let reliquary = put(&mut state, P0, "Dusk Rose Reliquary", Zone::Hand);
        let elves = put(&mut state, P1, "Llanowar Elves", Zone::Hand);
        for card in [reliquary, elves] {
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(card, Zone::Library));
        }
        let etali = put(&mut state, P0, "Etali, Primal Conqueror", Zone::Battlefield);
        state.players[0].spells_cast_this_turn = prior_casts;
        // Jodah currently gives Etali +2/+2. Sacrificing Jodah for the first
        // free cast makes this damage lethal, but SBAs must wait for Etali's
        // entire ability, including its second casting instruction, to finish.
        assert_eq!(engine::effective_toughness(&state, etali), 9);
        state.objects.get_mut(etali).damage = 7;
        for _ in 0..30 {
            match next(&mut state) {
                Decision::ChooseEffectTargets {
                    source,
                    ref legal_targets,
                    ..
                } => {
                    assert_eq!(source, etali);
                    assert!(legal_targets.contains(&Target::Object(reliquary)));
                    assert!(legal_targets.contains(&Target::Object(elves)));
                    break;
                }
                Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
                other => panic!("Etali first free cast: {other:?}"),
            }
        }
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(reliquary)),
        );
        assert!(
            matches!(next(&mut state), Decision::ChooseCostTargets { ref candidates, .. }
            if candidates.contains(&jodah))
        );
        act(&mut state, Action::ChooseCostTarget(jodah));
        assert!(
            matches!(next(&mut state), Decision::ChooseEffectTargets { source, ref legal_targets, .. }
            if source == etali && legal_targets == &[Target::Object(elves)])
        );
        assert_eq!(state.players[0].spells_cast_this_turn, prior_casts + 1);
        assert_eq!(state.objects.get(jodah).zone, Zone::Graveyard);
        assert_eq!(engine::effective_toughness(&state, etali), 7);
        assert_eq!(state.objects.get(etali).zone, Zone::Battlefield);
        assert!(state.stack.iter().any(|item| item.source == etali));
        assert!(!state.stack.iter().any(|item| item.source == cutter));
        assert!(state.engine.pending_triggers.is_empty());
        state = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(elves)),
        );
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert_eq!(state.players[0].spells_cast_this_turn, prior_casts + 2);
        assert_eq!(state.objects.get(etali).zone, Zone::Graveyard);
        assert!(state.engine.pending_effect.is_none());
        assert!(!state.stack.iter().any(|item| item.source == etali));
        assert_eq!(
            state
                .stack
                .iter()
                .filter(|item| item.source == cutter)
                .count(),
            1
        );
        assert_eq!(state.stack.last().unwrap().source, cutter);
        assert_eq!(state.objects.get(reliquary).zone, Zone::Stack);
        assert_eq!(state.objects.get(elves).zone, Zone::Stack);
    }
}

#[test]
fn etali_free_casts_opponents_spell_then_transforms_and_poison_uses_damage_lki() {
    let mut state = game();
    let foreign = put(&mut state, P1, "Llanowar Elves", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(foreign, Zone::Library),
    );
    let etali = put(&mut state, P0, "Etali, Primal Conqueror", Zone::Battlefield);
    let mut offered = false;
    for _ in 0..30 {
        match next(&mut state) {
            Decision::ChooseEffectTargets {
                ref legal_targets, ..
            } => {
                assert!(legal_targets.contains(&Target::Object(foreign)));
                act(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(foreign)),
                );
                offered = true;
            }
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => act(
                &mut state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            other => panic!("Etali free cast: {other:?}"),
        }
    }
    assert!(offered);
    assert_eq!(state.objects.get(foreign).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(foreign).controller, P0);
    assert_eq!(state.objects.get(foreign).owner, P1);
    assert!(state.players[P0.index()].battlefield.contains(&foreign));
    assert!(!state.players[P1.index()].battlefield.contains(&foreign));
    state.players[0].mana_pool[ManaColor::G.pool_index()] = 10;
    act(&mut state, Action::ActivateAbility(etali, 0));
    resolve_stack(&mut state);
    assert_eq!(state.objects.get(etali).v4.face_index, 1);
    assert_eq!(engine::effective_power(&state, etali), 11);
    assert!(engine::has_effective_keyword(
        &state,
        etali,
        Keywords::INDESTRUCTIBLE
    ));
    let generation = state.objects.get(etali).zone_change_count;
    event::log_combat_damage_to_player(&mut state, etali, generation, P1, 4);
    next(&mut state);
    assert_eq!(
        state.players[1].poison_counters.0, 0,
        "poison is a triggered ability"
    );
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(etali, Zone::Exile));
    resolve_stack(&mut state);
    assert_eq!(state.players[1].poison_counters.0, 4);
}

#[test]
fn smithy_taps_exactly_five_and_barracks_tracks_floating_mana_after_untap() {
    let mut state = game();
    let smithy = put(&mut state, P0, "Thousand Moons Smithy", Zone::Battlefield);
    resolve_stack(&mut state);
    let gnome = battlefield_named(&state, P0, "Gnome Soldier")[0];
    assert_eq!(engine::effective_power(&state, gnome), 2);
    assert_eq!(engine::effective_toughness(&state, gnome), 2);
    let mut pay = vec![smithy, gnome];
    for _ in 0..3 {
        pay.push(put(
            &mut state,
            P0,
            "Quirion Beastcaller",
            Zone::Battlefield,
        ));
    }
    let marker = mtg_kernel::event::CommittedEvent::BeginningPrecombatMainV1 { active_player: P0 };
    state.engine.event_log.push(marker.clone());
    state.engine.event_history.push(marker);
    loop {
        match next(&mut state) {
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            Decision::ChooseEffectTargets {
                ref legal_targets, ..
            } => {
                assert!(legal_targets.contains(&Target::Object(smithy)));
                break;
            }
            other => panic!("Smithy transform {other:?}"),
        }
    }
    act(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(smithy)),
    );
    for id in &pay[1..] {
        if state
            .engine
            .pending_effect
            .as_ref()
            .is_some_and(|p| p.choice.is_some())
        {
            assert!(engine::step(&mut state, Action::FinishEffectSelection).is_err());
            act(&mut state, Action::ChooseEffectTarget(Target::Object(*id)));
        }
    }
    resolve_stack(&mut state);
    assert_eq!(state.objects.get(smithy).v4.face_index, 1);
    assert!(pay.iter().all(|id| state.objects.get(*id).tapped));
    mtg_kernel::effect::execute(
        &mtg_kernel::effect::EffectOp::UntapObject {
            object: mtg_kernel::effect::ObjectRef::ThisSource,
        },
        &mtg_kernel::effect::ExecCtx::no_targets(smithy, P0),
        &mut state,
    );
    act(&mut state, Action::ActivateManaAbility(smithy));
    assert_eq!(state.players[0].restricted_mana_pool.0.len(), 1);
    let recruit = put(&mut state, P0, "Recruitment Officer", Zone::Hand);
    act(&mut state, Action::CastSpell(recruit));
    resolve_stack(&mut state);
    assert_eq!(battlefield_named(&state, P0, "Gnome Soldier").len(), 2);
    assert!(state.players[0].restricted_mana_pool.0.is_empty());
}

#[test]
fn barracks_checks_the_cast_adventure_form_for_direct_and_floating_mana() {
    for floating in [false, true] {
        for adventure in [false, true] {
            let mut state = game();
            let smithy = put(&mut state, P0, "Thousand Moons Smithy", Zone::Battlefield);
            resolve_stack(&mut state);
            let mut ctx = mtg_kernel::effect::ExecCtx::no_targets(smithy, P0);
            ctx.ability_source_contract = Some(
                mtg_kernel::state::AbilitySourceContractV4::capture(&state, smithy),
            );
            mtg_kernel::effect::execute(
                &mtg_kernel::effect::EffectOp::TransformSourceInPlace,
                &ctx,
                &mut state,
            );
            assert_eq!(state.objects.get(smithy).v4.face_index, 1);
            let recruiter = put(&mut state, P0, "Imodane's Recruiter", Zone::Hand);
            state.players[0].mana_pool[ManaColor::C.pool_index()] = if adventure { 4 } else { 1 };
            state.players[0].mana_pool[ManaColor::R.pool_index()] = u8::from(!adventure);
            next(&mut state);
            if floating {
                act(&mut state, Action::ActivateManaAbility(smithy));
                assert_eq!(state.players[0].restricted_mana_pool.0.len(), 1);
            }
            act(&mut state, Action::CastSpell(recruiter));
            if let Decision::ChooseSpellMode { legal_modes, .. } = next(&mut state) {
                let form = u8::from(adventure);
                assert!(legal_modes.contains(&form));
                act(&mut state, Action::ChooseSpellMode(form));
            }
            resolve_stack(&mut state);
            assert!(state.objects.get(smithy).tapped, "Barracks mana was spent");
            assert!(state.players[0].restricted_mana_pool.0.is_empty());
            assert_eq!(
                battlefield_named(&state, P0, "Gnome Soldier").len(),
                if adventure { 1 } else { 2 },
                "floating={floating}, adventure={adventure}"
            );
            assert_eq!(
                state.objects.get(recruiter).zone,
                if adventure {
                    Zone::Exile
                } else {
                    Zone::Battlefield
                }
            );
        }
    }
}

#[test]
fn twilight_target_admission_uses_cost_reduction_and_taxes() {
    let mut reduced = game();
    put(&mut reduced, P0, "Haughty Djinn", Zone::Battlefield);
    let elves = put(&mut reduced, P1, "Llanowar Elves", Zone::Battlefield);
    reduced.players[0].mana_pool[ManaColor::U.pool_index()] = 2;
    cast_twilight(&mut reduced, elves, 1);
    assert_eq!(reduced.objects.get(elves).controller, P0);

    let mut taxed = game();
    put(
        &mut taxed,
        P1,
        "Thalia, Guardian of Thraben",
        Zone::Battlefield,
    );
    put(&mut taxed, P1, "Llanowar Elves", Zone::Battlefield);
    let spell = put(&mut taxed, P0, "Blue Sun's Twilight", Zone::Hand);
    taxed.players[0].mana_pool[ManaColor::U.pool_index()] = 3;
    match next(&mut taxed) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => assert!(!castable_spells.contains(&spell)),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn cauldron_exposes_all_gwenna_pairs_and_resolves_granted_mana_immediately() {
    let mut state = game();
    let cauldron = put(&mut state, P0, "Agatha's Soul Cauldron", Zone::Battlefield);
    let host = put(&mut state, P0, "Tolarian Terror", Zone::Battlefield);
    let donor = to_graveyard(&mut state, P0, "Gwenna, Eyes of Gaea");
    act(&mut state, Action::ActivateAbility(cauldron, 0));
    drive(&mut state, &[Target::Object(donor), Target::Object(host)]);
    let choices: Vec<_> = activatable(&mut state)
        .into_iter()
        .filter(|(id, _)| *id == host)
        .collect();
    assert_eq!(choices.len(), 15);
    assert!(
        choices.contains(&(host, 15)),
        "the fifteenth color pair is offered"
    );
    act(&mut state, Action::ActivateAbility(host, 15));
    assert!(state.stack.is_empty());
    assert!(state.objects.get(host).tapped);
    let mana = &state.players[0].restricted_mana_pool.0;
    assert_eq!(mana.len(), 2);
    assert!(mana.iter().all(|unit| unit.color == ManaColor::G
        && unit.restriction
            == mtg_kernel::card_def::ManaSpendRestrictionDef::CreatureSpellOrCreatureAbility));
}

#[test]
fn cauldron_granted_sacrifice_ability_keeps_its_frozen_identity() {
    let mut state = game();
    let cauldron = put(&mut state, P0, "Agatha's Soul Cauldron", Zone::Battlefield);
    let host = put(&mut state, P0, "Tolarian Terror", Zone::Battlefield);
    let donor = to_graveyard(&mut state, P0, "Bloodtithe Harvester");
    let victim = put(&mut state, P1, "Gingerbrute", Zone::Battlefield);
    put(&mut state, P0, "Blood Token", Zone::Battlefield);
    act(&mut state, Action::ActivateAbility(cauldron, 0));
    drive(&mut state, &[Target::Object(donor), Target::Object(host)]);
    act(&mut state, Action::ActivateAbility(host, 1));
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    act(&mut state, Action::ChooseTarget(Target::Object(victim)));
    next(&mut state);
    assert_eq!(state.objects.get(host).zone, Zone::Graveyard);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(cauldron, Zone::Graveyard),
    );
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(donor, Zone::Graveyard),
    );
    let serialized = serde_json::to_string(&state).unwrap();
    state = serde_json::from_str(&serialized).unwrap();
    let observation = mtg_kernel::rl::observe_v2(
        &state,
        &mtg_kernel::surface_v2::HarnessSurfaceV2::new(),
        P0,
        0,
    )
    .unwrap();
    let grant = observation
        .projection
        .stack
        .last()
        .unwrap()
        .granted_ability
        .as_ref()
        .expect("public stack retains the granted ability's frozen donor");
    assert_eq!(grant.0.arena_id, donor.0);
    assert_eq!(grant.0.zone, Zone::Exile);
    assert_ne!(
        grant.0.zone_change_count,
        state.objects.get(donor).zone_change_count
    );
    assert_eq!(grant.1, 0);
    resolve_stack(&mut state);
    assert_eq!(state.objects.get(victim).zone, Zone::Graveyard);
}

#[test]
fn cauldron_grants_legacy_mana_and_preserves_donor_activation_conditions() {
    let mut state = game();
    let cauldron = put(&mut state, P0, "Agatha's Soul Cauldron", Zone::Battlefield);
    let host = put(&mut state, P0, "Tolarian Terror", Zone::Battlefield);
    let elves = to_graveyard(&mut state, P0, "Llanowar Elves");
    act(&mut state, Action::ActivateAbility(cauldron, 0));
    drive(&mut state, &[Target::Object(elves), Target::Object(host)]);
    act(&mut state, Action::ActivateAbility(host, 1));
    assert!(state.stack.is_empty());
    assert_eq!(state.players[0].mana_pool[ManaColor::G.pool_index()], 1);
    state.objects.get_mut(host).tapped = false;
    state.objects.get_mut(cauldron).tapped = false;
    let surge = to_graveyard(&mut state, P0, "Surge Engine");
    act(&mut state, Action::ActivateAbility(cauldron, 0));
    drive(&mut state, &[Target::Object(surge), Target::Object(host)]);
    state.players[0].mana_pool[ManaColor::G.pool_index()] = 20;
    // The host is already blue and lacks defender, so both restrictions
    // inspect its characteristics rather than the exiled card's.
    let choices = activatable(&mut state);
    assert!(choices.contains(&(host, 3)) && choices.contains(&(host, 4)));
    let before = state.players[0].library.len();
    act(&mut state, Action::ActivateAbility(host, 4));
    resolve_stack(&mut state);
    assert_eq!(state.players[0].library.len(), before - 3);
    assert!(!activatable(&mut state).contains(&(host, 4)));
    state.turn += 1;
    assert!(
        !activatable(&mut state).contains(&(host, 4)),
        "once only persists across turns"
    );
}
#[test]
fn innkeeper_doubles_oil_and_poison_placed_by_its_controller() {
    use mtg_kernel::effect::{EffectObjectBinding, EffectOp, ExecCtx};
    use mtg_kernel::standard_cards_v1::StandardOpV1;
    let mut state = game();
    let class = put(&mut state, P0, "Innkeeper's Talent", Zone::Battlefield);
    level_up(&mut state, class, 0, ManaColor::G, 1);
    level_up(&mut state, class, 1, ManaColor::G, 4);
    let adaptive = put(&mut state, P0, "Evolving Adaptive", Zone::Battlefield);
    assert_eq!(state.objects.get(adaptive).counters.oil, 2);
    let binding = EffectObjectBinding {
        object: adaptive,
        expected_zone: Zone::Battlefield,
        expected_zone_change_count: state.objects.get(adaptive).zone_change_count,
    };
    let ctx = ExecCtx::no_targets(adaptive, P0);
    mtg_kernel::effect::execute(
        &EffectOp::PutOilCounterOnBoundObject { object: binding },
        &ctx,
        &mut state,
    );
    assert_eq!(state.objects.get(adaptive).counters.oil, 4);
    mtg_kernel::effect::execute(
        &EffectOp::StandardV1(StandardOpV1::EtaliPoison {
            player: P1,
            amount: 2,
        }),
        &ctx,
        &mut state,
    );
    assert_eq!(state.players[1].poison_counters.0, 4);
    let opponent_ctx = ExecCtx::no_targets(adaptive, P1);
    mtg_kernel::effect::execute(
        &EffectOp::StandardV1(StandardOpV1::EtaliPoison {
            player: P0,
            amount: 2,
        }),
        &opponent_ctx,
        &mut state,
    );
    assert_eq!(
        state.players[0].poison_counters.0, 2,
        "recipient's Talent does not double an opponent's placement"
    );
}

fn stop_with_aegis_copy_trigger(state: &mut GameState, aegis: ObjectId, creature: ObjectId) {
    use mtg_kernel::effect::EffectOp;
    use mtg_kernel::standard_creature_choices_v1::CreatureChoiceV1;
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 2;
    act(state, Action::ActivateAbility(aegis, 0));
    let mut targeted = false;
    loop {
        match next(state) {
            Decision::ChooseTargets { .. } if !targeted => {
                act(state, Action::ChooseTarget(Target::Object(creature)));
                targeted = true;
            }
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.iter().any(|item| {
                    matches!(
                        item.inline_effect,
                        Some(EffectOp::CreatureChoiceV1(
                            CreatureChoiceV1::AegisCopy { .. }
                        ))
                    )
                }) =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => panic!("unexpected Aegis decision {other:?}"),
        }
    }
}

#[test]
fn aegis_waits_for_its_copy_trigger_and_does_nothing_after_detaching() {
    let mut state = game();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let host = put(&mut state, P0, "Tolarian Terror", Zone::Battlefield);
    let aegis = cast_aegis(&mut state, &[Target::Object(elves)]);
    stop_with_aegis_copy_trigger(&mut state, aegis, host);
    assert_eq!(state.objects.get(host).name, "Tolarian Terror");
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(aegis, Zone::Hand));
    resolve_stack(&mut state);
    assert_eq!(state.objects.get(host).name, "Tolarian Terror");
    assert_eq!(state.objects.get(elves).zone, Zone::Battlefield);
}

#[test]
fn copied_essence_channeler_dies_with_its_counter_transfer_ability() {
    let mut state = game();
    let channeler = put(&mut state, P1, "Essence Channeler", Zone::Battlefield);
    let host = put(&mut state, P0, "Tolarian Terror", Zone::Battlefield);
    let recipient = put(&mut state, P0, "Gingerbrute", Zone::Battlefield);
    let aegis = cast_aegis(&mut state, &[Target::Object(channeler)]);
    equip_aegis(&mut state, aegis, host);
    state.objects.get_mut(host).counters.plus1_plus1 = 3;
    state.objects.get_mut(host).counters.oil = 2;
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(host, Zone::Graveyard),
    );
    assert_eq!(state.objects.get(host).name, "Tolarian Terror");
    let serialized = serde_json::to_string(&state).unwrap();
    state = serde_json::from_str(&serialized).unwrap();
    drive(&mut state, &[Target::Object(recipient)]);
    assert_eq!(state.objects.get(recipient).counters.plus1_plus1, 3);
    assert_eq!(state.objects.get(recipient).counters.oil, 2);
    assert_eq!(state.objects.get(channeler).zone, Zone::Exile);
}

#[test]
fn aegis_controller_chooses_among_exiled_cards_at_resolution() {
    let mut state = game();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let host = put(&mut state, P0, "Tolarian Terror", Zone::Battlefield);
    let aegis = cast_aegis(&mut state, &[Target::Object(elves)]);
    let channeler = put(&mut state, P1, "Essence Channeler", Zone::Battlefield);
    let mut ctx = mtg_kernel::effect::ExecCtx::no_targets(aegis, P0);
    ctx.ability_source_contract = Some(mtg_kernel::state::AbilitySourceContractV4::capture(
        &state, aegis,
    ));
    ctx.targets = vec![Target::Object(channeler)];
    ctx.target_contracts = vec![mtg_kernel::state::StackTargetContractV4::capture(
        &state,
        Target::Object(channeler),
    )];
    mtg_kernel::effect::execute(
        &mtg_kernel::effect::EffectOp::ExileTargetLinkedToSource {
            object: mtg_kernel::effect::ObjectRef::Target(0),
        },
        &ctx,
        &mut state,
    );
    assert_eq!(state.engine.linked_exile_records.len(), 2);
    assert!(state.engine.halted.is_none());
    stop_with_aegis_copy_trigger(&mut state, aegis, host);
    loop {
        match next(&mut state) {
            Decision::ChooseEffectOption { option_count, .. } => {
                assert_eq!(option_count, 2, "mandatory copy has no decline option");
                let serialized = serde_json::to_string(&state).unwrap();
                state = serde_json::from_str(&serialized).unwrap();
                act(&mut state, Action::ChooseEffectOption(1));
                break;
            }
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected copy decision {other:?}"),
        }
    }
    resolve_stack(&mut state);
    assert_eq!(state.objects.get(host).name, "Essence Channeler");
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(aegis, Zone::Graveyard),
    );
    assert_eq!(state.objects.get(elves).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(channeler).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(host).name, "Tolarian Terror");
}

#[test]
fn aegis_leaving_before_its_etb_resolves_does_not_exile_the_target() {
    let mut state = game();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let aegis = put(&mut state, P0, "Assimilation Aegis", Zone::Battlefield);
    loop {
        match next(&mut state) {
            Decision::ChooseTargets { .. } => {
                act(&mut state, Action::ChooseTarget(Target::Object(elves)));
                break;
            }
            Decision::OrderTriggers { pending, .. } => act(
                &mut state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            other => panic!("unexpected ETB decision {other:?}"),
        }
    }
    next(&mut state);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(aegis, Zone::Graveyard),
    );
    resolve_stack(&mut state);
    assert_eq!(state.objects.get(elves).zone, Zone::Battlefield);
}

#[test]
fn aegis_copied_activations_freeze_the_printed_ability_before_sacrificing_the_host() {
    // Cover both immediate payment and payment resumed after choosing a discard.
    for (name, index) in [("Gingerbrute", 1), ("Masked Meower", 0)] {
        let mut state = game();
        let donor = put(&mut state, P1, name, Zone::Battlefield);
        let donor_definition = state.objects.get(donor).card_def;
        let host = put(&mut state, P0, "Tolarian Terror", Zone::Battlefield);
        let host_definition = state.objects.get(host).card_def;
        let aegis = cast_aegis(&mut state, &[Target::Object(donor)]);
        equip_aegis(&mut state, aegis, host);
        let source_generation = state.objects.get(host).zone_change_count;
        let pitch = put(&mut state, P0, "Forest", Zone::Hand);
        // Two candidates keep the resumed discard-payment path a real choice.
        let keep = put(&mut state, P0, "Island", Zone::Hand);
        let before_life = state.players[0].life;
        let before_library = state.players[0].library.len();
        state.players[0].mana_pool[ManaColor::U.pool_index()] = 2;
        act(&mut state, Action::ActivateAbility(host, index));
        if name == "Masked Meower" {
            assert!(matches!(next(&mut state), Decision::Discard { .. }));
            act(&mut state, Action::Discard(vec![pitch]));
        }
        next(&mut state);
        assert_eq!(state.objects.get(host).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(host).card_def, host_definition);
        let ability = state.stack.last().unwrap();
        assert_eq!(ability.v4.activated_ability_index, Some(index));
        let source = ability.v4.ability_source_contract.unwrap();
        assert_eq!(source.card_def, donor_definition);
        assert_eq!(source.zone_change_count, source_generation);
        let mut restored: GameState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        resolve_stack(&mut state);
        resolve_stack(&mut restored);
        assert_eq!(state, restored);
        if name == "Gingerbrute" {
            assert_eq!(state.players[0].life, before_life + 3);
        } else {
            assert_eq!(state.objects.get(pitch).zone, Zone::Graveyard);
            assert_eq!(state.objects.get(keep).zone, Zone::Hand);
            assert_eq!(state.players[0].library.len(), before_library - 1);
        }
    }
}

#[test]
fn cauldron_borrowed_kellan_upgrade_retains_the_donor_after_all_sources_leave() {
    for host_leaves in [false, true] {
        let mut state = game();
        let cauldron = put(&mut state, P0, "Agatha's Soul Cauldron", Zone::Battlefield);
        let host = put(&mut state, P0, "Gwenna, Eyes of Gaea", Zone::Battlefield);
        let host_definition = state.objects.get(host).card_def;
        let donor = to_graveyard(&mut state, P0, "Kellan, Planar Trailblazer");
        act(&mut state, Action::ActivateAbility(cauldron, 0));
        drive(&mut state, &[Target::Object(donor), Target::Object(host)]);
        let borrowed_index = (CARD_DEFS[usize::from(host_definition)]
            .activated_abilities
            .len()
            + 1) as u8;
        state.players[0].mana_pool[ManaColor::R.pool_index()] = 2;
        act(&mut state, Action::ActivateAbility(host, borrowed_index));
        next(&mut state);
        let grant = state.stack.last().unwrap().v4.cauldron_grant.0.unwrap();
        assert_eq!(grant.donor.source, donor);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(cauldron, Zone::Graveyard),
        );
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(donor, Zone::Graveyard),
        );
        state = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        resolve_stack(&mut state);
        let upgrade = state
            .objects
            .get(host)
            .v4
            .creature_upgrade
            .as_ref()
            .unwrap();
        assert_eq!(upgrade.combat_impulse_source, Some(grant.host));
        assert_eq!(upgrade.combat_impulse_donor, Some(grant.donor));
        let generation = state.objects.get(host).zone_change_count;
        event::log_combat_damage_to_player(&mut state, host, generation, P1, 1);
        let pending = mtg_kernel::trigger::collect_and_process(&mut state);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].granted_by, Some(grant.donor));
        assert_eq!(pending[0].source_contract.unwrap().source, host);
        state.engine.pending_triggers.extend(pending);
        state = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        for change_host in [false, true] {
            let mut altered = state.clone();
            if change_host {
                altered.engine.pending_triggers[0]
                    .source_contract
                    .as_mut()
                    .unwrap()
                    .zone_change_count += 1;
            } else {
                altered.engine.pending_triggers[0]
                    .granted_by
                    .as_mut()
                    .unwrap()
                    .card_def = host_definition;
            }
            assert!(matches!(
                engine::advance_until_decision(&mut altered),
                Decision::Halted { .. }
            ));
        }
        let top = state.players[0].library[0];
        if host_leaves {
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(host, Zone::Hand));
        }
        next(&mut state);
        assert_eq!(state.stack.last().unwrap().v4.granted_by, Some(grant.donor));
        let mut restored: GameState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        resolve_stack(&mut state);
        resolve_stack(&mut restored);
        assert_eq!(state, restored);
        assert_eq!(state.objects.get(top).zone, Zone::Exile);
        if host_leaves {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(host, Zone::Battlefield),
            );
            let generation = state.objects.get(host).zone_change_count;
            event::log_combat_damage_to_player(&mut state, host, generation, P1, 1);
            assert!(mtg_kernel::trigger::collect_and_process(&mut state).is_empty());
        }
    }
}
