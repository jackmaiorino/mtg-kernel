//! Real printing casts/native choices with one explicitly labelled event input.
//! Lands/libraries and the graveyard Elf are setup operands. Panda's gain uses
//! a committed event primitive, not an invented paid life-gain card or trigger.
use super::{
    entry_attack::{choice_guards, finish_entry, pay_cast, tap},
    *,
};
use crate::{
    card_def::Keywords,
    event::{self, CommittedEvent, ProposedEvent},
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

fn memorial<O: Owner>(actor: PlayerId) {
    let mut raw = crate::extra_turn_v1::tests::ready(actor);
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut raw).unwrap();
    let source = put(&mut raw, actor, "Akroma's Memorial", Zone::Hand);
    let elf = put(&mut raw, actor, "Llanowar Elves", Zone::Hand);
    let lands: Vec<_> = (0..8)
        .map(|_| put(&mut raw, actor, "Forest", Zone::Battlefield))
        .collect();
    let mut s = O::own(raw);
    printing(&s); // Both public family roots already cover the ID.
    pay_cast(&mut s, actor, source, &lands[..7]);
    finish_entry(&mut s, source);
    pay_cast(&mut s, actor, elf, &lands[7..]);
    finish_entry(&mut s, elf);
    let after_entry = s.snapshot();
    for replay in 0..2 {
        if replay != 0 {
            s.restore(&after_entry).unwrap();
        }
        printing(&s);
        for keyword in [
            Keywords::FLYING,
            Keywords::FIRST_STRIKE,
            Keywords::VIGILANCE,
            Keywords::TRAMPLE,
            Keywords::HASTE,
        ] {
            assert!(crate::engine::has_effective_keyword(
                s.world(),
                elf,
                keyword
            ));
            assert!(!crate::engine::has_effective_keyword(
                s.world(),
                source,
                keyword
            ));
        }
        assert_eq!(
            crate::controlled_creature_grants_v1::color_protection(s.world(), elf),
            (1 << 2) | (1 << 3)
        );
        assert!(s.world().objects.get(elf).summoning_sick);
        assert!(
            fact_count(
                O::packet(&s.encode()).state(),
                "protection_damage_equipment_blocking_targeting"
            ) > 0
        );
        // The actually paid Elf can attack in this same turn solely through haste.
        let turn = s.world().turn_key_v1();
        for _ in 0..32 {
            if s.actions().iter().any(|a| {
                matches!(a, ActionSemanticV1::DeclareAttackers { attackers, .. }
                if attackers.len() == 1 && attackers[0].arena_id == elf.0)
            }) {
                break;
            }
            pass(&mut s);
        }
        assert_eq!(s.world().turn_key_v1(), turn);
        let index = s
            .actions()
            .iter()
            .position(|a| {
                matches!(a, ActionSemanticV1::DeclareAttackers { attackers, .. }
            if attackers.len() == 1 && attackers[0].arena_id == elf.0)
            })
            .unwrap();
        choice_guards(&mut s, index);
        let prompt = s.snapshot();
        let attack = s.actions()[index].clone();
        step(&mut s, |a| a == &attack);
        assert!(
            !s.world().objects.get(elf).tapped,
            "actual vigilance declaration does not tap"
        );
        assert!(s.world().engine.event_history.iter().any(|event| matches!(event,
            CommittedEvent::DeclaredAttacker { source, controller, .. } if *source == elf && *controller == actor)));
        let declared = (gameplay(&s), O::packet(&s.encode()).clone());
        s.restore(&prompt).unwrap();
        step(&mut s, |a| a == &attack);
        assert_eq!((gameplay(&s), O::packet(&s.encode()).clone()), declared);
    }
}

