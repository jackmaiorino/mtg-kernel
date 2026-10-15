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
        let value = session.inner.decision_v52().unwrap();
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
