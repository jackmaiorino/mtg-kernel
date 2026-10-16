use super::*;
use crate::{
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    rl::{ActionSemanticV1, TargetRefV1},
    state::{GameState, Zone},
};
use serde_json::json;

fn state(actor: PlayerId, hidden: &str, life: i32) -> GameState {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    state.players[actor.index()].life = life;
    for player in [actor, actor.opponent()] {
        put(&mut state, player, "Forest", Zone::Hand);
        for _ in 0..12 {
            put(&mut state, player, "Forest", Zone::Library);
        }
    }
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
    state
}
fn own(state: GameState) -> ElendaSession {
    ElendaSession::from_session(FastActorSessionV1::from_elenda_v52_fixture_state(state)).unwrap()
}
fn expected(session: &ElendaSession) -> FastActorDecisionV1 {
    match session.current_response() {
        FastActorResponseV1::Decision(value) => value,
        other => panic!("genuine V52 decision missing:{other:?}"),
    }
}
fn gameplay(session: &ElendaSession) -> (Vec<u8>, (u64, u64)) {
    (
        serde_json::to_vec(session.inner.fixture_state_v52()).unwrap(),
        session.inner.fixture_counts_v52(),
    )
}
fn act(session: &mut ElendaSession, choose: impl Fn(&ActionSemanticV1) -> bool) {
    let backend = session.inner.decision_v52().unwrap();
    let index = backend
        .actions()
        .iter()
        .position(choose)
        .unwrap_or_else(|| panic!("actual V52 action absent:{:?}", backend.actions()));
    let decision = session.encode(backend.expected()).unwrap();
    assert_eq!(decision.packet.source(), Source::ElendaV52);
    assert_eq!(decision.packet.actions().len(), backend.actions().len());
    session.apply(&decision, index as u32).unwrap();
}
fn mana(session: &mut ElendaSession, land: ObjectId) {
    act(
        session,
        |action| matches!(action,ActionSemanticV1::ActivateManaAbility{source,..}if source.arena_id==land.0),
    );
}
fn cast(session: &mut ElendaSession, spell: ObjectId) {
    act(
        session,
        |action| matches!(action,ActionSemanticV1::CastSpell{source,..}if source.arena_id==spell.0),
    );
}
fn drain(session: &mut ElendaSession) -> crate::elenda_observation_v52::ElendaObservationV52 {
    for _ in 0..64 {
        let value = session.inner.decision_v52().unwrap_or_else(|error| {
            panic!(
                "genuine V52 stack drain lost its decision: {error:?}; response:{:?}",
                session.current_response()
            )
        });
        let state = session.inner.fixture_state_v52();
        if state.stack.is_empty()
            && state.engine.pending_cast.is_none()
            && state.engine.pending_activation.is_none()
            && state.engine.pending_effect.is_none()
            && state.engine.pending_triggers.is_empty()
        {
            assert!(value
                .actions()
                .iter()
                .any(|a| matches!(a, ActionSemanticV1::Pass { .. })));
            return value.observation().clone();
        }
        act(session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
    }
    panic!("genuine paid V52 stack did not finish")
}

#[test]
fn elenda_root_keeps_actual_cost_entry_order_and_complete_typed_statics() {
    let id = crate::card_def::card_id_by_name("Elenda, Saint of Dusk").unwrap();
    let descriptor = super::super::roots::elenda_printing(id).unwrap();
    let printed = &descriptor["elenda_printing"];
    let cast = &printed["ordered_cast_roots"][0];
    assert_eq!(
        cast["ordered_costs"][0]["PrintedMana"],
        json!({"generic":2,"pips":[{"Colored":"W"},{"Colored":"B"}],"x_count":0})
    );
    assert_eq!(cast["ordinary_origin"], "Hand");
    assert_eq!(cast["targets"], "None");
    assert_eq!(
        cast["ordered_resolution"],
        json!(["MoveOriginalSpellFromStackToBattlefieldUnderCasterControl"])
    );
    assert_eq!(
        cast["physical_departure"],
        "BattlefieldOnResolutionOtherwiseGraveyard"
    );
    let meaning = crate::rules_meaning_elenda_v52::printed_meaning(
        id,
        crate::rules_meaning_elenda_v52::RootV52::CreatureSpellAndPrintedStatics,
    )
    .unwrap();
    assert_eq!(printed["meaning"], serde_json::to_value(&meaning).unwrap());
    assert_eq!(meaning.ordered_rules.len(), 4);
    assert!(!meaning.model_admitted);
    // Ordinary recipes cannot silently absorb any of the three new fields.
    for name in [
        "Elenda, Saint of Dusk",
        "Spinner of Souls",
        "Flamewake Phoenix",
        "Painful Quandary",
        "Homunculus Horde",
    ] {
        let id = crate::card_def::card_id_by_name(name).unwrap();
        assert!(
            matches!(super::super::roots::baseline(id),Err(Error::UnsupportedPrintedRoot(actual))if actual==id)
        );
    }
}

#[test]
fn genuine_paid_elenda_and_ravenous_wide_coexistence_restore_and_finite_forward_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut raw = state(actor, "Lightning Bolt", 30);
        let amulet = put(&mut raw, actor, "Ravenous Amulet", Zone::Hand);
        let elenda = put(&mut raw, actor, "Elenda, Saint of Dusk", Zone::Hand);
        let forest: [ObjectId; 4] =
            std::array::from_fn(|_| put(&mut raw, actor, "Forest", Zone::Battlefield));
        let plains = put(&mut raw, actor, "Plains", Zone::Battlefield);
        let swamp = put(&mut raw, actor, "Swamp", Zone::Battlefield);
        // Declared initial numeric operands, before every real journal capture.
        // A noncreature land avoids inventing an extreme creature's SBA survival.
        let wide = put(&mut raw, actor, "Forest", Zone::Battlefield);
        raw.objects.get_mut(wide).counters.plus1_plus1 = i32::MAX;
        raw.objects.get_mut(wide).counters.soul = i32::MAX;
        raw.objects.get_mut(wide).damage = u32::MAX;
        let mut session = own(raw);
        mana(&mut session, forest[0]);
        mana(&mut session, forest[1]);
        cast(&mut session, amulet);
        drain(&mut session);
        for id in [forest[2], forest[3], plains, swamp] {
            mana(&mut session, id);
        }
        let initial = session.encode(expected(&session)).unwrap();
        cast(&mut session, elenda);
        assert_eq!(
            session.inner.fixture_state_v52().players[actor.index()].mana_pool,
            [0; 6]
        );
        assert!(session
            .inner
            .fixture_state_v52()
            .stack
            .iter()
            .any(|row| row.source == elenda));
        let paid = session.snapshot();
        let old = session.encode(expected(&session)).unwrap();
        let observation = drain(&mut session);
        assert_ne!(
            initial.packet.state(),
            session.encode(expected(&session)).unwrap().packet.state()
        );
        let row = observation
            .elenda
            .current
            .iter()
            .find(|r| r.source.arena_id == elenda.0)
            .unwrap();
        assert_eq!(
            (row.effective_power, row.effective_toughness),
            (Some(10), Some(10))
        );
        assert!(
            row.effective_lifelink && row.effective_menace && row.hexproof_from_opponents_instants
        );
        let exact = observation.common.battlefield[actor.index()]
            .iter()
            .find(|r| r.card.stable.arena_id == wide.0)
            .unwrap();
        assert_eq!(
            (exact.plus_one_counters, exact.soul, exact.marked_damage),
            (i32::MAX, i32::MAX, u32::MAX)
        );
        assert_eq!(
            session.inner.fixture_state_v52().objects.get(amulet).zone,
            Zone::Battlefield
        );
        let packet = session.encode(expected(&session)).unwrap();
        let scores = NativeModel::deterministic_fixture()
            .unwrap()
            .forward(packet.packet())
            .unwrap();
        assert_eq!(scores.len(), packet.packet.actions().len());
        assert!(scores.iter().all(|v| v.is_finite()));
        let complete = gameplay(&session);
        session.restore(&paid).unwrap();
        let before = gameplay(&session);
        assert!(matches!(session.apply(&old, 0), Err(Error::StaleBinding)));
        assert_eq!(gameplay(&session), before);
        assert_eq!(drain(&mut session), observation);
        assert_eq!(gameplay(&session), complete);
    }
}

