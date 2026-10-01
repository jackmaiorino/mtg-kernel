//! Batch B rules, exact tokens and pending-trigger incarnation behavior.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::effect::{self, EffectOp, ExecCtx, PlayerRef};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Zone};
use mtg_kernel::surface_v2::{
    HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SurfaceAction, SurfaceDecision,
};
use mtg_kernel::trigger;

fn ready() -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state =
        GameState::new_from_libraries(&[forest; 40], &[forest; 40], |_| "Forest".into(), 123);
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: true,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        _ => panic!("helper zone"),
    }
    id
}

fn move_to(state: &mut GameState, object: ObjectId, zone: Zone) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, zone));
}

fn enter(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let id = put(state, player, name, Zone::Hand);
    move_to(state, id, Zone::Battlefield);
    queue(state);
    id
}

fn queue(state: &mut GameState) {
    let triggers = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(triggers);
}

fn surface() -> HarnessSurfaceV2 {
    HarnessSurfaceV2::new_with_priority_mode_v1(
        PriorityModeV1::EngineWindowsV1,
        SuppressionAuditMode::Full,
    )
}

fn next(surface: &mut HarnessSurfaceV2, state: &mut GameState) -> Decision {
    match surface.next_decision(state) {
        SurfaceDecision::Decision(decision) => decision,
        other => panic!("expected decision, got {other:?}"),
    }
}

fn apply(surface: &mut HarnessSurfaceV2, state: &mut GameState, action: Action) {
    surface.apply(state, SurfaceAction::Action(action)).unwrap();
}

fn drain(surface: &mut HarnessSurfaceV2, state: &mut GameState) {
    for _ in 0..100 {
        match next(surface, state) {
            Decision::OrderTriggers { pending, .. } => apply(
                surface,
                state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => {
                assert!(state.engine.pending_triggers.is_empty());
                assert!(surface.suppressions().is_empty());
                return;
            }
            Decision::CastSpellOrPass { .. } => apply(surface, state, Action::Pass),
            other => panic!("unexpected batch B choice: {other:?}"),
        }
    }
    panic!("trigger drain did not finish");
}

fn draw(state: &mut GameState, source: ObjectId, controller: PlayerId, count: u32) {
    let ctx = ExecCtx {
        stack_item_id: None,
        source,
        controller,
        targets: Vec::new(),
        target_contracts: Vec::new(),
        discarded: Vec::new(),
        paid_cost_refs: Vec::new(),
        hidden_ability_source: None,
        ability_source_contract: None,
        kicked: false,
        optional_additional_cost_paid: None,
        x_value: 0,
    };
    effect::execute(
        &EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count,
        },
        &ctx,
        state,
    );
    queue(state);
}

fn tokens(state: &GameState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|id| state.objects.get(*id).card_def == card_id_by_name(name).unwrap())
        .collect()
}

#[test]
fn batch_b_definitions_append_and_tokens_have_exact_characteristics() {
    for (offset, name) in [
        "Blossoming Sands",
        "Thornwood Falls",
        "Dazzling Angel",
        "Clinquant Skymage",
        "Dwynen's Elite",
        "Good-Fortune Unicorn",
        "Guarded Heir",
        "Youthful Valkyrie",
        "Elf Warrior Token",
        "Knight Token",
    ]
    .into_iter()
    .enumerate()
    {
        let id = card_id_by_name(name).unwrap();
        assert_eq!(usize::from(id), 168 + offset);
        assert_eq!(CARD_DEFS[usize::from(id)].capability, CardCapability::Full);
        assert_eq!(CARD_DEFS[usize::from(id)].is_token, offset >= 8);
    }
    for (name, stats, color, subtypes) in [
        (
            "Elf Warrior Token",
            (1, 1),
            ManaColor::G,
            vec![Subtype::Elf, Subtype::Warrior],
        ),
        ("Knight Token", (3, 3), ManaColor::W, vec![Subtype::Knight]),
    ] {
        let mut state = ready();
        let token = put(&mut state, PlayerId::P0, name, Zone::Battlefield);
        let def = &CARD_DEFS[usize::from(state.objects.get(token).card_def)];
        assert_eq!(
            (
                engine::effective_power(&state, token),
                engine::effective_toughness(&state, token)
            ),
            stats
        );
        assert_eq!(def.colors, &[color]);
        assert_eq!(def.subtypes, subtypes);
        assert_eq!(def.keywords, Keywords::NONE);
        assert!(state.objects.get(token).summoning_sick);
    }
}

