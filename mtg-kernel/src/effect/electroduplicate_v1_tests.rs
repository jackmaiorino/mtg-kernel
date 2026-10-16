//! FDN85 native Partial. Every named paid spell uses original native cast,
//! target, payment/publication and response windows. Control/counter primitives
//! are labelled explicitly; there is no invented paid ability-counter card.
use super::*;
use crate::{
    engine::{self, Action, Decision},
    event::CommittedEvent,
    policy_observation_v6::tests::put,
    state::Step,
};
const NAME: &str = "Electroduplicate";
fn id(name: &str) -> u16 {
    card_def::card_id_by_name(name).unwrap()
}
fn ready(actor: PlayerId) -> GameState {
    let library = [id("Forest"); 40];
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &library,
        &library,
        crate::rl::card_name,
        110_471,
        actor,
    );
    s.step = Step::Main1;
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut s).unwrap();
    for (seat, names) in [
        (actor, ["Mountain", "Forest", "Island"]),
        (actor.opponent(), ["Island", "Forest", "Mountain"]),
    ] {
        for name in names {
            for _ in 0..8 {
                put(&mut s, seat, name, Zone::Battlefield);
            }
        }
    }
    s
}
fn restore(s: &GameState) -> GameState {
    let r: GameState = serde_json::from_slice(&serde_json::to_vec(s).unwrap()).unwrap();
    assert_eq!(s, &r);
    assert_eq!(s.state_hash(), r.state_hash());
    r
}
fn quiet(s: &mut GameState, d: Decision) {
    let a = match d {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
        Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        Decision::Discard { count, choices, .. } => {
            Action::Discard(choices.into_iter().take(count as usize).collect())
        }
        d => panic!("actual FDN85 native decision {d:?}"),
    };
    engine::step(s, a).unwrap();
    assert!(s.engine.halted.is_none(), "{:?}", s.engine.halted);
}
fn priority(s: &mut GameState, actor: PlayerId) {
    for _ in 0..16 {
        let d = engine::advance_until_decision(s);
        if matches!(d,Decision::CastSpellOrPass{player,..} if player==actor) {
            return;
        }
        quiet(s, d);
    }
    panic!("native priority absent")
}
fn finish(s: &mut GameState) {
    for _ in 0..128 {
        let d = engine::advance_until_decision(s);
        assert!(s.engine.halted.is_none(), "{:?}", s.engine.halted);
        if s.stack.is_empty()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_triggers.is_empty()
            && !crate::linked_aura_operation_v1::has_pending(s)
        {
            assert!(matches!(d, Decision::CastSpellOrPass { .. }));
            return;
        }
        quiet(s, d);
    }
    panic!("native resolution did not finish")
}
fn taps(s: &GameState, p: PlayerId) -> usize {
    s.players[p.index()]
        .battlefield
        .iter()
        .filter(|id| s.objects.get(**id).tapped)
        .count()
}
fn cast_card(
    s: &mut GameState,
    p: PlayerId,
    card: ObjectId,
    targets: &[Target],
    cost: usize,
) -> ObjectId {
    priority(s, p);
    assert!(
        matches!(engine::advance_until_decision(s),Decision::CastSpellOrPass{castable_spells,..} if castable_spells.contains(&card))
    );
    let before = taps(s, p);
    engine::step(s, Action::CastSpell(card)).unwrap();
    let mut next = 0;
    for _ in 0..16 {
        match engine::advance_until_decision(s) {
            Decision::ChooseTargets {
                player,
                legal_targets,
                ..
            } => {
                assert_eq!(player, p);
                let target = targets[next];
                assert!(legal_targets.contains(&target));
                engine::step(s, Action::ChooseTarget(target)).unwrap();
                next += 1;
            }
            Decision::CastSpellOrPass { player, .. } => {
                assert_eq!(player, p);
                assert_eq!(next, targets.len());
                assert!(s.engine.pending_cast.is_none());
                assert_eq!(taps(s, p) - before, cost);
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                assert_eq!(s.objects.get(card).zone, Zone::Stack);
                assert!(s.stack.iter().any(|r| r.source == card
                    && r.kind == StackItemKind::Spell
                    && r.controller == p));
                return card;
            }
            d => panic!("genuine paid cast {d:?}"),
        }
    }
    panic!("native paid cast did not publish")
}
fn cast(s: &mut GameState, p: PlayerId, name: &str, targets: &[Target], cost: usize) -> ObjectId {
    priority(s, p);
    let card = put(s, p, name, Zone::Hand);
    cast_card(s, p, card, targets, cost)
}
fn paid(s: &mut GameState, p: PlayerId, name: &str, cost: usize) -> ObjectId {
    let card = cast(s, p, name, &[], cost);
    finish(s);
    assert_eq!(s.objects.get(card).zone, Zone::Battlefield);
    card
}
fn copies(s: &GameState, definition: u16) -> Vec<ObjectId> {
    s.objects
        .iter()
        .filter_map(|(id, o)| {
            (o.zone == Zone::Battlefield
                && o.v4.is_token
                && o.card_def == definition
                && o.v4.copied_end_step_sacrifices_v1 != 0)
                .then_some(id)
        })
        .collect()
}
fn electro(s: &mut GameState, p: PlayerId, target: ObjectId) -> (ObjectId, ObjectId) {
    let original = copies(s, s.objects.get(target).card_def);
    let spell = cast(s, p, NAME, &[Target::Object(target)], 3);
    let mut r = restore(s);
    finish(s);
    finish(&mut r);
    assert_eq!(s, &r);
    let new = copies(s, s.objects.get(target).card_def)
        .into_iter()
        .filter(|id| !original.contains(id))
        .collect::<Vec<_>>();
    assert_eq!(new.len(), 1);
    (spell, new[0])
}
fn end_priority(s: &mut GameState) -> Decision {
    for _ in 0..400 {
        let d = engine::advance_until_decision(s);
        assert!(s.engine.halted.is_none(), "{:?}", s.engine.halted);
        if s.step == Step::End {
            return d;
        }
        quiet(s, d);
    }
    panic!("actual end step absent")
}
fn next_end(s: &mut GameState) -> Decision {
    // Leave the already-begun End step through genuine passes/cleanup/turns.
    for _ in 0..400 {
        let d = engine::advance_until_decision(s);
        assert!(s.engine.halted.is_none(), "{:?}", s.engine.halted);
        if s.step != Step::End {
            return end_priority(s);
        }
        quiet(s, d);
    }
    panic!("next actual end step absent")
}
fn publish_end(s: &mut GameState, d: Decision) -> Vec<StackItem> {
    quiet_if_order(s, d);
    s.stack
        .iter()
        .filter(|r| {
            matches!(
                r.inline_effect,
                Some(EffectOp::CopiedTokenEndStepSacrificeV1 { .. })
            )
        })
        .cloned()
        .collect()
}
fn quiet_if_order(s: &mut GameState, d: Decision) {
    match d {
        Decision::OrderTriggers { pending, .. } => {
            engine::step(s, Action::OrderTriggers((0..pending.len()).collect())).unwrap();
            let d = engine::advance_until_decision(s);
            assert!(matches!(d, Decision::CastSpellOrPass { .. }));
        }
        Decision::CastSpellOrPass { .. } => {}
        d => panic!("actual end-step placement {d:?}"),
    }
}
#[test]
fn genuine_paid_normal_flashback_countered_casts_and_exact_printing_restore_both_seats() {
    assert_eq!(id(NAME), 471);
    assert!(electroduplicate_definition_v1(471));
    assert!(card_def::preflight_fully_supported_deck(&[471]).is_err());
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        assert!(!serde_json::to_string(&s)
            .unwrap()
            .contains("copied_end_step_sacrifices_v1"));
        let source = paid(&mut s, actor, "Llanowar Elves", 1);
        let (spell, token) = electro(&mut s, actor, source);
        assert_eq!(s.objects.get(spell).zone, Zone::Graveyard);
        assert_eq!(s.objects.get(token).v4.copied_end_step_sacrifices_v1, 1);
        cast_card(&mut s, actor, spell, &[Target::Object(source)], 4);
        assert!(
            s.stack
                .iter()
                .find(|r| r.source == spell)
                .unwrap()
                .is_flashback
        );
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(s.objects.get(spell).zone, Zone::Exile);
        assert_eq!(copies(&s, id("Llanowar Elves")).len(), 2);
        assert!(crate::rl::observe_v1(&s, actor, 0).is_err());
        assert!(crate::drake_public_v53::capture(&s).is_err());
        let mut s = ready(actor);
        let source = paid(&mut s, actor, "Llanowar Elves", 1);
        let spell = cast(&mut s, actor, NAME, &[Target::Object(source)], 3);
        for flashback in [false, true] {
            if flashback {
                cast_card(&mut s, actor, spell, &[Target::Object(source)], 4);
            }
            cast(
                &mut s,
                actor.opponent(),
                "Counterspell",
                &[Target::Object(spell)],
                2,
            );
            let mut r = restore(&s);
            finish(&mut s);
            finish(&mut r);
            assert_eq!(s, r);
            assert_eq!(
                s.objects.get(spell).zone,
                if flashback {
                    Zone::Exile
                } else {
                    Zone::Graveyard
                }
            );
            assert!(copies(&s, id("Llanowar Elves")).is_empty());
        }
    }
}
#[test]
fn genuine_paid_haste_uses_original_native_attacker_commit_and_restores_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let source = paid(&mut s, actor, "Llanowar Elves", 1);
        let (_, token) = electro(&mut s, actor, source);
        assert!(s.objects.get(source).summoning_sick);
        assert!(!engine::has_effective_keyword(&s, source, Keywords::HASTE));
        assert!(engine::has_effective_keyword(&s, token, Keywords::HASTE));
        for _ in 0..48 {
            let d = engine::advance_until_decision(&mut s);
            if let Decision::DeclareAttackers { ref eligible, .. } = d {
                assert!(eligible.contains(&token));
                assert!(!eligible.contains(&source));
                let mut r = restore(&s);
                for b in [&mut s, &mut r] {
                    engine::step(b, Action::DeclareAttackers(vec![token])).unwrap();
                    assert_eq!(b.engine.combat.attackers, vec![token]);
                    assert!(b.objects.get(token).tapped);
                    assert_eq!(b.objects.get(token).controller, actor);
                }
                assert_eq!(s, r);
                break;
            }
            quiet(&mut s, d);
        }
        assert_eq!(s.engine.combat.attackers, vec![token]);
    }
}
#[test]
fn genuine_paid_response_bounce_recast_and_control_change_fizzle_original_target_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let source = paid(&mut s, actor, "Spectral Sailor", 1);
        let generation = s.objects.get(source).zone_change_count;
        let other = put(
            &mut s,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let spell = cast(&mut s, actor, NAME, &[Target::Object(source)], 3);
        let bounce = cast(
            &mut s,
            actor.opponent(),
            "Run Away Together",
            &[Target::Object(source), Target::Object(other)],
            2,
        );
        for _ in 0..12 {
            let d = engine::advance_until_decision(&mut s);
            if !s.stack.iter().any(|r| r.source == bounce) {
                break;
            }
            quiet(&mut s, d);
        }
        assert_eq!(s.objects.get(source).zone, Zone::Hand);
        cast_card(&mut s, actor, source, &[], 1);
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(s.objects.get(source).zone, Zone::Battlefield);
        assert!(s.objects.get(source).zone_change_count > generation);
        assert!(copies(&s, id("Spectral Sailor")).is_empty());
        assert_eq!(s.objects.get(spell).zone, Zone::Graveyard);
        let mut s = ready(actor);
        let source = paid(&mut s, actor, "Llanowar Elves", 1);
        let spell = cast(&mut s, actor, NAME, &[Target::Object(source)], 3);
        // Real native control primitive, not an illegal sorcery response claim.
        crate::standard_cards_v1::gain_control(&mut s, source, actor.opponent());
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert!(copies(&s, id("Llanowar Elves")).is_empty());
        assert_eq!(s.objects.get(spell).zone, Zone::Graveyard);
        let mut s = ready(actor);
        let source = put(
            &mut s,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        // Native control setup gives the spell controller, rather than the
        // physical source owner, ownership and control of the created token.
        crate::standard_cards_v1::gain_control(&mut s, source, actor);
        let (_, token) = electro(&mut s, actor, source);
        assert_eq!(s.objects.get(source).owner, actor.opponent());
        assert_eq!(s.objects.get(token).owner, actor);
        assert_eq!(s.objects.get(token).controller, actor);
    }
}
#[test]
fn genuine_paid_copy_of_copy_and_self_reflection_inherit_instances_and_countered_recurrence_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let source = paid(&mut s, actor, "Llanowar Elves", 1);
        let (_, first) = electro(&mut s, actor, source);
        cast(&mut s, actor, "Giant Growth", &[Target::Object(first)], 1);
        finish(&mut s);
        assert_eq!(engine::effective_power(&s, first), 4);
        let (_, second) = electro(&mut s, actor, first);
        assert_eq!(engine::effective_power(&s, second), 1);
        assert_eq!(s.objects.get(second).v4.copied_end_step_sacrifices_v1, 2);
        let before = copies(&s, id("Llanowar Elves"));
        cast(
            &mut s,
            actor,
            "Self-Reflection",
            &[Target::Object(second)],
            6,
        );
        finish(&mut s);
        let third = *copies(&s, id("Llanowar Elves"))
            .iter()
            .find(|id| !before.contains(id))
            .unwrap();
        assert_eq!(s.objects.get(third).v4.copied_end_step_sacrifices_v1, 2);
        let d = end_priority(&mut s);
        let mut r = restore(&s);
        let rows = publish_end(&mut s, d.clone());
        publish_end(&mut r, d);
        assert_eq!(s, r);
        assert_eq!(rows.len(), 5);
        let row = rows.iter().find(|r| r.source == first).unwrap().clone();
        // Existing native counter primitive. Limited has no paid ability-counter
        // producer; the actual token/spells above were all genuinely paid.
        for b in [&mut s, &mut r] {
            engine::counter_stack_item_by_id(b, row.v4.stack_item_id).unwrap();
        }
        assert_eq!(s, r);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(s.objects.get(first).zone, Zone::Battlefield);
        assert!(!s.players[actor.index()].battlefield.contains(&second));
        assert!(!s.players[actor.index()].battlefield.contains(&third));
        assert!(engine::has_effective_keyword(&s, first, Keywords::HASTE));
        assert!(engine::validated_stack_item_target_spec(&row, &s).is_err());
        let d = next_end(&mut s);
        let rows = publish_end(&mut s, d);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source, first);
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert!(!s.players[actor.index()].battlefield.contains(&first));
    }
}
#[test]
fn genuine_paid_suppression_removed_after_end_begins_and_captured_controller_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let source = paid(&mut s, actor, "Llanowar Elves", 1);
        let (_, token) = electro(&mut s, actor, source);
        let aura = cast(
            &mut s,
            actor,
            "Witness Protection",
            &[Target::Object(token)],
            1,
        );
        finish(&mut s);
        assert!(!engine::has_effective_keyword(&s, token, Keywords::HASTE));
        let d = end_priority(&mut s);
        assert!(matches!(d, Decision::CastSpellOrPass { .. }));
        assert!(!s.stack.iter().any(|r| r.source == token));
        assert_eq!(s.objects.get(token).zone, Zone::Battlefield);
        cast(
            &mut s,
            actor.opponent(),
            "Broken Wings",
            &[Target::Object(aura)],
            3,
        );
        finish(&mut s);
        assert_eq!(s.step, Step::End);
        assert!(engine::has_effective_keyword(&s, token, Keywords::HASTE));
        assert!(!s.stack.iter().any(|r| r.source == token));
        // Native control primitive before the next actual end-step event.
        crate::standard_cards_v1::gain_control(&mut s, token, actor.opponent());
        let mut r = restore(&s);
        let d = next_end(&mut s);
        let rd = next_end(&mut r);
        assert_eq!(d, rd);
        let rows = publish_end(&mut s, d);
        publish_end(&mut r, rd);
        assert_eq!(s, r);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].controller, actor.opponent());
        assert_eq!(rows[0].v4.ability_source_contract.unwrap().owner, actor);
        // A trigger cannot sacrifice a permanent its captured controller no
        // longer controls. The recurring ability itself stays on the token.
        for b in [&mut s, &mut r] {
            crate::standard_cards_v1::gain_control(b, token, actor);
            finish(b);
            assert_eq!(b.objects.get(token).zone, Zone::Battlefield);
        }
        assert_eq!(s, r);
        let d = next_end(&mut s);
        let rows = publish_end(&mut s, d);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].controller, actor);
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert!(!s.players[actor.index()].battlefield.contains(&token));
    }
}
#[test]
fn genuine_paid_season_one_batch_and_animated_land_copiable_values_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        paid(&mut s, actor, "Doubling Season", 5);
        let source = paid(&mut s, actor, "Llanowar Elves", 1);
        let start = s.engine.event_history.len();
        cast(&mut s, actor, NAME, &[Target::Object(source)], 3);
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        let tokens = copies(&s, id("Llanowar Elves"));
        assert_eq!(tokens.len(), 2);
        for token in tokens {
            assert_eq!(s.objects.get(token).v4.copied_end_step_sacrifices_v1, 1);
            assert!(engine::has_effective_keyword(&s, token, Keywords::HASTE));
        }
        assert_eq!(
            s.engine.event_history[start..]
                .iter()
                .filter(|e| matches!(e, CommittedEvent::TokenCreationReplacementV48 { .. }))
                .count(),
            1
        );
        let mut s = ready(actor);
        priority(&mut s, actor);
        let land = put(&mut s, actor, "Soulstone Sanctuary", Zone::Hand);
        engine::step(&mut s, Action::PlayLand(land)).unwrap();
        priority(&mut s, actor);
        let before = taps(&s, actor);
        engine::step(&mut s, Action::ActivateAbility(land, 0)).unwrap();
        priority(&mut s, actor);
        assert_eq!(taps(&s, actor) - before, 4);
        finish(&mut s);
        assert!(engine::object_has_type(&s, land, CardType::Creature));
        let (_, token) = electro(&mut s, actor, land);
        assert!(engine::object_has_type(&s, token, CardType::Land));
        assert!(!engine::object_has_type(&s, token, CardType::Creature));
        assert!(s.objects.get(token).v4.animation_timestamp.is_none());
        assert_eq!(s.objects.get(token).v4.copied_end_step_sacrifices_v1, 1);
        let d = end_priority(&mut s);
        let rows = publish_end(&mut s, d);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source, token);
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert!(!s.players[actor.index()].battlefield.contains(&token));
    }
}
#[test]
fn genuine_paid_native_source_frame_and_public_resume_forgery_refusals_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let source = paid(&mut s, actor, "Llanowar Elves", 1);
        cast(&mut s, actor, NAME, &[Target::Object(source)], 3);
        let item = s.stack.last().unwrap().clone();
        let ctx = crystal_prevention_v1::context(&item);
        let before = s.clone();
        assert!(begin_resumable_resolution(
            &EffectOp::ElectroduplicateCopyV1,
            &ctx,
            item.clone(),
            &mut s
        )
        .is_err());
        assert_eq!(s, before);
        for mode in 0..4 {
            let mut bad = restore(&s);
            let mut row = item.clone();
            row.inline_effect = Some(EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            });
            row.kind = StackItemKind::ActivatedAbility;
            row.v4.source_contract = None;
            let mut forged = ctx.clone();
            forged.stack_item_id = if mode % 2 == 0 {
                None
            } else {
                Some(crate::ids::StackItemId(u64::MAX))
            };
            if mode >= 2 {
                row.v4.stack_item_id = crate::ids::StackItemId(u64::MAX - 1);
            }
            bad.engine.pending_effect = Some(EffectContinuation {
                resolving_item: row,
                ctx: forged,
                frames: vec![EffectFrame::Program {
                    op: EffectOp::DrawCards {
                        player: PlayerRef::Controller,
                        count: 1,
                    },
                    path: vec![],
                }],
                choice: None,
                answered_choice_guard: None,
            });
            let before = bad.clone();
            assert!(resume_resumable_resolution(&mut bad).is_err());
            assert_eq!(bad, before);
        }
        for op in [
            EffectOp::ElectroduplicateCopyV1,
            EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            },
        ] {
            let mut bad = restore(&s);
            execute(&op, &ctx, &mut bad);
            let mut expected = s.clone();
            expected.engine.halted = Some((
                engine::UnsupportedMechanic::InvalidEffectContinuation,
                ctx.source,
            ));
            assert_eq!(bad, expected);
        }
        finish(&mut s);
        let token = copies(&s, id("Llanowar Elves"))[0];
        let mut erased = restore(&s);
        erased
            .objects
            .get_mut(token)
            .v4
            .copied_end_step_sacrifices_v1 = 0;
        let before = erased.clone();
        assert!(engine::step(&mut erased, Action::Pass).is_err());
        assert_eq!(erased, before);
        cast(&mut s, actor, NAME, &[Target::Object(token)], 3);
        let row = s.stack.last().unwrap().clone();
        let ctx = crystal_prevention_v1::context(&row);
        let mut overflow = restore(&s);
        overflow
            .objects
            .get_mut(token)
            .v4
            .copied_end_step_sacrifices_v1 = u32::MAX;
        let before = overflow.clone();
        assert!(electroduplicate_proposal_v1(&overflow, &ctx).is_err());
        assert_eq!(overflow, before);
        finish(&mut s);
        let d = end_priority(&mut s);
        let rows = publish_end(&mut s, d);
        let row = rows[0].clone();
        let receipt = s
            .engine
            .event_history
            .iter()
            .position(|e| matches!(e, CommittedEvent::CopiedTokenEndStepV1 { .. }))
            .unwrap();
        for mode in 0..4 {
            let mut bad = restore(&s);
            match mode {
                0 => {
                    bad.engine.event_history.remove(receipt);
                }
                1 => {
                    let copy = bad.engine.event_history[receipt].clone();
                    bad.engine.event_history.push(copy);
                }
                2 => {
                    bad.engine.event_history.swap(receipt, receipt - 1);
                }
                _ => {
                    if let CommittedEvent::CopiedTokenEndStepV1 { sources, .. } =
                        &mut bad.engine.event_history[receipt]
                    {
                        sources[0].1 += 1;
                    }
                }
            }
            let before = bad.clone();
            assert!(engine::validated_stack_item_target_spec(&row, &bad).is_err());
            assert_eq!(bad, before);
        }
        for mode in 0..4 {
            let mut bad = row.clone();
            match mode {
                0 => bad.controller = actor.opponent(),
                1 => {
                    bad.v4
                        .ability_source_contract
                        .as_mut()
                        .unwrap()
                        .zone_change_count += 1
                }
                2 => {
                    bad.inline_effect = Some(EffectOp::DrawCards {
                        player: PlayerRef::Controller,
                        count: 1,
                    })
                }
                _ => {
                    if let Some(EffectOp::CopiedTokenEndStepSacrificeV1 { ability, .. }) =
                        &mut bad.inline_effect
                    {
                        *ability = u32::MAX;
                    }
                }
            };
            let before = s.clone();
            assert!(engine::validated_stack_item_target_spec(&bad, &s).is_err());
            assert_eq!(s, before);
        }
    }
}

