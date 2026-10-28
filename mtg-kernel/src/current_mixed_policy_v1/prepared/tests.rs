use super::*;
use crate::current_mixed_policy_v1::NumericTree;
use crate::{
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    rl::{ActionSemanticV1, TargetRefV1},
    state::{GameState, Zone},
};

fn ready(actor: PlayerId) -> GameState {
    let mut s = crate::extra_turn_v1::tests::ready(actor);
    for p in [actor, actor.opponent()] {
        put(&mut s, p, "Forest", Zone::Hand);
        s.players[p.index()].mana_pool = [12; 6];
    }
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut s).unwrap();
    s
}
// Each stream starts from the same genuine pre-advance input. Reconstructing a
// native session from an already-current world would execute another Advance
// and append its own Drake receipt, even when the visible decision is unchanged.
struct FixturePrepared {
    model: PreparedSession,
    native: FastActorSessionV1,
}
struct FixtureSnapshot {
    model: PreparedSnapshot,
    native: crate::rl_session::FastActorSessionSnapshotV1,
}
impl std::ops::Deref for FixturePrepared {
    type Target = PreparedSession;
    fn deref(&self) -> &Self::Target {
        &self.model
    }
}
fn own(input: GameState) -> FixturePrepared {
    let model = PreparedSession::from_session(FastActorSessionV1::from_prepared_v57_fixture_state(
        input.clone(),
    ))
    .unwrap();
    let native = FastActorSessionV1::from_prepared_v57_fixture_state(input);
    let pair = FixturePrepared { model, native };
    pair.assert_native_current();
    pair
}
fn state(s: &FixturePrepared) -> &GameState {
    s.inner.fixture_state_v57()
}
fn expected(s: &FixturePrepared) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(e) = s.current_response() else {
        panic!("real model prompt absent: {:?}", s.current_response())
    };
    e
}
fn gameplay(s: &FixturePrepared) -> (Vec<u8>, (u64, u64)) {
    (
        serde_json::to_vec(state(s)).unwrap(),
        s.inner.fixture_counts_v57(),
    )
}
impl FixturePrepared {
    fn assert_native_current(&self) {
        assert_eq!(state(self), self.native.prepared_fixture_state());
        assert_eq!(
            self.inner.fixture_counts_v57(),
            (
                self.native.policy_step_count(),
                self.native.physical_decision_count()
            )
        );
        assert_eq!(self.current_response(), self.native.current_response());
        let FastActorResponseV1::Decision(current) = self.native.current_response() else {
            return;
        };
        let (_, actions, _) = self
            .native
            .prepared_current_decision_input_v57(current, current.acting_player)
            .unwrap();
        assert_eq!(
            actions.as_slice(),
            self.inner.decision_v57().unwrap().actions()
        );
    }
    fn assert_native_step(&mut self, actual: &FastActorResponseV1, index: u32) {
        let FastActorResponseV1::Decision(current) = self.native.current_response() else {
            panic!("independent native Prepared reference has no current decision")
        };
        let response = self
            .native
            .step(current.episode_id, current.step, index)
            .unwrap();
        assert_eq!(actual, &response);
        self.assert_native_current();
    }
    fn apply(&mut self, d: &PreparedDecision, index: u32) -> Result<FastActorResponseV1> {
        self.assert_native_current();
        let result = self.model.apply(d, index);
        if let Ok(actual) = &result {
            self.assert_native_step(actual, index);
        } else {
            // A rejected model decision must leave both complete streams equal.
            self.assert_native_current();
        }
        result
    }
    fn score_and_apply(
        &mut self,
        decision: &PreparedDecision,
        model: &NativeModel,
    ) -> Result<(Vec<f32>, u32, FastActorResponseV1)> {
        self.assert_native_current();
        let result = self.model.score_and_apply(decision, model);
        if let Ok((_, index, response)) = &result {
            self.assert_native_step(response, *index);
        } else {
            self.assert_native_current();
        }
        result
    }
    fn snapshot(&self) -> FixtureSnapshot {
        self.assert_native_current();
        FixtureSnapshot {
            model: self.model.snapshot(),
            native: self.native.snapshot_v1(),
        }
    }
    fn restore(&mut self, saved: &FixtureSnapshot) -> Result<()> {
        // The real model owner first authenticates its own original snapshot.
        // The independent native stream restores only its separately saved menu.
        let result = self.model.restore(&saved.model);
        if result.is_ok() {
            self.native.prepared_restore_v57(&saved.native).unwrap();
        }
        self.assert_native_current();
        result
    }
}
fn act(s: &mut FixturePrepared, choose: impl Fn(&ActionSemanticV1) -> bool) {
    let d = s.encode(expected(s)).unwrap();
    act_encoded(s, d, choose);
}
fn act_encoded(
    s: &mut FixturePrepared,
    d: PreparedDecision,
    choose: impl Fn(&ActionSemanticV1) -> bool,
) {
    assert_eq!(d.packet.source(), Source::PreparedV57);
    let native = &d.backend;
    let i = native
        .actions()
        .iter()
        .position(choose)
        .unwrap_or_else(|| panic!("real action absent:{:?}", native.actions()));
    assert_eq!(d.packet.actions().len(), native.actions().len());
    let logits = NativeModel::deterministic_fixture()
        .unwrap()
        .forward(d.packet())
        .unwrap();
    assert_eq!(logits.len(), native.actions().len());
    assert!(logits.iter().all(|v| v.is_finite()));
    s.apply(&d, i as u32).unwrap();
}
fn quiet_action(a: &ActionSemanticV1) -> bool {
    match a {
        ActionSemanticV1::Pass { .. }
        | ActionSemanticV1::ChooseAttackerInclusion { include: false, .. }
        | ActionSemanticV1::ChooseBlockerInclusion { include: false, .. } => true,
        ActionSemanticV1::DeclareAttackers { attackers, .. } => attackers.is_empty(),
        ActionSemanticV1::DeclareBlockersForAttacker { blockers, .. } => blockers.is_empty(),
        _ => false,
    }
}
fn pass(s: &mut FixturePrepared) {
    act(s, quiet_action);
}
fn priority(s: &mut FixturePrepared, p: PlayerId) {
    for _ in 0..8 {
        if expected(s).acting_player == p.into() {
            return;
        }
        pass(s);
    }
    panic!("real priority absent")
}
fn drain(s: &mut FixturePrepared) {
    for _ in 0..64 {
        let g = state(s);
        if g.stack.is_empty()
            && g.engine.pending_cast.is_none()
            && g.engine.pending_activation.is_none()
            && g.engine.pending_effect.is_none()
            && g.engine.pending_triggers.is_empty()
        {
            return;
        }
        pass(s);
    }
    panic!("real owned operation did not settle")
}
fn cast(s: &mut FixturePrepared, p: PlayerId, id: ObjectId, target: Option<ObjectId>) {
    priority(s, p);
    let before: u32 = state(s).players[p.index()]
        .mana_pool
        .iter()
        .map(|v| u32::from(*v))
        .sum();
    let cost =
        crate::card_def::CARD_DEFS[usize::from(state(s).objects.get(id).card_def)].mana_value;
    act(
        s,
        |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==id.0),
    );
    if let Some(id) = target {
        act(
            s,
            |a| matches!(a,ActionSemanticV1::ChooseTarget{target:TargetRefV1::Object{object},..}if object.arena_id==id.0),
        );
    }
    let after: u32 = state(s).players[p.index()]
        .mana_pool
        .iter()
        .map(|v| u32::from(*v))
        .sum();
    assert_eq!(before - after, u32::from(cost));
}
fn until(s: &mut FixturePrepared, choose: impl Fn(&ActionSemanticV1) -> bool) {
    for _ in 0..128 {
        if s.inner
            .decision_v57()
            .unwrap_or_else(|error| {
                panic!(
                    "genuine V57 family choice lost its decision: {error:?}; response:{:?}",
                    s.current_response()
                )
            })
            .actions()
            .iter()
            .any(&choose)
        {
            return;
        }
        pass(s);
    }
    panic!("real family choice absent")
}

