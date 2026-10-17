//! Genuine paid Chandra source fixtures. Real Partial493 follows the actual
//! Tinybones490 and Liliana491/Zombie492 definitions. Rust remains unrun.
//! Zone/counter/shield/face-down setup explicitly marked below is native
//! primitive coverage, not an invented paid printing or copy-spell issuer.
use super::*;
#[path = "chandra_public_v74_tests.rs"]
mod public_v74_tests;
use crate::{
    engine::time_stop_v1::tests::{cast, finish, next, pass, priority, restore, tapped},
    engine::{self, Action, Decision},
    policy_observation_v6::tests::put,
};
const NAME: &str = "Chandra, Flameshaper";
fn ready(p: PlayerId) -> GameState {
    super::super::vivien_reid_v1::tests::ready(
        p,
        &[
            "Forest",
            "Think Twice",
            "Mountain",
            "Island",
            "Forest",
            "Mountain",
            "Island",
            "Forest",
        ],
        &[],
        "Island",
    )
}
fn walker(s: &mut GameState, p: PlayerId) -> ObjectId {
    let c = cast(s, p, NAME, &[], 7);
    finish(s);
    assert_eq!(s.objects.get(c).zone, Zone::Battlefield);
    assert!(definition(s.objects.get(c).card_def));
    c
}
fn activate(s: &mut GameState, p: PlayerId, c: ObjectId, index: u8, units: &[Target]) -> StackItem {
    priority(s, p);
    assert!(
        matches!(next(s),Decision::CastSpellOrPass{activatable_abilities,..}if activatable_abilities.contains(&(c,index)))
    );
    let before = crate::planeswalker_v1::loyalty(s, c).unwrap();
    let expected = crate::planeswalker_v1::quote_loyalty_change_v48(
        s,
        c,
        [2, 1, -4][index as usize],
        crate::doubling_season_v48::CounterOriginV48::Cost,
    )
    .unwrap();
    engine::step(s, Action::ActivateAbility(c, index)).unwrap();
    let mut picked = 0;
    for _ in 0..32 {
        match next(s) {
            Decision::ChooseTargets {
                player,
                legal_targets,
                ..
            } => {
                assert_eq!(player, p);
                assert_eq!(index, 1);
                assert_eq!(picked, 0);
                assert!(legal_targets.contains(&units[0]));
                engine::step(s, Action::ChooseTarget(units[0])).unwrap();
                picked += 1;
            }
            Decision::ChooseEffectTargets {
                player,
                min_targets,
                max_targets,
                legal_targets,
                can_finish,
                ..
            } => {
                assert_eq!(player, p);
                assert_eq!(index, 2);
                assert_eq!((min_targets, max_targets), (0, 8));
                if units.is_empty() {
                    assert!(can_finish);
                    engine::step(s, Action::FinishEffectSelection).unwrap();
                } else {
                    assert!(legal_targets.contains(&units[picked]));
                    engine::step(s, Action::ChooseEffectTarget(units[picked])).unwrap();
                    picked += 1;
                }
            }
            Decision::CastSpellOrPass { player, .. } => {
                assert_eq!(player, p);
                assert_eq!(picked, units.len());
                assert!(s.engine.pending_activation.is_none());
                let row = s
                    .stack
                    .iter()
                    .find(|r| r.source == c && r.v4.activated_ability_index == Some(index))
                    .unwrap()
                    .clone();
                assert_eq!(row.kind, StackItemKind::ActivatedAbility);
                assert_eq!(row.controller, p);
                assert_eq!(row.inline_effect, Some(program(index as usize)));
                let (_, input) = publication(s, &row).unwrap();
                assert_eq!((input.before, input.after), (before, expected));
                if s.objects.get(c).zone == Zone::Battlefield {
                    assert_eq!(crate::planeswalker_v1::loyalty(s, c), Some(expected));
                } else {
                    assert_eq!(expected, 0);
                    assert_eq!(s.objects.get(c).zone, Zone::Graveyard);
                }
                validate_state(s).unwrap();
                return row;
            }
            d => panic!("Chandra activation did not publish {d:?}"),
        }
        if s.engine.pending_activation.as_ref().is_some_and(|pending| {
            pending
                .chandra_damage_v74
                .as_ref()
                .is_some_and(|d| d.finished)
        }) {
            // The completed answer awaits native Advance, never a second menu.
            let before = s.clone();
            assert!(engine::step(s, Action::FinishEffectSelection).is_err());
            assert_eq!(*s, before);
            assert!(engine::step(s, Action::ChooseEffectTarget(Target::Player(p))).is_err());
            assert_eq!(*s, before);
        }
    }
    panic!("Chandra activation missing")
}
fn exile_prompt(s: &mut GameState, p: PlayerId) -> Vec<ObjectId> {
    for _ in 0..32 {
        let d = next(s);
        if let Decision::ChooseEffectTargets {
            player,
            legal_targets,
            min_targets,
            max_targets,
            can_finish,
            ..
        } = d
        {
            assert_eq!(player, p);
            assert_eq!((min_targets, max_targets), (1, 1));
            assert!(!can_finish);
            validate_pending_effect_choice(s).unwrap();
            return legal_targets
                .into_iter()
                .map(|t| {
                    if let Target::Object(o) = t {
                        o
                    } else {
                        panic!("noncard exile option")
                    }
                })
                .collect();
        }
        pass(s, d);
    }
    panic!("Chandra actual public exile choice absent")
}
fn answer_exile(s: &mut GameState, chosen: ObjectId) {
    engine::step(s, Action::ChooseEffectTarget(Target::Object(chosen))).unwrap();
    validate_pending_effect_choice(s).unwrap();
    let mut answered = restore(s);
    finish(s);
    finish(&mut answered);
    assert_eq!(*s, answered);
}
fn next_turn(s: &mut GameState) {
    let turn = s.turn_key_v1();
    for _ in 0..220 {
        let d = next(s);
        if s.turn_key_v1() != turn {
            return;
        }
        pass(s, d);
    }
    panic!("ordinary next turn absent")
}
fn refuses(s: &GameState) {
    let before = s.clone();
    assert!(validate_state(s).is_err());
    assert_eq!(*s, before);
    let mut bad = restore(s);
    assert!(engine::step(&mut bad, Action::Pass).is_err());
    assert_eq!(bad, before);
}
#[test]
fn genuine_paid_plus_two_original_top_three_choice_ordinary_exile_cast_and_answer_restore_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for leaves in [false, true] {
            let mut s = ready(p);
            let c = walker(&mut s, p);
            assert_eq!(crate::planeswalker_v1::loyalty(&s, c), Some(6));
            let library = s.players[p.index()].library.clone();
            let row = activate(&mut s, p, c, 0, &[]);
            if leaves {
                // Native source-departure primitive during real response window.
                event::propose_and_commit(
                    &mut s,
                    event::ProposedEvent::zone_change(c, Zone::Graveyard),
                );
            }
            let options = exile_prompt(&mut s, p);
            assert_eq!(options, library[..3]);
            assert_eq!(
                s.players[p.index()].mana_pool[crate::mana::ManaColor::R.pool_index()],
                3
            );
            assert_eq!(s.players[p.index()].library.as_slice(), &library[3..]);
            let mut replay = restore(&s);
            let chosen = library[1];
            for b in [&mut s, &mut replay] {
                answer_exile(b, chosen);
                assert_eq!(b.objects.get(chosen).zone, Zone::Exile);
                priority(b, p);
                let d = next(b);
                assert!(
                    matches!(d,Decision::CastSpellOrPass{ref castable_spells,..}if castable_spells.contains(&chosen)&&!castable_spells.contains(&library[0])&&!castable_spells.contains(&library[2]))
                );
                let taps = tapped(b, p);
                engine::step(b, Action::CastSpell(chosen)).unwrap();
                priority(b, p);
                assert_eq!(tapped(b, p) - taps, 1);
                assert_eq!(
                    b.players[p.index()].mana_pool[crate::mana::ManaColor::R.pool_index()],
                    2
                );
                assert!(b
                    .stack
                    .iter()
                    .any(|r| r.source == chosen && r.kind == StackItemKind::Spell));
                let mut paid_restore = restore(b);
                finish(b);
                finish(&mut paid_restore);
                assert_eq!(*b, paid_restore);
                assert_eq!(b.objects.get(chosen).zone, Zone::Graveyard);
                validate_state(b).unwrap();
                assert!(rows(b).any(|(_,j)|matches!(j,JournalV74::Completed{publication:at}if matches!(self::at(b,*at),Ok(JournalV74::Published{item,..})if item.v4.stack_item_id==row.v4.stack_item_id))));
            }
            assert_eq!(s, replay);
        }
    }
}
#[test]
fn genuine_paid_chosen_land_obeys_real_land_quota_and_permission_expires_at_cleanup_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for play in [false, true] {
            let mut s = ready(p);
            let c = walker(&mut s, p);
            let library = s.players[p.index()].library.clone();
            activate(&mut s, p, c, 0, &[]);
            exile_prompt(&mut s, p);
            answer_exile(&mut s, library[0]);
            priority(&mut s, p);
            assert!(
                matches!(next(&mut s),Decision::CastSpellOrPass{land_drops,..}if land_drops.contains(&library[0])&&!land_drops.contains(&library[2]))
            );
            if play {
                engine::step(&mut s, Action::PlayLand(library[0])).unwrap();
                assert_eq!(s.players[p.index()].lands_played_this_turn, 1);
                assert_eq!(s.objects.get(library[0]).zone, Zone::Battlefield);
            }
            let mut replay = restore(&s);
            next_turn(&mut s);
            next_turn(&mut replay);
            assert_eq!(s, replay);
            validate_state(&s).unwrap();
            assert!(s
                .engine
                .exile_play_permissions
                .iter()
                .all(|permission| permission.object != library[0]));
            assert!(rows(&s).any(|(_, j)| matches!(j, JournalV74::Expired { .. })));
            if !play {
                assert_eq!(s.objects.get(library[0]).zone, Zone::Exile);
            }
        }
    }
}
#[test]
fn genuine_paid_short_or_empty_library_plus_two_has_no_invented_choice_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for library in [&[][..], &["Forest"][..]] {
            let mut s = super::super::vivien_reid_v1::tests::ready(p, library, &[], "Island");
            let c = walker(&mut s, p);
            activate(&mut s, p, c, 0, &[]);
            let mut replay = restore(&s);
            finish(&mut s);
            finish(&mut replay);
            assert_eq!(s, replay);
            assert_eq!(
                s.players[p.index()].mana_pool[crate::mana::ManaColor::R.pool_index()],
                3
            );
            assert_eq!(s.engine.exile_play_permissions.len(), library.len());
            validate_state(&s).unwrap();
        }
    }
}
#[test]
fn genuine_paid_plus_one_season_copiable_haste_sacrifice_and_copy_of_copy_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for season in [false, true] {
            let mut s = ready(p);
            if season {
                cast(&mut s, p, "Doubling Season", &[], 5);
                finish(&mut s);
            }
            let c = walker(&mut s, p);
            let original = cast(&mut s, p, "Healer's Hawk", &[], 1);
            finish(&mut s);
            activate(&mut s, p, c, 1, &[Target::Object(original)]);
            let before = s.players[p.index()].battlefield.clone();
            let mut replay = restore(&s);
            finish(&mut s);
            finish(&mut replay);
            assert_eq!(s, replay);
            let copies = s.players[p.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|o| !before.contains(o) && s.objects.get(*o).v4.is_token)
                .collect::<Vec<_>>();
            assert_eq!(copies.len(), if season { 2 } else { 1 });
            for copy in &copies {
                assert_eq!(s.objects.get(*copy).owner, p);
                assert_eq!(engine::checked_effective_pt(&s, *copy).unwrap(), (1, 1));
                for k in [Keywords::HASTE, Keywords::FLYING, Keywords::LIFELINK] {
                    assert!(engine::has_effective_keyword(&s, *copy, k));
                }
                assert_eq!(event::copiable_token_abilities_v1(&s, *copy).unwrap(), 1);
            }
            let before = s.players[p.index()].battlefield.clone();
            cast(
                &mut s,
                p,
                "Self-Reflection",
                &[Target::Object(copies[0])],
                6,
            );
            let mut replay = restore(&s);
            finish(&mut s);
            finish(&mut replay);
            assert_eq!(s, replay);
            let later = s.players[p.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|o| !before.contains(o) && s.objects.get(*o).v4.is_token)
                .collect::<Vec<_>>();
            assert_eq!(later.len(), if season { 2 } else { 1 });
            for copy in &later {
                assert_eq!(event::copiable_token_abilities_v1(&s, *copy).unwrap(), 1);
                assert!(engine::has_effective_keyword(&s, *copy, Keywords::HASTE));
            }
            for _ in 0..200 {
                let d = next(&mut s);
                if copies
                    .iter()
                    .chain(&later)
                    .all(|o| s.objects.get(*o).zone != Zone::Battlefield)
                {
                    break;
                }
                pass(&mut s, d);
            }
            assert!(copies
                .iter()
                .chain(&later)
                .all(|o| s.objects.get(*o).zone != Zone::Battlefield));
            assert_eq!(s.objects.get(original).zone, Zone::Battlefield);
            validate_state(&s).unwrap();
        }
    }
}
#[test]
fn genuine_paid_copy_ignores_continuous_pump_and_native_face_down_values_are_neutral_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for face_down in [false, true] {
            let mut s = ready(p);
            let c = walker(&mut s, p);
            let original = cast(&mut s, p, "Healer's Hawk", &[], 1);
            finish(&mut s);
            if face_down {
                // Labelled native face-down primitive; not a fabricated disguised cast.
                s.objects.get_mut(original).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
                    disguised: false,
                    lookers: 0,
                    hidden_by: None,
                    hidden_by_source: None,
                });
            } else {
                cast(&mut s, p, "Giant Growth", &[Target::Object(original)], 1);
                finish(&mut s);
                assert_eq!(engine::checked_effective_pt(&s, original).unwrap(), (4, 4));
            }
            let before = s.players[p.index()].battlefield.clone();
            activate(&mut s, p, c, 1, &[Target::Object(original)]);
            let mut replay = restore(&s);
            finish(&mut s);
            finish(&mut replay);
            assert_eq!(s, replay);
            let copy = *s.players[p.index()]
                .battlefield
                .iter()
                .find(|o| !before.contains(o) && s.objects.get(**o).v4.is_token)
                .unwrap();
            assert_eq!(
                engine::checked_effective_pt(&s, copy).unwrap(),
                if face_down { (2, 2) } else { (1, 1) }
            );
            assert!(engine::has_effective_keyword(&s, copy, Keywords::HASTE));
            assert_eq!(
                engine::has_effective_keyword(&s, copy, Keywords::FLYING),
                !face_down
            );
            validate_state(&s).unwrap();
        }
    }
}
#[test]
fn genuine_paid_minus_four_zero_or_eight_distinct_original_targets_and_full_allocation_restore_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for count in [0, 8] {
            let mut s = ready(p);
            let c = walker(&mut s, p);
            let targets = (0..count)
                .map(|_| {
                    Target::Object(put(
                        &mut s,
                        p.opponent(),
                        "Axgard Cavalry",
                        Zone::Battlefield,
                    ))
                })
                .collect::<Vec<_>>();
            let row = activate(&mut s, p, c, 2, &targets);
            assert_eq!(row.targets, targets);
            assert_eq!(crate::planeswalker_v1::loyalty(&s, c), Some(2));
            let start = s.engine.event_history.len();
            let mut replay = restore(&s);
            finish(&mut s);
            finish(&mut replay);
            assert_eq!(s, replay);
            let damage = s.engine.event_history[start..]
                .iter()
                .filter_map(|e| {
                    if let E::Damage {
                        source,
                        target,
                        amount,
                        ..
                    } = e
                    {
                        Some((*source, *target, *amount))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(
                damage,
                targets.iter().map(|t| (c, *t, 1)).collect::<Vec<_>>()
            );
            for t in &targets {
                if let Target::Object(o) = t {
                    assert_eq!(s.objects.get(*o).damage, 1);
                }
            }
            validate_state(&s).unwrap();
        }
    }
}
#[test]
fn genuine_paid_original_division_source_sba_and_individual_target_loss_keep_surviving_amount_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let c = walker(&mut s, p);
        let a = put(&mut s, p.opponent(), "Axgard Cavalry", Zone::Battlefield);
        let b = put(&mut s, p.opponent(), "Shivan Dragon", Zone::Battlefield);
        // Native loyalty placement primitive, not an invented paid activation.
        crate::planeswalker_v1::change_loyalty(&mut s, c, -2);
        assert_eq!(crate::planeswalker_v1::loyalty(&s, c), Some(4));
        let units = [Target::Object(a); 3]
            .into_iter()
            .chain([Target::Object(b); 5])
            .collect::<Vec<_>>();
        let row = activate(&mut s, p, c, 2, &units);
        assert_eq!(row.targets, vec![Target::Object(a), Target::Object(b)]);
        assert_eq!(s.objects.get(c).zone, Zone::Graveyard);
        // Actual zone event in the issued response window makes only slot0 illegal.
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::zone_change(a, Zone::Graveyard),
        );
        let start = s.engine.event_history.len();
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        let packets = s.engine.event_history[start..]
            .iter()
            .filter_map(|e| {
                if let E::Damage {
                    source,
                    target,
                    amount,
                    ..
                } = e
                {
                    Some((*source, *target, *amount))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(packets, vec![(c, Target::Object(b), 5)]);
        validate_state(&s).unwrap();
    }
}
#[test]
fn genuine_paid_divided_damage_tyrant_shield_order_uses_real_affected_choice_and_restore_both_seats(
) {
    use super::super::crystal_prevention_v1::PreventionV1;
    for p in [PlayerId::P0, PlayerId::P1] {
        for double_first in [false, true] {
            let mut s = ready(p);
            cast(&mut s, p, "Twinflame Tyrant", &[], 5);
            finish(&mut s);
            let c = walker(&mut s, p);
            let recipient = put(&mut s, p.opponent(), "Shivan Dragon", Zone::Battlefield);
            // Existing typed next-damage shield primitive, not a claimed paid prevent spell.
            s.engine.active_replacements.push(event::ActiveReplacement {
                id: 90_493,
                source: recipient,
                kind: event::ReplacementEffectKind::PreventNextDamage {
                    target: Target::Object(recipient),
                    remaining: 3,
                },
            });
            activate(&mut s, p, c, 2, &[Target::Object(recipient); 8]);
            for _ in 0..40 {
                let d = next(&mut s);
                if matches!(d, Decision::ChooseEffectOption { .. }) {
                    break;
                }
                pass(&mut s, d);
            }
            let start = s.engine.event_history.len();
            let mut replay = restore(&s);
            for branch in [&mut s, &mut replay] {
                let Some(PendingEffectChoice::ChooseOption {
                    player,
                    purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                    ..
                }) = branch
                    .engine
                    .pending_effect
                    .as_ref()
                    .unwrap()
                    .choice
                    .as_ref()
                else {
                    panic!("real affected choice absent")
                };
                assert_eq!(*player, p.opponent());
                let option = choice
                    .options
                    .iter()
                    .position(|o| {
                        if double_first {
                            matches!(o, PreventionV1::Multiplier(_))
                        } else {
                            matches!(o, PreventionV1::Replacement(_))
                        }
                    })
                    .unwrap();
                engine::step(branch, Action::ChooseEffectOption(option as u16)).unwrap();
                while branch
                    .engine
                    .pending_effect
                    .as_ref()
                    .is_some_and(|pending| pending.choice.is_some())
                {
                    assert!(matches!(next(branch), Decision::ChooseEffectOption { .. }));
                    engine::step(branch, Action::ChooseEffectOption(0)).unwrap();
                }
                finish(branch);
                assert_eq!(
                    branch.engine.event_history[start..]
                        .iter()
                        .filter_map(|e| {
                            if let E::Damage {
                                source,
                                target,
                                amount,
                                ..
                            } = e
                            {
                                (*source == c && *target == Target::Object(recipient))
                                    .then_some(*amount)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>(),
                    vec![if double_first { 13 } else { 10 }]
                );
                validate_state(branch).unwrap();
            }
            assert_eq!(s, replay);
        }
    }
}
#[test]
fn genuine_paid_allocation_exile_and_public_api_forgeries_refuse_readonly_and_hidden_worlds_are_independent_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut worlds = [
            super::super::vivien_reid_v1::tests::ready(
                p,
                &["Forest", "Think Twice", "Mountain", "Island"],
                &[NAME],
                "Forest",
            ),
            super::super::vivien_reid_v1::tests::ready(
                p,
                &["Forest", "Think Twice", "Mountain", "Island"],
                &[NAME],
                "Island",
            ),
        ];
        let mut menus = vec![];
        for s in &mut worlds {
            assert!(require_legacy(s, None).is_ok());
            assert!(require_legacy(s, Some(p)).is_err());
            assert!(require_legacy(s, Some(p.opponent())).is_ok());
            let c = walker(s, p);
            let row = activate(s, p, c, 0, &[]);
            let ctx = original_context(&row, s);
            let draw = EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            };
            let mut bad = restore(s);
            let before = bad.clone();
            assert!(begin_resumable_resolution(&draw, &ctx, row.clone(), &mut bad).is_err());
            assert_eq!(bad, before);
            execute(&draw, &ctx, &mut bad);
            assert!(bad.engine.halted.take().is_some());
            assert_eq!(bad, before);
            menus.push(exile_prompt(s, p));
            let pending = s.engine.pending_effect.clone().unwrap();
            let mut bad = restore(s);
            bad.engine.pending_effect.as_mut().unwrap().choice = None;
            bad.engine.pending_effect.as_mut().unwrap().frames.clear();
            let before = bad.clone();
            assert!(validate_pending_effect_choice(&bad).is_err());
            assert!(resume_resumable_resolution(&mut bad).is_err());
            assert_eq!(bad, before);
            let mut bad = restore(s);
            bad.stack
                .retain(|r| r.v4.stack_item_id != row.v4.stack_item_id);
            bad.engine.pending_effect = None;
            refuses(&bad);
            let mut bad = restore(s);
            let at = rows(&bad)
                .find_map(|(i, j)| matches!(j, JournalV74::Published { .. }).then_some(i))
                .unwrap();
            if let E::ChandraFlameshaperV74 { journal } = &mut bad.engine.event_history[at] {
                if let JournalV74::Published { input, .. } = journal.as_mut() {
                    input.after += 1;
                }
            }
            refuses(&bad);
            let mut bad = restore(s);
            let capture = rows(&bad)
                .find_map(|(i, j)| matches!(j, JournalV74::Exiled { .. }).then_some(i))
                .unwrap();
            if let E::ChandraFlameshaperV74 { journal } = &mut bad.engine.event_history[capture] {
                if let JournalV74::Exiled { public, .. } = journal.as_mut() {
                    public[0].card_db_id = crate::card_def::card_id_by_name("Island").unwrap();
                }
            }
            refuses(&bad);
            assert_eq!(s.engine.pending_effect.as_ref(), Some(&pending));
            answer_exile(s, menus.last().unwrap()[0]);
            validate_state(s).unwrap();
        }
        assert_eq!(menus[0], menus[1]);
        for viewer in [p, p.opponent()] {
            assert_eq!(
                crate::rl::observe_v1(&worlds[0], viewer, 0).unwrap_err(),
                crate::rl::observe_v1(&worlds[1], viewer, 0).unwrap_err()
            );
        }
        let mut s = ready(p);
        let c = walker(&mut s, p);
        priority(&mut s, p);
        engine::step(&mut s, Action::ActivateAbility(c, 2)).unwrap();
        assert!(matches!(next(&mut s), Decision::ChooseEffectTargets { .. }));
        let before = s.clone();
        assert!(engine::step(
            &mut s,
            Action::ChooseEffectTarget(Target::Player(p.opponent()))
        )
        .is_err());
        assert_eq!(s, before);
        let recipient = put(&mut s, p.opponent(), "Axgard Cavalry", Zone::Battlefield);
        engine::step(
            &mut s,
            Action::ChooseEffectTarget(Target::Object(recipient)),
        )
        .unwrap();
        let mut bad = restore(&s);
        let pending = bad.engine.pending_activation.as_mut().unwrap();
        pending.chandra_damage_v74.as_mut().unwrap().allocations[0] = 9;
        let before = bad.clone();
        assert!(engine::validate_pending_activation(
            &bad,
            bad.engine.pending_activation.as_ref().unwrap()
        )
        .is_err());
        assert!(engine::step(&mut bad, Action::Pass).is_err());
        assert_eq!(bad, before);
    }
}

#[test]
fn genuine_paid_division_union_includes_original_planeswalker_and_creature_contracts_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let c = walker(&mut s, p);
        let ajani = cast(&mut s, p, "Ajani, Caller of the Pride", &[], 3);
        finish(&mut s);
        let old = crate::planeswalker_v1::loyalty(&s, ajani).unwrap();
        assert!(old >= 4);
        let creature = put(&mut s, p.opponent(), "Axgard Cavalry", Zone::Battlefield);
        let units = [Target::Object(ajani); 3]
            .into_iter()
            .chain([Target::Object(creature); 5])
            .collect::<Vec<_>>();
        let row = activate(&mut s, p, c, 2, &units);
        assert_eq!(
            row.targets,
            vec![Target::Object(ajani), Target::Object(creature)]
        );
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(crate::planeswalker_v1::loyalty(&s, ajani), Some(old - 3));
        assert_eq!(s.objects.get(creature).zone, Zone::Graveyard);
        validate_state(&s).unwrap();
    }
}

