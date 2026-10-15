//! Shared genuine native paths. Adapters live beside each sealed owner.
use super::super::*;
use crate::{
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    rl::{ActionSemanticV1, TargetRefV1},
    state::{GameState, Zone},
};
mod crystal;
mod early_activation;
mod eaten;
mod enchantment_hunter;
mod entry_attack;
mod faebloom;
mod kellan;
mod memorial_panda;
mod rallier_bloom;
mod tyrant_herald;

pub(in crate::current_mixed_policy_v1) fn paid_rallier_bloom<O: Owner>() {
    rallier_bloom::paid::<O>();
}

pub(in crate::current_mixed_policy_v1) fn paid_entry_attack<O: Owner>() {
    entry_attack::paid::<O>();
}
pub(in crate::current_mixed_policy_v1) fn paid_memorial_panda<O: Owner>() {
    memorial_panda::paid::<O>();
}
pub(in crate::current_mixed_policy_v1) fn paid_crystal<O: Owner>() {
    crystal::paid::<O>();
    crystal::privacy::<O>();
}
pub(in crate::current_mixed_policy_v1) fn paid_tyrant_herald<O: Owner>() {
    tyrant_herald::paid::<O>();
    tyrant_herald::privacy::<O>();
}

pub(in crate::current_mixed_policy_v1) fn paid_early_activation<O: Owner>() {
    early_activation::paid::<O>();
}

