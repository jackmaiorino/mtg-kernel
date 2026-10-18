//! Both printed sources and damage spells are genuinely paid through all four
//! owners. Lands, initial low life and the vanilla opposing recipient are setup
//! operands. Control/concealment/departure below are labelled physical boundaries.
use super::{
    entry_attack::{choice_guards, finish_entry, pay_cast, tap},
    *,
};
use crate::{
    event::{self, CommittedEvent, ProposedEvent},
    state::{FaceDownV1, StackItemKind, Target},
};

fn printing<O: Owner>(s: &O, name: &str) {
    let d = s.encode();
    assert_eq!(
        fact_count(O::packet(&d).state(), "registered_permanent_printing"),
        1
    );
    assert!(
        fact_count(
            O::packet(&d).state(),
            if name == "Twinflame Tyrant" {
                "ordered_damage_changes"
            } else {
                "controller_cannot_lose_game"
            }
        ) > 0
    );
}
fn guarded_cast<O: Owner>(s: &mut O, actor: PlayerId, source: ObjectId, lands: &[ObjectId]) {
    assert_eq!(s.world().players[actor.index()].mana_pool, [0; 6]);
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
    let mut output = None;
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
        let result = (gameplay(s), O::packet(&s.encode()).clone());
        if let Some(previous) = &output {
            assert_eq!(&result, previous);
        } else {
            output = Some(result);
        }
    }
}
fn bolt<O: Owner>(s: &mut O, actor: PlayerId, source: ObjectId, land: ObjectId, to: Target) {
    tap(s, land);
    cast(s, source);
    let select = |a: &ActionSemanticV1| match to {
        Target::Player(p) => {
            matches!(a,ActionSemanticV1::ChooseTarget{target:TargetRefV1::Player{player},..}if *player==p.into())
        }
        Target::Object(id) => target(a, id),
        _ => panic!("fixture uses actual player or creature targets"),
    };
    let index = s.actions().iter().position(&select).unwrap();
    choice_guards(s, index);
    let prompt = s.snapshot();
    let mut result = None;
    for replay in 0..2 {
        if replay != 0 {
            s.restore(&prompt).unwrap();
        }
        step(s, select);
        assert!(s.world().engine.pending_cast.is_none());
        assert_eq!(s.world().players[actor.index()].mana_pool, [0; 6]);
        assert!(s.world().objects.get(land).tapped);
        assert!(s
            .world()
            .stack
            .iter()
            .any(|row| row.source == source && row.controller == actor && row.targets == vec![to]));
        let paid = s.snapshot();
        drain(s);
        let output = (gameplay(s), O::packet(&s.encode()).clone());
        s.restore(&paid).unwrap();
        drain(s);
        assert_eq!((gameplay(s), O::packet(&s.encode()).clone()), output);
        if let Some(previous) = &result {
            assert_eq!(&output, previous);
        } else {
            result = Some(output);
        }
    }
}
fn damage<O: Owner>(s: &O, source: ObjectId, to: Target) -> i32 {
    s.world()
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            CommittedEvent::Damage {
                source: from,
                target,
                amount,
            } if *from == source && *target == to => Some(*amount),
            _ => None,
        })
        .sum()
}
fn transfer(world: &mut GameState, source: ObjectId, to: PlayerId) {
    // Coherent physical controller change, not a printed control-card action.
    let old = world.objects.get(source).controller;
    world.players[old.index()]
        .battlefield
        .retain(|id| *id != source);
    world.players[to.index()].battlefield.push(source);
    world.objects.get_mut(source).controller = to;
}
fn tyrant<O: Owner>(actor: PlayerId, changed_controller: bool) {
    let mut raw = crate::extra_turn_v1::tests::ready(actor);
    let source = put(&mut raw, actor, "Twinflame Tyrant", Zone::Hand);
    let first = put(&mut raw, actor, "Lightning Bolt", Zone::Hand);
    let second = put(&mut raw, actor, "Lightning Bolt", Zone::Hand);
    let recipient = put(
        &mut raw,
        actor.opponent(),
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let lands: Vec<_> = [
        "Mountain", "Mountain", "Forest", "Forest", "Forest", "Mountain", "Mountain",
    ]
    .into_iter()
    .map(|n| put(&mut raw, actor, n, Zone::Battlefield))
    .collect();
    let mut s = O::own(raw);
    printing(&s, "Twinflame Tyrant");
    guarded_cast(&mut s, actor, source, &lands[..5]);
    finish_entry(&mut s, source);
    assert_eq!(
        crate::engine::checked_effective_pt(s.world(), source).unwrap(),
        (3, 5)
    );
    assert!(crate::engine::has_effective_keyword(
        s.world(),
        source,
        crate::card_def::Keywords::FLYING
    ));
    if changed_controller {
        let mut world = s.world().clone();
        transfer(&mut world, source, actor.opponent());
        assert_eq!(world.objects.get(source).owner, actor);
        s = O::own(serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap());
    }
    let amount = if changed_controller { 3 } else { 6 };
    let life = s.world().players[actor.opponent().index()].life;
    bolt(
        &mut s,
        actor,
        first,
        lands[5],
        Target::Player(actor.opponent()),
    );
    assert_eq!(damage(&s, first, Target::Player(actor.opponent())), amount);
    assert_eq!(
        s.world().players[actor.opponent().index()].life,
        life - amount
    );
    bolt(&mut s, actor, second, lands[6], Target::Object(recipient));
    assert_eq!(damage(&s, second, Target::Object(recipient)), amount);
    assert_eq!(s.world().objects.get(recipient).zone, Zone::Graveyard);
    printing(&s, "Twinflame Tyrant");
}
fn herald<O: Owner>(actor: PlayerId) {
    // Opponent's actual turn and real unresolved Think Twice establish both
    // timing properties. Initial life three is a labelled nonterminal operand.
    let mut raw = crate::extra_turn_v1::tests::ready(actor.opponent());
    raw.players[actor.index()].life = 3;
    let source = put(&mut raw, actor, "Herald of Eternal Dawn", Zone::Hand);
    let think = put(&mut raw, actor.opponent(), "Think Twice", Zone::Hand);
    let spell = put(&mut raw, actor.opponent(), "Lightning Bolt", Zone::Hand);
    let islands = [
        put(&mut raw, actor.opponent(), "Island", Zone::Battlefield),
        put(&mut raw, actor.opponent(), "Island", Zone::Battlefield),
    ];
    let red = put(&mut raw, actor.opponent(), "Mountain", Zone::Battlefield);
    let lands: Vec<_> = [
        "Plains", "Plains", "Plains", "Forest", "Forest", "Forest", "Forest",
    ]
    .into_iter()
    .map(|n| put(&mut raw, actor, n, Zone::Battlefield))
    .collect();
    let mut s = O::own(raw);
    pay_cast(&mut s, actor.opponent(), think, &islands);
    // Acquiring actor priority reveals their genuine hand to the owning packet.
    for _ in 0..8 {
        if s.actions().iter().any(|a|matches!(a,ActionSemanticV1::ActivateManaAbility{source:host,..}if host.arena_id==lands[0].0)){break;}
        pass(&mut s);
    }
    printing(&s, "Herald of Eternal Dawn");
    assert_eq!(s.world().active_player, actor.opponent());
    assert!(s.world().stack.iter().any(|row| row.source == think));
    guarded_cast(&mut s, actor, source, &lands);
    assert_eq!(s.world().active_player, actor.opponent());
    assert_eq!(s.world().stack.len(), 2);
    finish_entry(&mut s, source);
    assert_eq!(s.world().objects.get(think).zone, Zone::Graveyard);
    assert!(crate::game_outcome_protection_v1::cannot_lose(
        s.world(),
        actor
    ));
    assert!(crate::game_outcome_protection_v1::cannot_win(
        s.world(),
        actor.opponent()
    ));
    assert_eq!(
        crate::engine::checked_effective_pt(s.world(), source).unwrap(),
        (6, 6)
    );
    for keyword in [
        crate::card_def::Keywords::FLASH,
        crate::card_def::Keywords::FLYING,
    ] {
        assert!(crate::engine::has_effective_keyword(
            s.world(),
            source,
            keyword
        ));
    }
    // Current control changes the beneficiary, retaining original physical owner.
    let mut controlled = s.world().clone();
    transfer(&mut controlled, source, actor.opponent());
    assert_eq!(controlled.objects.get(source).owner, actor);
    assert!(!crate::game_outcome_protection_v1::cannot_lose(
        &controlled,
        actor
    ));
    assert!(crate::game_outcome_protection_v1::cannot_lose(
        &controlled,
        actor.opponent()
    ));
    let controlled =
        O::own(serde_json::from_slice(&serde_json::to_vec(&controlled).unwrap()).unwrap());
    printing(&controlled, "Herald of Eternal Dawn");
    assert_ne!(O::packet(&controlled.encode()), O::packet(&s.encode()));
    bolt(&mut s, actor.opponent(), spell, red, Target::Player(actor));
    assert_eq!(damage(&s, spell, Target::Player(actor)), 3);
    assert_eq!(s.world().players[actor.index()].life, 0);
    assert!(!s.world().players[actor.index()].has_lost);
    printing(&s, "Herald of Eternal Dawn");
    // A labelled departure primitive demonstrates a persistent life condition,
    // not an invented removal spell. This terminal branch has no model action.
    let mut departed = s.world().clone();
    event::propose_and_commit(
        &mut departed,
        ProposedEvent::zone_change(source, Zone::Graveyard),
    );
    crate::trigger::sba_fixed_point(&mut departed);
    assert!(!crate::game_outcome_protection_v1::cannot_lose(
        &departed, actor
    ));
    assert!(departed.players[actor.index()].has_lost);
}
pub(super) fn paid<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        tyrant::<O>(actor, false);
        tyrant::<O>(actor, true);
        herald::<O>(actor);
    }
}
pub(super) fn privacy<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for name in ["Twinflame Tyrant", "Herald of Eternal Dawn"] {
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
                    assert!(crate::twinflame_tyrant_v1::controller(&raw, face).is_none());
                    assert!(!crate::game_outcome_protection_v1::active(&raw, face));
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
