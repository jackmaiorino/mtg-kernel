//! FDN kicker, equipment and library-search reference cards: Burst Lightning,
//! Evolving Wilds, Solemn Simulacrum, Swiftfoot Boots, Grim Tutor,
//! Quick-Draw Katana, Adventuring Gear and Goldvein Pick.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardCapability, CardType, CostComponent,
    Keywords, Subtype, TargetSpec, CARD_DEFS, KERNEL_CARDDB_HASH,
};
use mtg_kernel::effect::{EffectOp, LibraryCardFilter, PlayerRef, TargetRef};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{Cost, ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::trigger::{self, TriggerCondition};

const NAMES: [&str; 8] = [
    "Burst Lightning",
    "Evolving Wilds",
    "Solemn Simulacrum",
    "Swiftfoot Boots",
    "Grim Tutor",
    "Quick-Draw Katana",
    "Adventuring Gear",
    "Goldvein Pick",
];

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn ready() -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], |_| "Plains".into(), 0x4644_4e45);
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = id(name);
    let object = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner: player,
        controller: player,
        zone,
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
    let seat = &mut state.players[player.index()];
    match zone {
        Zone::Hand => seat.hand.push(object),
        Zone::Battlefield => seat.battlefield.push(object),
        Zone::Library => seat.library.push(object),
        _ => panic!("helper zone"),
    }
    object
}