#[test]
fn sealed_v52_index_parity_foreign_packet_forgery_and_repeated_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut raw = state(actor, "Lightning Bolt", 20);
        let elenda = put(&mut raw, actor, "Elenda, Saint of Dusk", Zone::Hand);
        raw.players[actor.index()].mana_pool = [0; 6];
        raw.players[actor.index()].mana_pool[crate::mana::ManaColor::W.pool_index()] = 1;
        raw.players[actor.index()].mana_pool[crate::mana::ManaColor::B.pool_index()] = 1;
        raw.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] = 2;
        let mut reference = ElendaSessionV52::from_session(
            FastActorSessionV1::from_elenda_v52_fixture_state(raw.clone()),
        )
        .unwrap();
        let foreign = own(raw.clone());
        let foreign_decision = foreign.encode(expected(&foreign)).unwrap();
        let mut session = own(raw);
        let original = session.encode(expected(&session)).unwrap();
        let snapshot = session.snapshot();
        let before = gameplay(&session);
        assert!(session.apply(&foreign_decision, 0).is_err());
        assert!(session.restore(&foreign.snapshot()).is_err());
        let mut forged = original.clone();
        forged.packet.actions.reverse();
        assert!(session.apply(&forged, 0).is_err());
        let mut forged = original.clone();
        forged.packet.state = super::super::NumericTree::encode(&json!({"forged":true})).unwrap();
        assert!(session.apply(&forged, 0).is_err());
        let mut header = expected(&session);
        header.environment_revision += 1;
        assert!(session.encode(header).is_err());
        assert_eq!(gameplay(&session), before);
        let actual = session.inner.decision_v52().unwrap();
        let index = actual
            .actions()
            .iter()
            .position(
                |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==elenda.0),
            )
            .unwrap() as u32;
        let other = reference.decision_v52().unwrap();
        assert_eq!(actual.actions(), other.actions());
        assert_eq!(
            session.apply(&original, index).unwrap(),
            reference.step_v52(&other, index).unwrap()
        );
        assert_eq!(
            session.inner.fixture_state_v52(),
            reference.fixture_state_v52()
        );
        assert_eq!(
            session.inner.fixture_counts_v52(),
            reference.fixture_counts_v52()
        );
        let mut revision = expected(&session).environment_revision;
        for _ in 0..3 {
            session.restore(&snapshot).unwrap();
            assert!(expected(&session).environment_revision > revision);
            revision = expected(&session).environment_revision;
            let before = gameplay(&session);
            assert!(session.apply(&original, index).is_err());
            assert_eq!(gameplay(&session), before);
            let current = session.encode(expected(&session)).unwrap();
            assert_eq!(current.packet, original.packet);
            session.apply(&current, index).unwrap();
        }
    }
}

