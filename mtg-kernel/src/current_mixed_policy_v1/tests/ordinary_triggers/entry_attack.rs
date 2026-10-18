//! Actual printed casts and attacks through the four existing sealed owners.
//! Basic lands, the opposing vanilla blocker and forty-Forest libraries are
//! explicit setup operands. The attacker and sacrificed Elf are genuinely paid.
use super::*;
use crate::state::Step;

fn quiet<O: Owner>(s: &mut O) {
    let actions = s.actions();
    let next = actions
        .iter()
        .find(|a| quiet_action(a))
        .or_else(|| (actions.len() == 1).then(|| &actions[0]))
        .unwrap_or_else(|| panic!("unexpected real turn choice:{actions:?}"))
        .clone();
    step(s, |a| a == &next);
}

pub(super) fn tap<O: Owner>(s: &mut O, land: ObjectId) {
    for _ in 0..8 {
        if s.actions().iter().any(|a| matches!(a, ActionSemanticV1::ActivateManaAbility { source, .. } if source.arena_id == land.0)) {
            step(s, |a| matches!(a, ActionSemanticV1::ActivateManaAbility { source, .. } if source.arena_id == land.0));
            assert!(s.world().objects.get(land).tapped);
            return;
        }
        pass(s);
    }
    panic!("real mana ability absent")
}

pub(super) fn pay_cast<O: Owner>(s: &mut O, actor: PlayerId, source: ObjectId, lands: &[ObjectId]) {
    assert_eq!(s.world().players[actor.index()].mana_pool, [0; 6]);
    for land in lands {
        tap(s, *land);
    }
    cast(s, source);
    assert_eq!(s.world().players[actor.index()].mana_pool, [0; 6]);
    assert!(lands.iter().all(|land| s.world().objects.get(*land).tapped));
    assert_eq!(s.world().objects.get(source).zone, Zone::Stack);
    assert!(s.world().stack.iter().any(|row| row.source == source
        && row.controller == actor
        && row.kind == crate::state::StackItemKind::Spell));
}

pub(super) fn finish_entry<O: Owner>(s: &mut O, source: ObjectId) {
    let paid = s.snapshot();
    drain(s);
    assert_eq!(s.world().objects.get(source).zone, Zone::Battlefield);
    if crate::engine::object_has_type(s.world(), source, crate::card_def::CardType::Creature) {
        assert!(s.world().objects.get(source).summoning_sick);
    }
    let after = (gameplay(s), O::packet(&s.encode()).clone());
    s.restore(&paid).unwrap();
    drain(s);
    assert_eq!((gameplay(s), O::packet(&s.encode()).clone()), after);
}

pub(super) fn choice_guards<O: Owner>(s: &mut O, index: usize) {
    let issued = s.encode();
    let before = gameplay(s);
    let mut erased = issued.clone();
    O::erase(&mut erased);
    assert!(s.apply(&erased, index as u32).is_err());
    assert_eq!(gameplay(s), before);
    let mut foreign = O::own(s.world().clone());
    let foreign_before = gameplay(&foreign);
    assert!(foreign.apply(&issued, index as u32).is_err());
    assert_eq!(gameplay(&foreign), foreign_before);
    let snapshot = s.snapshot();
    s.restore(&snapshot).unwrap();
    let before = gameplay(s);
    assert!(matches!(
        s.apply(&issued, index as u32),
        Err(Error::StaleBinding)
    ));
    assert_eq!(gameplay(s), before);
    assert_eq!(O::packet(&s.encode()), O::packet(&issued));
}

fn assert_printing<O: Owner>(s: &O) {
    assert_eq!(
        fact_count(
            O::packet(&s.encode()).state(),
            "registered_permanent_printing"
        ),
        1
    );
}

