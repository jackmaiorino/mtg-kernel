//! Distinct numeric GraveyardReturn V61 owner. No predecessor artifact or executable is encoded.
use super::{normalize, Error, NativeModel, Packet, Result, Source};
use crate::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, GraveyardReturnDecisionV61,
    GraveyardReturnSessionV61, GraveyardReturnSnapshotV61,
};
use serde_json::{Map, Value};
use std::{collections::BTreeSet, sync::Arc};

pub struct GraveyardReturnSession {
    inner: GraveyardReturnSessionV61,
    owner: Arc<()>,
    generation: u64,
}
#[derive(Clone)]
pub struct GraveyardReturnSnapshot {
    inner: GraveyardReturnSnapshotV61,
    owner: Arc<()>,
}
/// Private executable authority stays in the sealed backend capability.
/// This type deliberately has no Debug, serde or public constructor.
#[derive(Clone)]
pub struct GraveyardReturnDecision {
    backend: GraveyardReturnDecisionV61,
    packet: Packet,
    owner: Arc<()>,
    generation: u64,
}
impl GraveyardReturnDecision {
    pub fn packet(&self) -> &Packet {
        &self.packet
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(super) fn fixture_packet_mut(&mut self) -> &mut Packet {
        &mut self.packet
    }
}
// Pure normalization of the exact freshly validated backend input. This
// function cannot capture another decision or acquire executable authority.
fn packet_from_current(
    candidates: &crate::graveyard_return_candidates_v61::GraveyardReturnCandidatesV61,
    actions: &[crate::rl::ActionSemanticV1],
) -> Result<Packet> {
    crate::graveyard_return_candidates_v61::validate_artifact(candidates)
        .map_err(|e| Error::Projection(e.to_string()))?;
    if candidates.candidates.len() != actions.len()
        || candidates
            .candidates
            .iter()
            .zip(actions)
            .any(|(row, action)| &row.action != action)
    {
        return Err(Error::StaleBinding);
    }
    // The complete observation and every typed per-action program/counter/LKI
    // annotation remain present. Raw executable pairs and index maps are never encoded.
    let state = serde_json::to_value(candidates).map_err(|e| Error::Projection(e.to_string()))?;
    let actions = candidates
        .candidates
        .iter()
        .map(serde_json::to_value)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Projection(e.to_string()))?;
    normalize::encode(Source::GraveyardReturnV61, state, actions)
}
impl GraveyardReturnSession {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self> {
        Ok(Self::from_graveyard_return(
            GraveyardReturnSessionV61::from_session(session)
                .map_err(|e| Error::Projection(e.to_string()))?,
        ))
    }
    /// Transfer a genuine sealed current owner, never an observation/world DTO.
    pub fn from_graveyard_return(inner: GraveyardReturnSessionV61) -> Self {
        Self {
            inner,
            owner: Arc::new(()),
            generation: 0,
        }
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.response()
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(super) fn fixture_state(&self) -> &crate::state::GameState {
        self.inner.fixture_state_v61()
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(super) fn fixture_counts(&self) -> (u64, u64) {
        self.inner.fixture_counts_v61()
    }
    pub fn snapshot(&self) -> GraveyardReturnSnapshot {
        GraveyardReturnSnapshot {
            inner: self.inner.snapshot_v61(),
            owner: self.owner.clone(),
        }
    }
    pub fn restore(&mut self, snapshot: &GraveyardReturnSnapshot) -> Result<()> {
        if !Arc::ptr_eq(&self.owner, &snapshot.owner) {
            return Err(Error::ForeignSnapshot);
        }
        let next = self.generation.checked_add(1).ok_or(Error::Range)?;
        // Backend authenticates the original binding before monotonic rebind.
        self.inner
            .restore_v61(&snapshot.inner)
            .map_err(|_| Error::StaleBinding)?;
        self.generation = next;
        Ok(())
    }
    pub fn encode(&self, expected: FastActorDecisionV1) -> Result<GraveyardReturnDecision> {
        let backend = self.inner.decision_v61().map_err(|_| Error::StaleBinding)?;
        if backend.expected() != expected {
            return Err(Error::StaleBinding);
        }
        let packet = packet_from_current(backend.candidates_v61(), backend.actions())?;
        Ok(GraveyardReturnDecision {
            backend,
            packet,
            owner: self.owner.clone(),
            generation: self.generation,
        })
    }
    pub fn apply(
        &mut self,
        decision: &GraveyardReturnDecision,
        selected_index: u32,
    ) -> Result<FastActorResponseV1> {
        if !Arc::ptr_eq(&self.owner, &decision.owner) || self.generation != decision.generation {
            return Err(Error::StaleBinding);
        }
        if usize::try_from(selected_index)
            .ok()
            .is_none_or(|i| i >= decision.packet.actions.len())
        {
            return Err(Error::ActionRange);
        }
        self.inner.step_with_current_check(
            &decision.backend,
            selected_index,
            |candidates, actions| {
                if packet_from_current(candidates, actions)? != decision.packet {
                    return Err(Error::StaleBinding);
                }
                Ok(())
            },
            |_| Error::StaleBinding,
            |error| Error::Projection(error.to_string()),
        )
    }
    pub fn score_and_apply(
        &mut self,
        decision: &GraveyardReturnDecision,
        model: &NativeModel,
    ) -> Result<(Vec<f32>, u32, FastActorResponseV1)> {
        let logits = model.forward(&decision.packet)?;
        if logits.len() != decision.packet.actions.len() || logits.is_empty() {
            return Err(Error::ModelContract);
        }
        let mut selected = 0;
        for index in 1..logits.len() {
            if logits[index] > logits[selected] {
                selected = index;
            }
        }
        let selected = u32::try_from(selected).map_err(|_| Error::Range)?;
        let response = self.apply(decision, selected)?;
        Ok((logits, selected, response))
    }
}

/// Validate the whole closed program before private normalization removes
/// identity/digest fields. Its full ordered rules remain in the numeric tree.
pub(super) fn typed_meaning(
    value: &Map<String, Value>,
    covered: &mut BTreeSet<u16>,
) -> Result<bool> {
    if value.get("schema").and_then(Value::as_str)
        != Some(crate::rules_meaning_graveyard_return_v61::SCHEMA)
    {
        return Ok(false);
    }
    let root: crate::rules_meaning_graveyard_return_v61::MeaningV61 =
        serde_json::from_value(Value::Object(value.clone()))
            .map_err(|e| Error::Projection(e.to_string()))?;
    crate::rules_meaning_graveyard_return_v61::validate(&root).map_err(Error::Projection)?;
    covered.insert(root.card_id);
    Ok(true)
}

/// A referenced printing always retains its complete immutable cast/entry and
/// ability program, including both counter timing and original-controller rules.
pub(super) fn printing(id: u16) -> Result<Option<Value>> {
    use crate::rules_meaning_graveyard_return_v61::{self as meaning, FamilyV61};
    let definition = crate::card_def::CARD_DEFS
        .get(usize::from(id))
        .ok_or(Error::UnsupportedPrintedRoot(id))?;
    for family in FamilyV61::ALL {
        if definition.name == family.name() {
            let root = meaning::printed_meaning(family).map_err(Error::Projection)?;
            if root.card_id != id {
                return Err(Error::UnsupportedPrintedRoot(id));
            }
            return serde_json::to_value(root)
                .map(Some)
                .map_err(|e| Error::Projection(e.to_string()));
        }
    }
    Ok(None)
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