#[test]
fn exact_new_printing_roots_preserve_cast_cost_characteristics_and_ordered_semantics() {
    for name in [
        "Elenda, Saint of Dusk",
        "Drake Hatcher",
        "Spinner of Souls",
        "Painful Quandary",
        "Flamewake Phoenix",
    ] {
        let id = crate::card_def::card_id_by_name(name).unwrap();
        let value = printing(id).unwrap().unwrap();
        assert!(value.get("prepared_printing").is_some() || value.get("elenda_printing").is_some());
        assert!(!serde_json::to_string(&value).unwrap().contains("EffectOp"));
    }
    let id = crate::card_def::card_id_by_name("Flamewake Phoenix").unwrap();
    let p = printing(id).unwrap().unwrap();
    let p = &p["prepared_printing"];
    assert_eq!(
        p["ordered_cast_roots"][0]["ordered_instructions"][0]["PayPrintedMana"],
        json!({"generic":1,"pips":[{"Colored":"R"},{"Colored":"R"}],"x_count":0})
    );
    assert_eq!(p["ordered_cast_roots"][0]["origin"], "Hand");
    assert_eq!(
        p["ordered_cast_roots"][0]["timing"],
        "CasterMainPhaseEmptyStackWithPriority"
    );
    let r = &p["ordered_special_roots"]["ordered_rules"][4]["GraveyardReturn"];
    assert_eq!(r["optional_colored_cost"], json!(["R"]));
    assert_eq!(r["optional_generic_cost"], 0);
    assert_eq!(
        r["intervening_if"]["checks"],
        json!(["TriggerCollection", "ResolutionBeforeOptionalPayment"])
    );
    assert_eq!(r["recheck_ferocious_after_payment_offer"], false);
    let id = crate::card_def::card_id_by_name("Painful Quandary").unwrap();
    let q = printing(id).unwrap().unwrap();
    let q = &q["prepared_printing"]["ordered_special_roots"]["ordered_rules"][0]
        ["OpponentGenuinelyCastsSpell"]["ordered_resolution"];
    assert_eq!(
        q[0],
        "OriginalCastingOpponentMaySelectZeroOrOneFromExactCurrentPrivateHand"
    );
    assert_eq!(
        q[3],
        json!({"OtherwiseOriginalCastingOpponentLosesLife":{"amount":5}})
    );
    let id = crate::card_def::card_id_by_name("Spinner of Souls").unwrap();
    let p = printing(id).unwrap().unwrap();
    assert_eq!(
        p["prepared_printing"]["ordered_special_roots"]["ordered_rules"][1]
            ["OtherControlledNontokenCreatureDies"]["optional_reveal"][1],
        "DeclineDoesNothing"
    );
    let id = crate::card_def::card_id_by_name("Drake Hatcher").unwrap();
    let p = printing(id).unwrap().unwrap();
    let r = &p["prepared_printing"]["ordered_special_roots"][2]["program"]["Activated"];
    assert_eq!(r["ordered_costs"][0]["RemoveCounters"]["count"], 3);
    assert_eq!(
        r["ordered_instructions"][0]["CreateToken"]["controller"],
        "OriginalActivatingController"
    );
    assert_eq!(r["countering_refunds_costs"], false);
    let horde = crate::card_def::card_id_by_name("Homunculus Horde").unwrap();
    assert!(printing(horde).unwrap().is_none());
    // Horde's root is the observer-trigger printing, not a Prepared one.
    assert!(super::super::roots::baseline(horde).is_ok());
}