pub(in crate::current_mixed_policy_v1) trait Owner:
    Sized
{
    type Decision: Clone;
    type Snapshot;
    fn own(state: GameState) -> Self;
    fn world(&self) -> &GameState;
    fn counts(&self) -> (u64, u64);
    fn actions(&self) -> Vec<ActionSemanticV1>;
    fn encode(&self) -> Self::Decision;
    fn packet(d: &Self::Decision) -> &Packet;
    fn erase(d: &mut Self::Decision);
    fn apply(&mut self, d: &Self::Decision, index: u32) -> Result<()>;
    fn snapshot(&self) -> Self::Snapshot;
    fn restore(&mut self, s: &Self::Snapshot) -> Result<()>;
}
fn fact_count(tree: &NumericTree, key: &str) -> usize {
    let rows = tree.rows();
    rows.iter()
        .enumerate()
        .filter(|(i, row)| {
            row[0] == 9
                && row[1] as usize == key.len()
                && key.bytes().enumerate().all(|(n, b)| {
                    rows.get(i + n + 1)
                        .is_some_and(|r| r[0] == 8 && r[1] == u16::from(b))
                })
        })
        .count()
}
fn gameplay<O: Owner>(s: &O) -> (Vec<u8>, (u64, u64)) {
    (serde_json::to_vec(s.world()).unwrap(), s.counts())
}
fn step<O: Owner>(s: &mut O, choose: impl Fn(&ActionSemanticV1) -> bool) {
    let actions = s.actions();
    let i = actions
        .iter()
        .position(choose)
        .unwrap_or_else(|| panic!("genuine native action absent:{actions:?}"));
    let d = s.encode();
    assert_eq!(O::packet(&d).actions().len(), actions.len());
    let logits = NativeModel::deterministic_fixture()
        .unwrap()
        .forward(O::packet(&d))
        .unwrap();
    assert_eq!(logits.len(), actions.len());
    assert!(logits.iter().all(|v| v.is_finite()));
    s.apply(&d, i as u32).unwrap();
}
fn pass<O: Owner>(s: &mut O) {
    step(s, |a| matches!(a, ActionSemanticV1::Pass { .. }));
}
fn drain<O: Owner>(s: &mut O) {
    for _ in 0..64 {
        let w = s.world();
        if w.stack.is_empty()
            && w.engine.pending_cast.is_none()
            && w.engine.pending_effect.is_none()
            && w.engine.pending_triggers.is_empty()
        {
            return;
        }
        pass(s);
    }
    panic!("actual paid spell/trigger did not finish")
}
fn target(a: &ActionSemanticV1, id: ObjectId) -> bool {
    matches!(a,ActionSemanticV1::ChooseTarget{target:TargetRefV1::Object{object},..}if object.arena_id==id.0)
}
fn cast<O: Owner>(s: &mut O, id: ObjectId) {
    for _ in 0..8 {
        if s.actions()
            .iter()
            .any(|a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==id.0))
        {
            break;
        }
        pass(s);
    }
    step(
        s,
        |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==id.0),
    );
}
fn initial(
    actor: PlayerId,
    name: &str,
) -> (
    GameState,
    ObjectId,
    ObjectId,
    Vec<ObjectId>,
    Option<ObjectId>,
) {
    let mut raw = crate::extra_turn_v1::tests::ready(actor);
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut raw).unwrap();
    let source = put(&mut raw, actor, name, Zone::Hand);
    let ash = name == "Ashroot Animist";
    let recipient = if ash {
        put(&mut raw, actor, "Llanowar Elves", Zone::Battlefield)
    } else {
        source
    };
    let names = if ash {
        vec!["Mountain", "Forest", "Forest", "Forest"]
    } else {
        vec!["Swamp", "Forest", "Forest", "Mountain"]
    };
    let lands = names
        .into_iter()
        .map(|n| put(&mut raw, actor, n, Zone::Battlefield))
        .collect();
    let bolt = (!ash).then(|| put(&mut raw, actor, "Lightning Bolt", Zone::Hand));
    (raw, source, recipient, lands, bolt)
}
pub(in crate::current_mixed_policy_v1) fn paid<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for name in ["Ashroot Animist", "Stromkirk Bloodthief"] {
            let ash = name == "Ashroot Animist";
            let (raw, source, recipient, lands, bolt) = initial(actor, name);
            let mut s = O::own(raw);
            // Both catalog family roots already mark these IDs covered. Referenced
            // own hand still requires its independent complete printing supplement.
            let initial = s.encode();
            assert_eq!(
                fact_count(O::packet(&initial).state(), "registered_permanent_printing"),
                1
            );
            let due = if ash { 4 } else { 3 };
            for id in &lands[..due] {
                step(
                    &mut s,
                    |a| matches!(a,ActionSemanticV1::ActivateManaAbility{source,..}if source.arena_id==id.0),
                );
            }
            let original_generation = s.world().objects.get(source).zone_change_count;
            cast(&mut s, source);
            assert_eq!(s.world().players[actor.index()].mana_pool, [0; 6]);
            assert!(lands[..due]
                .iter()
                .all(|id| s.world().objects.get(*id).tapped));
            assert_eq!(s.world().objects.get(source).zone, Zone::Stack);
            let paid = s.snapshot();
            drain(&mut s);
            assert_eq!(s.world().objects.get(source).zone, Zone::Battlefield);
            assert!(s.world().objects.get(source).summoning_sick);
            assert_eq!(s.world().objects.get(source).controller, actor);
            assert_eq!(
                s.world().objects.get(source).zone_change_count,
                original_generation + 2
            );
            let entry = gameplay(&s);
            let entry_packet = O::packet(&s.encode()).clone();
            s.restore(&paid).unwrap();
            drain(&mut s);
            assert_eq!(gameplay(&s), entry);
            assert_eq!(O::packet(&s.encode()), &entry_packet);
            if let Some(bolt) = bolt {
                for _ in 0..8 {
                    if s.actions().iter().any(|a|matches!(a,ActionSemanticV1::ActivateManaAbility{source,..}if source.arena_id==lands[3].0)){break;}
                    pass(&mut s);
                }
                step(
                    &mut s,
                    |a| matches!(a,ActionSemanticV1::ActivateManaAbility{source,..}if source.arena_id==lands[3].0),
                );
                cast(&mut s, bolt);
                step(
                    &mut s,
                    |a| matches!(a,ActionSemanticV1::ChooseTarget{target:TargetRefV1::Player{player},..}if *player==actor.opponent().into()),
                );
                drain(&mut s);
                assert_eq!(s.world().players[actor.opponent().index()].life, 17);
            }
            // Follow real turns, declaring only the paid Animist when eligible.
            // Bloodthief instead sees the actual committed Bolt damage at end step.
            for _ in 0..512 {
                let actions = s.actions();
                if actions.iter().any(|a| target(a, recipient)) {
                    break;
                }
                let next=actions.iter().find(|a|ash && matches!(a,ActionSemanticV1::DeclareAttackers{attackers,..}if attackers.len()==1 && attackers[0].arena_id==source.0))
                .or_else(||actions.iter().find(|a|matches!(a,ActionSemanticV1::Pass{..})))
                .or_else(||actions.iter().find(|a|matches!(a,ActionSemanticV1::DeclareAttackers{attackers,..}if attackers.is_empty())))
                .or_else(||(actions.len()==1).then(||&actions[0])).unwrap_or_else(||panic!("unexpected actual choice:{actions:?}")).clone();
                step(&mut s, |a| a == &next);
            }
            let actions = s.actions();
            let i = actions
                .iter()
                .position(|a| target(a, recipient))
                .expect("real family placement absent");
            let d = s.encode();
            let prompt = s.snapshot();
            let before = gameplay(&s);
            assert_eq!(
                fact_count(O::packet(&d).state(), "registered_permanent_printing"),
                1
            );
            let mut erased = d.clone();
            O::erase(&mut erased);
            assert!(s.apply(&erased, i as u32).is_err());
            assert_eq!(gameplay(&s), before);
            let mut result = None;
            for replay in 0..2 {
                if replay != 0 {
                    s.restore(&prompt).unwrap();
                    let before = gameplay(&s);
                    assert!(matches!(s.apply(&d, i as u32), Err(Error::StaleBinding)));
                    assert_eq!(gameplay(&s), before);
                    assert_eq!(O::packet(&s.encode()), O::packet(&d));
                }
                step(&mut s, |a| target(a, recipient));
                assert_eq!(s.world().stack.len(), 1);
                assert_eq!(
                    s.world().stack[0].kind,
                    crate::state::StackItemKind::TriggeredAbility
                );
                assert_eq!(s.world().stack[0].controller, actor);
                assert_eq!(s.world().stack[0].source, source);
                let published = s.snapshot();
                drain(&mut s);
                if ash {
                    assert_eq!(crate::engine::effective_power(s.world(), recipient), 5);
                    assert!(crate::engine::has_effective_keyword(
                        s.world(),
                        recipient,
                        crate::card_def::Keywords::TRAMPLE
                    ));
                    assert_eq!(s.world().objects.get(recipient).counters.plus1_plus1, 0);
                } else {
                    assert_eq!(s.world().objects.get(recipient).counters.plus1_plus1, 1);
                }
                let output = (gameplay(&s), O::packet(&s.encode()).clone());
                s.restore(&published).unwrap();
                drain(&mut s);
                assert_eq!((gameplay(&s), O::packet(&s.encode()).clone()), output);
                if let Some(prior) = &result {
                    assert_eq!(&output, prior);
                } else {
                    result = Some(output);
                }
            }
        }
    }
}
pub(in crate::current_mixed_policy_v1) fn privacy<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for name in [
            "Ashroot Animist",
            "Stromkirk Bloodthief",
            "Strongbox Raider",
            "Vampire Gourmand",
            "Akroma's Memorial",
            "Fiendish Panda",
            "Scavenging Ooze",
            "Heartfire Immolator",
            "Soul-Shackled Zombie",
            "Squad Rallier",
            "Bloom Tender",
        ] {
            for known in [false, true] {
                let mut a = crate::extra_turn_v1::tests::ready(actor);
                if known {
                    put(&mut a, actor, name, Zone::Hand);
                }
                let hidden = put(&mut a, actor.opponent(), name, Zone::Hand);
                let mut b = a.clone();
                b.objects.get_mut(hidden).card_def =
                    crate::card_def::card_id_by_name("Llanowar Elves").unwrap();
                b.objects.get_mut(hidden).name = "Llanowar Elves".into();
                let a = O::own(a);
                let b = O::own(b);
                let pa = a.encode();
                let pb = b.encode();
                assert_eq!(O::packet(&pa), O::packet(&pb));
                assert_eq!(
                    fact_count(O::packet(&pa).state(), "registered_permanent_printing"),
                    usize::from(known)
                );
            }
        }
    }
}

