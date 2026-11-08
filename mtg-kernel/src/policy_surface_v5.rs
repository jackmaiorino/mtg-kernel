//! Policy-only v5 decision surface.
//!
//! The rules engine and [`HarnessSurfaceV2`] intentionally keep their
//! aggregate combat declarations.  This wrapper presents those declarations
//! to a policy as a canonical binary scan and commits the aggregate action
//! exactly once, after the final Boolean answer.

use crate::engine::{self, Action, Decision};
use crate::ids::{ObjectId, PlayerId};
use crate::rl_session::FastActorCurrentCandidateProofV1;
use crate::state::GameState;
use crate::surface_v2::{
    HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SurfaceAction, SurfaceDecision,
};
use serde::{Deserialize, Serialize};
mod hunter_order_v45;
pub use hunter_order_v45::TriggerOrderContextV45;

pub const POLICY_SURFACE_VERSION: u32 = 5;
pub const POLICY_ENVIRONMENT_HASH_ALGORITHM: &str =
    "fnv1a64-serde-json-policy-environment-envelope-v1";
/// Sibling identity for sessions on environment-v2 randomness states. The
/// legacy constant above is frozen and unchanged.
pub const POLICY_ENVIRONMENT_HASH_ALGORITHM_ENVIRONMENT_V2: &str =
    "fnv1a64-serde-json-policy-environment-envelope-v2";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicySurfaceStageV5 {
    Surface,
    AttackerInclusion,
    BlockerInclusion,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PrivateCombatSelectionIdsV5 {
    pub attacker: Option<ObjectId>,
    pub candidate_index: u32,
    pub candidate_count: u32,
    pub selected: Vec<ObjectId>,
    pub current_candidate: ObjectId,
    pub remaining_after_current: Vec<ObjectId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PolicySurfaceContextIdsV5 {
    pub current_stage: PolicySurfaceStageV5,
    pub private_combat_selection: Option<PrivateCombatSelectionIdsV5>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PolicyDecisionV5 {
    Surface(SurfaceDecision),
    AttackerInclusion {
        player: PlayerId,
        attacker: ObjectId,
        candidate_index: u32,
        candidate_count: u32,
    },
    BlockerInclusion {
        player: PlayerId,
        attacker: ObjectId,
        blocker: ObjectId,
        candidate_index: u32,
        candidate_count: u32,
    },
}

impl PolicyDecisionV5 {
    pub fn actor(&self, state: &GameState) -> Option<PlayerId> {
        match self {
            PolicyDecisionV5::Surface(decision) => match decision {
                SurfaceDecision::Decision(decision) => match decision {
                    Decision::CastSpellOrPass { player, .. }
                    | Decision::ChooseTargets { player, .. }
                    | Decision::ChooseCostTargets { player, .. }
                    | Decision::ChooseCastMode { player, .. }
                    | Decision::ChooseKicker { player, .. }
                    | Decision::ChooseSpellMode { player, .. }
                    | Decision::ChooseTriggerMode { player, .. }
                    | Decision::ChooseEffectOption { player, .. }
                    | Decision::ChooseFullRangeCastXValueV59 { player, .. }
                    | Decision::ChooseEffectTargets { player, .. }
                    | Decision::ChooseEffectBoolean { player, .. }
                    | Decision::ChooseCombatDamageRange { player, .. }
                    | Decision::ChooseLegendPermanent { player, .. }
                    | Decision::ChooseLondonMulligan { player, .. }
                    | Decision::ChooseLondonBottom { player, .. }
                    | Decision::ChooseAttackTarget { player, .. }
                    | Decision::ChooseVariableCastTargets { player, .. }
                    | Decision::ChooseVariableCastKicker { player, .. }
                    | Decision::ChooseEntryColorV36 { player, .. }
                    | Decision::ChooseEntryCreatureTypeV1 { player, .. }
                    | Decision::ChooseOptionalCost { player, .. }
                    | Decision::ChooseSpellCopyPayment { player, .. }
                    | Decision::ChooseSpellCopyRetarget { player, .. }
                    | Decision::ChooseMadnessCast { player, .. }
                    | Decision::Discard { player, .. }
                    | Decision::DeclareAttackers { player, .. }
                    | Decision::DeclareBlockers { player, .. }
                    | Decision::OrderTriggers { player, .. } => Some(*player),
                    Decision::GameOver { .. } | Decision::Halted { .. } => None,
                },
                SurfaceDecision::DeclareBlockersForAttacker { attacker, .. } => {
                    Some(state.objects.get(*attacker).controller.opponent())
                }
            },
            PolicyDecisionV5::AttackerInclusion { player, .. }
            | PolicyDecisionV5::BlockerInclusion { player, .. } => Some(*player),
        }
    }

    pub fn substep(&self) -> (u32, u32) {
        match self {
            PolicyDecisionV5::Surface(_) => (0, 1),
            PolicyDecisionV5::AttackerInclusion {
                candidate_index,
                candidate_count,
                ..
            }
            | PolicyDecisionV5::BlockerInclusion {
                candidate_index,
                candidate_count,
                ..
            } => (*candidate_index, *candidate_count),
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(
                Decision::GameOver { .. } | Decision::Halted { .. }
            ))
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PolicyActionV5 {
    Surface(SurfaceAction),
    ChooseAttackerInclusion {
        actor: PlayerId,
        attacker: ObjectId,
        include: bool,
    },
    ChooseBlockerInclusion {
        actor: PlayerId,
        attacker: ObjectId,
        blocker: ObjectId,
        include: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "scan_kind", rename_all = "snake_case")]
enum CombatScanV5 {
    Attackers {
        player: PlayerId,
        ordered_candidates: Vec<ObjectId>,
        cursor: usize,
        selected: Vec<ObjectId>,
        binding: CombatScanBindingV5,
    },
    Blockers {
        player: PlayerId,
        attacker: ObjectId,
        ordered_candidates: Vec<ObjectId>,
        cursor: usize,
        selected: Vec<ObjectId>,
        binding: CombatScanBindingV5,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "binding_kind", content = "binding", rename_all = "snake_case")]
enum CombatScanBindingV5 {
    /// Standalone/public `PolicySurfaceV5` callers may retain and mutate their
    /// `GameState`, so their combat scan stays bound to the exact historical
    /// state-and-H2-surface hash.
    ExactSurfaceHash(u64),
    /// `RlEpisodeSessionV1` privately owns both the state and surface. Its
    /// crate-private path can therefore use a checked opaque revision without
    /// weakening the standalone boundary.
    OwnedEnvironmentRevision(u64),
}

#[derive(Debug, Clone, Copy)]
enum EnvironmentBindingModeV5 {
    Exact,
    OwnedRevision(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FastActorInPlaceApplyErrorV1 {
    RejectedBeforeMutation,
    InternalApplyFailure,
}

impl CombatScanV5 {
    fn validate_binding(
        &self,
        state: &GameState,
        surface: &HarnessSurfaceV2,
        mode: EnvironmentBindingModeV5,
    ) -> Result<(), String> {
        let binding = match self {
            CombatScanV5::Attackers { binding, .. } | CombatScanV5::Blockers { binding, .. } => {
                *binding
            }
        };
        let valid = match (binding, mode) {
            (CombatScanBindingV5::ExactSurfaceHash(expected), EnvironmentBindingModeV5::Exact) => {
                surface_binding_hash(state, surface)? == expected
            }
            (
                CombatScanBindingV5::OwnedEnvironmentRevision(expected),
                EnvironmentBindingModeV5::OwnedRevision(actual),
            ) => actual == expected,
            _ => false,
        };
        if valid {
            Ok(())
        } else {
            Err("stale policy combat scan environment binding".to_string())
        }
    }

    fn rebind_owned_revision(&mut self, revision: u64) -> Result<(), String> {
        let binding = match self {
            CombatScanV5::Attackers { binding, .. } | CombatScanV5::Blockers { binding, .. } => {
                binding
            }
        };
        match binding {
            CombatScanBindingV5::OwnedEnvironmentRevision(bound) => {
                *bound = revision;
                Ok(())
            }
            CombatScanBindingV5::ExactSurfaceHash(_) => {
                Err("cannot rebind an exact policy combat scan as owned".to_string())
            }
        }
    }

    fn context_for(&self, observer: PlayerId) -> Result<PolicySurfaceContextIdsV5, String> {
        self.validate_shape()?;
        let (stage, player, attacker, candidates, cursor, selected) = match self {
            CombatScanV5::Attackers {
                player,
                ordered_candidates,
                cursor,
                selected,
                ..
            } => (
                PolicySurfaceStageV5::AttackerInclusion,
                *player,
                None,
                ordered_candidates,
                *cursor,
                selected,
            ),
            CombatScanV5::Blockers {
                player,
                attacker,
                ordered_candidates,
                cursor,
                selected,
                ..
            } => (
                PolicySurfaceStageV5::BlockerInclusion,
                *player,
                Some(*attacker),
                ordered_candidates,
                *cursor,
                selected,
            ),
        };
        let private_combat_selection = if observer == player {
            Some(PrivateCombatSelectionIdsV5 {
                attacker,
                candidate_index: u32::try_from(cursor)
                    .map_err(|_| "combat scan candidate index exceeds u32".to_string())?,
                candidate_count: u32::try_from(candidates.len())
                    .map_err(|_| "combat scan candidate count exceeds u32".to_string())?,
                selected: selected.clone(),
                current_candidate: candidates[cursor],
                remaining_after_current: candidates[(cursor + 1)..].to_vec(),
            })
        } else {
            None
        };
        Ok(PolicySurfaceContextIdsV5 {
            current_stage: stage,
            private_combat_selection,
        })
    }

    fn validate_shape(&self) -> Result<(), String> {
        let (candidates, cursor, selected) = match self {
            CombatScanV5::Attackers {
                ordered_candidates,
                cursor,
                selected,
                ..
            }
            | CombatScanV5::Blockers {
                ordered_candidates,
                cursor,
                selected,
                ..
            } => (ordered_candidates, *cursor, selected),
        };
        if candidates.is_empty() || cursor >= candidates.len() {
            return Err("invalid empty or exhausted policy combat scan".to_string());
        }
        if selected.len() > cursor {
            return Err("invalid policy combat scan selected prefix length".to_string());
        }
        if selected.iter().any(|id| !candidates[..cursor].contains(id)) {
            return Err("invalid policy combat scan selection outside answered prefix".to_string());
        }
        Ok(())
    }

    /// The one scan-answer legality rule: `Ok` iff answering `include` for
    /// the current candidate keeps at least one completion that the engine's
    /// own aggregate validator accepts. Otherwise the engine's refusal of the
    /// largest completion, which names the requirement that can no longer be
    /// met.
    ///
    /// Probing the smallest and the largest completion is exact for the
    /// current requirements. Attacker legality is monotone in the declared
    /// set (only omitting a goaded attacker is refused), so the largest
    /// completion is legal whenever any is. A blocker declaration is legal
    /// when an attacker's count is zero or reaches its minimum, and every
    /// count between the two probes is reachable. A future requirement of
    /// another shape needs `scan_accepts_exactly_the_answers_that_keep_a_
    /// legal_completion` extended with it.
    fn answer_keeps_a_legal_completion(
        &self,
        state: &GameState,
        surface: &HarnessSurfaceV2,
        include: bool,
    ) -> Result<(), String> {
        let (candidates, cursor, selected) = match self {
            CombatScanV5::Attackers {
                ordered_candidates,
                cursor,
                selected,
                ..
            }
            | CombatScanV5::Blockers {
                ordered_candidates,
                cursor,
                selected,
                ..
            } => (ordered_candidates, *cursor, selected),
        };
        let validate = |declared: &[ObjectId]| match self {
            CombatScanV5::Attackers { .. } => engine::validate_declare_attackers(state, declared),
            CombatScanV5::Blockers { attacker, .. } => {
                surface.validate_declare_blockers_for_attacker(state, *attacker, declared)
            }
        };
        let mut smallest = selected.clone();
        if include {
            smallest.push(candidates[cursor]);
        }
        if validate(&smallest).is_ok() {
            return Ok(());
        }
        let mut largest = smallest;
        largest.extend_from_slice(&candidates[(cursor + 1)..]);
        validate(&largest)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LimitedAppendixV1 {
    #[default]
    Legacy,
    Artist,
    Niv,
    ChosenType,
    Drakuseth,
    Embercleave,
    Zimone,
    Omniscience,
    Tutelage,
    Moon,
}

#[derive(Clone)]
pub struct PolicySurfaceV5 {
    inner: HarnessSurfaceV2,
    scan: Option<CombatScanV5>,
    hunter_order_enabled_v45: bool,
    hunter_order_v45: Option<hunter_order_v45::TriggerOrderV45>,
    limited_appendix_v1: LimitedAppendixV1,
    /// W1 producer (lead ruling 7 (a)): where each presented decision fell
    /// in the committed stream, as (seat, event-history length, observation
    /// journal length), oldest first. A seat's marks bound the batches its
    /// bot-facing transcript receives. Bookkeeping only: no transition,
    /// observation or hash reads it.
    infoset_marks_v1: std::sync::Arc<Vec<(u8, u32, u32)>>,
}

/// The session surface's public context, carried by the W1 information-set
/// payload so an imported session flows like the live one. Id-free except
/// `combat`, whose objects the producer writes as canonical arena indices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceContextV1 {
    pub scalars: crate::surface_v2::HarnessSurfaceScalarsV1,
    pub hunter_order_enabled_v45: bool,
    /// A live combat declaration's progress (amendment 43.1); absent at
    /// every other decision, so those payloads keep their bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat: Option<CombatContextV1>,
    /// A live discard reshape's progress (amendment 43.1 part 2); absent at
    /// every other decision, so those payloads keep their bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discard: Option<DiscardContextV1>,
    /// A live optional-cost reshape (amendment 43.9): the paying player, the
    /// payable sub-costs and the stage. Id-free; absent at every other
    /// decision, so those payloads keep their bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optional_cost: Option<crate::surface_v2::OptionalCostReshapePublicV2>,
    /// The surface's one-shot madness reprompt exemption (amendment 43.9):
    /// the card being cast for its madness cost, in the producer's native
    /// id until it is renamed to a canonical arena index. Absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub madness_reprompt: Option<ObjectId>,
}

/// Amendment 43.9: the import's fail-closed check of a carried
/// optional-cost record against the imported state: the engine's pending
/// optional cost is the viewer's, with the same payable sub-costs, and the
/// `Which` stage is reached only when both are payable.
pub(crate) fn validate_optional_cost_in_state_v1(
    o: &crate::surface_v2::OptionalCostReshapePublicV2,
    state: &GameState,
    viewer: PlayerId,
) -> Result<(), String> {
    let Some(pending) = state.engine.pending_optional_cost.as_ref() else {
        return Err("optional-cost context: no pending optional cost".into());
    };
    if o.player != viewer || pending.player != viewer {
        return Err("optional-cost context: player".into());
    }
    if (o.discard_payable, o.sacrifice_payable)
        != (pending.discard_payable, pending.sacrifice_payable)
        || !(o.discard_payable || o.sacrifice_payable)
    {
        return Err("optional-cost context: payable sub-costs".into());
    }
    if o.stage == crate::surface_v2::OptionalCostStagePublicV2::Which
        && !(o.discard_payable && o.sacrifice_payable)
    {
        return Err("optional-cost context: stage".into());
    }
    Ok(())
}

/// Amendment 43.9: the import's fail-closed check of a carried madness
/// exemption: a tracked (`C:`) card cast by its madness route (its current
/// incarnation's cast origin) that is the spell being announced. The
/// exemption is armed when that announcement begins and travels only while
/// it lasts (Sol design review 2 on cut 2: a card already on the stack
/// cannot show that its exemption is unconsumed); after it, the producer
/// still refuses R2 `madness_reprompt`.
pub(crate) fn validate_madness_reprompt_in_state_v1(
    card: ObjectId,
    state: &GameState,
    handles: &[String],
) -> Result<(), String> {
    let tracked = handles
        .get(card.0 as usize)
        .is_some_and(|h| h.starts_with("C:"));
    if card.0 as usize >= state.objects.len() {
        return Err("madness context: card".into());
    }
    let object = state.objects.get(card);
    let madness = object
        .v4
        .spell_cast_origin
        .as_ref()
        .is_some_and(|origin| matches!(origin.route, crate::state::SpellCastRouteV4::Madness));
    let cast = state
        .engine
        .pending_cast
        .as_ref()
        .is_some_and(|p| p.spell == card);
    if !tracked || !madness || !cast {
        return Err("madness context: card".into());
    }
    Ok(())
}

/// A live discard reshape (amendment 43.1 part 2): the discarding player,
/// the cards picked so far in pick order, and how many are still needed.
/// The remaining choices are not carried: the import recomputes them from
/// the imported state's pending discard, so no hand order travels. All of
/// it is V's transcript (V's own picks at V's own discard).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscardContextV1 {
    pub player: PlayerId,
    pub chosen: Vec<ObjectId>,
    pub remaining_needed: u32,
}

impl DiscardContextV1 {
    /// Every picked card renamed through `f`; None if any has no image.
    pub(crate) fn rename(&self, f: impl Fn(ObjectId) -> Option<ObjectId>) -> Option<Self> {
        Some(Self {
            player: self.player,
            chosen: self
                .chosen
                .iter()
                .map(|x| f(*x))
                .collect::<Option<Vec<_>>>()?,
            remaining_needed: self.remaining_needed,
        })
    }

    /// The import's fail-closed check against the imported state; returns
    /// the remaining choices (the pending discard's choices not yet picked,
    /// in the imported hand's order). The state must hold the pending
    /// discard of `player`, who is the payload's viewer; the picks are
    /// distinct tracked (`C:`) choices of it; picks plus the count still
    /// needed equal the decision's count; at least one card is still
    /// needed and the remaining choices cover it.
    pub(crate) fn validate_in_state(
        &self,
        state: &GameState,
        handles: &[String],
        viewer: PlayerId,
    ) -> Result<Vec<ObjectId>, String> {
        let Some((player, count, choices)) = engine::pending_discard_decision_v1(state) else {
            return Err("discard context: no pending discard".into());
        };
        if self.player != player || self.player != viewer {
            return Err("discard context: player".into());
        }
        let tracked = |o: &ObjectId| {
            handles
                .get(o.0 as usize)
                .is_some_and(|h| h.starts_with("C:"))
        };
        let mut distinct = self.chosen.clone();
        distinct.sort_unstable();
        distinct.dedup();
        if distinct.len() != self.chosen.len()
            || !self
                .chosen
                .iter()
                .all(|x| tracked(x) && choices.contains(x))
        {
            return Err("discard context: picks".into());
        }
        let remaining: Vec<ObjectId> = choices
            .iter()
            .copied()
            .filter(|x| !self.chosen.contains(x))
            .collect();
        if self.remaining_needed == 0
            || self.chosen.len() as u64 + u64::from(self.remaining_needed) != u64::from(count)
            || (remaining.len() as u64) < u64::from(self.remaining_needed)
        {
            return Err("discard context: progress".into());
        }
        Ok(remaining)
    }
}

/// A live combat inclusion scan with the blocker reshape around it
/// (amendment 43.1): the candidates in the scan's own order, the cursor,
/// the answered inclusions, and for blocks the per-attacker reshape. All of
/// it is V's transcript: the order is the order of V's inclusion points,
/// and the progress is V's own answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatContextV1 {
    pub kind: CombatScanKindV1,
    pub player: PlayerId,
    /// The attacker being blocked (blocker scans only).
    pub attacker: Option<ObjectId>,
    pub candidates: Vec<ObjectId>,
    pub cursor: usize,
    pub selected: Vec<ObjectId>,
    /// The surface's blocker reshape (blocker scans only).
    pub blockers: Option<crate::surface_v2::BlockersReshapePublicV2>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatScanKindV1 {
    Attackers,
    Blockers,
}

impl CombatContextV1 {
    /// Every object renamed through `f`; None if any has no image.
    pub(crate) fn rename(&self, f: impl Fn(ObjectId) -> Option<ObjectId>) -> Option<Self> {
        let all = |xs: &[ObjectId]| xs.iter().map(|x| f(*x)).collect::<Option<Vec<_>>>();
        let blockers = match &self.blockers {
            None => None,
            Some(b) => Some(crate::surface_v2::BlockersReshapePublicV2 {
                current_attacker: match b.current_attacker {
                    None => None,
                    Some(a) => Some(f(a)?),
                },
                accumulated: b
                    .accumulated
                    .iter()
                    .map(|(x, y)| Some((f(*x)?, f(*y)?)))
                    .collect::<Option<Vec<_>>>()?,
                remaining: b
                    .remaining
                    .iter()
                    .map(|(a, xs)| Some((f(*a)?, all(xs)?)))
                    .collect::<Option<Vec<_>>>()?,
            }),
        };
        Some(Self {
            kind: self.kind,
            player: self.player,
            attacker: match self.attacker {
                None => None,
                Some(a) => Some(f(a)?),
            },
            candidates: all(&self.candidates)?,
            cursor: self.cursor,
            selected: all(&self.selected)?,
            blockers,
        })
    }

    /// The import's fail-closed check: objects inside the arena, a presented
    /// (unexhausted) scan whose selection is a subset of its answered
    /// prefix, and a blocker scan inside a reshape whose current attacker
    /// it names.
    pub(crate) fn validate(&self, arena_len: usize) -> Result<(), String> {
        let inside = |x: &ObjectId| (x.0 as usize) < arena_len;
        if !self.objects().iter().all(inside) {
            return Err("combat context names an object outside the arena".into());
        }
        if self.player.0 > 1 {
            return Err("combat context player".into());
        }
        let mut distinct = self.candidates.clone();
        distinct.sort_unstable();
        distinct.dedup();
        if distinct.len() != self.candidates.len() {
            return Err("combat context repeats a candidate".into());
        }
        if self.candidates.is_empty()
            || self.cursor >= self.candidates.len()
            || self.selected.len() > self.cursor
        {
            return Err("combat context progress".into());
        }
        // The selection is the included candidates of the answered prefix
        // in scan order: a distinct, ordered subsequence of it.
        let mut from = 0;
        for x in &self.selected {
            match self.candidates[from..self.cursor]
                .iter()
                .position(|c| c == x)
            {
                Some(k) => from += k + 1,
                None => return Err("combat context progress".into()),
            }
        }
        match (self.kind, self.attacker, &self.blockers) {
            (CombatScanKindV1::Attackers, None, None) => Ok(()),
            (CombatScanKindV1::Blockers, Some(a), Some(b)) if b.current_attacker == Some(a) => {
                Ok(())
            }
            _ => Err("combat context kind".into()),
        }
    }

    /// Every object the record names.
    fn objects(&self) -> Vec<ObjectId> {
        let mut ids: Vec<ObjectId> = self
            .candidates
            .iter()
            .chain(&self.selected)
            .copied()
            .collect();
        ids.extend(self.attacker);
        if let Some(b) = &self.blockers {
            ids.extend(b.current_attacker);
            ids.extend(b.accumulated.iter().flat_map(|(x, y)| [*x, *y]));
            ids.extend(
                b.remaining
                    .iter()
                    .flat_map(|(a, xs)| std::iter::once(*a).chain(xs.iter().copied())),
            );
        }
        ids
    }

    /// The import's check against the imported state (amendment 43.1):
    /// `validate`, every object a tracked (`C:`) object, and the record a
    /// declaration the surface itself would be presenting there: the step
    /// and the scanning player, the attacker scan's candidates exactly the
    /// engine's eligible attackers, and for blocks the current candidates
    /// exactly the attacker's legal blockers not yet assigned, every queued
    /// attacker's blockers exactly its legal blockers, and every assignment
    /// a legal block by a distinct blocker.
    pub(crate) fn validate_in_state(
        &self,
        state: &GameState,
        handles: &[String],
    ) -> Result<(), String> {
        use std::collections::BTreeSet;
        self.validate(state.objects.len())?;
        let tracked = |o: &ObjectId| {
            handles
                .get(o.0 as usize)
                .is_some_and(|h| h.starts_with("C:"))
        };
        if !self.objects().iter().all(tracked) {
            return Err("combat context names an untracked object".into());
        }
        let set = |xs: &[ObjectId]| xs.iter().copied().collect::<BTreeSet<_>>();
        let combat = &state.engine.combat;
        match self.kind {
            CombatScanKindV1::Attackers => {
                if state.step != crate::state::Step::DeclareAttackers
                    || combat.attackers_declared
                    || self.player != state.active_player
                {
                    return Err("combat context: no attacker declaration".into());
                }
                if set(&self.candidates) != set(&engine::eligible_attackers(state)) {
                    return Err("combat context: candidates are not the eligible attackers".into());
                }
            }
            CombatScanKindV1::Blockers => {
                let (Some(attacker), Some(b)) = (self.attacker, &self.blockers) else {
                    return Err("combat context kind".into());
                };
                if state.step != crate::state::Step::DeclareBlockers
                    || !combat.attackers_declared
                    || combat.blockers_declared
                    || self.player == state.active_player
                {
                    return Err("combat context: no blocker declaration".into());
                }
                // An attacker that left the battlefield left combat (the
                // engine's own declaration filter).
                let attacking = |a: &ObjectId| {
                    combat.attackers.contains(a)
                        && state.objects.get(*a).zone == crate::state::Zone::Battlefield
                };
                let legal = |a: ObjectId| set(&engine::legal_blockers_for(state, a));
                let used: Vec<ObjectId> = b.accumulated.iter().map(|(x, _)| *x).collect();
                if set(&used).len() != used.len()
                    || !b
                        .accumulated
                        .iter()
                        .all(|(x, a)| attacking(a) && legal(*a).contains(x))
                {
                    return Err("combat context: assigned blocks".into());
                }
                let mut current = legal(attacker);
                current.retain(|x| !used.contains(x));
                if !attacking(&attacker) || set(&self.candidates) != current {
                    return Err("combat context: candidates are not the legal blockers".into());
                }
                let queued: Vec<ObjectId> = b.remaining.iter().map(|(a, _)| *a).collect();
                if set(&queued).len() != queued.len()
                    || queued.contains(&attacker)
                    || !b.remaining.iter().all(|(a, xs)| {
                        attacking(a)
                            && !xs.is_empty()
                            && set(xs).len() == xs.len()
                            && set(xs) == legal(*a)
                    })
                {
                    return Err("combat context: queued attackers".into());
                }
            }
        }
        Ok(())
    }
}

impl PolicySurfaceV5 {
    /// Records that a decision of `seat` is presented with `events` committed
    /// events and `notes` journal notes (lead ruling 7 (a)).
    pub(crate) fn push_infoset_mark_v1(&mut self, seat: PlayerId, events: usize, notes: usize) {
        let mark = (seat.0, events as u32, notes as u32);
        if self.infoset_marks_v1.last() != Some(&mark) {
            std::sync::Arc::make_mut(&mut self.infoset_marks_v1).push(mark);
        }
    }

    /// The (event-history length, journal length) of every decision
    /// presented to `seat`, oldest first.
    pub(crate) fn infoset_marks_v1(&self, seat: PlayerId) -> Vec<(usize, usize)> {
        self.infoset_marks_v1
            .iter()
            .filter(|m| m.0 == seat.0)
            .map(|m| (m.1 as usize, m.2 as usize))
            .collect()
    }

    /// Carries another surface's marks (a session restarted on the same
    /// committed stream).
    pub(crate) fn carry_infoset_marks_v1(&mut self, from: &PolicySurfaceV5) {
        self.infoset_marks_v1 = from.infoset_marks_v1.clone();
    }

    /// None while the surface holds object-naming progress (a combat scan,
    /// a blocker reshape, a Hunter trigger order).
    // Amendment 43.9: the payload path reads the records directly; tests
    // and fixture imports still use the plain context.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn infoset_context_v1(&self) -> Option<SurfaceContextV1> {
        if self.scan.is_some() || self.hunter_order_v45.is_some() {
            return None;
        }
        Some(SurfaceContextV1 {
            scalars: self.inner.infoset_context_v1()?,
            hunter_order_enabled_v45: self.hunter_order_enabled_v45,
            combat: None,
            discard: None,
            optional_cost: None,
            madness_reprompt: None,
        })
    }

    /// As `infoset_context_v1`, and during a combat scan the context with
    /// its combat record in native object ids (the producer renames them).
    /// None while a Hunter trigger order or another reshape names objects.
    pub(crate) fn infoset_context_with_combat_v1(&self) -> Option<SurfaceContextV1> {
        let public = self.inner.public_context();
        // A live discard reshape (amendment 43.1 part 2) travels as the
        // discard record, in native ids (the producer renames them).
        let discard = public.discard.as_ref().map(|d| DiscardContextV1 {
            player: d.player,
            chosen: d.chosen.clone(),
            remaining_needed: d.remaining_needed,
        });
        // Amendment 43.9: a live optional-cost reshape and the madness
        // exemption travel as their own records (the madness card in native
        // ids, renamed by the producer).
        let optional_cost = public.optional_cost.clone();
        let madness_reprompt = public.madness_cast_reprompt_exemption;
        let Some(scan) = &self.scan else {
            if self.hunter_order_v45.is_some() || public.blockers.is_some() {
                return None;
            }
            return Some(SurfaceContextV1 {
                scalars: self.inner.infoset_scalars_with_reshapes_v1()?,
                hunter_order_enabled_v45: self.hunter_order_enabled_v45,
                combat: None,
                discard,
                optional_cost,
                madness_reprompt,
            });
        };
        if self.hunter_order_v45.is_some()
            || discard.is_some()
            || optional_cost.is_some()
            || madness_reprompt.is_some()
        {
            return None;
        }
        let blockers = public.blockers;
        let combat = match scan {
            CombatScanV5::Attackers {
                player,
                ordered_candidates,
                cursor,
                selected,
                ..
            } => CombatContextV1 {
                kind: CombatScanKindV1::Attackers,
                player: *player,
                attacker: None,
                candidates: ordered_candidates.clone(),
                cursor: *cursor,
                selected: selected.clone(),
                blockers,
            },
            CombatScanV5::Blockers {
                player,
                attacker,
                ordered_candidates,
                cursor,
                selected,
                ..
            } => CombatContextV1 {
                kind: CombatScanKindV1::Blockers,
                player: *player,
                attacker: Some(*attacker),
                candidates: ordered_candidates.clone(),
                cursor: *cursor,
                selected: selected.clone(),
                blockers,
            },
        };
        Some(SurfaceContextV1 {
            scalars: self.inner.infoset_scalars_with_reshapes_v1()?,
            hunter_order_enabled_v45: self.hunter_order_enabled_v45,
            combat: Some(combat),
            discard: None,
            optional_cost: None,
            madness_reprompt: None,
        })
    }

    /// Why a payload cannot carry this surface: the R2 subclass the bridge
    /// sends with the refusal (amendment 43.1 part 2). A fixed name of the
    /// surface state V's own decision presents.
    pub(crate) fn payload_refusal_detail_v1(&self, viewer: PlayerId) -> &'static str {
        if self.hunter_order_v45.is_some() {
            return "hunter_trigger_order";
        }
        let p = self.inner.public_context();
        if p.optional_cost.is_some() {
            "optional_cost_reshape"
        } else if p.madness_cast_reprompt_exemption.is_some() {
            "madness_reprompt"
        } else if p.discard.as_ref().is_some_and(|d| d.player != viewer) {
            "opponent_discard"
        } else if p.discard.is_some() {
            "discard_untracked"
        } else if self.scan.is_some() || p.blockers.is_some() {
            "combat_untracked"
        } else {
            "other_surface"
        }
    }

    /// Restores a carried optional-cost record (amendment 43.9).
    pub(crate) fn set_optional_cost_reshape_v1(
        &mut self,
        o: &crate::surface_v2::OptionalCostReshapePublicV2,
    ) {
        self.inner.set_optional_cost_reshape_v1(o);
    }

    /// Restores a carried madness reprompt exemption (amendment 43.9).
    pub(crate) fn set_madness_reprompt_v1(&mut self, card: ObjectId) {
        self.inner.set_madness_reprompt_v1(card);
    }

    /// Restores a carried discard record (amendment 43.1 part 2) with the
    /// remaining choices its import check computed from the imported state.
    pub(crate) fn set_discard_context_v1(
        &mut self,
        discard: &DiscardContextV1,
        remaining: Vec<ObjectId>,
    ) {
        self.inner.set_discard_reshape_v1(
            discard.player,
            discard.chosen.clone(),
            remaining,
            discard.remaining_needed,
        );
    }

    /// A restored combat scan (amendment 43.1) must still admit a legal
    /// declaration: including or excluding the current candidate keeps a
    /// completion the engine accepts (the scan's own answer rule).
    pub(crate) fn validate_restored_combat_scan_v1(&self, state: &GameState) -> Result<(), String> {
        let Some(scan) = &self.scan else {
            return Ok(());
        };
        scan.validate_shape()?;
        scan.answer_keeps_a_legal_completion(state, &self.inner, true)
            .or_else(|_| scan.answer_keeps_a_legal_completion(state, &self.inner, false))
            .map_err(|e| format!("combat context admits no legal declaration: {e}"))
    }

    /// Restores a carried (validated) combat record, amendment 43.1, before
    /// the imported session presents its first decision, bound to that
    /// session's environment revision.
    pub(crate) fn set_combat_context_v1(&mut self, combat: &CombatContextV1, revision: u64) {
        self.inner.set_blockers_reshape_v1(combat.blockers.as_ref());
        let binding = CombatScanBindingV5::OwnedEnvironmentRevision(revision);
        self.scan = Some(match (combat.kind, combat.attacker) {
            (CombatScanKindV1::Blockers, Some(attacker)) => CombatScanV5::Blockers {
                player: combat.player,
                attacker,
                ordered_candidates: combat.candidates.clone(),
                cursor: combat.cursor,
                selected: combat.selected.clone(),
                binding,
            },
            _ => CombatScanV5::Attackers {
                player: combat.player,
                ordered_candidates: combat.candidates.clone(),
                cursor: combat.cursor,
                selected: combat.selected.clone(),
                binding,
            },
        });
    }

    pub(crate) fn set_infoset_context_v1(&mut self, context: SurfaceContextV1) {
        self.inner.set_infoset_context_v1(context.scalars);
        self.hunter_order_enabled_v45 = context.hunter_order_enabled_v45;
    }

    pub fn new() -> Self {
        Self::new_with_suppression_audit_mode(SuppressionAuditMode::Full)
    }

    pub(crate) fn new_for_session() -> Self {
        Self::new_with_suppression_audit_mode(SuppressionAuditMode::Off)
    }

    /// Expose every engine priority window, retaining the binary combat scan.
    pub fn new_with_engine_priority_v1() -> Self {
        Self::new_with_priority_mode_v1(PriorityModeV1::EngineWindowsV1, SuppressionAuditMode::Full)
    }

    pub(crate) fn new_with_priority_mode_v1(
        mode: PriorityModeV1,
        audit: SuppressionAuditMode,
    ) -> Self {
        Self {
            inner: HarnessSurfaceV2::new_with_priority_mode_v1(mode, audit),
            scan: None,
            hunter_order_enabled_v45: false,
            hunter_order_v45: None,
            limited_appendix_v1: LimitedAppendixV1::Legacy,
            infoset_marks_v1: Default::default(),
        }
    }

    /// Diagnostic constructor for proving that suppression-audit retention is
    /// observationally inert. Public standalone callers retain the historical
    /// `Full` behavior; the privately owned production session selects `Off`.
    pub fn new_with_suppression_audit_mode(mode: SuppressionAuditMode) -> Self {
        Self {
            inner: HarnessSurfaceV2::new_with_suppression_audit_mode(mode),
            scan: None,
            hunter_order_enabled_v45: false,
            hunter_order_v45: None,
            limited_appendix_v1: LimitedAppendixV1::Legacy,
            infoset_marks_v1: Default::default(),
        }
    }

    pub(crate) fn set_limited_appendix_v1(&mut self, appendix: LimitedAppendixV1) {
        self.limited_appendix_v1 = appendix;
    }

    pub(crate) fn limited_appendix_v1(&self) -> LimitedAppendixV1 {
        self.limited_appendix_v1
    }

    pub fn harness_surface(&self) -> &HarnessSurfaceV2 {
        &self.inner
    }

    pub fn harness_public_context(&self) -> crate::surface_v2::HarnessSurfacePublicContextV2 {
        self.inner.public_context()
    }

    pub fn scan_active(&self) -> bool {
        self.scan.is_some()
    }

    #[cfg(test)]
    pub(crate) fn reset_harness_context_for_test(&mut self) {
        self.inner = HarnessSurfaceV2::new_with_priority_mode_v1(
            self.inner.priority_mode_v1(),
            self.inner.suppression_audit_mode(),
        );
    }

    pub fn discard_unanswered_scan(&mut self) -> Result<(), String> {
        if let Some(order) = &self.hunter_order_v45 {
            if !order.prefix.is_empty() {
                return Err("refusing to discard partially answered Hunter ordering".into());
            }
            self.hunter_order_v45 = None;
        }
        match &self.scan {
            None => Ok(()),
            Some(CombatScanV5::Attackers { cursor: 0, .. })
            | Some(CombatScanV5::Blockers { cursor: 0, .. }) => {
                self.scan = None;
                Ok(())
            }
            Some(_) => Err("refusing to discard a partially answered combat scan".to_string()),
        }
    }

    pub fn scan_context_for(
        &self,
        observer: PlayerId,
    ) -> Result<PolicySurfaceContextIdsV5, String> {
        match &self.scan {
            Some(scan) => scan.context_for(observer),
            None => Ok(PolicySurfaceContextIdsV5 {
                current_stage: PolicySurfaceStageV5::Surface,
                private_combat_selection: None,
            }),
        }
    }

    /// For the active scan step `decision`, whether `[include: false,
    /// include: true]` each keep a legal completion of the aggregate
    /// declaration. These are the answers `apply` accepts.
    pub fn feasible_scan_answers(
        &self,
        state: &GameState,
        decision: &PolicyDecisionV5,
    ) -> Result<[bool; 2], String> {
        let scan = self.scan.as_ref().ok_or("no active policy combat scan")?;
        if policy_decision_for_scan(scan)? != *decision {
            return Err("decision is not the active policy combat scan step".to_string());
        }
        Ok([false, true].map(|include| {
            scan.answer_keeps_a_legal_completion(state, &self.inner, include)
                .is_ok()
        }))
    }

    pub fn privileged_scan_context(&self) -> Result<PolicySurfaceContextIdsV5, String> {
        match &self.scan {
            Some(scan @ CombatScanV5::Attackers { player, .. })
            | Some(scan @ CombatScanV5::Blockers { player, .. }) => scan.context_for(*player),
            None => Ok(PolicySurfaceContextIdsV5 {
                current_stage: PolicySurfaceStageV5::Surface,
                private_combat_selection: None,
            }),
        }
    }

    pub(crate) fn scan_context_for_owned_revision_v1(
        &self,
        state: &GameState,
        observer: PlayerId,
        revision: u64,
    ) -> Result<PolicySurfaceContextIdsV5, String> {
        if let Some(scan) = &self.scan {
            scan.validate_binding(
                state,
                &self.inner,
                EnvironmentBindingModeV5::OwnedRevision(revision),
            )?;
        }
        self.scan_context_for(observer)
    }

    /// Rebind only the derived private scan after its original owning snapshot
    /// and current decision have already been authenticated by the caller.
    /// Standalone exact-hash scans and every old caller keep their old boundary.
    pub(crate) fn rebind_owned_revision_after_restore_v1(
        &mut self,
        state: &GameState,
        previous: u64,
        next: u64,
    ) -> Result<(), String> {
        if next <= previous {
            return Err("restored owner revision must advance".into());
        }
        if let Some(scan) = self.scan.as_ref() {
            scan.validate_binding(
                state,
                &self.inner,
                EnvironmentBindingModeV5::OwnedRevision(previous),
            )?;
            scan.validate_shape()?;
        }
        if let Some(order) = self.hunter_order_v45.as_ref() {
            self.validate_hunter_order_header_v45(
                state,
                previous,
                &PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::OrderTriggers {
                    player: order.player,
                    pending: order.pending.clone(),
                })),
                self.hunter_order_substeps_v45(state)?
                    .ok_or("Hunter ordering substeps absent")?,
            )?;
        }
        if let Some(scan) = self.scan.as_mut() {
            let (CombatScanV5::Attackers { binding, .. } | CombatScanV5::Blockers { binding, .. }) =
                scan;
            *binding = CombatScanBindingV5::OwnedEnvironmentRevision(next);
        }
        if let Some(order) = self.hunter_order_v45.as_mut() {
            order.binding = CombatScanBindingV5::OwnedEnvironmentRevision(next);
        }
        Ok(())
    }

    pub fn next_decision(&mut self, state: &mut GameState) -> Result<PolicyDecisionV5, String> {
        self.next_decision_with_binding(state, EnvironmentBindingModeV5::Exact)
    }

    pub(crate) fn next_decision_owned(
        &mut self,
        state: &mut GameState,
        environment_revision: u64,
    ) -> Result<PolicyDecisionV5, String> {
        self.next_decision_with_binding(
            state,
            EnvironmentBindingModeV5::OwnedRevision(environment_revision),
        )
    }

    fn next_decision_with_binding(
        &mut self,
        state: &mut GameState,
        binding_mode: EnvironmentBindingModeV5,
    ) -> Result<PolicyDecisionV5, String> {
        if let Some(order) = &self.hunter_order_v45 {
            let origin =
                PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::OrderTriggers {
                    player: order.player,
                    pending: order.pending.clone(),
                }));
            let EnvironmentBindingModeV5::OwnedRevision(revision) = binding_mode else {
                return Err("Hunter ordering requires its owning revision".into());
            };
            self.initialize_hunter_order_v45(state, &origin, revision)?;
            return Ok(origin);
        }
        if let Some(scan) = &self.scan {
            scan.validate_binding(state, &self.inner, binding_mode)?;
            return policy_decision_for_scan(scan);
        }

        let surfaced = self.inner.next_decision(state);
        match surfaced {
            SurfaceDecision::Decision(Decision::DeclareAttackers { player, eligible })
                if !eligible.is_empty() =>
            {
                self.scan = Some(CombatScanV5::Attackers {
                    player,
                    ordered_candidates: eligible,
                    cursor: 0,
                    selected: Vec::new(),
                    binding: scan_binding(state, &self.inner, binding_mode)?,
                });
                policy_decision_for_scan(self.scan.as_ref().expect("scan just initialized"))
            }
            SurfaceDecision::DeclareBlockersForAttacker {
                attacker,
                legal_blockers,
            } if !legal_blockers.is_empty() => {
                let player = state.objects.get(attacker).controller.opponent();
                self.scan = Some(CombatScanV5::Blockers {
                    player,
                    attacker,
                    ordered_candidates: legal_blockers,
                    cursor: 0,
                    selected: Vec::new(),
                    binding: scan_binding(state, &self.inner, binding_mode)?,
                });
                policy_decision_for_scan(self.scan.as_ref().expect("scan just initialized"))
            }
            other => {
                let origin = PolicyDecisionV5::Surface(other);
                if self.hunter_order_enabled_v45
                    && matches!(&origin, PolicyDecisionV5::Surface(SurfaceDecision::Decision(
                        Decision::OrderTriggers { pending, .. }
                    )) if pending.len() > crate::rl::MAX_TRIGGER_ORDER_OBJECTS)
                {
                    let EnvironmentBindingModeV5::OwnedRevision(revision) = binding_mode else {
                        return Err("Hunter ordering opt-in requires owning surface".into());
                    };
                    self.initialize_hunter_order_v45(state, &origin, revision)?;
                }
                Ok(origin)
            }
        }
    }

    pub fn apply(&mut self, state: &mut GameState, action: PolicyActionV5) -> Result<(), String> {
        let mut next_surface = self.clone();
        let mut next_state = state.clone();
        next_surface.apply_in_place(
            &mut next_state,
            action,
            EnvironmentBindingModeV5::Exact,
            None,
        )?;
        *self = next_surface;
        *state = next_state;
        Ok(())
    }

    pub(crate) fn apply_owned(
        &mut self,
        state: &mut GameState,
        action: PolicyActionV5,
        current_revision: u64,
        next_revision: u64,
    ) -> Result<(), String> {
        if current_revision.checked_add(1) != Some(next_revision) {
            return Err("invalid owned policy environment revision transition".to_string());
        }
        let mut next_surface = self.clone();
        let mut next_state = state.clone();
        next_surface.apply_in_place(
            &mut next_state,
            action,
            EnvironmentBindingModeV5::OwnedRevision(current_revision),
            Some(next_revision),
        )?;
        *self = next_surface;
        *state = next_state;
        Ok(())
    }

    fn validate_fast_actor_current_action(
        &self,
        state: &GameState,
        action: &PolicyActionV5,
        current_revision: u64,
        next_revision: u64,
    ) -> Result<(), String> {
        if current_revision.checked_add(1) != Some(next_revision) {
            return Err("invalid owned policy environment revision transition".to_string());
        }

        if self.hunter_order_v45.is_some() {
            return self.validate_hunter_order_action_v45(
                state,
                action,
                EnvironmentBindingModeV5::OwnedRevision(current_revision),
            );
        }
        let Some(scan) = self.scan.as_ref() else {
            if !matches!(action, PolicyActionV5::Surface(_)) {
                return Err("policy combat inclusion action without an active scan".to_string());
            }
            return Ok(());
        };

        scan.validate_binding(
            state,
            &self.inner,
            EnvironmentBindingModeV5::OwnedRevision(current_revision),
        )?;
        scan.validate_shape()?;

        let include = match (scan, action) {
            (
                CombatScanV5::Attackers {
                    player,
                    ordered_candidates,
                    cursor,
                    ..
                },
                PolicyActionV5::ChooseAttackerInclusion {
                    actor,
                    attacker,
                    include,
                },
            ) => {
                if *actor != *player {
                    return Err("stale policy combat scan actor binding".to_string());
                }
                if *attacker != ordered_candidates[*cursor] {
                    return Err("stale policy combat scan candidate binding".to_string());
                }
                *include
            }
            (
                CombatScanV5::Blockers {
                    player,
                    attacker,
                    ordered_candidates,
                    cursor,
                    ..
                },
                PolicyActionV5::ChooseBlockerInclusion {
                    actor,
                    attacker: action_attacker,
                    blocker,
                    include,
                },
            ) => {
                if *action_attacker != *attacker {
                    return Err("stale blocker scan attacker binding".to_string());
                }
                if *actor != *player {
                    return Err("stale policy combat scan actor binding".to_string());
                }
                if *blocker != ordered_candidates[*cursor] {
                    return Err("stale policy combat scan candidate binding".to_string());
                }
                *include
            }
            _ => return Err("policy action kind does not match active combat scan".to_string()),
        };

        // Aggregate combat commit is the only scan path that reaches H2/the
        // engine after advancing the scan. Preflight while everything is
        // still read-only: an answer must keep a legal completion (for the
        // final answer, the exact aggregate itself), so stale state cannot
        // partially mutate or reach H2's blocker `expect` boundary and no
        // accepted answer can strand the scan. `apply_in_place` applies the
        // same rule.
        scan.answer_keeps_a_legal_completion(state, &self.inner, include)
    }

    // Backend-only diagnostic. Do not attach this private scan to human views
    // or policy inputs. This deliberately omits hands, libraries, and RNG.
    fn fast_actor_rejection_value_v1(
        &self,
        state: &GameState,
        action: &PolicyActionV5,
        current_revision: u64,
        next_revision: u64,
        reason: &str,
    ) -> serde_json::Value {
        let battlefield: Vec<_> = state
            .objects
            .iter()
            .filter(|(_, object)| object.zone == crate::state::Zone::Battlefield)
            .map(|(id, object)| {
                serde_json::json!({
                    "id": id,
                    "card_def": object.card_def,
                    "name": object.name,
                    "controller": object.controller,
                    "tapped": object.tapped,
                    "damage": object.damage,
                    "minimum_blockers_override": object.v4.minimum_blockers_override,
                })
            })
            .collect();
        serde_json::json!({
            "schema": "mtg-kernel-fast-actor-prevalidation-rejection/v1",
            "visibility": "backend-private diagnostic, never a human view",
            "reason": reason,
            "action_debug": format!("{action:?}"),
            "current_revision": current_revision,
            "next_revision": next_revision,
            "turn": state.turn,
            "step": state.step,
            "active_player": state.active_player,
            "priority_player": state.priority_player,
            "scan": &self.scan,
            "combat": &state.engine.combat,
            "battlefield": battlefield,
        })
    }

    fn capture_fast_actor_rejection_v1(
        &self,
        state: &GameState,
        action: &PolicyActionV5,
        current_revision: u64,
        next_revision: u64,
        reason: &str,
    ) {
        let Some(path) = std::env::var_os("MTG_KERNEL_FAST_ACTOR_REJECTION_CAPTURE") else {
            return;
        };
        let path = std::path::PathBuf::from(path);
        let captured = (|| -> Result<(), String> {
            if !path.is_absolute() {
                return Err("absolute rejection capture path required".to_string());
            }
            let value = self.fast_actor_rejection_value_v1(
                state,
                action,
                current_revision,
                next_revision,
                reason,
            );
            let bytes = serde_json::to_vec(&value).map_err(|error| error.to_string())?;
            if bytes.len() > 16 * 1024 * 1024 {
                return Err("rejection capture exceeds 16 MiB".to_string());
            }
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|error| error.to_string())?;
            std::io::Write::write_all(&mut file, &bytes).map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())
        })();
        if let Err(error) = captured {
            eprintln!("fast actor rejection capture failed: {error}");
        }
    }

    /// Consumes an action proof created only from the fast session's exact
    /// current candidate. Ownership, revision, scan/action binding, and final
    /// combat aggregate legality are all checked before mutation; binding and
    /// application cannot be split across two policy surfaces.
    pub(crate) fn apply_fast_actor_current_candidate_in_place(
        &mut self,
        state: &mut GameState,
        proof: FastActorCurrentCandidateProofV1,
    ) -> Result<(), FastActorInPlaceApplyErrorV1> {
        let (owner_surface, action, current_revision, next_revision) = proof.into_parts();
        if !std::ptr::eq(owner_surface, std::ptr::from_ref(&*self)) {
            self.capture_fast_actor_rejection_v1(
                state,
                &action,
                current_revision,
                next_revision,
                "proof surface owner differs",
            );
            return Err(FastActorInPlaceApplyErrorV1::RejectedBeforeMutation);
        }
        self.validate_fast_actor_current_action(state, &action, current_revision, next_revision)
            .map_err(|reason| {
                self.capture_fast_actor_rejection_v1(
                    state,
                    &action,
                    current_revision,
                    next_revision,
                    &reason,
                );
                FastActorInPlaceApplyErrorV1::RejectedBeforeMutation
            })?;
        self.apply_in_place(
            state,
            action,
            EnvironmentBindingModeV5::OwnedRevision(current_revision),
            Some(next_revision),
        )
        .map_err(|_| FastActorInPlaceApplyErrorV1::InternalApplyFailure)
    }

    fn apply_in_place(
        &mut self,
        state: &mut GameState,
        action: PolicyActionV5,
        binding_mode: EnvironmentBindingModeV5,
        next_owned_revision: Option<u64>,
    ) -> Result<(), String> {
        if self.hunter_order_v45.is_some() {
            return self.apply_hunter_order_v45(state, action, binding_mode, next_owned_revision);
        }
        let Some(scan) = self.scan.as_mut() else {
            return match action {
                PolicyActionV5::Surface(action) => self.inner.apply(state, action),
                _ => Err("policy combat inclusion action without an active scan".to_string()),
            };
        };
        scan.validate_binding(state, &self.inner, binding_mode)?;
        scan.validate_shape()?;

        let (expected_actor, expected_candidate, include) = match (&*scan, action) {
            (
                CombatScanV5::Attackers {
                    player,
                    ordered_candidates: _,
                    cursor: _,
                    ..
                },
                PolicyActionV5::ChooseAttackerInclusion {
                    actor,
                    attacker,
                    include,
                },
            ) => (*player, (actor, attacker), include),
            (
                CombatScanV5::Blockers {
                    player,
                    attacker,
                    ordered_candidates: _,
                    cursor: _,
                    ..
                },
                PolicyActionV5::ChooseBlockerInclusion {
                    actor,
                    attacker: action_attacker,
                    blocker,
                    include,
                },
            ) => {
                if action_attacker != *attacker {
                    return Err("stale blocker scan attacker binding".to_string());
                }
                (*player, (actor, blocker), include)
            }
            _ => return Err("policy action kind does not match active combat scan".to_string()),
        };
        let (actor, candidate) = expected_candidate;
        if actor != expected_actor {
            return Err("stale policy combat scan actor binding".to_string());
        }
        let (current, completes_scan) = match &*scan {
            CombatScanV5::Attackers {
                ordered_candidates,
                cursor,
                ..
            }
            | CombatScanV5::Blockers {
                ordered_candidates,
                cursor,
                ..
            } => (
                ordered_candidates[*cursor],
                *cursor + 1 == ordered_candidates.len(),
            ),
        };
        if candidate != current {
            return Err("stale policy combat scan candidate binding".to_string());
        }
        // The rule `validate_fast_actor_current_action` preflights; the final
        // answer is validated by the aggregate commit below.
        if !completes_scan {
            scan.answer_keeps_a_legal_completion(state, &self.inner, include)?;
        }
        let (ordered_candidates, cursor, selected) = match scan {
            CombatScanV5::Attackers {
                ordered_candidates,
                cursor,
                selected,
                ..
            }
            | CombatScanV5::Blockers {
                ordered_candidates,
                cursor,
                selected,
                ..
            } => (ordered_candidates, cursor, selected),
        };
        if include {
            selected.push(candidate);
        }
        *cursor += 1;
        if *cursor < ordered_candidates.len() {
            if let Some(next_revision) = next_owned_revision {
                scan.rebind_owned_revision(next_revision)?;
            }
            return Ok(());
        }

        let completed = self.scan.as_ref().expect("completed scan exists");
        let aggregate = match completed {
            CombatScanV5::Attackers { selected, .. } => {
                SurfaceAction::Action(Action::DeclareAttackers(selected.clone()))
            }
            CombatScanV5::Blockers { selected, .. } => {
                SurfaceAction::DeclareBlockersForAttacker(selected.clone())
            }
        };

        self.inner.apply(state, aggregate)?;
        self.scan = None;
        Ok(())
    }
}