#[test]
fn dual_lands_enter_tapped_gain_life_on_resolution_and_offer_each_color() {
    for (name, colors) in [
        ("Blossoming Sands", [ManaColor::G, ManaColor::W]),
        ("Thornwood Falls", [ManaColor::G, ManaColor::U]),
    ] {
        for color in colors {
            let mut state = ready();
            let land = put(&mut state, PlayerId::P0, name, Zone::Hand);
            let mut surface = surface();
            next(&mut surface, &mut state);
            apply(&mut surface, &mut state, Action::PlayLand(land));
            assert!(state.objects.get(land).tapped);
            assert_eq!(state.players[0].life, 20);
            assert!(
                engine::step(&mut state, Action::ActivateManaAbilityChoice(land, color)).is_err()
            );
            drain(&mut surface, &mut state);
            assert_eq!(state.players[0].life, 21);
            state.objects.get_mut(land).tapped = false;
            next(&mut surface, &mut state);
            apply(
                &mut surface,
                &mut state,
                Action::ActivateManaAbilityChoice(land, color),
            );
            assert_eq!(state.players[0].mana_pool[color.pool_index()], 1);
            assert!(state.objects.get(land).tapped);
            assert!(state.stack.is_empty());
        }
    }
}

#[test]
fn land_life_trigger_survives_the_land_leaving() {
    let mut state = ready();
    let land = enter(&mut state, PlayerId::P0, "Blossoming Sands");
    let mut surface = surface();
    next(&mut surface, &mut state);
    move_to(&mut state, land, Zone::Graveyard);
    drain(&mut surface, &mut state);
    assert_eq!(state.players[0].life, 21);
}

#[test]
fn ordinary_batch_b_creatures_pay_exact_costs_and_enter_with_printed_stats() {
    for (name, mana, stats, keyword) in [
        (
            "Dazzling Angel",
            [1, 0, 0, 0, 0, 2],
            (2, 3),
            Keywords::FLYING,
        ),
        (
            "Clinquant Skymage",
            [0, 1, 0, 0, 0, 3],
            (1, 1),
            Keywords::FLYING,
        ),
        ("Dwynen's Elite", [0, 0, 0, 0, 1, 1], (2, 2), Keywords::NONE),
        (
            "Good-Fortune Unicorn",
            [1, 0, 0, 0, 1, 1],
            (2, 2),
            Keywords::NONE,
        ),
        (
            "Guarded Heir",
            [1, 0, 0, 0, 0, 5],
            (1, 1),
            Keywords::LIFELINK,
        ),
        (
            "Youthful Valkyrie",
            [1, 0, 0, 0, 0, 1],
            (1, 3),
            Keywords::FLYING,
        ),
    ] {
        let mut state = ready();
        let creature = put(&mut state, PlayerId::P0, name, Zone::Hand);
        assert!(engine::step(&mut state, Action::CastSpell(creature)).is_err());
        state.players[0].mana_pool = mana;
        let mut surface = surface();
        next(&mut surface, &mut state);
        apply(&mut surface, &mut state, Action::CastSpell(creature));
        next(&mut surface, &mut state);
        assert_eq!(state.players[0].mana_pool, [0; 6]);
        drain(&mut surface, &mut state);
        assert_eq!(state.objects.get(creature).zone, Zone::Battlefield);
        assert_eq!(
            (
                engine::effective_power(&state, creature),
                engine::effective_toughness(&state, creature)
            ),
            stats
        );
        assert_eq!(
            CARD_DEFS[usize::from(state.objects.get(creature).card_def)].keywords,
            keyword
        );
        assert!(state.objects.get(creature).summoning_sick);
    }
}

#[test]
fn dazzling_angel_counts_other_friendly_creatures_only() {
    let mut state = ready();
    let angel = enter(&mut state, PlayerId::P0, "Dazzling Angel");
    assert!(state.engine.pending_triggers.is_empty());
    enter(&mut state, PlayerId::P0, "Forest");
    enter(&mut state, PlayerId::P1, "Healer's Hawk");
    assert!(state.engine.pending_triggers.is_empty());
    enter(&mut state, PlayerId::P0, "Healer's Hawk");
    assert_eq!(state.engine.pending_triggers.len(), 1);
    move_to(&mut state, angel, Zone::Graveyard);
    drain(&mut surface(), &mut state);
    assert_eq!(state.players[0].life, 21);
    assert_eq!(state.players[1].life, 20);
}

