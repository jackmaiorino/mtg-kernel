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
// Run Away Together is the registered printed bounce instant. Its second
// creature is explicit native fixture setup, not a paid Elf claim; the cast
// below pays the real 1U and selects both distinct-controller targets.
fn paid_bounce_response(s: &mut GameState, p: PlayerId, target: ObjectId) -> ObjectId {
    let companion = put(s, p, "Llanowar Elves", Zone::Battlefield);
    assert_ne!(target, companion);
    assert_ne!(
        s.objects.get(target).controller,
        s.objects.get(companion).controller
    );
    cast(
        s,
        p,
        "Run Away Together",
        &[Target::Object(target), Target::Object(companion)],
        2,
    )
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
        for object in [first, second, third] {
            assert_eq!(
                engine::effective_keywords(&s, object, Keywords::HASTE),
                Keywords::HASTE
            );
            assert_eq!(
                engine::effective_keywords(&restore(&s), object, Keywords::HASTE),
                Keywords::HASTE
            );
            assert_eq!(
                engine::effective_keywords(&s, object, Keywords::VIGILANCE),
                Keywords::NONE
            );
        }
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
        let response = paid_bounce_response(&mut s, actor.opponent(), copied);
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
        assert!(
            crate::optional_trigger_public_v1::validate_state_for_consumer(
                &s,
                false,
                crate::optional_trigger_public_v1::ConsumerV1::Legacy
            )
            .is_err()
        );
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
        let response = paid_bounce_response(&mut s, actor.opponent(), original);
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

fn finish_zombie_copy(s: &mut GameState, target: Option<ObjectId>) {
    let mut chosen = false;
    for _ in 0..96 {
        let d = engine::advance_until_decision(s);
        assert!(s.engine.halted.is_none(), "{:?}", s.engine.halted);
        if s.stack.is_empty()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_triggers.is_empty()
            && !crate::linked_aura_operation_v1::has_pending(s)
        {
            return;
        }
        if let Decision::ChooseTargets {
            spell,
            legal_targets,
            can_finish,
            ..
        } = d
        {
            assert_eq!(s.objects.get(spell).card_def, id("Soul-Shackled Zombie"));
            let action = if let Some(card) = target.filter(|_| !chosen) {
                assert!(legal_targets.contains(&Target::Object(card)));
                chosen = true;
                Action::ChooseTarget(Target::Object(card))
            } else {
                assert!(can_finish);
                Action::FinishEffectSelection
            };
            let mut replay = restore(s);
            engine::step(s, action.clone()).unwrap();
            engine::step(&mut replay, action).unwrap();
            assert_eq!(*s, replay);
        } else {
            quiet(s, d);
        }
    }
    panic!("Soul copy ETB did not finish")
}

#[test]
fn genuine_paid_soul_copy_and_nightmare_entry_primitive_keep_original_etb_creation_index_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        for _ in 0..8 {
            put(&mut s, actor, "Swamp", Zone::Battlefield);
        }
        let original = cast(&mut s, actor, "Soul-Shackled Zombie", &[], 3);
        finish_zombie_copy(&mut s, None);
        cast(&mut s, actor, NAME, &[Target::Object(original)], 3);
        finish_zombie_copy(&mut s, None);
        let copied = copies(&s, id("Soul-Shackled Zombie"));
        assert_eq!(copied.len(), 1);
        let elf = paid(&mut s, actor, "Llanowar Elves", 1);
        cast(
            &mut s,
            actor.opponent(),
            "Lightning Bolt",
            &[Target::Object(elf)],
            1,
        );
        finish(&mut s);
        assert_eq!(s.objects.get(elf).zone, Zone::Graveyard);
        let before = [s.players[0].life, s.players[1].life];
        let nightmare = nightmare_copy_primitive(&mut s, actor, "Soul-Shackled Zombie");
        finish_zombie_copy(&mut s, Some(elf));
        assert_eq!(s.objects.get(elf).zone, Zone::Exile);
        assert_eq!(s.players[actor.index()].life, before[actor.index()] + 2);
        assert_eq!(
            s.players[actor.opponent().index()].life,
            before[actor.opponent().index()] - 2
        );
        let replay = restore(&s);
        for token in [copied[0], nightmare] {
            let source = AbilitySourceContractV4::capture(&s, token);
            crate::soul_shackled_zombie_v1::validate_source(&s, source).unwrap();
            crate::soul_shackled_zombie_v1::validate_source(&replay, source).unwrap();
            let entry = s
                .engine
                .event_history
                .iter()
                .find_map(|e| match e {
                    CommittedEvent::SingleGraveyardEntryV1 {
                        source_contract,
                        entry_event_index,
                    } if *source_contract == source => Some(*entry_event_index),
                    _ => None,
                })
                .unwrap();
            assert!(
                matches!(s.engine.event_history[entry], CommittedEvent::CreateToken { object, .. } if object == token)
            );
        }
    }
}