fn panda<O: Owner>(actor: PlayerId) {
    let mut raw = crate::extra_turn_v1::tests::ready(actor);
    let source = put(&mut raw, actor, "Fiendish Panda", Zone::Hand);
    let recipient = put(&mut raw, actor, "Llanowar Elves", Zone::Graveyard);
    let bolt = put(&mut raw, actor, "Lightning Bolt", Zone::Hand);
    let lands: Vec<_> = ["Forest", "Forest", "Plains", "Swamp", "Mountain"]
        .into_iter()
        .map(|name| put(&mut raw, actor, name, Zone::Battlefield))
        .collect();
    let mut s = O::own(raw);
    printing(&s); // Three full Panda meanings still require the ordinary supplement.
    pay_cast(&mut s, actor, source, &lands[..4]);
    finish_entry(&mut s, source);
    assert_eq!(s.world().objects.get(source).counters.plus1_plus1, 0);
    let generation = s.world().objects.get(source).zone_change_count;
    // Explicit committed life-gain primitive applied to the actual paid state.
    // No model action is claimed to issue it. Preserve every original private journal.
    let mut gained = s.world().clone();
    let before_life = gained.players[actor.index()].life;
    event::propose_and_commit(&mut gained, ProposedEvent::life_gain(actor, 5));
    assert_eq!(gained.players[actor.index()].life, before_life + 5);
    assert_eq!(gained.objects.get(source).counters.plus1_plus1, 0);
    assert_eq!(
        gained
            .engine
            .event_history
            .iter()
            .filter(|event| matches!(event,
        CommittedEvent::FiendishPandaLifeGainCaptureV1 { source: original, amount: 5, .. }
        if original.source == source && original.zone_change_count == generation))
            .count(),
        1
    );
    let restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(&gained).unwrap()).unwrap();
    let mut gain_results = Vec::new();
    for world in [gained, restored] {
        let mut owned = O::own(world);
        printing(&owned);
        assert!(fact_count(O::packet(&owned.encode()).state(), "one_counter_per_event") > 0);
        let waiting = owned.snapshot();
        drain(&mut owned);
        assert_eq!(owned.world().objects.get(source).counters.plus1_plus1, 1);
        assert_eq!(
            crate::engine::checked_effective_pt(owned.world(), source).unwrap(),
            (4, 3)
        );
        let resolved = (gameplay(&owned), O::packet(&owned.encode()).clone());
        owned.restore(&waiting).unwrap();
        drain(&mut owned);
        assert_eq!(
            (gameplay(&owned), O::packet(&owned.encode()).clone()),
            resolved
        );
        gain_results.push(resolved);
        s = owned;
    }
    assert_eq!(gain_results[0], gain_results[1]);
    // A real paid existing Lightning Bolt is the death issuer, not a supplied
    // zone change, death trigger, capture, LKI power or publication row.
    tap(&mut s, lands[4]);
    cast(&mut s, bolt);
    step(&mut s, |a| target(a, source));
    assert_eq!(s.world().players[actor.index()].mana_pool, [0; 6]);
    assert_eq!(s.world().objects.get(bolt).zone, Zone::Stack);
    let paid_bolt = s.snapshot();
    let mut outcomes = Vec::new();
    for replay in 0..2 {
        if replay != 0 {
            s.restore(&paid_bolt).unwrap();
        }
        for _ in 0..32 {
            if s.actions().iter().any(|a| target(a, recipient)) {
                break;
            }
            pass(&mut s);
        }
        assert_eq!(s.world().objects.get(source).zone, Zone::Graveyard);
        assert_eq!(s.world().objects.get(recipient).zone, Zone::Graveyard);
        assert!(s
            .world()
            .engine
            .event_history
            .iter()
            .any(|event| matches!(event,
            CommittedEvent::FiendishPandaDeathCaptureV1 { source: original, power: 4, .. }
            if original.source == source && original.zone_change_count == generation)));
        printing(&s);
        assert!(
            fact_count(
                O::packet(&s.encode()).state(),
                "target_excludes_bear_and_changeling"
            ) > 0
        );
        let index = s
            .actions()
            .iter()
            .position(|a| target(a, recipient))
            .unwrap();
        choice_guards(&mut s, index);
        let prompt = s.snapshot();
        step(&mut s, |a| target(a, recipient));
        assert!(s.world().stack.iter().any(|item| item.source == source
            && item.controller == actor
            && item.kind == crate::state::StackItemKind::TriggeredAbility));
        let published = s.snapshot();
        drain(&mut s);
        assert_eq!(s.world().objects.get(recipient).zone, Zone::Battlefield);
        assert!(!s.world().objects.get(recipient).tapped);
        assert_eq!(s.world().objects.get(source).zone, Zone::Graveyard);
        let result = (gameplay(&s), O::packet(&s.encode()).clone());
        s.restore(&published).unwrap();
        drain(&mut s);
        assert_eq!((gameplay(&s), O::packet(&s.encode()).clone()), result);
        s.restore(&prompt).unwrap();
        step(&mut s, |a| target(a, recipient));
        drain(&mut s);
        assert_eq!((gameplay(&s), O::packet(&s.encode()).clone()), result);
        outcomes.push(result);
    }
    assert_eq!(outcomes[0], outcomes[1]);
}

pub(super) fn paid<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        memorial::<O>(actor);
        panda::<O>(actor);
    }
}