#[test]
fn paid_prepared_casts_original_index_finite_logits_full_binding_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for name in [
            "Elenda, Saint of Dusk",
            "Drake Hatcher",
            "Spinner of Souls",
            "Painful Quandary",
            "Flamewake Phoenix",
        ] {
            // Each name is from this fixed public printing list, never a hand scan.
            eprintln!("prepared_fixture::paid_cast_family::{name}");
            let mut raw = ready(actor);
            let spell = put(&mut raw, actor, name, Zone::Hand);
            let mut s = own(raw);
            let decision = s.encode(expected(&s)).unwrap();
            let saved = s.snapshot();
            let original = gameplay(&s);
            let mut forged = decision.clone();
            forged.packet.actions.reverse();
            assert!(s.apply(&forged, 0).is_err());
            assert_eq!(gameplay(&s), original);
            let mut forged = decision.clone();
            forged.packet.state = NumericTree::encode(&json!({"fake":0})).unwrap();
            assert!(s.apply(&forged, 0).is_err());
            assert_eq!(gameplay(&s), original);
            let other = own(state(&s).clone());
            assert!(s
                .apply(&other.encode(expected(&other)).unwrap(), 0)
                .is_err());
            assert!(s.restore(&other.snapshot()).is_err());
            assert_eq!(gameplay(&s), original);
            cast(&mut s, actor, spell, None);
            drain(&mut s);
            assert_eq!(state(&s).objects.get(spell).zone, Zone::Battlefield);
            let output = gameplay(&s);
            let packet = s.encode(expected(&s)).unwrap().packet;
            s.restore(&saved).unwrap();
            assert!(s.apply(&decision, 0).is_err());
            cast(&mut s, actor, spell, None);
            drain(&mut s);
            assert_eq!(gameplay(&s), output);
            assert_eq!(s.encode(expected(&s)).unwrap().packet, packet);
            assert_ne!(packet, decision.packet);
            let input = s.encode(expected(&s)).unwrap();
            let model = NativeModel::deterministic_fixture().unwrap();
            s.assert_native_current();
            let FastActorResponseV1::Decision(native_current) = s.native.current_response() else {
                panic!("independent native score input absent")
            };
            let (_, native_actions, _) = s
                .native
                .prepared_current_decision_input_v57(native_current, native_current.acting_player)
                .unwrap();
            let (logits, _, _) = s.score_and_apply(&input, &model).unwrap();
            assert_eq!(logits.len(), native_actions.len());
            assert!(logits.iter().all(|logit| logit.is_finite()));
        }
    }
}

