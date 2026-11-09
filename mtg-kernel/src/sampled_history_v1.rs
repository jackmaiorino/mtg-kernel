//! Exact native historical source proofs retained by an opaque sampled owner.
//!
//! A sampled printing belongs to the future world. It is not evidence about a
//! source already captured by an earlier native event. Only those exact typed
//! occurrences may use this custody; live source validation remains unchanged.
use crate::{
    event::CommittedEvent as E,
    state::{AbilitySourceContractV4, GameState, StackItem, StackTargetContractV4, Zone},
};
use sha2::{Digest, Sha256};
use std::sync::Arc;

mod ability_sources;
use ability_sources::AbilitySourceProofV1;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ProofV1 {
    at: usize,
    source: AbilitySourceContractV4,
    event: [u8; 32],
}

/// Private constructors and skipped serialization prevent raw snapshots from
/// manufacturing a historical exception. Opaque owner snapshots retain it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CustodyV1(
    Arc<Vec<ProofV1>>,
    Arc<Vec<TargetProofV1>>,
    Arc<Vec<AbilitySourceProofV1>>,
);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TargetProofV1 {
    item: [u8; 32],
    archived: Option<(usize, [u8; 32])>,
    slot: usize,
    contract: StackTargetContractV4,
    public_face: u16,
}

impl CustodyV1 {
    pub(crate) fn hash_if_sampled<H: std::hash::Hasher>(&self, h: &mut H) {
        use std::hash::Hash;
        if !self.0.is_empty() || !self.1.is_empty() {
            "sampled-native-history/v1".hash(h);
            self.0.hash(h);
            self.1.hash(h);
        }
        if !self.2.is_empty() {
            "sampled-active-ability-source/v1".hash(h);
            self.2.hash(h);
        }
    }
}

fn fingerprint(event: &impl serde::Serialize) -> Result<[u8; 32], String> {
    let bytes = serde_json::to_vec(event).map_err(|e| format!("native source receipt: {e}"))?;
    Ok(Sha256::digest(bytes).into())
}

/// This typed list is also checked on every exception, so an unrelated event
/// at a valid old index cannot authorize a different historical contract.
fn sources(event: &E) -> Vec<AbilitySourceContractV4> {
    match event {
        E::SquadRallierResolutionBoundaryV1 { receipt } => vec![receipt.source],
        E::SphinxsTutelageDrawV1 { observers, .. } => observers.clone(),
        E::ProgenitusShuffledV1 { source, .. } => vec![*source],
        E::OmniscienceCastOptionsV1 { options } => vec![options.source],
        E::LathrilCombatDamageV1 { source, .. } => vec![*source],
        E::HarvesterCommitReceiptV1 { journal } => match journal.as_ref() {
            crate::harvester_receipts_v1::JournalV1::Published(publication) => publication
                .item
                .v4
                .ability_source_contract
                .into_iter()
                .collect(),
            _ => vec![],
        },
        E::LathrilReceiptV1 { journal } => match journal.as_ref() {
            crate::lathril_receipts_v1::JournalV1::Paid { item, .. } => {
                item.v4.ability_source_contract.into_iter().collect()
            }
            _ => vec![],
        },
        E::KinshipEntryV1 { journal } => match journal.as_ref() {
            crate::banner_of_kinship_v1::entry::JournalV1::Committed { entered } => {
                vec![entered.source]
            }
            crate::banner_of_kinship_v1::entry::JournalV1::Captured { choice, .. } => {
                choice.replacement.seasons.clone()
            }
            _ => vec![],
        },
        E::FiendArtisanPaidV1 { item, .. } => item.v4.ability_source_contract.into_iter().collect(),
        E::KaitoCunningV75 { .. } => crate::effect::kaito_cunning_v75::archived_sources_v1(event),
        E::VivienReidV1 { .. } => crate::effect::vivien_reid_v1::archived_sources_v1(event),
        E::TinybonesV72 { journal } => match journal.as_ref() {
            crate::engine::tinybones_v72::JournalV72::Discarded(discard) => {
                std::iter::once(discard.card)
                    .chain(discard.observers.iter().copied())
                    .collect()
            }
            _ => vec![],
        },
        E::ProtectionSourceColorDepartureV1 { snapshot, .. } => vec![snapshot.source],
        E::ControlledCreatureDeathCaptureV1 { source, .. }
        | E::CuratorEnteredV1 { source, .. }
        | E::NivMizzetVisionaryDamageSourceDepartureV1 { source, .. } => vec![*source],
        E::NivMizzetVisionaryDamageV1 {
            observer,
            damage_source,
            ..
        } => vec![*observer, *damage_source],
        E::DrakusethDamageSourceV76 { journal } => match journal.as_ref() {
            crate::drakuseth_damage_source_v76::JournalV76::Departure { facts, .. } => {
                vec![facts.source]
            }
            crate::drakuseth_damage_source_v76::JournalV76::Published { item } => {
                item.v4.ability_source_contract.into_iter().collect()
            }
            _ => vec![],
        },
        E::GreatwurmCombatCaptureV1 { capture } => std::iter::once(capture.creature)
            .chain(capture.observers.iter().copied())
            .collect(),
        E::FiendArtisanConsumerReceiptV1 { journal } => match journal.as_ref() {
            crate::fiend_artisan_receipts_v1::JournalV1::Sacrificed { source, .. } => vec![*source],
            _ => vec![],
        },
        E::EtaliPrimalStormReceiptV1 { journal } => match journal.as_ref() {
            crate::etali_primal_storm_receipts_v1::JournalV1::Published { item, .. } => {
                item.v4.ability_source_contract.into_iter().collect()
            }
            _ => vec![],
        },
        _ => vec![],
    }
}

