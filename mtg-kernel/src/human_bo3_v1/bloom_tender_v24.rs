//! Explicit bundle labels over genuine V8 input. Frozen labels stay unchanged.
use super::{
    visible::{self, Handles},
    *,
};
use crate::{
    bloom_tender_public_v1::BloomTenderObservationV1,
    rl::ActionSemanticV1,
    rl_session::{
        human_v8::HumanDecisionBindingV8, FastActorResponseV1, FastActorSessionSnapshotV1,
    },
};
use std::{collections::BTreeMap, sync::Arc};
type Error = HumanDecisionErrorV1;

#[derive(Clone)]
struct Owner {
    id: Arc<()>,
    generation: u64,
}
impl PartialEq for Owner {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.id, &other.id) && self.generation == other.generation
    }
}
impl Eq for Owner {}
pub struct HumanBloomTenderSessionV24 {
    inner: FastActorSessionV1,
    owner: Owner,
}
#[derive(Clone)]
pub struct HumanBloomTenderSnapshotV24 {
    inner: FastActorSessionSnapshotV1,
    owner: Arc<()>,
}
impl HumanBloomTenderSessionV24 {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self, crate::rl::RlContractError> {
        let inner = session.into_human_v8()?;
        if let FastActorResponseV1::Decision(expected) = inner.current_response() {
            inner
                .bloom_tender_current_input_v24(expected, expected.acting_player)
                .map_err(|_| crate::rl::RlContractError("unsupported Bloom Tender input".into()))?;
        }
        Ok(Self {
            inner,
            owner: Owner {
                id: Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<BloomTenderObservationV1, Error> {
        self.inner
            .bloom_tender_current_input_v24(expected, expected.acting_player)
            .map(|(o, _, _)| o)
            .map_err(|_| Error::StaleDecision)
    }
    pub fn snapshot_v24(&self) -> HumanBloomTenderSnapshotV24 {
        HumanBloomTenderSnapshotV24 {
            inner: self.inner.snapshot_v1(),
            owner: self.owner.id.clone(),
        }
    }
    pub fn restore_v24(&mut self, snapshot: &HumanBloomTenderSnapshotV24) -> Result<(), Error> {
        if !Arc::ptr_eq(&self.owner.id, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let generation = self
            .owner
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        let mut staged = self.inner.clone();
        staged.restore_v1(&snapshot.inner);
        if let FastActorResponseV1::Decision(expected) = staged.current_response() {
            staged
                .bloom_tender_current_input_v24(expected, expected.acting_player)
                .map_err(|_| Error::StaleDecision)?;
        }
        self.inner = staged;
        self.owner.generation = generation;
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanBloomTenderBundleV24 {
    pub source: HumanCardRefV1,
    pub colors: Vec<crate::mana::ManaColor>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanBloomTenderExtensionsV24 {
    pub predecessor: HumanExtensionsV8,
    pub mana_bundles: Vec<HumanBloomTenderBundleV24>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionBloomTenderV24 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanBloomTenderExtensionsV24>,
    pub actions: Vec<HumanLegalActionV1>,
}

fn state(
    observation: &BloomTenderObservationV1,
    handles: &mut Handles,
) -> Result<HumanVisibleStateV1<HumanBloomTenderExtensionsV24>, Error> {
    let old = super::life_ward_v8::project_state(&observation.predecessor, handles)?;
    let mana_bundles = observation
        .bundles
        .iter()
        .map(|row| {
            Ok(HumanBloomTenderBundleV24 {
                source: handles.get(&row.source)?.clone(),
                colors: row.colors.clone(),
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        extensions: HumanBloomTenderExtensionsV24 {
            predecessor: old.extensions,
            mana_bundles,
        },
        policy_context: old.policy_context,
        known_face_down_cards: old.known_face_down_cards,
        poison_counters: old.poison_counters,
        poison_prevention: old.poison_prevention,
        creatures_attacked_this_turn: old.creatures_attacked_this_turn,
        ninja_emblems: old.ninja_emblems,
        restricted_mana: old.restricted_mana,
    })
}
fn label(
    action: &ActionSemanticV1,
    observation: &BloomTenderObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    if let ActionSemanticV1::ActivateManaAbility { actor, source, .. } = action {
        if let Some(bundle) = observation.bundles.iter().find(|b| b.source == *source) {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            let mana = bundle
                .colors
                .iter()
                .map(|color| format!("{{{color:?}}}"))
                .collect::<String>();
            return Ok(format!(
                "Tap {}: add {} (one of each color among permanents you control)",
                handles.name(source)?,
                if mana.is_empty() { "no mana" } else { &mana }
            ));
        }
    }
    super::labels::label_v8(action, &observation.predecessor, handles, human)
}
type Projected = (
    HumanVisibleStateV1<HumanBloomTenderExtensionsV24>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);
fn project(
    observation: &BloomTenderObservationV1,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
) -> Result<Projected, Error> {
    let old = &observation.predecessor;
    if old.acting_player != human {
        return Err(Error::NotHumanTurn);
    }
    if actions.is_empty() {
        return Err(Error::UnsupportedPrompt);
    }
    let mut handles = Handles::from_parts(
        &old.projection,
        &old.own_hand,
        &old.known_hand_cards,
        &old.known_library_cards,
        old.extensions.decision_local_library.as_ref(),
    )?;
    for row in &old.projection.surface.stack {
        handles.insert(&row.source)?;
    }
    for row in &old.extensions.historical_public_sources {
        handles.insert(&row.source)?;
    }
    let initial = state(observation, &mut handles)?;
    let mut unary = BTreeMap::<String, Vec<String>>::new();
    for row in &initial.extensions.predecessor.ward_life_card_traits {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &initial.extensions.mana_bundles {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for rows in unary.values_mut() {
        rows.sort();
    }
    handles.canonicalize_parts(&initial, unary)?;
    let state = state(observation, &mut handles)?;
    let mut choices = actions
        .iter()
        .enumerate()
        .map(|(i, a)| {
            Ok((
                label(a, observation, &handles, human)?,
                u32::try_from(i).map_err(|_| Error::InvalidAction)?,
            ))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    choices.sort_by(|a, b| a.0.cmp(&b.0));
    if choices.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(Error::UnsupportedPrompt);
    }
    let indexes = choices.iter().map(|(_, i)| *i).collect();
    let actions = choices
        .into_iter()
        .enumerate()
        .map(|(i, (label, _))| HumanLegalActionV1 {
            action_index: i as u32,
            label,
        })
        .collect();
    Ok((state, actions, indexes))
}
struct Bound {
    owner: Owner,
    expected: FastActorDecisionV1,
    binding: HumanDecisionBindingV8,
    public: BloomTenderObservationV1,
    visible: HumanDecisionBloomTenderV24,
    indexes: Vec<u32>,
}
pub struct HumanBloomTenderProjectorV24 {
    seat: PlayerSeatV1,
    next: u64,
    pending: Option<Bound>,
    accepted: Option<(HumanActionReceiptV1, Owner)>,
}
impl HumanBloomTenderProjectorV24 {
    pub fn new(seat: PlayerSeatV1) -> Self {
        Self {
            seat,
            next: 1,
            pending: None,
            accepted: None,
        }
    }
    pub fn project_current(
        &mut self,
        session: &HumanBloomTenderSessionV24,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionBloomTenderV24, Error> {
        let (public, actions, binding) = session
            .inner
            .bloom_tender_current_input_v24(expected, self.seat)
            .map_err(|_| Error::StaleDecision)?;
        if let Some(old) = &self.pending {
            if old.owner == session.owner
                && old.expected == expected
                && old.binding == binding
                && old.public == public
            {
                return Ok(old.visible.clone());
            }
        }
        let next = self
            .next
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        let (state, actions, indexes) = project(&public, &actions, self.seat)?;
        let visible = HumanDecisionBloomTenderV24 {
            schema: "mtg-kernel-human-bloom-tender-decision/v24".into(),
            human_seat: self.seat,
            prompt_seq: self.next,
            state,
            actions,
        };
        self.pending = Some(Bound {
            owner: session.owner.clone(),
            expected,
            binding,
            public,
            visible: visible.clone(),
            indexes,
        });
        self.next = next;
        Ok(visible)
    }
    pub fn submit(
        &mut self,
        session: &mut HumanBloomTenderSessionV24,
        request: HumanActionRequestV1,
    ) -> Result<HumanActionReceiptV1, Error> {
        if let Some((receipt, owner)) = &self.accepted {
            if receipt.prompt_seq == request.prompt_seq {
                if *owner != session.owner {
                    return Err(Error::StaleDecision);
                }
                return if receipt.action_index == request.action_index {
                    Ok(receipt.clone())
                } else {
                    Err(Error::ConflictingRetry)
                };
            }
        }
        let old = self.pending.as_ref().ok_or(Error::StaleDecision)?;
        if old.owner != session.owner || old.visible.prompt_seq != request.prompt_seq {
            return Err(Error::StaleDecision);
        }
        let index = *old
            .indexes
            .get(request.action_index as usize)
            .ok_or(Error::InvalidAction)?;
        session
            .inner
            .human_validate_bound_decision_v8(old.expected, self.seat, &old.binding)
            .map_err(|_| Error::StaleDecision)?;
        if session.public_observation(old.expected)? != old.public {
            return Err(Error::StaleDecision);
        }
        let result = session
            .inner
            .step(old.expected.episode_id, old.expected.step, index);
        self.pending = None;
        result.map_err(|_| Error::ExecutionFailed)?;
        let receipt = HumanActionReceiptV1 {
            prompt_seq: request.prompt_seq,
            action_index: request.action_index,
        };
        self.accepted = Some((receipt.clone(), session.owner.clone()));
        Ok(receipt)
    }
}
#[cfg(test)]
mod tests;
