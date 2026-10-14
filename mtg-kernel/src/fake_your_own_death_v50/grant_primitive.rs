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

/// Capture one complete caller-owned grant roster against the same original
/// battlefield for every proposed death. The future owner must derive the
/// complete grant inventory and retain these operands through the actual batch.
/// Event order and distinct grant order are preserved; this is not APNAP
/// ordering or executable trigger publication authority.
pub fn primitive_capture_death_roster_v50(
    state: &GameState,
    grants: &[PrimitiveGrantV50],
    events: &[ProposedEvent],
) -> Result<Vec<PrimitiveDeathLookbackV50>, String> {
    for (index, grant) in grants.iter().enumerate() {
        if grants[..index]
            .iter()
            .any(|prior| prior.timestamp == grant.timestamp)
        {
            return Err("Fake primitive grant inventory repeats an issued timestamp".into());
        }
    }
    let mut deaths = Vec::new();
    let mut captures = Vec::new();
    for event in events {
        let ProposedEvent::ZoneChange(change) = event else {
            continue;
        };
        if change.to_zone != Zone::Graveyard
            || !state
                .objects
                .try_get(change.object)
                .is_some_and(|object| object.zone == Zone::Battlefield)
        {
            continue;
        }
        if deaths.contains(&change.object) {
            return Err("Fake primitive death roster repeats a battlefield departure".into());
        }
        deaths.push(change.object);
        for grant in grants {
            if let Some(capture) = primitive_death_lookback_v50(state, *grant, event)? {
                captures.push(capture);
            }
        }
    }
    Ok(captures)
}