#[test]
fn independently_constructed_hidden_worlds_and_public_threshold_operands_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut values = Vec::new();
        for (hand, library) in [
            ("Lightning Bolt", "Island"),
            ("Flamewake Phoenix", "Spinner of Souls"),
        ] {
            let mut raw = state(actor, hand, 21);
            put(&mut raw, actor.opponent(), library, Zone::Library);
            put(&mut raw, actor, "Elenda, Saint of Dusk", Zone::Hand);
            values.push(own(raw));
        }
        let left = values[0].encode(expected(&values[0])).unwrap();
        let right = values[1].encode(expected(&values[1])).unwrap();
        assert_eq!(left.packet, right.packet);
        // Public life and ordered current bonuses survive numeric encoding.
        let mut raw = state(actor, "Lightning Bolt", 30);
        put(&mut raw, actor.opponent(), "Island", Zone::Library);
        put(&mut raw, actor, "Elenda, Saint of Dusk", Zone::Hand);
        let changed = own(raw);
        assert_ne!(
            left.packet.state(),
            changed.encode(expected(&changed)).unwrap().packet.state()
        );
    }
}

#[test]
fn genuine_elenda_own_instant_target_keeps_original_action_order_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut raw = state(actor, "Forest", 21);
        let elenda = put(&mut raw, actor, "Elenda, Saint of Dusk", Zone::Hand);
        let bolt = put(&mut raw, actor, "Lightning Bolt", Zone::Hand);
        raw.players[actor.index()].mana_pool = [1; 6];
        raw.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] = 2;
        let mut session = own(raw);
        cast(&mut session, elenda);
        drain(&mut session);
        cast(&mut session, bolt);
        let menu = session.inner.decision_v52().unwrap();
        let target=menu.actions().iter().position(|a|matches!(a,ActionSemanticV1::ChooseTarget{target:TargetRefV1::Object{object},..}if object.arena_id==elenda.0)).unwrap();
        let packet = session.encode(menu.expected()).unwrap();
        assert_eq!(packet.packet.actions().len(), menu.actions().len());
        let saved = session.snapshot();
        session.apply(&packet, target as u32).unwrap();
        assert_eq!(
            session.inner.fixture_state_v52().players[actor.index()].mana_pool
                [crate::mana::ManaColor::R.pool_index()],
            0
        );
        let result = drain(&mut session);
        let complete = gameplay(&session);
        assert_eq!(
            session.inner.fixture_state_v52().objects.get(bolt).zone,
            Zone::Graveyard
        );
        assert_eq!(
            session.inner.fixture_state_v52().objects.get(elenda).damage,
            3
        );
        session.restore(&saved).unwrap();
        assert!(session.apply(&packet, target as u32).is_err());
        let current = session.encode(expected(&session)).unwrap();
        assert_eq!(current.packet, packet.packet);
        session.apply(&current, target as u32).unwrap();
        assert_eq!(drain(&mut session), result);
        assert_eq!(gameplay(&session), complete);
    }
}

#[test]
fn genuine_paid_conqueror_and_season_inherit_mandatory_printing_roots_in_v52_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (name, land) in [
            ("Bloodthirsty Conqueror", "Swamp"),
            ("Doubling Season", "Forest"),
        ] {
            let mut raw = state(actor, "Lightning Bolt", 30);
            let spell = put(&mut raw, actor, name, Zone::Hand);
            let lands = (0..5)
                .map(|_| put(&mut raw, actor, land, Zone::Battlefield))
                .collect::<Vec<_>>();
            let mut session = own(raw);
            for id in lands {
                mana(&mut session, id);
            }
            let original = session.encode(expected(&session)).unwrap();
            let saved = session.snapshot();
            session.restore(&saved).unwrap();
            assert!(matches!(
                session.apply(&original, 0),
                Err(Error::StaleBinding)
            ));
            assert_eq!(
                session.encode(expected(&session)).unwrap().packet(),
                original.packet()
            );
            cast(&mut session, spell);
            assert_eq!(
                session.inner.fixture_state_v52().players[actor.index()].mana_pool,
                [0; 6]
            );
            let paid = session.snapshot();
            drain(&mut session);
            assert_eq!(
                session.inner.fixture_state_v52().objects.get(spell).zone,
                Zone::Battlefield
            );
            assert_eq!(
                session
                    .inner
                    .fixture_state_v52()
                    .objects
                    .get(spell)
                    .controller,
                actor
            );
            let result = gameplay(&session);
            session.restore(&paid).unwrap();
            drain(&mut session);
            assert_eq!(gameplay(&session), result);
        }
    }
}