#[test]
fn genuine_paid_dragon_tokens_share_native_name_without_losing_distinct_printings_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let trainer = paid(&mut s, actor, "Dragon Trainer", 5);
        let four = s
            .objects
            .iter()
            .find_map(|(object, o)| {
                (o.zone == Zone::Battlefield && o.v4.is_token && o.card_def == id("Dragon Token"))
                    .then_some(object)
            })
            .unwrap();
        paid(&mut s, actor, "Rite of the Dragoncaller", 6);
        cast(&mut s, actor, "Think Twice", &[], 2);
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        let five = s
            .objects
            .iter()
            .find_map(|(object, o)| {
                (o.zone == Zone::Battlefield
                    && o.v4.is_token
                    && o.card_def == id("Dragon 5/5 Token"))
                .then_some(object)
            })
            .unwrap();
        assert_ne!(s.objects.get(four).card_def, s.objects.get(five).card_def);
        assert_eq!(engine::checked_effective_pt(&s, four).unwrap(), (4, 4));
        assert_eq!(engine::checked_effective_pt(&s, five).unwrap(), (5, 5));
        for token in [four, five] {
            assert_eq!(engine::effective_name(&s, token), "Dragon Token");
            assert!(engine::has_effective_keyword(&s, token, Keywords::FLYING));
        }
        assert!(engine::objects_share_name(&s, four, five));
        assert!(!engine::objects_share_name(&s, trainer, five));
    }
}