fn mana(state: &mut GameState, player: PlayerId, color: ManaColor, amount: u8) {
    state.players[player.index()].mana_pool[color.pool_index()] = amount;
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

/// Passes priority until the stack is empty or a non-priority decision is
/// pending, returning that decision.
fn settle(state: &mut GameState) -> Decision {
    for _ in 0..60 {
        let decision = next(state);
        match decision {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return decision,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => return other,
        }
    }
    panic!("resolution did not settle");
}

fn settle_to_priority(state: &mut GameState) {
    let decision = settle(state);
    assert!(
        matches!(decision, Decision::CastSpellOrPass { .. }),
        "unexpected decision: {decision:?}"
    );
    assert!(state.engine.pending_triggers.is_empty());
}

fn life(state: &GameState, player: PlayerId) -> i32 {
    state.players[player.index()].life
}

#[test]
fn appended_definitions_match_their_printed_characteristics() {
    let first = id(NAMES[0]);
    for (offset, name) in NAMES.iter().enumerate() {
        let card_def = id(name);
        assert_eq!(card_def, first + offset as u16, "{name} appends in order");
        assert_eq!(
            CARD_DEFS[card_def as usize].capability,
            CardCapability::Full
        );
    }
    assert_eq!(first as usize + NAMES.len(), CARD_DEFS.len());
    preflight_fully_supported_deck(&NAMES.map(id)).unwrap();
    println!("FDN catalog hash: {KERNEL_CARDDB_HASH:016x}");

    let burst = &CARD_DEFS[id("Burst Lightning") as usize];
    assert_eq!(burst.types, &[CardType::Instant]);
    assert_eq!(burst.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(burst.cost.generic, 0);
    assert_eq!(
        burst.kicker_cost,
        Some(Cost {
            pips: &[],
            generic: 4,
            x_count: 0,
        })
    );
    assert_eq!(burst.target_spec, TargetSpec::AnyTarget);
    assert_eq!(
        (burst.spell_effect)(),
        Some(EffectOp::Conditional {
            cond: mtg_kernel::effect::EffectCond::WasKicked,
            then: Box::new(EffectOp::DealDamage {
                target: TargetRef::Target(0),
                amount: 4,
            }),
            else_: Box::new(EffectOp::DealDamage {
                target: TargetRef::Target(0),
                amount: 2,
            }),
        })
    );

    let wilds = &CARD_DEFS[id("Evolving Wilds") as usize];
    assert_eq!(wilds.types, &[CardType::Land]);
    assert!(wilds.produces_mana.is_empty());
    assert_eq!(wilds.activated_abilities.len(), 1);
    assert_eq!(
        wilds.activated_abilities[0].cost,
        &[CostComponent::Tap, CostComponent::SacrificeSelf]
    );
    assert!(!wilds.activated_abilities[0].sorcery_speed_only);
    assert_eq!(
        (wilds.activated_abilities[0].effect)(),
        EffectOp::SearchLibraryToBattlefieldTapped {
            player: PlayerRef::Controller,
            filter: LibraryCardFilter::BasicLand,
        }
    );

    let solemn = &CARD_DEFS[id("Solemn Simulacrum") as usize];
    assert_eq!(solemn.types, &[CardType::Artifact, CardType::Creature]);
    assert_eq!(solemn.subtypes, &[Subtype::Golem]);
    assert!(Subtype::CREATURE_TYPES.contains(&Subtype::Golem));
    assert_eq!(solemn.colors, &[]);
    assert_eq!(solemn.cost.generic, 4);
    assert_eq!((solemn.power, solemn.toughness), (Some(2), Some(2)));
    let triggers = trigger::triggers_for(id("Solemn Simulacrum"));
    assert_eq!(triggers.len(), 2);
    assert_eq!(triggers[0].condition, TriggerCondition::Etb);
    assert_eq!(
        triggers[1].condition,
        TriggerCondition::LeftBattlefieldToGraveyard
    );

    let boots = &CARD_DEFS[id("Swiftfoot Boots") as usize];
    assert_eq!(boots.types, &[CardType::Artifact]);
    assert_eq!(boots.subtypes, &[Subtype::Equipment]);
    assert_eq!(boots.cost.generic, 2);
    let equipment = boots.equipment.unwrap();
    assert_eq!((equipment.power_delta, equipment.toughness_delta), (0, 0));
    let granted = Keywords(Keywords::HEXPROOF.0 | Keywords::HASTE.0);
    assert_eq!(equipment.controller_turn_keywords, granted);
    assert_eq!(equipment.other_turn_keywords, granted);
    assert_eq!(boots.activated_abilities.len(), 1);
    assert!(boots.activated_abilities[0].sorcery_speed_only);
    assert_eq!(
        boots.activated_abilities[0].target_spec,
        TargetSpec::ControlledCreature
    );
    assert!(!equipment.pt_controller_turn_only);

    let tutor = &CARD_DEFS[id("Grim Tutor") as usize];
    assert_eq!(tutor.types, &[CardType::Sorcery]);
    assert_eq!(
        tutor.cost.pips,
        &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::B)]
    );
    assert_eq!(tutor.cost.generic, 1);
    assert_eq!(tutor.target_spec, TargetSpec::None);
    assert_eq!(
        (tutor.spell_effect)(),
        Some(EffectOp::Sequence(vec![
            EffectOp::SearchLibraryToHand {
                player: PlayerRef::Controller,
                filter: LibraryCardFilter::AnyCard,
            },
            EffectOp::LoseLife {
                player: PlayerRef::Controller,
                amount: 3,
            },
        ]))
    );

    let katana = &CARD_DEFS[id("Quick-Draw Katana") as usize];
    assert_eq!(katana.types, &[CardType::Artifact]);
    assert_eq!(katana.subtypes, &[Subtype::Equipment]);
    assert_eq!(katana.cost.generic, 2);
    let equipment = katana.equipment.unwrap();
    assert!(equipment.pt_controller_turn_only);
    assert_eq!(equipment.pt_deltas(true), (2, 0));
    assert_eq!(equipment.pt_deltas(false), (0, 0));
    assert_eq!(equipment.controller_turn_keywords, Keywords::FIRST_STRIKE);
    assert_eq!(equipment.other_turn_keywords, Keywords::NONE);
    assert_eq!(katana.activated_abilities.len(), 1);
    assert!(katana.activated_abilities[0].sorcery_speed_only);
    assert_eq!(
        katana.activated_abilities[0].cost,
        &[CostComponent::Mana(Cost {
            pips: &[],
            generic: 2,
            x_count: 0,
        })]
    );

    for (name, cost, (power, toughness), condition) in [
        (
            "Adventuring Gear",
            1,
            (0, 0),
            TriggerCondition::ControlledLandEnters,
        ),
        (
            "Goldvein Pick",
            2,
            (1, 1),
            TriggerCondition::EquippedCreatureDealsCombatDamageToPlayer,
        ),
    ] {
        let card = &CARD_DEFS[id(name) as usize];
        assert_eq!(card.types, &[CardType::Artifact], "{name}");
        assert_eq!(card.subtypes, &[Subtype::Equipment], "{name}");
        assert_eq!(card.cost.generic, cost, "{name}");
        let equipment = card.equipment.unwrap();
        assert_eq!(
            (equipment.power_delta, equipment.toughness_delta),
            (power, toughness)
        );
        assert_eq!(card.activated_abilities.len(), 1);
        assert_eq!(
            card.activated_abilities[0].cost,
            &[CostComponent::Mana(Cost {
                pips: &[],
                generic: 1,
                x_count: 0,
            })]
        );
        let triggers = trigger::triggers_for(id(name));
        assert_eq!(triggers.len(), 1, "{name}");
        assert_eq!(triggers[0].condition, condition, "{name}");
    }
}