// Harvester fixtures below use real registered FDN54, actual paid spells and
// native activation/payment/publication. Initial lands/library are setup.
fn hv_ready(p: PlayerId) -> GameState {
    let mut s = ready(p);
    for seat in [p, p.opponent()] {
        for _ in 0..12 {
            put(&mut s, seat, "Swamp", Zone::Battlefield);
        }
    }
    s
}
fn hv_main(s: &mut GameState, p: PlayerId) {
    for _ in 0..600 {
        let d = engine::advance_until_decision(s);
        assert!(s.engine.halted.is_none(), "{:?}", s.engine.halted);
        if s.active_player == p
            && s.step == Step::Main1
            && s.stack.is_empty()
            && s.players[p.index()]
                .battlefield
                .iter()
                .all(|id| !s.objects.get(*id).summoning_sick)
            && matches!(d, Decision::CastSpellOrPass {player,..} if player==p)
        {
            return;
        }
        quiet(s, d);
    }
    panic!("actual subsequent own main absent");
}
fn hv_death(s: &mut GameState, p: PlayerId) -> ObjectId {
    let elf = paid(s, p, "Llanowar Elves", 1);
    cast(s, p.opponent(), "Lightning Bolt", &[Target::Object(elf)], 1);
    finish(s);
    assert_eq!(s.objects.get(elf).zone, Zone::Graveyard);
    assert!(harvester_v1::eligible(s, elf));
    elf
}
fn hv_activate(s: &mut GameState, p: PlayerId, source: ObjectId, target: ObjectId) -> StackItem {
    priority(s, p);
    assert!(
        matches!(engine::advance_until_decision(s), Decision::CastSpellOrPass {activatable_abilities,..}
        if activatable_abilities.contains(&(source,0)))
    );
    let before = taps(s, p);
    engine::step(s, Action::ActivateAbility(source, 0)).unwrap();
    assert!(
        matches!(engine::advance_until_decision(s),Decision::ChooseTargets {spell,player,ref legal_targets,..}
        if spell==source && player==p && legal_targets.contains(&Target::Object(target)))
    );
    let mut replay = restore(s);
    engine::step(s, Action::ChooseTarget(Target::Object(target))).unwrap();
    engine::step(&mut replay, Action::ChooseTarget(Target::Object(target))).unwrap();
    let d = engine::advance_until_decision(s);
    assert_eq!(d, engine::advance_until_decision(&mut replay));
    assert_eq!(*s, replay);
    assert!(matches!(d,Decision::CastSpellOrPass{player,..} if player==p));
    assert_eq!(taps(s, p) - before, 1);
    assert!(s.objects.get(source).tapped);
    let item = s
        .stack
        .iter()
        .find(|row| row.source == source && row.kind == StackItemKind::ActivatedAbility)
        .unwrap()
        .clone();
    assert_eq!(item.controller, p);
    assert_eq!(item.targets, [Target::Object(target)]);
    assert_eq!(item.v4.activated_ability_index, Some(0));
    assert_eq!(item.inline_effect, Some(harvester_v1::marker()));
    item
}
fn hv_nightmares(s: &GameState, p: PlayerId) -> Vec<ObjectId> {
    s.players[p.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|id| {
            let o = s.objects.get(*id);
            o.v4.is_token && engine::has_effective_subtype(s, *id, card_def::Subtype::Nightmare)
        })
        .collect()
}
fn hv_finish_pair(s: &mut GameState) {
    let mut replay = restore(s);
    finish(s);
    finish(&mut replay);
    assert_eq!(*s, replay);
    harvester_v1::validate_state(s).unwrap();
}
#[test]
fn genuine_paid_harvester_printing_sickness_any_graveyard_and_original_target_restore_both_seats() {
    assert_eq!(id("Abyssal Harvester"), 479);
    assert!(harvester_v1::definition(479));
    assert!(card_def::preflight_fully_supported_deck(&[479]).is_err());
    for p in [PlayerId::P0, PlayerId::P1] {
        for victim_owner in [p, p.opponent()] {
            let mut s = hv_ready(p);
            let early = hv_death(&mut s, p);
            let h = paid(&mut s, p, "Abyssal Harvester", 3);
            assert!(harvester_v1::eligible(&s, early));
            assert!(s.objects.get(h).summoning_sick);
            assert!(
                matches!(engine::advance_until_decision(&mut s),Decision::CastSpellOrPass{activatable_abilities,..}
                if !activatable_abilities.contains(&(h,0)))
            );
            let before = s.clone();
            assert!(engine::step(&mut s, Action::ActivateAbility(h, 0)).is_err());
            assert_eq!(s, before);
            hv_main(&mut s, p);
            let elf = if victim_owner == p {
                hv_death(&mut s, p)
            } else {
                hv_main(&mut s, p.opponent());
                hv_death(&mut s, p.opponent())
            };
            let target_generation = s.objects.get(elf).zone_change_count;
            hv_activate(&mut s, p, h, elf);
            assert!(crate::rl::observe_v1(&s, p, 0).is_err());
            hv_finish_pair(&mut s);
            assert_eq!(s.objects.get(elf).zone, Zone::Exile);
            assert!(s.objects.get(elf).zone_change_count > target_generation);
            let tokens = hv_nightmares(&s, p);
            assert_eq!(tokens.len(), 1);
            let token = s.objects.get(tokens[0]);
            assert_eq!(
                (token.card_def, token.owner, token.controller),
                (id("Llanowar Elves"), p, p)
            );
            assert!(engine::has_effective_subtype(
                &s,
                tokens[0],
                card_def::Subtype::Elf
            ));
            assert!(harvester_v1::copiable_nightmare(&s, tokens[0]).unwrap());
            assert!(crate::drake_public_v53::capture(&s).is_err());
        }
    }
}
#[test]
fn genuine_paid_harvester_season_exempts_all_new_tokens_and_cleans_other_nightmare_copies_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = hv_ready(p);
        let h = paid(&mut s, p, "Abyssal Harvester", 3);
        hv_main(&mut s, p);
        let elf = hv_death(&mut s, p);
        hv_activate(&mut s, p, h, elf);
        hv_finish_pair(&mut s);
        let original = hv_nightmares(&s, p)[0];
        let (_, electro) = electro(&mut s, p, original);
        assert!(harvester_v1::copiable_nightmare(&s, electro).unwrap());
        cast(&mut s, p, "Self-Reflection", &[Target::Object(original)], 6);
        hv_finish_pair(&mut s);
        let old = hv_nightmares(&s, p);
        assert_eq!(old.len(), 3);
        // An ordinary token is not a Nightmare and must survive cleanup.
        let ordinary = nightmare_free_elf_primitive(&mut s, p);
        // A native non-token Nightmare setup must not be exiled by this token-only instruction.
        let nontoken = put(&mut s, p, "Dreadwing Scavenger", Zone::Battlefield);
        paid(&mut s, p, "Doubling Season", 5);
        hv_main(&mut s, p);
        // Electro's original token may already have sacrificed at an actual end
        // step. Cleanup must exile all remaining old Nightmare token copies.
        let old = old
            .into_iter()
            .filter(|id| s.objects.get(*id).zone == Zone::Battlefield)
            .collect::<Vec<_>>();
        let next = hv_death(&mut s, p);
        hv_activate(&mut s, p, h, next);
        hv_finish_pair(&mut s);
        let new = hv_nightmares(&s, p);
        assert_eq!(new.len(), 2);
        for id in old {
            assert!(!s.players[p.index()].battlefield.contains(&id));
        }
        assert_eq!(s.objects.get(ordinary).zone, Zone::Battlefield);
        assert_eq!(s.objects.get(nontoken).zone, Zone::Battlefield);
        for id in new {
            assert_eq!(s.objects.get(id).owner, p);
            assert!(harvester_v1::copiable_nightmare(&s, id).unwrap());
        }
    }
}
// Explicit native token-entry setup, not a paid token-producing spell.
fn nightmare_free_elf_primitive(s: &mut GameState, p: PlayerId) -> ObjectId {
    let old = s.objects.len();
    let mut event = event::ProposedEvent::create_token(id("Llanowar Elves"), p);
    if let event::ProposedEvent::CreateToken(t) = &mut event {
        t.initialization_v48 = Some(crate::doubling_season_v48::TokenEntryV48 {
            copied: true,
            ..Default::default()
        });
    }
    event::propose_and_commit_batch(s, vec![event]);
    assert!(s.engine.halted.is_none());
    s.objects
        .iter()
        .find_map(|(id, o)| (id.0 as usize >= old && o.zone == Zone::Battlefield).then_some(id))
        .unwrap()
}
#[test]
fn genuine_paid_harvester_source_departure_and_control_response_keep_captured_controller_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for control in [false, true] {
            let mut s = hv_ready(p);
            let h = paid(&mut s, p, "Abyssal Harvester", 3);
            hv_main(&mut s, p);
            let elf = hv_death(&mut s, p);
            let row = hv_activate(&mut s, p, h, elf);
            if control {
                // Genuine native control primitive, not a sorcery response claim.
                crate::standard_cards_v1::gain_control(&mut s, h, p.opponent());
            } else {
                paid_bounce_response(&mut s, p.opponent(), h);
            }
            hv_finish_pair(&mut s);
            assert_eq!(hv_nightmares(&s, p).len(), 1);
            assert!(hv_nightmares(&s, p.opponent()).is_empty());
            assert_eq!(row.v4.ability_source_contract.unwrap().controller, p);
            assert_eq!(s.objects.get(h).owner, p);
            assert_eq!(s.objects.get(elf).zone, Zone::Exile);
        }
    }
}

