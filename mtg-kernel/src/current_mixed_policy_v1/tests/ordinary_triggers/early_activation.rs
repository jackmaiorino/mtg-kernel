//! Paid casts and real native activation/ETB menus. Lands, original graveyard
//! cards and the opposing vanilla recipient are explicit fixture operands.
use super::{
    entry_attack::{choice_guards, finish_entry, pay_cast, tap},
    *,
};
use crate::{
    event::CommittedEvent,
    ids::StackItemId,
    state::{StackItemKind, Target},
};

fn printing<O: Owner>(s: &O) {
    assert_eq!(
        fact_count(
            O::packet(&s.encode()).state(),
            "registered_permanent_printing"
        ),
        1
    );
}
fn activation_target<O: Owner>(s: &mut O, source: ObjectId, target_id: ObjectId) -> O::Snapshot {
    step(
        s,
        |a| matches!(a,ActionSemanticV1::ActivateAbility{source:host,ability_index:0,..}if host.arena_id==source.0),
    );
    let index = s
        .actions()
        .iter()
        .position(|a| target(a, target_id))
        .unwrap();
    choice_guards(s, index);
    let prompt = s.snapshot();
    step(s, |a| target(a, target_id));
    prompt
}
fn paid_row<O: Owner>(s: &O, source: ObjectId, actor: PlayerId) -> StackItemId {
    assert_eq!(s.world().players[actor.index()].mana_pool, [0; 6]);
    let row = s
        .world()
        .stack
        .iter()
        .find(|row| row.source == source && row.kind == StackItemKind::ActivatedAbility)
        .unwrap();
    assert_eq!(row.controller, actor);
    assert_eq!(row.v4.activated_ability_index, Some(0));
    row.v4.stack_item_id
}
fn countered<O: Owner>(s: &O, id: StackItemId) -> O {
    // Exact engine counter primitive on a genuinely paid row, not a fictional
    // counterspell/model action. Original payment and private journals remain.
    let mut world = s.world().clone();
    assert!(crate::engine::counter_stack_item_by_id(&mut world, id)
        .unwrap()
        .is_some());
    assert!(!world.stack.iter().any(|row| row.v4.stack_item_id == id));
    let mut result = O::own(world);
    drain(&mut result);
    result
}

fn ooze<O: Owner>(actor: PlayerId, name: &str, creature: bool, leave: bool) {
    let mut raw = crate::extra_turn_v1::tests::ready(actor);
    let source = put(&mut raw, actor, "Scavenging Ooze", Zone::Hand);
    let owner = if creature { actor.opponent() } else { actor };
    let selected = put(&mut raw, owner, name, Zone::Graveyard);
    let bolt = put(&mut raw, actor, "Lightning Bolt", Zone::Hand);
    let lands: Vec<_> = ["Forest", "Forest", "Forest", "Mountain"]
        .into_iter()
        .map(|name| put(&mut raw, actor, name, Zone::Battlefield))
        .collect();
    let mut s = O::own(raw);
    printing(&s);
    pay_cast(&mut s, actor, source, &lands[..2]);
    finish_entry(&mut s, source);
    assert!(s.world().objects.get(source).summoning_sick);
    tap(&mut s, lands[2]);
    let prompt = activation_target(&mut s, source, selected);
    let id = paid_row(&s, source, actor);
    assert!(
        !s.world().objects.get(source).tapped,
        "printed activation has no tap cost"
    );
    let stopped = countered(&s, id);
    assert_eq!(stopped.world().objects.get(selected).zone, Zone::Graveyard);
    assert_eq!(stopped.world().objects.get(source).counters.plus1_plus1, 0);
    assert_eq!(stopped.world().players[actor.index()].mana_pool, [0; 6]);
    let life = s.world().players[actor.index()].life;
    let paid = s.snapshot();
    let mut results = Vec::new();
    for replay in 0..2 {
        if replay != 0 {
            s.restore(&paid).unwrap();
        }
        if leave {
            tap(&mut s, lands[3]);
            cast(&mut s, bolt);
            step(&mut s, |a| target(a, source));
            for _ in 0..16 {
                if s.world().objects.get(source).zone == Zone::Graveyard {
                    break;
                }
                pass(&mut s);
            }
            assert_eq!(s.world().objects.get(source).zone, Zone::Graveyard);
            assert!(s.world().stack.iter().any(|row| row.v4.stack_item_id == id));
        }
        drain(&mut s);
        printing(&s);
        assert_eq!(s.world().objects.get(selected).zone, Zone::Exile);
        assert_eq!(s.world().objects.get(selected).owner, owner);
        assert_eq!(
            s.world().players[actor.index()].life,
            life + i32::from(creature)
        );
        assert_eq!(
            s.world().objects.get(source).counters.plus1_plus1,
            i32::from(creature && !leave)
        );
        assert_eq!(
            s.world().objects.get(source).zone,
            if leave {
                Zone::Graveyard
            } else {
                Zone::Battlefield
            }
        );
        results.push((gameplay(&s), O::packet(&s.encode()).clone()));
    }
    assert_eq!(results[0], results[1]);
    if !leave {
        s.restore(&prompt).unwrap();
        step(&mut s, |a| target(a, selected));
        drain(&mut s);
        assert_eq!((gameplay(&s), O::packet(&s.encode()).clone()), results[0]);
    }
}