fn ordinary_printing_count(packet: &Packet) -> usize {
    let field = "registered_permanent_printing";
    let rows = packet.state().rows();
    rows.iter()
        .enumerate()
        .filter(|(at, row)| {
            row[0] == 9
                && rows.get(at + 1..at + 1 + field.len()).is_some_and(|bytes| {
                    bytes
                        .iter()
                        .zip(field.bytes())
                        .all(|(r, b)| r[0] == 8 && r[1] == u16::from(b))
                })
        })
        .count()
}
#[test]
fn genuine_paid_frenzied_banner_inherit_printing_cast_color_mana_and_owned_restore_v52_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut raw = state(actor, "Lightning Bolt", 30);
        let goblin = put(&mut raw, actor, "Frenzied Goblin", Zone::Hand);
        let next_goblin = put(&mut raw, actor, "Frenzied Goblin", Zone::Hand);
        let banner = put(&mut raw, actor, "Heraldic Banner", Zone::Hand);
        let mountain = put(&mut raw, actor, "Mountain", Zone::Battlefield);
        let forests = (0..3)
            .map(|_| put(&mut raw, actor, "Forest", Zone::Battlefield))
            .collect::<Vec<_>>();
        let mut session = own(raw);
        let original = session.encode(expected(&session)).unwrap();
        assert_eq!(ordinary_printing_count(original.packet()), 2);
        let current = session.inner.decision_v52().unwrap();
        assert!(current.observation().common.families.iter().any(|f|matches!(f,crate::akroma_family_public_v40::FamilyFactsV40::Frenzied(v)if v.roots.len()==1)));
        assert!(current.observation().common.families.iter().any(|f|matches!(f,crate::akroma_family_public_v40::FamilyFactsV40::Banner(v)if v.roots.len()==4)));
        mana(&mut session, mountain);
        cast(&mut session, goblin);
        assert_eq!(
            session.inner.fixture_state_v52().players[actor.index()].mana_pool,
            [0; 6]
        );
        assert!(
            session
                .inner
                .fixture_state_v52()
                .objects
                .get(mountain)
                .tapped
        );
        assert_eq!(
            session.inner.fixture_state_v52().objects.get(goblin).zone,
            Zone::Stack
        );
        drain(&mut session);
        assert_eq!(
            session.inner.fixture_state_v52().objects.get(goblin).zone,
            Zone::Battlefield
        );
        for id in &forests {
            mana(&mut session, *id);
        }
        let current = session.inner.decision_v52().unwrap();
        let index = current
            .actions()
            .iter()
            .position(
                |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==banner.0),
            )
            .unwrap();
        let decision = session.encode(current.expected()).unwrap();
        assert_eq!(ordinary_printing_count(decision.packet()), 2);
        let mut erased = decision.clone();
        erased.packet.state = super::super::NumericTree::encode(&json!({})).unwrap();
        let before = gameplay(&session);
        assert!(matches!(
            session.apply(&erased, index as u32),
            Err(Error::StaleBinding)
        ));
        assert_eq!(gameplay(&session), before);
        session.apply(&decision, index as u32).unwrap();
        assert_eq!(
            session.inner.fixture_state_v52().players[actor.index()].mana_pool,
            [0; 6]
        );
        assert!(forests
            .iter()
            .all(|id| session.inner.fixture_state_v52().objects.get(*id).tapped));
        assert_eq!(
            session.inner.fixture_state_v52().objects.get(banner).zone,
            Zone::Stack
        );
        let paid = session.snapshot();
        let old = session.encode(expected(&session)).unwrap();
        let mut outcome = None;
        for replay in 0..2 {
            if replay != 0 {
                session.restore(&paid).unwrap();
                let before = gameplay(&session);
                assert!(matches!(session.apply(&old, 0), Err(Error::StaleBinding)));
                assert_eq!(gameplay(&session), before);
                assert_eq!(
                    session.encode(expected(&session)).unwrap().packet(),
                    old.packet()
                );
            }
            let mut chosen = 0;
            for _ in 0..32 {
                let world = session.inner.fixture_state_v52();
                if world.stack.is_empty()
                    && world.engine.pending_effect.is_none()
                    && !crate::linked_aura_operation_v1::has_banner_pending(world)
                {
                    break;
                }
                let current = session.inner.decision_v52().unwrap();
                if current
                    .actions()
                    .iter()
                    .any(|a| matches!(a, ActionSemanticV1::ChooseEntryColorV36 { .. }))
                {
                    assert_eq!(world.objects.get(banner).zone, Zone::Stack);
                    act(&mut session, |a| {
                        matches!(
                            a,
                            ActionSemanticV1::ChooseEntryColorV36 {
                                color: crate::mana::ManaColor::R,
                                ..
                            }
                        )
                    });
                    chosen += 1;
                } else {
                    act(&mut session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
                }
            }
            assert_eq!(chosen, 1);
            assert_eq!(
                session.inner.fixture_state_v52().objects.get(banner).zone,
                Zone::Battlefield
            );
            assert_eq!(
                session
                    .inner
                    .fixture_state_v52()
                    .objects
                    .get(banner)
                    .controller,
                actor
            );
            assert_eq!(
                session
                    .inner
                    .fixture_state_v52()
                    .objects
                    .get(banner)
                    .v4
                    .chosen_color,
                Some(crate::mana::ManaColor::R)
            );
            assert_eq!(
                crate::engine::effective_power(session.inner.fixture_state_v52(), goblin),
                2
            );
            let stack = session.inner.fixture_state_v52().stack.clone();
            mana(&mut session, banner);
            assert_eq!(session.inner.fixture_state_v52().stack, stack);
            assert!(session.inner.fixture_state_v52().objects.get(banner).tapped);
            assert_eq!(
                session.inner.fixture_state_v52().players[actor.index()].mana_pool
                    [crate::mana::ManaColor::R.pool_index()],
                1
            );
            cast(&mut session, next_goblin);
            assert_eq!(
                session.inner.fixture_state_v52().players[actor.index()].mana_pool,
                [0; 6]
            );
            drain(&mut session);
            assert_eq!(
                session
                    .inner
                    .fixture_state_v52()
                    .objects
                    .get(next_goblin)
                    .zone,
                Zone::Battlefield
            );
            let complete = gameplay(&session);
            if let Some(previous) = &outcome {
                assert_eq!(&complete, previous);
            } else {
                outcome = Some(complete);
            }
        }
    }
}
#[test]
fn frenzied_banner_printing_supplements_do_not_disclose_independent_hidden_hands_v52_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for known in [false, true] {
            let make = |hidden| {
                let mut raw = state(actor, hidden, 30);
                if known {
                    put(&mut raw, actor, "Frenzied Goblin", Zone::Hand);
                    put(&mut raw, actor, "Heraldic Banner", Zone::Hand);
                }
                own(raw)
            };
            let a = make("Frenzied Goblin");
            let b = make("Heraldic Banner");
            let pa = a.encode(expected(&a)).unwrap();
            let pb = b.encode(expected(&b)).unwrap();
            assert_eq!(pa.packet(), pb.packet());
            assert_eq!(
                ordinary_printing_count(pa.packet()),
                if known { 2 } else { 0 }
            );
        }
    }
}