fn horde_rows(s: &GameState) -> Vec<StackItem> {
    s.stack
        .iter()
        .filter(|r| r.inline_effect == Some(EffectOp::CreateTokenCopyOfSourceV1))
        .cloned()
        .collect()
}
fn publish_horde(s: &mut GameState, expected: usize) {
    for _ in 0..64 {
        let d = engine::advance_until_decision(s);
        assert!(s.engine.halted.is_none(), "{:?}", s.engine.halted);
        if !horde_rows(s).is_empty() {
            quiet_if_order(s, d);
            assert_eq!(horde_rows(s).len(), expected);
            return;
        }
        quiet(s, d);
    }
    panic!("genuine second-draw Horde trigger absent")
}
fn hordes(s: &GameState) -> Vec<ObjectId> {
    s.objects
        .iter()
        .filter_map(|(object, o)| {
            (o.zone == Zone::Battlefield
                && matches!(
                    card_def::CARD_DEFS[usize::from(o.card_def)].name,
                    "Homunculus Horde" | "Homunculus Horde Token"
                ))
            .then_some(object)
        })
        .collect()
}
fn second_draw_horde(s: &mut GameState, actor: PlayerId, expected: usize) {
    assert_eq!(s.players[actor.index()].draws_this_turn, 0);
    cast(s, actor, "Think Twice", &[], 2);
    finish(s);
    assert_eq!(s.players[actor.index()].draws_this_turn, 1);
    assert!(horde_rows(s).is_empty());
    cast(s, actor, "Think Twice", &[], 2);
    publish_horde(s, expected);
    assert_eq!(s.players[actor.index()].draws_this_turn, 2);
}
fn response_resolved_with_horde(s: &mut GameState, response: ObjectId) {
    for _ in 0..64 {
        let d = engine::advance_until_decision(s);
        assert!(s.engine.halted.is_none(), "{:?}", s.engine.halted);
        if s.objects.get(response).zone == Zone::Graveyard {
            assert!(!horde_rows(s).is_empty());
            assert!(matches!(d, Decision::CastSpellOrPass { .. }));
            return;
        }
        quiet(s, d);
    }
    panic!("paid response did not resolve before original Horde row")
}