impl Owner for FakeSession {
    type Decision = FakeDecision;
    type Snapshot = FakeSnapshot;
    fn own(s: GameState) -> Self {
        Self::from_session(FastActorSessionV1::from_fake_v50_fixture_state(s)).unwrap()
    }
    fn world(&self) -> &GameState {
        self.inner.fake_fixture_state()
    }
    fn counts(&self) -> (u64, u64) {
        (
            self.inner.policy_step_count(),
            self.inner.physical_decision_count(),
        )
    }
    fn actions(&self) -> Vec<ActionSemanticV1> {
        let e = match self.inner.current_response() {
            FastActorResponseV1::Decision(e) => e,
            other => panic!("real decision absent:{other:?}"),
        };
        self.inner
            .fake_current_decision_input_v50(e, e.acting_player)
            .unwrap()
            .1
    }
    fn encode(&self) -> Self::Decision {
        let e = match self.inner.current_response() {
            FastActorResponseV1::Decision(e) => e,
            other => panic!("real decision absent:{other:?}"),
        };
        FakeSession::encode(self, e).unwrap()
    }
    fn packet(d: &Self::Decision) -> &Packet {
        d.packet()
    }
    fn erase(d: &mut Self::Decision) {
        d.packet.state =
            crate::current_mixed_policy_v1::NumericTree::encode(&serde_json::json!({})).unwrap();
    }
    fn apply(&mut self, d: &Self::Decision, index: u32) -> Result<()> {
        let mut reference = self.inner.clone();
        let e = match self.inner.current_response() {
            FastActorResponseV1::Decision(e) => e,
            other => panic!("real decision absent:{other:?}"),
        };
        let actual = FakeSession::apply(self, d, index)?;
        assert_eq!(actual, reference.step(e.episode_id, e.step, index).unwrap());
        assert_eq!(
            self.inner.fake_fixture_state(),
            reference.fake_fixture_state()
        );
        Ok(())
    }
    fn snapshot(&self) -> Self::Snapshot {
        FakeSession::snapshot(self)
    }
    fn restore(&mut self, s: &Self::Snapshot) -> Result<()> {
        FakeSession::restore(self, s)
    }
}
#[test]
fn ashroot_stromkirk_paid_cast_real_target_choice_and_restore_fake_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid::<FakeSession>();
}
#[test]
fn strongbox_gourmand_paid_cast_attack_resolution_and_restore_fake_both_seats() {
    paid_entry_attack::<FakeSession>();
}
#[test]
fn memorial_panda_paid_cast_grants_and_gain_death_input_restore_fake_both_seats() {
    paid_memorial_panda::<FakeSession>();
}
#[test]
fn ashroot_stromkirk_private_opponent_printings_do_not_change_packet_fake_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::privacy::<FakeSession>();
}

