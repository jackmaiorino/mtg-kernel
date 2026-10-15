//! Real paid paths across the four distinct model owners. The independent
//! native session is an assertion oracle, never executable model input.
use super::*;
use crate::{
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    rl::{ActionSemanticV1, TargetRefV1},
    state::{GameState, Step, Zone},
};
use serde_json::{json, Value};

const SOURCES: [Source; 4] = [
    Source::FakeV50,
    Source::RavenousV51,
    Source::ElendaV52,
    Source::PreparedV57,
];
enum Owner {
    Fake(FakeSession),
    Ravenous(RavenousSession),
    Elenda(ElendaSession),
    Prepared(PreparedSession),
}
enum Choice {
    Fake(Box<FakeDecision>),
    Ravenous(Box<RavenousDecision>),
    Elenda(Box<ElendaDecision>),
    Prepared(Box<PreparedDecision>),
}
enum Snapshot {
    Fake(FakeSnapshot),
    Ravenous(RavenousSnapshot),
    Elenda(ElendaSnapshot),
    Prepared(PreparedSnapshot),
}
impl Owner {
    fn from_raw(source: Source, raw: FastActorSessionV1) -> Self {
        match source {
            Source::FakeV50 => Self::Fake(FakeSession::from_session(raw).unwrap()),
            Source::RavenousV51 => Self::Ravenous(RavenousSession::from_session(raw).unwrap()),
            Source::ElendaV52 => Self::Elenda(ElendaSession::from_session(raw).unwrap()),
            Source::PreparedV57 => Self::Prepared(PreparedSession::from_session(raw).unwrap()),
        }
    }
    fn current(&self) -> FastActorResponseV1 {
        match self {
            Self::Fake(s) => s.current_response(),
            Self::Ravenous(s) => s.current_response(),
            Self::Elenda(s) => s.current_response(),
            Self::Prepared(s) => s.current_response(),
        }
    }
    fn encode(&self, e: FastActorDecisionV1) -> Choice {
        match self {
            Self::Fake(s) => Choice::Fake(Box::new(s.encode(e).unwrap())),
            Self::Ravenous(s) => Choice::Ravenous(Box::new(s.encode(e).unwrap())),
            Self::Elenda(s) => Choice::Elenda(Box::new(s.encode(e).unwrap())),
            Self::Prepared(s) => Choice::Prepared(Box::new(s.encode(e).unwrap())),
        }
    }
    fn apply(&mut self, d: &Choice, index: u32) -> Result<FastActorResponseV1> {
        match (self, d) {
            (Self::Fake(s), Choice::Fake(d)) => s.apply(d, index),
            (Self::Ravenous(s), Choice::Ravenous(d)) => s.apply(d, index),
            (Self::Elenda(s), Choice::Elenda(d)) => s.apply(d, index),
            (Self::Prepared(s), Choice::Prepared(d)) => s.apply(d, index),
            _ => Err(Error::StaleBinding),
        }
    }
    fn snapshot(&self) -> Snapshot {
        match self {
            Self::Fake(s) => Snapshot::Fake(s.snapshot()),
            Self::Ravenous(s) => Snapshot::Ravenous(s.snapshot()),
            Self::Elenda(s) => Snapshot::Elenda(s.snapshot()),
            Self::Prepared(s) => Snapshot::Prepared(s.snapshot()),
        }
    }
    fn restore(&mut self, snapshot: &Snapshot) -> Result<()> {
        match (self, snapshot) {
            (Self::Fake(s), Snapshot::Fake(v)) => s.restore(v),
            (Self::Ravenous(s), Snapshot::Ravenous(v)) => s.restore(v),
            (Self::Elenda(s), Snapshot::Elenda(v)) => s.restore(v),
            (Self::Prepared(s), Snapshot::Prepared(v)) => s.restore(v),
            _ => Err(Error::ForeignSnapshot),
        }
    }
}
impl Choice {
    fn packet(&self) -> &Packet {
        match self {
            Self::Fake(d) => d.packet(),
            Self::Ravenous(d) => d.packet(),
            Self::Elenda(d) => d.packet(),
            Self::Prepared(d) => d.packet(),
        }
    }
}
struct Harness {
    source: Source,
    owner: Owner,
    native: FastActorSessionV1,
}
struct Saved {
    owner: Snapshot,
    native: FastActorSessionSnapshotV1,
    packet: Packet,
}
impl Harness {
    fn new(source: Source, state: GameState) -> Self {
        let native = match source {
            Source::FakeV50 => FastActorSessionV1::from_fake_v50_fixture_state(state),
            Source::RavenousV51 => FastActorSessionV1::from_ravenous_v51_fixture_state(state),
            Source::ElendaV52 => FastActorSessionV1::from_elenda_v52_fixture_state(state),
            Source::PreparedV57 => FastActorSessionV1::from_prepared_v57_fixture_state(state),
        };
        Self {
            source,
            owner: Owner::from_raw(source, native.clone()),
            native,
        }
    }
    fn state(&self) -> &GameState {
        match self.source {
            Source::FakeV50 => self.native.fake_fixture_state(),
            Source::RavenousV51 => self.native.ravenous_fixture_state(),
            Source::ElendaV52 => self.native.elenda_fixture_state(),
            Source::PreparedV57 => self.native.prepared_fixture_state(),
        }
    }
    fn expected(&self) -> FastActorDecisionV1 {
        let response = self.owner.current();
        assert_eq!(response, self.native.current_response());
        let FastActorResponseV1::Decision(e) = response else {
            panic!("actual model choice absent:{response:?}")
        };
        e
    }
    fn input(&self) -> (Value, Vec<ActionSemanticV1>) {
        let e = self.expected();
        macro_rules! input {
            ($getter:ident) => {{
                let (value, actions, _) = self.native.$getter(e, e.acting_player).unwrap();
                (serde_json::to_value(value).unwrap(), actions)
            }};
        }
        match self.source {
            Source::FakeV50 => input!(fake_current_decision_input_v50),
            Source::RavenousV51 => input!(ravenous_current_decision_input_v51),
            Source::ElendaV52 => input!(elenda_current_decision_input_v52),
            Source::PreparedV57 => input!(prepared_current_decision_input_v57),
        }
    }
    fn packet(&self) -> Packet {
        let (value, actions) = self.input();
        let d = self.owner.encode(self.expected());
        assert_eq!(d.packet().source(), self.source);
        assert_eq!(d.packet().actions().len(), actions.len());
        let canonical = normalize::encode(
            self.source,
            value,
            actions
                .iter()
                .map(|v| serde_json::to_value(v).unwrap())
                .collect(),
        )
        .unwrap();
        assert_eq!(d.packet(), &canonical);
        d.packet().clone()
    }
    fn act(&mut self, choose: impl Fn(&ActionSemanticV1) -> bool) {
        let (_, actions) = self.input();
        let index = actions
            .iter()
            .position(choose)
            .unwrap_or_else(|| panic!("native action absent:{actions:?}"));
        let e = self.expected();
        let decision = self.owner.encode(e);
        assert_eq!(decision.packet(), &self.packet());
        let scores = NativeModel::deterministic_fixture()
            .unwrap()
            .forward(decision.packet())
            .unwrap();
        assert_eq!(scores.len(), actions.len());
        assert!(scores.iter().all(|v| v.is_finite()));
        let reference = self
            .native
            .step(e.episode_id, e.step, index as u32)
            .unwrap();
        assert_eq!(
            self.owner.apply(&decision, index as u32).unwrap(),
            reference
        );
        self.packet();
    }
    fn save(&self) -> Saved {
        Saved {
            owner: self.owner.snapshot(),
            native: self.native.snapshot_v1(),
            packet: self.packet(),
        }
    }
    fn restore(&mut self, saved: &Saved) {
        let stale = self.owner.encode(self.expected());
        let old = self.expected();
        self.owner.restore(&saved.owner).unwrap();
        self.native.restore_v1(&saved.native);
        assert!(self.expected().environment_revision > old.environment_revision);
        assert!(matches!(
            self.owner.apply(&stale, 0),
            Err(Error::StaleBinding)
        ));
        assert_eq!(self.packet(), saved.packet);
    }
    fn gameplay(&self) -> (Vec<u8>, u64, u64) {
        (
            serde_json::to_vec(self.state()).unwrap(),
            self.native.policy_step_count(),
            self.native.physical_decision_count(),
        )
    }
    fn drain(&mut self, keep: Option<ObjectId>) {
        for _ in 0..64 {
            let s = self.state();
            if s.stack.is_empty()
                && s.engine.pending_cast.is_none()
                && s.engine.pending_activation.is_none()
                && s.engine.pending_effect.is_none()
                && s.engine.pending_triggers.is_empty()
                && s.pending_legend_rule_v1.is_none()
            {
                return;
            }
            self.act(|a| match a {
                ActionSemanticV1::Pass { .. } | ActionSemanticV1::OrderTriggers { .. } => true,
                ActionSemanticV1::ChooseLegendPermanent { keep: card, .. } => {
                    keep == Some(ObjectId(card.arena_id))
                }
                _ => false,
            });
        }
        panic!("real owned work did not finish")
    }
    fn mana(&mut self, id: ObjectId) {
        self.act(|a|matches!(a,ActionSemanticV1::ActivateManaAbility{source,..}if source.arena_id==id.0));
    }
    fn cast(&mut self, id: ObjectId) {
        self.act(|a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==id.0));
    }
    fn next_main(&mut self, actor: PlayerId) {
        let before = self.state().turn_key_v1();
        for _ in 0..160 {
            let s = self.state();
            if s.turn_key_v1() != before
                && s.step == Step::Main1
                && s.active_player == actor
                && self.expected().acting_player == actor.into()
            {
                return;
            }
            self.act(|a|matches!(a,ActionSemanticV1::Pass{..})||matches!(a,ActionSemanticV1::DeclareAttackers{attackers,..}if attackers.is_empty())||matches!(a,ActionSemanticV1::DeclareBlockersForAttacker{blockers,..}if blockers.is_empty()));
        }
        panic!("actual next main absent")
    }
}
fn ready(actor: PlayerId) -> GameState {
    let mut s = crate::extra_turn_v1::tests::ready(actor);
    for p in [actor, actor.opponent()] {
        put(&mut s, p, "Forest", Zone::Hand);
    }
    s
}
fn field_count(t: &NumericTree, key: &str) -> usize {
    let rows = t.rows();
    rows.iter()
        .enumerate()
        .filter(|(at, r)| {
            r[0] == 9
                && rows.get(at + 1..at + 1 + key.len()).is_some_and(|v| {
                    v.iter()
                        .zip(key.bytes())
                        .all(|(v, b)| v[0] == 8 && v[1] == u16::from(b))
                })
        })
        .count()
}
fn cats(s: &GameState, p: PlayerId) -> Vec<ObjectId> {
    let id = crate::card_def::card_id_by_name("Cat Token").unwrap();
    s.players[p.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|id_| s.objects.get(*id_).card_def == id)
        .collect()
}