#[test]
fn genuine_paid_horde_second_draw_copies_copiable_abilities_and_real_end_step_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let original = paid(&mut s, actor, "Homunculus Horde", 4);
        let (_, first) = electro(&mut s, actor, original);
        let (_, second) = electro(&mut s, actor, first);
        assert_eq!(s.objects.get(first).v4.copied_end_step_sacrifices_v1, 1);
        assert_eq!(s.objects.get(second).v4.copied_end_step_sacrifices_v1, 2);
        second_draw_horde(&mut s, actor, 3);
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        let all = hordes(&s);
        assert_eq!(all.len(), 6);
        let mut copied = Vec::new();
        for object in &all {
            assert_eq!(engine::checked_effective_pt(&s, *object).unwrap(), (2, 2));
            let count = s.objects.get(*object).v4.copied_end_step_sacrifices_v1;
            if count != 0 {
                assert!(engine::has_effective_keyword(&s, *object, Keywords::HASTE));
                copied.push(count);
            }
        }
        copied.sort_unstable();
        assert_eq!(copied, vec![1, 1, 2, 2]);
        let d = end_priority(&mut s);
        let rows = publish_end(&mut s, d);
        assert_eq!(rows.len(), 6);
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(hordes(&s).len(), 2);
        assert_eq!(s.objects.get(original).zone, Zone::Battlefield);
        assert!(hordes(&s)
            .into_iter()
            .all(|o| s.objects.get(o).v4.copied_end_step_sacrifices_v1 == 0));
    }
}

