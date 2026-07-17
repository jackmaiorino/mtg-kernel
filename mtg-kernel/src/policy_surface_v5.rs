//! Policy-only v5 decision surface.
//!
//! The rules engine and [`HarnessSurfaceV2`] intentionally keep their
//! aggregate combat declarations.  This wrapper presents those declarations
//! to a policy as a canonical binary scan and commits the aggregate action
//! exactly once, after the final Boolean answer.

use crate::engine::{Action, Decision};
use crate::ids::{ObjectId, PlayerId};
use crate::state::GameState;
use crate::surface_v2::{HarnessSurfaceV2, SurfaceAction, SurfaceDecision};
use serde::{Deserialize, Serialize};

pub const POLICY_SURFACE_VERSION: u32 = 5;
pub const POLICY_ENVIRONMENT_HASH_ALGORITHM: &str =
    "fnv1a64-serde-json-policy-environment-envelope-v1";

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
                    | Decision::ChooseEffectOption { player, .. }
                    | Decision::ChooseEffectTargets { player, .. }
                    | Decision::ChooseEffectBoolean { player, .. }
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
        bound_surface_hash: u64,
    },
    Blockers {
        player: PlayerId,
        attacker: ObjectId,
        ordered_candidates: Vec<ObjectId>,
        cursor: usize,
        selected: Vec<ObjectId>,
        bound_surface_hash: u64,
    },
}

impl CombatScanV5 {
    fn validate_binding(
        &self,
        state: &GameState,
        surface: &HarnessSurfaceV2,
    ) -> Result<(), String> {
        let expected = match self {
            CombatScanV5::Attackers {
                bound_surface_hash, ..
            }
            | CombatScanV5::Blockers {
                bound_surface_hash, ..
            } => *bound_surface_hash,
        };
        let actual = surface_binding_hash(state, surface)?;
        if actual != expected {
            return Err("stale policy combat scan environment binding".to_string());
        }
        Ok(())
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
}

#[derive(Clone, Default)]
pub struct PolicySurfaceV5 {
    inner: HarnessSurfaceV2,
    scan: Option<CombatScanV5>,
}

impl PolicySurfaceV5 {
    pub fn new() -> Self {
        Self::default()
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
        self.inner = HarnessSurfaceV2::new();
    }

    pub fn discard_unanswered_scan(&mut self) -> Result<(), String> {
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

    pub fn next_decision(&mut self, state: &mut GameState) -> Result<PolicyDecisionV5, String> {
        if let Some(scan) = &self.scan {
            scan.validate_binding(state, &self.inner)?;
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
                    bound_surface_hash: surface_binding_hash(state, &self.inner)?,
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
                    bound_surface_hash: surface_binding_hash(state, &self.inner)?,
                });
                policy_decision_for_scan(self.scan.as_ref().expect("scan just initialized"))
            }
            other => Ok(PolicyDecisionV5::Surface(other)),
        }
    }

    pub fn apply(&mut self, state: &mut GameState, action: PolicyActionV5) -> Result<(), String> {
        let mut next_surface = self.clone();
        let mut next_state = state.clone();
        next_surface.apply_in_place(&mut next_state, action)?;
        *self = next_surface;
        *state = next_state;
        Ok(())
    }

    fn apply_in_place(
        &mut self,
        state: &mut GameState,
        action: PolicyActionV5,
    ) -> Result<(), String> {
        let Some(scan) = self.scan.as_mut() else {
            return match action {
                PolicyActionV5::Surface(action) => self.inner.apply(state, action),
                _ => Err("policy combat inclusion action without an active scan".to_string()),
            };
        };
        scan.validate_binding(state, &self.inner)?;
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
        if candidate != ordered_candidates[*cursor] {
            return Err("stale policy combat scan candidate binding".to_string());
        }
        if include {
            selected.push(candidate);
        }
        *cursor += 1;
        if *cursor < ordered_candidates.len() {
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

fn surface_binding_hash(state: &GameState, surface: &HarnessSurfaceV2) -> Result<u64, String> {
    #[derive(Serialize)]
    struct SurfaceBindingEnvelopeV1 {
        schema_version: u32,
        diagnostic_state_hash_algorithm: &'static str,
        diagnostic_state_hash: u64,
        harness_surface_context: crate::surface_v2::HarnessSurfacePublicContextV2,
    }

    let envelope = SurfaceBindingEnvelopeV1 {
        schema_version: 1,
        diagnostic_state_hash_algorithm: crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM,
        diagnostic_state_hash: state.diagnostic_state_hash(),
        harness_surface_context: surface.public_context(),
    };
    let bytes = serde_json::to_vec(&envelope).map_err(|err| err.to_string())?;
    Ok(fnv1a64(&bytes))
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
                plotted_turn: None,
                zone_change_count: 0,
            });
            state.players[0].battlefield.push(id);
            ids.push(id);
        }
        (state, ids)
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
                plotted_turn: None,
                zone_change_count: 0,
            });
            state.players[1].battlefield.push(id);
            blockers.push(id);
        }
        (state, attacker, blockers)
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
        let mut attack_surface = PolicySurfaceV5::new();
        let attack_next = attack_surface.next_decision(&mut attack_state).unwrap();
        assert!(!matches!(
            attack_next,
            PolicyDecisionV5::AttackerInclusion { .. }
        ));
        assert!(attack_state.engine.combat.attackers_declared);
        assert!(attack_state.engine.combat.attackers.is_empty());

        let (mut block_state, _, _) = blocker_state(0);
        let mut block_surface = PolicySurfaceV5::new();
        let block_next = block_surface.next_decision(&mut block_state).unwrap();
        assert!(!matches!(
            block_next,
            PolicyDecisionV5::BlockerInclusion { .. }
        ));
        assert!(block_state.engine.combat.blockers_declared);
        assert!(block_state.engine.combat.blocked_by.is_empty());
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
        state.engine.combat.attackers = attackers;
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
        assert!(state.engine.combat.blockers_declared);
        assert_eq!(state.engine.combat.blocked_by[0].1, vec![blocker]);
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

    #[test]
    fn external_state_staleness_is_sanitized_nonmutating_and_retryable_after_restore() {
        let (mut state, _) = attacker_state(2);
        let mut surface = PolicySurfaceV5::new();
        let decision = surface.next_decision(&mut state).unwrap();
        let actions = legal_action_candidates_v5(&decision, &state).unwrap();
        let pristine_state = state.clone();
        let pristine_surface = surface.clone();

        state.players[1].life -= 1;
        let tampered_state = state.clone();
        let err = surface
            .apply(&mut state, actions[1].policy_action.clone())
            .unwrap_err();
        assert_eq!(err, "stale policy combat scan environment binding");
        assert!(!err.contains("0x"));
        assert_eq!(state, tampered_state);
        assert_eq!(surface.scan, pristine_surface.scan);

        state = pristine_state;
        assert_eq!(surface.next_decision(&mut state).unwrap(), decision);
        surface
            .apply(&mut state, actions[1].policy_action.clone())
            .unwrap();
        assert!(surface.scan_active());
    }
}
