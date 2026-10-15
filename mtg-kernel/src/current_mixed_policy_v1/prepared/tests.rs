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
fn own(s: GameState) -> PreparedSession {
    PreparedSession::from_session(FastActorSessionV1::from_prepared_v57_fixture_state(s)).unwrap()
}
fn state(s: &PreparedSession) -> &GameState {
    s.inner.fixture_state_v57()
}
fn expected(s: &PreparedSession) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(e) = s.current_response() else {
        panic!("real model prompt absent")
    };
    e
}
fn gameplay(s: &PreparedSession) -> (Vec<u8>, (u64, u64)) {
    (
        serde_json::to_vec(state(s)).unwrap(),
        s.inner.fixture_counts_v57(),
    )
}
// A fresh native Prepared-mode stream owns its own issued menu. It never
// clones or receives the model owner's sealed backend capability. Construction
// from the complete fixture world must preserve that world and exact menu;
// a surface-only prefix that cannot be reconstructed must fail these checks.
struct NativeReference {
    session: FastActorSessionV1,
    current: FastActorDecisionV1,
    original: FastActorDecisionV1,
    actions: Vec<ActionSemanticV1>,
    original_counts: (u64, u64),
    native_counts: (u64, u64),
}
fn native_reference(s: &PreparedSession) -> NativeReference {
    let session = FastActorSessionV1::from_prepared_v57_fixture_state(state(s).clone());
    assert_eq!(state(s), session.prepared_fixture_state());
    let FastActorResponseV1::Decision(current) = session.current_response() else {
        panic!("independent native Prepared reference has no current decision")
    };
    let original = expected(s);
    let (_, actions, _) = session
        .prepared_current_decision_input_v57(current, current.acting_player)
        .unwrap();
    assert_eq!(
        actions.as_slice(),
        s.inner.decision_v57().unwrap().actions()
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
        actions,
        original_counts: s.inner.fixture_counts_v57(),
        native_counts,
    }
}
fn rebase_counter(value: u64, native_before: u64, original_before: u64) -> u64 {
    original_before
        .checked_add(value.checked_sub(native_before).unwrap())
        .unwrap()
}
impl NativeReference {
    fn assert_step(&mut self, s: &PreparedSession, actual: FastActorResponseV1, index: u32) {
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
        assert_eq!(state(s), self.session.prepared_fixture_state());
        assert_eq!(
            s.inner.fixture_counts_v57(),
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
fn act(s: &mut PreparedSession, choose: impl Fn(&ActionSemanticV1) -> bool) {
    let d = s.encode(expected(s)).unwrap();
    assert_eq!(d.packet.source(), Source::PreparedV57);
    let native = s.inner.decision_v57().unwrap();
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
    let mut reference = native_reference(s);
    let response = s.apply(&d, i as u32).unwrap();
    reference.assert_step(s, response, i as u32);
}
fn pass(s: &mut PreparedSession) {
    act(s, |a| matches!(a, ActionSemanticV1::Pass { .. }));
}
fn priority(s: &mut PreparedSession, p: PlayerId) {
    for _ in 0..8 {
        if expected(s).acting_player == p.into() {
            return;
        }
        pass(s);
    }
    panic!("real priority absent")
}
fn drain(s: &mut PreparedSession) {
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
fn cast(s: &mut PreparedSession, p: PlayerId, id: ObjectId, target: Option<ObjectId>) {
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
fn until(s: &mut PreparedSession, choose: impl Fn(&ActionSemanticV1) -> bool) {
    for _ in 0..128 {
        if s.inner
            .decision_v57()
            .unwrap()
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
    assert!(
        matches!(super::super::roots::baseline(horde),Err(Error::UnsupportedPrintedRoot(actual))if actual==horde)
    );
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
            let mut reference = native_reference(&s);
            let (logits, index, response) = s.score_and_apply(&input, &model).unwrap();
            assert_eq!(logits.len(), reference.actions.len());
            assert!(logits.iter().all(|logit| logit.is_finite()));
            reference.assert_step(&s, response, index);
        }
    }
}

#[test]
fn paid_quandary_discard_or_life_and_spinner_reveal_choices_keep_exact_packets_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for discard in [false, true] {
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
            let d = s.inner.decision_v57().unwrap();
            let action=d.actions().iter().find(|a|matches!(a,ActionSemanticV1::DeclareAttackers{attackers,..}if attackers.iter().any(|r|r.arena_id==h.0)))
                .or_else(||d.actions().iter().find(|a|matches!(a,ActionSemanticV1::Pass{..})))
                .or_else(||(d.actions().len()==1).then(||&d.actions()[0]))
                .expect("real combat choice absent").clone();
            act(&mut s, |candidate| candidate == &action);
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

impl crate::current_mixed_policy_v1::tests::ordinary_triggers::Owner for PreparedSession {
    type Decision = PreparedDecision;
    type Snapshot = PreparedSnapshot;
    fn own(s: GameState) -> Self {
        Self::from_session(FastActorSessionV1::from_prepared_v57_fixture_state(s)).unwrap()
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
        PreparedSession::encode(self, e).unwrap()
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
        let actual = PreparedSession::apply(self, d, index)?;
        reference.assert_step(self, actual, index);
        Ok(())
    }
    fn snapshot(&self) -> Self::Snapshot {
        PreparedSession::snapshot(self)
    }
    fn restore(&mut self, s: &Self::Snapshot) -> Result<()> {
        PreparedSession::restore(self, s)
    }
}
#[test]
fn ashroot_stromkirk_paid_cast_real_target_choice_and_restore_prepared_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid::<PreparedSession>();
}
#[test]
fn strongbox_gourmand_paid_cast_attack_resolution_and_restore_prepared_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_entry_attack::<PreparedSession>(
    );
}
#[test]
fn ashroot_stromkirk_private_opponent_printings_do_not_change_packet_prepared_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::privacy::<PreparedSession>();
}

#[test]
fn memorial_panda_paid_cast_grants_and_gain_death_input_restore_prepared_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_memorial_panda::<PreparedSession>(
    );
}

#[test]
fn early_printings_paid_ordinary_activations_and_restore_prepared_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_early_activation::<
        PreparedSession,
    >();
}

#[test]
fn crystal_paid_cast_hexproof_prevention_control_restore_privacy_preparedsession_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_crystal::<PreparedSession>();
}

#[test]
fn tyrant_herald_paid_cast_damage_flash_restore_privacy_preparedsession_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_tyrant_herald::<PreparedSession>(
    );
}

#[test]
fn rallier_bloom_paid_cast_private_pick_and_mana_restore_prepared_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_rallier_bloom::<PreparedSession>(
    );
}

#[test]
fn enchantment_hunter_paid_cast_trigger_restore_privacy_preparedsession_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid_enchantment_hunter::<
        PreparedSession,
    >();
}
