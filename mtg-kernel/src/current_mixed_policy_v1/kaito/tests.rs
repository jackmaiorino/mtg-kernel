//! Actual paid native bindings and numeric model source parity. Rust unrun.
use super::*;
use crate::{
    effect::kaito_cunning_v75::tests::owner_v75_tests::owning_ready, ids::PlayerId,
    kaito_public_v75::StageV75, rl::ActionSemanticV1 as A,
};
mod archmage;
mod attack_triggers;
mod balmor;
mod bloodwitch;
mod bulk_up;
mod burst_lightning;
mod conditional_player_spell;
mod cost_reduced_spell;
mod counter_return_creatures;
mod creature_power_spell;
mod discard_cost_spell;
mod dragon_observers;
mod entry_branch;
mod entry_destruction;
mod entry_exile;
mod evolving;
mod exsanguinate;
mod goblin_surprise;
mod graveyard_batch_spell;
mod guarded_heir;
mod incinerating_blast;
mod library_search;
mod lookout;
mod luminous;
mod lunar_insight;
mod macabre_waltz;
mod micromancer;
mod observer_triggers;
mod ordinary_auras;
mod ordinary_batch_spells;
mod ordinary_equipment;
mod paladin;
mod pilfer;
mod sacrifice;
mod sailor;
mod seekers_folly;
mod shivan;
mod simple_triggers;
mod solemn;
mod surveil_many;
mod surveil_one;
mod syphoner;
mod targeted_entry;
mod team_statics;
mod threshold_loot;
mod triggered_enchantments;
mod trufflesnout;
mod unicorn;
mod utility_activations;
mod ward_discount_creatures;