#[test]
fn grim_tutor_finds_any_card_unrevealed_and_costs_three_life() {
    let mut state = ready();
    let land = put(&mut state, PlayerId::P0, "Plains", Zone::Library);
    let burst = put(&mut state, PlayerId::P0, "Burst Lightning", Zone::Library);
    let boots = put(&mut state, PlayerId::P0, "Swiftfoot Boots", Zone::Library);
    let tutor = put(&mut state, PlayerId::P0, "Grim Tutor", Zone::Hand);
    mana(&mut state, PlayerId::P0, ManaColor::B, 1);
    mana(&mut state, PlayerId::P0, ManaColor::C, 2);
    let decision = next(&mut state);
    assert!(
        matches!(&decision, Decision::CastSpellOrPass { castable_spells, .. }
        if !castable_spells.contains(&tutor))
    );
    mana(&mut state, PlayerId::P0, ManaColor::B, 2);
    mana(&mut state, PlayerId::P0, ManaColor::C, 1);
    next(&mut state);
    engine::step(&mut state, Action::CastSpell(tutor)).unwrap();
    match settle(&mut state) {
        Decision::ChooseEffectTargets {
            player: PlayerId::P0,
            legal_targets,
            min_targets: 0,
            max_targets: 1,
            can_finish: true,
            ..
        } => {
            for card in [land, burst, boots] {
                assert!(legal_targets.contains(&Target::Object(card)));
            }
            assert_eq!(legal_targets.len(), 3);
        }
        other => panic!("expected the library search, got {other:?}"),
    }
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(burst)),
    )
    .unwrap();
    settle_to_priority(&mut state);
    assert_eq!(state.objects.get(burst).zone, Zone::Hand);
    assert_eq!(state.objects.get(tutor).zone, Zone::Graveyard);
    assert_eq!(life(&state, PlayerId::P0), 17);
    assert!(state
        .known_hand_cards(PlayerId::P1, PlayerId::P0)
        .is_empty());
}

#[test]
fn grim_tutor_may_fail_to_find_and_still_costs_life() {
    let mut state = ready();
    let land = put(&mut state, PlayerId::P0, "Plains", Zone::Library);
    let tutor = put(&mut state, PlayerId::P0, "Grim Tutor", Zone::Hand);
    mana(&mut state, PlayerId::P0, ManaColor::B, 2);
    mana(&mut state, PlayerId::P0, ManaColor::C, 1);
    next(&mut state);
    engine::step(&mut state, Action::CastSpell(tutor)).unwrap();
    assert!(matches!(
        settle(&mut state),
        Decision::ChooseEffectTargets { .. }
    ));
    engine::step(&mut state, Action::FinishEffectSelection).unwrap();
    settle_to_priority(&mut state);
    assert_eq!(state.objects.get(land).zone, Zone::Library);
    assert_eq!(life(&state, PlayerId::P0), 17);
}

#[test]
fn quick_draw_katana_grants_its_bonus_only_during_its_controllers_turn() {
    let mut state = ready();
    let katana = put(
        &mut state,
        PlayerId::P0,
        "Quick-Draw Katana",
        Zone::Battlefield,
    );
    let host = put(
        &mut state,
        PlayerId::P0,
        "Solemn Simulacrum",
        Zone::Battlefield,
    );
    mana(&mut state, PlayerId::P0, ManaColor::C, 1);
    let decision = next(&mut state);
    assert!(
        matches!(&decision, Decision::CastSpellOrPass { activatable_abilities, .. }
        if !activatable_abilities.contains(&(katana, 0)))
    );
    mana(&mut state, PlayerId::P0, ManaColor::C, 2);
    next(&mut state);
    engine::step(&mut state, Action::ActivateAbility(katana, 0)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(host))).unwrap();
    settle_to_priority(&mut state);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    assert_eq!(
        state.objects.get(katana).v4.attached_to.unwrap().object,
        host
    );

    assert_eq!(engine::effective_power(&state, host), 4);
    assert_eq!(engine::effective_toughness(&state, host), 2);
    assert!(engine::has_effective_keyword(
        &state,
        host,
        Keywords::FIRST_STRIKE
    ));

    let mut theirs = state.clone();
    theirs.active_player = PlayerId::P1;
    assert_eq!(engine::effective_power(&theirs, host), 2);
    assert_eq!(engine::effective_toughness(&theirs, host), 2);
    assert!(!engine::has_effective_keyword(
        &theirs,
        host,
        Keywords::FIRST_STRIKE
    ));

    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(katana, Zone::Graveyard),
    );
    next(&mut state);
    assert_eq!(engine::effective_power(&state, host), 2);
    assert!(!engine::has_effective_keyword(
        &state,
        host,
        Keywords::FIRST_STRIKE
    ));
}

