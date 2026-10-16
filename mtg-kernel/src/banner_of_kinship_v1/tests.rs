use super::*;
use crate::state::Counters;
use std::hash::{Hash, Hasher};

#[derive(Default)]
struct HashBytes(Vec<u8>);
impl Hasher for HashBytes {
    fn finish(&self) -> u64 {
        0
    }
    fn write(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
}

#[test]
fn sparse_fellowship_preserves_empty_counter_bytes_and_full_width_roundtrip() {
    let old = br#"{"plus1_plus1":0,"minus1_minus1":0,"minus0_minus1":0,"stun":0,"lore":0}"#;
    let empty: Counters = serde_json::from_slice(old).unwrap();
    assert_eq!(serde_json::to_vec(&empty).unwrap(), old);
    let mut bytes = HashBytes::default();
    empty.hash(&mut bytes);
    assert_eq!(bytes.0, [0; 10]);
    for value in [1, i32::MAX, -1] {
        let counters = Counters {
            fellowship: value,
            ..empty
        };
        assert!(counters.any());
        let raw = serde_json::to_vec(&counters).unwrap();
        assert_eq!(serde_json::from_slice::<Counters>(&raw).unwrap(), counters);
        let mut with_counter = HashBytes::default();
        counters.hash(&mut with_counter);
        assert!(with_counter.0.starts_with(&bytes.0));
        assert_ne!(with_counter.0, bytes.0);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod native_primitives {
    use super::*;
    use crate::{
        engine::{self, Action, Decision},
        linked_aura_operation_v1::GeneralChoiceV36,
        policy_observation_v6::tests::{put, ready_state_with_turn_authority},
    };

    fn settle(state: &mut GameState) {
        for _ in 0..32 {
            match engine::advance_until_decision(state) {
                Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
                Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
                other => panic!("paid prerequisite resolution: {other:?}"),
            }
        }
        panic!("paid prerequisite did not settle");
    }

    /// Real payments and Heraldic Banner's real color stage exercise the
    /// pre-entry calculation input. They do not give Heraldic Banner a
    /// creature-type choice or claim to cast unregistered Banner of Kinship.
    #[test]
    fn paid_heraldic_preentry_primitive_keeps_full_domain_scene_controller_and_restore_both_seats()
    {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for season in [false, true] {
                let mut state = ready_state_with_turn_authority(actor);
                let forests = (0..if season { 9 } else { 4 })
                    .map(|_| put(&mut state, actor, "Forest", Zone::Battlefield))
                    .collect::<Vec<_>>();
                let elf = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
                engine::step(&mut state, Action::CastSpell(elf)).unwrap();
                settle(&mut state);
                assert_eq!(state.objects.get(elf).zone, Zone::Battlefield);
                if season {
                    let source = put(&mut state, actor, "Doubling Season", Zone::Hand);
                    engine::step(&mut state, Action::CastSpell(source)).unwrap();
                    settle(&mut state);
                    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
                }
                let heraldic = put(&mut state, actor, "Heraldic Banner", Zone::Hand);
                engine::step(&mut state, Action::CastSpell(heraldic)).unwrap();
                let mut reached = false;
                for _ in 0..16 {
                    match engine::advance_until_decision(&mut state) {
                        Decision::ChooseEntryColorV36 { player, .. } => {
                            assert_eq!(player, actor);
                            reached = true;
                            break;
                        }
                        Decision::CastSpellOrPass { .. } => {
                            engine::step(&mut state, Action::Pass).unwrap()
                        }
                        other => panic!("actual Heraldic entry: {other:?}"),
                    }
                }
                assert!(reached);
                assert_eq!(state.objects.get(heraldic).zone, Zone::Stack);
                assert!(forests.iter().all(|id| state.objects.get(*id).tapped));
                assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
                let pending = state
                    .engine
                    .linked_aura_operation_v1
                    .pending_general_v36
                    .as_ref()
                    .unwrap();
                let GeneralChoiceV36::Color { choice, .. } = &pending.choice else {
                    panic!("real Heraldic color stage")
                };
                let proposal = choice.proposal.clone();
                let prepared = EntryChoiceV1::capture(&state, &proposal).unwrap();
                assert_eq!(prepared.controller(), actor);
                assert_eq!(prepared.choices(), CreatureTypeV60::ALL);
                assert_eq!(prepared.choices().len(), 324);
                let original = state.clone();
                for chosen in prepared.choices() {
                    let quote = prepared.answer(&state, chosen.choice_index()).unwrap();
                    let count = i32::from(matches!(
                        chosen,
                        CreatureTypeV60::Elf | CreatureTypeV60::Druid
                    ));
                    assert_eq!(quote.before_replacements, count);
                    assert_eq!(quote.after_replacements, count * if season { 2 } else { 1 });
                }
                assert!(prepared.answer(&state, 324).is_err());
                assert_eq!(state, original);
                let mut restored: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                let rederived = EntryChoiceV1::capture(&restored, &proposal).unwrap();
                assert_eq!(
                    rederived
                        .answer(&restored, CreatureTypeV60::Elf.choice_index())
                        .unwrap(),
                    prepared
                        .answer(&state, CreatureTypeV60::Elf.choice_index())
                        .unwrap()
                );
                let mut changed = state.clone();
                changed.objects.get_mut(heraldic).zone_change_count += 1;
                assert!(prepared
                    .answer(&changed, CreatureTypeV60::Elf.choice_index())
                    .is_err());
                changed = state.clone();
                changed.objects.get_mut(elf).controller = actor.opponent();
                assert!(prepared
                    .answer(&changed, CreatureTypeV60::Elf.choice_index())
                    .is_err());
                for branch in [&mut state, &mut restored] {
                    engine::step(branch, Action::ChooseEffectOption(0)).unwrap();
                    assert_eq!(branch.objects.get(heraldic).zone, Zone::Battlefield);
                    assert_eq!(branch.objects.get(heraldic).counters.fellowship, 0);
                }
                assert_eq!(state, restored);
            }
        }
    }

    #[test]
    fn fellowship_native_storage_quotes_current_amount_and_old_counter_shapes_refuse_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_with_turn_authority(actor);
            let source = put(&mut state, actor, "Forest", Zone::Battlefield);
            let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let link = ObjectLinkV4 {
                object: source,
                zone_change_count: state.objects.get(source).zone_change_count,
            };
            // Native storage/arithmetic primitive, not an invented Forest ability.
            for amount in [1, 7, i32::MAX] {
                state.objects.get_mut(source).counters.fellowship = amount;
                let original = state.clone();
                assert_eq!(
                    quote_controlled_boost(&state, link, CreatureTypeV60::Elf, target).unwrap(),
                    (i128::from(amount), i128::from(amount))
                );
                assert_eq!(
                    quote_controlled_boost(&state, link, CreatureTypeV60::Bear, target).unwrap(),
                    (0, 0)
                );
                assert!(
                    crate::graveyard_return_public_v61::counter_values(&state, source).is_err()
                );
                assert!(crate::drake_public_v53::capture(&state).is_err());
                assert!(crate::rl::observe_v1(&state, actor, 0).is_err());
                assert_eq!(state, original);
                let restored: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                assert_eq!(
                    quote_controlled_boost(&restored, link, CreatureTypeV60::Elf, target),
                    quote_controlled_boost(&state, link, CreatureTypeV60::Elf, target)
                );
            }
            state.objects.get_mut(source).counters.fellowship = -1;
            assert!(quote_controlled_boost(&state, link, CreatureTypeV60::Elf, target).is_err());
            state.objects.get_mut(source).counters.fellowship = 7;
            state.objects.get_mut(target).controller = actor.opponent();
            assert_eq!(
                quote_controlled_boost(&state, link, CreatureTypeV60::Elf, target).unwrap(),
                (0, 0)
            );
            state.objects.get_mut(source).zone_change_count += 1;
            assert!(quote_controlled_boost(&state, link, CreatureTypeV60::Elf, target).is_err());
            assert!(CARD_DEFS.iter().all(|d| d.name != "Banner of Kinship"));
        }
    }
}