#[test]
fn genuine_paid_horde_departed_copy_uses_original_lki_and_refuses_corruption_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let original = paid(&mut s, actor, "Homunculus Horde", 4);
        let (_, copied) = electro(&mut s, actor, original);
        second_draw_horde(&mut s, actor, 2);
        let row = horde_rows(&s)
            .into_iter()
            .find(|r| r.source == copied)
            .unwrap();
        let ctx = crystal_prevention_v1::context(&row);
        for op in [
            EffectOp::CreateTokenCopyOfSourceV1,
            EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            },
        ] {
            let mut bad = restore(&s);
            assert!(begin_resumable_resolution(&op, &ctx, row.clone(), &mut bad).is_err());
            assert_eq!(bad, s);
            execute(&op, &ctx, &mut bad);
            let mut expected = s.clone();
            expected.engine.halted = Some((
                engine::UnsupportedMechanic::InvalidEffectContinuation,
                ctx.source,
            ));
            assert_eq!(bad, expected);
        }
        let response = cast(
            &mut s,
            actor.opponent(),
            "Unsummon",
            &[Target::Object(copied)],
            1,
        );
        response_resolved_with_horde(&mut s, response);
        assert_ne!(s.objects.get(copied).zone, Zone::Battlefield);
        let receipt = s
            .engine
            .event_history
            .iter()
            .position(|e| matches!(e, CommittedEvent::SourceCopyDepartureV1 { .. }))
            .unwrap();
        assert!(engine::validated_stack_item_target_spec(&row, &s).is_ok());
        for mode in 0..4 {
            let mut json = serde_json::to_value(&s).unwrap();
            let history = json["engine"]["event_history"].as_array_mut().unwrap();
            match mode {
                0 => {
                    history.remove(receipt);
                }
                1 => {
                    let duplicate = history[receipt].clone();
                    history.push(duplicate);
                }
                2 => {
                    history[receipt]["SourceCopyDepartureV1"]["departure"]["values"]["FaceUp"]
                        ["copied_sacrifices"] = serde_json::json!(0);
                }
                _ => {
                    history[receipt]["SourceCopyDepartureV1"]["departure"]["source"]
                        ["zone_change_count"] = serde_json::json!(1);
                }
            }
            let bad: GameState = serde_json::from_value(json).unwrap();
            let before = bad.clone();
            assert!(engine::validated_stack_item_target_spec(&row, &bad).is_err());
            assert_eq!(bad, before);
        }
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(hordes(&s).len(), 3);
        let descendants = hordes(&s)
            .into_iter()
            .filter(|o| s.objects.get(*o).v4.copied_end_step_sacrifices_v1 != 0)
            .collect::<Vec<_>>();
        assert_eq!(descendants.len(), 1);
        assert_eq!(
            s.objects
                .get(descendants[0])
                .v4
                .copied_end_step_sacrifices_v1,
            1
        );
        assert!(engine::has_effective_keyword(
            &s,
            descendants[0],
            Keywords::HASTE
        ));
    }
}