impl Owner for RavenousSession {
    type Decision = RavenousDecision;
    type Snapshot = RavenousSnapshot;
    fn own(s: GameState) -> Self {
        Self::from_session(FastActorSessionV1::from_ravenous_v51_fixture_state(s)).unwrap()
    }
    fn world(&self) -> &GameState {
        self.inner.ravenous_fixture_state()
    }
    fn counts(&self) -> (u64, u64) {
        (
            self.inner.policy_step_count(),
            self.inner.physical_decision_count(),
        )
    }
    fn actions(&self) -> Vec<ActionSemanticV1> {
        let e = match self.inner.current_response() {
            FastActorResponseV1::Decision(e) => e,
            other => panic!("real decision absent:{other:?}"),
        };
        self.inner
            .ravenous_current_decision_input_v51(e, e.acting_player)
            .unwrap()
            .1
    }
    fn encode(&self) -> Self::Decision {
        let e = match self.inner.current_response() {
            FastActorResponseV1::Decision(e) => e,
            other => panic!("real decision absent:{other:?}"),
        };
        RavenousSession::encode(self, e).unwrap()
    }
    fn packet(d: &Self::Decision) -> &Packet {
        d.packet()
    }
    fn erase(d: &mut Self::Decision) {
        d.packet.state =
            crate::current_mixed_policy_v1::NumericTree::encode(&serde_json::json!({})).unwrap();
    }
    fn apply(&mut self, d: &Self::Decision, index: u32) -> Result<()> {
        let mut reference = self.inner.clone();
        let e = match self.inner.current_response() {
            FastActorResponseV1::Decision(e) => e,
            other => panic!("real decision absent:{other:?}"),
        };
        let actual = RavenousSession::apply(self, d, index)?;
        assert_eq!(actual, reference.step(e.episode_id, e.step, index).unwrap());
        assert_eq!(
            self.inner.ravenous_fixture_state(),
            reference.ravenous_fixture_state()
        );
        Ok(())
    }
    fn snapshot(&self) -> Self::Snapshot {
        RavenousSession::snapshot(self)
    }
    fn restore(&mut self, s: &Self::Snapshot) -> Result<()> {
        RavenousSession::restore(self, s)
    }
}
#[test]
fn ashroot_stromkirk_paid_cast_real_target_choice_and_restore_ravenous_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::paid::<RavenousSession>();
}
#[test]
fn strongbox_gourmand_paid_cast_attack_resolution_and_restore_ravenous_both_seats() {
    paid_entry_attack::<RavenousSession>();
}
#[test]
fn memorial_panda_paid_cast_grants_and_gain_death_input_restore_ravenous_both_seats() {
    paid_memorial_panda::<RavenousSession>();
}
#[test]
fn ashroot_stromkirk_private_opponent_printings_do_not_change_packet_ravenous_both_seats() {
    crate::current_mixed_policy_v1::tests::ordinary_triggers::privacy::<RavenousSession>();
}