#[test]
fn paid_quandary_discard_or_life_and_spinner_reveal_choices_keep_exact_packets_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for discard in [false, true] {
            eprintln!(
                "{}",
                if discard {
                    "prepared_fixture::quandary_discard"
                } else {
                    "prepared_fixture::quandary_decline"
                }
            );
            let mut raw = ready(actor);
            let quandary = put(&mut raw, actor, "Painful Quandary", Zone::Hand);
            let think = put(&mut raw, actor.opponent(), "Think Twice", Zone::Hand);
            let choice = put(&mut raw, actor.opponent(), "Llanowar Elves", Zone::Hand);
            let mut s = own(raw);
            cast(&mut s, actor, quandary, None);
            drain(&mut s);
            cast(&mut s, actor.opponent(), think, None);
            until(&mut s, |a| {
                matches!(
                    a,
                    ActionSemanticV1::FinishEffectSelection {
                        selected_count: 0,
                        ..
                    }
                )
            });
            assert_eq!(expected(&s).acting_player, actor.opponent().into());
            let input = s.encode(expected(&s)).unwrap();
            let saved = s.snapshot();
            let choose = |a: &ActionSemanticV1| {
                if discard {
                    matches!(a,ActionSemanticV1::ChooseEffectTarget{target:TargetRefV1::Object{object},..}if object.arena_id==choice.0)
                } else {
                    matches!(
                        a,
                        ActionSemanticV1::FinishEffectSelection {
                            selected_count: 0,
                            ..
                        }
                    )
                }
            };
            act(&mut s, choose);
            drain(&mut s);
            assert_eq!(
                state(&s).players[actor.opponent().index()].life,
                if discard { 20 } else { 15 }
            );
            assert_eq!(
                state(&s).objects.get(choice).zone,
                if discard { Zone::Graveyard } else { Zone::Hand }
            );
            let result = gameplay(&s);
            s.restore(&saved).unwrap();
            assert!(s.apply(&input, 0).is_err());
            act(&mut s, choose);
            drain(&mut s);
            assert_eq!(gameplay(&s), result);
        }
        eprintln!("prepared_fixture::spinner_reveal");
        let mut raw = ready(actor);
        let spinner = put(&mut raw, actor, "Spinner of Souls", Zone::Hand);
        let elf = put(&mut raw, actor, "Llanowar Elves", Zone::Battlefield);
        let bolt = put(&mut raw, actor.opponent(), "Lightning Bolt", Zone::Hand);
        let mut s = own(raw);
        cast(&mut s, actor, spinner, None);
        drain(&mut s);
        cast(&mut s, actor.opponent(), bolt, Some(elf));
        until(&mut s, |a| {
            matches!(
                a,
                ActionSemanticV1::ChooseEffectOption {
                    option_count: 2,
                    ..
                }
            )
        });
        let d = s.encode(expected(&s)).unwrap();
        let saved = s.snapshot();
        assert_eq!(d.packet.actions().len(), 2);
        assert_ne!(d.packet.actions()[0], d.packet.actions()[1]);
        let library = state(&s).players[actor.index()].library.clone();
        let rng = serde_json::to_value(state(&s)).unwrap();
        act(&mut s, |a| {
            matches!(
                a,
                ActionSemanticV1::ChooseEffectOption {
                    option_index: 0,
                    ..
                }
            )
        });
        drain(&mut s);
        assert_eq!(state(&s).players[actor.index()].library, library);
        // Complete world equality on replay checks the retained RNG too.
        let result = gameplay(&s);
        s.restore(&saved).unwrap();
        assert!(s.apply(&d, 0).is_err());
        act(&mut s, |a| {
            matches!(
                a,
                ActionSemanticV1::ChooseEffectOption {
                    option_index: 0,
                    ..
                }
            )
        });
        drain(&mut s);
        assert_eq!(gameplay(&s), result);
        assert_ne!(serde_json::to_value(state(&s)).unwrap(), rng);
    }
}