#[test]
fn genuine_paid_horde_season_copies_ignore_response_boost_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        // Additional genuine lands fund all paid casts before this turn's second draw.
        for _ in 0..8 {
            put(&mut s, actor, "Forest", Zone::Battlefield);
        }
        let original = paid(&mut s, actor, "Homunculus Horde", 4);
        let (_, copied) = electro(&mut s, actor, original);
        paid(&mut s, actor, "Doubling Season", 5);
        second_draw_horde(&mut s, actor, 2);
        let response = cast(&mut s, actor, "Giant Growth", &[Target::Object(copied)], 1);
        response_resolved_with_horde(&mut s, response);
        assert_eq!(engine::checked_effective_pt(&s, copied).unwrap(), (5, 5));
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(hordes(&s).len(), 6);
        let inherited = hordes(&s)
            .into_iter()
            .filter(|o| s.objects.get(*o).v4.copied_end_step_sacrifices_v1 != 0)
            .collect::<Vec<_>>();
        assert_eq!(inherited.len(), 3);
        for object in inherited {
            assert_eq!(s.objects.get(object).v4.copied_end_step_sacrifices_v1, 1);
            assert!(engine::has_effective_keyword(&s, object, Keywords::HASTE));
            assert_eq!(
                engine::checked_effective_pt(&s, object).unwrap(),
                if object == copied { (5, 5) } else { (2, 2) }
            );
        }
    }
}

