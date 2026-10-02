//! Koma spell protection, ward, combat triggers and pending-choice restore.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, Keywords, Subtype, Supertype, WardCostDef,
    CARD_DEFS,
};
use mtg_kernel::combat_damage_v1::enable_foundations_combat_v1;
use mtg_kernel::effect::EffectBooleanChoicePurpose;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface_v2::{
    HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SurfaceAction, SurfaceDecision,
};
use mtg_kernel::trigger;

fn ready() -> GameState {
    let island = card_id_by_name("Island").unwrap();
    let mut state =
        GameState::new_from_libraries(&[island; 40], &[island; 40], |_| "Island".into(), 123);
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let object = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Battlefield => state.players[player.index()].battlefield.push(object),
        Zone::Hand => state.players[player.index()].hand.push(object),
        _ => panic!("helper zone"),
    }
    object
}

fn surface() -> HarnessSurfaceV2 {
    HarnessSurfaceV2::new_with_priority_mode_v1(
        PriorityModeV1::EngineWindowsV1,
        SuppressionAuditMode::Full,
    )
}

fn next(surface: &mut HarnessSurfaceV2, state: &mut GameState) -> Decision {
    match surface.next_decision(state) {
        SurfaceDecision::Decision(decision @ Decision::Halted { .. }) => panic!("{decision:?}"),
        SurfaceDecision::Decision(decision) => decision,
        other => panic!("expected decision: {other:?}"),
    }
}

fn apply(surface: &mut HarnessSurfaceV2, state: &mut GameState, action: Action) {
    surface.apply(state, SurfaceAction::Action(action)).unwrap();
}

fn drain(surface: &mut HarnessSurfaceV2, state: &mut GameState, payment: Option<bool>) -> usize {
    let mut payment_choices = 0;
    for _ in 0..100 {
        let decision = next(surface, state);
        if matches!(decision, Decision::CastSpellOrPass { .. }) && state.stack.is_empty() {
            assert!(state.engine.pending_triggers.is_empty());
            return payment_choices;
        }
        let action = match decision {
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::ChooseEffectBoolean { .. } => {
                payment_choices += 1;
                Action::ChooseEffectBoolean(payment.expect("explicit payment answer"))
            }
            other => panic!("unexpected resolution decision {other:?}"),
        };
        apply(surface, state, action);
    }
    panic!("resolution did not finish");
}

fn cast_at(
    surface: &mut HarnessSurfaceV2,
    state: &mut GameState,
    actor: PlayerId,
    name: &str,
    target: ObjectId,
    blue_mana: u8,
) -> ObjectId {
    let spell = put(state, actor, name, Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = blue_mana;
    let decision = next(surface, state);
    if matches!(decision, Decision::CastSpellOrPass { player, .. } if player != actor) {
        apply(surface, state, Action::Pass);
    }
    assert!(
        matches!(next(surface, state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&spell))
    );
    apply(surface, state, Action::CastSpell(spell));
    assert!(
        matches!(next(surface, state), Decision::ChooseTargets { legal_targets, .. }
        if legal_targets.contains(&Target::Object(target)))
    );
    apply(surface, state, Action::ChooseTarget(Target::Object(target)));
    spell
}

fn koma_on_stack(surface: &mut HarnessSurfaceV2, state: &mut GameState, extra: u8) -> ObjectId {
    let koma = put(state, PlayerId::P0, "Koma, World-Eater", Zone::Hand);
    state.players[0].mana_pool[ManaColor::G.pool_index()] = 4;
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 3 + extra;
    next(surface, state);
    apply(surface, state, Action::CastSpell(koma));
    assert_eq!(state.objects.get(koma).zone, Zone::Stack);
    koma
}

fn coils(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|id| {
            state.objects.get(*id).card_def == card_id_by_name("Koma's Coil Token").unwrap()
        })
        .collect()
}

fn combat(blocker: Option<&str>) -> (GameState, ObjectId, Option<ObjectId>) {
    let mut state = ready();
    state.step = Step::DeclareBlockers;
    enable_foundations_combat_v1(&mut state).unwrap();
    state.engine.combat.attackers_declared = true;
    state.engine.combat.blockers_declared = true;
    let koma = put(
        &mut state,
        PlayerId::P0,
        "Koma, World-Eater",
        Zone::Battlefield,
    );
    let blocker = blocker.map(|name| put(&mut state, PlayerId::P1, name, Zone::Battlefield));
    state.engine.combat.attackers.push(koma);
    state
        .engine
        .combat
        .blocked_by
        .push((koma, blocker.into_iter().collect()));
    (state, koma, blocker)
}