#[test]
fn early_printings_paid_ordinary_activations_and_restore_fake_both_seats() {
    paid_early_activation::<FakeSession>();
}
#[test]
fn early_printings_paid_ordinary_activations_and_restore_ravenous_both_seats() {
    paid_early_activation::<RavenousSession>();
}

#[test]
fn crystal_paid_cast_hexproof_prevention_control_restore_privacy_fake_both_seats() {
    paid_crystal::<FakeSession>();
}
#[test]
fn crystal_paid_cast_hexproof_prevention_control_restore_privacy_ravenous_both_seats() {
    paid_crystal::<RavenousSession>();
}

#[test]
fn tyrant_herald_paid_cast_damage_flash_restore_privacy_fake_both_seats() {
    paid_tyrant_herald::<FakeSession>();
}
#[test]
fn tyrant_herald_paid_cast_damage_flash_restore_privacy_ravenous_both_seats() {
    paid_tyrant_herald::<RavenousSession>();
}

#[test]
fn rallier_bloom_paid_cast_private_pick_and_mana_restore_fake_both_seats() {
    paid_rallier_bloom::<FakeSession>();
}
#[test]
fn rallier_bloom_paid_cast_private_pick_and_mana_restore_ravenous_both_seats() {
    paid_rallier_bloom::<RavenousSession>();
}

pub(in crate::current_mixed_policy_v1) fn paid_enchantment_hunter<O: Owner>() {
    enchantment_hunter::paid::<O>();
    enchantment_hunter::privacy::<O>();
}
#[test]
fn enchantment_hunter_paid_cast_trigger_restore_privacy_fake_both_seats() {
    paid_enchantment_hunter::<FakeSession>();
}
#[test]
fn enchantment_hunter_paid_cast_trigger_restore_privacy_ravenous_both_seats() {
    paid_enchantment_hunter::<RavenousSession>();
}

pub(in crate::current_mixed_policy_v1) fn paid_eaten<O: Owner>() {
    eaten::paid::<O>();
    eaten::privacy::<O>();
}
#[test]
fn eaten_paid_additional_branches_exile_restore_privacy_fake_both_seats() {
    paid_eaten::<FakeSession>();
}
#[test]
fn eaten_paid_additional_branches_exile_restore_privacy_ravenous_both_seats() {
    paid_eaten::<RavenousSession>();
}

pub(in crate::current_mixed_policy_v1) fn paid_faebloom<O: Owner>() {
    faebloom::paid::<O>();
    faebloom::privacy::<O>();
}
#[test]
fn faebloom_paid_instant_reflexive_season_restore_privacy_fake_both_seats() {
    paid_faebloom::<FakeSession>();
}
#[test]
fn faebloom_paid_instant_reflexive_season_restore_privacy_ravenous_both_seats() {
    paid_faebloom::<RavenousSession>();
}

pub(in crate::current_mixed_policy_v1) fn paid_kellan<O: Owner>() {
    kellan::paid::<O>();
    kellan::privacy::<O>();
}
#[test]
fn kellan_paid_upgrades_combat_impulse_restore_privacy_fake_both_seats() {
    paid_kellan::<FakeSession>();
}
#[test]
fn kellan_paid_upgrades_combat_impulse_restore_privacy_ravenous_both_seats() {
    paid_kellan::<RavenousSession>();
}