#[test]
fn genuine_paid_harvester_target_departure_primitive_fizzles_without_cleanup_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = hv_ready(p);
        let h = paid(&mut s, p, "Abyssal Harvester", 3);
        hv_main(&mut s, p);
        let old = hv_death(&mut s, p);
        hv_activate(&mut s, p, h, old);
        hv_finish_pair(&mut s);
        hv_main(&mut s, p);
        let target = hv_death(&mut s, p);
        let before = hv_nightmares(&s, p);
        hv_activate(&mut s, p, h, target);
        // Native zone-change primitive: there is no claim of a paid instant
        // graveyard-return spell in this catalog.
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::zone_change(target, Zone::Hand),
        );
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::zone_change(target, Zone::Graveyard),
        );
        assert!(harvester_v1::eligible(&s, target));
        hv_finish_pair(&mut s);
        assert_eq!(s.objects.get(target).zone, Zone::Graveyard);
        assert_eq!(hv_nightmares(&s, p), before);
    }
}
#[test]
fn genuine_paid_harvester_public_api_changed_root_kind_unknown_stamp_and_erased_context_refuse_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = hv_ready(p);
        let h = paid(&mut s, p, "Abyssal Harvester", 3);
        hv_main(&mut s, p);
        let target = hv_death(&mut s, p);
        let item = hv_activate(&mut s, p, h, target);
        let ctx = crystal_prevention_v1::context(&item);
        let draw = EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        };
        for mode in 0..4 {
            let mut bad = restore(&s);
            let mut row = item.clone();
            let mut context = ctx.clone();
            row.kind = StackItemKind::Spell;
            row.inline_effect = Some(draw.clone());
            context.ability_source_contract = None;
            context.stack_item_id = if mode % 2 == 0 {
                None
            } else {
                Some(crate::ids::StackItemId(u64::MAX))
            };
            if mode >= 2 {
                row.v4.ability_source_contract = None;
            }
            let before = bad.clone();
            assert!(begin_resumable_resolution(&draw, &context, row.clone(), &mut bad).is_err());
            assert_eq!(bad, before);
            bad.engine.pending_effect = Some(EffectContinuation {
                resolving_item: row,
                ctx: context.clone(),
                frames: vec![EffectFrame::Program {
                    op: draw.clone(),
                    path: vec![],
                }],
                choice: None,
                answered_choice_guard: None,
            });
            let before = bad.clone();
            assert!(resume_resumable_resolution(&mut bad).is_err());
            assert_eq!(bad, before);
            let mut bad = restore(&s);
            execute(&draw, &context, &mut bad);
            let mut expected = s.clone();
            expected.engine.halted =
                Some((engine::UnsupportedMechanic::InvalidEffectContinuation, h));
            assert_eq!(bad, expected);
        }
        hv_finish_pair(&mut s);
        assert_eq!(s.objects.get(target).zone, Zone::Exile);
    }
}
#[test]
fn genuine_paid_harvester_graveyard_receipt_turn_generation_and_origin_tampering_refuse_readonly_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = hv_ready(p);
        let _h = paid(&mut s, p, "Abyssal Harvester", 3);
        hv_main(&mut s, p);
        let target = hv_death(&mut s, p);
        let first=s.engine.event_history.iter().position(|e|matches!(e,CommittedEvent::HarvesterGraveyardEntryV1{entry} if entry.card.object==target)).unwrap();
        assert!(harvester_v1::eligible(&s, target));
        hv_main(&mut s, p.opponent());
        assert!(!harvester_v1::eligible(&s, target));
        for mode in 0..4 {
            let mut bad = restore(&s);
            let current_turn = bad.turn_key_v1();
            let current_player = bad.active_player;
            if let CommittedEvent::HarvesterGraveyardEntryV1 { entry } =
                &mut bad.engine.event_history[first]
            {
                match mode {
                    0 => {
                        entry.turn = current_turn;
                        entry.active_player = current_player;
                    }
                    1 => entry.card.expected_zone_change_count += 1,
                    2 => entry.card.expected_zone_change_count -= 1,
                    _ => {
                        entry.origin =
                            harvester_v1::GraveyardOriginV1::ZoneChange { history_index: 0 }
                    }
                }
            }
            let before = bad.clone();
            assert!(harvester_v1::validate_state(&bad).is_err());
            assert_eq!(bad, before);
            assert!(!harvester_v1::eligible(&bad, target));
        }
        // A real later put is eligible again and its origin cannot be relabelled
        // to the older move of the same physical object.
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::zone_change(target, Zone::Hand),
        );
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::zone_change(target, Zone::Graveyard),
        );
        assert!(harvester_v1::eligible(&s, target));
        harvester_v1::validate_state(&s).unwrap();
        let mut bad = restore(&s);
        let old = match bad.engine.event_history[first] {
            CommittedEvent::HarvesterGraveyardEntryV1 { entry } => entry.origin,
            _ => unreachable!(),
        };
        let last = bad
            .engine
            .event_history
            .iter_mut()
            .rev()
            .find_map(|e| {
                if let CommittedEvent::HarvesterGraveyardEntryV1 { entry } = e {
                    (entry.card.object == target).then_some(entry)
                } else {
                    None
                }
            })
            .unwrap();
        last.origin = old;
        assert!(harvester_v1::validate_state(&bad).is_err());
    }
}