fn enter_damage(state: &mut GameState) -> Decision {
    for _ in 0..2 {
        assert!(matches!(
            engine::advance_until_decision(state),
            Decision::CastSpellOrPass { .. }
        ));
        engine::step(state, Action::Pass).unwrap();
    }
    let decision = engine::advance_until_decision(state);
    assert_eq!(state.step, Step::CombatDamage);
    decision
}

fn assign(state: &mut GameState, amounts: &[(ObjectId, Target, i32)]) {
    for _ in 0..256 {
        match engine::advance_until_decision(state) {
            Decision::ChooseCombatDamageRange {
                source,
                recipient,
                minimum,
                maximum,
                split_at,
                ..
            } => {
                let amount = amounts
                    .iter()
                    .find(|(s, t, _)| *s == source && *t == recipient)
                    .unwrap_or_else(|| {
                        panic!("missing intended damage {source:?} -> {recipient:?}")
                    })
                    .2;
                assert!((minimum..=maximum).contains(&amount));
                engine::step(
                    state,
                    Action::ChooseCombatDamageRange {
                        upper_half: amount > split_at,
                    },
                )
                .unwrap();
            }
            Decision::CastSpellOrPass { .. } => return,
            other => panic!("unexpected damage decision {other:?}"),
        }
    }
    panic!("damage choices did not finish");
}

#[test]
fn printed_koma_and_coils_have_exact_costs_characteristics_and_admission() {
    assert_eq!(card_id_by_name("Koma, World-Eater"), Some(195));
    assert_eq!(card_id_by_name("Koma's Coil Token"), Some(196));
    let koma = &CARD_DEFS[195];
    assert_eq!(
        (koma.power, koma.toughness, koma.mana_value),
        (Some(8), Some(12), 7)
    );
    assert_eq!(koma.cost.generic, 3);
    assert_eq!(
        koma.cost.pips,
        &[
            Pip::Colored(ManaColor::G),
            Pip::Colored(ManaColor::G),
            Pip::Colored(ManaColor::U),
            Pip::Colored(ManaColor::U)
        ]
    );
    assert_eq!(koma.colors, &[ManaColor::G, ManaColor::U]);
    assert_eq!(koma.subtypes, &[Subtype::Serpent]);
    assert_eq!(koma.supertypes, &[Supertype::Legendary]);
    assert!(koma.keywords.has(Keywords::TRAMPLE));
    assert_eq!(koma.ward_cost, Some(WardCostDef::Generic(4)));
    assert!(koma.spell_cannot_be_countered);
    let coil = &CARD_DEFS[196];
    assert_eq!(coil.object_name, "Koma's Coil");
    assert_eq!(
        (coil.power, coil.toughness, coil.mana_value),
        (Some(3), Some(3), 0)
    );
    assert_eq!(coil.colors, &[ManaColor::U]);
    assert_eq!(coil.subtypes, &[Subtype::Serpent]);
    assert!(coil.supertypes.is_empty());
    assert_eq!(coil.keywords, Keywords::NONE);
    assert!(!coil.spell_cannot_be_countered);
    assert_eq!(coil.ward_cost, None);
    assert!(coil.is_token);
    preflight_fully_supported_deck(&[195]).unwrap();
    assert!(preflight_fully_supported_deck(&[196]).is_err());
}

#[test]
fn casting_requires_seven_mana_including_two_green_and_two_blue() {
    for (green, blue) in [(4, 2), (6, 1)] {
        let mut state = ready();
        let koma = put(&mut state, PlayerId::P0, "Koma, World-Eater", Zone::Hand);
        state.players[0].mana_pool[ManaColor::G.pool_index()] = green;
        state.players[0].mana_pool[ManaColor::U.pool_index()] = blue;
        let before = state.state_hash();
        assert!(engine::step(&mut state, Action::CastSpell(koma)).is_err());
        assert_eq!(state.state_hash(), before);
    }
    let mut state = ready();
    let mut surf = surface();
    let koma = koma_on_stack(&mut surf, &mut state, 0);
    drain(&mut surf, &mut state, None);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    assert_eq!(state.objects.get(koma).zone, Zone::Battlefield);
}

