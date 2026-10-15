//! Genuine paid Kellan and upgrades, combat, impulse and land play through
//! all four owners. Mountains and Forest libraries are explicit setup.
//! Counter/control/blink branches are labelled kernel primitives on the paid
//! source or produced trigger, never claimed printed response card actions.
use super::{
    entry_attack::{choice_guards, finish_entry, tap},
    *,
};
use crate::{
    card_def::{Keywords, Subtype},
    event::{self, CommittedEvent, ProposedEvent},
    ids::StackItemId,
    state::{FaceDownV1, StackItemKind, Step, Target},
};
fn printing<O: Owner>(s: &O) {
    let d = s.encode();
    assert_eq!(
        fact_count(O::packet(&d).state(), "registered_permanent_printing"),
        1
    );
    assert!(fact_count(O::packet(&d).state(), "ordered_upgrade_roots") > 0);
}
fn quiet<O: Owner>(s: &mut O) {
    let actions = s.actions();
    let chosen=actions.iter().find(|a|matches!(a,ActionSemanticV1::Pass{..}))
        .or_else(||actions.iter().find(|a|matches!(a,ActionSemanticV1::DeclareAttackers{attackers,..}if attackers.is_empty())))
        .or_else(||actions.iter().find(|a|matches!(a,ActionSemanticV1::DeclareBlockersForAttacker{blockers,..}if blockers.is_empty())))
        .or_else(||(actions.len()==1).then(||&actions[0])).unwrap_or_else(||panic!("unexpected real turn choice:{actions:?}")).clone();
    step(s, |a| a == &chosen);
}
fn activate<O: Owner>(
    s: &mut O,
    actor: PlayerId,
    source: ObjectId,
    index: u8,
    lands: &[ObjectId],
) -> StackItemId {
    assert_eq!(s.world().players[actor.index()].mana_pool, [0; 6]);
    for land in lands {
        tap(s, *land);
    }
    let position=s.actions().iter().position(|a|matches!(a,ActionSemanticV1::ActivateAbility{source:host,ability_index,..}if host.arena_id==source.0 && *ability_index==index)).unwrap();
    choice_guards(s, position);
    let hand = s.snapshot();
    let mut result = None;
    let mut selected = None;
    for replay in 0..2 {
        if replay != 0 {
            s.restore(&hand).unwrap();
        }
        step(
            s,
            |a| matches!(a,ActionSemanticV1::ActivateAbility{source:host,ability_index,..}if host.arena_id==source.0 && *ability_index==index),
        );
        assert_eq!(s.world().players[actor.index()].mana_pool, [0; 6]);
        assert!(lands.iter().all(|id| s.world().objects.get(*id).tapped));
        assert!(!s.world().objects.get(source).tapped);
        let item = s
            .world()
            .stack
            .iter()
            .find(|item| item.source == source && item.kind == StackItemKind::ActivatedAbility)
            .unwrap();
        assert_eq!(item.controller, actor);
        assert_eq!(item.v4.activated_ability_index, Some(index));
        assert!(item.targets.is_empty());
        assert_eq!(
            item.v4.ability_source_contract.unwrap().zone_change_count,
            s.world().objects.get(source).zone_change_count
        );
        selected = Some(item.v4.stack_item_id);
        printing(s);
        let output = (gameplay(s), O::packet(&s.encode()).clone());
        if let Some(old) = &result {
            assert_eq!(&output, old);
        } else {
            result = Some(output);
        }
    }
    selected.unwrap()
}
fn finish_upgrade<O: Owner>(s: &mut O) {
    let paid = s.snapshot();
    drain(s);
    printing(s);
    let output = (gameplay(s), O::packet(&s.encode()).clone());
    s.restore(&paid).unwrap();
    drain(s);
    assert_eq!((gameplay(s), O::packet(&s.encode()).clone()), output);
}
fn form<O: Owner>(s: &O, source: ObjectId, subtype: Subtype, pt: (i16, i16), double: bool) {
    assert_eq!(
        crate::engine::checked_effective_pt(s.world(), source).unwrap(),
        pt
    );
    for required in [Subtype::Human, Subtype::Faerie, subtype] {
        assert!(crate::engine::has_effective_subtype(
            s.world(),
            source,
            required
        ));
    }
    for absent in [Subtype::Scout, Subtype::Detective, Subtype::Rogue] {
        assert_eq!(
            crate::engine::has_effective_subtype(s.world(), source, absent),
            absent == subtype
        );
    }
    assert_eq!(
        crate::engine::has_effective_keyword(s.world(), source, Keywords::DOUBLE_STRIKE),
        double
    );
}
fn blink(raw: &mut GameState, source: ObjectId) {
    // Explicit physical departure/reentry primitive, preserving actual journals.
    event::propose_and_commit(raw, ProposedEvent::zone_change(source, Zone::Hand));
    event::propose_and_commit(raw, ProposedEvent::zone_change(source, Zone::Battlefield));
}
fn pending_departure<O: Owner>(s: &O, source: ObjectId, actor: PlayerId) {
    let mut raw = s.world().clone();
    blink(&mut raw, source);
    let mut owned = O::own(raw);
    finish_upgrade(&mut owned);
    form(&owned, source, Subtype::Scout, (2, 1), false);
    assert!(owned
        .world()
        .objects
        .get(source)
        .v4
        .creature_upgrade
        .is_none());
    assert_eq!(owned.world().players[actor.index()].mana_pool, [0; 6]);
}
fn captured_consequences<O: Owner>(s: &O, source: ObjectId, actor: PlayerId) {
    let top = s.world().players[actor.index()].library[0];
    let other = s.world().players[actor.opponent().index()].library[0];
    for control in [false, true] {
        let mut raw = s.world().clone();
        if control {
            crate::standard_cards_v1::gain_control(&mut raw, source, actor.opponent());
        } else {
            blink(&mut raw, source);
        }
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&raw).unwrap()).unwrap();
        let mut result = None;
        for world in [raw, restored] {
            let mut owned = O::own(world);
            finish_upgrade(&mut owned);
            assert_eq!(owned.world().objects.get(top).zone, Zone::Exile);
            assert_eq!(owned.world().objects.get(other).zone, Zone::Library);
            let permission = owned
                .world()
                .engine
                .exile_play_permissions
                .iter()
                .find(|p| p.object == top)
                .unwrap();
            assert_eq!(permission.holder, actor);
            assert_eq!(
                permission.zone_change_generation,
                owned.world().objects.get(top).zone_change_count
            );
            assert_eq!(
                permission.expiry,
                crate::engine::PlayPermissionExpiry::EndOfTurn
            );
            assert_eq!(
                fact_count(O::packet(&owned.encode()).state(), "permission_incarnation"),
                1
            );
            let output = (gameplay(&owned), O::packet(&owned.encode()).clone());
            if let Some(old) = &result {
                assert_eq!(&output, old);
            } else {
                result = Some(output);
            }
        }
    }
}
fn run<O: Owner>(actor: PlayerId) {
    let mut raw = crate::extra_turn_v1::tests::ready(actor);
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut raw).unwrap();
    let source = put(&mut raw, actor, "Kellan, Planar Trailblazer", Zone::Hand);
    let lands: Vec<_> = (0..11)
        .map(|_| put(&mut raw, actor, "Mountain", Zone::Battlefield))
        .collect();
    let mut s = O::own(raw);
    printing(&s);
    tap(&mut s, lands[0]);
    let cast_index = s
        .actions()
        .iter()
        .position(
            |a| matches!(a,ActionSemanticV1::CastSpell{source:host,..}if host.arena_id==source.0),
        )
        .unwrap();
    choice_guards(&mut s, cast_index);
    let hand = s.snapshot();
    let mut foreign = O::own(s.world().clone());
    let before = gameplay(&foreign);
    assert!(matches!(
        foreign.restore(&hand),
        Err(Error::ForeignSnapshot)
    ));
    assert_eq!(gameplay(&foreign), before);
    cast(&mut s, source);
    assert_eq!(s.world().players[actor.index()].mana_pool, [0; 6]);
    assert!(s.world().objects.get(lands[0]).tapped);
    assert_eq!(s.world().objects.get(source).zone, Zone::Stack);
    finish_entry(&mut s, source);
    form(&s, source, Subtype::Scout, (2, 1), false);
    assert!(s.world().objects.get(source).summoning_sick);
    // Rogue's Detective condition is not an activation restriction: it pays
    // three mana now and resolves without changing this Scout.
    activate(&mut s, actor, source, 1, &lands[1..4]);
    finish_upgrade(&mut s);
    assert!(s.world().objects.get(source).v4.creature_upgrade.is_none());
    form(&s, source, Subtype::Scout, (2, 1), false);
    let row = activate(&mut s, actor, source, 0, &lands[4..6]);
    pending_departure(&s, source, actor);
    // Exact counter primitive on a genuine paid activation proves no refund.
    let mut countered = s.world().clone();
    crate::engine::counter_stack_item_by_id(&mut countered, row).unwrap();
    let mut countered = O::own(countered);
    finish_upgrade(&mut countered);
    form(&countered, source, Subtype::Scout, (2, 1), false);
    assert_eq!(countered.world().players[actor.index()].mana_pool, [0; 6]);
    finish_upgrade(&mut s);
    form(&s, source, Subtype::Detective, (2, 1), false);
    let granted = s
        .world()
        .objects
        .get(source)
        .v4
        .creature_upgrade
        .clone()
        .unwrap();
    assert!(granted.combat_impulse.is_some());
    assert_eq!(granted.combat_impulse_source.unwrap().source, source);
    assert!(fact_count(O::packet(&s.encode()).state(), "combat_impulse_source") > 0);
    // Scout condition fails on the current Detective, with the paid activation
    // still legal. The original persistent grant and timestamp remain exact.
    activate(&mut s, actor, source, 0, &lands[6..8]);
    finish_upgrade(&mut s);
    assert_eq!(
        s.world().objects.get(source).v4.creature_upgrade,
        Some(granted.clone())
    );
    activate(&mut s, actor, source, 1, &lands[8..11]);
    finish_upgrade(&mut s);
    form(&s, source, Subtype::Rogue, (3, 2), true);
    assert_eq!(
        s.world()
            .objects
            .get(source)
            .v4
            .creature_upgrade
            .as_ref()
            .unwrap()
            .combat_impulse,
        granted.combat_impulse
    );
    let mut attacked = false;
    for _ in 0..512 {
        if s.world().stack.iter().any(|item| {
            item.source == source
                && item.kind == StackItemKind::TriggeredAbility
                && item.v4.granted_by.is_some()
        }) {
            break;
        }
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
    let item = s
        .world()
        .stack
        .iter()
        .find(|item| item.source == source && item.v4.granted_by.is_some())
        .expect("actual first-strike impulse trigger");
    assert_eq!(item.controller, actor);
    assert_eq!(item.v4.ability_source_contract.unwrap().controller, actor);
    assert_eq!(item.v4.granted_by.unwrap().source, source);
    let library = s.world().players[actor.index()].library.len();
    captured_consequences(&s, source, actor);
    finish_upgrade(&mut s);
    assert_eq!(s.world().players[actor.index()].library.len(), library - 1);
    for _ in 0..128 {
        if s.world().step == Step::Main2
            && s.world().priority_player == actor
            && s.world().active_player == actor
            && s.world().stack.is_empty()
        {
            break;
        }
        quiet(&mut s);
    }
    assert_eq!(s.world().step, Step::Main2);
    assert_eq!(s.world().players[actor.index()].library.len(), library - 2);
    assert_eq!(s.world().players[actor.opponent().index()].life, 14);
    let damage: Vec<_> = s
        .world()
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            CommittedEvent::Damage {
                source: host,
                target: Target::Player(to),
                amount,
            } if *host == source && *to == actor.opponent() => Some(*amount),
            _ => None,
        })
        .collect();
    assert_eq!(damage, vec![3, 3]);
    let permissions = &s.world().engine.exile_play_permissions;
    assert_eq!(permissions.len(), 2);
    let exiled = permissions
        .iter()
        .map(|p| {
            assert_eq!(p.holder, actor);
            assert_eq!(p.expiry, crate::engine::PlayPermissionExpiry::EndOfTurn);
            assert_eq!(
                p.zone_change_generation,
                s.world().objects.get(p.object).zone_change_count
            );
            p.object
        })
        .collect::<Vec<_>>();
    assert_eq!(
        fact_count(O::packet(&s.encode()).state(), "permission_incarnation"),
        2
    );
    let index = s
        .actions()
        .iter()
        .position(
            |a| matches!(a,ActionSemanticV1::PlayLand{source:host,..}if host.arena_id==exiled[0].0),
        )
        .unwrap();
    choice_guards(&mut s, index);
    let main = s.snapshot();
    let mut played = None;
    for replay in 0..2 {
        if replay != 0 {
            s.restore(&main).unwrap();
        }
        step(
            &mut s,
            |a| matches!(a,ActionSemanticV1::PlayLand{source:host,..}if host.arena_id==exiled[0].0),
        );
        assert_eq!(s.world().objects.get(exiled[0]).zone, Zone::Battlefield);
        assert_eq!(s.world().objects.get(exiled[0]).controller, actor);
        assert_eq!(s.world().players[actor.index()].lands_played_this_turn, 1);
        assert_eq!(s.world().objects.get(exiled[1]).zone, Zone::Exile);
        assert_eq!(
            fact_count(O::packet(&s.encode()).state(), "permission_incarnation"),
            1
        );
        let output = (gameplay(&s), O::packet(&s.encode()).clone());
        if let Some(old) = &played {
            assert_eq!(&output, old);
        } else {
            played = Some(output);
        }
    }
    for _ in 0..128 {
        if s.world().active_player != actor {
            break;
        }
        quiet(&mut s);
    }
    assert_ne!(s.world().active_player, actor);
    assert!(s.world().engine.exile_play_permissions.is_empty());
    assert_eq!(s.world().objects.get(exiled[1]).zone, Zone::Exile);
    form(&s, source, Subtype::Rogue, (3, 2), true);
    printing(&s);
    let mut departed = s.world().clone();
    blink(&mut departed, source);
    let reset = O::own(departed);
    form(&reset, source, Subtype::Scout, (2, 1), false);
    assert!(reset
        .world()
        .objects
        .get(source)
        .v4
        .creature_upgrade
        .is_none());
    printing(&reset);
}
pub(super) fn paid<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        run::<O>(actor);
    }
}
pub(super) fn privacy<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for known in [false, true] {
            let build = |hidden: &str| {
                let mut raw = crate::extra_turn_v1::tests::ready(actor);
                if known {
                    put(&mut raw, actor, "Kellan, Planar Trailblazer", Zone::Hand);
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
            let a = build("Kellan, Planar Trailblazer");
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