/// Casts Burst Lightning at P1 with `generic` extra mana, answering the
/// kicker prompt (when offered) with `kick`. Returns whether it was offered.
fn cast_burst(generic: u8, kick: bool) -> (GameState, bool) {
    let mut state = ready();
    let burst = put(&mut state, PlayerId::P0, "Burst Lightning", Zone::Hand);
    mana(&mut state, PlayerId::P0, ManaColor::R, 1);
    mana(&mut state, PlayerId::P0, ManaColor::C, generic);
    next(&mut state);
    engine::step(&mut state, Action::CastSpell(burst)).unwrap();
    let mut offered = false;
    for _ in 0..8 {
        match next(&mut state) {
            Decision::ChooseKicker { player, spell } => {
                assert_eq!((player, spell), (PlayerId::P0, burst));
                offered = true;
                engine::step(&mut state, Action::ChooseKicker(kick)).unwrap();
            }
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Player(PlayerId::P1)));
                engine::step(
                    &mut state,
                    Action::ChooseTarget(Target::Player(PlayerId::P1)),
                )
                .unwrap();
            }
            Decision::CastSpellOrPass { .. } => break,
            other => panic!("unexpected cast decision: {other:?}"),
        }
    }
    settle_to_priority(&mut state);
    assert_eq!(state.objects.get(burst).zone, Zone::Graveyard);
    (state, offered)
}

#[test]
fn burst_lightning_deals_two_unkicked_and_four_kicked() {
    let (state, offered) = cast_burst(0, false);
    assert!(!offered, "kicker is not offered without {{4}} available");
    assert_eq!(life(&state, PlayerId::P1), 18);

    let (state, offered) = cast_burst(4, false);
    assert!(offered);
    assert_eq!(life(&state, PlayerId::P1), 18);
    assert_eq!(state.players[0].mana_pool[ManaColor::C.pool_index()], 4);

    let (state, offered) = cast_burst(4, true);
    assert!(offered);
    assert_eq!(life(&state, PlayerId::P1), 16);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

#[test]
fn burst_lightning_is_instant_speed_and_kills_a_creature() {
    let mut state = ready();
    state.active_player = PlayerId::P1;
    state.priority_player = PlayerId::P0;
    state.step = Step::BeginCombat;
    let burst = put(&mut state, PlayerId::P0, "Burst Lightning", Zone::Hand);
    let bear = put(
        &mut state,
        PlayerId::P1,
        "Solemn Simulacrum",
        Zone::Battlefield,
    );
    mana(&mut state, PlayerId::P0, ManaColor::R, 1);
    let decision = next(&mut state);
    assert!(
        matches!(&decision, Decision::CastSpellOrPass { player: PlayerId::P0, castable_spells, .. }
        if castable_spells.contains(&burst)),
        "{decision:?}"
    );
    engine::step(&mut state, Action::CastSpell(burst)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(bear))).unwrap();
    // Solemn's own dies trigger belongs to P1; decline the draw.
    match settle(&mut state) {
        Decision::ChooseEffectOption {
            player: PlayerId::P1,
            option_count: 2,
            ..
        } => engine::step(&mut state, Action::ChooseEffectOption(0)).unwrap(),
        other => panic!("expected the dies option, got {other:?}"),
    }
    settle_to_priority(&mut state);
    assert_eq!(state.objects.get(bear).zone, Zone::Graveyard);
}

