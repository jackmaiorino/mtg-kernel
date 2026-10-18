//! Actual native Eaten Alive casts through all four sealed owners. The two
//! own Elves are genuinely cast and paid; lands/libraries/opposing creatures
//! are setup operands. One opposing Elf receives a labelled control primitive
//! and tapped/sick setup flags to test controller-based sacrifice eligibility.
//! Owner::apply keeps the current native header/response/world parity assertions.
use super::{
    entry_attack::{choice_guards, finish_entry, pay_cast, tap},
    *,
};
use crate::{
    engine::CostKind,
    event::CommittedEvent,
    mana::ManaColor,
    state::{FaceDownV1, StackItemKind, Target},
};
fn printing<O: Owner>(s: &O) {
    let d = s.encode();
    assert_eq!(
        fact_count(O::packet(&d).state(), "registered_sorcery_printing"),
        1
    );
    assert!(fact_count(O::packet(&d).state(), "additional_cost_mandatory") > 0);
}
fn choose_guarded<O: Owner>(s: &mut O, choose: impl Fn(&ActionSemanticV1) -> bool) {
    let index = s.actions().iter().position(&choose).unwrap();
    choice_guards(s, index);
    step(s, choose);
}
fn run<O: Owner>(
    actor: PlayerId,
    pay_mana: bool,
    sacrifice_stolen: bool,
    self_target: bool,
    available: u8,
) {
    let mut raw = crate::extra_turn_v1::tests::ready(actor);
    let source = put(&mut raw, actor, "Eaten Alive", Zone::Hand);
    let own = [
        put(&mut raw, actor, "Llanowar Elves", Zone::Hand),
        put(&mut raw, actor, "Llanowar Elves", Zone::Hand),
    ];
    let victim = put(
        &mut raw,
        actor.opponent(),
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let stolen = put(
        &mut raw,
        actor.opponent(),
        "Llanowar Elves",
        Zone::Battlefield,
    );
    // Explicit setup/control primitive, not a claimed paid control-card cast.
    crate::standard_cards_v1::gain_control(&mut raw, stolen, actor);
    raw.objects.get_mut(stolen).tapped = true;
    raw.objects.get_mut(stolen).summoning_sick = true;
    let forests = [
        put(&mut raw, actor, "Forest", Zone::Battlefield),
        put(&mut raw, actor, "Forest", Zone::Battlefield),
    ];
    let swamps: Vec<_> = (0..available)
        .map(|_| put(&mut raw, actor, "Swamp", Zone::Battlefield))
        .collect();
    let mut s = O::own(raw);
    printing(&s);
    for (elf, forest) in own.into_iter().zip(forests) {
        pay_cast(&mut s, actor, elf, &[forest]);
        finish_entry(&mut s, elf);
        assert!(s.world().objects.get(elf).summoning_sick);
    }
    for swamp in &swamps {
        tap(&mut s, *swamp);
    }
    assert_eq!(
        s.world().players[actor.index()].mana_pool[ManaColor::B.pool_index()],
        available
    );
    let hand = s.snapshot();
    let mut foreign = O::own(s.world().clone());
    let before = gameplay(&foreign);
    assert!(matches!(
        foreign.restore(&hand),
        Err(Error::ForeignSnapshot)
    ));
    assert_eq!(gameplay(&foreign), before);
    let selected = if sacrifice_stolen { stolen } else { own[0] };
    let recipient = if self_target { selected } else { victim };
    let mut paid_result = None;
    for replay in 0..2 {
        if replay != 0 {
            s.restore(&hand).unwrap();
        }
        choose_guarded(
            &mut s,
            |a| matches!(a,ActionSemanticV1::CastSpell{source:host,..}if host.arena_id==source.0),
        );
        // Target selection is separate from payment. A land cannot be a target.
        assert!(!s.actions().iter().any(|a| target(a, forests[0])));
        assert!(s.world().engine.pending_cast.is_some());
        assert_eq!(
            s.world().players[actor.index()].mana_pool[ManaColor::B.pool_index()],
            available
        );
        printing(&s);
        choose_guarded(&mut s, |a| target(a, recipient));
        if available == 5 {
            let options: Vec<_> = s
                .actions()
                .iter()
                .filter_map(|a| match a {
                    ActionSemanticV1::ChooseEffectOption {
                        source: host,
                        option_index,
                        option_count: 2,
                        ..
                    } if host.arena_id == source.0 => Some(*option_index),
                    _ => None,
                })
                .collect();
            assert_eq!(options, vec![0, 1]);
            let branch = s.snapshot();
            choose_guarded(
                &mut s,
                |a| matches!(a,ActionSemanticV1::ChooseEffectOption{source:host,option_index,..}if host.arena_id==source.0 && *option_index==u16::from(pay_mana)),
            );
            let answer = (gameplay(&s), O::packet(&s.encode()).clone());
            s.restore(&branch).unwrap();
            step(
                &mut s,
                |a| matches!(a,ActionSemanticV1::ChooseEffectOption{source:host,option_index,..}if host.arena_id==source.0 && *option_index==u16::from(pay_mana)),
            );
            assert_eq!((gameplay(&s), O::packet(&s.encode()).clone()), answer);
        } else {
            // Only B is funded. The native planner must select the sole payable
            // sacrifice branch automatically, retaining its real creature pick.
            assert!(!pay_mana);
            assert!(!s
                .actions()
                .iter()
                .any(|a| matches!(a, ActionSemanticV1::ChooseEffectOption { .. })));
        }
        if !pay_mana {
            let mut candidates: Vec<_> = s
                .actions()
                .iter()
                .filter_map(|a| match a {
                    ActionSemanticV1::ChooseCostTarget {
                        source: host,
                        cost_kind: CostKind::SacrificeCreatures,
                        remaining: 1,
                        candidate,
                        ..
                    } if host.arena_id == source.0 => Some(candidate.arena_id),
                    _ => None,
                })
                .collect();
            let mut expected = vec![own[0].0, own[1].0, stolen.0];
            candidates.sort();
            expected.sort();
            assert_eq!(candidates, expected);
            assert!(!candidates.contains(&victim.0));
            assert!(!candidates.contains(&forests[0].0));
            assert_eq!(s.world().objects.get(stolen).owner, actor.opponent());
            assert!(
                s.world().objects.get(stolen).tapped
                    && s.world().objects.get(stolen).summoning_sick
            );
            let cost = s.snapshot();
            choose_guarded(
                &mut s,
                |a| matches!(a,ActionSemanticV1::ChooseCostTarget{candidate,..}if candidate.arena_id==selected.0),
            );
            let answer = (gameplay(&s), O::packet(&s.encode()).clone());
            s.restore(&cost).unwrap();
            step(
                &mut s,
                |a| matches!(a,ActionSemanticV1::ChooseCostTarget{candidate,..}if candidate.arena_id==selected.0),
            );
            assert_eq!((gameplay(&s), O::packet(&s.encode()).clone()), answer);
        }
        assert!(s.world().engine.pending_cast.is_none());
        assert_eq!(s.world().objects.get(source).zone, Zone::Stack);
        let due = if pay_mana { 5 } else { 1 };
        assert_eq!(
            s.world().players[actor.index()].mana_pool[ManaColor::B.pool_index()],
            available - due
        );
        assert_eq!(
            s.world().objects.get(selected).zone,
            if pay_mana {
                Zone::Battlefield
            } else {
                Zone::Graveyard
            }
        );
        if !pay_mana {
            let owner = s.world().objects.get(selected).owner;
            assert!(s.world().players[owner.index()]
                .graveyard
                .contains(&selected));
        }
        assert!(swamps.iter().all(|id| s.world().objects.get(*id).tapped));
        assert!(s.world().stack.iter().any(|item| item.source == source
            && item.controller == actor
            && item.kind == StackItemKind::Spell
            && item.targets == vec![Target::Object(recipient)]
            && item.v4.target_contracts.len() == 1));
        assert_eq!(
            s.world()
                .engine
                .event_history
                .iter()
                .filter(|e| matches!(e, CommittedEvent::MandatoryCastPaymentV1 { .. }))
                .count(),
            1
        );
        printing(&s); // Includes the real selected/sacrificed incarnation receipt.
        let paid = (gameplay(&s), O::packet(&s.encode()).clone());
        if let Some(old) = &paid_result {
            assert_eq!(&paid, old);
        } else {
            paid_result = Some(paid);
        }
        let published = s.snapshot();
        let mut result = None;
        for resolve in 0..2 {
            if resolve != 0 {
                s.restore(&published).unwrap();
            }
            drain(&mut s);
            assert_eq!(s.world().objects.get(source).zone, Zone::Graveyard);
            assert_eq!(
                s.world().objects.get(recipient).zone,
                if self_target {
                    Zone::Graveyard
                } else {
                    Zone::Exile
                }
            );
            assert_eq!(
                s.world().players[actor.index()].mana_pool[ManaColor::B.pool_index()],
                available - due
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
        run::<O>(actor, false, false, false, 5);
        run::<O>(actor, true, false, false, 5);
        run::<O>(actor, false, true, false, 5);
        run::<O>(actor, false, false, true, 1);
    }
}
pub(super) fn privacy<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for known in [false, true] {
            let build = |hidden: &str| {
                let mut raw = crate::extra_turn_v1::tests::ready(actor);
                if known {
                    put(&mut raw, actor, "Eaten Alive", Zone::Hand);
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
            let a = build("Eaten Alive");
            let b = build("Llanowar Elves");
            assert_eq!(O::packet(&a.encode()), O::packet(&b.encode()));
            assert_eq!(
                fact_count(
                    O::packet(&a.encode()).state(),
                    "registered_sorcery_printing"
                ),
                usize::from(known)
            );
        }
    }
}