/// Called only for an already validated sealed FullPool root. Each newly
/// admitted source must also pass ordinary validation. Recursive samples can
/// retain an older exception only while its exact indexed receipt still matches.
#[cfg(feature = "limited-fdn-fixtures")]
pub(crate) fn capture(state: &GameState) -> Result<CustodyV1, String> {
    let mut proofs = Vec::new();
    for (at, event) in state.engine.event_history.iter().enumerate() {
        if let E::ProtectionSourceColorDepartureV1 { snapshot, .. } = event {
            // Only later physical hand/library incarnations may be rewritten.
            // Preserve already-proven history after that incarnation is revealed.
            let live = state
                .objects
                .try_get(snapshot.source.source)
                .ok_or("color departure source absent")?;
            if live.spell_copy_origin.is_some()
                || live.v4.is_token
                || !(matches!(live.zone, Zone::Hand | Zone::Library)
                    || allows_rebound_source(state, at, snapshot.source))
            {
                continue;
            }
        }
        let contracts = sources(event);
        if contracts.is_empty() {
            continue;
        }
        let event_hash = fingerprint(event)?;
        for source in contracts {
            validate_source(state, at, source)?;
            proofs.push(ProofV1 {
                at,
                source,
                event: event_hash,
            });
        }
    }
    let mut targets = Vec::new();
    let mut items = state
        .stack
        .iter()
        .chain(
            state
                .engine
                .pending_effect
                .as_ref()
                .map(|p| &p.resolving_item),
        )
        .map(|item| (item, None))
        .collect::<Vec<_>>();
    if state
        .engine
        .event_history
        .has_any(&[crate::event_history_v1::EventKindV1::DrakusethDamageSourceV76])
    {
        crate::drakuseth_damage_source_v76::validate_state(state)?;
        for (at, event) in state.engine.event_history.iter().enumerate() {
            if let E::DrakusethDamageSourceV76 { journal } = event {
                if let crate::drakuseth_damage_source_v76::JournalV76::Published { item } =
                    journal.as_ref()
                {
                    items.push((item.as_ref(), Some((at, fingerprint(event)?))));
                }
            }
        }
    }
    for (item, archived) in items {
        if item.targets.is_empty() {
            continue;
        }
        if archived.is_none() {
            crate::engine::validated_stack_item_target_spec(item, state)?;
        }
        let item_hash = fingerprint(item)?;
        let refs = crate::rl::frozen_stack_target_refs(state, item).map_err(|e| e.to_string())?;
        for (slot, (&contract, projected)) in item.v4.target_contracts.iter().zip(refs).enumerate()
        {
            let (
                StackTargetContractV4::Object { zone, .. },
                crate::rl::TargetRefV1::Object { object },
            ) = (contract, projected)
            else {
                continue;
            };
            // A private chooser's hand/library target must never become an
            // actor-independent frozen reference in a later opponent rollout.
            if matches!(zone, Zone::Hand | Zone::Library) {
                continue;
            }
            let proof = TargetProofV1 {
                item: item_hash,
                archived,
                slot,
                contract,
                public_face: object.card_db_id,
            };
            if !targets.contains(&proof) {
                targets.push(proof);
            }
        }
    }
    Ok(CustodyV1(
        Arc::new(proofs),
        Arc::new(targets),
        Arc::new(ability_sources::capture(state)?),
    ))
}

