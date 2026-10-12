//! Ordinary selected permission ownership. Legacy permissions emit no records.
use super::*;
use crate::event::CommittedEvent;
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PermissionInstallationV1 {
    pub index: u32,
    pub stack: StackItemId,
    pub chosen: EffectObjectBinding,
    pub permission: crate::engine::PlayPermission,
    pub granted_turn: u32,
}
fn installations(state: &GameState) -> Vec<&PermissionInstallationV1> {
    state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::StrongboxPermissionGrantedV1 { installation } => {
                Some(installation.as_ref())
            }
            _ => None,
        })
        .collect()
}
fn starts(state: &GameState, index: u32) -> Vec<(usize, u32, PlayerId)> {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, event)| match event {
            CommittedEvent::StrongboxPermissionTurnStartedV1 {
                installation,
                turn,
                holder,
            } if *installation == index => Some((at, *turn, *holder)),
            _ => None,
        })
        .collect()
}
fn expires(state: &GameState, index: u32) -> Vec<(usize, u32)> {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, event)| match event {
            CommittedEvent::StrongboxPermissionExpiredV1 {
                installation,
                cleanup,
            } if *installation == index => Some((at, *cleanup)),
            _ => None,
        })
        .collect()
}
pub(super) fn validate(state: &GameState) -> Result<(), String> {
    for (at, event) in state.engine.event_history.iter().enumerate() {
        match event {
            CommittedEvent::StrongboxPermissionGrantedV1 { installation: g } => {
                let bindings = state
                    .engine
                    .event_history
                    .iter()
                    .filter_map(|event| match event {
                        CommittedEvent::ExilePlaySelectionCapturedV1 {
                            resolving_stack_item,
                            binding,
                        } if *resolving_stack_item == g.stack => Some(binding.as_ref()),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let [binding] = bindings.as_slice() else {
                    return Err("Strongbox grant lacks unique original exile capture".into());
                };
                if g.index as usize != at
                    || g.granted_turn != binding.granted_turn
                    || !binding.exiled.contains(&g.chosen)
                    || g.permission.object != g.chosen.object
                    || g.permission.holder != binding.player
                    || g.permission.zone_change_generation != g.chosen.expected_zone_change_count
                    || g.permission.without_mana_cost.0
                    || g.permission.expiry
                        != (crate::engine::PlayPermissionExpiry::UntilHoldersNextTurn {
                            holder_turn_started: false,
                        })
                    || installations(state)
                        .iter()
                        .filter(|other| other.stack == g.stack)
                        .count()
                        != 1
                    || !matches!(state.engine.event_history.get(at.wrapping_sub(1)),Some(CommittedEvent::ExilePlaySelectionConsumedV1 {resolving_stack_item,selected:Some(selected)}) if *resolving_stack_item==g.stack && *selected==g.chosen)
                {
                    return Err("Strongbox grant changed selection/controller/incarnation/ordinary permission".into());
                }
                let started = starts(state, g.index);
                let expired = expires(state, g.index);
                if started.len() > 1
                    || expired.len() > 1
                    || started.iter().any(|(at, turn, holder)| {
                        *at <= g.index as usize
                            || *turn == g.granted_turn
                            || *holder != g.permission.holder
                    })
                {
                    return Err("Strongbox holder-next-turn occurrence changed".into());
                }
                let mut expected = g.permission.clone();
                if !started.is_empty() {
                    expected.expiry = crate::engine::PlayPermissionExpiry::UntilHoldersNextTurn {
                        holder_turn_started: true,
                    };
                }
                let live = state
                    .engine
                    .exile_play_permissions
                    .iter()
                    .filter(|permission| {
                        permission.object == g.chosen.object
                            && permission.holder == g.permission.holder
                            && permission.zone_change_generation
                                == g.chosen.expected_zone_change_count
                    })
                    .collect::<Vec<_>>();
                if expired.is_empty() {
                    if !matches!(live.as_slice(),[p] if **p==expected) {
                        return Err(
                            "Strongbox permission erased/changed/duplicated before expiry".into(),
                        );
                    }
                } else if !live.is_empty() {
                    return Err("Strongbox expired permission resurrected".into());
                }
            }
            CommittedEvent::StrongboxPermissionTurnStartedV1 { installation, .. } => {
                if !installations(state)
                    .iter()
                    .any(|g| g.index == *installation)
                {
                    return Err("orphan Strongbox holder turn".into());
                }
            }
            CommittedEvent::StrongboxPermissionCleanupV1 {
                turn,
                holder,
                installations: ids,
            } => {
                if ids.is_empty() || ids.iter().enumerate().any(|(i, id)| ids[..i].contains(id)) {
                    return Err("Strongbox Cleanup roster invalid".into());
                }
                for (offset, id) in ids.iter().enumerate() {
                    let grants = installations(state);
                    let g = grants
                        .into_iter()
                        .find(|g| g.index == *id)
                        .ok_or("Strongbox Cleanup lost grant")?;
                    let started = starts(state, *id);
                    if !matches!(started.as_slice(),[(before,begin,actor)] if *before<at && *begin==*turn && *actor==*holder && *holder==g.permission.holder)
                        || !matches!(state.engine.event_history.get(at+1+offset),Some(CommittedEvent::StrongboxPermissionExpiredV1 {installation,cleanup}) if installation==id && *cleanup as usize==at)
                        || expires(state, *id).len() != 1
                    {
                        return Err(
                            "Strongbox expiry lacks complete holder Cleanup occurrence".into()
                        );
                    }
                }
            }
            CommittedEvent::StrongboxPermissionExpiredV1 {
                installation,
                cleanup,
            } => {
                let Some(CommittedEvent::StrongboxPermissionCleanupV1 { installations, .. }) =
                    state.engine.event_history.get(*cleanup as usize)
                else {
                    return Err("Strongbox early expiry lacks Cleanup authority".into());
                };
                if *cleanup as usize >= at || !installations.contains(installation) {
                    return Err("Strongbox expiry not in Cleanup roster".into());
                }
            }
            _ => {}
        }
    }
    Ok(())
}
pub(crate) fn untap(state: &mut GameState) -> Result<(), String> {
    validate(state)?;
    let holder = state.active_player;
    let turn = state.turn_key_v1();
    let grants = installations(state)
        .iter()
        .filter(|g| {
            g.permission.holder == holder
                && starts(state, g.index).is_empty()
                && expires(state, g.index).is_empty()
        })
        .map(|g| g.index)
        .collect::<Vec<_>>();
    for installation in grants {
        state
            .engine
            .event_history
            .push(CommittedEvent::StrongboxPermissionTurnStartedV1 {
                installation,
                turn,
                holder,
            });
    }
    Ok(())
}
pub(crate) fn cleanup(state: &mut GameState) -> Result<(), String> {
    validate(state)?;
    let holder = state.active_player;
    let turn = state.turn_key_v1();
    let ids = installations(state)
        .iter()
        .filter(|g| {
            g.permission.holder == holder
                && !starts(state, g.index).is_empty()
                && expires(state, g.index).is_empty()
        })
        .map(|g| g.index)
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(());
    }
    let cleanup = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "Strongbox Cleanup overflow")?;
    state
        .engine
        .event_history
        .push(CommittedEvent::StrongboxPermissionCleanupV1 {
            turn,
            holder,
            installations: ids.clone(),
        });
    for installation in ids {
        state
            .engine
            .event_history
            .push(CommittedEvent::StrongboxPermissionExpiredV1 {
                installation,
                cleanup,
            });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_registered_permission_hooks_preserve_full_bytes_and_hash_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = crate::policy_observation_v6::tests::ready_state();
            state.active_player = actor;
            let before = serde_json::to_vec(&state).unwrap();
            let hash = state.diagnostic_state_hash();
            untap(&mut state).unwrap();
            cleanup(&mut state).unwrap();
            validate(&state).unwrap();
            assert_eq!(before, serde_json::to_vec(&state).unwrap());
            assert_eq!(hash, state.diagnostic_state_hash());
        }
    }
}
