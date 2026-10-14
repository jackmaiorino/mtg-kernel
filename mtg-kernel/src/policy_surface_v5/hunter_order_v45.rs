//! Private opt-in permutation scan. Partial answers never enter the engine.
use super::*;
use crate::trigger::PendingTrigger;
pub(crate) type HunterOrderPartsV45<'a> = (PlayerId, &'a [PendingTrigger], &'a [usize]);

#[derive(Clone)]
pub(super) struct TriggerOrderV45 {
    pub(super) player: PlayerId,
    pub(super) pending: Vec<PendingTrigger>,
    pub(super) prefix: Vec<usize>,
    pub(super) binding: CombatScanBindingV5,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerOrderContextV45 {
    pub controller: crate::rl::PlayerSeatV1,
    pub ordered_prefix: Vec<usize>,
    pub pending_count: usize,
}
impl TriggerOrderV45 {
    fn validate(
        &self,
        state: &GameState,
        inner: &HarnessSurfaceV2,
        mode: EnvironmentBindingModeV5,
    ) -> Result<(), String> {
        match (self.binding, mode) {
            (CombatScanBindingV5::ExactSurfaceHash(saved), EnvironmentBindingModeV5::Exact)
                if saved == surface_binding_hash(state, inner)? => {}
            (
                CombatScanBindingV5::OwnedEnvironmentRevision(saved),
                EnvironmentBindingModeV5::OwnedRevision(current),
            ) if saved == current => {}
            _ => return Err("stale Hunter ordering environment".into()),
        }
        if self.pending.len() <= crate::rl::MAX_TRIGGER_ORDER_OBJECTS
            || self.prefix.len() >= self.pending.len()
            || self.pending.iter().any(|p| p.controller != self.player)
        {
            return Err("invalid Hunter ordering group shape".into());
        }
        let mut seen = vec![false; self.pending.len()];
        for &index in &self.prefix {
            let Some(selected) = seen.get_mut(index) else {
                return Err("Hunter ordering prefix out of range".into());
            };
            if *selected {
                return Err("Hunter ordering prefix repeats an occurrence".into());
            }
            *selected = true;
        }
        let mut probe = state.clone();
        let actual = crate::engine::advance_until_decision(&mut probe);
        if probe != *state
            || actual
                != (Decision::OrderTriggers {
                    player: self.player,
                    pending: self.pending.clone(),
                })
        {
            return Err("Hunter ordering no longer owns the complete actual group".into());
        }
        Ok(())
    }
}
impl PolicySurfaceV5 {
    pub(crate) fn enable_hunter_order_v45(&mut self) {
        self.hunter_order_enabled_v45 = true;
    }
    pub(crate) fn initialize_hunter_order_v45(
        &mut self,
        state: &GameState,
        origin: &PolicyDecisionV5,
        revision: u64,
    ) -> Result<(), String> {
        if !self.hunter_order_enabled_v45 {
            return Ok(());
        }
        if let Some(order) = &self.hunter_order_v45 {
            if self.scan.is_some() {
                return Err("Hunter ordering overlaps a combat scan".into());
            }
            order.validate(
                state,
                &self.inner,
                EnvironmentBindingModeV5::OwnedRevision(revision),
            )?;
            if *origin
                != PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::OrderTriggers {
                    player: order.player,
                    pending: order.pending.clone(),
                }))
            {
                return Err("Hunter ordering origin changed".into());
            }
            return Ok(());
        }
        if let PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::OrderTriggers {
            player,
            pending,
        })) = origin
        {
            if pending.len() > crate::rl::MAX_TRIGGER_ORDER_OBJECTS {
                if self.scan.is_some() {
                    return Err("Hunter ordering cannot begin inside combat selection".into());
                }
                let order = TriggerOrderV45 {
                    player: *player,
                    pending: pending.clone(),
                    prefix: Vec::new(),
                    binding: CombatScanBindingV5::OwnedEnvironmentRevision(revision),
                };
                order.validate(
                    state,
                    &self.inner,
                    EnvironmentBindingModeV5::OwnedRevision(revision),
                )?;
                self.hunter_order_v45 = Some(order);
            }
        }
        Ok(())
    }
    pub(crate) fn hunter_order_parts_v45(
        &self,
        state: &GameState,
    ) -> Result<Option<HunterOrderPartsV45<'_>>, String> {
        let Some(order) = &self.hunter_order_v45 else {
            return Ok(None);
        };
        if !self.hunter_order_enabled_v45 || self.scan.is_some() {
            return Err("Hunter ordering lost its exclusive opted-in surface".into());
        }
        // Read-only producers authenticate the full group; the owning backend
        // separately checks the revision against its current header.
        let mode = match order.binding {
            CombatScanBindingV5::OwnedEnvironmentRevision(r) => {
                EnvironmentBindingModeV5::OwnedRevision(r)
            }
            CombatScanBindingV5::ExactSurfaceHash(_) => EnvironmentBindingModeV5::Exact,
        };
        order.validate(state, &self.inner, mode)?;
        Ok(Some((order.player, &order.pending, &order.prefix)))
    }
    pub fn hunter_order_context_v45(
        &self,
        state: &GameState,
    ) -> Result<Option<TriggerOrderContextV45>, String> {
        self.hunter_order_parts_v45(state).map(|parts| {
            parts.map(|(player, pending, prefix)| TriggerOrderContextV45 {
                controller: player.into(),
                ordered_prefix: prefix.to_vec(),
                pending_count: pending.len(),
            })
        })
    }
    pub(crate) fn hunter_order_substeps_v45(
        &self,
        state: &GameState,
    ) -> Result<Option<(u32, u32)>, String> {
        self.hunter_order_parts_v45(state)?
            .map(|(_, pending, prefix)| {
                Ok((
                    u32::try_from(prefix.len())
                        .map_err(|_| "Hunter ordering prefix exceeds u32")?,
                    u32::try_from(pending.len())
                        .map_err(|_| "Hunter ordering group exceeds u32")?,
                ))
            })
            .transpose()
    }
    pub(crate) fn validate_hunter_order_header_v45(
        &self,
        state: &GameState,
        revision: u64,
        origin: &PolicyDecisionV5,
        substeps: (u32, u32),
    ) -> Result<bool, String> {
        let Some(order) = &self.hunter_order_v45 else {
            return Ok(false);
        };
        order.validate(
            state,
            &self.inner,
            EnvironmentBindingModeV5::OwnedRevision(revision),
        )?;
        if *origin
            != PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::OrderTriggers {
                player: order.player,
                pending: order.pending.clone(),
            }))
            || self.hunter_order_substeps_v45(state)? != Some(substeps)
        {
            return Err("Hunter ordering header does not bind its private prefix".into());
        }
        Ok(true)
    }
    pub(super) fn validate_hunter_order_action_v45(
        &self,
        state: &GameState,
        action: &PolicyActionV5,
        mode: EnvironmentBindingModeV5,
    ) -> Result<(), String> {
        let order = self
            .hunter_order_v45
            .as_ref()
            .ok_or("no Hunter ordering scan")?;
        order.validate(state, &self.inner, mode)?;
        let PolicyActionV5::Surface(SurfaceAction::Action(Action::OrderTriggers(next))) = action
        else {
            return Err("Hunter ordering requires one next occurrence".into());
        };
        if next.len() != order.prefix.len() + 1
            || !next.starts_with(&order.prefix)
            || next
                .last()
                .is_none_or(|index| *index >= order.pending.len() || order.prefix.contains(index))
        {
            return Err("stale or malformed Hunter next ordering occurrence".into());
        }
        Ok(())
    }
    pub(super) fn apply_hunter_order_v45(
        &mut self,
        state: &mut GameState,
        action: PolicyActionV5,
        mode: EnvironmentBindingModeV5,
        next_revision: Option<u64>,
    ) -> Result<(), String> {
        self.validate_hunter_order_action_v45(state, &action, mode)?;
        let PolicyActionV5::Surface(SurfaceAction::Action(Action::OrderTriggers(next))) = action
        else {
            unreachable!("validated ordering action")
        };
        let order = self
            .hunter_order_v45
            .as_mut()
            .ok_or("Hunter ordering absent")?;
        if next.len() < order.pending.len() {
            order.prefix = next;
            if let Some(revision) = next_revision {
                order.binding = CombatScanBindingV5::OwnedEnvironmentRevision(revision);
            }
            return Ok(());
        }
        // The first and only engine action contains the complete permutation.
        self.inner
            .apply(state, SurfaceAction::Action(Action::OrderTriggers(next)))?;
        self.hunter_order_v45 = None;
        Ok(())
    }
}