/// Full-pool opening-hand coverage is distinct from paid branch qualification.
/// Every frozen printing must survive the current owning projection and numeric
/// encoder, even when it has no legal cast target in this initial mana scene.
#[test]
fn every_frozen_fdn_printing_in_own_hand_has_current_numeric_coverage_both_seats() {
    fn check(name: &str, p: PlayerId, model: &NativeModel) -> std::result::Result<(), String> {
        use crate::{
            policy_observation_v6::tests::put,
            state::{GameState, Step, Zone},
        };
        let id = crate::card_def::card_id_by_name(name).ok_or("unregistered frozen printing")?;
        let forest = crate::card_def::card_id_by_name("Forest").unwrap();
        let island = crate::card_def::card_id_by_name("Island").unwrap();
        let mut s = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[island; 40],
            crate::rl::card_name,
            110_286,
            p,
        );
        s.step = Step::Main1;
        // Initial mana resources, not a fabricated payment or paid owner.
        for land in ["Plains", "Island", "Swamp", "Mountain", "Forest"] {
            for _ in 0..3 {
                put(&mut s, p, land, Zone::Battlefield);
            }
        }
        let card = put(&mut s, p, name, Zone::Hand);
        let own = KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
            .map_err(|e| format!("owning session: {e}"))?;
        let FastActorResponseV1::Decision(h) = own.current_response() else {
            return Err(format!("no owning decision: {:?}", own.current_response()));
        };
        let d = own.encode(h).map_err(|e| format!("numeric encode: {e}"))?;
        if !d
            .backend
            .observation()
            .common
            .common
            .own_hand
            .iter()
            .any(|c| {
                c.stable.arena_id == card.0 && c.stable.card_db_id == id && c.card_name == name
            })
        {
            return Err("issued observation omitted the actual own-hand printing".into());
        }
        let scores = model
            .forward(d.packet())
            .map_err(|e| format!("model forward: {e}"))?;
        if d.packet().source() != Source::KaitoV75
            || scores.len() != d.backend.actions().len()
            || scores.is_empty()
            || scores.iter().any(|score| !score.is_finite())
        {
            return Err("model did not score every original candidate finitely".into());
        }
        Ok(())
    }
    let target: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../data/limited/fdn_v1/booster_pool_v1.json"
    )))
    .unwrap();
    let names = target["card_names"].as_array().unwrap();
    assert_eq!(names.len(), 286);
    let model = NativeModel::deterministic_fixture().unwrap();
    let mut failures = Vec::new();
    for name in names {
        let name = name.as_str().unwrap();
        for p in [PlayerId::P0, PlayerId::P1] {
            eprintln!("full286 opening-hand printing={name} seat={p:?}");
            if let Err(error) = check(name, p, &model) {
                failures.push(format!("{name} {p:?}: {error}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "full286 opening-hand coverage gaps:\n{}",
        failures.join("\n")
    );
}

fn act(own: &mut KaitoSession, native: &mut FastActorSessionV1, choose: impl Fn(&A) -> bool) {
    let FastActorResponseV1::Decision(h) = own.current_response() else {
        panic!("numeric V75 ended")
    };
    let d = own.encode(h).unwrap();
    let i = d
        .backend
        .actions()
        .iter()
        .position(choose)
        .unwrap_or_else(|| panic!("missing numeric V75 action: {:?}", d.backend.actions()));
    let original = d.backend.fixture_original_index_v75(i as u32).unwrap();
    let scores = NativeModel::deterministic_fixture()
        .unwrap()
        .forward(d.packet())
        .unwrap();
    assert_eq!(scores.len(), d.backend.actions().len());
    assert!(scores.iter().all(|v| v.is_finite()));
    assert_eq!(d.packet().source(), Source::KaitoV75);
    assert_eq!(
        own.apply(&d, i as u32).unwrap(),
        native.step(h.episode_id, h.step, original).unwrap()
    );
    assert_eq!(own.fixture_state(), native.kaito_fixture_state());
    assert_eq!(
        own.fixture_counts(),
        (native.policy_step_count(), native.physical_decision_count())
    );
}

// Native spell-target menus may identify the physical spell as an Object in
// Zone::Stack. A StackItem spelling must likewise identify an actual spell,
// never an ability or a source permanent on the battlefield.
fn is_original_spell_target(action: &A, original: crate::ids::ObjectId) -> bool {
    match action {
        A::ChooseTarget {
            target: crate::rl::TargetRefV1::Object { object },
            ..
        } => object.arena_id == original.0 && object.zone == crate::state::Zone::Stack,
        A::ChooseTarget {
            target:
                crate::rl::TargetRefV1::StackItem {
                    source,
                    kind: crate::rl::StackItemKindV2::Spell,
                    ..
                },
            ..
        } => source.arena_id == original.0 && source.zone == crate::state::Zone::Stack,
        _ => false,
    }
}

#[test]
fn gainlands_actual_play_entry_life_and_each_native_mana_color_keep_numeric_parity_both_seats() {
    use crate::{
        policy_observation_v6::tests::put,
        state::{GameState, Step, Zone},
    };
    let names = [
        "Blossoming Sands",
        "Thornwood Falls",
        "Bloodfell Caves",
        "Dismal Backwater",
        "Jungle Hollow",
        "Rugged Highlands",
        "Scoured Barrens",
        "Swiftwater Cliffs",
        "Tranquil Cove",
        "Wind-Scarred Crag",
    ];
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for name in names {
        let id = crate::card_def::card_id_by_name(name).unwrap();
        let meaning = super::super::roots::baseline(id).unwrap();
        assert_eq!(
            meaning["gain_land_printing"]["ordinary_play"]["is_cast"],
            false
        );
        assert_eq!(
            meaning["gain_land_printing"]["mana_ability"]["choose_exactly_one"],
            true
        );
        assert_eq!(
            meaning["gain_land_printing"]["entry_trigger"]["ordered_resolution"][0]["amount"],
            1
        );
        for p in [PlayerId::P0, PlayerId::P1] {
            let scene = || {
                let mut s = GameState::new_from_libraries_with_starting_player_v1(
                    &[forest; 40],
                    &[forest; 40],
                    crate::rl::card_name,
                    110_286,
                    p,
                );
                s.step = Step::Main1;
                s
            };
            let pair = |s: GameState| {
                let native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
                let own =
                    KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
                        .unwrap();
                (own, native)
            };
            let mut s = scene();
            let land = put(&mut s, p, name, Zone::Hand);
            let (mut own, mut native) = pair(s);
            act(
                &mut own,
                &mut native,
                |a| matches!(a, A::PlayLand { source, .. } if source.arena_id == land.0),
            );
            assert_eq!(
                own.fixture_state().objects.get(land).zone,
                Zone::Battlefield
            );
            assert!(own.fixture_state().objects.get(land).tapped);
            for _ in 0..16 {
                if own.fixture_state().players[p.index()].life == 21 {
                    break;
                }
                act(&mut own, &mut native, |a| {
                    matches!(
                        a,
                        A::Pass { .. } | A::OrderTriggers { .. } | A::ChooseTriggerOrderNext { .. }
                    )
                });
            }
            assert_eq!(own.fixture_state().players[p.index()].life, 21);
            assert_eq!(own.fixture_state().players[p.opponent().index()].life, 20);
            assert_eq!(
                own.fixture_state().players[p.index()].lands_played_this_turn,
                1
            );
            for color in crate::card_def::CARD_DEFS[usize::from(id)].mana_ability_choices {
                // A separate initial untapped battlefield scene checks each
                // mana alternative. It is not a claim that entry untapped.
                let mut s = scene();
                let land = put(&mut s, p, name, Zone::Battlefield);
                s.objects.get_mut(land).tapped = false;
                let (mut own, mut native) = pair(s);
                act(
                    &mut own,
                    &mut native,
                    |a| matches!(a, A::ActivateManaAbility { source, mana_choice: Some(selected), .. } if source.arena_id == land.0 && selected == color),
                );
                assert!(own.fixture_state().objects.get(land).tapped);
                assert_eq!(
                    own.fixture_state().players[p.index()].mana_pool[color.pool_index()],
                    1
                );
                assert_eq!(
                    own.fixture_state().players[p.index()]
                        .mana_pool
                        .iter()
                        .copied()
                        .map(u16::from)
                        .sum::<u16>(),
                    1
                );
                assert_eq!(own.fixture_state().players[p.index()].life, 20);
            }
        }
    }
}

#[test]
fn five_untargeted_printings_actual_paid_cast_resolution_numeric_parity_and_restore_both_seats() {
    use crate::{
        engine::{self, Action, Decision},
        policy_observation_v6::tests::put,
        state::{GameState, Step, Zone},
    };
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for name in [
        "Aetherize",
        "Boltwave",
        "Day of Judgment",
        "Overrun",
        "Preposterous Proportions",
    ] {
        for p in [PlayerId::P0, PlayerId::P1] {
            let aetherize = name == "Aetherize";
            let active = if aetherize { p.opponent() } else { p };
            let mut s = GameState::new_from_libraries_with_starting_player_v1(
                &[forest; 40],
                &[forest; 40],
                crate::rl::card_name,
                110_287,
                active,
            );
            s.step = Step::Main1;
            let land = match name {
                "Aetherize" => "Island",
                "Boltwave" => "Mountain",
                "Day of Judgment" => "Plains",
                _ => "Forest",
            };
            for _ in 0..8 {
                put(&mut s, p, land, Zone::Battlefield);
            }
            let friendly = put(&mut s, p, "Llanowar Elves", Zone::Battlefield);
            let opposing = put(&mut s, p.opponent(), "Llanowar Elves", Zone::Battlefield);
            let idle = put(&mut s, p.opponent(), "Llanowar Elves", Zone::Battlefield);
            let later = put(&mut s, p, "Llanowar Elves", Zone::Hand);
            let spell = put(&mut s, p, name, Zone::Hand);
            if aetherize {
                s.step = Step::DeclareAttackers;
                assert!(matches!(
                    engine::advance_until_decision(&mut s),
                    Decision::DeclareAttackers { .. }
                ));
                engine::step(&mut s, Action::DeclareAttackers(vec![opposing])).unwrap();
                assert!(
                    matches!(engine::advance_until_decision(&mut s), Decision::CastSpellOrPass { player, .. } if player == active)
                );
                engine::step(&mut s, Action::Pass).unwrap();
                assert!(
                    matches!(engine::advance_until_decision(&mut s), Decision::CastSpellOrPass { player, .. } if player == p)
                );
            }
            let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
            let mut own =
                KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
                    .unwrap();
            act(
                &mut own,
                &mut native,
                |a| matches!(a, A::CastSpell { source, .. } if source.arena_id == spell.0),
            );
            assert!(own.fixture_state().engine.pending_cast.is_none());
            assert_eq!(own.fixture_state().objects.get(spell).zone, Zone::Stack);
            let def = &crate::card_def::CARD_DEFS
                [usize::from(crate::card_def::card_id_by_name(name).unwrap())];
            let tapped = own.fixture_state().players[p.index()]
                .battlefield
                .iter()
                .filter(|&&id| own.fixture_state().objects.get(id).tapped)
                .count();
            assert_eq!(tapped, usize::from(def.cost.generic) + def.cost.pips.len());
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            settle(&mut own, &mut native);
            assert_eq!(own.fixture_state().objects.get(spell).zone, Zone::Graveyard);
            match name {
                "Aetherize" => {
                    assert_eq!(own.fixture_state().objects.get(opposing).zone, Zone::Hand);
                    assert!(own.fixture_state().players[p.opponent().index()]
                        .hand
                        .contains(&opposing));
                    assert_eq!(
                        own.fixture_state().objects.get(idle).zone,
                        Zone::Battlefield
                    );
                    assert_eq!(
                        own.fixture_state().objects.get(friendly).zone,
                        Zone::Battlefield
                    );
                    assert!(!own
                        .fixture_state()
                        .engine
                        .combat
                        .attackers
                        .contains(&opposing));
                }
                "Boltwave" => {
                    assert_eq!(own.fixture_state().players[p.opponent().index()].life, 17);
                    assert_eq!(own.fixture_state().players[p.index()].life, 20);
                    assert_eq!(
                        own.fixture_state().objects.get(opposing).zone,
                        Zone::Battlefield
                    );
                }
                "Day of Judgment" => {
                    for id in [friendly, opposing, idle] {
                        assert_eq!(own.fixture_state().objects.get(id).zone, Zone::Graveyard);
                    }
                }
                _ => {
                    let amount = if name == "Overrun" { 3 } else { 10 };
                    let keyword = if name == "Overrun" {
                        crate::card_def::Keywords::TRAMPLE
                    } else {
                        crate::card_def::Keywords::VIGILANCE
                    };
                    assert_eq!(
                        engine::checked_effective_pt(own.fixture_state(), friendly).unwrap(),
                        (1 + amount, 1 + amount)
                    );
                    assert!(engine::has_effective_keyword(
                        own.fixture_state(),
                        friendly,
                        keyword
                    ));
                    assert_eq!(
                        engine::checked_effective_pt(own.fixture_state(), opposing).unwrap(),
                        (1, 1)
                    );
                }
            }
            let expected = own.fixture_state().clone();
            own.restore(&saved).unwrap();
            native.kaito_restore_v75(&raw).unwrap();
            settle(&mut own, &mut native);
            assert_eq!(own.fixture_state(), &expected);
            if matches!(name, "Overrun" | "Preposterous Proportions") {
                act(
                    &mut own,
                    &mut native,
                    |a| matches!(a, A::CastSpell { source, .. } if source.arena_id == later.0),
                );
                settle(&mut own, &mut native);
                assert_eq!(
                    engine::checked_effective_pt(own.fixture_state(), later).unwrap(),
                    (1, 1)
                );
            }
        }
    }
}

#[test]
fn eight_targeted_printings_paid_target_payment_resolution_and_pending_restore_both_seats() {
    check_targeted_printings(&[
        "Stab",
        "Fleeting Distraction",
        "Sure Strike",
        "Snakeskin Veil",
        "Hero's Downfall",
        "Broken Wings",
        "Make Your Move",
        "Zombify",
    ]);
}

#[test]
fn flight_and_pie_paid_target_payment_complete_resolution_and_pending_restore_both_seats() {
    check_targeted_printings(&["Fleeting Flight", "Bake into a Pie"]);
}

#[test]
fn joust_paid_actual_attacker_target_filter_damage_life_and_pending_restore_both_seats() {
    check_targeted_printings(&["Joust Through"]);
}

fn check_targeted_printings(names: &[&str]) {
    use crate::{
        engine,
        policy_observation_v6::tests::put,
        state::{GameState, Step, Zone},
    };
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for &name in names {
        for p in [PlayerId::P0, PlayerId::P1] {
            let mut s = GameState::new_from_libraries_with_starting_player_v1(
                &[forest; 40],
                &[forest; 40],
                crate::rl::card_name,
                110_288,
                if name == "Joust Through" {
                    p.opponent()
                } else {
                    p
                },
            );
            s.step = Step::Main1;
            let land = match name {
                "Stab" | "Hero's Downfall" | "Zombify" | "Bake into a Pie" => "Swamp",
                "Fleeting Distraction" => "Island",
                "Sure Strike" => "Mountain",
                "Make Your Move" | "Fleeting Flight" | "Joust Through" => "Plains",
                _ => "Forest",
            };
            for _ in 0..5 {
                put(&mut s, p, land, Zone::Battlefield);
            }
            let friendly = put(&mut s, p, "Llanowar Elves", Zone::Battlefield);
            let opposing = put(&mut s, p.opponent(), "Llanowar Elves", Zone::Battlefield);
            let target = match name {
                "Broken Wings" => put(&mut s, p.opponent(), "Serra Angel", Zone::Battlefield),
                "Make Your Move" => put(&mut s, p.opponent(), "Serra Angel", Zone::Battlefield),
                "Zombify" => put(&mut s, p, "Llanowar Elves", Zone::Graveyard),
                "Sure Strike" | "Snakeskin Veil" | "Fleeting Flight" => friendly,
                _ => opposing,
            };
            let spell = put(&mut s, p, name, Zone::Hand);
            let library_before = s.players[p.index()].library.len();
            if name == "Joust Through" {
                s.step = Step::DeclareAttackers;
                assert!(matches!(engine::advance_until_decision(&mut s),
                    engine::Decision::DeclareAttackers { player, ref eligible }
                    if player == p.opponent() && eligible.contains(&opposing)));
                engine::step(&mut s, engine::Action::DeclareAttackers(vec![opposing])).unwrap();
                assert!(matches!(engine::advance_until_decision(&mut s),
                    engine::Decision::CastSpellOrPass { player, .. } if player == p.opponent()));
                engine::step(&mut s, engine::Action::Pass).unwrap();
                assert!(matches!(engine::advance_until_decision(&mut s),
                    engine::Decision::CastSpellOrPass { player, .. } if player == p));
            }
            let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
            let mut own =
                KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
                    .unwrap();
            act(
                &mut own,
                &mut native,
                |a| matches!(a,A::CastSpell{source,..}if source.arena_id==spell.0),
            );
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            let mut completed = None;
            for replay in [false, true] {
                if replay {
                    own.restore(&saved).unwrap();
                    native.kaito_restore_v75(&raw).unwrap();
                }
                if matches!(name, "Snakeskin Veil" | "Broken Wings" | "Make Your Move") {
                    let FastActorResponseV1::Decision(h) = own.current_response() else {
                        panic!()
                    };
                    let d = own.encode(h).unwrap();
                    assert!(!d.backend.actions().iter().any(|a|matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==opposing.0)));
                }
                if name == "Joust Through" {
                    let FastActorResponseV1::Decision(h) = own.current_response() else {
                        panic!()
                    };
                    let d = own.encode(h).unwrap();
                    assert!(!d.backend.actions().iter().any(|a| matches!(a,
                        A::ChooseTarget { target: crate::rl::TargetRefV1::Object { object }, .. }
                        if object.arena_id == friendly.0)));
                }
                act(
                    &mut own,
                    &mut native,
                    |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==target.0),
                );
                assert!(own.fixture_state().engine.pending_cast.is_none());
                assert_eq!(own.fixture_state().objects.get(spell).zone, Zone::Stack);
                let def = &crate::card_def::CARD_DEFS
                    [usize::from(crate::card_def::card_id_by_name(name).unwrap())];
                let tapped = own.fixture_state().players[p.index()]
                    .battlefield
                    .iter()
                    .filter(|&&id| own.fixture_state().objects.get(id).tapped)
                    .count();
                assert_eq!(tapped, usize::from(def.cost.generic) + def.cost.pips.len());
                settle(&mut own, &mut native);
                assert_eq!(own.fixture_state().objects.get(spell).zone, Zone::Graveyard);
                match name {
                    "Stab" | "Hero's Downfall" | "Broken Wings" | "Make Your Move" => assert_eq!(
                        own.fixture_state().objects.get(target).zone,
                        Zone::Graveyard
                    ),
                    "Fleeting Distraction" => {
                        assert_eq!(
                            engine::checked_effective_pt(own.fixture_state(), target).unwrap(),
                            (0, 1)
                        );
                        assert_eq!(
                            own.fixture_state().players[p.index()].library.len(),
                            library_before - 1
                        );
                    }
                    "Sure Strike" => {
                        assert_eq!(
                            engine::checked_effective_pt(own.fixture_state(), target).unwrap(),
                            (4, 1)
                        );
                        assert!(engine::has_effective_keyword(
                            own.fixture_state(),
                            target,
                            crate::card_def::Keywords::FIRST_STRIKE
                        ));
                    }
                    "Snakeskin Veil" => {
                        assert_eq!(
                            engine::checked_effective_pt(own.fixture_state(), target).unwrap(),
                            (2, 2)
                        );
                        assert_eq!(
                            own.fixture_state().objects.get(target).counters.plus1_plus1,
                            1
                        );
                        assert!(engine::has_effective_keyword(
                            own.fixture_state(),
                            target,
                            crate::card_def::Keywords::HEXPROOF
                        ));
                    }
                    "Fleeting Flight" => {
                        let state = own.fixture_state();
                        assert_eq!(engine::checked_effective_pt(state, target).unwrap(), (2, 2));
                        assert_eq!(state.objects.get(target).counters.plus1_plus1, 1);
                        assert!(engine::has_effective_keyword(
                            state,
                            target,
                            crate::card_def::Keywords::FLYING
                        ));
                        assert!(state.engine.active_replacements.iter().any(|replacement| matches!(
                            replacement.kind,
                            crate::event::ReplacementEffectKind::PreventCombatDamageToObjectUntilEndOfTurn { object, turn, active_player }
                            if replacement.source == spell && object.object == target
                                && object.zone_change_count == state.objects.get(target).zone_change_count
                                && turn == state.turn_key_v1()
                                && active_player == state.active_player
                        )));
                    }
                    "Bake into a Pie" => {
                        let state = own.fixture_state();
                        assert_eq!(state.objects.get(target).zone, Zone::Graveyard);
                        let foods: Vec<_> = state.players[p.index()]
                            .battlefield
                            .iter()
                            .copied()
                            .filter(|&id| {
                                crate::card_def::CARD_DEFS
                                    [usize::from(state.objects.get(id).card_def)]
                                .name
                                    == "Food Token"
                            })
                            .collect();
                        assert_eq!(foods.len(), 1);
                        assert!(state.objects.get(foods[0]).v4.is_token);
                        assert_eq!(state.objects.get(foods[0]).controller, p);
                    }
                    "Joust Through" => {
                        assert_eq!(
                            own.fixture_state().objects.get(target).zone,
                            Zone::Graveyard
                        );
                        assert_eq!(own.fixture_state().players[p.index()].life, 21);
                        assert_eq!(
                            own.fixture_state().objects.get(friendly).zone,
                            Zone::Battlefield
                        );
                    }
                    "Zombify" => {
                        assert_eq!(
                            own.fixture_state().objects.get(target).zone,
                            Zone::Battlefield
                        );
                        assert_eq!(own.fixture_state().objects.get(target).controller, p);
                        assert_eq!(own.fixture_state().objects.get(target).zone_change_count, 1);
                    }
                    _ => unreachable!(),
                }
                if let Some(expected) = &completed {
                    assert_eq!(own.fixture_state(), expected);
                } else {
                    completed = Some(own.fixture_state().clone());
                }
            }
        }
    }
}

#[test]
fn paid_fleeting_distraction_skips_draw_after_real_stab_removes_its_only_target_both_seats() {
    check_target_departure_skips_secondary_resolution("Fleeting Distraction");
}

#[test]
fn paid_pie_skips_food_after_real_stab_removes_its_only_target_both_seats() {
    check_target_departure_skips_secondary_resolution("Bake into a Pie");
}

#[test]
fn paid_joust_skips_life_after_real_stab_removes_its_only_attacking_target_both_seats() {
    check_target_departure_skips_secondary_resolution("Joust Through");
}

fn check_target_departure_skips_secondary_resolution(name: &str) {
    use crate::{
        policy_observation_v6::tests::put,
        state::{GameState, Step, Zone},
    };
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_289,
            if name == "Joust Through" {
                p.opponent()
            } else {
                p
            },
        );
        s.step = Step::Main1;
        let land = match name {
            "Fleeting Distraction" => "Island",
            "Joust Through" => "Plains",
            _ => "Swamp",
        };
        let cost = if name == "Bake into a Pie" { 4 } else { 1 };
        for _ in 0..cost {
            put(&mut s, p, land, Zone::Battlefield);
        }
        put(&mut s, p.opponent(), "Swamp", Zone::Battlefield);
        let target = put(&mut s, p.opponent(), "Llanowar Elves", Zone::Battlefield);
        let spell = put(&mut s, p, name, Zone::Hand);
        let response = put(&mut s, p.opponent(), "Stab", Zone::Hand);
        if name == "Joust Through" {
            s.step = Step::DeclareAttackers;
            assert!(matches!(crate::engine::advance_until_decision(&mut s),
                crate::engine::Decision::DeclareAttackers { ref eligible, .. } if eligible.contains(&target)));
            crate::engine::step(
                &mut s,
                crate::engine::Action::DeclareAttackers(vec![target]),
            )
            .unwrap();
            assert!(matches!(crate::engine::advance_until_decision(&mut s),
                crate::engine::Decision::CastSpellOrPass { player, .. } if player == p.opponent()));
            crate::engine::step(&mut s, crate::engine::Action::Pass).unwrap();
            assert!(matches!(crate::engine::advance_until_decision(&mut s),
                crate::engine::Decision::CastSpellOrPass { player, .. } if player == p));
        }
        let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
        let mut own =
            KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
                .unwrap();
        let choose_target = |a: &A| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==target.0);
        act(
            &mut own,
            &mut native,
            |a| matches!(a,A::CastSpell{source,..}if source.arena_id==spell.0),
        );
        act(&mut own, &mut native, choose_target);
        act(&mut own, &mut native, |a| matches!(a, A::Pass { .. }));
        act(
            &mut own,
            &mut native,
            |a| matches!(a,A::CastSpell{source,..}if source.arena_id==response.0),
        );
        act(&mut own, &mut native, choose_target);
        assert_eq!(own.fixture_state().stack.len(), 2);
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        settle(&mut own, &mut native);
        assert_eq!(
            own.fixture_state().objects.get(target).zone,
            Zone::Graveyard
        );
        assert_eq!(own.fixture_state().players[p.index()].library.len(), 40);
        assert_eq!(own.fixture_state().players[p.index()].life, 20);
        assert!(!own.fixture_state().players[p.index()]
            .battlefield
            .iter()
            .any(|&id| crate::card_def::CARD_DEFS
                [usize::from(own.fixture_state().objects.get(id).card_def)]
            .name
                == "Food Token"));
        assert_eq!(own.fixture_state().objects.get(spell).zone, Zone::Graveyard);
        assert_eq!(
            own.fixture_state().objects.get(response).zone,
            Zone::Graveyard
        );
        let expected = own.fixture_state().clone();
        own.restore(&saved).unwrap();
        native.kaito_restore_v75(&raw).unwrap();
        settle(&mut own, &mut native);
        assert_eq!(own.fixture_state(), &expected);
    }
}