#[test]
fn paid_spinner_accept_reveals_real_creature_randomizes_bottom_and_replays_private_native_stream_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let seat: crate::rl::PlayerSeatV1 = actor.into();
        let build = |hidden_hand: &str, hidden_library: &str| {
            let id = |name| crate::card_def::card_id_by_name(name).unwrap();
            // These are the initial physical library orders, before either
            // owning stream advances. The first two cards are noncreatures;
            // the genuine reveal must find the third, not an invented choice.
            let mut ordered = vec![id("Forest"); 40];
            ordered[1] = id("Island");
            ordered[2] = id("Llanowar Elves");
            ordered[12] = id(hidden_library);
            let other_library = vec![id(hidden_library); 40];
            let libraries = if actor == PlayerId::P0 {
                [ordered.as_slice(), other_library.as_slice()]
            } else {
                [other_library.as_slice(), ordered.as_slice()]
            };
            let mut raw = GameState::new_from_libraries_with_starting_player_v1(
                libraries[0],
                libraries[1],
                crate::rl::card_name,
                110_423,
                actor,
            );
            raw.step = crate::state::Step::Main1;
            for player in [actor, actor.opponent()] {
                put(&mut raw, player, "Forest", Zone::Hand);
                raw.players[player.index()].mana_pool = [12; 6];
            }
            crate::combat_damage_v1::enable_foundations_combat_v1(&mut raw).unwrap();
            put(&mut raw, actor.opponent(), hidden_hand, Zone::Hand);
            let spinner = put(&mut raw, actor, "Spinner of Souls", Zone::Hand);
            let elf = put(&mut raw, actor, "Llanowar Elves", Zone::Battlefield);
            let bolt = put(&mut raw, actor.opponent(), "Lightning Bolt", Zone::Hand);
            let library = raw.players[actor.index()].library.clone();
            let mut stream = own(raw);
            cast(&mut stream, actor, spinner, None);
            drain(&mut stream);
            cast(&mut stream, actor.opponent(), bolt, Some(elf));
            until(&mut stream, |action| {
                matches!(action, ActionSemanticV1::ChooseEffectOption {
                    actor: chooser, source, option_count: 2, ..
                } if *chooser == seat && source.arena_id == spinner.0)
            });
            assert_eq!(state(&stream).objects.get(elf).zone, Zone::Graveyard);
            assert_eq!(state(&stream).objects.get(bolt).zone, Zone::Graveyard);
            assert_eq!(state(&stream).objects.get(spinner).zone, Zone::Battlefield);
            assert_eq!(state(&stream).players[actor.index()].library, library);
            assert_eq!(state(&stream).players[actor.index()].life, 20);
            assert_eq!(state(&stream).players[actor.opponent().index()].life, 20);
            (stream, spinner, library)
        };
        let (mut stream, spinner, library) = build("Forest", "Forest");
        let (mut hidden, hidden_spinner, hidden_library) = build("Mountain", "Island");
        assert_ne!(gameplay(&stream), gameplay(&hidden));
        let decision = stream.encode(expected(&stream)).unwrap();
        let hidden_decision = hidden.encode(expected(&hidden)).unwrap();
        assert_eq!(decision.packet, hidden_decision.packet);
        assert_eq!(decision.packet.source(), Source::PreparedV57);
        assert_eq!(decision.packet.actions().len(), 2);
        assert_ne!(decision.packet.actions()[0], decision.packet.actions()[1]);
        let observation = decision.backend.observation();
        let [root] = observation.spinner.stack.as_slice() else {
            panic!("one genuine Spinner death root must own the choice")
        };
        assert_eq!(
            root.stage,
            crate::spinner_public_v54::StageV54::AwaitingReveal
        );
        assert_eq!(root.root.original_source.object, spinner.0);
        assert_eq!(
            root.root.original_source.generation,
            state(&stream).objects.get(spinner).zone_change_count
        );
        assert_eq!(root.root.captured_controller, seat);
        assert_eq!(
            root.root.current_original_source.as_ref(),
            Some(&root.root.original_source)
        );
        let native_actions = stream.inner.decision_v57().unwrap().actions().to_vec();
        let reveal_index = native_actions
            .iter()
            .position(|action| {
                matches!(action, ActionSemanticV1::ChooseEffectOption {
                actor: chooser, source, option_index: 1, option_count: 2
            } if *chooser == seat && source.arena_id == spinner.0)
            })
            .unwrap();
        assert_eq!(reveal_index, 1);
        let logits = NativeModel::deterministic_fixture()
            .unwrap()
            .forward(decision.packet())
            .unwrap();
        assert_eq!(logits.len(), native_actions.len());
        assert_eq!(logits.len(), decision.packet.actions().len());
        assert!(logits.iter().all(|value| value.is_finite()));
        let saved = stream.snapshot();
        let input = gameplay(&stream);
        let rng_before = *state(&stream).legacy_rng().unwrap();
        let mut forged = decision.clone();
        forged.packet.actions.reverse();
        assert!(stream.apply(&forged, reveal_index as u32).is_err());
        assert_eq!(gameplay(&stream), input);
        let mut erased = decision.clone();
        erased.packet.state = NumericTree::encode(&json!({})).unwrap();
        assert!(stream.apply(&erased, reveal_index as u32).is_err());
        assert!(stream.apply(&decision, 2).is_err());
        assert!(stream.apply(&hidden_decision, reveal_index as u32).is_err());
        assert!(stream.restore(&hidden.snapshot()).is_err());
        assert_eq!(gameplay(&stream), input);
        let check_reveal = |stream: &FixturePrepared, original: &[ObjectId]| {
            let current = state(stream);
            let found = original[2];
            assert_eq!(current.objects.get(found).zone, Zone::Hand);
            assert_eq!(current.objects.get(found).zone_change_count, 1);
            assert!(current.players[actor.index()].hand.contains(&found));
            assert!(current
                .known_hand_cards(actor.opponent(), actor)
                .iter()
                .any(|entry| entry.object == found && entry.zone_change_count == 1));
            let after = &current.players[actor.index()].library;
            assert_eq!(after.len(), original.len() - 1);
            assert_eq!(&after[..after.len() - 2], &original[3..]);
            let mut bottom = after[after.len() - 2..].to_vec();
            let mut expected = original[..2].to_vec();
            bottom.sort();
            expected.sort();
            assert_eq!(bottom, expected);
            for observer in [actor, actor.opponent()] {
                assert!(current.known_library_cards(observer, actor).is_empty());
            }
            assert_ne!(*current.legacy_rng().unwrap(), rng_before);
            stream.assert_native_current();
        };
        stream.apply(&decision, reveal_index as u32).unwrap();
        drain(&mut stream);
        check_reveal(&stream, &library);
        let output = gameplay(&stream);
        let output_rng = *state(&stream).legacy_rng().unwrap();
        let packet = stream.encode(expected(&stream)).unwrap().packet;
        assert!(stream.apply(&decision, reveal_index as u32).is_err());
        assert_eq!(gameplay(&stream), output);
        act(&mut hidden, |action| {
            matches!(action, ActionSemanticV1::ChooseEffectOption {
            actor: chooser, source, option_index: 1, option_count: 2
        } if *chooser == seat && source.arena_id == hidden_spinner.0)
        });
        drain(&mut hidden);
        check_reveal(&hidden, &hidden_library);
        assert_eq!(packet, hidden.encode(expected(&hidden)).unwrap().packet);
        stream.restore(&saved).unwrap();
        assert_eq!(gameplay(&stream), input);
        assert_eq!(*state(&stream).legacy_rng().unwrap(), rng_before);
        assert!(stream.apply(&decision, reveal_index as u32).is_err());
        let replay = stream.encode(expected(&stream)).unwrap();
        assert_eq!(replay.packet, decision.packet);
        stream.apply(&replay, reveal_index as u32).unwrap();
        drain(&mut stream);
        check_reveal(&stream, &library);
        assert_eq!(gameplay(&stream), output);
        assert_eq!(*state(&stream).legacy_rng().unwrap(), output_rng);
        assert_eq!(stream.encode(expected(&stream)).unwrap().packet, packet);
    }
}

