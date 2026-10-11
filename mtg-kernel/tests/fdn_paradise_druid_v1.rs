//! Unregistered Paradise Druid fixtures; actual gameplay admission remains pending.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready(active: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        1109,
        active,
    );
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
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Library => state.players[player.index()].library.push(id),
        _ => panic!("fixture zone"),
    }
    id
}

fn next(state: &mut GameState) -> Decision {
    let choice = engine::advance_until_decision(state);
    assert!(!matches!(choice, Decision::Halted { .. }), "{choice:?}");
    choice
}

fn priority(state: &mut GameState, player: PlayerId) {
    let Decision::CastSpellOrPass {
        player: chooser, ..
    } = next(state)
    else {
        panic!("priority decision absent");
    };
    if chooser != player {
        engine::step(state, Action::Pass).unwrap();
        assert!(
            matches!(next(state), Decision::CastSpellOrPass { player: chooser, .. } if chooser == player)
        );
    }
}

fn cast(state: &mut GameState, spell: ObjectId, target: Option<Target>) {
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell))
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    for _ in 0..16 {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                let target = target.expect("fixture target");
                assert!(legal_targets.contains(&target));
                engine::step(state, Action::ChooseTarget(target)).unwrap();
            }
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap();
            }
            Decision::CastSpellOrPass { .. } if state.engine.pending_cast.is_none() => return,
            choice => panic!("unexpected cast decision {choice:?}"),
        }
    }
    panic!("cast did not finish");
}

fn settle(state: &mut GameState) -> Decision {
    for _ in 0..64 {
        let choice = next(state);
        match choice {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return choice
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { ref pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap();
            }
            _ => return choice,
        }
    }
    panic!("resolution did not settle");
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn same(state: &GameState, replay: &GameState) {
    assert_eq!(
        serde_json::to_value(state).unwrap(),
        serde_json::to_value(replay).unwrap()
    );
}

#[test]
fn metadata_actual_cast_and_summoning_sickness_keep_conditional_hexproof() {
    let def = &CARD_DEFS[card_id_by_name("Paradise Druid").unwrap() as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Elf, Subtype::Druid]);
    assert_eq!(
        (def.power, def.toughness, def.mana_value),
        (Some(2), Some(1), 2)
    );
    assert_eq!(def.cost.generic, 1);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::G)]);
    assert!(!def.keywords.has(Keywords::HEXPROOF));
    assert_eq!(
        def.produces_mana,
        &[
            ManaColor::W,
            ManaColor::U,
            ManaColor::B,
            ManaColor::R,
            ManaColor::G
        ]
    );
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Paradise Druid", Zone::Hand);
        state.players[player.index()].mana_pool[4] = 1;
        state.players[player.index()].mana_pool[5] = 1;
        priority(&mut state, player);
        cast(&mut state, source, None);
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            assert!(matches!(settle(current), Decision::CastSpellOrPass { .. }));
            assert_eq!(current.objects.get(source).zone, Zone::Battlefield);
            assert!(current.objects.get(source).summoning_sick);
            assert!(engine::has_effective_keyword(
                current,
                source,
                Keywords::HEXPROOF
            ));
            priority(current, player);
            let Decision::CastSpellOrPass { mana_abilities, .. } = next(current) else {
                panic!("priority")
            };
            assert!(!mana_abilities.contains(&source));
            let before = serde_json::to_vec(&*current).unwrap();
            assert!(engine::step(
                current,
                Action::ActivateManaAbilityChoice(source, ManaColor::G)
            )
            .is_err());
            assert_eq!(serde_json::to_vec(&*current).unwrap(), before);
        }
        same(&state, &replay);
    }
}

#[test]
fn each_real_mana_color_taps_and_immediately_removes_hexproof_after_restore() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for color in [
            ManaColor::W,
            ManaColor::U,
            ManaColor::B,
            ManaColor::R,
            ManaColor::G,
        ] {
            let mut state = ready(player);
            let source = put(&mut state, player, "Paradise Druid", Zone::Battlefield);
            priority(&mut state, player);
            assert!(engine::has_effective_keyword(
                &state,
                source,
                Keywords::HEXPROOF
            ));
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                engine::step(current, Action::ActivateManaAbilityChoice(source, color)).unwrap();
                assert!(current.objects.get(source).tapped);
                assert_eq!(
                    current.players[player.index()].mana_pool[color.pool_index()],
                    1
                );
                assert!(!engine::has_effective_keyword(
                    current,
                    source,
                    Keywords::HEXPROOF
                ));
                assert!(current.stack.is_empty());
                let before = serde_json::to_vec(&*current).unwrap();
                assert!(
                    engine::step(current, Action::ActivateManaAbilityChoice(source, color))
                        .is_err()
                );
                assert_eq!(serde_json::to_vec(&*current).unwrap(), before);
            }
            same(&state, &replay);
        }
    }
}

