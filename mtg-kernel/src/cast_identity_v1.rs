//! Public, incarnation-bound identity of a genuinely finalized spell cast.
//! The bare SpellCast event remains the trigger hook. This history-only
//! receipt precedes it so Storm's existing SpellCast-to-journal adjacency is
//! unchanged. A concealed cast never stores its private card definition here.

use crate::{
    event::CommittedEvent,
    ids::{ObjectId, PlayerId, StackItemId},
    state::{GameState, StackSourceContractV4, Zone},
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PublicIdentityV1 {
    Known { card_def: u16 },
    Concealed { disguised: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReceiptV1 {
    pub spell_cast_history_index: u64,
    pub spell: ObjectId,
    pub owner: PlayerId,
    pub controller: PlayerId,
    pub origin_zone: Zone,
    pub origin_zone_change_count: u32,
    pub stack_zone_change_count: u32,
    pub stack_item_id: StackItemId,
    pub public_identity: PublicIdentityV1,
}

/// Called only after successful 601.2 finalization, directly before the
/// existing native SpellCast marker. It does not enter the trigger event log.
pub(crate) fn publish(
    state: &mut GameState,
    source: StackSourceContractV4,
    stack_item_id: StackItemId,
) {
    let origin = source
        .spell_cast_origin
        .expect("finalized physical cast retains its original incarnation");
    debug_assert_eq!(source.zone, Zone::Stack);
    debug_assert_eq!(origin.finalized_method, Some(source.cast_method));
    debug_assert!(source.spell_copy_origin.is_none());
    let public_identity = if let Some(face) = state.objects.get(source.source).v4.face_down_v1 {
        PublicIdentityV1::Concealed {
            disguised: face.disguised,
        }
    } else {
        PublicIdentityV1::Known {
            card_def: source.card_def,
        }
    };
    let receipt = ReceiptV1 {
        spell_cast_history_index: u64::try_from(state.engine.event_history.len())
            .expect("cast history index fits u64")
            + 1,
        spell: source.source,
        owner: source.owner,
        controller: source.controller,
        origin_zone: origin.origin_zone,
        origin_zone_change_count: origin.origin_zone_change_count,
        stack_zone_change_count: source.zone_change_count,
        stack_item_id,
        public_identity,
    };
    state
        .engine
        .event_history
        .push(CommittedEvent::PublishedCastIdentityV1 { receipt });
}

/// The historical scope uses only originally published identities. Missing or
/// malformed proof refuses the whole scope instead of rereading a later
/// hidden physical printing. Current live sources retain their own checks.
pub(crate) fn unsupported_public_history(
    state: &GameState,
    unsupported: impl Fn(u16) -> bool,
) -> Result<bool, String> {
    if state
        .engine
        .event_log
        .iter()
        .any(|event| matches!(event, CommittedEvent::PublishedCastIdentityV1 { .. }))
    {
        return Err("finalized cast identity receipt entered the trigger log".into());
    }
    let history = &state.engine.event_history;
    let mut seen_stack_items = HashSet::new();
    let mut seen_incarnations = HashSet::new();
    let mut has_unsupported = false;
    for (at, event) in history.iter().enumerate() {
        match event {
            CommittedEvent::PublishedCastIdentityV1 { receipt } => {
                let cast_at = at.checked_add(1).ok_or("cast history index overflow")?;
                if receipt.spell_cast_history_index
                    != u64::try_from(cast_at).map_err(|_| "cast history index width")?
                    || history.get(cast_at)
                        != Some(&CommittedEvent::SpellCast {
                            spell: receipt.spell,
                            controller: receipt.controller,
                        })
                    || receipt.stack_item_id == StackItemId::default()
                    || receipt.origin_zone == Zone::Stack
                    || receipt.origin_zone_change_count.checked_add(1)
                        != Some(receipt.stack_zone_change_count)
                    || !seen_stack_items.insert(receipt.stack_item_id)
                    || !seen_incarnations.insert((receipt.spell, receipt.stack_zone_change_count))
                {
                    return Err("finalized cast identity receipt lost its original marker".into());
                }
                let live = state
                    .objects
                    .try_get(receipt.spell)
                    .ok_or("finalized cast source object absent")?;
                if live.owner != receipt.owner
                    || live.zone_change_count < receipt.stack_zone_change_count
                    || (live.zone_change_count == receipt.stack_zone_change_count
                        && (live.zone != Zone::Stack || live.controller != receipt.controller))
                {
                    return Err("finalized cast receipt lost its original incarnation".into());
                }
                if let PublicIdentityV1::Known { card_def } = receipt.public_identity {
                    if crate::card_def::CARD_DEFS
                        .get(usize::from(card_def))
                        .is_none()
                        || (live.zone_change_count == receipt.stack_zone_change_count
                            && live.v4.face_down_v1.is_none()
                            && live.card_def != card_def)
                    {
                        return Err("finalized cast public identity is inconsistent".into());
                    }
                    has_unsupported |= unsupported(card_def);
                }
            }
            CommittedEvent::SpellCast { spell, controller } => {
                if at == 0
                    || !matches!(history.get(at - 1),
                        Some(CommittedEvent::PublishedCastIdentityV1 { receipt })
                            if receipt.spell == *spell && receipt.controller == *controller)
                {
                    return Err("SpellCast lacks finalized public identity proof".into());
                }
            }
            _ => {}
        }
    }
    Ok(has_unsupported)
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests {
    use super::*;
    use crate::{
        card_def,
        engine::{self, Action, Decision},
        event::{self, ProposedEvent},
        policy_observation_v6::tests::put,
    };

    fn receipt_for(state: &GameState, spell: ObjectId) -> (usize, ReceiptV1) {
        state
            .engine
            .event_history
            .iter()
            .enumerate()
            .find_map(|(at, event)| match event {
                CommittedEvent::PublishedCastIdentityV1 { receipt } if receipt.spell == spell => {
                    Some((at, *receipt))
                }
                _ => None,
            })
            .expect("paid spell has a native cast identity")
    }

    fn paid_then_rehidden(actor: PlayerId) -> (GameState, ObjectId, usize, ReceiptV1) {
        let mut state = crate::call_observation_v62::tests::ready(actor, "Forest");
        let spell = crate::graveyard_return_public_v61::tests::paid(
            &mut state,
            actor,
            "Fiend Artisan",
            None,
        );
        let (at, receipt) = receipt_for(&state, spell);
        assert_eq!(
            receipt.public_identity,
            PublicIdentityV1::Known {
                card_def: card_def::card_id_by_name("Fiend Artisan").unwrap()
            }
        );
        assert_eq!(
            state.engine.event_history.get(at + 1),
            Some(&CommittedEvent::SpellCast {
                spell,
                controller: actor
            })
        );
        assert_eq!(state.objects.get(spell).zone, Zone::Battlefield);
        // A native move makes the genuinely paid original cast source an
        // eligible hidden library slot; the cast receipt remains original.
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(spell, Zone::Library));
        assert!(crate::trigger::collect_and_process(&mut state).is_empty());
        assert!(state.players[actor.index()].library.contains(&spell));
        (state, spell, at, receipt)
    }

    #[test]
    fn paid_rehidden_unsupported_cast_retains_original_public_identity_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, spell, _, receipt) = paid_then_rehidden(actor);
            assert!(unsupported_public_history(&state, |id| id >= 460).unwrap());
            assert!(crate::call_observation_v62::require_supported(&state, Some(actor)).is_err());
            assert!(
                crate::end_turn_observation_v68::require_supported(&state, Some(actor)).is_err()
            );
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(restored, state);
            assert!(unsupported_public_history(&restored, |id| id >= 460).unwrap());
            assert_eq!(receipt_for(&restored, spell).1, receipt);

            // Negative hidden-slot permutation: a current private library
            // printing may differ without relabeling the old public cast.
            let mut different_private_slot = restored;
            different_private_slot.objects.get_mut(spell).card_def =
                card_def::card_id_by_name("Forest").unwrap();
            let before = different_private_slot.clone();
            assert!(unsupported_public_history(&different_private_slot, |id| id >= 460).unwrap());
            assert!(crate::call_observation_v62::require_supported(
                &different_private_slot,
                Some(actor)
            )
            .is_err());
            assert_eq!(different_private_slot, before);
        }
    }

    #[test]
    fn supported_paid_cast_does_not_inherit_later_private_slot_identity_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = crate::call_observation_v62::tests::ready(actor, "Forest");
            let spell = crate::graveyard_return_public_v61::tests::paid(
                &mut state,
                actor,
                "Alesha, Who Laughs at Fate",
                None,
            );
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(spell, Zone::Library));
            assert!(crate::trigger::collect_and_process(&mut state).is_empty());
            assert!(!unsupported_public_history(&state, |id| id >= 460).unwrap());
            assert!(crate::call_observation_v62::require_supported(&state, None).is_ok());
            assert!(crate::end_turn_observation_v68::require_supported(&state, None).is_ok());
            state.objects.get_mut(spell).card_def =
                card_def::card_id_by_name("Fiend Artisan").unwrap();
            let before = state.clone();
            assert!(!unsupported_public_history(&state, |id| id >= 460).unwrap());
            assert!(crate::call_observation_v62::require_supported(&state, None).is_ok());
            assert!(crate::end_turn_observation_v68::require_supported(&state, None).is_ok());
            assert_eq!(state, before);
        }
    }

    #[test]
    fn cast_receipt_loss_relink_and_suffix_forgery_refuse_readonly_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (original, spell, at, receipt) = paid_then_rehidden(actor);
            let mut missing = original.clone();
            missing.engine.event_history.remove(at);
            let mut relinked = original.clone();
            let CommittedEvent::PublishedCastIdentityV1 { receipt: changed } =
                &mut relinked.engine.event_history[at]
            else {
                unreachable!()
            };
            changed.spell_cast_history_index += 1;
            let mut suffix = original.clone();
            suffix
                .engine
                .event_history
                .push(CommittedEvent::PublishedCastIdentityV1 { receipt });
            let mut log_forgery = original.clone();
            log_forgery
                .engine
                .event_log
                .push(CommittedEvent::PublishedCastIdentityV1 { receipt });
            for forged in [missing, relinked, suffix, log_forgery] {
                let before = forged.clone();
                assert!(unsupported_public_history(&forged, |id| id >= 460).is_err());
                assert!(
                    crate::call_observation_v62::require_supported(&forged, Some(actor)).is_err()
                );
                assert!(
                    crate::end_turn_observation_v68::require_supported(&forged, Some(actor))
                        .is_err()
                );
                assert_eq!(forged, before);
            }
            assert_eq!(receipt_for(&original, spell).1, receipt);
        }
    }
}

#[cfg(all(test, feature = "standard-magezero-fixtures"))]
#[path = "cast_identity_v1/standard_tests.rs"]
mod standard_tests;
