use super::*;
use crate::{
    card_def::{self, CardCapability},
    effect::{self, EffectFrame, EffectOp},
    engine::{self, Action, Decision},
    event::{self, ProposedEvent},
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    state::{GameState, Step, Target, Zone},
};

fn ready(actor: PlayerId) -> GameState {
    let mut s = crate::extra_turn_v1::tests::ready(actor);
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut s).unwrap();
    for p in [actor, actor.opponent()] {
        for _ in 0..3 {
            put(&mut s, p, "Island", Zone::Battlefield);
        }
        for _ in 0..10 {
            put(&mut s, p, "Forest", Zone::Battlefield);
        }
    }
    s
}
fn quiet(s: &mut GameState, d: Decision) {
    let action = match d {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
        Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
        Decision::ChooseEntryColorV36 { colors, .. } => {
            assert_eq!(colors.len(), 5);
            Action::ChooseEffectOption(0)
        }
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        other => panic!("unexpected actual native choice:{other:?}"),
    };
    engine::step(s, action).unwrap();
    assert!(s.engine.halted.is_none());
}
fn settle(s: &mut GameState) {
    for _ in 0..80 {
        let d = engine::advance_until_decision(s);
        if s.stack.is_empty()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_cast.is_none()
            && s.engine.pending_triggers.is_empty()
            && !crate::linked_aura_operation_v1::has_pending(s)
        {
            assert!(matches!(d, Decision::CastSpellOrPass { .. }));
            return;
        }
        quiet(s, d);
    }
    panic!("real resolution failed to settle")
}
fn priority(s: &mut GameState, actor: PlayerId) {
    for _ in 0..8 {
        let d = engine::advance_until_decision(s);
        if matches!(d,Decision::CastSpellOrPass{player,..}if player==actor) {
            return;
        }
        quiet(s, d);
    }
    panic!("original caster lacks priority")
}
fn cast(s: &mut GameState, actor: PlayerId, spell: ObjectId, target: Option<ObjectId>) {
    priority(s, actor);
    let d = engine::advance_until_decision(s);
    assert!(
        matches!(d,Decision::CastSpellOrPass{castable_spells,..}if castable_spells.contains(&spell))
    );
    let before = s.players[actor.index()]
        .battlefield
        .iter()
        .filter(|id| s.objects.get(**id).tapped)
        .count();
    let mv = card_def::CARD_DEFS[s.objects.get(spell).card_def as usize].mana_value;
    engine::step(s, Action::CastSpell(spell)).unwrap();
    if let Some(target) = target {
        assert!(
            matches!(engine::advance_until_decision(s),Decision::ChooseTargets{legal_targets,..}if legal_targets.contains(&Target::Object(target)))
        );
        engine::step(s, Action::ChooseTarget(Target::Object(target))).unwrap();
    }
    // Native Step announces the cast/target; Advance finalizes the pending
    // cast, pays its actual quote and publishes the original spell before
    // returning priority to its caster. Assert payment only at that boundary.
    assert!(matches!(engine::advance_until_decision(s),
        Decision::CastSpellOrPass { player, .. } if player == actor));
    assert!(s.engine.pending_cast.is_none());
    assert!(s.engine.halted.is_none());
    let after = s.players[actor.index()]
        .battlefield
        .iter()
        .filter(|id| s.objects.get(**id).tapped)
        .count();
    assert_eq!(after - before, usize::from(mv));
    assert_eq!(s.players[actor.index()].mana_pool, [0; 6]);
    assert_eq!(s.objects.get(spell).zone, Zone::Stack);
    settle(s);
    assert_eq!(s.objects.get(spell).zone, Zone::Battlefield);
}
fn seek(s: &mut GameState, source: ObjectId, actor: PlayerId) -> Vec<Target> {
    for _ in 0..256 {
        let d = engine::advance_until_decision(s);
        if let Decision::ChooseTargets {
            spell: actual,
            legal_targets,
            ..
        } = d
        {
            assert_eq!(actual, source);
            assert_eq!(s.active_player, actor);
            assert_eq!(s.step, Step::Upkeep);
            return legal_targets;
        }
        if s.step == Step::Upkeep && s.active_player == actor.opponent() {
            assert!(!s.stack.iter().any(owns_item));
            assert!(!s.engine.pending_triggers.iter().any(|p| p.source == source));
        }
        quiet(s, d);
    }
    panic!("genuine own-upkeep placement absent")
}
fn build(actor: PlayerId, target_name: &str, season: bool) -> (GameState, ObjectId, ObjectId) {
    let mut s = ready(actor);
    let target = put(&mut s, actor, target_name, Zone::Hand);
    cast(&mut s, actor, target, None);
    if season {
        let enchantment = put(&mut s, actor, "Doubling Season", Zone::Hand);
        cast(&mut s, actor, enchantment, None);
    }
    // New source always follows the real paid target spell.
    // Replenish real untapped land objects for the six-mana source payment.
    for _ in 0..2 {
        put(&mut s, actor, "Island", Zone::Battlefield);
    }
    for _ in 0..4 {
        put(&mut s, actor, "Forest", Zone::Battlefield);
    }
    let source = put(&mut s, actor, "Extravagant Replication", Zone::Hand);
    cast(&mut s, actor, source, None);
    (s, source, target)
}
fn tokens(s: &GameState, actor: PlayerId, id: u16) -> Vec<ObjectId> {
    s.players[actor.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|id0| {
            let o = s.objects.get(*id0);
            o.v4.is_token && o.card_def == id
        })
        .collect()
}
fn publish(s: &mut GameState, source: ObjectId, target: ObjectId) {
    engine::step(s, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(s),
        Decision::CastSpellOrPass { .. }
    ));
    let item = s.stack.last().unwrap();
    assert!(owns_item(item));
    assert_eq!(item.source, source);
    assert_eq!(item.targets, vec![Target::Object(target)]);
    engine::validated_stack_item_target_spec(item, s).unwrap();
}
#[test]
fn exact_partial_printing_and_native_target_contract_remain_unadmitted() {
    let id = card_def::card_id_by_name("Extravagant Replication").unwrap();
    assert_eq!(id, 453);
    let d = &card_def::CARD_DEFS[id as usize];
    assert_eq!(d.capability, CardCapability::Full);
    assert!(d.has_full_support());
    assert!(d.is_castable());
    assert_eq!(d.types, [card_def::CardType::Enchantment]);
    assert_eq!(d.mana_value, 6);
    assert_eq!(d.cost.generic, 4);
    assert_eq!(
        d.cost.pips,
        &[crate::mana::Pip::Colored(crate::mana::ManaColor::U); 2]
    );
    card_def::preflight_fully_supported_deck(&[id]).unwrap();
    assert_eq!(
        crate::trigger::trigger_target_spec(id),
        card_def::TargetSpec::AnotherControlledNonlandPermanent
    );
    assert_eq!(crate::trigger::triggers_for(id).len(), 1);
    assert_eq!((crate::trigger::triggers_for(id)[0].effect)(), root());
}
#[test]
fn paid_own_upkeep_creature_and_noncreature_copy_prompt_publication_restore_and_season_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        for name in ["Llanowar Elves", "Ichor Wellspring", "Heraldic Banner"] {
            for season in [false, true] {
                let (mut s, source, target) = build(actor, name, season);
                let land = s.players[actor.index()]
                    .battlefield
                    .iter()
                    .copied()
                    .find(|id| s.objects.get(*id).name == "Forest")
                    .unwrap();
                let other = put(
                    &mut s,
                    actor.opponent(),
                    "Ichor Wellspring",
                    Zone::Battlefield,
                );
                let legal = seek(&mut s, source, actor);
                assert!(legal.contains(&Target::Object(target)));
                assert!(!legal.contains(&Target::Object(source)));
                assert!(!legal.contains(&Target::Object(land)));
                assert!(!legal.contains(&Target::Object(other)));
                let prompt = serde_json::to_vec(&s).unwrap();
                let mut output = None;
                for _ in 0..2 {
                    let mut current: GameState = serde_json::from_slice(&prompt).unwrap();
                    publish(&mut current, source, target);
                    let published = serde_json::to_vec(&current).unwrap();
                    let mut restored: GameState = serde_json::from_slice(&published).unwrap();
                    for world in [&mut current, &mut restored] {
                        settle(world);
                        let copies = tokens(world, actor, world.objects.get(target).card_def);
                        assert_eq!(copies.len(), if season { 2 } else { 1 });
                        for id in copies {
                            let token = world.objects.get(id);
                            assert_eq!((token.owner, token.controller), (actor, actor));
                            assert!(!token.tapped);
                            assert_eq!(token.damage, 0);
                            assert_eq!(token.counters.plus1_plus1, 0);
                        }
                    }
                    assert_eq!(current, restored);
                    if let Some(previous) = &output {
                        assert_eq!(&current, previous);
                    } else {
                        output = Some(current);
                    }
                }
            }
        }
    }
}
#[test]
fn target_control_departure_reentry_and_source_departure_or_control_preserve_original_trigger_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for case in 0..4 {
            let (mut s, source, target) = build(actor, "Llanowar Elves", false);
            seek(&mut s, source, actor);
            publish(&mut s, source, target);
            let before = s.objects.get(target).zone_change_count;
            match case {
                // Accepted native control primitive, not a claimed instant theft printing.
                0 => crate::standard_cards_v1::gain_control(&mut s, target, actor.opponent()),
                1 => {
                    event::propose_and_commit(
                        &mut s,
                        ProposedEvent::zone_change(target, Zone::Hand),
                    );
                    event::propose_and_commit(
                        &mut s,
                        ProposedEvent::zone_change(target, Zone::Battlefield),
                    );
                    assert_ne!(s.objects.get(target).zone_change_count, before);
                }
                2 => {
                    let wings = put(&mut s, actor.opponent(), "Broken Wings", Zone::Hand);
                    priority(&mut s, actor.opponent());
                    assert!(
                        matches!(engine::advance_until_decision(&mut s),Decision::CastSpellOrPass{castable_spells,..}if castable_spells.contains(&wings))
                    );
                    engine::step(&mut s, Action::CastSpell(wings)).unwrap();
                    assert!(
                        matches!(engine::advance_until_decision(&mut s),Decision::ChooseTargets{legal_targets,..}if legal_targets.contains(&Target::Object(source)))
                    );
                    engine::step(&mut s, Action::ChooseTarget(Target::Object(source))).unwrap();
                }
                _ => crate::standard_cards_v1::gain_control(&mut s, source, actor.opponent()),
            }
            let bytes = serde_json::to_vec(&s).unwrap();
            let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
            settle(&mut s);
            settle(&mut restored);
            assert_eq!(s, restored);
            assert_eq!(
                tokens(&s, actor, s.objects.get(target).card_def).len(),
                usize::from(case >= 2)
            );
            assert!(tokens(&s, actor.opponent(), s.objects.get(target).card_def).is_empty());
            if case == 2 {
                assert_eq!(s.objects.get(source).zone, Zone::Graveyard);
            }
        }
    }
}
#[test]
fn paid_aura_copy_chooses_untargeted_host_before_entry_and_restores_live_and_answered_stage_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for season in [false, true] {
            let mut s = ready(actor);
            let own = put(&mut s, actor, "Llanowar Elves", Zone::Battlefield);
            let other = put(
                &mut s,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let aura = put(&mut s, actor, "Witness Protection", Zone::Hand);
            cast(&mut s, actor, aura, Some(own));
            if season {
                let d = put(&mut s, actor, "Doubling Season", Zone::Hand);
                cast(&mut s, actor, d, None);
            }
            for _ in 0..6 {
                put(&mut s, actor, "Island", Zone::Battlefield);
            }
            let source = put(&mut s, actor, "Extravagant Replication", Zone::Hand);
            cast(&mut s, actor, source, None);
            seek(&mut s, source, actor);
            publish(&mut s, source, aura);
            for _ in 0..12 {
                let d = engine::advance_until_decision(&mut s);
                if matches!(d, Decision::ChooseEffectTargets { .. }) {
                    break;
                }
                quiet(&mut s, d);
            }
            assert!(
                matches!(engine::advance_until_decision(&mut s),Decision::ChooseEffectTargets{player,legal_targets,can_finish:false,..}if player==actor && legal_targets.contains(&Target::Object(other)))
            );
            assert!(tokens(&s, actor, s.objects.get(aura).card_def).is_empty());
            effect::validate_pending_effect_choice(&s).unwrap();
            let pending = serde_json::to_vec(&s).unwrap();
            let mut result = None;
            for replay in 0..2 {
                if replay != 0 {
                    s = serde_json::from_slice(&pending).unwrap();
                }
                effect::choose_resumable_target(&mut s, Target::Object(other)).unwrap();
                effect::validate_pending_effect_choice(&s).unwrap();
                let answered = serde_json::to_vec(&s).unwrap();
                let mut restored: GameState = serde_json::from_slice(&answered).unwrap();
                if season {
                    assert_eq!(
                        effect::resume_resumable_resolution(&mut s).unwrap(),
                        effect::ResumableProgress::Suspended
                    );
                    assert_eq!(
                        effect::resume_resumable_resolution(&mut restored).unwrap(),
                        effect::ResumableProgress::Suspended
                    );
                    assert_eq!(s, restored);
                    assert!(tokens(&s, actor, s.objects.get(aura).card_def).is_empty());
                    effect::validate_pending_effect_choice(&s).unwrap();
                    // Every actual expanded Aura chooses independently, before
                    // any entry; the second member deliberately uses another host.
                    let second_prompt = serde_json::to_vec(&s).unwrap();
                    restored = serde_json::from_slice(&second_prompt).unwrap();
                    effect::choose_resumable_target(&mut s, Target::Object(own)).unwrap();
                    effect::choose_resumable_target(&mut restored, Target::Object(own)).unwrap();
                    effect::validate_pending_effect_choice(&s).unwrap();
                    assert_eq!(s, restored);
                    restored = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
                }
                settle(&mut s);
                settle(&mut restored);
                assert_eq!(s, restored);
                crate::doubling_season_v48::validate_token_journals(&s).unwrap();
                let copies = tokens(&s, actor, s.objects.get(aura).card_def);
                assert_eq!(copies.len(), if season { 2 } else { 1 });
                let mut actual_hosts = vec![];
                for id in copies {
                    let host = s.objects.get(id).v4.attached_to.unwrap();
                    assert!(s.objects.get(host.object).attachments.contains(&id));
                    assert_eq!(
                        host.zone_change_count,
                        s.objects.get(host.object).zone_change_count
                    );
                    actual_hosts.push(host.object);
                }
                assert!(actual_hosts.contains(&other));
                if season {
                    assert!(actual_hosts.contains(&own));
                }
                if let Some(previous) = &result {
                    assert_eq!(&s, previous);
                } else {
                    result = Some(s.clone());
                }
            }
            for case in 0..7 {
                let mut bad: GameState = serde_json::from_slice(&pending).unwrap();
                let p = bad.engine.pending_effect.as_mut().unwrap();
                match case {
                    0 => {
                        p.choice = None;
                        p.frames.clear();
                    }
                    1 => {
                        p.choice = None;
                        p.frames = vec![EffectFrame::Program {
                            op: EffectOp::Sequence(vec![]),
                            path: vec![],
                        }];
                    }
                    2 => {
                        if let Some(effect::PendingEffectChoice::SelectTargets { legal, .. }) =
                            &mut p.choice
                        {
                            legal.push(effect::EffectTargetCandidate {
                                target: Target::Object(other),
                                expected_object: None,
                            });
                        }
                    }
                    3 => {
                        if let Some(effect::PendingEffectChoice::SelectTargets {
                            purpose:
                                effect::EffectTargetSelectionPurpose::AttachCopiedAuraV1 { choice },
                            ..
                        }) = &mut p.choice
                        {
                            choice.template.card_def =
                                card_def::card_id_by_name("Ichor Wellspring").unwrap();
                        }
                    }
                    4 => p.frames.push(EffectFrame::Program {
                        op: EffectOp::Sequence(vec![]),
                        path: vec![1],
                    }),
                    5 | 6 => {
                        if let Some(effect::PendingEffectChoice::SelectTargets {
                            purpose:
                                effect::EffectTargetSelectionPurpose::AttachCopiedAuraV1 { choice },
                            ..
                        }) = &mut p.choice
                        {
                            if case == 5 {
                                choice.replacement.seasons.clear();
                                choice.replacement.controller = actor.opponent();
                            } else {
                                choice.selected_hosts.resize(
                                    choice.replacement.replaced_token_count(1).unwrap(),
                                    choice.candidates[0],
                                );
                            }
                        }
                    }
                    _ => unreachable!(),
                }
                let before = serde_json::to_vec(&bad).unwrap();
                assert!(effect::resume_resumable_resolution(&mut bad).is_err());
                assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
            }
        }
    }
}
#[test]
fn face_down_copy_uses_neutral_values_in_independent_hidden_worlds_and_transformed_face_is_selected_before_entry(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut copies = vec![];
        for hidden in ["Llanowar Elves", "Tolarian Terror"] {
            let mut s = ready(actor);
            let target = put(&mut s, actor, hidden, Zone::Battlefield);
            // Registered face-down primitive; no new hidden-printing producer claim.
            s.objects.get_mut(target).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
                disguised: true,
                lookers: 1 << actor.index(),
                hidden_by: None,
                hidden_by_source: None,
            });
            let private = put(&mut s, actor.opponent(), hidden, Zone::Hand);
            assert_eq!(s.objects.get(private).zone, Zone::Hand);
            let source = put(&mut s, actor, "Extravagant Replication", Zone::Hand);
            cast(&mut s, actor, source, None);
            seek(&mut s, source, actor);
            publish(&mut s, source, target);
            settle(&mut s);
            let token = *s.players[actor.index()]
                .battlefield
                .iter()
                .find(|id| s.objects.get(**id).v4.is_token)
                .unwrap();
            let live = s.objects.get(token);
            assert_eq!(
                live.card_def,
                card_def::card_id_by_name(crate::self_reflection_v1::ANONYMOUS_BLUEPRINT).unwrap()
            );
            assert!(live.v4.face_down_v1.is_none());
            assert_eq!(engine::effective_power(&s, token), 2);
            copies.push((
                live.clone(),
                s.engine
                    .event_history
                    .iter()
                    .filter_map(|e| {
                        if let event::CommittedEvent::CreateToken {
                            token_def,
                            controller,
                            ..
                        } = e
                        {
                            Some((*token_def, *controller))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>(),
            ));
        }
        // Arena allocation is identical; only the concealed original differs.
        assert_eq!(copies[0], copies[1]);
        let (mut s, source, target) = build(actor, "Delver of Secrets", false);
        event::propose_and_commit(&mut s, ProposedEvent::transform_in_place(target, 1));
        seek(&mut s, source, actor);
        publish(&mut s, source, target);
        settle(&mut s);
        let token = tokens(&s, actor, s.objects.get(target).card_def)[0];
        assert_eq!(s.objects.get(token).v4.face_index, 1);
        assert_eq!(engine::effective_power(&s, token), 3);
        assert_eq!(s.objects.get(token).name, "Insectile Aberration");
    }
}