fn scan_binding(
    state: &GameState,
    surface: &HarnessSurfaceV2,
    mode: EnvironmentBindingModeV5,
) -> Result<CombatScanBindingV5, String> {
    match mode {
        EnvironmentBindingModeV5::Exact => Ok(CombatScanBindingV5::ExactSurfaceHash(
            surface_binding_hash(state, surface)?,
        )),
        EnvironmentBindingModeV5::OwnedRevision(revision) => {
            Ok(CombatScanBindingV5::OwnedEnvironmentRevision(revision))
        }
    }
}

impl Default for PolicySurfaceV5 {
    fn default() -> Self {
        Self::new()
    }
}

fn surface_binding_hash(state: &GameState, surface: &HarnessSurfaceV2) -> Result<u64, String> {
    #[cfg(test)]
    TEST_EXACT_SURFACE_HASH_CALLS.with(|calls| calls.set(calls.get().saturating_add(1)));

    Ok(fnv1a64(&surface_binding_envelope_bytes(state, surface)?))
}

/// Serialized surface-binding outer-envelope bytes, dispatched on the
/// state's diagnostic algorithm identity. Exactly one typed envelope is
/// serialized per state; the matching diagnostic identity is hardcoded in
/// each branch; any unknown future identity is a typed error. Private so
/// the production API surface is unchanged while same-module tests can
/// inspect the serialized schema and diagnostic identity.
fn surface_binding_envelope_bytes(
    state: &GameState,
    surface: &HarnessSurfaceV2,
) -> Result<Vec<u8>, String> {
    #[derive(Serialize)]
    struct SurfaceBindingEnvelopeV1 {
        schema_version: u32,
        diagnostic_state_hash_algorithm: &'static str,
        diagnostic_state_hash: u64,
        harness_surface_context: crate::surface_v2::HarnessSurfacePublicContextV2,
    }

    #[derive(Serialize)]
    struct SurfaceBindingEnvelopeV2 {
        schema_version: u32,
        diagnostic_state_hash_algorithm: &'static str,
        diagnostic_state_hash: u64,
        harness_surface_context: crate::surface_v2::HarnessSurfacePublicContextV2,
    }

    match state.diagnostic_state_hash_algorithm() {
        crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM => {
            serde_json::to_vec(&SurfaceBindingEnvelopeV1 {
                schema_version: 1,
                diagnostic_state_hash_algorithm: crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM,
                diagnostic_state_hash: state.diagnostic_state_hash(),
                harness_surface_context: surface.public_context(),
            })
            .map_err(|err| err.to_string())
        }
        crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM_ENVIRONMENT_V2 => {
            serde_json::to_vec(&SurfaceBindingEnvelopeV2 {
                schema_version: 2,
                diagnostic_state_hash_algorithm:
                    crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM_ENVIRONMENT_V2,
                diagnostic_state_hash: state.diagnostic_state_hash(),
                harness_surface_context: surface.public_context(),
            })
            .map_err(|err| err.to_string())
        }
        unknown => Err(format!(
            "unrecognized diagnostic state hash algorithm identity: {unknown}"
        )),
    }
}