#[test]
fn counterspell_can_target_koma_but_leaves_it_to_resolve() {
    let mut state = ready();
    let mut surf = surface();
    let koma = koma_on_stack(&mut surf, &mut state, 0);
    let counter = cast_at(&mut surf, &mut state, PlayerId::P1, "Counterspell", koma, 2);
    for _ in 0..2 {
        assert!(matches!(
            next(&mut surf, &mut state),
            Decision::CastSpellOrPass { .. }
        ));
        apply(&mut surf, &mut state, Action::Pass);
    }
    next(&mut surf, &mut state);
    assert_eq!(state.objects.get(counter).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(koma).zone, Zone::Stack);
    drain(&mut surf, &mut state, None);
    assert_eq!(state.objects.get(koma).zone, Zone::Battlefield);
}

#[test]
fn payable_force_spike_choice_remains_and_declining_does_not_counter_koma() {
    let mut state = ready();
    let mut surf = surface();
    let koma = koma_on_stack(&mut surf, &mut state, 1);
    let counter = cast_at(&mut surf, &mut state, PlayerId::P1, "Force Spike", koma, 1);
    for _ in 0..2 {
        next(&mut surf, &mut state);
        apply(&mut surf, &mut state, Action::Pass);
    }
    assert!(matches!(
        next(&mut surf, &mut state),
        Decision::ChooseEffectBoolean {
            player: PlayerId::P0,
            purpose: EffectBooleanChoicePurpose::CounterTargetUnlessPaysGeneric { generic: 1, .. },
            ..
        }
    ));
    let saved = serde_json::to_vec(&state).unwrap();
    apply(&mut surf, &mut state, Action::ChooseEffectBoolean(false));
    drain(&mut surf, &mut state, None);
    let mut restored: GameState = serde_json::from_slice(&saved).unwrap();
    engine::step(&mut restored, Action::ChooseEffectBoolean(false)).unwrap();
    drain(&mut surface(), &mut restored, None);
    assert_eq!(state.state_hash(), restored.state_hash());
    assert_eq!(state.objects.get(koma).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(counter).zone, Zone::Graveyard);
    assert_eq!(state.players[0].mana_pool.iter().sum::<u8>(), 1);
}

#[test]
fn unpayable_force_spike_does_not_counter_koma_or_offer_impossible_payment() {
    let mut state = ready();
    let mut surf = surface();
    let koma = koma_on_stack(&mut surf, &mut state, 0);
    let counter = cast_at(&mut surf, &mut state, PlayerId::P1, "Force Spike", koma, 1);
    assert_eq!(drain(&mut surf, &mut state, None), 0);
    assert_eq!(state.objects.get(koma).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(counter).zone, Zone::Graveyard);
}

#[test]
fn declining_payable_ward_counters_the_opposing_bounce_spell() {
    let mut state = ready();
    let koma = put(
        &mut state,
        PlayerId::P0,
        "Koma, World-Eater",
        Zone::Battlefield,
    );
    let mut surf = surface();
    let bounce = cast_at(&mut surf, &mut state, PlayerId::P1, "Unsummon", koma, 5);
    assert_eq!(drain(&mut surf, &mut state, Some(false)), 1);
    assert_eq!(state.objects.get(koma).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(bounce).zone, Zone::Graveyard);
    assert_eq!(state.players[1].mana_pool[ManaColor::U.pool_index()], 4);
}

#[test]
fn paying_four_for_ward_resolves_the_bounce_and_restores_the_payment_choice() {
    let mut state = ready();
    let koma = put(
        &mut state,
        PlayerId::P0,
        "Koma, World-Eater",
        Zone::Battlefield,
    );
    let mut surf = surface();
    cast_at(&mut surf, &mut state, PlayerId::P1, "Unsummon", koma, 5);
    for _ in 0..2 {
        assert!(matches!(
            next(&mut surf, &mut state),
            Decision::CastSpellOrPass { .. }
        ));
        apply(&mut surf, &mut state, Action::Pass);
    }
    assert!(matches!(
        next(&mut surf, &mut state),
        Decision::ChooseEffectBoolean {
            player: PlayerId::P1,
            purpose: EffectBooleanChoicePurpose::CounterUnlessPaysGeneric { generic: 4, .. },
            ..
        }
    ));
    let saved = serde_json::to_vec(&state).unwrap();
    apply(&mut surf, &mut state, Action::ChooseEffectBoolean(true));
    drain(&mut surf, &mut state, None);
    let mut restored: GameState = serde_json::from_slice(&saved).unwrap();
    engine::step(&mut restored, Action::ChooseEffectBoolean(true)).unwrap();
    drain(&mut surface(), &mut restored, None);
    assert_eq!(state.state_hash(), restored.state_hash());
    assert_eq!(state.objects.get(koma).zone, Zone::Hand);
    assert_eq!(state.players[1].mana_pool, [0; 6]);
}