/// Native entry-initialization primitive, not a registered or paid Harvester.
fn nightmare_copy_primitive(s: &mut GameState, p: PlayerId, name: &str) -> ObjectId {
    let before = s.objects.len();
    let mut proposal = event::ProposedEvent::create_token(id(name), p);
    if let event::ProposedEvent::CreateToken(token) = &mut proposal {
        token.initialization_v48 = Some(crate::doubling_season_v48::TokenEntryV48 {
            copied: true,
            copied_nightmare_v1: true,
            ..Default::default()
        });
    }
    event::propose_and_commit_batch(s, vec![proposal]);
    assert!(s.engine.halted.is_none(), "{:?}", s.engine.halted);
    let result = s
        .objects
        .iter()
        .find_map(|(o, live)| {
            (o.0 as usize >= before
                && live.zone == Zone::Battlefield
                && live.controller == p
                && live.v4.copied_nightmare_v1)
                .then_some(o)
        })
        .unwrap();
    harvester_v1::validate_state(s).unwrap();
    result
}

#[test]
fn native_nightmare_entry_primitive_paid_copy_of_copy_and_season_keep_copiable_type_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let original = nightmare_copy_primitive(&mut s, actor, "Homunculus Horde");
        paid(&mut s, actor, "Doubling Season", 5);
        let reflection = cast(
            &mut s,
            actor,
            "Self-Reflection",
            &[Target::Object(original)],
            6,
        );
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(s.objects.get(reflection).zone, Zone::Graveyard);
        assert_eq!(hordes(&s).len(), 3);
        let before = hordes(&s);
        cast(&mut s, actor, NAME, &[Target::Object(original)], 3);
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        let electro_copy = hordes(&s)
            .into_iter()
            .find(|o| !before.contains(o))
            .unwrap();
        assert_eq!(hordes(&s).len(), 5);
        let mut replay = restore(&s);
        second_draw_horde(&mut s, actor, 5);
        second_draw_horde(&mut replay, actor, 5);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(hordes(&s).len(), 15);
        for object in hordes(&s) {
            assert!(harvester_v1::copiable_nightmare(&s, object).unwrap());
            assert!(engine::has_effective_subtype(
                &s,
                object,
                card_def::Subtype::Nightmare
            ));
            assert!(engine::has_effective_subtype(
                &s,
                object,
                card_def::Subtype::Homunculus
            ));
        }
        assert_eq!(
            s.objects.get(electro_copy).v4.copied_end_step_sacrifices_v1,
            1
        );
        assert!(crate::optional_trigger_public_v1::validate_state(&s).is_err());
    }
}

