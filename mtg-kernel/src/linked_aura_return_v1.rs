//! Exact, untargeted host choices for an Aura returning from linked exile.
//!
//! These primitives do not publish a choice or manufacture a resolving owner.
//! The departure continuation must own their invocation before later work.
use crate::effect::EffectObjectBinding;
use crate::event::CommittedEvent;
use crate::ids::PlayerId;
use crate::state::{GameState, LinkedExileRecordV4, Zone};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HostChoiceV1 {
    pub link: LinkedExileRecordV4,
    pub departure_history_index: u64,
    pub owner: PlayerId,
    pub aura: EffectObjectBinding,
    pub original_candidates: Vec<EffectObjectBinding>,
}

fn original_departure(state: &GameState, link: LinkedExileRecordV4) -> Result<usize, String> {
    let entry = crate::banishing_light_v1::validate_source(state, link.source)?;
    state.engine.event_history[entry + 1..]
        .iter()
        .position(|event| {
            matches!(event, CommittedEvent::ZoneChange {
            object, from: Zone::Battlefield, ..
        } if *object == link.source.source)
        })
        .map(|offset| entry + 1 + offset)
        .ok_or_else(|| "linked Aura has no actual original-source departure".into())
}

/// Capture actual post-departure legality, without a targeting event or Ward.
pub fn prepare(state: &GameState, link: LinkedExileRecordV4) -> Result<HostChoiceV1, String> {
    let departure = original_departure(state, link)?;
    if crate::banishing_light_v1::original_token(state, link)? {
        return Err("an exiled token cannot return or request an Aura attachment".into());
    }
    let live = state
        .objects
        .try_get(link.exiled)
        .ok_or("linked Aura is absent")?;
    if live.zone != Zone::Exile
        || live.owner != link.exiled_owner
        || live.zone_change_count != link.exiled_zone_change_count
        || !state.exile.contains(&link.exiled)
    {
        return Err("linked Aura lost its exact original exile incarnation".into());
    }
    let aura = EffectObjectBinding {
        object: link.exiled,
        expected_zone: Zone::Exile,
        expected_zone_change_count: link.exiled_zone_change_count,
    };
    let original_candidates = crate::effect::exiled_aura_hosts(state, aura)?;
    Ok(HostChoiceV1 {
        link,
        departure_history_index: u64::try_from(departure)
            .map_err(|_| "departure index exceeds u64")?,
        owner: live.owner,
        aura,
        original_candidates,
    })
}

/// Readonly checks precede any physical return. The captured menu must still
/// equal the actual post-departure choices, not merely contain the answer.
pub fn validate(state: &GameState, choice: &HostChoiceV1) -> Result<(), String> {
    if prepare(state, choice.link)? != *choice {
        return Err("linked Aura owner, original departure or legal hosts changed".into());
    }
    Ok(())
}

pub fn answer(
    state: &mut GameState,
    choice: &HostChoiceV1,
    host: EffectObjectBinding,
) -> Result<(), String> {
    validate(state, choice)?;
    if !choice.original_candidates.contains(&host) {
        return Err("linked Aura host is outside the exact owner-choice menu".into());
    }
    crate::effect::put_exiled_aura_attached(state, choice.aura, host)
}

/// No legal enchant host is a required no-return outcome, rather than an
/// unattached battlefield entry which a later SBA would destroy.
pub fn must_remain_exiled(state: &GameState, choice: &HostChoiceV1) -> Result<bool, String> {
    validate(state, choice)?;
    Ok(choice.original_candidates.is_empty())
}

pub(crate) fn record_capture(state: &mut GameState, choice: HostChoiceV1) -> Result<u64, String> {
    validate(state, &choice)?;
    let at =
        u64::try_from(state.engine.event_history.len()).map_err(|_| "Aura capture exceeds u64")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::BanishingAuraHostsCapturedV1 {
            choice: Box::new(choice),
        });
    Ok(at)
}