fn raid<O: Owner>(actor: PlayerId) {
    let mut raw = crate::extra_turn_v1::tests::ready(actor);
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut raw).unwrap();
    let source = put(&mut raw, actor, "Strongbox Raider", Zone::Hand);
    let elf = put(&mut raw, actor, "Llanowar Elves", Zone::Hand);
    let forest = put(&mut raw, actor, "Forest", Zone::Battlefield);
    let mountains: Vec<_> = (0..4)
        .map(|_| put(&mut raw, actor, "Mountain", Zone::Battlefield))
        .collect();
    let mut s = O::own(raw);
    assert_printing(&s); // Family coverage cannot omit the own-hand printing.
    pay_cast(&mut s, actor, elf, &[forest]);
    finish_entry(&mut s, elf);
    let mut attacked = false;
    for _ in 0..512 {
        if attacked
            && s.world().active_player == actor
            && s.world().priority_player == actor
            && s.world().step == Step::Main2
            && s.world().stack.is_empty()
        {
            break;
        }
        let attack = s.actions().iter().find(|a| attack_choice(a, elf)).cloned();
        if !attacked {
            if let Some(attack) = attack {
                assert!(!s.world().objects.get(elf).summoning_sick);
                step(&mut s, |a| a == &attack);
                finish_attack_declaration(&mut s, elf);
                attacked = true;
                continue;
            }
        }
        quiet(&mut s);
    }
    assert!(attacked && s.world().step == Step::Main2 && s.world().active_player == actor);
    assert_eq!(
        s.world().attack_declaration_turn_v1,
        Some(crate::state::AttackDeclarationTurnV1 {
            turn: s.world().turn_key_v1(),
            active_player: actor
        })
    );
    let library_before = s.world().players[actor.index()].library.len();
    pay_cast(&mut s, actor, source, &mountains);
    let paid = s.snapshot();
    for replay in 0..2 {
        if replay != 0 {
            s.restore(&paid).unwrap();
        }
        for _ in 0..32 {
            if s.actions().iter().any(|a| matches!(a, ActionSemanticV1::ChooseEffectTarget { source: host, min_targets: 1, max_targets: 1, .. } if host.arena_id == source.0)) { break; }
            pass(&mut s);
        }
        let options: Vec<_> = s.actions().into_iter().filter(|a| matches!(a,
            ActionSemanticV1::ChooseEffectTarget { source: host, target: TargetRefV1::Object { .. }, selected_count: 0, min_targets: 1, max_targets: 1, .. }
            if host.arena_id == source.0)).collect();
        assert_eq!(options.len(), 2);
        assert_eq!(s.world().objects.get(source).zone, Zone::Battlefield);
        assert_eq!(
            s.world().players[actor.index()].library.len(),
            library_before - 2
        );
        assert_printing(&s);
        assert!(
            fact_count(
                O::packet(&s.encode()).state(),
                "choose_exactly_one_surviving_exile"
            ) > 0
        );
        let ActionSemanticV1::ChooseEffectTarget {
            target: TargetRefV1::Object { object },
            ..
        } = &options[1]
        else {
            unreachable!()
        };
        let chosen = ObjectId(object.arena_id);
        let unchosen = match &options[0] {
            ActionSemanticV1::ChooseEffectTarget {
                target: TargetRefV1::Object { object },
                ..
            } => ObjectId(object.arena_id),
            _ => unreachable!(),
        };
        assert_eq!(s.world().objects.get(chosen).zone, Zone::Exile);
        let index = s.actions().iter().position(|a| a == &options[1]).unwrap();
        choice_guards(&mut s, index);
        let offer = s.snapshot();
        step(&mut s, |a| a == &options[1]);
        drain(&mut s);
        let permission = s
            .world()
            .engine
            .exile_play_permissions
            .iter()
            .find(|p| p.object == chosen)
            .unwrap();
        assert_eq!(permission.holder, actor);
        assert_eq!(
            permission.zone_change_generation,
            s.world().objects.get(chosen).zone_change_count
        );
        assert_eq!(s.world().engine.exile_play_permissions.len(), 1);
        let granted = (gameplay(&s), O::packet(&s.encode()).clone());
        s.restore(&offer).unwrap();
        step(&mut s, |a| a == &options[1]);
        drain(&mut s);
        assert_eq!((gameplay(&s), O::packet(&s.encode()).clone()), granted);
        // Actual ordinary land timing/quota and exact selected exile permission.
        for _ in 0..8 {
            if s.actions().iter().any(|a| matches!(a, ActionSemanticV1::PlayLand { source, .. } if source.arena_id == chosen.0)) { break; }
            pass(&mut s);
        }
        assert!(!s.actions().iter().any(|a| matches!(a, ActionSemanticV1::PlayLand { source, .. } if source.arena_id == unchosen.0)));
        let before_play = s.snapshot();
        step(
            &mut s,
            |a| matches!(a, ActionSemanticV1::PlayLand { source, .. } if source.arena_id == chosen.0),
        );
        assert_eq!(s.world().objects.get(chosen).zone, Zone::Battlefield);
        assert_eq!(s.world().objects.get(unchosen).zone, Zone::Exile);
        let played = (gameplay(&s), O::packet(&s.encode()).clone());
        s.restore(&before_play).unwrap();
        step(
            &mut s,
            |a| matches!(a, ActionSemanticV1::PlayLand { source, .. } if source.arena_id == chosen.0),
        );
        assert_eq!((gameplay(&s), O::packet(&s.encode()).clone()), played);
    }
}

