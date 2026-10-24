//! Genuine native FDN194 Partial481 fixtures. Initial lands/libraries/Hand cards
//! are labelled support operands; payment, turns, attack and casting are native.
use super::*;
use crate::{
    engine::{self, Action, Decision},
    policy_observation_v6::tests::put,
};

const NAME: &str = "Etali, Primal Storm";
fn id(name: &str) -> u16 {
    card_def::card_id_by_name(name).unwrap()
}
fn restored(s: &GameState) -> GameState {
    let r: GameState = serde_json::from_slice(&serde_json::to_vec(s).unwrap()).unwrap();
    assert_eq!(*s, r);
    assert_eq!(s.state_hash(), r.state_hash());
    r
}
fn next(s: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(s);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}
fn answer(s: &mut GameState, a: Action) {
    let mut r = restored(s);
    engine::step(s, a.clone()).unwrap();
    engine::step(&mut r, a).unwrap();
    assert_eq!(next(s), next(&mut r));
    assert_eq!(*s, r);
}
fn quiet(s: &mut GameState, d: Decision) {
    let a = match d {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
        Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        other => panic!("unexpected actual Etali decision {other:?}"),
    };
    answer(s, a);
}
fn settle(s: &mut GameState) {
    for _ in 0..160 {
        let d = next(s);
        if matches!(d, Decision::CastSpellOrPass { .. })
            && s.stack.is_empty()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_cast.is_none()
            && s.engine.pending_triggers.is_empty()
        {
            return;
        }
        quiet(s, d);
    }
    panic!("actual Etali stack did not finish");
}
fn paid(p: PlayerId, own: &[&str], other: &[&str]) -> (GameState, ObjectId) {
    let a = own.iter().map(|name| id(name)).collect::<Vec<_>>();
    let b = other.iter().map(|name| id(name)).collect::<Vec<_>>();
    let libraries = if p == PlayerId::P0 {
        [&a[..], &b[..]]
    } else {
        [&b[..], &a[..]]
    };
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        libraries[0],
        libraries[1],
        crate::rl::card_name,
        110_481,
        p,
    );
    s.step = crate::state::Step::Main1;
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut s).unwrap();
    let source = put(&mut s, p, NAME, Zone::Hand);
    let lands = (0..6)
        .map(|_| put(&mut s, p, "Mountain", Zone::Battlefield))
        .collect::<Vec<_>>();
    for _ in 0..4 {
        put(&mut s, p.opponent(), "Swamp", Zone::Battlefield);
    }
    assert!(
        matches!(next(&mut s), Decision::CastSpellOrPass {player,ref castable_spells,..}
        if player == p && castable_spells.contains(&source))
    );
    answer(&mut s, Action::CastSpell(source));
    assert!(matches!(next(&mut s), Decision::CastSpellOrPass {player,..} if player == p));
    assert!(lands.iter().all(|land| s.objects.get(*land).tapped));
    assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
    let row = s.stack.last().unwrap();
    assert_eq!(row.source, source);
    assert_eq!(row.kind, crate::state::StackItemKind::Spell);
    assert_eq!(row.controller, p);
    engine::validated_stack_item_target_spec(row, &s).unwrap();
    assert!(s.engine.event_history.iter().any(|e| matches!(e,
        event::CommittedEvent::SpellCast {spell,controller} if *spell == source && *controller == p)));
    settle(&mut s);
    assert_eq!(s.objects.get(source).zone, Zone::Battlefield);
    assert!(s.objects.get(source).summoning_sick);
    (s, source)
}
fn attack(s: &mut GameState, p: PlayerId, source: ObjectId) {
    let turn = s.turn_key_v1();
    let generation = s.objects.get(source).zone_change_count;
    let mut saw_sick = false;
    for _ in 0..512 {
        let d = next(s);
        if let Decision::DeclareAttackers {
            player,
            ref eligible,
        } = d
        {
            if player == p && s.turn_key_v1() == turn {
                assert!(!eligible.contains(&source));
                saw_sick = true;
            } else if player == p && eligible.contains(&source) {
                assert!(saw_sick);
                answer(s, Action::DeclareAttackers(vec![source]));
                assert_eq!(s.engine.combat.attackers, [source]);
                assert!(s.engine.event_history.iter().any(|e| matches!(e,
                    event::CommittedEvent::DeclaredAttacker {source:actual,source_zone_change_count,controller}
                    if *actual == source && *source_zone_change_count == generation && *controller == p)));
                return;
            }
        }
        quiet(s, d);
    }
    panic!("genuine paid source never reached its next attack");
}
fn batch(s: &mut GameState, p: PlayerId) -> ExileBatchChoice {
    for _ in 0..48 {
        let d = next(s);
        if let Some(pending) = &s.engine.pending_effect {
            if let Some(PendingEffectChoice::SelectTargets {
                purpose: EffectTargetSelectionPurpose::ExileBatch { choice },
                ..
            }) = &pending.choice
            {
                assert!(
                    matches!(d,Decision::ChooseEffectTargets {player,can_finish:true,..} if player == p)
                );
                validate_batch(s, pending, choice).unwrap();
                return choice.clone();
            }
        }
        quiet(s, d);
    }
    panic!("real top-card free-cast batch absent");
}
#[test]
fn paid_six_mana_printing_attack_sickness_and_old_consumer_refusal_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, source) = paid(p, &["Forest"; 40], &["Forest"; 40]);
        assert_eq!(id(NAME), 481);
        let d = &card_def::CARD_DEFS[id(NAME) as usize];
        assert_eq!(d.capability, card_def::CardCapability::Partial);
        assert_eq!(
            (d.power, d.toughness, d.cost.generic, d.mana_value),
            (Some(6), Some(6), 4, 6)
        );
        assert!(d.supertypes.contains(&card_def::Supertype::Legendary));
        assert_eq!(s.objects.get(source).owner, p);
        assert!(card_def::preflight_fully_supported_deck(&[id(NAME)]).is_err());
        assert!(crate::rl::observe_v1(&s, p, 0).is_err());
        assert!(crate::drake_public_v53::capture(&s).is_err());
    }
}
#[test]
fn paid_attack_exiles_only_each_top_land_or_empty_library_as_one_batch_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for empty in [false, true] {
            let cards = if empty {
                vec!["Forest"]
            } else {
                vec!["Forest"; 40]
            };
            let (mut s, source) = paid(p, &cards, &cards);
            attack(&mut s, p, source);
            let tops = [p, p.opponent()]
                .iter()
                .filter_map(|seat| s.players[seat.index()].library.first().copied())
                .collect::<Vec<_>>();
            let start = s.engine.event_history.len();
            settle(&mut s);
            let exiled = s.engine.event_history[start..]
                .iter()
                .filter_map(|e| match e {
                    event::CommittedEvent::ZoneChange {
                        object,
                        from: Zone::Library,
                        to: Zone::Exile,
                        ..
                    } => Some(*object),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(exiled, tops);
            assert_eq!(exiled.len(), if empty { 0 } else { 2 });
            for card in tops {
                assert_eq!(s.objects.get(card).zone, Zone::Exile);
            }
        }
    }
}
#[test]
fn paid_attack_optional_decline_preserves_both_nonlands_and_exact_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = paid(
            p,
            &["Forest", "Llanowar Elves", "Forest"],
            &["Forest", "Juggernaut", "Forest"],
        );
        attack(&mut s, p, source);
        let choice = batch(&mut s, p);
        assert_eq!(choice.scans.iter().filter_map(|scan| scan.hit).count(), 2);
        let before = s.clone();
        answer(&mut s, Action::FinishEffectSelection);
        settle(&mut s);
        for scan in choice.scans {
            assert_eq!(s.objects.get(scan.hit.unwrap().object).zone, Zone::Exile);
        }
        assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
        assert_ne!(s, before);
    }
}
#[test]
fn paid_attack_casts_both_nonlands_in_either_order_without_paying_base_cost_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for reverse in [false, true] {
            let (mut s, source) = paid(
                p,
                &["Forest", "Llanowar Elves", "Forest"],
                &["Forest", "Juggernaut", "Forest"],
            );
            attack(&mut s, p, source);
            let choice = batch(&mut s, p);
            let mut hits = choice
                .scans
                .iter()
                .filter_map(|scan| scan.hit)
                .collect::<Vec<_>>();
            if reverse {
                hits.reverse();
            }
            let start = s.engine.event_history.len();
            for hit in &hits {
                let current = batch(&mut s, p);
                assert!(
                    batch_candidates(&s, s.engine.pending_effect.as_ref().unwrap(), &current)
                        .contains(hit)
                );
                answer(
                    &mut s,
                    Action::ChooseEffectTarget(Target::Object(hit.object)),
                );
            }
            settle(&mut s);
            let casts = s.engine.event_history[start..]
                .iter()
                .filter_map(|e| match e {
                    event::CommittedEvent::SpellCast { spell, controller } if *controller == p => {
                        Some(*spell)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                casts,
                hits.iter().map(|card| card.object).collect::<Vec<_>>()
            );
            for hit in hits {
                let o = s.objects.get(hit.object);
                assert_eq!((o.zone, o.controller), (Zone::Battlefield, p));
            }
            assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
        }
    }
}
#[test]
fn paid_attack_changed_top_player_root_history_and_incarnation_refuse_readonly_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = paid(
            p,
            &["Forest", "Llanowar Elves", "Forest"],
            &["Forest", "Juggernaut", "Forest"],
        );
        attack(&mut s, p, source);
        let choice = batch(&mut s, p);
        for mode in 0..6 {
            let mut bad = restored(&s);
            let mut pending = bad.engine.pending_effect.clone().unwrap();
            let mut forged = choice.clone();
            match mode {
                0 => forged.players.reverse(),
                1 => forged.scans[0].exiled.clear(),
                2 => forged.scans[0].history_start = forged.scans[0].history_end,
                3 => forged.return_rest_to_bottom = true,
                4 => {
                    pending.resolving_item.inline_effect = Some(EffectOp::ExileUntilThenCastV1 {
                        players: choice.players.clone(),
                        predicate: choice.predicate,
                        return_rest_to_bottom: false,
                    });
                }
                _ => {
                    bad.objects
                        .get_mut(choice.scans[0].hit.unwrap().object)
                        .zone_change_count += 1
                }
            }
            let before = bad.clone();
            assert!(validate_batch(&bad, &pending, &forged).is_err());
            assert_eq!(bad, before);
        }
    }
}