/// Test oracle judged only by the engine's aggregate validators, by brute
/// force: does some completion of the active scan, after answering `include`
/// for the current candidate, declare legally?
#[cfg(test)]
pub(crate) fn scan_answer_has_legal_completion_for_test(
    surface: &PolicySurfaceV5,
    state: &GameState,
    include: bool,
) -> bool {
    let scan = surface.scan.as_ref().expect("active scan");
    let (candidates, cursor, selected) = match scan {
        CombatScanV5::Attackers {
            ordered_candidates,
            cursor,
            selected,
            ..
        }
        | CombatScanV5::Blockers {
            ordered_candidates,
            cursor,
            selected,
            ..
        } => (ordered_candidates, *cursor, selected),
    };
    let mut prefix = selected.clone();
    if include {
        prefix.push(candidates[cursor]);
    }
    let remaining = &candidates[(cursor + 1)..];
    (0..(1usize << remaining.len())).any(|mask| {
        let mut aggregate = prefix.clone();
        aggregate.extend(
            remaining
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1 << index) != 0)
                .map(|(_, id)| *id),
        );
        match scan {
            CombatScanV5::Attackers { .. } => {
                engine::validate_declare_attackers(state, &aggregate).is_ok()
            }
            CombatScanV5::Blockers { attacker, .. } => surface
                .inner
                .validate_declare_blockers_for_attacker(state, *attacker, &aggregate)
                .is_ok(),
        }
    })
}