#[test]
fn exact_legendary_printing_roots_keep_ordinary_costs_static_bonus_ward_and_permission() {
    for (name, generic, color, roots, activations) in [
        ("Arahbo, the First Fang", 2, "W", 2, 0),
        ("Zul Ashur, Lich Lord", 1, "B", 3, 1),
    ] {
        let id = crate::card_def::card_id_by_name(name).unwrap();
        let value = roots::registered_permanent_printing(id).unwrap().unwrap();
        let p = &value["registered_permanent_printing"];
        let cast = &p["ordered_cast_roots"][0];
        assert_eq!(cast["ordinary_origin"], "Hand");
        assert_eq!(cast["timing"], "CasterMainPhaseEmptyStackWithPriority");
        assert_eq!(cast["targets"], "None");
        assert_eq!(
            cast["ordered_costs"][0]["PrintedMana"],
            json!({"generic":generic,"pips":[{"Colored":color}],"x_count":0})
        );
        assert_eq!(
            cast["ordered_resolution"],
            json!(["MoveOriginalSpellFromStackToBattlefieldUnderCasterControl"])
        );
        assert_eq!(p["canonical_family_roots"].as_array().unwrap().len(), roots);
        assert_eq!(
            p["characteristics"]["printed_activated_abilities"],
            activations
        );
        assert_eq!(p["characteristics"]["power"], 2);
        assert_eq!(p["characteristics"]["toughness"], 2);
        if activations == 0 {
            assert_eq!(
                p["ordered_static_rules"][0],
                json!({"OtherControlledEffectiveCatCreatures":{"power":1,"toughness":1,"exclude_source":true}})
            );
            assert!(p["printed_activation"].is_null());
        } else {
            let a = &p["printed_activation"];
            assert_eq!(a["ordered_costs"], json!(["TapOriginalSource"]));
            assert_eq!(a["targets"], "OneOwnGraveyardEffectiveZombieCreatureCard");
            assert_eq!(a["activation_zone"], "Battlefield");
            assert_eq!(
                p["ordered_static_rules"][0],
                json!({"OpponentSpellOrAbilityTargetsSourcePayLifeOrCounter":{"life":2}})
            );
            assert_eq!(
                a["ordered_resolution"][0]["CastOriginalTargetFromOwnGraveyardThisTurn"]
                    ["ordinary_cast_timing_and_all_costs"],
                true
            );
        }
        assert!(roots::baseline(id).is_err());
    }
}