#[test]
fn evolving_wilds_fetches_only_a_basic_land_tapped() {
    let mut state = ready();
    let plains = put(&mut state, PlayerId::P0, "Plains", Zone::Library);
    let other_wilds = put(&mut state, PlayerId::P0, "Evolving Wilds", Zone::Library);
    let creature = put(&mut state, PlayerId::P0, "Solemn Simulacrum", Zone::Library);
    let island = put(&mut state, PlayerId::P0, "Island", Zone::Library);
    let wilds = put(
        &mut state,
        PlayerId::P0,
        "Evolving Wilds",
        Zone::Battlefield,
    );

    let decision = next(&mut state);
    assert!(
        matches!(&decision, Decision::CastSpellOrPass { activatable_abilities, .. }
        if activatable_abilities.contains(&(wilds, 0))),
        "{decision:?}"
    );
    assert!(engine::step(&mut state, Action::ActivateManaAbility(wilds)).is_err());
    engine::step(&mut state, Action::ActivateAbility(wilds, 0)).unwrap();
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(state.objects.get(wilds).zone, Zone::Graveyard);
    match settle(&mut state) {
        Decision::ChooseEffectTargets {
            player: PlayerId::P0,
            legal_targets,
            min_targets: 0,
            max_targets: 1,
            can_finish: true,
            ..
        } => {
            let mut legal = legal_targets.clone();
            legal.sort_by_key(|target| match target {
                Target::Object(object) => object.0,
                Target::Player(_) => u32::MAX,
            });
            assert_eq!(legal, vec![Target::Object(plains), Target::Object(island)]);
            for illegal in [other_wilds, creature] {
                let hash = state.state_hash();
                assert!(engine::step(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(illegal))
                )
                .is_err());
                assert_eq!(state.state_hash(), hash);
            }
        }
        other => panic!("expected the library search, got {other:?}"),
    }
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(island)),
    )
    .unwrap();
    settle_to_priority(&mut state);
    assert_eq!(state.objects.get(island).zone, Zone::Battlefield);
    assert!(state.objects.get(island).tapped);
    assert_eq!(state.objects.get(plains).zone, Zone::Library);
    assert_eq!(state.players[0].library.len(), 3);
}

#[test]
fn evolving_wilds_may_fail_to_find() {
    let mut state = ready();
    let plains = put(&mut state, PlayerId::P0, "Plains", Zone::Library);
    let wilds = put(
        &mut state,
        PlayerId::P0,
        "Evolving Wilds",
        Zone::Battlefield,
    );
    next(&mut state);
    engine::step(&mut state, Action::ActivateAbility(wilds, 0)).unwrap();
    assert!(matches!(
        settle(&mut state),
        Decision::ChooseEffectTargets { .. }
    ));
    engine::step(&mut state, Action::FinishEffectSelection).unwrap();
    settle_to_priority(&mut state);
    assert_eq!(state.objects.get(plains).zone, Zone::Library);
    assert_eq!(state.objects.get(wilds).zone, Zone::Graveyard);
}

fn cast_solemn(state: &mut GameState) -> ObjectId {
    let solemn = put(state, PlayerId::P0, "Solemn Simulacrum", Zone::Hand);
    mana(state, PlayerId::P0, ManaColor::C, 3);
    let decision = next(state);
    assert!(
        matches!(&decision, Decision::CastSpellOrPass { castable_spells, .. }
        if !castable_spells.contains(&solemn)),
        "Solemn needs four mana"
    );
    mana(state, PlayerId::P0, ManaColor::C, 4);
    next(state);
    engine::step(state, Action::CastSpell(solemn)).unwrap();
    solemn
}

#[test]
fn solemn_simulacrum_entry_fetches_a_basic_land_tapped() {
    let mut state = ready();
    let forest = put(&mut state, PlayerId::P0, "Forest", Zone::Library);
    let wilds = put(&mut state, PlayerId::P0, "Evolving Wilds", Zone::Library);
    let solemn = cast_solemn(&mut state);
    match settle(&mut state) {
        Decision::ChooseEffectTargets {
            player: PlayerId::P0,
            source,
            legal_targets,
            min_targets: 0,
            max_targets: 1,
            ..
        } => {
            assert_eq!(source, solemn);
            assert_eq!(legal_targets, vec![Target::Object(forest)]);
        }
        other => panic!("expected the entry search, got {other:?}"),
    }
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(forest)),
    )
    .unwrap();
    settle_to_priority(&mut state);
    assert_eq!(state.objects.get(solemn).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(forest).zone, Zone::Battlefield);
    assert!(state.objects.get(forest).tapped);
    assert_eq!(state.objects.get(wilds).zone, Zone::Library);
}