fn complete(own: &mut KaitoSession, native: &mut FastActorSessionV1) {
    for _ in 0..512 {
        let FastActorResponseV1::Decision(h) = own.current_response() else {
            panic!()
        };
        let d = own.encode(h).unwrap();
        if d.backend
            .observation()
            .kaito
            .activations
            .last()
            .is_some_and(|a| a.stage == StageV75::Resolved)
        {
            return;
        }
        let finish = d
            .backend
            .actions()
            .iter()
            .any(|a| matches!(a, A::FinishEffectSelection { .. }));
        act(own, native, |a| {
            if finish {
                matches!(a, A::FinishEffectSelection { .. })
            } else {
                matches!(
                    a,
                    A::Pass { .. }
                        | A::OrderTriggers { .. }
                        | A::ChooseTriggerOrderNext { .. }
                        | A::ChooseEffectTarget { .. }
                        | A::ChooseTarget { .. }
                        | A::Discard { .. }
                        | A::ChooseAttackerInclusion { include: false, .. }
                        | A::ChooseBlockerInclusion { include: false, .. }
                )
            }
        });
    }
    panic!("numeric native root incomplete")
}

#[test]
fn paid_numeric_kaito_all_three_roots_actual_discard_ninja_emblem_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for index in [0, 1, 2] {
            let (mut s, k, _) = owning_ready(p, false, "Island");
            if index == 2 {
                crate::effect::kaito_cunning_v75::tests::owner_v75_tests::prepare_ultimate(
                    &mut s, k,
                );
            }
            let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
            let mut own =
                KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
                    .unwrap();
            act(
                &mut own,
                &mut native,
                |a| matches!(a,A::ActivateAbility{source,ability_index,..}if source.arena_id==k.0&&*ability_index==index),
            );
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            let old = own.encode(h).unwrap();
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            complete(&mut own, &mut native);
            let expected = own.fixture_state().clone();
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            let d = own.encode(h).unwrap();
            let f = &d.backend.observation().kaito;
            assert_eq!(f.activations[0].ability_index, index);
            assert_eq!(
                f.activations[0].loyalty_after,
                match index {
                    0 => 4,
                    1 => 1,
                    2 => 0,
                    _ => unreachable!(),
                }
            );
            if index == 0 {
                assert_eq!(
                    f.activations[0].draw,
                    crate::kaito_public_v75::DrawV75::Drawn
                );
                assert!(f.activations[0].discard.as_ref().unwrap().answered);
            }
            if index == 1 {
                assert_eq!(f.ninja_batches[0].created.len(), 1);
                assert!(printing(495).unwrap().is_some());
            }
            if index == 2 {
                assert_eq!(f.emblems.len(), 1);
                assert_eq!(f.emblems[0].controller, p.into());
            }
            own.restore(&saved).unwrap();
            native.kaito_restore_v75(&raw).unwrap();
            let before = own.fixture_state().clone();
            assert!(own.apply(&old, 0).is_err());
            assert_eq!(own.fixture_state(), &before);
            complete(&mut own, &mut native);
            assert_eq!(own.fixture_state(), &expected);
        }
    }
}
#[test]
fn paid_numeric_kaito_departed_or_concealed_original_controller_keeps_private_discard_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for concealed in [false, true] {
            let (s, _, _) =
                crate::effect::kaito_cunning_v75::tests::owner_v75_tests::published_source_boundary(
                    p, concealed,
                );
            let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
            let mut own =
                KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
                    .unwrap();
            complete(&mut own, &mut native);
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            let d = own.encode(h).unwrap();
            assert_eq!(
                d.backend.observation().kaito.activations[0]
                    .source
                    .captured_controller,
                p.into()
            );
            assert_eq!(
                d.backend.observation().kaito.activations[0].stage,
                StageV75::Resolved
            );
        }
    }
}
fn settle(own: &mut KaitoSession, native: &mut FastActorSessionV1) {
    for _ in 0..512 {
        let s = own.fixture_state();
        if s.stack.is_empty()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_discard.is_none()
            && s.engine.pending_triggers.is_empty()
        {
            return;
        }
        act(own, native, |a| {
            matches!(
                a,
                A::Pass { .. }
                    | A::OrderTriggers { .. }
                    | A::ChooseTriggerOrderNext { .. }
                    | A::ChooseEffectTarget { .. }
                    | A::ChooseTarget { .. }
                    | A::Discard { .. }
                    | A::ChooseAttackerInclusion { include: false, .. }
                    | A::ChooseBlockerInclusion { include: false, .. }
            )
        });
    }
    panic!("Kaito passive work incomplete")
}
#[test]
fn paid_numeric_kaito_original_combat_and_persistent_emblem_cast_roles_match_native_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for emblem in [false, true] {
            let s = if emblem {
                crate::effect::kaito_cunning_v75::tests::owner_v75_tests::emblem_cast_boundary(p)
            } else {
                crate::effect::kaito_cunning_v75::tests::owner_v75_tests::combat_boundary(p, false)
            };
            let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
            let mut own =
                KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
                    .unwrap();
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            let d = own.encode(h).unwrap();
            let f = &d.backend.observation().kaito;
            if emblem {
                assert_eq!(f.casts.len(), 2);
                assert_eq!(f.emblems.len(), 1);
            } else {
                assert_eq!(f.combats.len(), 1);
                assert_eq!(f.combats[0].final_damage, 2);
            }
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            settle(&mut own, &mut native);
            let expected = own.fixture_state().clone();
            own.restore(&saved).unwrap();
            native.kaito_restore_v75(&raw).unwrap();
            settle(&mut own, &mut native);
            assert_eq!(own.fixture_state(), &expected);
        }
    }
}
#[test]
fn paid_numeric_foreign_forged_packet_and_action_refusals_leave_native_unchanged_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, _, _) = owning_ready(p, false, "Island");
        let mut a =
            KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s.clone()))
                .unwrap();
        let mut b = KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
            .unwrap();
        let FastActorResponseV1::Decision(h) = a.current_response() else {
            panic!()
        };
        let d = a.encode(h).unwrap();
        let before = a.fixture_state().clone();
        let counts = a.fixture_counts();
        assert!(b.apply(&d, 0).is_err());
        assert!(a.restore(&b.snapshot()).is_err());
        let mut erased = d.clone();
        erased.packet.actions.clear();
        assert!(a.apply(&erased, 0).is_err());
        let mut changed = d.clone();
        changed.packet.state = crate::current_mixed_policy_v1::NumericTree::encode(
            &serde_json::json!({"replacement":"forged"}),
        )
        .unwrap();
        assert!(a.apply(&changed, 0).is_err());
        let mut forged = d.backend.candidates_v75().clone();
        forged.observation.kaito.activations.clear();
        forged.observation.kaito.authorized_printings.clear();
        assert!(packet_from_current(&forged, d.backend.actions()).is_err());
        assert!(a.apply(&d, u32::MAX).is_err());
        assert_eq!(a.fixture_state(), &before);
        assert_eq!(a.fixture_counts(), counts);
    }
}
#[test]
fn independently_paid_numeric_hidden_world_equality_and_full_native_integer_distinction_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut packets = vec![];
        for hidden in ["Island", "Imprisoned in the Moon"] {
            let (s, _, _) = owning_ready(p, false, hidden);
            let own =
                KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
                    .unwrap();
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            packets.push(own.encode(h).unwrap().packet().clone());
        }
        assert_eq!(packets[0], packets[1]);
        let (s, id, _) = owning_ready(p, false, "Island");
        let mut wide = vec![];
        for count in [i32::MAX - 1, i32::MAX] {
            let mut world = s.clone();
            let c = &mut world.objects.get_mut(id).counters;
            // Explicit native storage boundary on a genuinely six-mana paid printing.
            c.stash = count;
            c.fellowship = i32::MAX;
            c.charge = i32::MAX;
            let own =
                KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(world))
                    .unwrap();
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            let d = own.encode(h).unwrap();
            let current = d.backend.observation().kaito.battlefield[p.index()]
                .iter()
                .find(|c| c.card.object == id.0)
                .unwrap();
            assert_eq!(current.counters.stash, count);
            assert!(NativeModel::deterministic_fixture()
                .unwrap()
                .forward(d.packet())
                .unwrap()
                .iter()
                .all(|v| v.is_finite()));
            wide.push(d.packet().clone());
        }
        assert_ne!(wide[0], wide[1]);
    }
}
