//! Sealed Akroma successor. Complete typed authority stays backend-only.
use super::visible::{self, Handles};
use super::*;
use crate::akroma_public_v1::AkromaObservationV1;
use crate::rl::ActionSemanticV1;
use crate::rl_session::akroma_v40::AkromaDecisionBindingV40;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct AuthorityV40 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV40 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV40 {}
pub struct HumanAkromaSessionV40 {
    inner: FastActorSessionV1,
    authority: AuthorityV40,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanAkromaSnapshotV40 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanAkromaSessionV40 {
    pub fn from_session(
        session: FastActorSessionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_akroma_v40()?,
            human_seat,
            authority: AuthorityV40 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v40(&self) -> HumanAkromaSnapshotV40 {
        HumanAkromaSnapshotV40 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v40(&mut self, snapshot: &HumanAkromaSnapshotV40) -> Result<(), Error> {
        if !std::sync::Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let next = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .akroma_restore_v40(&snapshot.snapshot)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = next;
        Ok(())
    }
    /// Public typed transport, not the safe Human DTO and not scorer admission.
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::akroma_public_v1::AkromaObservationV1, crate::rl::RlContractError> {
        if expected.acting_player != self.human_seat {
            return Err(crate::rl::RlContractError(
                "Akroma Human view is fixed to its selected seat".into(),
            ));
        }
        self.inner
            .akroma_current_decision_input_v40(expected, self.human_seat)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale Akroma public decision".into()))
    }
    /// Host-only policy callback. Opponent private rosters never enter a Human DTO.
    pub fn step_opponent_with<F>(
        &mut self,
        expected: FastActorDecisionV1,
        mut policy: F,
    ) -> Result<FastActorResponseV1, crate::rl::RlContractError>
    where
        F: FnMut(
            &AkromaObservationV1,
            &[ActionSemanticV1],
        ) -> Result<u32, crate::rl::RlContractError>,
    {
        if expected.acting_player == self.human_seat {
            return Err(crate::rl::RlContractError(
                "opponent callback cannot act for the Human seat".into(),
            ));
        }
        let (observation, actions, binding) = self
            .inner
            .akroma_current_decision_input_v40(expected, expected.acting_player)
            .map_err(|_| crate::rl::RlContractError("stale opponent input".into()))?;
        let index = policy(&observation, &actions)?;
        if index as usize >= actions.len() {
            return Err(crate::rl::RlContractError(
                "opponent action out of range".into(),
            ));
        }
        self.inner
            .akroma_validate_bound_decision_v40(expected, expected.acting_player, &binding)
            .map_err(|_| crate::rl::RlContractError("opponent input changed".into()))?;
        self.inner
            .step(expected.episode_id, expected.step, index)
            .map_err(|error| crate::rl::RlContractError(format!("opponent execution:{error:?}")))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionAkromaV40 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanAkromaExtensionsV40>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanAkromaExtensionsV40 {
    pub predecessor: HumanExtensionsV9,
    pub grants: Vec<HumanAkromaGrantV40>,
    pub families: Vec<super::akroma_families_v40::HumanFamilyV40>,
    pub color_departures: Vec<HumanColorDepartureV40>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanAkromaGrantV40 {
    pub source: HumanCardRefV1,
    pub controller: PlayerSeatV1,
    pub recipients: Vec<HumanCardRefV1>,
    pub keyword_mask: u32,
    pub protection_color_mask: u8,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanColorDepartureV40 {
    pub source_at_departure: HumanCardRefV1,
    pub event_order: u64,
    pub original_face_index: u8,
    pub was_face_down: bool,
    pub color_mask: u8,
}
fn parts(observation: &AkromaObservationV1) -> super::life_ward_v8::ProjectionPartsV8<'_> {
    super::life_ward_v8::ProjectionPartsV8 {
        projection: &observation.projection,
        own_hand: &observation.own_hand,
        known_library_cards: &observation.known_library_cards,
        known_hand_cards: &observation.known_hand_cards,
        known_face_down_cards: &observation.known_face_down_cards,
        extensions: &observation.extensions,
    }
}
fn state(
    observation: &AkromaObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
) -> Result<HumanVisibleStateV1<HumanAkromaExtensionsV40>, Error> {
    let old = super::life_ward_v8::project_state_parts(&parts(observation), handles)?;
    let seat = if human == PlayerSeatV1::P0 { 0 } else { 1 };
    let mut grants = observation
        .grants
        .iter()
        .map(|row| {
            let mut recipients = row
                .recipients
                .iter()
                .map(|card| handles.get(card).cloned())
                .collect::<Result<Vec<_>, Error>>()?;
            recipients.sort_by(|a, b| a.handle.cmp(&b.handle));
            Ok(HumanAkromaGrantV40 {
                source: handles.get(&row.source)?.clone(),
                controller: row.controller,
                recipients,
                keyword_mask: row.keyword_mask,
                protection_color_mask: row.protection_color_mask,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    grants.sort_by(|a, b| a.source.handle.cmp(&b.source.handle));
    let color_departures = observation
        .color_departures
        .iter()
        .map(|row| {
            Ok(HumanColorDepartureV40 {
                source_at_departure: handles.get(&row.source_at_departure)?.clone(),
                event_order: row.event_order,
                original_face_index: row.original_face_index,
                was_face_down: row.was_face_down,
                color_mask: row.color_mask,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        extensions: HumanAkromaExtensionsV40 {
            predecessor: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: observation.attacked_this_turn[seat],
                opponent_attacked_this_turn: observation.attacked_this_turn[1 - seat],
            },
            grants,
            families: super::akroma_families_v40::states(families, handles, human)?,
            color_departures,
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

fn action_source_id(action: &ActionSemanticV1) -> Option<u16> {
    use ActionSemanticV1::*;
    match action {
        PlayLand { source, .. }
        | CastSpell { source, .. }
        | ActivateManaAbility { source, .. }
        | ActivateAbility { source, .. }
        | PlotSpell { source, .. }
        | ChooseTarget { source, .. }
        | ChooseCostTarget { source, .. }
        | ChooseCastMode { source, .. }
        | ChooseKicker { source, .. }
        | ChooseSpellMode { source, .. }
        | ChooseEffectOption { source, .. }
        | ChooseEffectTarget { source, .. }
        | FinishEffectSelection { source, .. }
        | ChooseEffectColor { source, .. }
        | ChooseEffectNumber { source, .. }
        | ChooseEffectBoolean { source, .. }
        | FinishTargetSelection { source, .. }
        | ChooseSpellCopyPayment { source, .. }
        | ChooseSpellCopyRetarget { source, .. }
        | ChooseCombatDamageRange { source, .. }
        | TurnFaceUp { source, .. } => Some(source.card_db_id),
        #[cfg(feature = "limited-fdn-fixtures")]
        ChooseTriggerOrderNext { source, .. } => Some(source.card_db_id),
        _ => None,
    }
}

fn label(
    action: &ActionSemanticV1,
    observation: &AkromaObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
) -> Result<String, Error> {
    if let ActionSemanticV1::CastSpell { source, actor, .. } = action {
        if crate::card_def::CARD_DEFS
            .get(usize::from(source.card_db_id))
            .is_some_and(|d| d.controlled_creature_grant.is_some())
        {
            if *actor != human {
                return Err(Error::NotHumanTurn);
            }
            let selector = crate::rules_meaning_akroma_v1::live_selector()
                .map_err(|_| Error::UnsupportedPrompt)?;
            crate::rules_meaning_akroma_v1::meaning(
                &selector,
                source.card_db_id,
                crate::rules_meaning_akroma_v1::RootV1::ControlledCreatureGrant,
            )
            .map_err(|_| Error::UnsupportedPrompt)?;
            return Ok(format!("Begin casting {} (printed cost {{7}}): your creatures have flying, first strike, vigilance, trample, haste and protection from black and red",handles.name(source)?));
        }
    }
    if let Some(label) = super::akroma_families_v40::label(
        action_source_id(action),
        action,
        families,
        handles,
        human,
    )? {
        return Ok(label);
    }
    super::labels::label_parts_v8(action, &parts(observation), handles, human)
}

type ProjectedAkromaV40 = (
    HumanVisibleStateV1<HumanAkromaExtensionsV40>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);

fn project_base(
    observation: &AkromaObservationV1,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
) -> Result<ProjectedAkromaV40, Error> {
    if observation.acting_player != human {
        return Err(Error::NotHumanTurn);
    }
    if actions.is_empty() {
        return Err(Error::UnsupportedPrompt);
    }
    observation
        .validate_artifact()
        .map_err(|_| Error::InvalidVisibleReference)?;
    let mut handles = Handles::from_parts(
        &observation.projection,
        &observation.own_hand,
        &observation.known_hand_cards,
        &observation.known_library_cards,
        observation.extensions.decision_local_library.as_ref(),
    )?;
    for row in &observation.projection.surface.stack {
        handles.insert(&row.source)?;
    }
    for row in &observation.extensions.historical_public_sources {
        handles.insert(&row.source)?;
    }
    for row in &observation.grants {
        handles.insert(&row.source)?;
        for recipient in &row.recipients {
            handles.insert(recipient)?;
        }
    }
    for row in &observation.color_departures {
        handles.insert(&row.source_at_departure)?;
    }
    super::akroma_families_v40::register(families, &mut handles)?;
    let initial = state(observation, &mut handles, human, families)?;
    let mut unary = BTreeMap::<String, Vec<String>>::new();
    for row in &initial
        .extensions
        .predecessor
        .predecessor
        .ward_life_card_traits
    {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &initial.extensions.grants {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
        for recipient in &row.recipients {
            unary
                .entry(recipient.handle.clone())
                .or_default()
                .push(format!(
                    "akroma_grant_{}_{}",
                    row.keyword_mask, row.protection_color_mask
                ));
        }
    }
    for row in &initial.extensions.color_departures {
        unary
            .entry(row.source_at_departure.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    super::akroma_families_v40::unary(&initial.extensions.families, &mut unary)?;
    for rows in unary.values_mut() {
        rows.sort();
        rows.dedup();
    }
    handles.canonicalize_parts(&initial, unary)?;
    let state = state(observation, &mut handles, human, families)?;
    let mut choices = actions
        .iter()
        .enumerate()
        .map(|(index, action)| {
            Ok((
                label(action, observation, &handles, human, families)?,
                u32::try_from(index).map_err(|_| Error::InvalidAction)?,
            ))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    choices.sort_by(|a, b| a.0.cmp(&b.0));
    if choices.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(Error::UnsupportedPrompt);
    }
    let indexes = choices.iter().map(|(_, index)| *index).collect();
    let actions = choices
        .into_iter()
        .enumerate()
        .map(|(index, (label, _))| HumanLegalActionV1 {
            action_index: index as u32,
            label,
        })
        .collect();
    Ok((state, actions, indexes))
}

struct BoundAkromaV40 {
    authority: AuthorityV40,
    expected: FastActorDecisionV1,
    binding: AkromaDecisionBindingV40,
    public: crate::akroma_public_v1::AkromaObservationV1,
    visible: HumanDecisionAkromaV40,
    engine_indexes: Vec<u32>,
}
pub struct HumanAkromaProjectorV40 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundAkromaV40>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV40)>,
}
impl HumanAkromaProjectorV40 {
    pub fn new(human_seat: PlayerSeatV1) -> Self {
        Self {
            human_seat,
            next_prompt_seq: 1,
            pending: None,
            accepted: None,
        }
    }
    pub fn project_current(
        &mut self,
        session: &HumanAkromaSessionV40,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionAkromaV40, Error> {
        if expected.acting_player != self.human_seat || self.human_seat != session.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .akroma_current_decision_input_v40(expected, self.human_seat)
            .map_err(|_| Error::StaleDecision)?;
        let public = session
            .public_observation(expected)
            .map_err(|_| Error::StaleDecision)?;
        if let Some(pending) = &self.pending {
            if pending.authority == session.authority
                && pending.expected == expected
                && pending.binding == binding
                && pending.public == public
            {
                return Ok(pending.visible.clone());
            }
        }
        let next = self
            .next_prompt_seq
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        let (state, actions, engine_indexes) =
            project_base(&observation, &actions, self.human_seat, &binding.families)?;
        let visible = HumanDecisionAkromaV40 {
            schema: "mtg-kernel-human-akroma-decision/v40".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundAkromaV40 {
            authority: session.authority.clone(),
            expected,
            binding,
            public,
            visible: visible.clone(),
            engine_indexes,
        });
        self.next_prompt_seq = next;
        Ok(visible)
    }
    pub fn submit(
        &mut self,
        session: &mut HumanAkromaSessionV40,
        request: HumanActionRequestV1,
    ) -> Result<HumanActionReceiptV1, Error> {
        if let Some((receipt, authority)) = &self.accepted {
            if receipt.prompt_seq == request.prompt_seq {
                if *authority != session.authority {
                    return Err(Error::StaleDecision);
                }
                return if receipt.action_index == request.action_index {
                    Ok(receipt.clone())
                } else {
                    Err(Error::ConflictingRetry)
                };
            }
        }
        let pending = self.pending.as_ref().ok_or(Error::StaleDecision)?;
        if pending.authority != session.authority
            || request.prompt_seq != pending.visible.prompt_seq
        {
            return Err(Error::StaleDecision);
        }
        let index = *pending
            .engine_indexes
            .get(request.action_index as usize)
            .ok_or(Error::InvalidAction)?;
        session
            .inner
            .akroma_validate_bound_decision_v40(pending.expected, self.human_seat, &pending.binding)
            .map_err(|_| Error::StaleDecision)?;
        if session
            .public_observation(pending.expected)
            .map_err(|_| Error::StaleDecision)?
            != pending.public
        {
            return Err(Error::StaleDecision);
        }
        let result = session
            .inner
            .step(pending.expected.episode_id, pending.expected.step, index);
        self.pending = None;
        result.map_err(|_| Error::ExecutionFailed)?;
        let receipt = HumanActionReceiptV1 {
            prompt_seq: request.prompt_seq,
            action_index: request.action_index,
        };
        self.accepted = Some((receipt.clone(), session.authority.clone()));
        Ok(receipt)
    }
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "akroma_v40_history_tests.rs"]
mod history_tests;
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "akroma_v40_mixed_tests.rs"]
mod mixed_tests;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::{put, ready_state_for};
    use crate::state::Zone;
    fn ready(actor: PlayerId) -> FastActorSessionV1 {
        let mut state = ready_state_for(actor);
        put(&mut state, actor, "Forest", Zone::Hand);
        put(&mut state, actor, "Mountain", Zone::Hand);
        put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
        FastActorSessionV1::from_akroma_v40_fixture_state(state)
    }
    #[test]
    fn registered_owning_pending_and_accepted_requests_cannot_revive_after_restore_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut session =
                HumanAkromaSessionV40::from_session(ready(actor), actor.into()).unwrap();
            let snapshot = session.snapshot_v40();
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("actual priority");
            };
            let mut projector = HumanAkromaProjectorV40::new(actor.into());
            let menu = projector.project_current(&session, expected).unwrap();
            let action = menu
                .actions
                .iter()
                .find(|row| row.label.contains("Play") && row.label.contains("Forest"))
                .unwrap();
            let request = HumanActionRequestV1 {
                prompt_seq: menu.prompt_seq,
                action_index: action.action_index,
            };
            let receipt = projector.submit(&mut session, request.clone()).unwrap();
            assert_eq!(
                projector.submit(&mut session, request.clone()).unwrap(),
                receipt
            );
            session.restore_v40(&snapshot).unwrap();
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, request),
                Err(Error::StaleDecision)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("restored priority");
            };
            let menu = projector.project_current(&session, expected).unwrap();
            let request = HumanActionRequestV1 {
                prompt_seq: menu.prompt_seq,
                action_index: 0,
            };
            session.restore_v40(&snapshot).unwrap();
            assert_eq!(
                projector.submit(&mut session, request),
                Err(Error::StaleDecision)
            );
            let foreign = HumanAkromaSessionV40::from_session(ready(actor), actor.into())
                .unwrap()
                .snapshot_v40();
            assert_eq!(session.restore_v40(&foreign), Err(Error::StaleDecision));
            assert!(session
                .public_observation(FastActorDecisionV1 {
                    acting_player: actor.opponent().into(),
                    ..expected
                })
                .is_err());
        }
    }
    #[test]
    fn registered_absence_transport_retains_predecessor_fields_and_opponent_hand_privacy() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let core = ready(actor);
            let FastActorResponseV1::Decision(expected) = core.current_response() else {
                panic!("actual priority");
            };
            let state = core.akroma_fixture_state();
            let old = crate::rl::observe_policy_v8(
                state,
                &crate::policy_surface_v5::PolicySurfaceV5::new_for_session(),
                actor,
                0,
                0,
                0,
                1,
            )
            .unwrap();
            let session = HumanAkromaSessionV40::from_session(core, actor.into()).unwrap();
            let new = session.public_observation(expected).unwrap();
            assert!(new.grants.is_empty());
            assert!(new.color_departures.is_empty());
            assert_eq!(new.own_hand, old.own_hand);
            assert_eq!(new.known_hand_cards, old.known_hand_cards);
            let mut projector = HumanAkromaProjectorV40::new(actor.into());
            let menu = projector.project_current(&session, expected).unwrap();
            let json = serde_json::to_string(&menu).unwrap();
            for private in [
                "arena_id",
                "private_snapshot_json",
                "source_contract",
                "event_history",
                "card_def",
            ] {
                assert!(!json.contains(private));
            }
            assert!(!json.contains("Lightning Bolt"));
        }
    }

    #[test]
    fn registered_actual_ward_prompt_preserves_predecessor_actions_and_all_private_fields() {
        let (mut state, _, _) = crate::policy_observation_v6::tests::ward_multi_targeter_state();
        crate::policy_observation_v6::tests::reach_ward_payment(&mut state);
        let base = FastActorSessionV1::from_human_v8_fixture_state(state);
        let FastActorResponseV1::Decision(expected) = base.current_response() else {
            panic!("genuine Ward payment");
        };
        let actor = expected.acting_player;
        let old = HumanRaidSessionV9::from_session(base.clone()).unwrap();
        let old = HumanDecisionProjectorV9::new(actor)
            .project_current(&old, expected)
            .unwrap();
        let session = HumanAkromaSessionV40::from_session(base, actor).unwrap();
        let new = HumanAkromaProjectorV40::new(actor)
            .project_current(&session, expected)
            .unwrap();
        assert_eq!(new.actions, old.actions);
        assert_eq!(new.state.public, old.state.public);
        assert_eq!(new.state.extensions.predecessor, old.state.extensions);
        assert_eq!(
            new.state.known_face_down_cards,
            old.state.known_face_down_cards
        );
        assert_eq!(new.state.poison_counters, old.state.poison_counters);
        assert_eq!(new.state.poison_prevention, old.state.poison_prevention);
        assert_eq!(
            new.state.creatures_attacked_this_turn,
            old.state.creatures_attacked_this_turn
        );
        assert_eq!(new.state.ninja_emblems, old.state.ninja_emblems);
        assert_eq!(new.state.restricted_mana, old.state.restricted_mana);
    }
}
