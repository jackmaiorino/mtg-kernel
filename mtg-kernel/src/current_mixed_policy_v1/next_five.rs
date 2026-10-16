//! Distinct numeric NextFive V59 owner. No predecessor artifact or executable is encoded.
use super::{normalize, Error, NativeModel, Packet, Result, Source};
use crate::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, NextFiveDecisionV59,
    NextFiveSessionV59, NextFiveSnapshotV59,
};
use serde_json::{json, Map, Value};
use std::{collections::BTreeSet, sync::Arc};

pub struct NextFiveSession {
    inner: NextFiveSessionV59,
    owner: Arc<()>,
    generation: u64,
}
#[derive(Clone)]
pub struct NextFiveSnapshot {
    inner: NextFiveSnapshotV59,
    owner: Arc<()>,
}
/// Private executable authority stays in the sealed backend capability.
/// This type deliberately has no Debug, serde or public constructor.
#[derive(Clone)]
pub struct NextFiveDecision {
    backend: NextFiveDecisionV59,
    packet: Packet,
    owner: Arc<()>,
    generation: u64,
}
impl NextFiveDecision {
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
impl NextFiveSession {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self> {
        Ok(Self::from_next_five(
            NextFiveSessionV59::from_session(session)
                .map_err(|e| Error::Projection(e.to_string()))?,
        ))
    }
    /// Transfer a genuine sealed current owner, never an observation/world DTO.
    pub fn from_next_five(inner: NextFiveSessionV59) -> Self {
        Self {
            inner,
            owner: Arc::new(()),
            generation: 0,
        }
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(super) fn fixture_state(&self) -> &crate::state::GameState {
        self.inner.fixture_state_v59()
    }
    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(super) fn fixture_counts(&self) -> (u64, u64) {
        self.inner.fixture_counts_v59()
    }
    pub fn snapshot(&self) -> NextFiveSnapshot {
        NextFiveSnapshot {
            inner: self.inner.snapshot_v59(),
            owner: self.owner.clone(),
        }
    }
    pub fn restore(&mut self, snapshot: &NextFiveSnapshot) -> Result<()> {
        if !Arc::ptr_eq(&self.owner, &snapshot.owner) {
            return Err(Error::ForeignSnapshot);
        }
        let next = self.generation.checked_add(1).ok_or(Error::Range)?;
        // Backend authenticates the original binding before monotonic rebind.
        self.inner
            .restore_v59(&snapshot.inner)
            .map_err(|_| Error::StaleBinding)?;
        self.generation = next;
        Ok(())
    }
    pub fn encode(&self, expected: FastActorDecisionV1) -> Result<NextFiveDecision> {
        let backend = self.inner.decision_v59().map_err(|_| Error::StaleBinding)?;
        if backend.expected() != expected {
            return Err(Error::StaleBinding);
        }
        let state = serde_json::to_value(backend.observation())
            .map_err(|e| Error::Projection(e.to_string()))?;
        let actions = backend
            .actions()
            .iter()
            .map(serde_json::to_value)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Projection(e.to_string()))?;
        let packet = normalize::encode(Source::NextFiveV59, state, actions)?;
        Ok(NextFiveDecision {
            backend,
            packet,
            owner: self.owner.clone(),
            generation: self.generation,
        })
    }
    pub fn apply(
        &mut self,
        decision: &NextFiveDecision,
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
        self.inner
            .validate_decision_v59(&decision.backend)
            .map_err(|_| Error::StaleBinding)?;
        if self.encode(decision.backend.expected())?.packet != decision.packet {
            return Err(Error::StaleBinding);
        }
        // The real owning step repeats complete current private validation
        // immediately before applying the original native ordered index.
        self.inner
            .step_v59(&decision.backend, selected_index)
            .map_err(|e| Error::Projection(e.to_string()))
    }
    pub fn score_and_apply(
        &mut self,
        decision: &NextFiveDecision,
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

/// Authenticate complete closed V59 programs before any identity/digest fields
/// are removed. All ordered rules and all full width public facts survive.
pub(super) fn typed_meaning(
    value: &Map<String, Value>,
    covered: &mut BTreeSet<u16>,
) -> Result<bool> {
    if value.get("schema").and_then(Value::as_str)
        == Some(crate::rules_meaning_next_five_v59::SCHEMA)
    {
        let root: crate::rules_meaning_next_five_v59::MeaningV59 =
            serde_json::from_value(Value::Object(value.clone()))
                .map_err(|e| Error::Projection(e.to_string()))?;
        crate::rules_meaning_next_five_v59::validate(&root).map_err(Error::Projection)?;
        covered.insert(root.card_id);
        return Ok(true);
    }
    if value
        .get("selector")
        .and_then(|s| s.get("schema"))
        .and_then(Value::as_str)
        == Some(crate::rules_meaning_midnight_snack_v58::SCHEMA)
    {
        let root: crate::rules_meaning_midnight_snack_v58::MeaningV58 =
            serde_json::from_value(Value::Object(value.clone()))
                .map_err(|e| Error::Projection(e.to_string()))?;
        crate::rules_meaning_midnight_snack_v58::validate(&root).map_err(Error::Projection)?;
        covered.insert(root.card_id);
        return Ok(true);
    }
    Ok(false)
}

/// Mandatory even when an ability root already covers this printing. The
/// selectors exhaustively validate every CardDef field and the native recipe.
/// Food retains its complete closed paid activation, including turn gain use.
pub(super) fn printing(id: u16) -> Result<Option<Value>> {
    use crate::rules_meaning_next_five_v59::{self as meaning, FamilyV59};
    let definition = crate::card_def::CARD_DEFS
        .get(usize::from(id))
        .ok_or(Error::UnsupportedPrintedRoot(id))?;
    for family in FamilyV59::ALL {
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
    if definition.name == "Food Token" {
        let snack = meaning::printed_meaning(FamilyV59::Snack).map_err(Error::Projection)?;
        let snack = crate::rules_meaning_midnight_snack_v58::printed_meaning(snack.card_id)
            .map_err(Error::Projection)?;
        if snack.food.card_id != id {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
        return Ok(Some(json!({"card_id":id,"food_printing":snack.food})));
    }
    super::next_five_support::printing(id)
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