#[test]
fn solemn_simulacrum_death_offers_an_optional_draw() {
    for draw in [false, true] {
        let mut state = ready();
        let card = put(&mut state, PlayerId::P0, "Plains", Zone::Library);
        let solemn = put(
            &mut state,
            PlayerId::P0,
            "Solemn Simulacrum",
            Zone::Battlefield,
        );
        next(&mut state);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(solemn, Zone::Graveyard),
        );
        match settle(&mut state) {
            Decision::ChooseEffectOption {
                player: PlayerId::P0,
                source,
                option_count: 2,
            } => assert_eq!(source, solemn),
            other => panic!("expected the dies option, got {other:?}"),
        }
        engine::step(&mut state, Action::ChooseEffectOption(u16::from(draw))).unwrap();
        settle_to_priority(&mut state);
        let expected = if draw { Zone::Hand } else { Zone::Library };
        assert_eq!(state.objects.get(card).zone, expected);
    }
}

#[test]
fn solemn_simulacrum_leaving_for_another_zone_does_not_draw() {
    let mut state = ready();
    put(&mut state, PlayerId::P0, "Plains", Zone::Library);
    let solemn = put(
        &mut state,
        PlayerId::P0,
        "Solemn Simulacrum",
        Zone::Battlefield,
    );
    next(&mut state);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(solemn, Zone::Hand));
    settle_to_priority(&mut state);
    assert!(state.stack.is_empty());
    assert_eq!(state.players[0].hand.len(), 1);
}

fn equip(state: &mut GameState, boots: ObjectId, target: ObjectId) {
    mana(state, PlayerId::P0, ManaColor::C, 1);
    next(state);
    engine::step(state, Action::ActivateAbility(boots, 0)).unwrap();
    assert!(matches!(next(state), Decision::ChooseTargets { .. }));
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    settle_to_priority(state);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    assert_eq!(
        state.objects.get(boots).v4.attached_to.unwrap().object,
        target
    );
    state.validate_attachment_relations().unwrap();
}

#[test]
fn swiftfoot_boots_cast_for_two_and_equip_for_one_at_sorcery_speed() {
    let mut state = ready();
    let boots = put(&mut state, PlayerId::P0, "Swiftfoot Boots", Zone::Hand);
    mana(&mut state, PlayerId::P0, ManaColor::C, 1);
    let decision = next(&mut state);
    assert!(
        matches!(&decision, Decision::CastSpellOrPass { castable_spells, .. }
        if !castable_spells.contains(&boots))
    );
    mana(&mut state, PlayerId::P0, ManaColor::C, 2);
    next(&mut state);
    engine::step(&mut state, Action::CastSpell(boots)).unwrap();
    settle_to_priority(&mut state);
    assert_eq!(state.objects.get(boots).zone, Zone::Battlefield);
    assert!(state.objects.get(boots).v4.attached_to.is_none());

    for (step, active, legal) in [
        (Step::BeginCombat, PlayerId::P0, false),
        (Step::Main1, PlayerId::P1, false),
        (Step::Main1, PlayerId::P0, true),
    ] {
        let mut state = state.clone();
        state.step = step;
        state.active_player = active;
        state.priority_player = PlayerId::P0;
        put(
            &mut state,
            PlayerId::P0,
            "Solemn Simulacrum",
            Zone::Battlefield,
        );
        mana(&mut state, PlayerId::P0, ManaColor::C, 1);
        let decision = next(&mut state);
        assert!(
            matches!(&decision, Decision::CastSpellOrPass { activatable_abilities, .. }
            if activatable_abilities.contains(&(boots, 0)) == legal),
            "step={step:?} active={active:?}: {decision:?}"
        );
    }
}