#[test]
fn unpayable_ward_counters_without_a_payment_choice() {
    let mut state = ready();
    let koma = put(
        &mut state,
        PlayerId::P0,
        "Koma, World-Eater",
        Zone::Battlefield,
    );
    let mut surf = surface();
    let bounce = cast_at(&mut surf, &mut state, PlayerId::P1, "Unsummon", koma, 1);
    assert_eq!(drain(&mut surf, &mut state, None), 0);
    assert_eq!(state.objects.get(koma).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(bounce).zone, Zone::Graveyard);
}

#[test]
fn own_controller_bounce_does_not_trigger_ward() {
    let mut state = ready();
    let koma = put(
        &mut state,
        PlayerId::P0,
        "Koma, World-Eater",
        Zone::Battlefield,
    );
    let mut surf = surface();
    cast_at(&mut surf, &mut state, PlayerId::P0, "Unsummon", koma, 1);
    assert_eq!(drain(&mut surf, &mut state, None), 0);
    assert_eq!(state.objects.get(koma).zone, Zone::Hand);
}

#[test]
fn unblocked_combat_damage_creates_exactly_four_coils() {
    let (mut state, koma, _) = combat(None);
    enter_damage(&mut state);
    assign(&mut state, &[(koma, Target::Player(PlayerId::P1), 8)]);
    assert_eq!(state.players[1].life, 12);
    drain(&mut surface(), &mut state, None);
    let created = coils(&state, PlayerId::P0);
    assert_eq!(created.len(), 4);
    assert!(coils(&state, PlayerId::P1).is_empty());
    for id in created {
        assert_eq!(state.objects.get(id).name, "Koma's Coil");
        assert_eq!(engine::effective_power(&state, id), 3);
        assert_eq!(engine::effective_toughness(&state, id), 3);
    }
}

#[test]
fn fully_blocked_combat_does_not_create_coils() {
    let (mut state, koma, blocker) = combat(Some("Tolarian Terror"));
    let blocker = blocker.unwrap();
    state.objects.get_mut(blocker).counters.plus1_plus1 = 4;
    enter_damage(&mut state);
    assign(
        &mut state,
        &[
            (koma, Target::Object(blocker), 8),
            (blocker, Target::Object(koma), 9),
        ],
    );
    drain(&mut surface(), &mut state, None);
    assert_eq!(state.players[1].life, 20);
    assert!(coils(&state, PlayerId::P0).is_empty());
    assert_eq!(state.objects.get(koma).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(koma).damage, 9);
}

#[test]
fn trample_damage_creates_coils_after_koma_dies_in_the_same_damage_wave() {
    let (mut state, koma, blocker) = combat(Some("Treetop Snarespinner"));
    let blocker = blocker.unwrap();
    enter_damage(&mut state);
    assign(
        &mut state,
        &[
            (koma, Target::Object(blocker), 4),
            (koma, Target::Player(PlayerId::P1), 4),
            (blocker, Target::Object(koma), 1),
        ],
    );
    assert_eq!(state.players[1].life, 16);
    assert_eq!(state.objects.get(koma).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(blocker).zone, Zone::Graveyard);
    let saved = serde_json::to_vec(&state).unwrap();
    drain(&mut surface(), &mut state, None);
    let mut restored: GameState = serde_json::from_slice(&saved).unwrap();
    drain(&mut surface(), &mut restored, None);
    assert_eq!(state.state_hash(), restored.state_hash());
    assert_eq!(coils(&state, PlayerId::P0).len(), 4);
}

#[test]
fn noncombat_damage_to_a_player_does_not_create_coils() {
    let mut state = ready();
    let koma = put(
        &mut state,
        PlayerId::P0,
        "Koma, World-Eater",
        Zone::Battlefield,
    );
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(koma, Target::Player(PlayerId::P1), 8),
    );
    let pending = trigger::collect_and_process(&mut state);
    state.engine.pending_triggers.extend(pending);
    drain(&mut surface(), &mut state, None);
    assert_eq!(state.players[1].life, 12);
    assert!(coils(&state, PlayerId::P0).is_empty());
}