#[test]
fn genuine_paid_sower_cast_target_payment_and_restriction_restore_v52_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut raw = state(actor, "Lightning Bolt", 30);
        let source = put(&mut raw, actor, "Sower of Chaos", Zone::Hand);
        let target = put(
            &mut raw,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let mountains = (0..2)
            .map(|_| put(&mut raw, actor, "Mountain", Zone::Battlefield))
            .collect::<Vec<_>>();
        let forests = (0..5)
            .map(|_| put(&mut raw, actor, "Forest", Zone::Battlefield))
            .collect::<Vec<_>>();
        let mut session = own(raw);
        let initial = session.inner.decision_v52().unwrap();
        assert!(initial.observation().common.families.iter().any(|f|matches!(f,crate::akroma_family_public_v40::FamilyFactsV40::Sower(v)if v.roots.len()==1)));
        assert_eq!(
            ordinary_printing_count(session.encode(initial.expected()).unwrap().packet()),
            1
        );
        for id in std::iter::once(&mountains[0]).chain(forests[..3].iter()) {
            mana(&mut session, *id);
        }
        cast(&mut session, source);
        assert_eq!(
            session.inner.fixture_state_v52().players[actor.index()].mana_pool,
            [0; 6]
        );
        assert_eq!(
            session.inner.fixture_state_v52().objects.get(source).zone,
            Zone::Stack
        );
        let cast_paid = session.snapshot();
        let mut entry = None;
        for replay in 0..2 {
            if replay != 0 {
                session.restore(&cast_paid).unwrap();
            }
            drain(&mut session);
            assert_eq!(
                session.inner.fixture_state_v52().objects.get(source).zone,
                Zone::Battlefield
            );
            assert!(
                session
                    .inner
                    .fixture_state_v52()
                    .objects
                    .get(source)
                    .summoning_sick
            );
            let result = gameplay(&session);
            if let Some(previous) = &entry {
                assert_eq!(&result, previous);
            } else {
                entry = Some(result);
            }
        }
        for id in std::iter::once(&mountains[1]).chain(forests[3..].iter()) {
            mana(&mut session, *id);
        }
        act(
            &mut session,
            |a| matches!(a,ActionSemanticV1::ActivateAbility{source:card,ability_index:0,granted_ability,..}if card.arena_id==source.0 && granted_ability.is_none()),
        );
        let world = session.inner.fixture_state_v52();
        assert!(world.stack.is_empty());
        assert!(world.engine.pending_activation.is_some());
        assert!(!world.objects.get(source).tapped);
        assert_eq!(
            world.players[actor.index()]
                .mana_pool
                .iter()
                .copied()
                .map(u32::from)
                .sum::<u32>(),
            3
        );
        let current = session.inner.decision_v52().unwrap();
        let index=current.actions().iter().position(|a|matches!(a,ActionSemanticV1::ChooseTarget{source:card,target:TargetRefV1::Object{object},remaining:1,..}if card.arena_id==source.0 && object.arena_id==target.0 && object.zone==Zone::Battlefield)).unwrap();
        let original = session.encode(current.expected()).unwrap();
        assert_eq!(ordinary_printing_count(original.packet()), 1);
        let pending = session.snapshot();
        let mut outcome = None;
        for replay in 0..2 {
            if replay != 0 {
                session.restore(&pending).unwrap();
                let before = gameplay(&session);
                assert!(matches!(
                    session.apply(&original, index as u32),
                    Err(Error::StaleBinding)
                ));
                assert_eq!(gameplay(&session), before);
                assert_eq!(
                    session.encode(expected(&session)).unwrap().packet(),
                    original.packet()
                );
            }
            let current = session.encode(expected(&session)).unwrap();
            let mut erased = current.clone();
            erased.packet.state = super::super::NumericTree::encode(&json!({})).unwrap();
            let before = gameplay(&session);
            assert!(matches!(
                session.apply(&erased, index as u32),
                Err(Error::StaleBinding)
            ));
            assert_eq!(gameplay(&session), before);
            session.apply(&current, index as u32).unwrap();
            let paid = session.inner.fixture_state_v52();
            assert_eq!(paid.players[actor.index()].mana_pool, [0; 6]);
            assert!(!paid.objects.get(source).tapped);
            assert!(mountains
                .iter()
                .chain(forests.iter())
                .all(|id| paid.objects.get(*id).tapped));
            assert_eq!(paid.stack.len(), 1);
            let ability = &paid.stack[0];
            assert_eq!(ability.kind, crate::state::StackItemKind::ActivatedAbility);
            assert_eq!(ability.source, source);
            assert_eq!(ability.controller, actor);
            assert_eq!(ability.targets, vec![crate::state::Target::Object(target)]);
            let captured = ability.v4.ability_source_contract.unwrap();
            assert_eq!(captured.source, source);
            assert_eq!(
                captured.zone_change_count,
                paid.objects.get(source).zone_change_count
            );
            let paid_snapshot = session.snapshot();
            drain(&mut session);
            assert!(crate::sower_of_chaos_v1::cant_block(
                session.inner.fixture_state_v52(),
                target
            ));
            let result = gameplay(&session);
            session.restore(&paid_snapshot).unwrap();
            drain(&mut session);
            assert_eq!(gameplay(&session), result);
            if let Some(previous) = &outcome {
                assert_eq!(&result, previous);
            } else {
                outcome = Some(result);
            }
        }
    }
}
#[test]
fn sower_printing_supplement_preserves_independent_hidden_hand_invariance_v52_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for known in [false, true] {
            let make = |hidden| {
                let mut raw = state(actor, hidden, 30);
                if known {
                    put(&mut raw, actor, "Sower of Chaos", Zone::Hand);
                }
                own(raw)
            };
            let a = make("Sower of Chaos");
            let b = make("Frenzied Goblin");
            let pa = a.encode(expected(&a)).unwrap();
            let pb = b.encode(expected(&b)).unwrap();
            assert_eq!(pa.packet(), pb.packet());
            assert_eq!(ordinary_printing_count(pa.packet()), usize::from(known));
        }
    }
}