fn hv_finish_mill(s: &mut GameState) {
    for _ in 0..160 {
        let d = engine::advance_until_decision(s);
        assert!(s.engine.halted.is_none(), "{:?}", s.engine.halted);
        if s.stack.is_empty()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_triggers.is_empty()
        {
            return;
        }
        let action = match d {
            Decision::ChooseTargets {
                player,
                ref legal_targets,
                ..
            } => {
                let target = Target::Player(player.opponent());
                assert!(legal_targets.contains(&target));
                Some(Action::ChooseTarget(target))
            }
            Decision::ChooseEffectTargets {
                ref legal_targets,
                can_finish: false,
                ..
            } => Some(Action::ChooseEffectTarget(legal_targets[0])),
            _ => None,
        };
        if let Some(action) = action {
            let mut replay = restore(s);
            engine::step(s, action.clone()).unwrap();
            engine::step(&mut replay, action).unwrap();
            assert_eq!(*s, replay);
        } else {
            quiet(s, d);
        }
    }
    panic!("actual Tutelage chain did not complete");
}
#[test]
fn genuine_paid_harvester_discard_and_mill_from_anywhere_are_eligible_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        // Libraries are declared at construction. Tutelage genuinely mills the
        // creature and land, so this does not confuse 'put there' with 'died'.
        let own = vec![id("Forest"); 40];
        let mut other = own.clone();
        other[0] = id("Llanowar Elves");
        let libraries = if p == PlayerId::P0 {
            [&own[..], &other[..]]
        } else {
            [&other[..], &own[..]]
        };
        let mut s = GameState::new_from_libraries_with_starting_player_v1(
            libraries[0],
            libraries[1],
            crate::rl::card_name,
            110_479,
            p,
        );
        s.step = Step::Main1;
        crate::combat_damage_v1::enable_foundations_combat_v1(&mut s).unwrap();
        for seat in [p, p.opponent()] {
            for name in ["Forest", "Island", "Mountain", "Swamp"] {
                for _ in 0..12 {
                    put(&mut s, seat, name, Zone::Battlefield);
                }
            }
        }
        let h = paid(&mut s, p, "Abyssal Harvester", 3);
        let (_, fast) = electro(&mut s, p, h);
        paid(&mut s, p, "Sphinx's Tutelage", 3);
        cast(&mut s, p, "Think Twice", &[], 2);
        hv_finish_mill(&mut s);
        let milled = s.players[p.opponent().index()]
            .graveyard
            .iter()
            .copied()
            .find(|object| s.objects.get(*object).card_def == id("Llanowar Elves"))
            .unwrap();
        assert!(harvester_v1::eligible(&s, milled));
        hv_activate(&mut s, p, fast, milled);
        hv_finish_pair(&mut s);
        assert_eq!(s.objects.get(milled).zone, Zone::Exile);
        let mut s = hv_ready(p);
        let h = paid(&mut s, p, "Abyssal Harvester", 3);
        hv_main(&mut s, p);
        let discard = put(&mut s, p, "Llanowar Elves", Zone::Hand);
        priority(&mut s, p);
        let thrill = put(&mut s, p, "Thrill of Possibility", Zone::Hand);
        let before = taps(&s, p);
        engine::step(&mut s, Action::CastSpell(thrill)).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut s),Decision::Discard{player,count:1,ref choices} if player==p && choices.contains(&discard))
        );
        let mut replay = restore(&s);
        engine::step(&mut s, Action::Discard(vec![discard])).unwrap();
        engine::step(&mut replay, Action::Discard(vec![discard])).unwrap();
        assert_eq!(
            engine::advance_until_decision(&mut s),
            engine::advance_until_decision(&mut replay)
        );
        assert_eq!(s, replay);
        assert_eq!(taps(&s, p) - before, 2);
        assert!(s.stack.last().unwrap().discarded.contains(&discard));
        hv_finish_pair(&mut s);
        assert!(harvester_v1::eligible(&s, discard));
        hv_activate(&mut s, p, h, discard);
        hv_finish_pair(&mut s);
        assert_eq!(s.objects.get(discard).zone, Zone::Exile);
    }
}
#[test]
fn genuine_paid_harvester_copied_activation_haste_and_printed_ability_suppression_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = hv_ready(p);
        let h = paid(&mut s, p, "Abyssal Harvester", 3);
        let (_, copy) = electro(&mut s, p, h);
        assert!(engine::has_effective_keyword(&s, copy, Keywords::HASTE));
        assert!(s.objects.get(copy).summoning_sick);
        let elf = hv_death(&mut s, p);
        hv_activate(&mut s, p, copy, elf);
        hv_finish_pair(&mut s);
        assert_eq!(s.objects.get(elf).zone, Zone::Exile);
        hv_main(&mut s, p);
        let target = hv_death(&mut s, p);
        let aura = cast(&mut s, p, "Witness Protection", &[Target::Object(h)], 1);
        hv_finish_pair(&mut s);
        assert!(
            matches!(engine::advance_until_decision(&mut s),Decision::CastSpellOrPass{activatable_abilities,..} if !activatable_abilities.contains(&(h,0)))
        );
        cast(
            &mut s,
            p.opponent(),
            "Broken Wings",
            &[Target::Object(aura)],
            3,
        );
        hv_finish_pair(&mut s);
        hv_activate(&mut s, p, h, target);
        hv_finish_pair(&mut s);
        assert_eq!(s.objects.get(target).zone, Zone::Exile);
    }
}

