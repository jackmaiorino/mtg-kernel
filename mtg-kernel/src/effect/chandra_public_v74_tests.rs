//! Genuine paid native roots, read-only DTO checks. Rust remains unrun.
use super::*;
use crate::{chandra_public_v74 as facts, rl::PlayerSeatV1};
fn capture(s: &GameState, v: PlayerId) -> facts::FactsV74 {
    let before = s.clone();
    let f = facts::capture(s, v).unwrap();
    assert_eq!(*s, before);
    facts::validate_current(s, v, &f).unwrap();
    let decoded: facts::FactsV74 =
        serde_json::from_slice(&serde_json::to_vec(&f).unwrap()).unwrap();
    assert_eq!(decoded, f);
    assert!(!f.complete_ordinary_observation && !f.owning_consumer_admitted);
    f
}
#[test]
fn paid_damage_allocation_zero_two_eight_original_contracts_before_payment_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for count in [0, 2, 8] {
            let mut s = ready(p);
            let c = walker(&mut s, p);
            let targets = (0..count)
                .map(|_| put(&mut s, p.opponent(), "Axgard Cavalry", Zone::Battlefield))
                .collect::<Vec<_>>();
            priority(&mut s, p);
            engine::step(&mut s, Action::ActivateAbility(c, 2)).unwrap();
            let _ = next(&mut s);
            let f = capture(&s, p.opponent());
            let a = f.pending_allocation.as_ref().unwrap();
            assert!(a.targets.is_empty() && a.can_finish_zero);
            assert_eq!((a.loyalty_before, a.remaining_units), (6, 8));
            assert!(f.activations.is_empty());
            let units = match count {
                0 => vec![],
                2 => [vec![targets[0]; 3], vec![targets[1]; 5]].concat(),
                _ => targets.clone(),
            };
            if units.is_empty() {
                engine::step(&mut s, Action::FinishEffectSelection).unwrap();
            } else {
                for (i, id) in units.iter().enumerate() {
                    assert!(matches!(next(&mut s), Decision::ChooseEffectTargets { .. }));
                    engine::step(&mut s, Action::ChooseEffectTarget(Target::Object(*id))).unwrap();
                    let f = capture(&s, p);
                    let a = f.pending_allocation.as_ref().unwrap();
                    assert_eq!(a.remaining_units, 7 - i as u8);
                    assert_eq!(a.loyalty_before, 6);
                    assert!(!a.can_finish_zero);
                    assert_eq!(a.finished, i == 7);
                }
            }
            let f = capture(&s, p);
            assert!(f.pending_allocation.as_ref().unwrap().finished);
            let _ = next(&mut s);
            let f = capture(&s, p);
            assert!(f.pending_allocation.is_none());
            let a = &f.activations[0];
            assert_eq!((a.loyalty_before, a.loyalty_after), (6, 2));
            assert_eq!(a.targets.len(), count);
            assert_eq!(
                a.targets
                    .iter()
                    .map(|t| t.allocated_damage.unwrap())
                    .collect::<Vec<_>>(),
                match count {
                    0 => vec![],
                    2 => vec![3, 5],
                    _ => vec![1; 8],
                }
            );
            let mut replay = restore(&s);
            finish(&mut s);
            finish(&mut replay);
            assert_eq!(s, replay);
            assert_eq!(
                capture(&s, p).activations[0].stage,
                facts::ActivationStageV74::NativeCompleted
            );
        }
    }
}
#[test]
fn paid_public_exile_choice_sourcegone_ordinary_spell_cost_land_quota_and_expiry_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for land in [false, true] {
            let mut s = ready(p);
            let c = walker(&mut s, p);
            let original = s.objects.get(c).zone_change_count;
            let library = s.players[p.index()].library.clone();
            assert!(capture(&s, p.opponent()).exiles.is_empty());
            activate(&mut s, p, c, 0, &[]);
            event::propose_and_commit(
                &mut s,
                event::ProposedEvent::zone_change(c, Zone::Graveyard),
            );
            let options = exile_prompt(&mut s, p);
            assert_eq!(options, library[..3]);
            let f = capture(&s, p.opponent());
            let e = &f.exiles[0];
            assert!(e.choice_current && e.chosen.is_none());
            assert_eq!(e.cards.len(), 3);
            assert_eq!(
                e.cards
                    .iter()
                    .map(|c| c.original.object)
                    .collect::<Vec<_>>(),
                options.iter().map(|id| id.0).collect::<Vec<_>>()
            );
            assert!(e
                .cards
                .iter()
                .all(|c| c.original.zone == Zone::Exile && c.current_original.is_some()));
            assert_eq!(f.activations[0].source.original_source.generation, original);
            assert!(f.activations[0].source.current_original_source.is_none());
            let chosen = library[usize::from(!land)];
            let mut replay = restore(&s);
            for branch in [&mut s, &mut replay] {
                answer_exile(branch, chosen);
                priority(branch, p);
                let f = capture(branch, p.opponent());
                let e = &f.exiles[0];
                assert_eq!(e.chosen, Some(u16::from(!land)));
                assert_eq!(e.grant_for_land, Some(land));
                assert!(e.permission_present && !e.expired);
                if land {
                    engine::step(branch, Action::PlayLand(chosen)).unwrap();
                    assert_eq!(branch.players[p.index()].lands_played_this_turn, 1);
                } else {
                    let before = tapped(branch, p);
                    engine::step(branch, Action::CastSpell(chosen)).unwrap();
                    priority(branch, p);
                    assert_eq!(tapped(branch, p) - before, 1);
                    assert_eq!(
                        branch.players[p.index()].mana_pool[crate::mana::ManaColor::R.pool_index()],
                        2
                    );
                    finish(branch);
                }
                let f = capture(branch, p.opponent());
                assert!(f.exiles[0].cards[usize::from(!land)]
                    .current_original
                    .is_none());
                next_turn(branch);
                let f = capture(branch, p);
                assert!(f.exiles[0].expired && !f.exiles[0].permission_present);
            }
            assert_eq!(s, replay);
            let hidden = put(&mut s, p, NAME, Zone::Hand);
            assert!(capture(&s, p)
                .authorized_printings
                .iter()
                .any(|r| r.object == hidden.0));
            assert!(!capture(&s, p.opponent())
                .authorized_printings
                .iter()
                .any(|r| r.object == hidden.0));
        }
    }
}
#[test]
fn paid_copy_season_copy_of_copy_real_copiable_instances_and_end_step_receipts_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for season in [false, true] {
            let mut s = ready(p);
            if season {
                cast(&mut s, p, "Doubling Season", &[], 5);
                finish(&mut s);
            }
            let c = walker(&mut s, p);
            let creature = cast(&mut s, p, "Healer's Hawk", &[], 1);
            finish(&mut s);
            s.objects.get_mut(c).counters.stash = 70_000;
            s.objects.get_mut(c).counters.fellowship = 80_000;
            activate(&mut s, p, c, 1, &[Target::Object(creature)]);
            // Labelled concealment boundary after the genuine paid activation.
            let mut concealed = restore(&s);
            concealed.objects.get_mut(c).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
                disguised: false,
                lookers: 0,
                hidden_by: None,
                hidden_by_source: None,
            });
            let f = capture(&concealed, p.opponent());
            assert_eq!(
                f.activations[0].source.original_source.identity,
                crate::optional_trigger_public_v1::IdentityV1::Known { card_id: 493 }
            );
            assert!(matches!(
                f.activations[0]
                    .source
                    .current_original_source
                    .as_ref()
                    .unwrap()
                    .identity,
                crate::optional_trigger_public_v1::IdentityV1::FaceDown { .. }
            ));
            let mut replay = restore(&s);
            finish(&mut s);
            finish(&mut replay);
            assert_eq!(s, replay);
            let f = capture(&s, p.opponent());
            let n = if season { 2 } else { 1 };
            assert_eq!(f.copied_tokens.len(), n);
            assert!(f
                .copied_tokens
                .iter()
                .all(|t| t.sacrifice_instances == 1 && t.current_haste == Some(true)));
            let current = f.battlefield[p.index()]
                .iter()
                .find(|r| r.card.object == c.0)
                .unwrap();
            assert_eq!(
                (current.counters.stash, current.counters.fellowship),
                (70_000, 80_000)
            );
            let first = ObjectId(f.copied_tokens[0].source.original_source.object);
            cast(&mut s, p, "Self-Reflection", &[Target::Object(first)], 6);
            finish(&mut s);
            let f = capture(&s, p);
            assert_eq!(f.copied_tokens.len(), 2 * n);
            assert!(f.copied_tokens.iter().all(|t| t.sacrifice_instances == 1));
            for _ in 0..200 {
                let d = next(&mut s);
                if f.copied_tokens.iter().all(|t| {
                    s.objects
                        .get(ObjectId(t.source.original_source.object))
                        .zone
                        != Zone::Battlefield
                }) {
                    break;
                }
                pass(&mut s, d);
            }
            let f = capture(&s, p);
            assert!(f.copied_tokens.iter().all(|t| t.current_haste.is_none()));
            assert_eq!(
                f.end_steps
                    .iter()
                    .map(|e| e.finished_count as usize)
                    .sum::<usize>(),
                2 * n
            );
            assert!(f
                .end_steps
                .iter()
                .all(|e| e.collected && e.finished_count == e.published_count));
            let at = s
                .engine
                .event_history
                .iter()
                .position(|e| matches!(e, E::CopiedTokenSacrificePublishedV1 { .. }))
                .unwrap();
            let mut forged = restore(&s);
            let E::CopiedTokenSacrificePublishedV1 { item, .. } =
                &mut forged.engine.event_history[at]
            else {
                panic!()
            };
            item.controller = p.opponent();
            let before = forged.clone();
            assert!(facts::capture(&forged, p).is_err());
            assert_eq!(forged, before);
        }
    }
}
#[test]
fn paid_original_division_target_loss_and_sourcegone_historical_damage_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let c = walker(&mut s, p);
        let a = put(&mut s, p.opponent(), "Axgard Cavalry", Zone::Battlefield);
        let b = put(&mut s, p.opponent(), "Shivan Dragon", Zone::Battlefield);
        // Native loyalty placement operand makes the genuine -4 cost remove its source.
        crate::planeswalker_v1::change_loyalty(&mut s, c, -2);
        let units = [vec![Target::Object(a); 3], vec![Target::Object(b); 5]].concat();
        activate(&mut s, p, c, 2, &units);
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::zone_change(a, Zone::Graveyard),
        );
        let f = capture(&s, p.opponent());
        let row = &f.activations[0];
        assert_eq!(
            row.targets
                .iter()
                .map(|t| t.allocated_damage.unwrap())
                .collect::<Vec<_>>(),
            vec![3, 5]
        );
        assert!(!row.targets[0].legal_now && row.targets[0].current_original.is_none());
        assert!(row.targets[1].legal_now);
        let source = row.damage_source.as_ref().unwrap();
        assert!(source.historical && source.source.current_original_source.is_none());
        assert_eq!(source.source.captured_controller, PlayerSeatV1::from(p));
        let start = s.engine.event_history.len();
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(
            s.engine.event_history[start..]
                .iter()
                .filter_map(|e| match e {
                    E::Damage {
                        source,
                        target,
                        amount,
                    } if *source == c => Some((*target, *amount)),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec![(Target::Object(b), 5)]
        );
        let f = capture(&s, p);
        let mut bad = f.clone();
        bad.activations[0].targets[1].allocated_damage = Some(8);
        assert!(facts::validate_current(&s, p, &bad).is_err());
    }
}
#[test]
fn paid_actual_crystal_affected_options_and_original_menu_index_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for double_first in [false, true] {
            let mut s = ready(p);
            cast(&mut s, p, "Twinflame Tyrant", &[], 5);
            finish(&mut s);
            let c = walker(&mut s, p);
            let target = put(&mut s, p.opponent(), "Shivan Dragon", Zone::Battlefield);
            // Existing next-damage shield primitive, distinct from a paid spell.
            s.engine.active_replacements.push(event::ActiveReplacement {
                id: 90_494,
                source: target,
                kind: event::ReplacementEffectKind::PreventNextDamage {
                    target: Target::Object(target),
                    remaining: 3,
                },
            });
            activate(&mut s, p, c, 2, &[Target::Object(target); 8]);
            for _ in 0..40 {
                let d = next(&mut s);
                if matches!(d, Decision::ChooseEffectOption { .. }) {
                    break;
                }
                pass(&mut s, d);
            }
            let f = capture(&s, p);
            let menu = f.pending_damage.as_ref().unwrap();
            assert_eq!(menu.chooser, PlayerSeatV1::from(p.opponent()));
            assert_eq!(menu.recipient.object, target.0);
            assert_eq!((menu.original_amount, menu.remaining_amount), (8, 8));
            assert_eq!(f.activations[0].stage, facts::ActivationStageV74::Resolving);
            assert!(menu
                .options
                .iter()
                .any(|o| o.kind == facts::PreventionKindV74::Multiplier));
            assert!(menu.options.iter().any(
                |o| o.kind == facts::PreventionKindV74::Shield && o.shield_remaining == Some(3)
            ));
            let option = menu
                .options
                .iter()
                .find(|o| {
                    o.kind
                        == if double_first {
                            facts::PreventionKindV74::Multiplier
                        } else {
                            facts::PreventionKindV74::Shield
                        }
                })
                .unwrap()
                .menu_index;
            let mut raw = serde_json::to_value(&f).unwrap();
            raw["pending_damage"]["options"][0]["history_index"] = 0.into();
            assert!(serde_json::from_value::<facts::FactsV74>(raw).is_err());
            let start = s.engine.event_history.len();
            let mut replay = restore(&s);
            for branch in [&mut s, &mut replay] {
                engine::step(branch, Action::ChooseEffectOption(option)).unwrap();
                while branch
                    .engine
                    .pending_effect
                    .as_ref()
                    .is_some_and(|p| p.choice.is_some())
                {
                    let _ = next(branch);
                    engine::step(branch, Action::ChooseEffectOption(0)).unwrap();
                }
                finish(branch);
                assert!(capture(branch, p).pending_damage.is_none());
                assert_eq!(
                    branch.engine.event_history[start..]
                        .iter()
                        .filter_map(|e| match e {
                            E::Damage {
                                source,
                                target: t,
                                amount,
                            } if *source == c && *t == Target::Object(target) => Some(*amount),
                            _ => None,
                        })
                        .collect::<Vec<_>>(),
                    vec![if double_first { 13 } else { 10 }]
                );
            }
            assert_eq!(s, replay);
        }
    }
}
#[test]
fn paid_face_down_copy_target_never_projects_hidden_original_printing_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let c = walker(&mut s, p);
        let creature = cast(&mut s, p, "Healer's Hawk", &[], 1);
        finish(&mut s);
        // Native face-down primitive; the actual original target contract stays private.
        s.objects.get_mut(creature).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: false,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        });
        activate(&mut s, p, c, 1, &[Target::Object(creature)]);
        let f = capture(&s, p.opponent());
        let target = &f.activations[0].targets[0];
        assert_eq!(target.original.object, creature.0);
        assert!(matches!(
            target.current_original.as_ref().unwrap().identity,
            crate::optional_trigger_public_v1::IdentityV1::FaceDown { .. }
        ));
        let raw = serde_json::to_value(target).unwrap();
        assert!(raw["original"].get("card_id").is_none());
        assert!(raw["original"].get("identity").is_none());
        finish(&mut s);
        let f = capture(&s, p.opponent());
        assert_eq!(f.copied_tokens.len(), 1);
        let copy = &f.copied_tokens[0];
        let form = f.battlefield[p.index()]
            .iter()
            .find(|r| r.card.object == copy.source.original_source.object)
            .unwrap();
        assert_eq!((form.form.power, form.form.toughness), (Some(2), Some(2)));
        assert_eq!(copy.current_haste, Some(true));
    }
}