#[test]
fn genuine_paid_wildwood_x_zero_three_255_native_menu_stack_and_entry_restore_v52_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for x in [0u8, 3, 255] {
            let mut raw = state(actor, "Lightning Bolt", 30);
            let source = put(&mut raw, actor, "Wildwood Scourge", Zone::Hand);
            let forest = put(&mut raw, actor, "Forest", Zone::Battlefield);
            // Initial bounded funding predates every operation; actual finalization
            // pays this generic pool and G from the genuine Forest activation.
            raw.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] = x;
            let mut session = own(raw);
            assert_eq!(
                crate::engine::object_mana_value(session.inner.fixture_state_v52(), source),
                1
            );
            let initial = session.inner.decision_v52().unwrap();
            assert!(initial.observation().common.families.iter().any(|f|matches!(f,crate::akroma_family_public_v40::FamilyFactsV40::Wildwood(v)if v.roots.len()==1)));
            assert_eq!(
                ordinary_printing_count(session.encode(initial.expected()).unwrap().packet()),
                1
            );
            mana(&mut session, forest);
            cast(&mut session, source);
            let original_choice = if x != 0 {
                let current = session.inner.decision_v52().unwrap();
                assert_eq!(current.actions().len(), usize::from(x) + 1);
                for (i, a) in current.actions().iter().enumerate() {
                    assert!(
                        matches!(a,ActionSemanticV1::ChooseEffectOption{source:card,option_index,option_count,..}if card.arena_id==source.0 && usize::from(*option_index)==i && *option_count==u16::from(x)+1)
                    );
                }
                assert_eq!(
                    session
                        .inner
                        .fixture_state_v52()
                        .engine
                        .pending_cast
                        .as_ref()
                        .unwrap()
                        .x_value,
                    None
                );
                assert_eq!(
                    crate::engine::object_mana_value(session.inner.fixture_state_v52(), source),
                    1
                );
                let decision = session.encode(current.expected()).unwrap();
                assert_eq!(decision.packet().actions().len(), usize::from(x) + 1);
                Some((session.snapshot(), decision))
            } else {
                assert!(session
                    .inner
                    .fixture_state_v52()
                    .engine
                    .pending_cast
                    .is_none());
                None
            };
            let mut final_world = None;
            for replay in 0..if x == 0 { 1 } else { 2 } {
                if let Some((snapshot, original)) = &original_choice {
                    if replay != 0 {
                        session.restore(snapshot).unwrap();
                        let before = gameplay(&session);
                        assert!(matches!(
                            session.apply(original, u32::from(x)),
                            Err(Error::StaleBinding)
                        ));
                        assert_eq!(gameplay(&session), before);
                        assert_eq!(
                            session.encode(expected(&session)).unwrap().packet(),
                            original.packet()
                        );
                    }
                    let current = session.encode(expected(&session)).unwrap();
                    let mut erased = current.clone();
                    erased.packet.state = super::super::NumericTree::encode(&json!({})).unwrap();
                    let before = gameplay(&session);
                    assert!(matches!(
                        session.apply(&erased, u32::from(x)),
                        Err(Error::StaleBinding)
                    ));
                    assert_eq!(gameplay(&session), before);
                    assert!(matches!(
                        session.apply(&current, u32::from(x) + 1),
                        Err(Error::ActionRange)
                    ));
                    assert_eq!(gameplay(&session), before);
                    session.apply(&current, u32::from(x)).unwrap();
                }
                let paid = session.inner.fixture_state_v52();
                assert!(paid.engine.pending_cast.is_none());
                assert_eq!(paid.players[actor.index()].mana_pool, [0; 6]);
                assert!(paid.objects.get(forest).tapped);
                assert_eq!(paid.objects.get(source).zone, Zone::Stack);
                assert_eq!(paid.stack.len(), 1);
                assert_eq!(paid.stack[0].v4.x_value, u16::from(x));
                assert_eq!(
                    crate::engine::object_mana_value(paid, source),
                    u16::from(x) + 1
                );
                let current = session.inner.decision_v52().unwrap();
                assert_eq!(
                    current.observation().common.projection.surface.stack[0].x_value,
                    u16::from(x)
                );
                let paid_snapshot = session.snapshot();
                let mut entry = None;
                for repeat in 0..2 {
                    if repeat != 0 {
                        session.restore(&paid_snapshot).unwrap();
                    }
                    drain(&mut session);
                    let world = session.inner.fixture_state_v52();
                    assert_eq!(
                        world.objects.get(source).zone,
                        if x == 0 {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
                    assert_eq!(crate::engine::object_mana_value(world, source), 1);
                    assert_eq!(world.objects.get(source).counters.plus1_plus1, i32::from(x));
                    assert_eq!(
                        world
                            .engine
                            .event_history
                            .iter()
                            .filter_map(|event| match event {
                                crate::event::CommittedEvent::PlusOneCountersAdded {
                                    object,
                                    count,
                                    ..
                                } if *object == source => Some(*count),
                                _ => None,
                            })
                            .sum::<i32>(),
                        i32::from(x)
                    );
                    assert_eq!(world.engine.event_history.iter().filter(|event|matches!(event,crate::event::CommittedEvent::PlusOneCountersAdded{object,count,..}if *object==source && *count>0)).count(),usize::from(x!=0));
                    let result = gameplay(&session);
                    if let Some(previous) = &entry {
                        assert_eq!(&result, previous);
                    } else {
                        entry = Some(result);
                    }
                }
                if let Some(previous) = &final_world {
                    assert_eq!(entry.as_ref().unwrap(), previous);
                } else {
                    final_world = entry;
                }
            }
        }
    }
}
#[test]
fn wildwood_variable_printing_supplement_preserves_independent_hidden_hands_v52_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for known in [false, true] {
            let make = |hidden| {
                let mut raw = state(actor, hidden, 30);
                if known {
                    put(&mut raw, actor, "Wildwood Scourge", Zone::Hand);
                }
                own(raw)
            };
            let a = make("Wildwood Scourge");
            let b = make("Frenzied Goblin");
            let pa = a.encode(expected(&a)).unwrap();
            let pb = b.encode(expected(&b)).unwrap();
            assert_eq!(pa.packet(), pb.packet());
            assert_eq!(ordinary_printing_count(pa.packet()), usize::from(known));
        }
    }
}