#[test]
fn native_nightmare_entry_primitive_departed_horde_lki_and_component_tamper_refuse_readonly_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let original = nightmare_copy_primitive(&mut s, actor, "Homunculus Horde");
        second_draw_horde(&mut s, actor, 1);
        let row = horde_rows(&s).pop().unwrap();
        let mut bad = restore(&s);
        bad.objects.get_mut(original).v4.copied_nightmare_v1 = false;
        let before = bad.clone();
        assert!(harvester_v1::validate_state(&bad).is_err());
        assert_eq!(bad, before);
        assert!(engine::validated_stack_item_target_spec(&row, &bad).is_err());
        let response = cast(
            &mut s,
            actor.opponent(),
            "Unsummon",
            &[Target::Object(original)],
            1,
        );
        response_resolved_with_horde(&mut s, response);
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        let descendant = hordes(&s).pop().unwrap();
        assert!(harvester_v1::copiable_nightmare(&s, descendant).unwrap());
        assert_ne!(descendant, original);
        assert!(!s.objects.get(original).v4.copied_nightmare_v1);
        let receipt = s
            .engine
            .event_history
            .iter()
            .position(|e| matches!(e, CommittedEvent::NightmareCopyEnteredV1 { .. }))
            .unwrap();
        let mut bad = restore(&s);
        bad.engine.event_history.remove(receipt);
        let before = bad.clone();
        assert!(harvester_v1::validate_state(&bad).is_err());
        assert_eq!(bad, before);
    }
}