fn sacrifice<O: Owner>(actor: PlayerId) {
    let mut raw = crate::extra_turn_v1::tests::ready(actor);
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut raw).unwrap();
    let source = put(&mut raw, actor, "Vampire Gourmand", Zone::Hand);
    let elf = put(&mut raw, actor, "Llanowar Elves", Zone::Hand);
    let blocker = put(
        &mut raw,
        actor.opponent(),
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let forest = put(&mut raw, actor, "Forest", Zone::Battlefield);
    let generic = put(&mut raw, actor, "Forest", Zone::Battlefield);
    let swamp = put(&mut raw, actor, "Swamp", Zone::Battlefield);
    let mut s = O::own(raw);
    assert_printing(&s);
    pay_cast(&mut s, actor, elf, &[forest]);
    finish_entry(&mut s, elf);
    pay_cast(&mut s, actor, source, &[generic, swamp]);
    finish_entry(&mut s, source);
    let generation = s.world().objects.get(source).zone_change_count;
    let mut attacked = false;
    for _ in 0..512 {
        if s.actions().iter().any(|a| matches!(a, ActionSemanticV1::ChooseEffectTarget { source: host, min_targets: 0, max_targets: 1, .. } if host.arena_id == source.0)) { break; }
        let attack = s
            .actions()
            .iter()
            .find(|a| attack_choice(a, source))
            .cloned();
        if let Some(attack) = attack {
            assert!(!s.world().objects.get(source).summoning_sick);
            step(&mut s, |a| a == &attack);
            finish_attack_declaration(&mut s, source);
            // Gourmand, unlike the vanilla Raid Elf, has a genuine self-attack
            // trigger and therefore an exact native declaration event.
            assert!(s.world().engine.event_history.iter().any(|event| matches!(event,
                crate::event::CommittedEvent::DeclaredAttacker { source: host, source_zone_change_count, controller }
                if *host == source && *source_zone_change_count == generation && *controller == actor)));
            attacked = true;
        } else {
            quiet(&mut s);
        }
    }
    assert!(attacked);
    assert_printing(&s);
    assert!(fact_count(O::packet(&s.encode()).state(), "sacrifice_is_not_targeting") > 0);
    assert!(!s
        .actions()
        .iter()
        .any(|a| matches!(a, ActionSemanticV1::ChooseTarget { .. })));
    let pay_action = s.actions().iter().find(|a| matches!(a, ActionSemanticV1::ChooseEffectTarget {
        target: TargetRefV1::Object { object }, min_targets: 0, max_targets: 1, .. } if object.arena_id == elf.0)).unwrap().clone();
    let decline = s
        .actions()
        .iter()
        .find(|a| {
            matches!(
                a,
                ActionSemanticV1::FinishEffectSelection {
                    selected_count: 0,
                    ..
                }
            )
        })
        .unwrap()
        .clone();
    let index = s.actions().iter().position(|a| a == &pay_action).unwrap();
    choice_guards(&mut s, index);
    let prompt = s.snapshot();
    let hand = s.world().players[actor.index()].hand.len();
    for pay in [false, true] {
        let mut prior = None;
        for _ in 0..2 {
            s.restore(&prompt).unwrap();
            step(&mut s, |a| a == if pay { &pay_action } else { &decline });
            drain(&mut s);
            assert_eq!(
                s.world().objects.get(elf).zone,
                if pay {
                    Zone::Graveyard
                } else {
                    Zone::Battlefield
                }
            );
            assert_eq!(
                s.world().players[actor.index()].hand.len(),
                hand + usize::from(pay)
            );
            assert_eq!(
                crate::vampire_gourmand_v1::cant_be_blocked(s.world(), source),
                pay
            );
            assert_eq!(s.world().objects.get(source).zone_change_count, generation);
            assert!(!crate::engine::has_effective_keyword(
                s.world(),
                source,
                crate::card_def::Keywords::CANT_BE_BLOCKED
            ));
            let result = (gameplay(&s), O::packet(&s.encode()).clone());
            if let Some(previous) = &prior {
                assert_eq!(&result, previous);
            } else {
                prior = Some(result);
            }
            let after_answer = s.snapshot();
            let life = s.world().players[actor.opponent().index()].life;
            let mut could_block = false;
            for _ in 0..128 {
                if s.world().step == Step::Main2 {
                    break;
                }
                // The native V5 surface offers one binary inclusion choice
                // per legal blocker before committing the defender's list.
                let actions = s.actions();
                let offered = actions.iter().find(|a| {
                    matches!(a,
                    ActionSemanticV1::ChooseBlockerInclusion {
                        attacker, blocker: candidate, include: true, ..
                    } if attacker.arena_id == source.0 && candidate.arena_id == blocker.0)
                });
                if let Some(ActionSemanticV1::ChooseBlockerInclusion {
                    actor: chooser,
                    attacker,
                    blocker: candidate,
                    ..
                }) = offered
                {
                    assert_eq!(*chooser, actor.opponent().into());
                    assert_eq!(attacker.zone_change_count, generation);
                    assert_eq!(attacker.controller, actor.into());
                    assert_eq!(attacker.zone, Zone::Battlefield);
                    let original_blocker = s.world().objects.get(blocker);
                    assert_eq!(original_blocker.zone, Zone::Battlefield);
                    assert_eq!(original_blocker.controller, actor.opponent());
                    assert_eq!(
                        candidate.zone_change_count,
                        original_blocker.zone_change_count
                    );
                    assert_eq!(candidate.controller, actor.opponent().into());
                    assert_eq!(candidate.zone, Zone::Battlefield);
                }
                let offered = offered.is_some();
                if pay {
                    assert!(!offered);
                }
                could_block |= offered;
                quiet(&mut s);
            }
            assert_eq!(s.world().step, Step::Main2);
            assert_eq!(could_block, !pay);
            assert_eq!(s.world().players[actor.opponent().index()].life, life - 2);
            s.restore(&after_answer).unwrap();
            assert_eq!(
                (gameplay(&s), O::packet(&s.encode()).clone()),
                prior.as_ref().unwrap().clone()
            );
        }
    }
}

pub(super) fn paid<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        raid::<O>(actor);
        sacrifice::<O>(actor);
    }
}