#[test]
fn swiftfoot_boots_grant_haste_and_hexproof_only_while_attached() {
    let mut state = ready();
    let boots = put(
        &mut state,
        PlayerId::P0,
        "Swiftfoot Boots",
        Zone::Battlefield,
    );
    let first = put(
        &mut state,
        PlayerId::P0,
        "Solemn Simulacrum",
        Zone::Battlefield,
    );
    let second = put(
        &mut state,
        PlayerId::P0,
        "Solemn Simulacrum",
        Zone::Battlefield,
    );
    state.objects.get_mut(first).summoning_sick = true;
    for keyword in [Keywords::HASTE, Keywords::HEXPROOF] {
        assert!(!engine::has_effective_keyword(&state, first, keyword));
    }
    equip(&mut state, boots, first);
    for keyword in [Keywords::HASTE, Keywords::HEXPROOF] {
        assert!(engine::has_effective_keyword(&state, first, keyword));
        assert!(!engine::has_effective_keyword(&state, second, keyword));
    }
    assert_eq!(engine::effective_power(&state, first), 2);
    assert_eq!(engine::effective_toughness(&state, first), 2);

    // The opponent cannot target the equipped creature, on either turn.
    let mut opposing = state.clone();
    opposing.active_player = PlayerId::P1;
    opposing.priority_player = PlayerId::P1;
    let burst = put(&mut opposing, PlayerId::P1, "Burst Lightning", Zone::Hand);
    mana(&mut opposing, PlayerId::P1, ManaColor::R, 1);
    next(&mut opposing);
    engine::step(&mut opposing, Action::CastSpell(burst)).unwrap();
    match next(&mut opposing) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(first)));
            assert!(legal_targets.contains(&Target::Object(second)));
        }
        other => panic!("expected targets, got {other:?}"),
    }

    // The summoning-sick equipped creature can attack.
    state.step = Step::DeclareAttackers;
    state.engine.combat.attackers_declared = false;
    state.engine.combat.blockers_declared = false;
    match next(&mut state) {
        Decision::DeclareAttackers { eligible, .. } => {
            assert!(eligible.contains(&first));
        }
        other => panic!("expected attackers, got {other:?}"),
    }

    // Re-equipping moves both grants.
    let mut moved = ready();
    let boots = put(
        &mut moved,
        PlayerId::P0,
        "Swiftfoot Boots",
        Zone::Battlefield,
    );
    let first = put(
        &mut moved,
        PlayerId::P0,
        "Solemn Simulacrum",
        Zone::Battlefield,
    );
    let second = put(
        &mut moved,
        PlayerId::P0,
        "Solemn Simulacrum",
        Zone::Battlefield,
    );
    equip(&mut moved, boots, first);
    equip(&mut moved, boots, second);
    for keyword in [Keywords::HASTE, Keywords::HEXPROOF] {
        assert!(!engine::has_effective_keyword(&moved, first, keyword));
        assert!(engine::has_effective_keyword(&moved, second, keyword));
    }

    // Removing the Boots removes both grants.
    event::propose_and_commit(
        &mut moved,
        ProposedEvent::zone_change(boots, Zone::Graveyard),
    );
    next(&mut moved);
    for keyword in [Keywords::HASTE, Keywords::HEXPROOF] {
        assert!(!engine::has_effective_keyword(&moved, second, keyword));
    }
    moved.validate_attachment_relations().unwrap();
}

fn equip_one(state: &mut GameState, equipment: ObjectId, target: ObjectId) {
    mana(state, PlayerId::P0, ManaColor::C, 1);
    next(state);
    engine::step(state, Action::ActivateAbility(equipment, 0)).unwrap();
    assert!(matches!(next(state), Decision::ChooseTargets { .. }));
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    settle_to_priority(state);
    assert_eq!(
        state.objects.get(equipment).v4.attached_to.unwrap().object,
        target
    );
}

fn play_land(state: &mut GameState, player: PlayerId) -> ObjectId {
    let land = put(state, player, "Forest", Zone::Hand);
    next(state);
    engine::step(state, Action::PlayLand(land)).unwrap();
    land
}

#[test]
fn adventuring_gear_landfall_pumps_the_equipped_creature_until_end_of_turn() {
    let mut state = ready();
    let gear = put(
        &mut state,
        PlayerId::P0,
        "Adventuring Gear",
        Zone::Battlefield,
    );
    let host = put(
        &mut state,
        PlayerId::P0,
        "Solemn Simulacrum",
        Zone::Battlefield,
    );
    let other = put(
        &mut state,
        PlayerId::P0,
        "Solemn Simulacrum",
        Zone::Battlefield,
    );
    equip_one(&mut state, gear, host);
    assert_eq!(engine::effective_power(&state, host), 2);

    play_land(&mut state, PlayerId::P0);
    settle_to_priority(&mut state);
    assert_eq!(engine::effective_power(&state, host), 4);
    assert_eq!(engine::effective_toughness(&state, host), 4);
    assert_eq!(engine::effective_power(&state, other), 2);

    // A second land the same turn stacks another +2/+2.
    state.players[0].lands_played_this_turn = 0;
    play_land(&mut state, PlayerId::P0);
    settle_to_priority(&mut state);
    assert_eq!(engine::effective_power(&state, host), 6);

    // Moving the Gear leaves the earlier boosts on the original host.
    equip_one(&mut state, gear, other);
    assert_eq!(engine::effective_power(&state, host), 6);
    assert_eq!(engine::effective_power(&state, other), 2);
}