#[test]
fn genuine_paid_condemn_then_draw_and_flashback_keep_physical_generation_history_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let own = [id("Forest"); 2];
        let other = [id("Forest"); 40];
        let libraries = if p == PlayerId::P0 {
            [&own[..], &other[..]]
        } else {
            [&other[..], &own[..]]
        };
        let mut s = GameState::new_from_libraries_with_starting_player_v1(
            libraries[0],
            libraries[1],
            crate::rl::card_name,
            110_479,
            p,
        );
        s.step = Step::Main1;
        crate::combat_damage_v1::enable_foundations_combat_v1(&mut s).unwrap();
        for seat in [p, p.opponent()] {
            for name in ["Forest", "Island", "Mountain", "Swamp", "Plains"] {
                for _ in 0..8 {
                    put(&mut s, seat, name, Zone::Battlefield);
                }
            }
        }
        paid(&mut s, p, "Abyssal Harvester", 3);
        let elf = paid(&mut s, p, "Llanowar Elves", 1);
        hv_main(&mut s, p);
        for _ in 0..64 {
            let d = engine::advance_until_decision(&mut s);
            if let Decision::DeclareAttackers { player, eligible } = d {
                assert_eq!(player, p);
                assert!(eligible.contains(&elf));
                engine::step(&mut s, Action::DeclareAttackers(vec![elf])).unwrap();
                break;
            }
            quiet(&mut s, d);
        }
        assert!(s.engine.combat.attackers.contains(&elf));
        cast(&mut s, p.opponent(), "Condemn", &[Target::Object(elf)], 1);
        hv_finish_pair(&mut s);
        assert_eq!(s.objects.get(elf).zone, Zone::Library);
        let mut replay = restore(&s);
        for _ in 0..2 {
            cast(&mut s, p, "Think Twice", &[], 2);
            finish(&mut s);
            cast(&mut replay, p, "Think Twice", &[], 2);
            finish(&mut replay);
            assert_eq!(s, replay);
            harvester_v1::validate_state(&s).unwrap();
        }
        assert_eq!(s.objects.get(elf).zone, Zone::Hand);
        // Noncreature cards create no Harvester entry, and their real printed
        // flashback announcement therefore cannot corrupt a retained receipt.
        let twice = s.players[p.index()]
            .graveyard
            .iter()
            .copied()
            .find(|object| s.objects.get(*object).card_def == id("Think Twice"))
            .unwrap();
        assert!(!s.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::HarvesterGraveyardEntryV1{entry} if entry.card.object==twice)));
        // A third draw needs a card, so this distinct flashback scene uses the
        // original declared full library and a genuinely paid physical spell.
        let mut s = hv_ready(p);
        let twice = cast(&mut s, p, "Think Twice", &[], 2);
        hv_finish_pair(&mut s);
        cast_card(&mut s, p, twice, &[], 3);
        assert!(s.stack.last().unwrap().is_flashback);
        hv_finish_pair(&mut s);
        assert_eq!(s.objects.get(twice).zone, Zone::Exile);
    }
}