#[test]
fn paid_hatcher_real_combat_payment_history_and_exact_width_privacy_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let build = |hidden: &str, wide_value: i32| {
            let mut raw = ready(actor);
            put(&mut raw, actor.opponent(), hidden, Zone::Hand);
            put(&mut raw, actor.opponent(), hidden, Zone::Library);
            let h = put(&mut raw, actor, "Drake Hatcher", Zone::Hand);
            let wide = put(&mut raw, actor, "Forest", Zone::Battlefield);
            // Declared initial noncreature operands before any private input.
            raw.objects.get_mut(wide).counters.soul = wide_value;
            raw.objects.get_mut(wide).counters.plus1_plus1 = wide_value;
            raw.objects.get_mut(wide).counters.incubation = wide_value;
            raw.objects.get_mut(wide).damage = u32::MAX;
            let mut s = own(raw);
            cast(&mut s, actor, h, None);
            drain(&mut s);
            (s, h)
        };
        let (mut s, h) = build("Spinner of Souls", i32::MAX);
        let (other, _) = build("Painful Quandary", i32::MAX);
        assert_ne!(gameplay(&s), gameplay(&other));
        let (narrower, _) = build("Spinner of Souls", i32::MAX - 1);
        assert_ne!(
            s.encode(expected(&s)).unwrap().packet,
            narrower.encode(expected(&narrower)).unwrap().packet
        );
        assert_eq!(
            s.encode(expected(&s)).unwrap().packet,
            other.encode(expected(&other)).unwrap().packet
        );
        for _ in 0..1024 {
            if state(&s).objects.get(h).counters.incubation >= 3 {
                break;
            }
            let encoded = s.encode(expected(&s)).unwrap();
            let d = &encoded.backend;
            let action=d.actions().iter().find(|a| {
                    matches!(a,ActionSemanticV1::ChooseAttackerInclusion{attacker,include:true,..}if attacker.arena_id==h.0)
                        || matches!(a,ActionSemanticV1::DeclareAttackers{attackers,..}if attackers.iter().any(|r|r.arena_id==h.0))
                })
                .or_else(||d.actions().iter().find(|a| quiet_action(a)))
                .or_else(||(d.actions().len()==1).then(||&d.actions()[0]))
                .expect("real combat choice absent").clone();
            act_encoded(&mut s, encoded, |candidate| candidate == &action);
        }
        assert_eq!(state(&s).objects.get(h).counters.incubation, 3);
        drain(&mut s);
        priority(&mut s, actor);
        let d = s.encode(expected(&s)).unwrap();
        let saved = s.snapshot();
        let root = &d.backend.observation().drake;
        assert_eq!(root.combat.len(), 3);
        assert!(root.combat.iter().all(|r| r.final_positive_damage == 1));
        act(
            &mut s,
            |a| matches!(a,ActionSemanticV1::ActivateAbility{source,ability_index:0,..}if source.arena_id==h.0),
        );
        assert_eq!(state(&s).objects.get(h).counters.incubation, 0);
        drain(&mut s);
        assert!(
            matches!(s.inner.decision_v57().unwrap().observation().drake.activations[0].outcome,
            crate::drake_public_v53::ActivationOutcomeV53::Resolved{ref tokens}if tokens.len()==1)
        );
        let result = gameplay(&s);
        let packet = s.encode(expected(&s)).unwrap().packet;
        s.restore(&saved).unwrap();
        assert!(s.apply(&d, 0).is_err());
        act(
            &mut s,
            |a| matches!(a,ActionSemanticV1::ActivateAbility{source,ability_index:0,..}if source.arena_id==h.0),
        );
        drain(&mut s);
        assert_eq!(gameplay(&s), result);
        assert_eq!(s.encode(expected(&s)).unwrap().packet, packet);
    }
}