// A fresh native Elenda-mode stream owns its own issued menu. It never
// clones or receives the model owner's sealed backend capability. Construction
// from the complete fixture world must preserve that world and exact menu;
// a surface-only prefix that cannot be reconstructed must fail these checks.
struct NativeReference {
    session: FastActorSessionV1,
    current: FastActorDecisionV1,
    original: FastActorDecisionV1,
    original_counts: (u64, u64),
    native_counts: (u64, u64),
}
fn native_reference(s: &ElendaSession) -> NativeReference {
    let session =
        FastActorSessionV1::from_elenda_v52_fixture_state(s.inner.fixture_state_v52().clone());
    assert_eq!(s.inner.fixture_state_v52(), session.elenda_fixture_state());
    let FastActorResponseV1::Decision(current) = session.current_response() else {
        panic!("independent native Elenda reference has no current decision")
    };
    let original = expected(s);
    let (_, actions, _) = session
        .elenda_current_decision_input_v52(current, current.acting_player)
        .unwrap();
    assert_eq!(
        actions.as_slice(),
        s.inner.decision_v52().unwrap().actions()
    );
    assert_eq!(
        (
            current.episode_id,
            current.substep_index,
            current.substep_count,
            current.acting_player,
            current.decision_kind,
            current.legal_action_count
        ),
        (
            original.episode_id,
            original.substep_index,
            original.substep_count,
            original.acting_player,
            original.decision_kind,
            original.legal_action_count
        )
    );
    let native_counts = (
        session.policy_step_count(),
        session.physical_decision_count(),
    );
    NativeReference {
        session,
        current,
        original,
        original_counts: s.inner.fixture_counts_v52(),
        native_counts,
    }
}
fn rebase_counter(value: u64, native_before: u64, original_before: u64) -> u64 {
    original_before
        .checked_add(value.checked_sub(native_before).unwrap())
        .unwrap()
}
impl NativeReference {
    fn assert_step(&mut self, s: &ElendaSession, actual: FastActorResponseV1, index: u32) {
        let mut response = self
            .session
            .step(self.current.episode_id, self.current.step, index)
            .unwrap();
        // Only independent stream clock origins differ. Every actor/kind/menu,
        // substep, outcome/reason/reward/deck field is still compared exactly.
        match &mut response {
            FastActorResponseV1::Decision(next) => {
                next.step = rebase_counter(next.step, self.current.step, self.original.step);
                next.environment_revision = rebase_counter(
                    next.environment_revision,
                    self.current.environment_revision,
                    self.original.environment_revision,
                );
                next.physical_decision_id = rebase_counter(
                    next.physical_decision_id,
                    self.current.physical_decision_id,
                    self.original.physical_decision_id,
                );
            }
            FastActorResponseV1::Terminal(terminal) => {
                terminal.policy_step_count = rebase_counter(
                    terminal.policy_step_count,
                    self.native_counts.0,
                    self.original_counts.0,
                );
                terminal.physical_decision_count = rebase_counter(
                    terminal.physical_decision_count,
                    self.native_counts.1,
                    self.original_counts.1,
                );
            }
        }
        assert_eq!(actual, response);
        assert_eq!(
            s.inner.fixture_state_v52(),
            self.session.elenda_fixture_state()
        );
        assert_eq!(
            s.inner.fixture_counts_v52(),
            (
                rebase_counter(
                    self.session.policy_step_count(),
                    self.native_counts.0,
                    self.original_counts.0
                ),
                rebase_counter(
                    self.session.physical_decision_count(),
                    self.native_counts.1,
                    self.original_counts.1
                )
            )
        );
    }
}