#[test]
fn a_dying_angel_still_triggers_before_state_based_actions() {
    let mut state = ready();
    let angel = put(
        &mut state,
        PlayerId::P0,
        "Dazzling Angel",
        Zone::Battlefield,
    );
    state.objects.get_mut(angel).damage = 3;
    enter(&mut state, PlayerId::P0, "Healer's Hawk");
    assert_eq!(state.objects.get(angel).zone, Zone::Graveyard);
    assert_eq!(state.engine.pending_triggers.len(), 1);
    drain(&mut surface(), &mut state);
    assert_eq!(state.players[0].life, 21);
}

#[test]
fn skymage_triggers_for_every_successful_friendly_draw_and_restores_pending_order() {
    let mut state = ready();
    let mage = put(
        &mut state,
        PlayerId::P0,
        "Clinquant Skymage",
        Zone::Battlefield,
    );
    draw(&mut state, mage, PlayerId::P1, 2);
    assert!(state.engine.pending_triggers.is_empty());
    draw(&mut state, mage, PlayerId::P0, 3);
    assert_eq!(state.engine.pending_triggers.len(), 3);
    assert_eq!(state.objects.get(mage).counters.plus1_plus1, 0);
    let mut surface = surface();
    assert!(
        matches!(next(&mut surface, &mut state), Decision::OrderTriggers { pending, .. } if pending.len() == 3)
    );
    let mut restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    let mut restored_surface = surface.clone();
    for (game, harness) in [
        (&mut state, &mut surface),
        (&mut restored, &mut restored_surface),
    ] {
        apply(harness, game, Action::OrderTriggers(vec![2, 0, 1]));
        drain(harness, game);
        assert_eq!(game.objects.get(mage).counters.plus1_plus1, 3);
        assert_eq!(engine::effective_power(game, mage), 4);
    }
    assert_eq!(state, restored);
}

#[test]
fn failed_draw_does_not_create_a_skymage_counter_trigger() {
    let mut state = ready();
    let mage = put(
        &mut state,
        PlayerId::P0,
        "Clinquant Skymage",
        Zone::Battlefield,
    );
    state.players[0].library.clear();
    draw(&mut state, mage, PlayerId::P0, 1);
    assert!(state.engine.pending_triggers.is_empty());
    assert_eq!(state.objects.get(mage).counters.plus1_plus1, 0);
}

#[test]
fn pending_skymage_counter_does_not_follow_a_blinked_source() {
    let mut state = ready();
    let mage = put(
        &mut state,
        PlayerId::P0,
        "Clinquant Skymage",
        Zone::Battlefield,
    );
    draw(&mut state, mage, PlayerId::P0, 1);
    let mut surface = surface();
    next(&mut surface, &mut state);
    move_to(&mut state, mage, Zone::Exile);
    move_to(&mut state, mage, Zone::Battlefield);
    drain(&mut surface, &mut state);
    assert_eq!(state.objects.get(mage).counters.plus1_plus1, 0);
}

#[test]
fn dwynen_requires_another_elf_at_entry_and_checks_again_at_resolution() {
    for lose_other in [false, true] {
        let mut state = ready();
        let elf = put(
            &mut state,
            PlayerId::P0,
            "Llanowar Elves",
            Zone::Battlefield,
        );
        enter(&mut state, PlayerId::P0, "Dwynen's Elite");
        assert_eq!(state.engine.pending_triggers.len(), 1);
        let mut surface = surface();
        next(&mut surface, &mut state);
        if lose_other {
            move_to(&mut state, elf, Zone::Graveyard);
        }
        drain(&mut surface, &mut state);
        assert_eq!(
            tokens(&state, PlayerId::P0, "Elf Warrior Token").len(),
            usize::from(!lose_other)
        );
    }
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    enter(&mut state, PlayerId::P0, "Dwynen's Elite");
    assert!(state.engine.pending_triggers.is_empty());
    enter(&mut state, PlayerId::P0, "Llanowar Elves");
    drain(&mut surface(), &mut state);
    assert!(tokens(&state, PlayerId::P0, "Elf Warrior Token").is_empty());
}

#[test]
fn dwynen_condition_can_be_satisfied_by_a_new_elf_before_resolution() {
    let mut state = ready();
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    enter(&mut state, PlayerId::P0, "Dwynen's Elite");
    let mut surface = surface();
    next(&mut surface, &mut state);
    move_to(&mut state, elf, Zone::Graveyard);
    enter(&mut state, PlayerId::P0, "Elf Warrior Token");
    drain(&mut surface, &mut state);
    assert_eq!(tokens(&state, PlayerId::P0, "Elf Warrior Token").len(), 2);
}