#[test]
fn genuine_paid_arahbo_entry_other_cat_static_and_owned_restore_all_four_modes_both_seats() {
    for source in SOURCES {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut raw = ready(actor);
            let arahbo = put(&mut raw, actor, "Arahbo, the First Fang", Zone::Hand);
            let lions = put(&mut raw, actor, "Savannah Lions", Zone::Hand);
            let lands = (0..4)
                .map(|_| put(&mut raw, actor, "Plains", Zone::Battlefield))
                .collect::<Vec<_>>();
            let mut h = Harness::new(source, raw);
            assert_eq!(
                field_count(h.packet().state(), "registered_permanent_printing"),
                1
            );
            for id in &lands[..3] {
                h.mana(*id);
            }
            h.cast(arahbo);
            assert_eq!(h.state().players[actor.index()].mana_pool, [0; 6]);
            assert!(h.state().stack.iter().any(|item| item.source == arahbo));
            let paid = h.save();
            h.drain(None);
            let entered = h.gameplay();
            h.restore(&paid);
            h.drain(None);
            assert_eq!(h.gameplay(), entered);
            assert_eq!(h.state().objects.get(arahbo).zone, Zone::Battlefield);
            assert_eq!(
                crate::engine::checked_effective_pt(h.state(), arahbo).unwrap(),
                (2, 2)
            );
            assert_eq!(cats(h.state(), actor).len(), 1);
            assert_eq!(
                crate::engine::checked_effective_pt(h.state(), cats(h.state(), actor)[0]).unwrap(),
                (2, 2)
            );
            h.mana(lands[3]);
            let before = h.save();
            h.cast(lions);
            h.drain(None);
            assert_eq!(cats(h.state(), actor).len(), 2);
            assert_eq!(
                crate::engine::checked_effective_pt(h.state(), lions).unwrap(),
                (3, 2)
            );
            assert_eq!(h.state().players[actor.index()].mana_pool, [0; 6]);
            let played = h.gameplay();
            let final_packet = h.packet();
            // A separate owner wrapping the same actual native core is foreign.
            let mut foreign = Owner::from_raw(source, h.native.clone());
            let old = foreign.encode(h.expected());
            assert!(matches!(
                foreign.restore(&before.owner),
                Err(Error::ForeignSnapshot)
            ));
            assert_eq!(foreign.encode(h.expected()).packet(), old.packet());
            h.restore(&before);
            h.cast(lions);
            h.drain(None);
            assert_eq!(h.gameplay(), played);
            assert_eq!(h.packet(), final_packet);
        }
    }
}

