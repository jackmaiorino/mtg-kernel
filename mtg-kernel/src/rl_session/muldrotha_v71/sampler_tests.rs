//! Genuine paid Muldrotha stages. Current-slot sampling source only, UNRUN.
use super::tests::Pair;
use super::*;
use crate::{
    card_def::CardType,
    engine::muldrotha_v1::tests as native,
    event::{self, CommittedEvent as E, ProposedEvent},
    ids::ObjectId,
    policy_observation_v6::tests::put,
    rl::ActionSemanticV1 as A,
    state::{Target, Zone},
};
fn scene(p: PlayerId) -> (GameState, ObjectId) {
    let mut s = native::ready(p, "Island");
    let source = native::paid(&mut s, p);
    put(&mut s, p, "Mountain", Zone::Library);
    put(&mut s, p.opponent(), "Forest", Zone::Hand);
    put(&mut s, p.opponent(), "Mountain", Zone::Hand);
    // Genuine actor knowledge stays outside the eligible unknown pool.
    s.reveal_library_top(p, p, 1);
    (s, source)
}
fn sampled(owner: &MuldrothaSessionV71, seed: u64) -> MuldrothaSessionV71 {
    let d = owner.decision_v71().unwrap();
    let before = owner.fixture_state_v71().clone();
    let mut clone = owner.sampled_clone_v71(&d, seed).unwrap();
    let repeat = owner.sampled_clone_v71(&d, seed).unwrap();
    assert_eq!(clone.fixture_state_v71(), repeat.fixture_state_v71());
    assert!(clone.fresh_binding().unwrap() == d.binding);
    assert_eq!(
        clone.fixture_state_v71().engine.event_history,
        before.engine.event_history
    );
    assert_eq!(
        clone.fixture_state_v71().engine.event_log,
        before.engine.event_log
    );
    assert_eq!(owner.fixture_state_v71(), &before);
    assert!(!std::sync::Arc::ptr_eq(&clone.owner, &owner.owner));
    assert!(clone.validate_decision_v71(&d).is_err());
    let fresh = clone.decision_v71().unwrap();
    assert!(owner.validate_decision_v71(&fresh).is_err());
    let decoded: GameState =
        serde_json::from_slice(&serde_json::to_vec(clone.fixture_state_v71()).unwrap()).unwrap();
    assert_eq!(&decoded, clone.fixture_state_v71());
    observation::validate_state(&decoded).unwrap();
    let saved = clone.snapshot_v71();
    clone.restore_v71(&saved).unwrap();
    assert!(clone.validate_decision_v71(&fresh).is_err());
    assert_eq!(
        clone.decision_v71().unwrap().candidates_v71(),
        fresh.candidates_v71()
    );
    clone
}
#[test]
fn paid_permission_multitype_sampling_keeps_native_source_type_cost_and_index_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for kind in [CardType::Creature, CardType::Artifact] {
            let (mut s, source) = scene(p);
            let card = put(&mut s, p, "Burnished Hart", Zone::Graveyard);
            let before = native::tapped(&s, p);
            let known = s.players[p.index()].library[0];
            let mut pair = Pair::new(s);
            pair.act(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==card.0));
            let original = pair.own.decision_v71().unwrap();
            let mut changed = false;
            for seed in 1..=8 {
                let clone = sampled(&pair.own, seed);
                assert_eq!(
                    clone.fixture_state_v71().players[p.index()].library[0],
                    known
                );
                changed |=
                    clone.fixture_state_v71().players != pair.own.fixture_state_v71().players;
            }
            assert!(changed);
            let mut derived = sampled(&pair.own, 8171);
            let d = derived.decision_v71().unwrap();
            let ordinal = d
                .candidates_v71()
                .candidates
                .iter()
                .position(|r| {
                    r.muldrotha_permission
                        .as_ref()
                        .and_then(|p| p.permission.as_ref())
                        .is_some_and(|permission| {
                            permission.permanent_type == kind
                                && permission.source.original_source.object == source.0
                        })
                })
                .unwrap();
            let original_index = d.fixture_original_index_v71(ordinal as u32).unwrap();
            let mut direct = derived.fixture_native_clone_v71();
            assert_eq!(
                derived.step_v71(&d, ordinal as u32).unwrap(),
                direct
                    .step(d.expected().episode_id, d.expected().step, original_index)
                    .unwrap()
            );
            assert_eq!(
                derived.fixture_state_v71(),
                direct.muldrotha_fixture_state()
            );
            assert_eq!(native::tapped(derived.fixture_state_v71(), p) - before, 3);
            assert_eq!(
                derived
                    .decision_v71()
                    .unwrap()
                    .observation()
                    .muldrotha
                    .budgets[0]
                    .spent,
                [kind]
            );
            assert_eq!(
                pair.own.decision_v71().unwrap().candidates_v71(),
                original.candidates_v71()
            );
        }
    }
}
#[test]
fn paid_bestow_adventure_land_published_and_finished_stages_sample_without_refunding_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for (name, kind, form, x, cost) in [
            ("Fang Dragon", CardType::Creature, 0, 0, 7),
            ("Nyxborn Hydra", CardType::Enchantment, 1, 3, 5),
        ] {
            let (mut s, source) = scene(p);
            let host = put(&mut s, p, "Llanowar Elves", Zone::Hand);
            native::pay(&mut s, p, host, None, None);
            native::finish(&mut s);
            let card = put(&mut s, p, name, Zone::Graveyard);
            let before = native::tapped(&s, p);
            native::pay_form(
                &mut s,
                p,
                card,
                Some((source, kind)),
                Some(Target::Object(host)),
                form,
                x,
            );
            assert_eq!(native::tapped(&s, p) - before, cost);
            let own = Pair::new(s.clone()).own;
            let clone = sampled(&own, 8172);
            assert_eq!(
                clone
                    .decision_v71()
                    .unwrap()
                    .observation()
                    .muldrotha
                    .budgets[0]
                    .spent,
                [kind]
            );
            native::finish(&mut s);
            let own = Pair::new(s).own;
            let clone = sampled(&own, 8173);
            assert_eq!(
                clone.decision_v71().unwrap().observation().muldrotha.plays[0].stage,
                crate::muldrotha_public_v71::StageV71::NoLongerPublished
            );
            assert_eq!(
                clone
                    .decision_v71()
                    .unwrap()
                    .observation()
                    .muldrotha
                    .budgets[0]
                    .spent,
                [kind]
            );
        }
        let (mut s, _) = scene(p);
        let land = put(&mut s, p, "Forest", Zone::Graveyard);
        let mut pair = Pair::new(s);
        pair.act(|a| matches!(a,A::PlayLand{source,..}if source.arena_id==land.0));
        pair.permission(CardType::Land);
        let clone = sampled(&pair.own, 8174);
        assert_eq!(
            clone.fixture_state_v71().players[p.index()].lands_played_this_turn,
            1
        );
        assert_eq!(
            clone
                .decision_v71()
                .unwrap()
                .observation()
                .muldrotha
                .budgets[0]
                .spent,
            [CardType::Land]
        );
    }
}
#[test]
fn paid_history_hidden_source_or_card_and_forged_original_permission_refuse_readonly_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for hide_source in [false, true] {
            let (mut s, source) = scene(p);
            let card = put(&mut s, p, "Llanowar Elves", Zone::Graveyard);
            native::pay(&mut s, p, card, Some((source, CardType::Creature)), None);
            native::finish(&mut s);
            let hidden = if hide_source { source } else { card };
            // Named departure setup after genuine payment/publication. No old
            // receipt or permission source is rewritten to this incarnation.
            event::propose_and_commit(&mut s, ProposedEvent::zone_change(hidden, Zone::Library));
            assert!(crate::engine::muldrotha_v1::sampler_conflicts_v71(&s, &[hidden]).unwrap());
            let owner = Pair::new(s).own;
            let d = owner.decision_v71().unwrap();
            let before = owner.fixture_state_v71().clone();
            assert!(owner.sampled_clone_v71(&d, 8175).is_err());
            assert_eq!(owner.fixture_state_v71(), &before);
        }
        let (mut s, source) = scene(p);
        let card = put(&mut s, p, "Llanowar Elves", Zone::Graveyard);
        native::pay(&mut s, p, card, Some((source, CardType::Creature)), None);
        let E::MuldrothaOptionsV1 { options } = s
            .engine
            .event_history
            .iter_mut()
            .find(|e| matches!(e, E::MuldrothaOptionsV1 { .. }))
            .unwrap()
        else {
            panic!()
        };
        options.permissions[0].permanent_type = CardType::Land;
        let before = s.clone();
        assert!(
            super::super::flat_action_v4::redeterminize_muldrotha_v71(&mut s, p, 8176).is_err()
        );
        assert_eq!(s, before);
    }
}