#[test]
fn bound_copy_rejects_forged_context_before_installing_continuation_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source, target) = build(actor, "Llanowar Elves", false);
        seek(&mut s, source, actor);
        publish(&mut s, source, target);
        let item = s.stack.last().unwrap().clone();
        let mut ctx = effect::ExecCtx::no_targets(source, actor);
        ctx.stack_item_id = Some(item.v4.stack_item_id);
        ctx.ability_source_contract = item.v4.ability_source_contract;
        ctx.targets = item.targets.clone();
        ctx.target_contracts = item.v4.target_contracts.clone();
        for case in 0..3 {
            let mut bad = ctx.clone();
            match case {
                0 => bad.controller = actor.opponent(),
                1 => bad.target_contracts.clear(),
                _ => bad.stack_item_id = None,
            };
            let before = serde_json::to_vec(&s).unwrap();
            assert!(
                effect::begin_resumable_resolution(&root(), &bad, item.clone(), &mut s).is_err()
            );
            assert_eq!(serde_json::to_vec(&s).unwrap(), before);
        }
    }
}

#[test]
fn mandatory_copy_has_no_target_or_publication_when_only_its_own_source_is_nonland_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let source = put(&mut s, actor, "Extravagant Replication", Zone::Hand);
        cast(&mut s, actor, source, None);
        let original = s.turn_key_v1();
        let mut reached = false;
        for _ in 0..256 {
            let d = engine::advance_until_decision(&mut s);
            assert!(!s.stack.iter().any(owns_item));
            assert!(!matches!(d,Decision::ChooseTargets{spell,..}if spell==source));
            if s.active_player == actor && s.turn_key_v1() != original && s.step == Step::Main1 {
                reached = true;
                break;
            }
            quiet(&mut s, d);
        }
        assert!(reached);
        assert!(!s.players[actor.index()].battlefield.iter().any(|id| s
            .objects
            .get(*id)
            .v4
            .is_token));
    }
}