#[test]
fn returned_dwynen_is_another_elf_for_its_old_trigger() {
    let mut state = ready();
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let elite = enter(&mut state, PlayerId::P0, "Dwynen's Elite");
    let mut surface = surface();
    next(&mut surface, &mut state);
    move_to(&mut state, elf, Zone::Graveyard);
    move_to(&mut state, elite, Zone::Exile);
    move_to(&mut state, elite, Zone::Battlefield);
    queue(&mut state);
    assert!(state.engine.pending_triggers.is_empty());
    drain(&mut surface, &mut state);
    assert_eq!(tokens(&state, PlayerId::P0, "Elf Warrior Token").len(), 1);
}

#[test]
fn unicorn_triggers_are_not_targets_and_refer_to_the_exact_entrant() {
    for blink_entrant in [false, true] {
        let mut state = ready();
        let unicorn = enter(&mut state, PlayerId::P0, "Good-Fortune Unicorn");
        enter(&mut state, PlayerId::P0, "Forest");
        enter(&mut state, PlayerId::P1, "Healer's Hawk");
        assert!(state.engine.pending_triggers.is_empty());
        let entrant = enter(&mut state, PlayerId::P0, "Healer's Hawk");
        let mut surface = surface();
        assert!(matches!(
            next(&mut surface, &mut state),
            Decision::CastSpellOrPass { .. }
        ));
        assert!(state.stack.last().unwrap().targets.is_empty());
        move_to(&mut state, unicorn, Zone::Graveyard);
        if blink_entrant {
            move_to(&mut state, entrant, Zone::Exile);
            move_to(&mut state, entrant, Zone::Battlefield);
        }
        drain(&mut surface, &mut state);
        assert_eq!(
            state.objects.get(entrant).counters.plus1_plus1,
            i16::from(!blink_entrant)
        );
    }
}

#[test]
fn unicorn_counter_follows_the_same_incarnation_after_control_changes() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Good-Fortune Unicorn",
        Zone::Battlefield,
    );
    let entrant = enter(&mut state, PlayerId::P0, "Healer's Hawk");
    let mut surface = surface();
    next(&mut surface, &mut state);
    state.players[0].battlefield.retain(|id| *id != entrant);
    state.players[1].battlefield.push(entrant);
    state.objects.get_mut(entrant).controller = PlayerId::P1;
    drain(&mut surface, &mut state);
    assert_eq!(state.objects.get(entrant).counters.plus1_plus1, 1);
}

#[test]
fn guarded_heir_creates_two_distinct_knights_after_source_leaves() {
    let mut state = ready();
    let heir = enter(&mut state, PlayerId::P0, "Guarded Heir");
    let mut surface = surface();
    next(&mut surface, &mut state);
    move_to(&mut state, heir, Zone::Graveyard);
    drain(&mut surface, &mut state);
    let knights = tokens(&state, PlayerId::P0, "Knight Token");
    assert_eq!(knights.len(), 2);
    assert_ne!(knights[0], knights[1]);
    for knight in knights {
        assert_eq!(
            (
                engine::effective_power(&state, knight),
                engine::effective_toughness(&state, knight)
            ),
            (3, 3)
        );
        assert!(state.objects.get(knight).summoning_sick);
        assert_eq!(
            CARD_DEFS[usize::from(state.objects.get(knight).card_def)].keywords,
            Keywords::NONE
        );
    }
}

#[test]
fn knight_creation_triggers_angel_and_unicorn_once_per_token_and_restores_order() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Dazzling Angel",
        Zone::Battlefield,
    );
    put(
        &mut state,
        PlayerId::P0,
        "Good-Fortune Unicorn",
        Zone::Battlefield,
    );
    let heir = enter(&mut state, PlayerId::P0, "Guarded Heir");
    let mut surface = surface();
    assert!(
        matches!(next(&mut surface, &mut state), Decision::OrderTriggers { pending, .. } if pending.len() == 3)
    );
    let mut restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    let mut restored_surface = surface.clone();
    for (game, harness) in [
        (&mut state, &mut surface),
        (&mut restored, &mut restored_surface),
    ] {
        // Heir's token trigger resolves first, introducing a second ordering decision.
        let pending = &game.engine.pending_triggers;
        let heir_index = pending
            .iter()
            .position(|trigger| trigger.source == heir)
            .unwrap();
        let mut order: Vec<_> = (0..pending.len())
            .filter(|index| *index != heir_index)
            .collect();
        order.push(heir_index);
        apply(harness, game, Action::OrderTriggers(order));
        drain(harness, game);
        assert_eq!(game.players[0].life, 23);
        assert_eq!(game.objects.get(heir).counters.plus1_plus1, 1);
        let knights = tokens(game, PlayerId::P0, "Knight Token");
        assert_eq!(knights.len(), 2);
        for knight in knights {
            assert_eq!(game.objects.get(knight).counters.plus1_plus1, 1);
        }
    }
    assert_eq!(state, restored);
}

