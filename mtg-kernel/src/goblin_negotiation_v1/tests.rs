use crate::{
    card_def::{self, CardCapability, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS},
    engine::{self, Action, Decision},
    event::{self, CommittedEvent, ProposedEvent},
    ids::{ObjectId, PlayerId},
    mana::{ManaColor, Pip},
    policy_observation_v6::tests::put,
    state::{GameState, Target, Zone},
};
fn ready(actor: PlayerId) -> GameState {
    let mut s = crate::extra_turn_v1::tests::ready(actor);
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut s).unwrap();
    for _ in 0..10 {
        put(&mut s, actor, "Mountain", Zone::Battlefield);
    }
    s
}
fn next(s: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(s);
    assert!(s.engine.halted.is_none(), "{d:?}");
    d
}
fn quiet(s: &mut GameState, d: Decision) {
    let a = match d {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        other => panic!("unexpected real resolution choice: {other:?}"),
    };
    engine::step(s, a).unwrap();
}
fn settle(s: &mut GameState) {
    for _ in 0..80 {
        let d = next(s);
        if s.stack.is_empty()
            && s.engine.pending_cast.is_none()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_triggers.is_empty()
        {
            assert!(matches!(d, Decision::CastSpellOrPass { .. }));
            return;
        }
        quiet(s, d);
    }
    panic!("actual resolution did not settle");
}
fn restore(s: &GameState) -> GameState {
    let copy: GameState = serde_json::from_slice(&serde_json::to_vec(s).unwrap()).unwrap();
    assert_eq!(s.state_hash(), copy.state_hash());
    copy
}
fn goblins(s: &GameState, actor: PlayerId) -> Vec<ObjectId> {
    s.players[actor.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|id| CARD_DEFS[usize::from(s.objects.get(*id).card_def)].name == "Goblin Token")
        .collect()
}
fn damage(s: &GameState, source: ObjectId, target: ObjectId) -> i32 {
    s.engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::Damage {
                source: from,
                target: Target::Object(to),
                amount,
            } if *from == source && *to == target => Some(*amount),
            _ => None,
        })
        .sum()
}
fn tapped(s: &GameState, actor: PlayerId) -> usize {
    s.players[actor.index()]
        .battlefield
        .iter()
        .filter(|id| s.objects.get(**id).tapped)
        .count()
}
fn announce(s: &mut GameState, actor: PlayerId, target: ObjectId, x: u16) -> ObjectId {
    let spell = put(s, actor, "Goblin Negotiation", Zone::Hand);
    announce_source(s, actor, spell, target, x);
    spell
}
fn announce_source(s: &mut GameState, actor: PlayerId, spell: ObjectId, target: ObjectId, x: u16) {
    assert!(
        matches!(next(s), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&spell))
    );
    let before = tapped(s, actor);
    engine::step(s, Action::CastSpell(spell)).unwrap();
    assert!(
        matches!(next(s), Decision::ChooseTargets { legal_targets, .. }
        if legal_targets.contains(&Target::Object(target))
            && !legal_targets.contains(&Target::Player(actor.opponent())))
    );
    engine::step(s, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(
        matches!(next(s), Decision::ChooseEffectOption { option_count, .. } if option_count > x)
    );
    engine::step(s, Action::ChooseEffectOption(x)).unwrap();
    assert!(matches!(next(s), Decision::CastSpellOrPass { player, .. } if player == actor));
    assert!(s.engine.pending_cast.is_none());
    assert_eq!(s.objects.get(spell).zone, Zone::Stack);
    assert_eq!(tapped(s, actor) - before, usize::from(x) + 2);
    assert_eq!(s.stack.last().unwrap().v4.x_value, x);
}
#[test]
fn exact_partial_printing_reuses_existing_goblin_dependency() {
    let id = card_def::card_id_by_name("Goblin Negotiation").unwrap();
    assert_eq!(id, 456);
    let d = &CARD_DEFS[usize::from(id)];
    assert_eq!(d.capability, CardCapability::Partial);
    assert_eq!(d.types, [CardType::Sorcery]);
    assert_eq!(d.colors, [ManaColor::R]);
    assert_eq!((d.mana_value, d.cost.generic, d.cost.x_count), (2, 0, 1));
    assert_eq!(d.cost.pips, [Pip::Colored(ManaColor::R); 2]);
    assert_eq!(d.target_spec, TargetSpec::Creature);
    assert!(card_def::preflight_fully_supported_deck(&[id]).is_err());
    let token = &CARD_DEFS[usize::from(card_def::card_id_by_name("Goblin Token").unwrap())];
    assert!(token.is_token && token.has_full_support());
    assert_eq!(token.subtypes, [Subtype::Goblin]);
    assert_eq!(token.colors, [ManaColor::R]);
    assert_eq!((token.power, token.toughness), (Some(1), Some(1)));
    assert_eq!(token.keywords, Keywords::default());
}
#[test]
fn paid_x_zero_under_lethal_exact_lethal_and_excess_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (x, expected) in [(0, 0), (1, 0), (2, 0), (5, 3)] {
            let mut s = ready(actor);
            let target = put(
                &mut s,
                actor.opponent(),
                "Axgard Cavalry",
                Zone::Battlefield,
            );
            let spell = announce(&mut s, actor, target, x);
            settle(&mut s);
            assert_eq!(damage(&s, spell, target), i32::from(x));
            assert_eq!(goblins(&s, actor).len(), expected);
            assert!(goblins(&s, actor.opponent()).is_empty());
            assert_eq!(
                s.objects.get(target).zone,
                if x < 2 {
                    Zone::Battlefield
                } else {
                    Zone::Graveyard
                }
            );
            assert_eq!(s.objects.get(spell).zone, Zone::Graveyard);
            for goblin in goblins(&s, actor) {
                assert_eq!(s.objects.get(goblin).owner, actor);
                assert_eq!(s.objects.get(goblin).controller, actor);
                assert!(s.objects.get(goblin).summoning_sick);
            }
        }
    }
}
#[test]
fn genuine_damage_already_marked_reduces_lethal_threshold() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let target = put(
            &mut s,
            actor.opponent(),
            "Axgard Cavalry",
            Zone::Battlefield,
        );
        let first = put(&mut s, actor, "End the Festivities", Zone::Hand);
        next(&mut s);
        engine::step(&mut s, Action::CastSpell(first)).unwrap();
        settle(&mut s);
        assert_eq!(s.objects.get(target).damage, 1);
        let spell = announce(&mut s, actor, target, 4);
        settle(&mut s);
        assert_eq!(damage(&s, spell, target), 4);
        assert_eq!(goblins(&s, actor).len(), 3);
    }
}
#[test]
fn paid_cast_target_illegal_or_new_incarnation_fizzles_all_instructions() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for reenter in [false, true] {
            let mut s = ready(actor);
            let target = put(
                &mut s,
                actor.opponent(),
                "Axgard Cavalry",
                Zone::Battlefield,
            );
            let spell = announce(&mut s, actor, target, 5);
            let old = s.objects.get(target).zone_change_count;
            // Primitive intervening zone changes exercise genuine cast target contracts.
            event::propose_and_commit(&mut s, ProposedEvent::zone_change(target, Zone::Hand));
            if reenter {
                event::propose_and_commit(
                    &mut s,
                    ProposedEvent::zone_change(target, Zone::Battlefield),
                );
            }
            assert!(s.objects.get(target).zone_change_count > old);
            settle(&mut s);
            assert_eq!(damage(&s, spell, target), 0);
            assert!(goblins(&s, actor).is_empty());
            assert_eq!(s.objects.get(target).damage, 0);
        }
    }
}
#[test]
fn season_doubles_actual_excess_batch_for_its_controller_only() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for own_season in [false, true] {
            let mut s = ready(actor);
            let holder = if own_season { actor } else { actor.opponent() };
            put(&mut s, holder, "Doubling Season", Zone::Battlefield);
            let target = put(&mut s, actor, "Axgard Cavalry", Zone::Battlefield);
            let spell = announce(&mut s, actor, target, 5);
            settle(&mut s);
            assert_eq!(damage(&s, spell, target), 5);
            assert_eq!(goblins(&s, actor).len(), if own_season { 6 } else { 3 });
            assert!(goblins(&s, actor.opponent()).is_empty());
        }
    }
}
#[test]
fn damage_doubling_changes_excess_but_never_the_chosen_x() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        put(&mut s, actor, "Twinflame Tyrant", Zone::Battlefield);
        let target = put(
            &mut s,
            actor.opponent(),
            "Axgard Cavalry",
            Zone::Battlefield,
        );
        let spell = announce(&mut s, actor, target, 3);
        settle(&mut s);
        assert_eq!(damage(&s, spell, target), 6);
        assert_eq!(goblins(&s, actor).len(), 4);
    }
}
#[test]
fn real_crystal_prevention_means_no_damage_and_no_excess_tokens() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        put(
            &mut s,
            actor.opponent(),
            "Crystal Barricade",
            Zone::Battlefield,
        );
        let target = put(
            &mut s,
            actor.opponent(),
            "Axgard Cavalry",
            Zone::Battlefield,
        );
        let spell = announce(&mut s, actor, target, 5);
        settle(&mut s);
        assert_eq!(damage(&s, spell, target), 0);
        assert_eq!(s.objects.get(target).zone, Zone::Battlefield);
        assert!(goblins(&s, actor).is_empty());
    }
}
#[test]
fn insufficient_red_and_unpayable_x_refuse_without_mutation() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = crate::extra_turn_v1::tests::ready(actor);
        put(&mut s, actor, "Mountain", Zone::Battlefield);
        for _ in 0..8 {
            put(&mut s, actor, "Island", Zone::Battlefield);
        }
        put(
            &mut s,
            actor.opponent(),
            "Axgard Cavalry",
            Zone::Battlefield,
        );
        let spell = put(&mut s, actor, "Goblin Negotiation", Zone::Hand);
        assert!(
            matches!(next(&mut s), Decision::CastSpellOrPass { castable_spells, .. } if !castable_spells.contains(&spell))
        );
        let before = s.state_hash();
        assert!(engine::step(&mut s, Action::CastSpell(spell)).is_err());
        assert_eq!(s.state_hash(), before);
        put(&mut s, actor, "Mountain", Zone::Battlefield);
        engine::step(&mut s, Action::CastSpell(spell)).unwrap();
        let Decision::ChooseTargets { legal_targets, .. } = next(&mut s) else {
            panic!("creature target");
        };
        engine::step(&mut s, Action::ChooseTarget(legal_targets[0])).unwrap();
        assert!(matches!(
            next(&mut s),
            Decision::ChooseEffectOption {
                option_count: 9,
                ..
            }
        ));
        let before = s.state_hash();
        assert!(engine::step(&mut s, Action::ChooseEffectOption(9)).is_err());
        assert_eq!(s.state_hash(), before);
    }
}
#[test]
fn native_pending_x_and_finalized_stack_round_trip_and_resolve_identically() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let target = put(
            &mut s,
            actor.opponent(),
            "Axgard Cavalry",
            Zone::Battlefield,
        );
        let spell = put(&mut s, actor, "Goblin Negotiation", Zone::Hand);
        next(&mut s);
        engine::step(&mut s, Action::CastSpell(spell)).unwrap();
        assert!(matches!(next(&mut s), Decision::ChooseTargets { .. }));
        engine::step(&mut s, Action::ChooseTarget(Target::Object(target))).unwrap();
        assert!(matches!(next(&mut s), Decision::ChooseEffectOption { .. }));
        let mut pending = restore(&s);
        for branch in [&mut s, &mut pending] {
            engine::step(branch, Action::ChooseEffectOption(5)).unwrap();
            next(branch);
            assert_eq!(tapped(branch, actor), 7);
        }
        assert_eq!(s.state_hash(), pending.state_hash());
        let mut stacked = restore(&s);
        for branch in [&mut s, &mut pending, &mut stacked] {
            settle(branch);
        }
        assert_eq!(s.state_hash(), pending.state_hash());
        assert_eq!(s.state_hash(), stacked.state_hash());
        assert_eq!(goblins(&s, actor).len(), 3);
    }
}