#[test]
fn paid_attack_public_begin_unknown_context_and_direct_resume_refuse_native_row_theft_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = paid(
            p,
            &["Forest", "Llanowar Elves", "Forest"],
            &["Forest", "Juggernaut", "Forest"],
        );
        attack(&mut s, p, source);
        for _ in 0..32 {
            let d = next(&mut s);
            if s.stack.last().is_some_and(top_batch_item) {
                break;
            }
            quiet(&mut s, d);
        }
        let row = s.stack.last().unwrap().clone();
        assert!(top_batch_item(&row));
        let ctx = crystal_prevention_v1::context(&row);
        let mut bad = restored(&s);
        let before = bad.clone();
        assert!(begin_resumable_resolution(
            &EffectOp::ExileTopEachPlayerThenCastV1,
            &ctx,
            row.clone(),
            &mut bad
        )
        .is_err());
        assert_eq!(bad, before);
        let mut forged = ctx.clone();
        forged.stack_item_id = Some(crate::ids::StackItemId(u64::MAX));
        let draw = EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        };
        let mut bad = restored(&s);
        let mut expected = bad.clone();
        execute_without_linked_aura_v1(&draw, &forged, &mut bad);
        expected.engine.halted = Some((
            engine::UnsupportedMechanic::InvalidEffectContinuation,
            source,
        ));
        assert_eq!(bad, expected);
        batch(&mut s, p);
        let mut bad = restored(&s);
        let before = bad.clone();
        assert!(resume_resumable_resolution(&mut bad).is_err());
        assert_eq!(bad, before);
        answer(&mut s, Action::FinishEffectSelection);
        settle(&mut s);
    }
}

