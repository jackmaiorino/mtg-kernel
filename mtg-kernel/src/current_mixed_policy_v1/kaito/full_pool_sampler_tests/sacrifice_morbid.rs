use super::{
    burn_draw::{act, owner, pass, settle, tap_count},
    *,
};
use crate::{
    akroma_family_public_v40::FamilyFactsV40,
    effect::thousand_year_storm_v1_tests::ready,
    event::{self, ProposedEvent},
    ids::ObjectId,
    rl::{ActionSemanticV1 as A, StackItemKindV2, TargetRefV1},
    state::Step,
};

fn assert_legacy_refuses(name: &str) {
    let id = card(name);
    assert!(matches!(
        super::super::super::roots::baseline(id),
        Err(Error::UnsupportedPrintedRoot(actual)) if actual == id
    ));
}

fn cast(owner: &mut KaitoSession, id: ObjectId) {
    act(
        owner,
        |action| matches!(action, A::CastSpell { source, .. } if source.arena_id == id.0),
    );
}

#[test]
fn full_pool_arbiter_paid_sacrifice_and_private_discard_bind_original_actions_both_seats() {
    assert_legacy_refuses("Arbiter of Woe");
    for actor in [PlayerId::P0, PlayerId::P1] {
        let scene = |concealed| {
            let mut state = ready(actor, "Forest");
            modes(&mut state);
            let source = put(&mut state, actor, "Arbiter of Woe", Zone::Hand);
            let payment = put(&mut state, actor, "Savannah Lions", Zone::Battlefield);
            let other = put(&mut state, actor, "Savannah Lions", Zone::Battlefield);
            let foreign = put(
                &mut state,
                actor.opponent(),
                "Savannah Lions",
                Zone::Battlefield,
            );
            let discard = put(&mut state, actor.opponent(), "Swamp", Zone::Hand);
            let retain = put(&mut state, actor.opponent(), concealed, Zone::Hand);
            (
                owner(state),
                source,
                payment,
                other,
                foreign,
                discard,
                retain,
            )
        };
        let (mut left, source, payment, other, foreign, discard, retain) = scene("Island");
        let (mut right, _, _, _, _, _, _) = scene("Mountain");
        assert_eq!(decision(&left).packet(), decision(&right).packet());
        for current in [&mut left, &mut right] {
            cast(current, source);
            let bound = decision(current);
            let targets: Vec<_> = bound
                .fixture_backend_v75()
                .actions()
                .iter()
                .filter_map(|action| match action {
                    A::ChooseCostTarget {
                        source: row,
                        candidate,
                        remaining: 1,
                        ..
                    } if row.arena_id == source.0 => Some(candidate.arena_id),
                    _ => None,
                })
                .collect();
            assert_eq!(targets, [payment.0, other.0]);
            assert!(!targets.contains(&foreign.0));
            act(
                current,
                |action| matches!(action, A::ChooseCostTarget { candidate, .. } if candidate.arena_id == payment.0),
            );
            let paid = current.fixture_state();
            assert_eq!(tap_count(paid, actor), 6);
            assert_eq!(paid.objects.get(payment).zone, Zone::Graveyard);
            assert_eq!(paid.objects.get(source).zone, Zone::Stack);
            assert_eq!(paid.stack[0].v4.paid_cost_refs[0].object, payment);
        }
        assert_eq!(decision(&left).packet(), decision(&right).packet());
        for _ in 0..8 {
            if left.fixture_state().engine.pending_discard.is_some() {
                break;
            }
            pass(&mut left);
        }
        let choosing = decision(&left);
        let backend = choosing.fixture_backend_v75();
        let surface = &backend.observation().common.common.projection.surface;
        let pending = surface.engine_context.pending_discard.as_ref().unwrap();
        assert_eq!(
            (pending.player, pending.count),
            (actor.opponent().into(), 1)
        );
        assert_eq!(surface.life_totals, [200, 200]);
        assert_eq!(
            left.fixture_state().players[actor.index()].library.len(),
            64
        );
        assert!(backend.actions().iter().all(|action| matches!(
            action, A::Discard { actor: chooser, cards }
            if *chooser == actor.opponent().into() && cards.len() == 1
                && [discard.0, retain.0].contains(&cards[0].arena_id)
        )));
        assert_eq!(backend.actions().len(), 2);
        let snapshot = left.snapshot();
        left.restore(&snapshot).unwrap();
        assert!(left.training_input_v1(&choosing).is_err());
        assert_eq!(choosing.packet(), decision(&left).packet());
        act(
            &mut left,
            |action| matches!(action, A::Discard { cards, .. } if cards[0].arena_id == discard.0),
        );
        let after = left.fixture_state();
        assert_eq!(after.objects.get(source).zone, Zone::Battlefield);
        assert_eq!(after.objects.get(discard).zone, Zone::Graveyard);
        assert_eq!(after.objects.get(retain).zone, Zone::Hand);
        assert_eq!(after.players[actor.index()].library.len(), 63);
        assert_eq!(after.players[actor.index()].life, 202);
        assert_eq!(after.players[actor.opponent().index()].life, 198);
    }
}