#[test]
fn youthful_valkyrie_checks_the_other_friendly_entrants_angel_type() {
    let mut state = ready();
    let valkyrie = enter(&mut state, PlayerId::P0, "Youthful Valkyrie");
    assert!(state.engine.pending_triggers.is_empty());
    enter(&mut state, PlayerId::P1, "Dazzling Angel");
    enter(&mut state, PlayerId::P0, "Healer's Hawk");
    enter(&mut state, PlayerId::P0, "Forest");
    assert!(state.engine.pending_triggers.is_empty());
    enter(&mut state, PlayerId::P0, "Dazzling Angel");
    drain(&mut surface(), &mut state);
    assert_eq!(state.objects.get(valkyrie).counters.plus1_plus1, 1);
    assert_eq!(state.players[0].life, 20);
}

#[test]
fn youthful_valkyrie_counter_does_not_follow_a_returned_source() {
    let mut state = ready();
    let valkyrie = put(
        &mut state,
        PlayerId::P0,
        "Youthful Valkyrie",
        Zone::Battlefield,
    );
    enter(&mut state, PlayerId::P0, "Dazzling Angel");
    let mut surface = surface();
    next(&mut surface, &mut state);
    move_to(&mut state, valkyrie, Zone::Exile);
    move_to(&mut state, valkyrie, Zone::Battlefield);
    drain(&mut surface, &mut state);
    assert_eq!(state.objects.get(valkyrie).counters.plus1_plus1, 0);
}

#[test]
fn simultaneous_entries_are_seen_by_new_unicorn_and_angel_observers() {
    let mut state = ready();
    let unicorn = put(&mut state, PlayerId::P0, "Good-Fortune Unicorn", Zone::Hand);
    let angel = put(&mut state, PlayerId::P0, "Dazzling Angel", Zone::Hand);
    let hawk = put(&mut state, PlayerId::P0, "Healer's Hawk", Zone::Hand);
    event::propose_and_commit_batch(
        &mut state,
        vec![
            ProposedEvent::zone_change(unicorn, Zone::Battlefield),
            ProposedEvent::zone_change(angel, Zone::Battlefield),
            ProposedEvent::zone_change(hawk, Zone::Battlefield),
        ],
    );
    queue(&mut state);
    assert_eq!(state.engine.pending_triggers.len(), 4);
    drain(&mut surface(), &mut state);
    assert_eq!(state.players[0].life, 22);
    assert_eq!(state.objects.get(unicorn).counters.plus1_plus1, 0);
    assert_eq!(state.objects.get(angel).counters.plus1_plus1, 1);
    assert_eq!(state.objects.get(hawk).counters.plus1_plus1, 1);
}

#[test]
fn unicorn_adds_a_counter_without_creating_a_ward_targeting_trigger() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Good-Fortune Unicorn",
        Zone::Battlefield,
    );
    let terror = enter(&mut state, PlayerId::P0, "Tolarian Terror");
    assert_eq!(state.engine.pending_triggers.len(), 1);
    let mut surface = surface();
    next(&mut surface, &mut state);
    assert!(state.stack.last().unwrap().targets.is_empty());
    drain(&mut surface, &mut state);
    assert_eq!(state.objects.get(terror).counters.plus1_plus1, 1);
}

#[test]
fn guarded_heir_lifelink_applies_to_combat_damage() {
    let mut state = ready();
    let heir = put(&mut state, PlayerId::P0, "Guarded Heir", Zone::Battlefield);
    state.objects.get_mut(heir).summoning_sick = false;
    state.step = Step::DeclareBlockers;
    state.engine.combat.attackers_declared = true;
    state.engine.combat.blockers_declared = true;
    state.engine.combat.attackers = vec![heir];
    let mut surface = surface();
    for _ in 0..6 {
        let decision = next(&mut surface, &mut state);
        if state.players[1].life == 19 {
            break;
        }
        assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
        apply(&mut surface, &mut state, Action::Pass);
    }
    assert_eq!(state.players[1].life, 19);
    assert_eq!(state.players[0].life, 21);
}