#[test]
fn adventuring_gear_ignores_opposing_lands_and_does_nothing_unattached() {
    let mut state = ready();
    let gear = put(
        &mut state,
        PlayerId::P0,
        "Adventuring Gear",
        Zone::Battlefield,
    );
    let host = put(
        &mut state,
        PlayerId::P0,
        "Solemn Simulacrum",
        Zone::Battlefield,
    );
    equip_one(&mut state, gear, host);
    state.active_player = PlayerId::P1;
    state.priority_player = PlayerId::P1;
    let opposing = play_land(&mut state, PlayerId::P1);
    settle_to_priority(&mut state);
    assert_eq!(state.objects.get(opposing).zone, Zone::Battlefield);
    assert_eq!(engine::effective_power(&state, host), 2);

    let mut loose = ready();
    put(
        &mut loose,
        PlayerId::P0,
        "Adventuring Gear",
        Zone::Battlefield,
    );
    let creature = put(
        &mut loose,
        PlayerId::P0,
        "Solemn Simulacrum",
        Zone::Battlefield,
    );
    play_land(&mut loose, PlayerId::P0);
    settle_to_priority(&mut loose);
    assert_eq!(engine::effective_power(&loose, creature), 2);
}

#[test]
fn adventuring_gear_boosts_the_host_it_is_attached_to_at_resolution() {
    let mut state = ready();
    let gear = put(
        &mut state,
        PlayerId::P0,
        "Adventuring Gear",
        Zone::Battlefield,
    );
    let host = put(
        &mut state,
        PlayerId::P0,
        "Solemn Simulacrum",
        Zone::Battlefield,
    );
    equip_one(&mut state, gear, host);
    play_land(&mut state, PlayerId::P0);
    // The landfall trigger is on the stack; its host leaves before it resolves.
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert!(!state.stack.is_empty());
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(host, Zone::Hand));
    settle_to_priority(&mut state);
    assert!(state.engine.until_end_of_turn.is_empty());
}

fn attack_unblocked(state: &mut GameState, attacker: ObjectId) {
    state.step = Step::DeclareAttackers;
    state.engine.combat.attackers_declared = false;
    state.engine.combat.blockers_declared = false;
    match next(state) {
        Decision::DeclareAttackers { eligible, .. } => assert!(eligible.contains(&attacker)),
        other => panic!("expected attackers, got {other:?}"),
    }
    engine::step(state, Action::DeclareAttackers(vec![attacker])).unwrap();
    for _ in 0..40 {
        match next(state) {
            Decision::DeclareBlockers { .. } => {
                engine::step(state, Action::DeclareBlockers(Vec::new())).unwrap();
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.step == Step::Main2 =>
            {
                return;
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected combat decision: {other:?}"),
        }
    }
    panic!("combat did not reach the second main phase");
}

fn treasures(state: &GameState, player: PlayerId) -> usize {
    let treasure = id("Treasure Token");
    state.players[player.index()]
        .battlefield
        .iter()
        .filter(|&&object| state.objects.get(object).card_def == treasure)
        .count()
}

#[test]
fn goldvein_pick_pumps_and_makes_treasure_on_combat_damage_to_a_player() {
    let mut state = ready();
    let pick = put(&mut state, PlayerId::P0, "Goldvein Pick", Zone::Battlefield);
    let host = put(
        &mut state,
        PlayerId::P0,
        "Solemn Simulacrum",
        Zone::Battlefield,
    );
    let other = put(
        &mut state,
        PlayerId::P0,
        "Solemn Simulacrum",
        Zone::Battlefield,
    );
    equip_one(&mut state, pick, host);
    assert_eq!(engine::effective_power(&state, host), 3);
    assert_eq!(engine::effective_toughness(&state, host), 3);

    let mut unequipped = state.clone();
    attack_unblocked(&mut state, host);
    assert_eq!(life(&state, PlayerId::P1), 17);
    assert_eq!(treasures(&state, PlayerId::P0), 1);

    attack_unblocked(&mut unequipped, other);
    assert_eq!(life(&unequipped, PlayerId::P1), 18);
    assert_eq!(treasures(&unequipped, PlayerId::P0), 0);
}