#[test]
fn paid_attack_real_removal_response_preserves_original_controller_and_departed_source_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = paid(
            p,
            &["Forest", "Llanowar Elves", "Forest"],
            &["Forest", "Juggernaut", "Forest"],
        );
        attack(&mut s, p, source);
        let original = s
            .stack
            .iter()
            .find(|row| top_batch_item(row))
            .unwrap()
            .clone();
        let captured = original.v4.ability_source_contract.unwrap();
        for _ in 0..16 {
            let d = next(&mut s);
            if matches!(d,Decision::CastSpellOrPass{player,..}if player==p.opponent()) {
                break;
            }
            quiet(&mut s, d);
        }
        let removal = put(&mut s, p.opponent(), "Hero's Downfall", Zone::Hand);
        assert!(
            matches!(next(&mut s),Decision::CastSpellOrPass{ref castable_spells,..}if castable_spells.contains(&removal))
        );
        answer(&mut s, Action::CastSpell(removal));
        assert!(
            matches!(next(&mut s),Decision::ChooseTargets{ref legal_targets,..}if legal_targets.contains(&Target::Object(source)))
        );
        answer(&mut s, Action::ChooseTarget(Target::Object(source)));
        let row = s.stack.last().unwrap();
        assert_eq!(row.source, removal);
        assert_eq!(s.players[p.opponent().index()].mana_pool, [0; 6]);
        for _ in 0..32 {
            let d = next(&mut s);
            if s.objects.get(source).zone == Zone::Graveyard {
                break;
            }
            quiet(&mut s, d);
        }
        assert_eq!(s.objects.get(source).zone, Zone::Graveyard);
        assert!(s.objects.get(source).zone_change_count > captured.zone_change_count);
        let choice = batch(&mut s, p);
        let pending = s.engine.pending_effect.as_ref().unwrap();
        assert_eq!(pending.ctx.controller, p);
        assert_eq!(pending.ctx.ability_source_contract, Some(captured));
        assert_eq!(choice.scans.iter().filter_map(|scan| scan.hit).count(), 2);
        answer(&mut s, Action::FinishEffectSelection);
        settle(&mut s);
    }
}
#[test]
fn paid_attack_free_x_is_zero_and_real_additional_discard_is_still_paid_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut own = vec!["Forest"; 40];
        own[1] = "Wildwood Scourge";
        let (mut s, source) = paid(p, &own, &["Forest"; 40]);
        attack(&mut s, p, source);
        let choice = batch(&mut s, p);
        let card = choice.scans[0].hit.unwrap().object;
        answer(&mut s, Action::ChooseEffectTarget(Target::Object(card)));
        let row = s.stack.iter().find(|row| row.source == card).unwrap();
        assert_eq!(row.kind, crate::state::StackItemKind::Spell);
        assert_eq!(row.v4.x_value, 0);
        assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
        settle(&mut s);
        assert_eq!(s.objects.get(card).zone, Zone::Graveyard);
        let mut own = vec!["Forest"; 40];
        own[1] = "Thrill of Possibility";
        let (mut s, source) = paid(p, &own, &["Forest"; 40]);
        attack(&mut s, p, source);
        let choice = batch(&mut s, p);
        let card = choice.scans[0].hit.unwrap().object;
        let discard = s.players[p.index()].hand[0];
        assert_eq!(s.objects.get(discard).card_def, id("Forest"));
        // Keep an actual choice: a singleton hand is discarded automatically.
        let kept = put(&mut s, p, "Mountain", Zone::Hand);
        answer(&mut s, Action::ChooseEffectTarget(Target::Object(card)));
        assert!(matches!(next(&mut s),Decision::Discard{player,count:1,..}if player==p));
        answer(&mut s, Action::Discard(vec![discard]));
        let row = s.stack.iter().find(|row| row.source == card).unwrap();
        assert!(row.discarded.contains(&discard));
        assert_eq!(s.objects.get(discard).zone, Zone::Graveyard);
        assert_eq!(s.objects.get(kept).zone, Zone::Hand);
        assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
        settle(&mut s);
    }
}
#[test]
fn independent_prepaid_hidden_libraries_do_not_change_actual_public_free_cast_menu_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let own = ["Forest", "Llanowar Elves", "Forest", "Forest"];
        let (mut a, source) = paid(p, &own, &["Forest", "Juggernaut", "Forest", "Forest"]);
        let (mut b, other) = paid(p, &own, &["Forest", "Juggernaut", "Swamp", "Island"]);
        assert_eq!(source, other);
        assert_ne!(a, b);
        attack(&mut a, p, source);
        attack(&mut b, p, other);
        let ca = batch(&mut a, p);
        let cb = batch(&mut b, p);
        assert_eq!(next(&mut a), next(&mut b));
        assert_eq!(
            ca.scans
                .iter()
                .filter_map(|scan| scan.hit)
                .collect::<Vec<_>>(),
            cb.scans
                .iter()
                .filter_map(|scan| scan.hit)
                .collect::<Vec<_>>()
        );
        answer(&mut a, Action::FinishEffectSelection);
        answer(&mut b, Action::FinishEffectSelection);
        settle(&mut a);
        settle(&mut b);
        assert_eq!(next(&mut a), next(&mut b));
    }
}