#[test]
fn paid_phoenix_actual_graveyard_offer_red_payment_and_original_return_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut raw = ready(actor);
        let elenda = put(&mut raw, actor, "Elenda, Saint of Dusk", Zone::Hand);
        let phoenix = put(&mut raw, actor, "Flamewake Phoenix", Zone::Hand);
        let bolt = put(&mut raw, actor.opponent(), "Lightning Bolt", Zone::Hand);
        let red = put(&mut raw, actor, "Mountain", Zone::Battlefield);
        let mut s = own(raw);
        cast(&mut s, actor, elenda, None);
        drain(&mut s);
        cast(&mut s, actor, phoenix, None);
        drain(&mut s);
        cast(&mut s, actor.opponent(), bolt, Some(phoenix));
        drain(&mut s);
        assert_eq!(state(&s).objects.get(phoenix).zone, Zone::Graveyard);
        until(&mut s, |a| {
            matches!(a, ActionSemanticV1::ChooseEffectBoolean { value: true, .. })
        });
        let d = s.encode(expected(&s)).unwrap();
        let saved = s.snapshot();
        assert!(d.backend.observation().phoenix.ferocious_now[actor.index()]);
        assert!(d
            .backend
            .observation()
            .phoenix
            .triggers
            .iter()
            .any(|r| matches!(
                r.stage,
                crate::phoenix_public_v56::StageV56::PaymentOffered { can_pay: true }
            )));
        let before = state(&s).objects.get(phoenix).zone_change_count;
        assert!(!state(&s).objects.get(red).tapped);
        act(&mut s, |a| {
            matches!(a, ActionSemanticV1::ChooseEffectBoolean { value: true, .. })
        });
        drain(&mut s);
        assert_eq!(state(&s).objects.get(phoenix).zone, Zone::Battlefield);
        assert_eq!(state(&s).objects.get(phoenix).controller, actor);
        assert_eq!(state(&s).objects.get(phoenix).zone_change_count, before + 1);
        assert!(state(&s).objects.get(red).tapped);
        let output = gameplay(&s);
        let packet = s.encode(expected(&s)).unwrap().packet;
        s.restore(&saved).unwrap();
        assert!(s.apply(&d, 0).is_err());
        act(&mut s, |a| {
            matches!(a, ActionSemanticV1::ChooseEffectBoolean { value: true, .. })
        });
        drain(&mut s);
        assert_eq!(gameplay(&s), output);
        assert_eq!(s.encode(expected(&s)).unwrap().packet, packet);
    }
}

