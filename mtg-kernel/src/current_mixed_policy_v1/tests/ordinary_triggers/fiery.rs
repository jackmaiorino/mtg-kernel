//! Fiery, Fishing Pole, equip and the later Bolt are genuinely paid native
//! actions. Libraries, lands, the Elf and its surviving-case counters are setup.
//! Exact counter and target-reentry branches are labelled kernel primitives on
//! that paid history. Owner::apply compares the actual full world and counters
//! with an independently stepped native reference after every accepted action.
use super::{
    entry_attack::{choice_guards, finish_entry, pay_cast, tap},
    *,
};
use crate::{
    event::{self, ProposedEvent},
    state::{FaceDownV1, StackItemKind},
};
fn printing<O: Owner>(s: &O) {
    assert_eq!(
        fact_count(
            O::packet(&s.encode()).state(),
            "registered_instant_printing"
        ),
        1
    );
}
fn guarded<O: Owner>(s: &mut O, choose: impl Fn(&ActionSemanticV1) -> bool) {
    let index = s.actions().iter().position(&choose).unwrap();
    choice_guards(s, index);
    step(s, choose);
}
fn run<O: Owner>(actor: PlayerId, optional: bool, survives: bool) {
    let mut raw = crate::extra_turn_v1::tests::ready(actor);
    let source = put(&mut raw, actor, "Fiery Annihilation", Zone::Hand);
    let pole = put(&mut raw, actor, "Fishing Pole", Zone::Hand);
    let bolt = put(&mut raw, actor, "Lightning Bolt", Zone::Hand);
    let creature = put(&mut raw, actor, "Llanowar Elves", Zone::Battlefield);
    if survives {
        raw.objects.get_mut(creature).counters.plus1_plus1 = 6;
    }
    let other = put(
        &mut raw,
        actor.opponent(),
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let forests: Vec<_> = (0..3)
        .map(|_| put(&mut raw, actor, "Forest", Zone::Battlefield))
        .collect();
    let mountains: Vec<_> = (0..4)
        .map(|_| put(&mut raw, actor, "Mountain", Zone::Battlefield))
        .collect();
    // Keep real priority decisions available before Cleanup clears the shield.
    put(&mut raw, actor.opponent(), "Forest", Zone::Battlefield);
    let mut s = O::own(raw);
    printing(&s);
    pay_cast(&mut s, actor, pole, &forests[..1]);
    finish_entry(&mut s, pole);
    for land in &forests[1..] {
        tap(&mut s, *land);
    }
    for _ in 0..8 {
        if s.actions().iter().any(|a| matches!(a, ActionSemanticV1::ActivateAbility { source, ability_index:0, .. } if source.arena_id == pole.0)) { break; }
        pass(&mut s);
    }
    step(
        &mut s,
        |a| matches!(a, ActionSemanticV1::ActivateAbility { source, ability_index:0, .. } if source.arena_id == pole.0),
    );
    step(&mut s, |a| target(a, creature));
    assert_eq!(s.world().players[actor.index()].mana_pool, [0; 6]);
    drain(&mut s);
    assert_eq!(
        s.world().objects.get(pole).v4.attached_to.unwrap().object,
        creature
    );
    // The genuine paid equip publishes a Fishing family host incarnation.
    // Its distinct wire spelling must share the ordinary ordinal binding;
    // unrelated or extended raw-reference shapes still refuse conversion.
    let fishing = crate::fishing_pole_public_v39::facts_v39(s.world()).unwrap();
    let family = crate::akroma_family_public_v40::FamilyFactsV40::Fishing(Box::new(
        crate::akroma_family_public_v40::FishingFactsV40 {
            poles: fishing.poles,
            operations: fishing.operations,
            roots: Vec::new(),
        },
    ));
    let original = serde_json::to_value(family).unwrap();
    let mut converted = original.clone();
    crate::current_mixed_policy_v1::fishing_public_operands::prepare(&mut converted).unwrap();
    let host = &converted["Fishing"]["poles"][0]["attached_host"];
    assert_eq!(host["object"], creature.0);
    assert_eq!(
        host["generation"],
        s.world().objects.get(creature).zone_change_count
    );
    assert_eq!(
        converted["Fishing"]["poles"][0]["source"],
        original["Fishing"]["poles"][0]["source"]
    );
    for key in ["private_operand", "unknown_generation"] {
        let mut forged = original.clone();
        forged["Fishing"]["poles"][0]["attached_host"][key] = serde_json::json!(9);
        assert!(
            crate::current_mixed_policy_v1::fishing_public_operands::prepare(&mut forged).is_err()
        );
    }
    for land in &mountains[..3] {
        tap(&mut s, *land);
    }
    cast(&mut s, source);
    assert!(!s
        .actions()
        .iter()
        .any(|a| matches!(a, ActionSemanticV1::FinishEffectSelection { .. })));
    assert!(s.actions().iter().any(|a| target(a, other)));
    assert!(!s.actions().iter().any(|a| target(a, pole)));
    printing(&s); // Pending mandatory selection still has a real source contract.
    guarded(&mut s, |a| target(a, creature));
    let options = s.actions();
    assert!(options.iter().any(|a| target(a, pole)));
    assert!(options
        .iter()
        .any(|a| matches!(a, ActionSemanticV1::FinishEffectSelection { .. })));
    assert!(!options.iter().any(|a| target(a, other)));
    let pending = s.snapshot();
    let pending_world_and_counts = gameplay(&s);
    let issued = s.encode();
    let mut foreign = O::own(s.world().clone());
    let before = gameplay(&foreign);
    assert!(matches!(
        foreign.restore(&pending),
        Err(Error::ForeignSnapshot)
    ));
    assert_eq!(gameplay(&foreign), before);
    let mut expected = None;
    for replay in 0..2 {
        if replay != 0 {
            s.restore(&pending).unwrap();
            assert_eq!(gameplay(&s), pending_world_and_counts);
            let before = gameplay(&s);
            assert!(matches!(s.apply(&issued, 0), Err(Error::StaleBinding)));
            assert_eq!(gameplay(&s), before);
            assert_eq!(O::packet(&s.encode()), O::packet(&issued));
        }
        guarded(&mut s, |a| {
            if optional {
                target(a, pole)
            } else {
                matches!(a, ActionSemanticV1::FinishEffectSelection { .. })
            }
        });
        assert_eq!(s.world().players[actor.index()].mana_pool, [0; 6]);
        assert!(mountains[..3]
            .iter()
            .all(|id| s.world().objects.get(*id).tapped));
        let item = s
            .world()
            .stack
            .iter()
            .find(|row| row.source == source)
            .unwrap();
        assert_eq!(item.kind, StackItemKind::Spell);
        assert_eq!(item.controller, actor);
        assert_eq!(item.targets.len(), if optional { 2 } else { 1 });
        assert_eq!(item.v4.target_contracts.len(), item.targets.len());
        assert_eq!(item.v4.source_contract.unwrap().controller, actor);
        let stack_id = item.v4.stack_item_id;
        if replay == 0 {
            // Exact counter primitive: no damage, equipment exile or shield.
            let mut countered = s.world().clone();
            crate::engine::counter_stack_item_by_id(&mut countered, stack_id).unwrap();
            let mut countered = O::own(countered);
            drain(&mut countered);
            assert_eq!(countered.world().objects.get(source).zone, Zone::Graveyard);
            assert_eq!(countered.world().objects.get(creature).damage, 0);
            assert_eq!(countered.world().objects.get(pole).zone, Zone::Battlefield);
            assert!(crate::fiery_public_v1::facts_v1(countered.world())
                .unwrap()
                .death_replacements
                .is_empty());
            countered.encode();
            // A new incarnation of the original creature invalidates both
            // original-target relations, including the attached Equipment.
            let mut reentered = s.world().clone();
            event::propose_and_commit(
                &mut reentered,
                ProposedEvent::zone_change(creature, Zone::Hand),
            );
            event::propose_and_commit(
                &mut reentered,
                ProposedEvent::zone_change(creature, Zone::Battlefield),
            );
            let mut reentered = O::own(reentered);
            drain(&mut reentered);
            assert_eq!(
                reentered.world().objects.get(creature).zone,
                Zone::Battlefield
            );
            assert_eq!(reentered.world().objects.get(creature).damage, 0);
            assert_eq!(reentered.world().objects.get(pole).zone, Zone::Battlefield);
            assert!(crate::fiery_public_v1::facts_v1(reentered.world())
                .unwrap()
                .death_replacements
                .is_empty());
            reentered.encode();
        }
        let paid = s.snapshot();
        let paid_world_and_counts = gameplay(&s);
        drain(&mut s);
        assert_eq!(s.world().objects.get(source).zone, Zone::Graveyard);
        assert_eq!(
            s.world().objects.get(pole).zone,
            if optional {
                Zone::Exile
            } else {
                Zone::Battlefield
            }
        );
        assert_eq!(
            s.world().objects.get(creature).zone,
            if survives {
                Zone::Battlefield
            } else {
                Zone::Exile
            }
        );
        assert_eq!(s.world().objects.get(other).damage, 0);
        let facts = crate::fiery_public_v1::facts_v1(s.world()).unwrap();
        assert_eq!(facts.death_replacements.len(), 1);
        let shield = &facts.death_replacements[0];
        assert_eq!(shield.original_target.arena_id, creature.0);
        assert_eq!(shield.original_target.zone_change_count, 0);
        assert_eq!(shield.original_target_present, survives);
        assert_eq!(shield.source_at_resolution.controller, actor.into());
        assert_eq!(
            shield.lifecycle,
            crate::fiery_public_v1::LifecycleV1::Unexpired
        );
        if survives {
            assert_eq!(s.world().objects.get(creature).damage, 5);
        }
        let after = (gameplay(&s), O::packet(&s.encode()).clone());
        s.restore(&paid).unwrap();
        assert_eq!(gameplay(&s), paid_world_and_counts);
        drain(&mut s);
        assert_eq!((gameplay(&s), O::packet(&s.encode()).clone()), after);
        if let Some(old) = &expected {
            assert_eq!(&after, old);
        } else {
            expected = Some(after);
        }
    }
    if survives {
        // The original replacement remains effective after Fiery has left.
        tap(&mut s, mountains[3]);
        cast(&mut s, bolt);
        step(&mut s, |a| target(a, creature));
        drain(&mut s);
        assert_eq!(s.world().objects.get(creature).zone, Zone::Exile);
        assert_eq!(s.world().objects.get(bolt).zone, Zone::Graveyard);
        let facts = crate::fiery_public_v1::facts_v1(s.world()).unwrap();
        assert_eq!(facts.death_replacements.len(), 1);
        assert!(!facts.death_replacements[0].original_target_present);
        s.encode();
    }
}
pub(super) fn paid<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for optional in [false, true] {
            for survives in [false, true] {
                run::<O>(actor, optional, survives);
            }
        }
    }
}
pub(super) fn privacy<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for known in [false, true] {
            let build = |hidden: &str| {
                let mut raw = crate::extra_turn_v1::tests::ready(actor);
                if known {
                    put(&mut raw, actor, "Fiery Annihilation", Zone::Hand);
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
            let a = build("Fiery Annihilation");
            let b = build("Llanowar Elves");
            assert_eq!(O::packet(&a.encode()), O::packet(&b.encode()));
            assert_eq!(
                fact_count(
                    O::packet(&a.encode()).state(),
                    "registered_instant_printing"
                ),
                usize::from(known)
            );
        }
    }
}