#[test]
fn direct_original_source_reentry_operand_excludes_only_captured_incarnation_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut s, source, target) = build(actor, "Llanowar Elves", false);
        seek(&mut s, source, actor);
        publish(&mut s, source, target);
        let original = s.stack.last().unwrap().v4.ability_source_contract.unwrap();
        let spec = card_def::TargetSpec::AnotherControlledNonlandPermanent;
        assert!(!engine::effect_target_is_legal_from_ability_source(
            &s,
            original,
            actor,
            spec,
            &[Target::Object(source)],
            0
        ));
        // Direct registered zone primitives isolate the operand contract. This
        // does not claim a legal player priority window during target selection.
        event::propose_and_commit(&mut s, ProposedEvent::zone_change(source, Zone::Hand));
        assert!(!engine::effect_target_is_legal_from_ability_source(
            &s,
            original,
            actor,
            spec,
            &[Target::Object(source)],
            0
        ));
        event::propose_and_commit(
            &mut s,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        assert_ne!(
            original.zone_change_count,
            s.objects.get(source).zone_change_count
        );
        assert!(engine::effect_target_is_legal_from_ability_source(
            &s,
            original,
            actor,
            spec,
            &[Target::Object(source)],
            0
        ));
        let restored: GameState = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert!(engine::effect_target_is_legal_from_ability_source(
            &restored,
            original,
            actor,
            spec,
            &[Target::Object(source)],
            0
        ));
    }
}