#[test]
fn genuine_paid_liliana_zombie_copy_uses_distinct_native_permit_and_raw_partial_creation_refuses_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let liliana = cast(&mut s, p, "Liliana, Dreadhorde General", &[], 6);
        finish(&mut s);
        priority(&mut s, p);
        assert!(
            matches!(next(&mut s),Decision::CastSpellOrPass{activatable_abilities,..}if activatable_abilities.contains(&(liliana,0)))
        );
        engine::step(&mut s, Action::ActivateAbility(liliana, 0)).unwrap();
        priority(&mut s, p);
        assert_eq!(crate::planeswalker_v1::loyalty(&s, liliana), Some(7));
        assert!(s
            .stack
            .iter()
            .any(|r| r.source == liliana && r.inline_effect == Some(EffectOp::LilianaZombieV1)));
        finish(&mut s);
        let blueprint = card_def::card_id_by_name("Zombie Token").unwrap();
        assert_eq!(blueprint, 492);
        let zombie = s.players[p.index()]
            .battlefield
            .iter()
            .copied()
            .find(|o| s.objects.get(*o).v4.is_token && s.objects.get(*o).card_def == blueprint)
            .unwrap();
        assert_eq!(event::copiable_token_abilities_v1(&s, zombie).unwrap(), 0);
        let mut bad = restore(&s);
        let before = bad.clone();
        event::propose_and_commit_batch(
            &mut bad,
            vec![event::ProposedEvent::create_token(blueprint, p)],
        );
        assert!(bad.engine.halted.take().is_some());
        assert_eq!(bad, before);
        let c = walker(&mut s, p);
        let before = s.players[p.index()].battlefield.clone();
        activate(&mut s, p, c, 1, &[Target::Object(zombie)]);
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        let copied = s.players[p.index()]
            .battlefield
            .iter()
            .copied()
            .find(|o| {
                !before.contains(o)
                    && s.objects.get(*o).v4.is_token
                    && s.objects.get(*o).card_def == blueprint
            })
            .unwrap();
        assert_eq!(s.objects.get(copied).owner, p);
        assert_eq!(engine::checked_effective_pt(&s, copied).unwrap(), (2, 2));
        assert!(engine::has_effective_keyword(&s, copied, Keywords::HASTE));
        assert_eq!(event::copiable_token_abilities_v1(&s, copied).unwrap(), 1);
        validate_state(&s).unwrap();
    }
}

#[path = "chandra_owner_v74_tests.rs"]
pub(crate) mod owner_v74_tests;