#[test]
fn paid_toughness_increase_changes_excess_at_resolution() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        put(&mut s, actor, "Forest", Zone::Battlefield);
        let target = put(
            &mut s,
            actor.opponent(),
            "Axgard Cavalry",
            Zone::Battlefield,
        );
        let growth = put(&mut s, actor, "Giant Growth", Zone::Hand);
        next(&mut s);
        engine::step(&mut s, Action::CastSpell(growth)).unwrap();
        assert!(matches!(next(&mut s), Decision::ChooseTargets { .. }));
        engine::step(&mut s, Action::ChooseTarget(Target::Object(target))).unwrap();
        settle(&mut s);
        assert_eq!(tapped(&s, actor), 1);
        assert_eq!(engine::effective_toughness(&s, target), 5);
        let spell = announce(&mut s, actor, target, 6);
        settle(&mut s);
        assert_eq!(damage(&s, spell, target), 6);
        assert_eq!(goblins(&s, actor).len(), 1);
    }
}

#[test]
fn paid_stack_season_count_overflow_refuses_before_damage_or_removal_both_seats_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        // Primitive board setup uses 64 actual registered Season permanents.
        // This triggers the existing usize count overflow without allocating tokens.
        for _ in 0..64 {
            put(&mut s, actor, "Doubling Season", Zone::Battlefield);
        }
        let target = put(
            &mut s,
            actor.opponent(),
            "Axgard Cavalry",
            Zone::Battlefield,
        );
        let spell = announce(&mut s, actor, target, 3);
        assert_eq!(tapped(&s, actor), 5);
        engine::step(&mut s, Action::Pass).unwrap();
        assert!(
            matches!(next(&mut s), Decision::CastSpellOrPass { player, .. } if player == actor.opponent())
        );
        engine::step(&mut s, Action::Pass).unwrap();
        assert_eq!(s.engine.priority_passes, [true, true]);
        let mut restored = restore(&s);
        let before = serde_json::to_value(&s).unwrap();
        let hash = s.state_hash();
        for branch in [&mut s, &mut restored] {
            assert!(
                matches!(engine::advance_until_decision(branch), Decision::Halted { source, .. } if source == spell)
            );
            assert_eq!(branch.objects.get(target).damage, 0);
            assert_eq!(damage(branch, spell, target), 0);
            assert_eq!(branch.objects.get(spell).zone, Zone::Stack);
            assert_eq!(branch.stack.len(), 1);
            assert_eq!(branch.stack[0].source, spell);
            assert!(goblins(branch, actor).is_empty());
            // Only the diagnostic halt may differ. Compare the complete world,
            // including stack, paid mana, replacements and every event journal.
            let mut unchanged = branch.clone();
            unchanged.engine.halted = None;
            assert_eq!(serde_json::to_value(&unchanged).unwrap(), before);
            assert_eq!(unchanged.state_hash(), hash);
        }
        assert_eq!(s.state_hash(), restored.state_hash());
    }
}

