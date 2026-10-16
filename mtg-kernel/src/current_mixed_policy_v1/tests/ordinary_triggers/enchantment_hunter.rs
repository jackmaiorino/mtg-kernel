//! Genuine paid casts and native triggers through V50, V51, V52 and V57.
//! Basic lands, libraries and opposing vanilla victims are setup operands.
//! Banishing source departure is a labelled committed-event primitive on the
//! genuinely paid source. No fixture claims a removal spell issued that event.
use super::{
    entry_attack::{choice_guards, finish_entry, pay_cast, tap},
    *,
};
use crate::{
    card_def::Keywords,
    event::{self, CommittedEvent, ProposedEvent},
    state::{FaceDownV1, StackItemKind},
};
fn printing<O: Owner>(s: &O) {
    assert!(
        fact_count(
            O::packet(&s.encode()).state(),
            "registered_permanent_printing"
        ) >= 1
    );
    assert!(fact_count(O::packet(&s.encode()).state(), "ordinary_origin") >= 1);
}
fn quiet<O: Owner>(s: &mut O) {
    let actions = s.actions();
    let chosen=actions.iter().find(|a|matches!(a,ActionSemanticV1::Pass{..}))
        .or_else(||actions.iter().find(|a|matches!(a,ActionSemanticV1::DeclareAttackers{attackers,..}if attackers.is_empty())))
        .or_else(||actions.iter().find(|a|matches!(a,ActionSemanticV1::DeclareBlockersForAttacker{blockers,..}if blockers.is_empty())))
        .or_else(||(actions.len()==1).then(||&actions[0])).unwrap_or_else(||panic!("unexpected turn choice:{actions:?}")).clone();
    step(s, |a| a == &chosen);
}
fn guarded_cast<O: Owner>(s: &mut O, actor: PlayerId, source: ObjectId, lands: &[ObjectId]) {
    for id in lands {
        tap(s, *id);
    }
    let index = s
        .actions()
        .iter()
        .position(
            |a| matches!(a,ActionSemanticV1::CastSpell{source:host,..}if host.arena_id==source.0),
        )
        .unwrap();
    choice_guards(s, index);
    let hand = s.snapshot();
    let mut previous = None;
    for replay in 0..2 {
        if replay != 0 {
            s.restore(&hand).unwrap();
        }
        cast(s, source);
        assert!(s.world().engine.pending_cast.is_none());
        assert_eq!(s.world().players[actor.index()].mana_pool, [0; 6]);
        assert!(lands.iter().all(|id| s.world().objects.get(*id).tapped));
        assert_eq!(s.world().objects.get(source).zone, Zone::Stack);
        assert!(s.world().stack.iter().any(|row| row.source == source
            && row.controller == actor
            && row.kind == StackItemKind::Spell));
        let output = (gameplay(s), O::packet(&s.encode()).clone());
        if let Some(old) = &previous {
            assert_eq!(&output, old);
        } else {
            previous = Some(output);
        }
    }
}
fn banishing<O: Owner>(actor: PlayerId) {
    let mut raw = crate::extra_turn_v1::tests::ready(actor);
    let source = put(&mut raw, actor, "Banishing Light", Zone::Hand);
    let victim = put(
        &mut raw,
        actor.opponent(),
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let lands: Vec<_> = ["Plains", "Forest", "Forest"]
        .into_iter()
        .map(|name| put(&mut raw, actor, name, Zone::Battlefield))
        .collect();
    let mut s = O::own(raw);
    printing(&s);
    guarded_cast(&mut s, actor, source, &lands);
    let paid = s.snapshot();
    let mut result = None;
    for replay in 0..2 {
        if replay != 0 {
            s.restore(&paid).unwrap();
        }
        for _ in 0..32 {
            if s.actions().iter().any(|a| target(a, victim)) {
                break;
            }
            pass(&mut s);
        }
        assert_eq!(s.world().objects.get(source).zone, Zone::Battlefield);
        let index = s
            .actions()
            .iter()
            .position(|a| target(a, victim))
            .expect("actual Banishing ETB target");
        choice_guards(&mut s, index);
        printing(&s);
        let prompt = s.snapshot();
        step(&mut s, |a| target(a, victim));
        assert!(s.world().stack.iter().any(|row| row.source == source
            && row.controller == actor
            && row.kind == StackItemKind::TriggeredAbility
            && row.targets == vec![crate::state::Target::Object(victim)]));
        let published = s.snapshot();
        drain(&mut s);
        assert_eq!(s.world().objects.get(victim).zone, Zone::Exile);
        let exiled = (gameplay(&s), O::packet(&s.encode()).clone());
        s.restore(&published).unwrap();
        drain(&mut s);
        assert_eq!((gameplay(&s), O::packet(&s.encode()).clone()), exiled);
        s.restore(&prompt).unwrap();
        step(&mut s, |a| target(a, victim));
        drain(&mut s);
        assert_eq!((gameplay(&s), O::packet(&s.encode()).clone()), exiled);
        if let Some(old) = &result {
            assert_eq!(&exiled, old);
        } else {
            result = Some(exiled);
        }
    }
    // Labelled physical departure primitive, with the real installed linked
    // exile duration and private journals retained from the paid native path.
    let mut departed = s.world().clone();
    event::propose_and_commit(
        &mut departed,
        ProposedEvent::zone_change(source, Zone::Graveyard),
    );
    crate::trigger::sba_fixed_point(&mut departed);
    let restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(&departed).unwrap()).unwrap();
    assert_eq!(departed, restored);
    let owned = O::own(restored);
    assert_eq!(owned.world().objects.get(source).zone, Zone::Graveyard);
    assert_eq!(owned.world().objects.get(victim).zone, Zone::Battlefield);
    assert_eq!(
        owned.world().objects.get(victim).controller,
        actor.opponent()
    );
    assert_eq!(owned.world().objects.get(victim).owner, actor.opponent());
    printing(&owned);
}
fn garruk<O: Owner>(actor: PlayerId, ferocious: bool) {
    let mut raw = crate::extra_turn_v1::tests::ready(actor);
    let source = put(&mut raw, actor, "Garruk's Uprising", Zone::Hand);
    let first = ferocious.then(|| put(&mut raw, actor, "High-Society Hunter", Zone::Hand));
    let second = put(&mut raw, actor, "High-Society Hunter", Zone::Hand);
    let lands: Vec<_> = (0..13)
        .map(|_| put(&mut raw, actor, "Swamp", Zone::Battlefield))
        .collect();
    let green = put(&mut raw, actor, "Forest", Zone::Battlefield);
    let mut s = O::own(raw);
    printing(&s);
    if let Some(first) = first {
        pay_cast(&mut s, actor, first, &lands[..5]);
        finish_entry(&mut s, first);
    }
    let library = s.world().players[actor.index()].library.len();
    guarded_cast(&mut s, actor, source, &[green, lands[5], lands[6]]);
    finish_entry(&mut s, source);
    assert_eq!(
        s.world().players[actor.index()].library.len(),
        library - usize::from(ferocious)
    );
    if let Some(first) = first {
        assert!(crate::engine::has_effective_keyword(
            s.world(),
            first,
            Keywords::TRAMPLE
        ));
    }
    printing(&s);
    let library = s.world().players[actor.index()].library.len();
    let hand = s.world().players[actor.index()].hand.len();
    guarded_cast(&mut s, actor, second, &lands[7..12]);
    finish_entry(&mut s, second);
    assert_eq!(s.world().players[actor.index()].library.len(), library - 1);
    assert_eq!(s.world().players[actor.index()].hand.len(), hand);
    assert_eq!(
        crate::engine::checked_effective_pt(s.world(), second).unwrap(),
        (5, 3)
    );
    for keyword in [Keywords::FLYING, Keywords::TRAMPLE] {
        assert!(crate::engine::has_effective_keyword(
            s.world(),
            second,
            keyword
        ));
    }
    // Departure primitive tests that continuous trample ends, independently of
    // the completed original-controller draw consequences above.
    let mut departed = s.world().clone();
    event::propose_and_commit(
        &mut departed,
        ProposedEvent::zone_change(source, Zone::Graveyard),
    );
    assert!(!crate::engine::has_effective_keyword(
        &departed,
        second,
        Keywords::TRAMPLE
    ));
    assert!(crate::engine::has_effective_keyword(
        &departed,
        second,
        Keywords::FLYING
    ));
}
fn hunter<O: Owner>(actor: PlayerId) {
    let mut raw = crate::extra_turn_v1::tests::ready(actor);
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut raw).unwrap();
    let source = put(&mut raw, actor, "High-Society Hunter", Zone::Hand);
    let elf = put(&mut raw, actor, "Llanowar Elves", Zone::Hand);
    let forest = put(&mut raw, actor, "Forest", Zone::Battlefield);
    let lands: Vec<_> = (0..5)
        .map(|_| put(&mut raw, actor, "Swamp", Zone::Battlefield))
        .collect();
    let mut s = O::own(raw);
    printing(&s);
    pay_cast(&mut s, actor, elf, &[forest]);
    finish_entry(&mut s, elf);
    guarded_cast(&mut s, actor, source, &lands);
    finish_entry(&mut s, source);
    let generation = s.world().objects.get(source).zone_change_count;
    let mut attacked = false;
    for _ in 0..512 {
        if s.actions().iter().any(|a|matches!(a,ActionSemanticV1::ChooseEffectTarget{source:host,min_targets:0,max_targets:1,..}if host.arena_id==source.0)) {break;}
        let attack=s.actions().iter().find(|a|matches!(a,ActionSemanticV1::DeclareAttackers{attackers,..}if attackers.len()==1 && attackers[0].arena_id==source.0)).cloned();
        if let Some(attack) = attack {
            assert!(!s.world().objects.get(source).summoning_sick);
            step(&mut s, |a| a == &attack);
            attacked = true;
        } else {
            quiet(&mut s);
        }
    }
    assert!(attacked);
    assert!(s.world().engine.event_history.iter().any(|e|matches!(e,CommittedEvent::DeclaredAttacker{source:host,controller,..}if *host==source && *controller==actor)));
    assert!(!s
        .actions()
        .iter()
        .any(|a| matches!(a, ActionSemanticV1::ChooseTarget { .. })));
    let select=s.actions().iter().find(|a|matches!(a,ActionSemanticV1::ChooseEffectTarget{target:TargetRefV1::Object{object},min_targets:0,max_targets:1,..}if object.arena_id==elf.0)).unwrap().clone();
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
    let index = s.actions().iter().position(|a| a == &select).unwrap();
    choice_guards(&mut s, index);
    printing(&s);
    let prompt = s.snapshot();
    let hand = s.world().players[actor.index()].hand.len();
    let library = s.world().players[actor.index()].library.len();
    for pay in [false, true] {
        let mut result = None;
        for _ in 0..2 {
            s.restore(&prompt).unwrap();
            step(&mut s, |a| a == if pay { &select } else { &decline });
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
                s.world().objects.get(source).counters.plus1_plus1,
                i32::from(pay)
            );
            assert_eq!(s.world().objects.get(source).zone_change_count, generation);
            assert_eq!(
                s.world().players[actor.index()].hand.len(),
                hand + usize::from(pay)
            );
            assert_eq!(
                s.world().players[actor.index()].library.len(),
                library - usize::from(pay)
            );
            let output = (gameplay(&s), O::packet(&s.encode()).clone());
            if let Some(old) = &result {
                assert_eq!(&output, old);
            } else {
                result = Some(output);
            }
        }
    }
}
pub(super) fn paid<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        banishing::<O>(actor);
        garruk::<O>(actor, false);
        garruk::<O>(actor, true);
        hunter::<O>(actor);
    }
}
pub(super) fn privacy<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for name in [
            "Banishing Light",
            "Garruk's Uprising",
            "High-Society Hunter",
        ] {
            for known in [false, true] {
                let build = |hidden: &str| {
                    let mut raw = crate::extra_turn_v1::tests::ready(actor);
                    if known {
                        put(&mut raw, actor, name, Zone::Hand);
                    }
                    put(&mut raw, actor.opponent(), hidden, Zone::Hand);
                    let face = put(&mut raw, actor.opponent(), hidden, Zone::Battlefield);
                    raw.objects.get_mut(face).v4.face_down_v1 = Some(FaceDownV1 {
                        disguised: true,
                        lookers: 0,
                        hidden_by: None,
                        hidden_by_source: None,
                    });
                    O::own(raw)
                };
                let a = build(name);
                let b = build("Llanowar Elves");
                assert_eq!(O::packet(&a.encode()), O::packet(&b.encode()));
                assert_eq!(
                    fact_count(
                        O::packet(&a.encode()).state(),
                        "registered_permanent_printing"
                    ),
                    usize::from(known)
                );
            }
        }
    }
}
