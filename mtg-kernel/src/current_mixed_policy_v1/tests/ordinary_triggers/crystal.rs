//! The Crystal, Elf and every Bolt are genuinely cast and paid through each
//! sealed owner. Basic lands/libraries are setup operands. The separate control
//! and concealed-face boundaries are explicitly labelled physical primitives.
use super::{
    entry_attack::{choice_guards, finish_entry, pay_cast, tap},
    *,
};
use crate::{
    event::CommittedEvent,
    state::{FaceDownV1, StackItemKind, Target},
};

fn printing<O: Owner>(s: &O) {
    let d = s.encode();
    assert_eq!(
        fact_count(O::packet(&d).state(), "registered_permanent_printing"),
        1
    );
    assert!(
        fact_count(
            O::packet(&d).state(),
            "opponents_cannot_target_controller_with_spells_or_abilities"
        ) > 0
    );
    assert!(
        fact_count(
            O::packet(&d).state(),
            "prevent_all_positive_noncombat_damage"
        ) > 0
    );
}
fn player(a: &ActionSemanticV1, p: PlayerId) -> bool {
    matches!(a, ActionSemanticV1::ChooseTarget { target: TargetRefV1::Player { player }, .. } if *player == p.into())
}
fn paid_bolt<O: Owner>(
    s: &mut O,
    caster: PlayerId,
    bolt: ObjectId,
    mountain: ObjectId,
    to: Target,
    hexproof: Option<PlayerId>,
) {
    tap(s, mountain);
    cast(s, bolt);
    let actions = s.actions();
    for seat in [PlayerId::P0, PlayerId::P1] {
        assert_eq!(
            actions.iter().any(|a| player(a, seat)),
            hexproof != Some(seat) || seat == caster
        );
    }
    let choose = |a: &ActionSemanticV1| match to {
        Target::Player(p) => player(a, p),
        Target::Object(id) => target(a, id),
        _ => panic!("fixture only selects actual players or creatures"),
    };
    let index = actions.iter().position(&choose).unwrap();
    choice_guards(s, index);
    let prompt = s.snapshot();
    let mut result = None;
    for replay in 0..2 {
        if replay != 0 {
            s.restore(&prompt).unwrap();
        }
        step(s, choose);
        assert_eq!(s.world().players[caster.index()].mana_pool, [0; 6]);
        assert!(s.world().objects.get(mountain).tapped);
        assert!(s.world().engine.pending_cast.is_none());
        assert!(s.world().stack.iter().any(|row| row.source == bolt
            && row.controller == caster
            && row.kind == StackItemKind::Spell
            && row.targets == vec![to]));
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

fn run<O: Owner>(actor: PlayerId, transferred: bool) {
    let mut raw = crate::extra_turn_v1::tests::ready(actor);
    let crystal = put(&mut raw, actor, "Crystal Barricade", Zone::Hand);
    let elf = put(&mut raw, actor, "Llanowar Elves", Zone::Hand);
    let own_bolt = put(&mut raw, actor, "Lightning Bolt", Zone::Hand);
    let other_bolt = put(&mut raw, actor.opponent(), "Lightning Bolt", Zone::Hand);
    let third_bolt = put(&mut raw, actor.opponent(), "Lightning Bolt", Zone::Hand);
    let forest = put(&mut raw, actor, "Forest", Zone::Battlefield);
    let lands = [
        put(&mut raw, actor, "Plains", Zone::Battlefield),
        put(&mut raw, actor, "Forest", Zone::Battlefield),
    ];
    let red = put(&mut raw, actor, "Mountain", Zone::Battlefield);
    let other_red = put(&mut raw, actor.opponent(), "Mountain", Zone::Battlefield);
    let third_red = put(&mut raw, actor.opponent(), "Mountain", Zone::Battlefield);
    let mut s = O::own(raw);
    printing(&s); // Crystal used to fail the generic Creature-only baseline here.
    pay_cast(&mut s, actor, elf, &[forest]);
    finish_entry(&mut s, elf);
    for land in lands {
        tap(&mut s, land);
    }
    let cast_index = s
        .actions()
        .iter()
        .position(
            |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==crystal.0),
        )
        .unwrap();
    choice_guards(&mut s, cast_index);
    let hand = s.snapshot();
    cast(&mut s, crystal);
    assert!(s.world().engine.pending_cast.is_none());
    assert_eq!(s.world().players[actor.index()].mana_pool, [0; 6]);
    assert_eq!(s.world().objects.get(crystal).zone, Zone::Stack);
    let cast_output = (gameplay(&s), O::packet(&s.encode()).clone());
    s.restore(&hand).unwrap();
    cast(&mut s, crystal);
    assert_eq!((gameplay(&s), O::packet(&s.encode()).clone()), cast_output);
    finish_entry(&mut s, crystal);
    printing(&s);
    assert_eq!(
        crate::engine::checked_effective_pt(s.world(), crystal).unwrap(),
        (0, 4)
    );
    assert!(crate::engine::has_effective_keyword(
        s.world(),
        crystal,
        crate::card_def::Keywords::DEFENDER
    ));
    let controller = if transferred {
        // Coherent current-control primitive applied after genuine payment.
        // This does not claim a printed control spell or a paid control action.
        let mut world = s.world().clone();
        world.players[actor.index()]
            .battlefield
            .retain(|id| *id != crystal);
        world.players[actor.opponent().index()]
            .battlefield
            .push(crystal);
        world.objects.get_mut(crystal).controller = actor.opponent();
        assert_eq!(world.objects.get(crystal).owner, actor);
        s = O::own(serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap());
        actor.opponent()
    } else {
        actor
    };
    assert!(crate::crystal_barricade_v1::controller_has_hexproof(
        s.world(),
        controller
    ));
    assert!(!crate::crystal_barricade_v1::controller_has_hexproof(
        s.world(),
        controller.opponent()
    ));
    if transferred {
        // The physical owner's Elf is no longer protected; current controller
        // is beneficiary. The opponent player is now forbidden to this caster.
        paid_bolt(
            &mut s,
            actor,
            own_bolt,
            red,
            Target::Object(elf),
            Some(controller),
        );
        assert_eq!(s.world().objects.get(elf).zone, Zone::Graveyard);
        assert!(s.world().engine.event_history.iter().any(|e|matches!(e,CommittedEvent::Damage{source,target:Target::Object(to),amount:3}if *source==own_bolt && *to==elf)));
    } else {
        // Controller may target themself. Player damage is not prevented.
        let life = s.world().players[actor.index()].life;
        paid_bolt(
            &mut s,
            actor,
            own_bolt,
            red,
            Target::Player(actor),
            Some(controller),
        );
        assert_eq!(s.world().players[actor.index()].life, life - 3);
        // Opponent may target the other creature, but no positive damage commits.
        paid_bolt(
            &mut s,
            actor.opponent(),
            other_bolt,
            other_red,
            Target::Object(elf),
            Some(controller),
        );
        assert_eq!(s.world().objects.get(elf).zone, Zone::Battlefield);
        assert_eq!(s.world().objects.get(elf).damage, 0);
        assert!(!s
            .world()
            .engine
            .event_history
            .iter()
            .any(|e| matches!(e,CommittedEvent::Damage{source,..}if *source==other_bolt)));
        // A single Barricade does not protect itself, even though it is a creature.
        paid_bolt(
            &mut s,
            actor.opponent(),
            third_bolt,
            third_red,
            Target::Object(crystal),
            Some(controller),
        );
        assert_eq!(s.world().objects.get(crystal).zone, Zone::Battlefield);
        assert_eq!(s.world().objects.get(crystal).damage, 3);
    }
    printing(&s);
}
pub(super) fn paid<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        run::<O>(actor, false);
        run::<O>(actor, true);
    }
}
pub(super) fn privacy<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for known in [false, true] {
            // Independent worlds share only public face/roster. No paid history
            // is erased, and no face-down producer or missing permission is invented.
            let build = |hidden: &str| {
                let mut raw = crate::extra_turn_v1::tests::ready(actor);
                if known {
                    put(&mut raw, actor, "Crystal Barricade", Zone::Hand);
                }
                put(&mut raw, actor.opponent(), hidden, Zone::Hand);
                let face = put(&mut raw, actor.opponent(), hidden, Zone::Battlefield);
                raw.objects.get_mut(face).v4.face_down_v1 = Some(FaceDownV1 {
                    disguised: true,
                    lookers: 0,
                    hidden_by: None,
                    hidden_by_source: None,
                });
                // Concealment disables the printed static abilities before
                // any underlying definition becomes a public source/operand.
                assert!(!crate::crystal_barricade_v1::is_active_source(&raw, face));
                assert!(!crate::crystal_barricade_v1::controller_has_hexproof(
                    &raw,
                    actor.opponent()
                ));
                O::own(raw)
            };
            let a = build("Crystal Barricade");
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