#[test]
fn typed_public_references_keep_identity_and_reuse_common_ranks_without_raw_stamps() {
    let id = crate::card_def::card_id_by_name("Forest").unwrap();
    let make = |object, generation| {
        json!({
        "common":{"arena_id":object,"card_db_id":id,"owner":"p0","controller":"p1","zone":"Battlefield","zone_change_count":generation},
        "drake":{"incarnation":{"object":object,"generation":generation},"owner":"p0","controller":"p1","zone":"Battlefield","identity":{"Known":{"card_id":id}}},
        "phoenix":{"object":object,"generation":generation,"owner":"p0","controller":"p1","zone":"Battlefield","identity":{"Known":{"card_id":id}}}})
    };
    let encode = |value| {
        normalize::encode(
            Source::PreparedV57,
            value,
            vec![json!({"action_kind":"pass","actor":"p1"})],
        )
        .unwrap()
    };
    let a = encode(make(4, 9));
    let b = encode(make(90, 700));
    assert_eq!(a, b);
    let mut altered = make(4, 9);
    altered["phoenix"]["controller"] = json!("p0");
    assert_ne!(a, encode(altered));
    let mut concealed = make(4, 9);
    concealed["phoenix"]["identity"] = json!({"FaceDown":{"disguised":true}});
    assert_ne!(a, encode(concealed));
    let mut value = make(4, 9);
    references(&mut value).unwrap();
    assert_eq!(value["common"], value["drake"]["public_reference"]);
    assert_eq!(value["common"], value["phoenix"]["public_reference"]);
    assert_eq!(
        value["drake"]["public_identity"],
        json!({"Known":{"card_id":id}})
    );
}

impl crate::current_mixed_policy_v1::tests::ordinary_triggers::Owner for FixturePrepared {
    type Decision = PreparedDecision;
    type Snapshot = FixtureSnapshot;
    fn own(s: GameState) -> Self {
        own(s)
    }
    fn world(&self) -> &GameState {
        self.inner.fixture_state_v57()
    }
    fn counts(&self) -> (u64, u64) {
        self.inner.fixture_counts_v57()
    }
    fn actions(&self) -> Vec<ActionSemanticV1> {
        self.inner.decision_v57().unwrap().actions().to_vec()
    }
    fn encode(&self) -> Self::Decision {
        let e = match self.inner.current_response() {
            FastActorResponseV1::Decision(e) => e,
            other => panic!("real decision absent:{other:?}"),
        };
        self.model.encode(e).unwrap()
    }
    fn packet(d: &Self::Decision) -> &crate::current_mixed_policy_v1::Packet {
        d.packet()
    }
    fn erase(d: &mut Self::Decision) {
        d.packet.state =
            crate::current_mixed_policy_v1::NumericTree::encode(&serde_json::json!({})).unwrap();
    }
    fn apply(&mut self, d: &Self::Decision, index: u32) -> Result<()> {
        FixturePrepared::apply(self, d, index)?;
        Ok(())
    }
    fn snapshot(&self) -> Self::Snapshot {
        FixturePrepared::snapshot(self)
    }
    fn restore(&mut self, s: &Self::Snapshot) -> Result<()> {
        FixturePrepared::restore(self, s)
    }
}
#[test]
fn ashroot_stromkirk_paid_cast_real_target_choice_and_restore_prepared_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid::<FixturePrepared>();
}
#[test]
fn strongbox_gourmand_paid_cast_attack_resolution_and_restore_prepared_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_entry_attack::<FixturePrepared>(
    );
}
#[test]
fn ashroot_stromkirk_private_opponent_printings_do_not_change_packet_prepared_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::privacy::<FixturePrepared>();
}

#[test]
fn memorial_panda_paid_cast_grants_and_gain_death_input_restore_prepared_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_memorial_panda::<FixturePrepared>(
    );
}

#[test]
fn early_printings_paid_ordinary_activations_and_restore_prepared_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_early_activation::<
        FixturePrepared,
    >();
}

#[test]
fn crystal_paid_cast_hexproof_prevention_control_restore_privacy_preparedsession_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_crystal::<FixturePrepared>();
}

#[test]
fn tyrant_herald_paid_cast_damage_flash_restore_privacy_preparedsession_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_tyrant_herald::<FixturePrepared>(
    );
}

#[test]
fn rallier_bloom_paid_cast_private_pick_and_mana_restore_prepared_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_rallier_bloom::<FixturePrepared>(
    );
}

#[test]
fn enchantment_hunter_paid_cast_trigger_restore_privacy_preparedsession_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_enchantment_hunter::<
        FixturePrepared,
    >();
}

#[test]
fn eaten_paid_additional_branches_exile_restore_privacy_preparedsession_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_eaten::<FixturePrepared>();
}

#[test]
fn faebloom_paid_instant_reflexive_season_restore_privacy_preparedsession_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_faebloom::<FixturePrepared>();
}

#[test]
fn kellan_paid_upgrades_combat_impulse_restore_privacy_preparedsession_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_kellan::<FixturePrepared>();
}

#[test]
fn fiery_paid_optional_equipment_death_replacement_restore_privacy_prepared_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_fiery::<FixturePrepared>();
}

#[test]
fn divine_paid_ordinary_kicker_zero_multiple_restore_privacy_prepared_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_divine::<FixturePrepared>();
}