impl crate::current_mixed_policy_v1::tests::ordinary_triggers::Owner for ElendaSession {
    type Decision = ElendaDecision;
    type Snapshot = ElendaSnapshot;
    fn own(s: GameState) -> Self {
        Self::from_session(FastActorSessionV1::from_elenda_v52_fixture_state(s)).unwrap()
    }
    fn world(&self) -> &GameState {
        self.inner.fixture_state_v52()
    }
    fn counts(&self) -> (u64, u64) {
        self.inner.fixture_counts_v52()
    }
    fn actions(&self) -> Vec<ActionSemanticV1> {
        self.inner.decision_v52().unwrap().actions().to_vec()
    }
    fn encode(&self) -> Self::Decision {
        let e = match self.inner.current_response() {
            FastActorResponseV1::Decision(e) => e,
            other => panic!("real decision absent:{other:?}"),
        };
        ElendaSession::encode(self, e).unwrap()
    }
    fn packet(d: &Self::Decision) -> &crate::current_mixed_policy_v1::Packet {
        d.packet()
    }
    fn erase(d: &mut Self::Decision) {
        d.packet.state =
            crate::current_mixed_policy_v1::NumericTree::encode(&serde_json::json!({})).unwrap();
    }
    fn apply(&mut self, d: &Self::Decision, index: u32) -> Result<()> {
        let mut reference = native_reference(self);
        let actual = ElendaSession::apply(self, d, index)?;
        reference.assert_step(self, actual, index);
        Ok(())
    }
    fn snapshot(&self) -> Self::Snapshot {
        ElendaSession::snapshot(self)
    }
    fn restore(&mut self, s: &Self::Snapshot) -> Result<()> {
        ElendaSession::restore(self, s)
    }
}
#[test]
fn ashroot_stromkirk_paid_cast_real_target_choice_and_restore_elenda_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid::<ElendaSession>();
}
#[test]
fn strongbox_gourmand_paid_cast_attack_resolution_and_restore_elenda_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_entry_attack::<ElendaSession>();
}
#[test]
fn ashroot_stromkirk_private_opponent_printings_do_not_change_packet_elenda_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::privacy::<ElendaSession>();
}

#[test]
fn memorial_panda_paid_cast_grants_and_gain_death_input_restore_elenda_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_memorial_panda::<ElendaSession>(
    );
}

#[test]
fn early_printings_paid_ordinary_activations_and_restore_elenda_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_early_activation::<ElendaSession>(
    );
}

#[test]
fn crystal_paid_cast_hexproof_prevention_control_restore_privacy_elendasession_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_crystal::<ElendaSession>();
}

#[test]
fn tyrant_herald_paid_cast_damage_flash_restore_privacy_elendasession_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_tyrant_herald::<ElendaSession>();
}

#[test]
fn rallier_bloom_paid_cast_private_pick_and_mana_restore_elenda_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_rallier_bloom::<ElendaSession>();
}

#[test]
fn enchantment_hunter_paid_cast_trigger_restore_privacy_elendasession_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_enchantment_hunter::<
        ElendaSession,
    >();
}

#[test]
fn eaten_paid_additional_branches_exile_restore_privacy_elendasession_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_eaten::<ElendaSession>();
}

#[test]
fn faebloom_paid_instant_reflexive_season_restore_privacy_elendasession_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_faebloom::<ElendaSession>();
}

#[test]
fn kellan_paid_upgrades_combat_impulse_restore_privacy_elendasession_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_kellan::<ElendaSession>();
}

#[test]
fn fiery_paid_optional_equipment_death_replacement_restore_privacy_elenda_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_fiery::<ElendaSession>();
}

#[test]
fn divine_paid_ordinary_kicker_zero_multiple_restore_privacy_elenda_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_divine::<ElendaSession>();
}