fn morbid(owner: &KaitoSession) -> bool {
    let d = decision(owner);
    d.fixture_backend_v75()
        .observation()
        .common
        .common
        .families
        .iter()
        .find_map(|family| match family {
            FamilyFactsV40::Cerberus(facts) => Some(facts.creature_died_this_turn),
            _ => None,
        })
        .unwrap()
}

fn enter_end_step(owner: &mut KaitoSession) {
    for _ in 0..8 {
        if owner.fixture_state().step == Step::End {
            return;
        }
        pass(owner);
    }
    panic!("native end step did not start");
}

#[test]
fn full_pool_needletooth_paid_morbid_end_step_binds_original_target_both_seats() {
    assert_legacy_refuses("Needletooth Pack");
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, "Forest");
        modes(&mut state);
        let source = put(&mut state, actor, "Needletooth Pack", Zone::Hand);
        let target = put(&mut state, actor, "Savannah Lions", Zone::Battlefield);
        let foreign = put(
            &mut state,
            actor.opponent(),
            "Savannah Lions",
            Zone::Battlefield,
        );
        let victim = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let mut paid = owner(state);
        cast(&mut paid, source);
        assert_eq!(tap_count(paid.fixture_state(), actor), 5);
        settle(&mut paid);
        assert_eq!(
            paid.fixture_state().objects.get(source).zone,
            Zone::Battlefield
        );

        let mut quiet = paid.fixture_state().clone();
        // Start the native Main2 window with the two otherwise identical paid boards.
        quiet.step = Step::Main2;
        quiet.priority_player = actor;
        let mut death = quiet.clone();
        event::propose_and_commit(
            &mut death,
            ProposedEvent::zone_change(victim, Zone::Graveyard),
        );
        let mut quiet = owner(quiet);
        enter_end_step(&mut quiet);
        assert!(!morbid(&quiet));
        assert!(quiet.fixture_state().stack.is_empty());
        assert!(quiet.fixture_state().engine.pending_triggers.is_empty());

        let mut death = owner(death);
        enter_end_step(&mut death);
        assert!(morbid(&death));
        let choosing = decision(&death);
        let eligible: Vec<_> = choosing
            .fixture_backend_v75()
            .actions()
            .iter()
            .filter_map(|action| match action {
                A::ChooseTarget {
                    source: row,
                    target: TargetRefV1::Object { object },
                    remaining: 1,
                    ..
                } if row.arena_id == source.0 => Some(object.arena_id),
                _ => None,
            })
            .collect();
        assert_eq!(eligible.len(), 2);
        assert!(eligible.contains(&source.0));
        assert!(eligible.contains(&target.0));
        assert!(!eligible.contains(&foreign.0));
        act(&mut death, |action| {
            matches!(
                action, A::ChooseTarget { target: TargetRefV1::Object { object }, .. } if object.arena_id == target.0
            )
        });
        let published = decision(&death);
        let stack = &published
            .fixture_backend_v75()
            .observation()
            .common
            .common
            .projection
            .surface
            .stack;
        assert!(
            matches!(stack.as_slice(), [row] if row.source.arena_id == source.0
            && row.stack_item_kind == StackItemKindV2::TriggeredAbility
            && matches!(row.targets.as_slice(), [TargetRefV1::Object { object }] if object.arena_id == target.0))
        );
        settle(&mut death);
        assert_eq!(
            death
                .fixture_state()
                .objects
                .get(source)
                .counters
                .plus1_plus1,
            0
        );
        assert_eq!(
            death
                .fixture_state()
                .objects
                .get(target)
                .counters
                .plus1_plus1,
            2
        );
        assert_eq!(
            death
                .fixture_state()
                .objects
                .get(foreign)
                .counters
                .plus1_plus1,
            0
        );
    }
}