#[test]
fn opponent_targeting_excludes_untapped_source_and_own_targeting_remains_legal() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player.opponent());
        let source = put(&mut state, player, "Paradise Druid", Zone::Battlefield);
        let other = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
        let terror = put(&mut state, player.opponent(), "Terror", Zone::Hand);
        state.players[player.opponent().index()].mana_pool[2] = 1;
        state.players[player.opponent().index()].mana_pool[5] = 1;
        priority(&mut state, player.opponent());
        engine::step(&mut state, Action::CastSpell(terror)).unwrap();
        let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) else {
            panic!("target prompt")
        };
        assert!(!legal_targets.contains(&Target::Object(source)));
        assert!(legal_targets.contains(&Target::Object(other)));
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::ChooseTarget(Target::Object(source))).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            engine::step(current, Action::ChooseTarget(Target::Object(other))).unwrap();
            settle(current);
            assert_eq!(current.objects.get(other).zone, Zone::Graveyard);
            assert_eq!(current.objects.get(source).zone, Zone::Battlefield);
        }
        same(&state, &replay);

        let mut state = ready(player);
        let source = put(&mut state, player, "Paradise Druid", Zone::Battlefield);
        let growth = put(&mut state, player, "Giant Growth", Zone::Hand);
        state.players[player.index()].mana_pool[4] = 1;
        priority(&mut state, player);
        cast(&mut state, growth, Some(Target::Object(source)));
        settle(&mut state);
        assert_eq!(engine::effective_power(&state, source), 5);
        assert!(engine::has_effective_keyword(
            &state,
            source,
            Keywords::HEXPROOF
        ));
    }
}

#[test]
fn actual_targeted_spell_rechecks_hexproof_after_untap_and_restore() {
    use mtg_kernel::effect::{self, EffectOp, ExecCtx, ObjectRef};
    for player in [PlayerId::P0, PlayerId::P1] {
        for untap in [false, true] {
            let mut state = ready(player.opponent());
            let source = put(&mut state, player, "Paradise Druid", Zone::Battlefield);
            event::propose_and_commit(&mut state, ProposedEvent::tap(source));
            let terror = put(&mut state, player.opponent(), "Terror", Zone::Hand);
            state.players[player.opponent().index()].mana_pool[2] = 1;
            state.players[player.opponent().index()].mana_pool[5] = 1;
            priority(&mut state, player.opponent());
            cast(&mut state, terror, Some(Target::Object(source)));
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                if untap {
                    effect::execute(
                        &EffectOp::UntapObject {
                            object: ObjectRef::ThisSource,
                        },
                        &ExecCtx::no_targets(source, player),
                        current,
                    );
                    assert!(engine::has_effective_keyword(
                        current,
                        source,
                        Keywords::HEXPROOF
                    ));
                }
                settle(current);
                assert_eq!(
                    current.objects.get(source).zone,
                    if untap {
                        Zone::Battlefield
                    } else {
                        Zone::Graveyard
                    }
                );
                assert_eq!(current.objects.get(terror).zone, Zone::Graveyard);
            }
            same(&state, &replay);
        }
    }
}

#[test]
fn witness_protection_suppresses_both_printed_abilities_and_removal_restores_them() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Paradise Druid", Zone::Battlefield);
        let aura = put(&mut state, player, "Witness Protection", Zone::Hand);
        state.players[player.index()].mana_pool[1] = 1;
        priority(&mut state, player);
        cast(&mut state, aura, Some(Target::Object(source)));
        settle(&mut state);
        priority(&mut state, player);
        assert!(!engine::has_effective_keyword(
            &state,
            source,
            Keywords::HEXPROOF
        ));
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(
            &mut state,
            Action::ActivateManaAbilityChoice(source, ManaColor::G)
        )
        .is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            event::propose_and_commit(current, ProposedEvent::zone_change(aura, Zone::Graveyard));
            assert!(engine::has_effective_keyword(
                current,
                source,
                Keywords::HEXPROOF
            ));
            engine::step(
                current,
                Action::ActivateManaAbilityChoice(source, ManaColor::G),
            )
            .unwrap();
            assert!(!engine::has_effective_keyword(
                current,
                source,
                Keywords::HEXPROOF
            ));
            assert_eq!(current.players[player.index()].mana_pool[4], 1);
        }
        same(&state, &replay);
    }
}

#[test]
fn conditional_keyword_uses_current_controller_zone_face_and_tap_state() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let source = put(&mut state, player, "Paradise Druid", Zone::Battlefield);
        assert!(engine::has_effective_keyword(
            &state,
            source,
            Keywords::HEXPROOF
        ));
        state.objects.get_mut(source).controller = player.opponent();
        state.players[player.index()]
            .battlefield
            .retain(|id| *id != source);
        state.players[player.opponent().index()]
            .battlefield
            .push(source);
        assert!(engine::has_effective_keyword(
            &state,
            source,
            Keywords::HEXPROOF
        ));
        state.priority_player = player;
        assert!(!engine::legal_targets_for(
            mtg_kernel::card_def::TargetSpec::Creature,
            &[],
            &state
        )
        .contains(&Target::Object(source)));
        state.priority_player = player.opponent();
        assert!(
            engine::legal_targets_for(mtg_kernel::card_def::TargetSpec::Creature, &[], &state)
                .contains(&Target::Object(source))
        );
        state.objects.get_mut(source).v4.face_index = 1;
        assert!(!engine::has_effective_keyword(
            &state,
            source,
            Keywords::HEXPROOF
        ));
        state.objects.get_mut(source).v4.face_index = 0;
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        assert!(!engine::has_effective_keyword(
            &state,
            source,
            Keywords::HEXPROOF
        ));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        assert!(engine::has_effective_keyword(
            &state,
            source,
            Keywords::HEXPROOF
        ));
        let replay = restored(&state);
        assert!(engine::has_effective_keyword(
            &replay,
            source,
            Keywords::HEXPROOF
        ));
        same(&state, &replay);
    }
}