fn heartfire<O: Owner>(actor: PlayerId) {
    let mut raw = crate::extra_turn_v1::tests::ready(actor);
    let source = put(&mut raw, actor, "Heartfire Immolator", Zone::Hand);
    let think = put(&mut raw, actor, "Think Twice", Zone::Hand);
    let elf = put(&mut raw, actor, "Llanowar Elves", Zone::Hand);
    let recipient = put(
        &mut raw,
        actor.opponent(),
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let lands: Vec<_> = [
        "Forest", "Mountain", "Forest", "Island", "Mountain", "Forest",
    ]
    .into_iter()
    .map(|name| put(&mut raw, actor, name, Zone::Battlefield))
    .collect();
    let mut s = O::own(raw);
    printing(&s);
    pay_cast(&mut s, actor, source, &lands[..2]);
    finish_entry(&mut s, source);
    pay_cast(&mut s, actor, think, &lands[2..4]);
    drain(&mut s);
    assert_eq!(
        crate::engine::checked_effective_pt(s.world(), source).unwrap(),
        (3, 3)
    );
    pay_cast(&mut s, actor, elf, &lands[5..]);
    finish_entry(&mut s, elf);
    assert_eq!(
        crate::engine::checked_effective_pt(s.world(), source).unwrap(),
        (3, 3),
        "creature cast is not prowess"
    );
    printing(&s);
    assert!(
        fact_count(
            O::packet(&s.encode()).state(),
            "copied_spell_without_cast_does_not_trigger"
        ) > 0
    );
    tap(&mut s, lands[4]);
    step(
        &mut s,
        |a| matches!(a,ActionSemanticV1::ActivateAbility{source:host,ability_index:0,..}if host.arena_id==source.0),
    );
    assert!(!s.actions().iter().any(|a| matches!(
        a,
        ActionSemanticV1::ChooseTarget {
            target: TargetRefV1::Player { .. },
            ..
        }
    )));
    let index = s
        .actions()
        .iter()
        .position(|a| target(a, recipient))
        .unwrap();
    choice_guards(&mut s, index);
    let prompt = s.snapshot();
    step(&mut s, |a| target(a, recipient));
    let id = paid_row(&s, source, actor);
    assert_eq!(s.world().objects.get(source).zone, Zone::Graveyard);
    assert!(s.world().engine.event_history.iter().any(|event|matches!(event,
        CommittedEvent::SourcePowerActivationCommittedV1{item,power:3,..}if item.v4.stack_item_id==id)));
    let stopped = countered(&s, id);
    assert_eq!(stopped.world().objects.get(source).zone, Zone::Graveyard);
    assert_eq!(
        stopped.world().objects.get(recipient).zone,
        Zone::Battlefield
    );
    assert_eq!(stopped.world().players[actor.index()].mana_pool, [0; 6]);
    let paid = s.snapshot();
    let mut results = Vec::new();
    for replay in 0..2 {
        if replay != 0 {
            s.restore(&paid).unwrap();
        }
        drain(&mut s);
        printing(&s);
        let damage: i32 = s
            .world()
            .engine
            .event_history
            .iter()
            .filter_map(|event| match event {
                CommittedEvent::Damage {
                    source: from,
                    target: Target::Object(to),
                    amount,
                } if *from == source && *to == recipient => Some(*amount),
                _ => None,
            })
            .sum();
        assert_eq!(damage, 3);
        assert_eq!(s.world().objects.get(recipient).zone, Zone::Graveyard);
        assert_eq!(s.world().objects.get(source).zone, Zone::Graveyard);
        results.push((gameplay(&s), O::packet(&s.encode()).clone()));
    }
    assert_eq!(results[0], results[1]);
    s.restore(&prompt).unwrap();
    step(&mut s, |a| target(a, recipient));
    drain(&mut s);
    assert_eq!((gameplay(&s), O::packet(&s.encode()).clone()), results[0]);
}

fn zombie<O: Owner>(actor: PlayerId, names: &[&str]) {
    let mut raw = crate::extra_turn_v1::tests::ready(actor);
    let source = put(&mut raw, actor, "Soul-Shackled Zombie", Zone::Hand);
    let first = put(
        &mut raw,
        actor.opponent(),
        if names.is_empty() { "Forest" } else { names[0] },
        Zone::Graveyard,
    );
    let second = put(
        &mut raw,
        actor.opponent(),
        if names.len() < 2 { "Forest" } else { names[1] },
        Zone::Graveyard,
    );
    let foreign = put(&mut raw, actor, "Llanowar Elves", Zone::Graveyard);
    let lands: Vec<_> = ["Forest", "Forest", "Forest", "Swamp"]
        .into_iter()
        .map(|name| put(&mut raw, actor, name, Zone::Battlefield))
        .collect();
    let mut s = O::own(raw);
    printing(&s);
    let life = [
        s.world().players[actor.index()].life,
        s.world().players[actor.opponent().index()].life,
    ];
    pay_cast(&mut s, actor, source, &lands);
    let paid = s.snapshot();
    let mut results = Vec::new();
    for replay in 0..2 {
        if replay != 0 {
            s.restore(&paid).unwrap();
        }
        for _ in 0..16 {
            if s.actions().iter().any(|a| target(a, first)) {
                break;
            }
            pass(&mut s);
        }
        assert_eq!(s.world().objects.get(source).zone, Zone::Battlefield);
        printing(&s);
        let index = s.actions().iter().position(|a| target(a, first)).unwrap();
        choice_guards(&mut s, index);
        let prompt = s.snapshot();
        for answer_replay in 0..2 {
            if answer_replay != 0 {
                s.restore(&prompt).unwrap();
            }
            if !names.is_empty() {
                step(&mut s, |a| target(a, first));
                assert!(!s
                    .actions()
                    .iter()
                    .any(|a| target(a, first) || target(a, foreign)));
                if names.len() == 2 {
                    let next = s.actions().iter().position(|a| target(a, second)).unwrap();
                    choice_guards(&mut s, next); // Restore/authenticate the actual one-target prefix.
                    step(&mut s, |a| target(a, second));
                }
            }
            if names.len() < 2 {
                step(
                    &mut s,
                    |a| matches!(a,ActionSemanticV1::FinishTargetSelection{selected_count,..}if usize::from(*selected_count)==names.len()),
                );
            }
            assert!(s.world().stack.iter().any(|row| row.source == source
                && row.controller == actor
                && row.kind == StackItemKind::TriggeredAbility));
            let published = s.snapshot();
            drain(&mut s);
            printing(&s);
            let amount = if names.contains(&"Llanowar Elves") {
                2
            } else {
                0
            };
            assert_eq!(s.world().players[actor.index()].life, life[0] + amount);
            assert_eq!(
                s.world().players[actor.opponent().index()].life,
                life[1] - amount
            );
            assert_eq!(
                s.world().objects.get(first).zone,
                if names.is_empty() {
                    Zone::Graveyard
                } else {
                    Zone::Exile
                }
            );
            assert_eq!(
                s.world().objects.get(second).zone,
                if names.len() == 2 {
                    Zone::Exile
                } else {
                    Zone::Graveyard
                }
            );
            assert_eq!(s.world().objects.get(foreign).zone, Zone::Graveyard);
            let output = (gameplay(&s), O::packet(&s.encode()).clone());
            s.restore(&published).unwrap();
            drain(&mut s);
            assert_eq!((gameplay(&s), O::packet(&s.encode()).clone()), output);
            results.push(output);
        }
    }
    assert!(results.iter().all(|result| result == &results[0]));
}

pub(super) fn paid<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        ooze::<O>(actor, "Llanowar Elves", true, false);
        ooze::<O>(actor, "Lightning Bolt", false, false);
        ooze::<O>(actor, "Llanowar Elves", true, true);
        heartfire::<O>(actor);
        for names in [
            &[][..],
            &["Forest"][..],
            &["Llanowar Elves"][..],
            &["Llanowar Elves", "Llanowar Elves"][..],
        ] {
            zombie::<O>(actor, names);
        }
    }
}