#[test]
fn genuine_paid_harvester_target_roster_and_public_rows_ignore_unrevealed_opponent_hand_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut worlds = Vec::new();
        for secret in ["Forest", "Consuming Aberration"] {
            let mut s = hv_ready(p);
            let hidden = put(&mut s, p.opponent(), secret, Zone::Hand);
            let h = paid(&mut s, p, "Abyssal Harvester", 3);
            hv_main(&mut s, p);
            let target = hv_death(&mut s, p);
            assert!(s.players[p.opponent().index()].hand.contains(&hidden));
            priority(&mut s, p);
            let roster = engine::legal_targets_for(harvester_v1::target_spec(), &[], &s);
            assert_eq!(roster, [Target::Object(target)]);
            let row = hv_activate(&mut s, p, h, target);
            let serialized = serde_json::to_string(&row).unwrap();
            assert!(!serialized.contains(secret));
            worlds.push((roster, row));
        }
        assert_eq!(worlds[0], worlds[1]);
    }
}

#[test]
fn genuine_paid_nightmare_native_aegis_layer_save_restore_and_paid_copy_values_both_seats() {
    use crate::state::{AbilitySourceContractV4, ObjectLinkV4};
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = hv_ready(p);
        let harvester = paid(&mut s, p, "Abyssal Harvester", 3);
        hv_main(&mut s, p);
        let elf = hv_death(&mut s, p);
        hv_activate(&mut s, p, harvester, elf);
        hv_finish_pair(&mut s);
        let original = hv_nightmares(&s, p)[0];
        let host = ObjectLinkV4 {
            object: original,
            zone_change_count: s.objects.get(original).zone_change_count,
        };
        assert!(harvester_v1::copiable_nightmare(&s, original).unwrap());
        assert_eq!(s.objects.get(original).card_def, id("Llanowar Elves"));
        assert_eq!(s.objects.get(original).owner, p);
        assert_eq!(s.objects.get(elf).zone, Zone::Exile);

        let attachment = paid(&mut s, p, "Viridian Longbow", 1);
        let attachment_generation = s.objects.get(attachment).zone_change_count;
        // Accepted native Aegis copy-value primitive with registered physical
        // stand-ins. Longbow is genuinely paid, but has no printed Aegis ability.
        // Donor exile/link and attachment are labelled setup primitives. The
        // Nightmare component itself came only from the paid Harvester above.
        let donor = put(&mut s, p.opponent(), "Axgard Cavalry", Zone::Hand);
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::zone_change(donor, Zone::Exile),
        );
        s.attach_object_exact(
            attachment,
            attachment_generation,
            host.object,
            host.zone_change_count,
        )
        .unwrap();
        let source = AbilitySourceContractV4::capture(&s, attachment);
        s.objects.get_mut(donor).v4.exiled_by = Some(ObjectLinkV4 {
            object: attachment,
            zone_change_count: attachment_generation,
        });
        let mut ctx = ExecCtx::no_targets(attachment, p);
        ctx.ability_source_contract = Some(source);
        let timestamp = s.objects.get(attachment).v4.layer_timestamp.unwrap();
        let binding = EffectObjectBinding {
            object: donor,
            expected_zone: Zone::Exile,
            expected_zone_change_count: s.objects.get(donor).zone_change_count,
        };
        assert_eq!(
            crate::standard_cards_v1::aegis_copy_candidates(&s, &ctx, host, timestamp),
            [binding]
        );
        let mut stale = binding;
        stale.expected_zone_change_count += 1;
        let before = s.clone();
        crate::standard_cards_v1::apply_aegis_copy(&mut s, &ctx, host, timestamp, stale);
        assert_eq!(s, before);
        crate::standard_cards_v1::apply_aegis_copy(
            &mut s,
            &ctx,
            ObjectLinkV4 {
                zone_change_count: host.zone_change_count + 1,
                ..host
            },
            timestamp,
            binding,
        );
        assert_eq!(s, before);

        let history_start = s.engine.event_history.len();
        let mut replay = restore(&s);
        for branch in [&mut s, &mut replay] {
            crate::standard_cards_v1::apply_aegis_copy(branch, &ctx, host, timestamp, binding);
            let live = branch.objects.get(original);
            assert_eq!(
                (live.zone, live.zone_change_count),
                (Zone::Battlefield, host.zone_change_count)
            );
            assert_eq!((live.owner, live.controller), (p, p));
            assert!(live.v4.is_token);
            assert_eq!(live.card_def, id("Axgard Cavalry"));
            assert!(!harvester_v1::copiable_nightmare(branch, original).unwrap());
            assert!(!engine::has_effective_subtype(
                branch,
                original,
                card_def::Subtype::Nightmare
            ));
            assert!(crate::standard_cards_v1::had_card_def(
                branch,
                original,
                host.zone_change_count,
                id("Llanowar Elves")
            ));
            assert!(crate::standard_cards_v1::had_card_def(
                branch,
                original,
                host.zone_change_count,
                id("Axgard Cavalry")
            ));
            assert_eq!(branch.objects.get(donor).owner, p.opponent());
            assert_eq!(branch.objects.get(donor).zone, Zone::Exile);
            harvester_v1::validate_state(branch).unwrap();
        }
        assert_eq!(s, replay);
        restore(&s);

        // A real paid copy while the layer is active sees Cavalry, without the
        // saved Nightmare exception. This is not a hand-authored token blueprint.
        let before_copy = s.players[p.index()].battlefield.clone();
        let layered_copy_spell = cast(&mut s, p, "Self-Reflection", &[Target::Object(original)], 6);
        hv_finish_pair(&mut s);
        let plain = *s.players[p.index()]
            .battlefield
            .iter()
            .find(|object| {
                let live = s.objects.get(**object);
                !before_copy.contains(object)
                    && live.v4.is_token
                    && live.card_def == id("Axgard Cavalry")
            })
            .unwrap();
        assert!(!harvester_v1::copiable_nightmare(&s, plain).unwrap());
        assert_eq!(
            (s.objects.get(plain).owner, s.objects.get(plain).controller),
            (p, p)
        );

        let mut replay = restore(&s);
        for branch in [&mut s, &mut replay] {
            // Native source departure invalidates the actual attachment and
            // restores the saved copiable values on the same host incarnation.
            event::propose_and_commit(
                branch,
                event::ProposedEvent::zone_change(attachment, Zone::Hand),
            );
            let live = branch.objects.get(original);
            assert_eq!(
                (live.zone, live.zone_change_count),
                (Zone::Battlefield, host.zone_change_count)
            );
            assert_eq!((live.owner, live.controller), (p, p));
            assert_eq!(live.card_def, id("Llanowar Elves"));
            assert!(live.v4.is_token);
            assert!(harvester_v1::copiable_nightmare(branch, original).unwrap());
            assert!(engine::has_effective_subtype(
                branch,
                original,
                card_def::Subtype::Nightmare
            ));
            assert!(!harvester_v1::copiable_nightmare(branch, plain).unwrap());
            branch.validate_attachment_relations().unwrap();
            harvester_v1::validate_state(branch).unwrap();
        }
        assert_eq!(s, replay);
        assert_eq!(s.state_hash(), replay.state_hash());
        let frozen_copy = s
            .engine
            .event_history
            .iter()
            .find_map(|event| match event {
                CommittedEvent::ThousandYearStormV1 {
                    journal: thousand_year_storm_v1::JournalV1::Cast { spell, .. },
                } if spell.source == layered_copy_spell => Some(spell.as_ref().clone()),
                _ => None,
            })
            .unwrap();
        // The actual copy spell targeted the Cavalry layer. Restoring the
        // same host to its saved Elf values cannot rewrite that frozen target.
        assert!(matches!(frozen_copy.v4.target_contracts[0],
            crate::state::StackTargetContractV4::Object { card_def, .. }
            if card_def == id("Axgard Cavalry")));
        engine::validate_copy_snapshot(&s, &frozen_copy).unwrap();
        for forged in 0..3 {
            let mut bad = frozen_copy.clone();
            let crate::state::StackTargetContractV4::Object {
                card_def,
                owner,
                zone_change_count,
                ..
            } = &mut bad.v4.target_contracts[0]
            else {
                unreachable!()
            };
            match forged {
                0 => *card_def = id("Swamp"),
                1 => *owner = p.opponent(),
                _ => *zone_change_count = host.zone_change_count + 1,
            }
            let before = s.clone();
            assert!(engine::validate_copy_snapshot(&s, &bad).is_err());
            assert_eq!(s, before);
        }
        let layers: Vec<_> = s
            .engine
            .event_history
            .iter()
            .enumerate()
            .skip(history_start)
            .filter_map(|(at, event)| {
                if let CommittedEvent::NightmareCopyLayerV1 { layer } = event {
                    assert_eq!(layer.source.source, original);
                    assert_eq!(layer.source.zone_change_count, host.zone_change_count);
                    assert_eq!((layer.source.owner, layer.source.controller), (p, p));
                    Some((at, layer.nightmare, layer.source.card_def))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            layers
                .iter()
                .map(|(_, value, def)| (*value, *def))
                .collect::<Vec<_>>(),
            [(false, id("Axgard Cavalry")), (true, id("Llanowar Elves"))]
        );
        let mut forged = restore(&s);
        if let CommittedEvent::NightmareCopyLayerV1 { layer } =
            &mut forged.engine.event_history[layers[0].0]
        {
            layer.nightmare = true;
        }
        let before = forged.clone();
        assert!(harvester_v1::validate_state(&forged).is_err());
        assert_eq!(forged, before);

        let before_copy = hv_nightmares(&s, p);
        cast(&mut s, p, "Self-Reflection", &[Target::Object(original)], 6);
        hv_finish_pair(&mut s);
        let copied = *hv_nightmares(&s, p)
            .iter()
            .find(|object| !before_copy.contains(object))
            .unwrap();
        assert_eq!(s.objects.get(copied).card_def, id("Llanowar Elves"));
        assert!(harvester_v1::copiable_nightmare(&s, copied).unwrap());
        assert_eq!(
            (
                s.objects.get(copied).owner,
                s.objects.get(copied).controller
            ),
            (p, p)
        );
        assert_eq!(s.objects.get(elf).zone, Zone::Exile);
        restore(&s);
    }
}