/// This is a historical face only. It does not make a stale incarnation legal
/// as a current target and never changes native resolution/fizzle checks.
pub(crate) fn object_target_face(
    state: &GameState,
    item: &StackItem,
    slot: usize,
    contract: StackTargetContractV4,
) -> Option<u16> {
    let StackTargetContractV4::Object {
        object,
        owner,
        zone,
        zone_change_count,
        spell_copy_origin,
        ..
    } = contract
    else {
        return None;
    };
    let live = state.objects.try_get(object)?;
    if matches!(zone, Zone::Hand | Zone::Library)
        || live.owner != owner
        || live.zone_change_count <= zone_change_count
        || spell_copy_origin.is_some()
        || live.spell_copy_origin != spell_copy_origin
        || item.v4.target_contracts.get(slot) != Some(&contract)
        || item.targets.get(slot) != Some(&contract.target())
    {
        return None;
    }
    let item_hash = fingerprint(item).ok()?;
    state
        .sampled_history_v1
        .1
        .iter()
        .find(|proof| {
            proof.item == item_hash
                && proof.slot == slot
                && proof.contract == contract
                && proof.archived.is_none_or(|(at, digest)| {
                    state
                        .engine
                        .event_history
                        .get(at)
                        .and_then(|event| fingerprint(event).ok())
                        == Some(digest)
                })
        })
        .map(|proof| proof.public_face)
}

pub(crate) fn validate_source(
    state: &GameState,
    at: usize,
    source: AbilitySourceContractV4,
) -> Result<(), String> {
    let ordinary = crate::engine::validate_historical_ability_source_contract(state, source);
    if ordinary.is_ok() || allows_rebound_source(state, at, source) {
        Ok(())
    } else {
        ordinary
    }
}

pub(crate) fn allows_rebound_source(
    state: &GameState,
    at: usize,
    source: AbilitySourceContractV4,
) -> bool {
    let Some(event) = state.engine.event_history.get(at) else {
        return false;
    };
    if !sources(event).contains(&source) {
        return false;
    }
    let Ok(digest) = fingerprint(event) else {
        return false;
    };
    if !state
        .sampled_history_v1
        .0
        .iter()
        .any(|proof| proof.at == at && proof.source == source && proof.event == digest)
    {
        return false;
    }
    let Some(live) = state.objects.try_get(source.source) else {
        return false;
    };
    // A same-generation exception is needed only for a revealed card shuffled
    // within its library. All other same-generation/live contracts stay strict.
    let same_library_shuffle = source.zone == Zone::Library
        && matches!(event,
        E::ProgenitusShuffledV1 { source: original, move_history_index: None, .. }
            if *original == source);
    if live.owner != source.owner
        || live.zone_change_count < source.zone_change_count
        || live.zone_change_count == source.zone_change_count
            && (!same_library_shuffle || live.zone != source.zone)
    {
        return false;
    }
    crate::engine::validate_historical_permanent_copy_source(state, source, live).is_ok()
}