#[test]
fn paid_aura_single_host_autoselects_all_members_and_no_host_creates_none_before_entry_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        for no_host in [false, true] {
            let mut s = ready(actor);
            let host = put(&mut s, actor, "Llanowar Elves", Zone::Battlefield);
            let aura = put(&mut s, actor, "Witness Protection", Zone::Hand);
            cast(&mut s, actor, aura, Some(host));
            let season = put(&mut s, actor, "Doubling Season", Zone::Hand);
            cast(&mut s, actor, season, None);
            for _ in 0..6 {
                put(&mut s, actor, "Island", Zone::Battlefield);
            }
            let source = put(&mut s, actor, "Extravagant Replication", Zone::Hand);
            cast(&mut s, actor, source, None);
            seek(&mut s, source, actor);
            publish(&mut s, source, aura);
            if no_host {
                // Isolate the no-legal-host copy-entry boundary before SBAs:
                // a real host departure detaches the still-present target Aura.
                // Ordinary native advancement would instead remove that Aura
                // first, making its target illegal and creating no token too.
                event::propose_and_commit(&mut s, ProposedEvent::zone_change(host, Zone::Hand));
                let item = s.stack.last().unwrap().clone();
                let mut ctx = effect::ExecCtx::no_targets(source, actor);
                ctx.stack_item_id = Some(item.v4.stack_item_id);
                ctx.ability_source_contract = item.v4.ability_source_contract;
                ctx.targets = item.targets.clone();
                ctx.target_contracts = item.v4.target_contracts.clone();
                let start = s.engine.event_history.len();
                assert!(matches!(
                    effect::begin_resumable_resolution(&root(), &ctx, item, &mut s).unwrap(),
                    effect::ResumableProgress::Complete(_)
                ));
                assert!(!s.engine.event_history[start..]
                    .iter()
                    .any(|e| matches!(e, event::CommittedEvent::CreateToken { .. })));
                assert!(tokens(&s, actor, s.objects.get(aura).card_def).is_empty());
            } else {
                settle(&mut s);
                let copies = tokens(&s, actor, s.objects.get(aura).card_def);
                assert_eq!(copies.len(), 2);
                for copy in copies {
                    assert_eq!(s.objects.get(copy).v4.attached_to.unwrap().object, host);
                }
                crate::doubling_season_v48::validate_token_journals(&s).unwrap();
            }
        }
    }
}