#[cfg(test)]
thread_local! {
    static TEST_EXACT_SURFACE_HASH_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_test_exact_surface_hash_calls() {
    TEST_EXACT_SURFACE_HASH_CALLS.with(|calls| calls.set(0));
}

#[cfg(test)]
pub(crate) fn test_exact_surface_hash_calls() -> u64 {
    TEST_EXACT_SURFACE_HASH_CALLS.with(std::cell::Cell::get)
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn policy_decision_for_scan(scan: &CombatScanV5) -> Result<PolicyDecisionV5, String> {
    scan.validate_shape()?;
    match scan {
        CombatScanV5::Attackers {
            player,
            ordered_candidates,
            cursor,
            ..
        } => Ok(PolicyDecisionV5::AttackerInclusion {
            player: *player,
            attacker: ordered_candidates[*cursor],
            candidate_index: u32::try_from(*cursor)
                .map_err(|_| "combat scan candidate index exceeds u32".to_string())?,
            candidate_count: u32::try_from(ordered_candidates.len())
                .map_err(|_| "combat scan candidate count exceeds u32".to_string())?,
        }),
        CombatScanV5::Blockers {
            player,
            attacker,
            ordered_candidates,
            cursor,
            ..
        } => Ok(PolicyDecisionV5::BlockerInclusion {
            player: *player,
            attacker: *attacker,
            blocker: ordered_candidates[*cursor],
            candidate_index: u32::try_from(*cursor)
                .map_err(|_| "combat scan candidate index exceeds u32".to_string())?,
            candidate_count: u32::try_from(ordered_candidates.len())
                .map_err(|_| "combat scan candidate count exceeds u32".to_string())?,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_def::card_id_by_name;
    use crate::rl::{
        card_name, legal_action_candidates_v5, observe_policy_v5, ActionSemanticV1, PlayerSeatV1,
    };
    use crate::state::{Counters, GameObject, ObjectStateV4, Step, Zone};

    fn attacker_state(count: usize) -> (GameState, Vec<ObjectId>) {
        let mut state = GameState::new_from_libraries(&[], &[], card_name, 91);
        state.step = Step::DeclareAttackers;
        state.active_player = PlayerId::P0;
        state.priority_player = PlayerId::P0;
        state.engine.combat.attackers_declared = false;
        let card_def = card_id_by_name("Voldaren Epicure").unwrap();
        let mut ids = Vec::new();
        for _ in 0..count {
            let id = state.objects.push(GameObject {
                card_def,
                name: "Voldaren Epicure".to_string(),
                owner: PlayerId::P0,
                controller: PlayerId::P0,
                zone: Zone::Battlefield,
                tapped: false,
                summoning_sick: false,
                damage: 0,
                counters: Counters::default(),
                attachments: Vec::new(),
                v4: ObjectStateV4::from_card_def(card_def),
                spell_copy_origin: None,
                plotted_turn: None,
                zone_change_count: 0,
            });
            state.players[0].battlefield.push(id);
            ids.push(id);
        }
        (state, ids)
    }

    /// Sources of this game's committed combat damage to a player, in
    /// commit order. The combat record is cleared as the end of combat step
    /// ends (511.3), so a test whose commit auto-advanced past combat reads
    /// what its declarations did from the permanent event history instead.
    fn combat_damage_to_player_sources(state: &GameState) -> Vec<ObjectId> {
        state
            .engine
            .event_history
            .iter()
            .filter_map(|event| match event {
                crate::event::CommittedEvent::CombatDamageToPlayer { source, .. } => Some(*source),
                _ => None,
            })
            .collect()
    }

    /// Sources of the committed damage dealt to one creature, in commit
    /// order (in these combat-only fixtures, exactly its combat opponents).
    fn damage_sources_to(state: &GameState, target: ObjectId) -> Vec<ObjectId> {
        state
            .engine
            .event_history
            .iter()
            .filter_map(|event| match event {
                crate::event::CommittedEvent::Damage {
                    source,
                    target: crate::state::Target::Object(id),
                    amount,
                } if *id == target && *amount > 0 => Some(*source),
                _ => None,
            })
            .collect()
    }

    fn blocker_state(count: usize) -> (GameState, ObjectId, Vec<ObjectId>) {
        let (mut state, attackers) = attacker_state(1);
        let attacker = attackers[0];
        state.step = Step::DeclareBlockers;
        state.priority_player = PlayerId::P1;
        state.engine.combat.attackers_declared = true;
        state.engine.combat.attackers = vec![attacker];
        state.engine.combat.blockers_declared = false;
        let card_def = card_id_by_name("Voldaren Epicure").unwrap();
        let mut blockers = Vec::new();
        for _ in 0..count {
            let id = state.objects.push(GameObject {
                card_def,
                name: "Voldaren Epicure".to_string(),
                owner: PlayerId::P1,
                controller: PlayerId::P1,
                zone: Zone::Battlefield,
                tapped: false,
                summoning_sick: false,
                damage: 0,
                counters: Counters::default(),
                attachments: Vec::new(),
                v4: ObjectStateV4::from_card_def(card_def),
                spell_copy_origin: None,
                plotted_turn: None,
                zone_change_count: 0,
            });
            state.players[1].battlefield.push(id);
            blockers.push(id);
        }
        (state, attacker, blockers)
    }

    #[test]
    fn rejection_capture_preserves_exact_combat_reason_without_hidden_zones_or_mutation() {
        let (mut state, attacker, blockers) = blocker_state(2);
        state.objects.get_mut(attacker).v4.minimum_blockers_override = Some(2);
        let mut hidden = state.objects.get(attacker).clone();
        hidden.name = "private-library-sentinel".to_string();
        hidden.zone = Zone::Library;
        let hidden_id = state.objects.push(hidden);
        state.players[0].library.push(hidden_id);
        let mut surface = PolicySurfaceV5::new();
        surface.next_decision_owned(&mut state, 7).unwrap();
        surface
            .apply_owned(
                &mut state,
                PolicyActionV5::ChooseBlockerInclusion {
                    actor: PlayerId::P1,
                    attacker,
                    blocker: blockers[0],
                    include: true,
                },
                7,
                8,
            )
            .unwrap();
        surface.next_decision_owned(&mut state, 8).unwrap();
        let action = PolicyActionV5::ChooseBlockerInclusion {
            actor: PlayerId::P1,
            attacker,
            blocker: blockers[1],
            include: false,
        };
        let before_state = state.clone();
        let before_surface = surface.clone();
        let reason = surface
            .validate_fast_actor_current_action(&state, &action, 8, 9)
            .unwrap_err();
        assert_eq!(
            reason,
            format!("{attacker} requires at least 2 creatures to block it")
        );
        let value = surface.fast_actor_rejection_value_v1(&state, &action, 8, 9, &reason);
        assert_eq!(value["reason"], reason);
        assert_eq!(value["scan"]["cursor"], 1);
        assert_eq!(value["scan"]["selected"], serde_json::json!([blockers[0]]));
        assert_eq!(value["current_revision"], 8);
        assert_eq!(value["next_revision"], 9);
        assert_eq!(value["battlefield"].as_array().unwrap().len(), 3);
        assert!(value["action_debug"]
            .as_str()
            .unwrap()
            .contains("include: false"));
        assert!(!value.to_string().contains("private-library-sentinel"));
        assert!(value.get("state").is_none());
        assert!(value.get("players").is_none());
        assert_eq!(state, before_state);
        assert_eq!(surface.scan, before_surface.scan);
        assert_eq!(
            surface_binding_hash(&state, &surface.inner).unwrap(),
            surface_binding_hash(&before_state, &before_surface.inner).unwrap()
        );
    }

    fn apply_attacker_bits(surface: &mut PolicySurfaceV5, state: &mut GameState, bits: &[bool]) {
        for (index, include) in bits.iter().copied().enumerate() {
            let decision = surface.next_decision(state).unwrap();
            let PolicyDecisionV5::AttackerInclusion {
                player,
                attacker,
                candidate_index,
                candidate_count,
            } = decision
            else {
                panic!("expected attacker inclusion");
            };
            assert_eq!(candidate_index as usize, index);
            assert_eq!(candidate_count as usize, bits.len());
            surface
                .apply(
                    state,
                    PolicyActionV5::ChooseAttackerInclusion {
                        actor: player,
                        attacker,
                        include,
                    },
                )
                .unwrap();
        }
    }

    #[test]
    fn thirteen_attackers_have_two_actions_per_microstep_and_one_atomic_commit() {
        let (mut state, candidates) = attacker_state(13);
        let initial = state.clone();
        let mut surface = PolicySurfaceV5::new();
        let bits: Vec<bool> = (0..13).map(|index| index % 3 == 0).collect();
        for (index, include) in bits.iter().copied().enumerate() {
            let decision = surface.next_decision(&mut state).unwrap();
            let actions = legal_action_candidates_v5(&decision, &state).unwrap();
            assert_eq!(actions.len(), 2);
            assert_eq!(actions[0].record.selected_index, 0);
            assert_eq!(actions[1].record.selected_index, 1);
            assert!(matches!(
                actions[0].record.semantic,
                ActionSemanticV1::ChooseAttackerInclusion { include: false, .. }
            ));
            assert!(matches!(
                actions[1].record.semantic,
                ActionSemanticV1::ChooseAttackerInclusion { include: true, .. }
            ));
            assert!(actions
                .iter()
                .all(|action| action.record.stable_id.starts_with("legal-action-v5:")));
            surface
                .apply(
                    &mut state,
                    actions[usize::from(include)].policy_action.clone(),
                )
                .unwrap();
            if index + 1 < bits.len() {
                assert_eq!(state, initial, "partial scan mutated GameState");
            }
        }
        let expected: Vec<_> = candidates
            .into_iter()
            .zip(bits)
            .filter_map(|(id, include)| include.then_some(id))
            .collect();
        assert_eq!(state.engine.combat.attackers, expected);
        assert!(state.engine.combat.attackers_declared);
    }

    #[test]
    fn zero_candidate_combat_inherits_h2_silent_commit_without_policy_microstep() {
        let (mut attack_state, _) = attacker_state(0);
        let attack_turn = attack_state.turn;
        let mut attack_surface = PolicySurfaceV5::new();
        let attack_next = attack_surface.next_decision(&mut attack_state).unwrap();
        assert!(!matches!(
            attack_next,
            PolicyDecisionV5::AttackerInclusion { .. }
        ));
        // The silent commit auto-advances past combat, whose record is
        // cleared as the end of combat step ends (511.3): the empty
        // declaration shows as having left the declare attackers step with
        // no combat damage dealt.
        assert!(attack_state.turn != attack_turn || attack_state.step != Step::DeclareAttackers);
        assert!(combat_damage_to_player_sources(&attack_state).is_empty());

        let (mut block_state, attacker, _) = blocker_state(0);
        let mut block_surface = PolicySurfaceV5::new();
        let block_next = block_surface.next_decision(&mut block_state).unwrap();
        assert!(!matches!(
            block_next,
            PolicyDecisionV5::BlockerInclusion { .. }
        ));
        // The empty block declaration left the attacker unblocked.
        assert_eq!(
            combat_damage_to_player_sources(&block_state),
            vec![attacker]
        );
        assert!(damage_sources_to(&block_state, attacker).is_empty());
    }

    #[test]
    fn thirteen_blockers_have_two_actions_per_microstep_and_one_atomic_commit() {
        let (mut state, attacker, blockers) = blocker_state(13);
        let initial = state.clone();
        let mut surface = PolicySurfaceV5::new();
        for (index, blocker) in blockers.iter().copied().enumerate() {
            let decision = surface.next_decision(&mut state).unwrap();
            let PolicyDecisionV5::BlockerInclusion {
                player,
                attacker: fixed_attacker,
                blocker: current,
                candidate_index,
                candidate_count,
            } = decision
            else {
                panic!("expected blocker inclusion");
            };
            assert_eq!(player, PlayerId::P1);
            assert_eq!(fixed_attacker, attacker);
            assert_eq!(current, blocker);
            assert_eq!(candidate_index as usize, index);
            assert_eq!(candidate_count, 13);
            let actions =
                legal_action_candidates_v5(&surface.next_decision(&mut state).unwrap(), &state)
                    .unwrap();
            assert!(matches!(
                actions[0].record.semantic,
                ActionSemanticV1::ChooseBlockerInclusion { include: false, .. }
            ));
            assert!(matches!(
                actions[1].record.semantic,
                ActionSemanticV1::ChooseBlockerInclusion { include: true, .. }
            ));
            surface
                .apply(
                    &mut state,
                    actions[usize::from(index % 4 == 0)].policy_action.clone(),
                )
                .unwrap();
            if index + 1 < blockers.len() {
                assert_eq!(state, initial);
            }
        }
        assert!(state.engine.combat.blockers_declared);
        assert_eq!(state.engine.combat.blocked_by.len(), 1);
        assert_eq!(
            state.engine.combat.blocked_by[0].1,
            blockers
                .into_iter()
                .enumerate()
                .filter_map(|(index, id)| (index % 4 == 0).then_some(id))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn blocker_selected_for_first_attacker_is_not_reoffered_for_second() {
        let (mut state, attackers) = attacker_state(2);
        state.step = Step::DeclareBlockers;
        state.priority_player = PlayerId::P1;
        state.engine.combat.attackers_declared = true;
        state.engine.combat.attackers = attackers.clone();
        let card_def = card_id_by_name("Voldaren Epicure").unwrap();
        let blocker = state.objects.push(GameObject {
            card_def,
            name: "Voldaren Epicure".to_string(),
            owner: PlayerId::P1,
            controller: PlayerId::P1,
            zone: Zone::Battlefield,
            tapped: false,
            summoning_sick: false,
            damage: 0,
            counters: Counters::default(),
            attachments: Vec::new(),
            v4: ObjectStateV4::from_card_def(card_def),
            spell_copy_origin: None,
            plotted_turn: None,
            zone_change_count: 0,
        });
        state.players[1].battlefield.push(blocker);
        let mut surface = PolicySurfaceV5::new();
        let first = surface.next_decision(&mut state).unwrap();
        let actions = legal_action_candidates_v5(&first, &state).unwrap();
        surface
            .apply(&mut state, actions[1].policy_action.clone())
            .unwrap();
        let next = surface.next_decision(&mut state).unwrap();
        assert!(!matches!(
            next,
            PolicyDecisionV5::BlockerInclusion { blocker: candidate, .. }
                if candidate == blocker
        ));
        // Combat has ended (511.3 cleared its record): the blocker blocked
        // only the first attacker, and the second went unblocked.
        assert_eq!(damage_sources_to(&state, blocker), vec![attackers[0]]);
        assert_eq!(damage_sources_to(&state, attackers[0]), vec![blocker]);
        assert_eq!(combat_damage_to_player_sources(&state), vec![attackers[1]]);
    }

    #[test]
    fn exhaustive_six_candidate_scan_matches_aggregate_engine_transition() {
        for count in 1..=6 {
            for mask in 0..(1usize << count) {
                let (mut scanned, candidates) = attacker_state(count);
                let mut expected = scanned.clone();
                let bits: Vec<bool> = (0..count)
                    .map(|index| mask & (1usize << index) != 0)
                    .collect();
                let picked: Vec<_> = candidates
                    .iter()
                    .copied()
                    .zip(bits.iter().copied())
                    .filter_map(|(id, include)| include.then_some(id))
                    .collect();
                crate::engine::step(&mut expected, Action::DeclareAttackers(picked)).unwrap();
                let mut surface = PolicySurfaceV5::new();
                apply_attacker_bits(&mut surface, &mut scanned, &bits);
                assert_eq!(scanned, expected, "count={count} mask={mask:#x}");
            }
        }
    }

    #[test]
    fn private_scan_context_changes_chooser_hash_and_is_absent_for_opponent() {
        let (mut state, _) = attacker_state(3);
        let mut surface = PolicySurfaceV5::new();
        let first = surface.next_decision(&mut state).unwrap();
        let before = observe_policy_v5(&state, &surface, PlayerId::P0, 0, 0, 0, 3).unwrap();
        let opponent = observe_policy_v5(&state, &surface, PlayerId::P1, 0, 0, 0, 3).unwrap();
        assert!(before
            .projection
            .policy_surface_context
            .private_combat_selection
            .is_some());
        assert!(opponent
            .projection
            .policy_surface_context
            .private_combat_selection
            .is_none());
        let actions = legal_action_candidates_v5(&first, &state).unwrap();
        surface
            .apply(&mut state, actions[1].policy_action.clone())
            .unwrap();
        let _ = surface.next_decision(&mut state).unwrap();
        let after = observe_policy_v5(&state, &surface, PlayerId::P0, 1, 0, 1, 3).unwrap();
        assert_ne!(
            before.visible_projection_hash,
            after.visible_projection_hash
        );
        assert_eq!(before.acting_player, PlayerSeatV1::P0);
    }

    #[test]
    fn stable_ids_are_display_independent_and_exactly_bound_to_boolean() {
        let (mut state, _) = attacker_state(1);
        let mut surface = PolicySurfaceV5::new();
        let decision = surface.next_decision(&mut state).unwrap();
        let actions = legal_action_candidates_v5(&decision, &state).unwrap();
        assert_ne!(actions[0].record.stable_id, actions[1].record.stable_id);
        assert_eq!(
            actions[0].record.stable_id,
            "legal-action-v5:e247d7fbf4a7bf81"
        );
        assert_eq!(
            actions[1].record.stable_id,
            "legal-action-v5:2b3f89ec54196a32"
        );
        let rebuilt = crate::rl::make_legal_action_v5(
            0,
            actions[0].record.semantic.clone(),
            Some("arbitrary diagnostic label".to_string()),
        )
        .unwrap();
        assert_eq!(rebuilt.stable_id, actions[0].record.stable_id);
    }

    #[test]
    fn failed_final_commit_preserves_state_scan_and_retry_identity() {
        let (mut state, candidates) = attacker_state(3);
        let mut surface = PolicySurfaceV5::new();
        let first = surface.next_decision(&mut state).unwrap();
        let first_actions = legal_action_candidates_v5(&first, &state).unwrap();
        surface
            .apply(&mut state, first_actions[1].policy_action.clone())
            .unwrap();
        let second = surface.next_decision(&mut state).unwrap();
        let second_actions = legal_action_candidates_v5(&second, &state).unwrap();
        surface
            .apply(&mut state, second_actions[1].policy_action.clone())
            .unwrap();

        let CombatScanV5::Attackers { selected, .. } =
            surface.scan.as_mut().expect("scan remains active")
        else {
            unreachable!()
        };
        selected[1] = candidates[0];
        let state_before = state.clone();
        let surface_before = surface.clone();
        let final_decision = surface.next_decision(&mut state).unwrap();
        let final_actions = legal_action_candidates_v5(&final_decision, &state).unwrap();
        assert!(surface
            .apply(&mut state, final_actions[0].policy_action.clone())
            .is_err());
        assert_eq!(state, state_before);
        assert_eq!(surface.scan, surface_before.scan);
        assert_eq!(
            surface.privileged_scan_context().unwrap(),
            surface_before.privileged_scan_context().unwrap()
        );
        let retry = surface.next_decision(&mut state).unwrap();
        assert_eq!(retry, final_decision);
        assert_eq!(
            legal_action_candidates_v5(&retry, &state)
                .unwrap()
                .iter()
                .map(|action| action.record.stable_id.clone())
                .collect::<Vec<_>>(),
            final_actions
                .iter()
                .map(|action| action.record.stable_id.clone())
                .collect::<Vec<_>>()
        );
    }

    fn goad(state: &mut GameState, attacker: ObjectId) {
        let expires_at_turn = state.turn + 1;
        state.objects.get_mut(attacker).v4.goaded_by = vec![crate::state::GoadStateV4 {
            player: PlayerId::P1,
            expires_at_turn,
        }];
    }

    fn inclusion_action(decision: &PolicyDecisionV5, include: bool) -> PolicyActionV5 {
        match *decision {
            PolicyDecisionV5::AttackerInclusion {
                player, attacker, ..
            } => PolicyActionV5::ChooseAttackerInclusion {
                actor: player,
                attacker,
                include,
            },
            PolicyDecisionV5::BlockerInclusion {
                player,
                attacker,
                blocker,
                ..
            } => PolicyActionV5::ChooseBlockerInclusion {
                actor: player,
                attacker,
                blocker,
                include,
            },
            PolicyDecisionV5::Surface(ref other) => {
                panic!("expected a combat scan step, got {other:?}")
            }
        }
    }

    /// Walks every reachable answer sequence of every scan in this combat
    /// step (including later per-attacker blocker scans) and requires the
    /// surface to accept exactly the answers that keep a legal completion.
    fn assert_scan_accepts_exactly_the_completable_answers(
        surface: &PolicySurfaceV5,
        state: &GameState,
        label: &str,
    ) {
        let mut probe_state = state.clone();
        let mut probe_surface = surface.clone();
        let decision = probe_surface.next_decision(&mut probe_state).unwrap();
        if matches!(decision, PolicyDecisionV5::Surface(_)) {
            return;
        }
        for include in [false, true] {
            let completable =
                scan_answer_has_legal_completion_for_test(&probe_surface, &probe_state, include);
            let mut next_state = probe_state.clone();
            let mut next_surface = probe_surface.clone();
            let accepted = next_surface
                .apply(&mut next_state, inclusion_action(&decision, include))
                .is_ok();
            assert_eq!(
                accepted, completable,
                "{label}: scan {:?}, answer include={include}",
                probe_surface.scan
            );
            if accepted {
                assert_scan_accepts_exactly_the_completable_answers(
                    &next_surface,
                    &next_state,
                    label,
                );
            }
        }
    }

    /// Spellbench launch-benchmark reproduction (Elves, Undercity Arena
    /// goad): declining a goaded attacker at a non-final scan step used to be
    /// accepted, after which both final answers were refused, so no legal
    /// declaration remained.
    #[test]
    fn declining_a_goaded_attacker_is_refused_before_the_scan_is_stranded() {
        let (mut state, candidates) = attacker_state(2);
        goad(&mut state, candidates[0]);
        let mut surface = PolicySurfaceV5::new();
        let first = surface.next_decision(&mut state).unwrap();
        assert!(matches!(
            first,
            PolicyDecisionV5::AttackerInclusion { attacker, .. } if attacker == candidates[0]
        ));
        let state_before = state.clone();
        let surface_before = surface.clone();
        let error = surface
            .apply(&mut state, inclusion_action(&first, false))
            .unwrap_err();
        assert_eq!(
            error,
            "one or more goaded creatures able to attack was omitted"
        );
        assert_eq!(state, state_before);
        assert_eq!(surface.scan, surface_before.scan);

        surface
            .apply(&mut state, inclusion_action(&first, true))
            .unwrap();
        let second = surface.next_decision(&mut state).unwrap();
        surface
            .apply(&mut state, inclusion_action(&second, false))
            .unwrap();
        assert!(state.engine.combat.attackers_declared);
        assert_eq!(state.engine.combat.attackers, vec![candidates[0]]);
    }

    /// A partial block that can no longer reach the attacker's minimum is
    /// refused when it is answered, not only at the final commit.
    #[test]
    fn a_partial_block_that_cannot_reach_the_minimum_is_refused_when_answered() {
        let (mut state, attacker, blockers) = blocker_state(3);
        state.objects.get_mut(attacker).v4.minimum_blockers_override = Some(3);
        let mut surface = PolicySurfaceV5::new();
        let first = surface.next_decision(&mut state).unwrap();
        surface
            .apply(&mut state, inclusion_action(&first, true))
            .unwrap();
        let second = surface.next_decision(&mut state).unwrap();
        let state_before = state.clone();
        let surface_before = surface.clone();
        let error = surface
            .apply(&mut state, inclusion_action(&second, false))
            .unwrap_err();
        assert_eq!(
            error,
            format!("{attacker} requires at least 3 creatures to block it")
        );
        assert_eq!(state, state_before);
        assert_eq!(surface.scan, surface_before.scan);

        surface
            .apply(&mut state, inclusion_action(&second, true))
            .unwrap();
        let third = surface.next_decision(&mut state).unwrap();
        surface
            .apply(&mut state, inclusion_action(&third, true))
            .unwrap();
        assert!(state.engine.combat.blockers_declared);
        assert_eq!(state.engine.combat.blocked_by, vec![(attacker, blockers)]);
    }

    #[test]
    fn scan_accepts_exactly_the_answers_that_keep_a_legal_completion() {
        for count in 1..=4 {
            for goad_mask in 0..(1usize << count) {
                let (mut state, candidates) = attacker_state(count);
                for (index, id) in candidates.iter().copied().enumerate() {
                    if goad_mask & (1 << index) != 0 {
                        goad(&mut state, id);
                    }
                }
                assert_scan_accepts_exactly_the_completable_answers(
                    &PolicySurfaceV5::new(),
                    &state,
                    &format!("attackers count={count} goad_mask={goad_mask:#06b}"),
                );
            }
        }
        for count in 1..=4 {
            for minimum in 1..=3u8 {
                let (mut state, attacker, _) = blocker_state(count);
                state.objects.get_mut(attacker).v4.minimum_blockers_override = Some(minimum);
                assert_scan_accepts_exactly_the_completable_answers(
                    &PolicySurfaceV5::new(),
                    &state,
                    &format!("blockers count={count} minimum={minimum}"),
                );
            }
        }
        // H2 removes blockers already assigned to an earlier attacker, so a
        // later minimum-two attacker can be scanned with a single candidate.
        for minimum in 1..=3u8 {
            let (mut state, first_attacker, _) = blocker_state(3);
            let card_def = state.objects.get(first_attacker).card_def;
            let mut second = state.objects.get(first_attacker).clone();
            second.v4 = ObjectStateV4::from_card_def(card_def);
            second.v4.minimum_blockers_override = Some(minimum);
            let second_attacker = state.objects.push(second);
            state.players[0].battlefield.push(second_attacker);
            state.engine.combat.attackers.push(second_attacker);
            assert_scan_accepts_exactly_the_completable_answers(
                &PolicySurfaceV5::new(),
                &state,
                &format!("two attackers, second minimum={minimum}"),
            );
        }
    }

    #[test]
    fn external_state_staleness_is_sanitized_nonmutating_and_retryable_after_restore() {
        reset_test_exact_surface_hash_calls();
        let (mut state, _) = attacker_state(2);
        let mut surface = PolicySurfaceV5::new();
        let decision = surface.next_decision(&mut state).unwrap();
        assert_eq!(test_exact_surface_hash_calls(), 1);
        let actions = legal_action_candidates_v5(&decision, &state).unwrap();
        let pristine_state = state.clone();
        let pristine_surface = surface.clone();

        state.players[1].life -= 1;
        let tampered_state = state.clone();
        let err = surface
            .apply(&mut state, actions[1].policy_action.clone())
            .unwrap_err();
        assert_eq!(test_exact_surface_hash_calls(), 2);
        assert_eq!(err, "stale policy combat scan environment binding");
        assert!(!err.contains("0x"));
        assert_eq!(state, tampered_state);
        assert_eq!(surface.scan, pristine_surface.scan);

        state = pristine_state;
        assert_eq!(surface.next_decision(&mut state).unwrap(), decision);
        assert_eq!(test_exact_surface_hash_calls(), 3);
        surface
            .apply(&mut state, actions[1].policy_action.clone())
            .unwrap();
        assert_eq!(test_exact_surface_hash_calls(), 4);
        assert!(surface.scan_active());
    }

    fn standalone_binding_states() -> (GameState, GameState) {
        let mut legacy =
            GameState::new_from_libraries(&[1, 2, 3], &[4, 5, 6, 7], |c| format!("card-{c}"), 99);
        legacy.draw_card(PlayerId::P0);
        legacy.draw_card(PlayerId::P1);
        let mut v2 = GameState::new_from_libraries_environment_v2(
            &[1, 2, 3],
            &[4, 5, 6, 7],
            |c| format!("card-{c}"),
            99,
        );
        v2.draw_card(PlayerId::P0);
        v2.draw_card(PlayerId::P1);
        (legacy, v2)
    }

    #[test]
    fn surface_binding_envelopes_are_diagnostic_dispatched_with_exact_goldens() {
        let (legacy, v2) = standalone_binding_states();
        let surface = crate::surface_v2::HarnessSurfaceV2::new();
        // Stale duplicate of state.rs's own
        // `diagnostic_state_hash_contract_and_golden_value_are_frozen`
        // golden (same fixture shape: two_card_libraries/debug_names, seed
        // 99, draw P0 then P1): that test was re-baselined to
        // 0x3313_5945_dcb9_4ed1 when Task 11 added `GameState::monarch`
        // (serialized unconditionally, shifting the v8 envelope for every
        // state), but this file's own copy of the same literal was missed.
        // Old value: 0xa921_902d_e1a8_d8ce.
        assert_eq!(legacy.diagnostic_state_hash(), 0x3313_5945_dcb9_4ed1);
        // Re-pinned for the pauper-meta-cards-v1 card lane's wave 1 (Task
        // 13, identity finalisation): the SurfaceBinding envelope only
        // serializes `diagnostic_state_hash` plus surface context, never
        // any CardDef-shaped state, so this has the same root cause as the
        // state-hash re-pin directly above (`GameState::monarch`, added in
        // Task 11, serialized unconditionally and shifting the envelope
        // for every state), not the wave's new cards moving
        // KERNEL_CARDDB_HASH. Old value: 0xd281_e56f_4389_60a9. New value
        // is this test's own live-computed hash, read directly from a
        // failing run (never hand-typed).
        assert_eq!(
            surface_binding_hash(&legacy, &surface).expect("legacy binding hashes"),
            0x6940_0c4c_9fdc_2b49,
            "final-pool-v8 legacy SurfaceBinding V1 golden"
        );
        // Re-pinned for the pauper-meta-cards-v1 card lane's wave 1 (Task
        // 13, identity finalisation): same root cause as the legacy state
        // hash above (`GameState::monarch`, added in Task 11, is serialized
        // unconditionally and shifts the v8/v9 envelope for every state).
        // Old value: 0x8ecd_b59c_374e_2345.
        assert_eq!(v2.diagnostic_state_hash(), 0x9689_f972_2063_c266);
        // Re-pinned for the pauper-meta-cards-v1 card lane's wave 1 (Task
        // 13, identity finalisation): same root cause as the two goldens
        // above. Old value: 0x5c8f_cd1c_7941_a53e.
        assert_eq!(
            surface_binding_hash(&v2, &surface).expect("v2 binding hashes"),
            0xc8e2_f885_b7e8_8615,
            "final-pool-v9 SurfaceBinding V2 golden"
        );

        let legacy_bytes =
            surface_binding_envelope_bytes(&legacy, &surface).expect("legacy envelope bytes");
        assert!(
            legacy_bytes.starts_with(br#"{"schema_version":1,"diagnostic_state_hash_algorithm":"#),
            "V1 field order is frozen"
        );
        let legacy_value: serde_json::Value =
            serde_json::from_slice(&legacy_bytes).expect("legacy envelope parses");
        assert_eq!(legacy_value["schema_version"], 1);
        assert_eq!(
            legacy_value["diagnostic_state_hash_algorithm"],
            "fnv1a64-serde-json-game-state-envelope-v8"
        );
        assert!(
            legacy_value.get("hash_algorithm").is_none(),
            "surface binding envelopes carry no hash_algorithm field"
        );

        let v2_bytes = surface_binding_envelope_bytes(&v2, &surface).expect("v2 envelope bytes");
        assert!(
            v2_bytes.starts_with(br#"{"schema_version":2,"diagnostic_state_hash_algorithm":"#),
            "V2 keeps the identical field order"
        );
        let v2_value: serde_json::Value =
            serde_json::from_slice(&v2_bytes).expect("v2 envelope parses");
        assert_eq!(v2_value["schema_version"], 2);
        assert_eq!(
            v2_value["diagnostic_state_hash_algorithm"],
            "fnv1a64-serde-json-game-state-envelope-v9"
        );
        assert!(v2_value.get("hash_algorithm").is_none());
    }

    /// Environment-v2 sibling of `attacker_state`: identical battlefield
    /// construction on a v2 randomness state.
    fn attacker_state_environment_v2(count: usize) -> (GameState, Vec<ObjectId>) {
        let mut state = GameState::new_from_libraries_environment_v2(&[], &[], card_name, 91);
        state.step = Step::DeclareAttackers;
        state.active_player = PlayerId::P0;
        state.priority_player = PlayerId::P0;
        state.engine.combat.attackers_declared = false;
        let card_def = card_id_by_name("Voldaren Epicure").unwrap();
        let mut ids = Vec::new();
        for _ in 0..count {
            let id = state.objects.push(GameObject {
                card_def,
                name: "Voldaren Epicure".to_string(),
                owner: PlayerId::P0,
                controller: PlayerId::P0,
                zone: Zone::Battlefield,
                tapped: false,
                summoning_sick: false,
                damage: 0,
                counters: Counters::default(),
                attachments: Vec::new(),
                v4: ObjectStateV4::from_card_def(card_def),
                spell_copy_origin: None,
                plotted_turn: None,
                zone_change_count: 0,
            });
            state.players[0].battlefield.push(id);
            ids.push(id);
        }
        (state, ids)
    }

    #[test]
    fn external_state_staleness_is_sanitized_on_environment_v2_states() {
        reset_test_exact_surface_hash_calls();
        let (mut state, _) = attacker_state_environment_v2(2);
        let mut surface = PolicySurfaceV5::new();
        let decision = surface.next_decision(&mut state).unwrap();
        assert_eq!(test_exact_surface_hash_calls(), 1);
        let actions = legal_action_candidates_v5(&decision, &state).unwrap();
        let pristine_state = state.clone();
        let pristine_surface = surface.clone();

        state.players[1].life -= 1;
        let tampered_state = state.clone();
        let err = surface
            .apply(&mut state, actions[1].policy_action.clone())
            .unwrap_err();
        assert_eq!(test_exact_surface_hash_calls(), 2);
        assert_eq!(err, "stale policy combat scan environment binding");
        assert!(!err.contains("0x"));
        assert_eq!(state, tampered_state);
        assert_eq!(surface.scan, pristine_surface.scan);

        state = pristine_state;
        assert_eq!(surface.next_decision(&mut state).unwrap(), decision);
        assert_eq!(test_exact_surface_hash_calls(), 3);
        surface
            .apply(&mut state, actions[1].policy_action.clone())
            .unwrap();
        assert_eq!(test_exact_surface_hash_calls(), 4);
        assert!(surface.scan_active());
    }
}