#[test]
fn paid_ready_resume_owner_binds_entire_pending_and_remaining_stack_without_public_loan_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source) = paid(
            p,
            &["Forest", "Llanowar Elves", "Forest"],
            &["Forest", "Juggernaut", "Forest"],
        );
        attack(&mut s, p, source);
        batch(&mut s, p);
        engine::step(&mut s, Action::FinishEffectSelection).unwrap();
        let row = s
            .engine
            .pending_effect
            .as_ref()
            .unwrap()
            .resolving_item
            .clone();
        assert!(acquire_top_batch_resume_v1(&s, &row).unwrap().is_some());
        for mode in 0..3 {
            let mut bad = restored(&s);
            let owner = acquire_top_batch_resume_v1(&bad, &row).unwrap().unwrap();
            let position =
                engine::resolution_cast_v1::resolving_index(&bad, row.v4.stack_item_id).unwrap();
            assert_eq!(bad.stack.remove(position), row);
            match mode {
                0 => bad.engine.pending_effect.as_mut().unwrap().ctx.controller = p.opponent(),
                1 => bad.engine.pending_effect.as_mut().unwrap().frames.clear(),
                _ => bad.stack.push(row.clone()),
            }
            let before = bad.clone();
            assert!(resume_owned_top_batch_v1(&mut bad, owner).is_err());
            assert_eq!(bad, before);
        }
        settle(&mut s);
    }
}