/// Bind immediately after the captured move commits, before another move or
/// token state-based action. The caller must own the original grant/lookback
/// and this committed event; this primitive does not publish an ability.
/// The returned context freezes the actual graveyard incarnation and the
/// battlefield controller at death. Later resolution must retain this context.
pub fn primitive_bind_committed_death_v50(
    state: &GameState,
    lookback: &PrimitiveDeathLookbackV50,
    history_index: u64,
) -> Result<crate::effect::ExecCtx, String> {
    let source = lookback.battlefield_source;
    if source.zone != Zone::Battlefield
        || lookback.grant.recipient.object != source.source
        || lookback.grant.recipient.zone_change_count != source.zone_change_count
        || source.zone_change_count.checked_add(1) != Some(lookback.graveyard_zone_change_count)
    {
        return Err("Fake primitive death lookback changed its original incarnation".into());
    }
    let event = usize::try_from(history_index)
        .ok()
        .and_then(|index| state.engine.event_history.get(index));
    if !matches!(event, Some(crate::event::CommittedEvent::ZoneChange {
        object, from: Zone::Battlefield, to: Zone::Graveyard, controller_before
    }) if *object == source.source && *controller_before == source.controller)
    {
        return Err("Fake primitive lacks its exact committed battlefield death".into());
    }
    if state
        .engine
        .event_history
        .iter()
        .skip(
            usize::try_from(history_index).map_err(|_| "Fake primitive death index overflow")? + 1,
        )
        .any(|event| {
            matches!(event, crate::event::CommittedEvent::ZoneChange { object, .. }
        if *object == source.source)
        })
    {
        return Err("Fake primitive selected an older move of the original object".into());
    }
    let object = state
        .objects
        .try_get(source.source)
        .ok_or("Fake primitive original graveyard object is absent")?;
    if object.zone != Zone::Graveyard
        || object.zone_change_count != lookback.graveyard_zone_change_count
        || object.owner != source.owner
        || object.v4.is_token != lookback.was_token_at_death
        || !state.players[object.owner.index()]
            .graveyard
            .contains(&source.source)
    {
        return Err("Fake primitive binding missed the original graveyard incarnation".into());
    }
    let mut context = crate::effect::ExecCtx::no_targets(source.source, source.controller);
    context.ability_source_contract = Some(AbilitySourceContractV4::capture(state, source.source));
    Ok(context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::{advance_to_next_main_for, put, ready_state_for};

    fn last_death_index(state: &GameState, object: crate::ids::ObjectId) -> u64 {
        state
            .engine
            .event_history
            .iter()
            .rposition(|event| {
                matches!(event,
            crate::event::CommittedEvent::ZoneChange {
                object: actual, from: Zone::Battlefield, to: Zone::Graveyard, ..
            } if *actual == object)
            })
            .unwrap() as u64
    }

    #[test]
    fn primitive_committed_death_keeps_owner_and_controller_distinct_across_two_grants() {
        for owner in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(owner);
            let creature = put(&mut state, owner, "Llanowar Elves", Zone::Battlefield);
            let recipient = binding(&state, creature);
            let first = primitive_install_grant_v50(&mut state, recipient).unwrap();
            let second = primitive_install_grant_v50(&mut state, recipient).unwrap();
            crate::standard_cards_v1::gain_control(&mut state, creature, owner.opponent());
            let death = ProposedEvent::zone_change(creature, Zone::Graveyard);
            let lookbacks = [first, second].map(|grant| {
                primitive_death_lookback_v50(&state, grant, &death)
                    .unwrap()
                    .unwrap()
            });
            crate::event::propose_and_commit(&mut state, death);
            let death_index = last_death_index(&state, creature);
            let contexts = lookbacks.map(|lookback| {
                primitive_bind_committed_death_v50(&state, &lookback, death_index).unwrap()
            });
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(
                contexts[0],
                primitive_bind_committed_death_v50(&restored, &lookbacks[0], death_index).unwrap()
            );
            for context in contexts {
                assert_eq!(context.controller, owner.opponent());
                assert_eq!(context.ability_source_contract.unwrap().owner, owner);
                let program =
                    super::super::primitive_death_program_v50(&state, &context, false).unwrap();
                crate::effect::execute(&program, &context, &mut state);
            }
            assert_eq!(state.objects.get(creature).zone, Zone::Battlefield);
            assert_eq!(state.objects.get(creature).controller, owner);
            assert!(state.objects.get(creature).tapped);
            assert_eq!(crate::engine::effective_power(&state, creature), 1);
            let treasure = crate::card_def::card_id_by_name("Treasure Token").unwrap();
            for actor in [owner, owner.opponent()] {
                let count = state.players[actor.index()]
                    .battlefield
                    .iter()
                    .filter(|object| state.objects.get(**object).card_def == treasure)
                    .count();
                assert_eq!(count, if actor == owner { 0 } else { 2 });
            }
        }
    }

    #[test]
    fn primitive_simultaneous_deaths_keep_distinct_grants_and_original_controllers() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let first = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let second = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let first_binding = binding(&state, first);
            let second_binding = binding(&state, second);
            let grants = [
                primitive_install_grant_v50(&mut state, first_binding).unwrap(),
                primitive_install_grant_v50(&mut state, first_binding).unwrap(),
                primitive_install_grant_v50(&mut state, second_binding).unwrap(),
            ];
            crate::standard_cards_v1::gain_control(&mut state, first, actor.opponent());
            let events = [
                ProposedEvent::zone_change(first, Zone::Graveyard),
                ProposedEvent::zone_change(second, Zone::Graveyard),
            ];
            let before = serde_json::to_vec(&state).unwrap();
            assert!(
                primitive_capture_death_roster_v50(&state, &[grants[0], grants[0]], &events)
                    .is_err()
            );
            assert!(primitive_capture_death_roster_v50(
                &state,
                &grants,
                &[events[0].clone(), events[0].clone()]
            )
            .is_err());
            let captures = primitive_capture_death_roster_v50(&state, &grants, &events).unwrap();
            assert_eq!(captures.len(), 3);
            assert_eq!(
                captures
                    .iter()
                    .map(|capture| capture.grant)
                    .collect::<Vec<_>>(),
                grants
            );
            assert!(captures
                .iter()
                .all(|capture| capture.battlefield_source.controller == actor.opponent()));
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            crate::event::propose_and_commit_batch(&mut state, events.to_vec());
            for capture in captures {
                let source = capture.battlefield_source.source;
                let context = primitive_bind_committed_death_v50(
                    &state,
                    &capture,
                    last_death_index(&state, source),
                )
                .unwrap();
                assert_eq!(context.controller, actor.opponent());
                assert_eq!(
                    context.ability_source_contract.unwrap().owner,
                    if source == first {
                        actor
                    } else {
                        actor.opponent()
                    }
                );
            }
        }
    }

    #[test]
    fn primitive_bound_death_does_not_follow_a_later_exile_response_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let creature = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let recipient = binding(&state, creature);
            let grant = primitive_install_grant_v50(&mut state, recipient).unwrap();
            let death = ProposedEvent::zone_change(creature, Zone::Graveyard);
            let lookback = primitive_death_lookback_v50(&state, grant, &death)
                .unwrap()
                .unwrap();
            crate::event::propose_and_commit(&mut state, death);
            let index = last_death_index(&state, creature);
            let context = primitive_bind_committed_death_v50(&state, &lookback, index).unwrap();
            crate::event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(creature, Zone::Exile),
            );
            let before = serde_json::to_vec(&state).unwrap();
            assert!(primitive_bind_committed_death_v50(&state, &lookback, index).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            let context: crate::effect::ExecCtx =
                serde_json::from_slice(&serde_json::to_vec(&context).unwrap()).unwrap();
            let program =
                super::super::primitive_death_program_v50(&state, &context, false).unwrap();
            assert!(matches!(&program, crate::effect::EffectOp::Sequence(ops)
                if matches!(ops.as_slice(), [crate::effect::EffectOp::CreateToken {..}])));
            crate::effect::execute(&program, &context, &mut state);
            assert_eq!(state.objects.get(creature).zone, Zone::Exile);
            let treasure = crate::card_def::card_id_by_name("Treasure Token").unwrap();
            assert_eq!(
                state.players[actor.index()]
                    .battlefield
                    .iter()
                    .filter(|object| state.objects.get(**object).card_def == treasure)
                    .count(),
                1
            );
        }
    }

    #[test]
    fn primitive_committed_death_refuses_an_older_occurrence_and_changed_capture() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let creature = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let recipient = binding(&state, creature);
            let grant = primitive_install_grant_v50(&mut state, recipient).unwrap();
            let death = ProposedEvent::zone_change(creature, Zone::Graveyard);
            let first = primitive_death_lookback_v50(&state, grant, &death)
                .unwrap()
                .unwrap();
            crate::event::propose_and_commit(&mut state, death);
            let first_index = last_death_index(&state, creature);
            crate::event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(creature, Zone::Battlefield),
            );
            let recipient = binding(&state, creature);
            let grant = primitive_install_grant_v50(&mut state, recipient).unwrap();
            let death = ProposedEvent::zone_change(creature, Zone::Graveyard);
            let second = primitive_death_lookback_v50(&state, grant, &death)
                .unwrap()
                .unwrap();
            crate::event::propose_and_commit(&mut state, death);
            let second_index = last_death_index(&state, creature);
            let before = serde_json::to_vec(&state).unwrap();
            assert!(primitive_bind_committed_death_v50(&state, &second, first_index).is_err());
            assert!(primitive_bind_committed_death_v50(&state, &first, second_index).is_err());
            assert!(primitive_bind_committed_death_v50(&state, &second, u64::MAX).is_err());
            let mut changed = second;
            changed.battlefield_source.controller = actor.opponent();
            assert!(primitive_bind_committed_death_v50(&state, &changed, second_index).is_err());
            let mut changed = second;
            changed.battlefield_source.owner = actor.opponent();
            assert!(primitive_bind_committed_death_v50(&state, &changed, second_index).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            primitive_bind_committed_death_v50(&state, &second, second_index).unwrap();
        }
    }

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
            let simultaneous = [
                ProposedEvent::zone_change(witness, Zone::Graveyard),
                ProposedEvent::zone_change(creature, Zone::Graveyard),
            ];
            let captures =
                primitive_capture_death_roster_v50(&state, &[earlier, later], &simultaneous)
                    .unwrap();
            assert_eq!(captures.len(), 1);
            assert_eq!(captures[0].grant, later);
            // Removing the Aura in a separate event genuinely restores the
            // earlier ability. That later board cannot describe simultaneous death.
            let mut separate = state.clone();
            crate::event::propose_and_commit(&mut separate, simultaneous[0].clone());
            assert!(earlier.active(&separate).unwrap());
            let mut simultaneous_state = state.clone();
            crate::event::propose_and_commit_batch(&mut simultaneous_state, simultaneous.to_vec());
            primitive_bind_committed_death_v50(
                &simultaneous_state,
                &captures[0],
                last_death_index(&simultaneous_state, creature),
            )
            .unwrap();
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
            let context = primitive_bind_committed_death_v50(
                &state,
                &capture,
                last_death_index(&state, copy),
            )
            .unwrap();
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