fn shield_primitive(s: &mut GameState, source: ObjectId, target: ObjectId, remaining: i32) {
    // No current FDN card grants this numeric shield. This is the existing
    // replacement primitive around a genuine paid Goblin Negotiation cast.
    let id = s.engine.next_replacement_id;
    s.engine.next_replacement_id += 1;
    s.engine.active_replacements.push(event::ActiveReplacement {
        id,
        source,
        kind: event::ReplacementEffectKind::PreventNextDamage {
            target: Target::Object(target),
            remaining,
        },
    });
}
fn wait_ordering(s: &mut GameState, affected: PlayerId) {
    for _ in 0..12 {
        let d = next(s);
        if let Decision::ChooseEffectOption { player, .. } = d {
            assert!(crate::effect::crystal_prevention_v1::is_choice(s));
            assert_eq!(player, affected);
            return;
        }
        quiet(s, d);
    }
    panic!("actual affected-player ordering did not appear");
}
fn choice_slot(s: &GameState, kind: &str) -> u16 {
    use crate::effect::{
        crystal_prevention_v1::PreventionV1, EffectOptionChoicePurpose, PendingEffectChoice,
    };
    let Some(PendingEffectChoice::ChooseOption {
        purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
        ..
    }) = s
        .engine
        .pending_effect
        .as_ref()
        .and_then(|p| p.choice.as_ref())
    else {
        panic!("actual replacement menu");
    };
    choice
        .options
        .iter()
        .position(|option| match (kind, option) {
            ("double", PreventionV1::Multiplier(_))
            | ("numeric", PreventionV1::Replacement(_))
            | ("crystal", PreventionV1::Barricade(_)) => true,
            _ => false,
        })
        .unwrap() as u16
}
fn ordered_finish_with_restores(s: &mut GameState, first: &str) {
    let mut first = Some(first);
    loop {
        let mut restored = restore(s);
        let slot = first.take().map_or(0, |kind| choice_slot(s, kind));
        for branch in [&mut *s, &mut restored] {
            let facts = super::public_ordering_stage(branch).unwrap().unwrap();
            assert_eq!(facts.original_x, 3);
            assert_eq!(facts.lethal_damage, 2);
            engine::step(branch, Action::ChooseEffectOption(slot)).unwrap();
        }
        assert_eq!(s.state_hash(), restored.state_hash());
        if s.engine.pending_effect.as_ref().unwrap().choice.is_none() {
            // The final answer has committed its actual damage, while the
            // retained frame has not created any tokens or moved the spell.
            let mut before_completion = restore(s);
            assert!(goblins(s, s.active_player).is_empty());
            for branch in [&mut *s, &mut restored, &mut before_completion] {
                settle(branch);
            }
            assert_eq!(s.state_hash(), restored.state_hash());
            assert_eq!(s.state_hash(), before_completion.state_hash());
            return;
        }
        assert!(matches!(next(s), Decision::ChooseEffectOption { .. }));
    }
}
#[test]
fn actual_numeric_prevention_and_doubling_orders_have_different_excess_both_seats_restore_every_choice(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (first, dealt, tokens) in [("double", 5, 6), ("numeric", 4, 4)] {
            let mut s = ready(actor);
            let tyrant = put(&mut s, actor, "Twinflame Tyrant", Zone::Battlefield);
            put(&mut s, actor, "Doubling Season", Zone::Battlefield);
            let target = put(
                &mut s,
                actor.opponent(),
                "Axgard Cavalry",
                Zone::Battlefield,
            );
            shield_primitive(&mut s, tyrant, target, 1);
            let spell = announce(&mut s, actor, target, 3);
            wait_ordering(&mut s, actor.opponent());
            ordered_finish_with_restores(&mut s, first);
            assert_eq!(damage(&s, spell, target), dealt);
            assert_eq!(goblins(&s, actor).len(), tokens);
            let completed = super::public_completed(&s).unwrap();
            assert_eq!(completed.len(), 1);
            assert_eq!(completed[0].actual_damage, dealt);
            assert_eq!(completed[0].excess, (dealt - 2) as u32);
            assert_eq!(completed[0].tokens_created, tokens as u64);
            assert_eq!(s.objects.get(spell).zone, Zone::Graveyard);
        }
    }
}
#[test]
fn actual_crystal_prevention_selected_first_completes_once_without_damage_or_tokens() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let tyrant = put(&mut s, actor, "Twinflame Tyrant", Zone::Battlefield);
        put(
            &mut s,
            actor.opponent(),
            "Crystal Barricade",
            Zone::Battlefield,
        );
        let target = put(
            &mut s,
            actor.opponent(),
            "Axgard Cavalry",
            Zone::Battlefield,
        );
        shield_primitive(&mut s, tyrant, target, 1);
        let spell = announce(&mut s, actor, target, 3);
        wait_ordering(&mut s, actor.opponent());
        ordered_finish_with_restores(&mut s, "crystal");
        assert_eq!(damage(&s, spell, target), 0);
        assert!(goblins(&s, actor).is_empty());
        assert_eq!(s.objects.get(target).zone, Zone::Battlefield);
        assert_eq!(super::public_completed(&s).unwrap()[0].tokens_created, 0);
    }
}
#[test]
fn genuine_foreign_owned_exile_permission_cast_keeps_captured_controller_for_excess_tokens() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let tyrant = put(&mut s, actor, "Twinflame Tyrant", Zone::Battlefield);
        let target = put(
            &mut s,
            actor.opponent(),
            "Axgard Cavalry",
            Zone::Battlefield,
        );
        let spell = put(&mut s, actor.opponent(), "Goblin Negotiation", Zone::Hand);
        event::propose_and_commit(&mut s, ProposedEvent::zone_change(spell, Zone::Exile));
        // Existing play-permission primitive, followed by the genuine paid cast.
        s.engine
            .exile_play_permissions
            .push(engine::PlayPermission {
                without_mana_cost: Default::default(),
                object: spell,
                holder: actor,
                zone_change_generation: s.objects.get(spell).zone_change_count,
                play_or_cast: engine::PlayOrCast::Cast,
                expiry: engine::PlayPermissionExpiry::EndOfTurn,
            });
        shield_primitive(&mut s, tyrant, target, 1);
        announce_source(&mut s, actor, spell, target, 3);
        wait_ordering(&mut s, actor.opponent());
        ordered_finish_with_restores(&mut s, "numeric");
        assert_eq!(damage(&s, spell, target), 4);
        assert_eq!(goblins(&s, actor).len(), 2);
        assert!(goblins(&s, actor.opponent()).is_empty());
        assert_eq!(s.objects.get(spell).owner, actor.opponent());
        assert!(s.players[actor.opponent().index()]
            .graveyard
            .contains(&spell));
    }
}
#[test]
fn native_pending_ordering_refuses_stale_source_target_reentry_and_changed_actor() {
    use crate::effect::{EffectOptionChoicePurpose, PendingEffectChoice};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let tyrant = put(&mut s, actor, "Twinflame Tyrant", Zone::Battlefield);
        let target = put(
            &mut s,
            actor.opponent(),
            "Axgard Cavalry",
            Zone::Battlefield,
        );
        shield_primitive(&mut s, tyrant, target, 1);
        let spell = announce(&mut s, actor, target, 3);
        wait_ordering(&mut s, actor.opponent());
        for changed in 0..3 {
            let mut stale = restore(&s);
            if changed < 2 {
                let object = if changed == 0 { spell } else { target };
                // Raw zone-commit primitive invalidates a saved continuation;
                // this is an incarnation guard test, not priority during resolution.
                event::commit(&mut stale, ProposedEvent::zone_change(object, Zone::Exile));
                event::commit(
                    &mut stale,
                    ProposedEvent::zone_change(
                        object,
                        if changed == 0 {
                            Zone::Hand
                        } else {
                            Zone::Battlefield
                        },
                    ),
                );
            } else {
                let Some(PendingEffectChoice::ChooseOption {
                    player,
                    purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                    ..
                }) = stale
                    .engine
                    .pending_effect
                    .as_mut()
                    .unwrap()
                    .choice
                    .as_mut()
                else {
                    unreachable!()
                };
                *player = actor;
                choice.chooser = actor;
            }
            let before = serde_json::to_vec(&stale).unwrap();
            assert!(engine::step(&mut stale, Action::ChooseEffectOption(0)).is_err());
            assert_eq!(serde_json::to_vec(&stale).unwrap(), before);
            assert!(goblins(&stale, actor).is_empty());
            assert_eq!(damage(&stale, spell, target), 0);
        }
    }
}
