//! Temporary-grant operands for the future printing's private owner.
//!
//! These primitives install the ordinary +2/+0 modifier and return a separate
//! ability operand. They do not authenticate a spell or publish a trigger.
//! The printing's future owner must retain and authenticate every operand;
//! an operand is not a GameState admission or a public consumer projection.

use crate::card_def::CardType;
use crate::effect::EffectObjectBinding;
use crate::engine::{EffectDuration, Layers, UntilEndOfTurnEffect};
use crate::event::ProposedEvent;
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Zone};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrimitiveGrantV50 {
    recipient: ObjectLinkV4,
    timestamp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrimitiveDeathLookbackV50 {
    pub grant: PrimitiveGrantV50,
    pub battlefield_source: AbilitySourceContractV4,
    pub was_token_at_death: bool,
    pub graveyard_zone_change_count: u32,
}

impl PrimitiveGrantV50 {
    fn modifier(self) -> UntilEndOfTurnEffect {
        UntilEndOfTurnEffect::ResolvedObjectEffect {
            object_id: self.recipient.object,
            object_zone_change_count: self.recipient.zone_change_count,
            layer: Layers::POWER_TOUGHNESS,
            timestamp: self.timestamp,
            duration: EffectDuration::EndOfTurn,
            power: 2,
            toughness: 0,
            grant_haste: false,
        }
    }

    /// Cleanup clears the ordinary modifier even if the round/seat is reused.
    /// A zone change makes this operand stale even before cleanup. The later
    /// printing owner must distinguish normal expiry from forged erasure.
    pub fn active(self, state: &GameState) -> Result<bool, String> {
        let Some(object) = state.objects.try_get(self.recipient.object) else {
            return Ok(false);
        };
        if object.zone != Zone::Battlefield
            || object.zone_change_count != self.recipient.zone_change_count
        {
            return Ok(false);
        }
        let matching: Vec<_> = state
            .engine
            .until_end_of_turn
            .iter()
            .filter(|effect| {
                matches!(effect, UntilEndOfTurnEffect::ResolvedObjectEffect {timestamp, ..}
                if *timestamp == self.timestamp)
            })
            .collect();
        match matching.as_slice() {
            [] => Ok(false),
            [modifier] if **modifier == self.modifier() => {
                Ok(crate::continuous_characteristics_v1::grant_survives(
                    state,
                    self.recipient.object,
                    self.timestamp,
                ))
            }
            _ => Err("Fake primitive grant lost its unique exact temporary modifier".into()),
        }
    }
}

/// Caller already owns the effect input and target legality. This narrower
/// primitive still refuses a missing, stale or noncreature recipient before
/// changing the state or consuming a timestamp. Each call installs one grant.
pub fn primitive_install_grant_v50(
    state: &mut GameState,
    recipient: EffectObjectBinding,
) -> Result<PrimitiveGrantV50, String> {
    let object = state
        .objects
        .try_get(recipient.object)
        .ok_or("Fake primitive grant recipient is absent")?;
    if recipient.expected_zone != Zone::Battlefield
        || object.zone != Zone::Battlefield
        || object.zone_change_count != recipient.expected_zone_change_count
        || !state.players[object.controller.index()]
            .battlefield
            .contains(&recipient.object)
        || !crate::engine::object_has_type(state, recipient.object, CardType::Creature)
    {
        return Err("Fake primitive grant recipient changed incarnation or creature type".into());
    }
    let timestamp = state.engine.next_effect_timestamp;
    timestamp
        .checked_add(1)
        .ok_or("Fake primitive grant timestamp overflow")?;
    let grant = PrimitiveGrantV50 {
        recipient: ObjectLinkV4 {
            object: recipient.object,
            zone_change_count: recipient.expected_zone_change_count,
        },
        timestamp,
    };
    crate::effect::install_temporary_boost(state, recipient, 2, 0, crate::card_def::Keywords::NONE);
    Ok(grant)
}

/// Capture while the original creature is still present, before the committed
/// move clears its incarnation. A control change changes the trigger's seat;
/// it does not change the caster or make the grant follow another incarnation.
pub fn primitive_death_lookback_v50(
    state: &GameState,
    grant: PrimitiveGrantV50,
    event: &ProposedEvent,
) -> Result<Option<PrimitiveDeathLookbackV50>, String> {
    let ProposedEvent::ZoneChange(change) = event else {
        return Ok(None);
    };
    if change.object != grant.recipient.object
        || change.to_zone != Zone::Graveyard
        || !grant.active(state)?
        || !crate::engine::object_has_type(state, change.object, CardType::Creature)
    {
        return Ok(None);
    }
    let object = state.objects.get(change.object);
    Ok(Some(PrimitiveDeathLookbackV50 {
        grant,
        battlefield_source: AbilitySourceContractV4::capture(state, change.object),
        was_token_at_death: object.v4.is_token,
        graveyard_zone_change_count: object
            .zone_change_count
            .checked_add(1)
            .ok_or("Fake primitive death generation overflow")?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::{advance_to_next_main_for, put, ready_state_for};

    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn primitive_paid_witness_removes_only_earlier_ability_grant_and_preserves_power_both_seats() {
        use crate::engine::{Action, Decision};
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            for player in [actor, actor.opponent()] {
                for _ in 0..40 {
                    put(&mut state, player, "Forest", Zone::Library);
                }
            }
            let creature = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let recipient = binding(&state, creature);
            let earlier = primitive_install_grant_v50(&mut state, recipient).unwrap();
            let witness = put(&mut state, actor, "Witness Protection", Zone::Hand);
            state.players[actor.index()].mana_pool[crate::mana::ManaColor::U.pool_index()] = 1;
            assert!(matches!(crate::engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { player, .. } if player == actor));
            crate::engine::step(&mut state, Action::CastSpell(witness)).unwrap();
            assert!(matches!(
                crate::engine::advance_until_decision(&mut state),
                Decision::ChooseTargets { .. }
            ));
            crate::engine::step(
                &mut state,
                Action::ChooseTarget(crate::state::Target::Object(creature)),
            )
            .unwrap();
            let mut settled = false;
            for _ in 0..40 {
                match crate::engine::advance_until_decision(&mut state) {
                    Decision::CastSpellOrPass { .. }
                        if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
                    {
                        settled = true;
                        break;
                    }
                    Decision::CastSpellOrPass { .. } => {
                        crate::engine::step(&mut state, Action::Pass).unwrap()
                    }
                    other => panic!("unexpected genuine Witness resolution {other:?}"),
                }
            }
            assert!(settled);
            assert_eq!(state.objects.get(witness).zone, Zone::Battlefield);
            assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
            assert!(!earlier.active(&state).unwrap());
            assert_eq!(crate::engine::effective_power(&state, creature), 3);
            let later = primitive_install_grant_v50(&mut state, recipient).unwrap();
            assert!(later.active(&state).unwrap());
            assert!(!earlier.active(&state).unwrap());
            assert_eq!(crate::engine::effective_power(&state, creature), 5);
            let death = ProposedEvent::zone_change(creature, Zone::Graveyard);
            assert!(primitive_death_lookback_v50(&state, earlier, &death)
                .unwrap()
                .is_none());
            let expected = primitive_death_lookback_v50(&state, later, &death)
                .unwrap()
                .unwrap();
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(
                primitive_death_lookback_v50(&restored, later, &death).unwrap(),
                Some(expected)
            );
            assert!(!earlier.active(&restored).unwrap());
        }
    }

    fn binding(state: &GameState, object: crate::ids::ObjectId) -> EffectObjectBinding {
        let source = state.objects.get(object);
        EffectObjectBinding {
            object,
            expected_zone: source.zone,
            expected_zone_change_count: source.zone_change_count,
        }
    }

    #[test]
    fn primitive_multiple_grants_capture_distinct_current_controller_and_restore_both_seats() {
        for owner in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(owner);
            let creature = put(&mut state, owner, "Llanowar Elves", Zone::Battlefield);
            let recipient = binding(&state, creature);
            let first = primitive_install_grant_v50(&mut state, recipient).unwrap();
            let second = primitive_install_grant_v50(&mut state, recipient).unwrap();
            assert_ne!(first.timestamp, second.timestamp);
            assert_eq!(crate::engine::effective_power(&state, creature), 5);
            // Exercise the actual shared control primitive. This is not a
            // claim of a paid control spell in the absent Fake printing path.
            crate::standard_cards_v1::gain_control(&mut state, creature, owner.opponent());
            let death = ProposedEvent::zone_change(creature, Zone::Graveyard);
            let snapshots: Vec<_> = [first, second]
                .into_iter()
                .map(|grant| {
                    primitive_death_lookback_v50(&state, grant, &death)
                        .unwrap()
                        .unwrap()
                })
                .collect();
            for capture in &snapshots {
                assert_eq!(capture.battlefield_source.owner, owner);
                assert_eq!(capture.battlefield_source.controller, owner.opponent());
                assert!(!capture.was_token_at_death);
            }
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            let restored_grants: Vec<PrimitiveGrantV50> =
                serde_json::from_slice(&serde_json::to_vec(&vec![first, second]).unwrap()).unwrap();
            let replay: Vec<_> = restored_grants
                .into_iter()
                .map(|grant| {
                    primitive_death_lookback_v50(&restored, grant, &death)
                        .unwrap()
                        .unwrap()
                })
                .collect();
            assert_eq!(snapshots, replay);
            let before = serde_json::to_vec(&state).unwrap();
            assert_eq!(before, serde_json::to_vec(&restored).unwrap());
        }
    }

    #[test]
    fn primitive_grant_does_not_follow_a_departed_and_reentered_recipient_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let creature = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let original = binding(&state, creature);
            let grant = primitive_install_grant_v50(&mut state, original).unwrap();
            crate::event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(creature, Zone::Graveyard),
            );
            crate::event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(creature, Zone::Battlefield),
            );
            assert_eq!(crate::engine::effective_power(&state, creature), 1);
            assert!(!grant.active(&state).unwrap());
            assert!(primitive_death_lookback_v50(
                &state,
                grant,
                &ProposedEvent::zone_change(creature, Zone::Graveyard)
            )
            .unwrap()
            .is_none());
            let before = serde_json::to_vec(&state).unwrap();
            assert!(primitive_install_grant_v50(&mut state, original).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }

    #[test]
    fn primitive_grants_expire_at_real_cleanup_before_next_main_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            for player in [actor, actor.opponent()] {
                for _ in 0..40 {
                    put(&mut state, player, "Forest", Zone::Library);
                }
            }
            let creature = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let recipient = binding(&state, creature);
            let grant = primitive_install_grant_v50(&mut state, recipient).unwrap();
            assert!(grant.active(&state).unwrap());
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            advance_to_next_main_for(&mut state, actor.opponent());
            advance_to_next_main_for(&mut restored, actor.opponent());
            assert!(!grant.active(&state).unwrap());
            assert_eq!(crate::engine::effective_power(&state, creature), 1);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }

    #[test]
    fn primitive_copied_token_death_captures_live_token_fact_and_only_creates_treasure_both_seats()
    {
        for owner in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(owner);
            let original = put(&mut state, owner, "Llanowar Elves", Zone::Battlefield);
            // Actual shared token-copy creation, with a nontoken definition.
            let copy =
                crate::standard_cards_v1::create_token_copy(&mut state, original, owner).unwrap();
            assert!(state.objects.get(copy).v4.is_token);
            let recipient = binding(&state, copy);
            let grant = primitive_install_grant_v50(&mut state, recipient).unwrap();
            crate::standard_cards_v1::gain_control(&mut state, copy, owner.opponent());
            let death = ProposedEvent::zone_change(copy, Zone::Graveyard);
            let capture = primitive_death_lookback_v50(&state, grant, &death)
                .unwrap()
                .unwrap();
            assert!(capture.was_token_at_death);
            assert_eq!(capture.battlefield_source.controller, owner.opponent());
            crate::event::propose_and_commit(&mut state, death);
            assert_eq!(
                state.objects.get(copy).zone_change_count,
                capture.graveyard_zone_change_count
            );
            let mut context =
                crate::effect::ExecCtx::no_targets(copy, capture.battlefield_source.controller);
            context.ability_source_contract = Some(AbilitySourceContractV4::capture(&state, copy));
            let program = super::super::primitive_death_program_v50(
                &state,
                &context,
                capture.was_token_at_death,
            )
            .unwrap();
            assert!(matches!(&program, crate::effect::EffectOp::Sequence(ops)
                if matches!(ops.as_slice(), [crate::effect::EffectOp::CreateToken {..}])));
            crate::effect::execute(&program, &context, &mut state);
            assert_eq!(state.objects.get(copy).zone, Zone::Graveyard);
            let treasure = crate::card_def::card_id_by_name("Treasure Token").unwrap();
            assert_eq!(
                state.players[owner.opponent().index()]
                    .battlefield
                    .iter()
                    .filter(|object| state.objects.get(**object).card_def == treasure)
                    .count(),
                1
            );
        }
    }
}