#[test]
fn genuine_paid_zul_tap_target_permission_graveyard_cast_legend_and_restore_all_four_modes_both_seats(
) {
    for source in SOURCES {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut raw = ready(actor);
            let zul = put(&mut raw, actor, "Zul Ashur, Lich Lord", Zone::Hand);
            // Real physical graveyard cards are declared initial fixture operands.
            // Their prior casting/death is not claimed by this permission test.
            let target = put(&mut raw, actor, "Zul Ashur, Lich Lord", Zone::Graveyard);
            let spare = put(&mut raw, actor, "Zul Ashur, Lich Lord", Zone::Graveyard);
            let foreign = put(
                &mut raw,
                actor.opponent(),
                "Zul Ashur, Lich Lord",
                Zone::Graveyard,
            );
            let forest = put(&mut raw, actor, "Forest", Zone::Battlefield);
            let swamp = put(&mut raw, actor, "Swamp", Zone::Battlefield);
            let mut h = Harness::new(source, raw);
            assert_eq!(
                field_count(h.packet().state(), "registered_permanent_printing"),
                1
            );
            h.mana(forest);
            h.mana(swamp);
            h.cast(zul);
            h.drain(None);
            assert_eq!(h.state().players[actor.index()].mana_pool, [0; 6]);
            h.next_main(actor);
            assert!(!h.state().objects.get(zul).summoning_sick);
            h.act(|a|matches!(a,ActionSemanticV1::ActivateAbility{source,ability_index:0,..}if source.arena_id==zul.0));
            let (_, actions) = h.input();
            for id in [target, spare] {
                assert!(actions.iter().any(|a|matches!(a,ActionSemanticV1::ChooseTarget{target:TargetRefV1::Object{object},..}if object.arena_id==id.0)));
            }
            assert!(!actions.iter().any(|a|matches!(a,ActionSemanticV1::ChooseTarget{target:TargetRefV1::Object{object},..}if object.arena_id==foreign.0)));
            let pending = h.save();
            let generation = h.state().objects.get(target).zone_change_count;
            let play = |h: &mut Harness| {
                h.act(|a|matches!(a,ActionSemanticV1::ChooseTarget{target:TargetRefV1::Object{object},..}if object.arena_id==target.0));
                h.drain(None);
                assert!(h.state().objects.get(zul).tapped);
                assert!(crate::zul_ashur_v1::permission(
                    h.state(),
                    target,
                    actor,
                    generation
                ));
                assert!(!crate::zul_ashur_v1::permission(
                    h.state(),
                    target,
                    actor.opponent(),
                    generation
                ));
                h.mana(forest);
                h.mana(swamp);
                let (_, actions) = h.input();
                assert!(actions.iter().any(|a|matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==target.0&&source.zone==Zone::Graveyard)));
                h.cast(target);
                assert_eq!(h.state().players[actor.index()].mana_pool, [0; 6]);
                h.drain(Some(target));
                assert_eq!(h.state().objects.get(target).zone, Zone::Battlefield);
                assert_eq!(h.state().objects.get(zul).zone, Zone::Graveyard);
                assert_eq!(h.state().objects.get(target).controller, actor);
                assert_eq!(h.state().objects.get(spare).zone, Zone::Graveyard);
                assert!(!crate::zul_ashur_v1::permission(
                    h.state(),
                    target,
                    actor,
                    h.state().objects.get(target).zone_change_count
                ));
            };
            play(&mut h);
            let played = h.gameplay();
            let packet = h.packet();
            h.restore(&pending);
            play(&mut h);
            assert_eq!(h.gameplay(), played);
            assert_eq!(h.packet(), packet);
        }
    }
}

#[test]
fn legendary_supplements_follow_actor_knowledge_without_unknown_hand_or_face_down_identity_leak_all_modes(
) {
    for source in SOURCES {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for known in [false, true] {
                let build = |hidden: &str| {
                    let mut raw = ready(actor);
                    if known {
                        put(&mut raw, actor, "Arahbo, the First Fang", Zone::Hand);
                        put(&mut raw, actor, "Zul Ashur, Lich Lord", Zone::Hand);
                    }
                    put(&mut raw, actor.opponent(), hidden, Zone::Hand);
                    let concealed = put(&mut raw, actor.opponent(), hidden, Zone::Battlefield);
                    raw.objects.get_mut(concealed).v4.face_down_v1 =
                        Some(crate::state::FaceDownV1 {
                            disguised: true,
                            lookers: 0,
                            hidden_by: None,
                            hidden_by_source: None,
                        });
                    Harness::new(source, raw)
                };
                let a = build("Arahbo, the First Fang");
                let b = build("Zul Ashur, Lich Lord");
                assert_eq!(a.packet(), b.packet());
                assert_eq!(
                    field_count(a.packet().state(), "registered_permanent_printing"),
                    if known { 2 } else { 0 }
                );
            }
        }
    }
}
