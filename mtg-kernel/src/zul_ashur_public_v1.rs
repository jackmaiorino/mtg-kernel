//! Captured public identities are distinct from later live or hidden incarnations.
use crate::event::CommittedEvent;
use crate::rl::{CardStableRefV1, PlayerSeatV1, RlContractError};
use crate::state::{GameState, StackTargetContractV4};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionV1 {
    pub source_at_activation: CardStableRefV1,
    pub original_target: CardStableRefV1,
    pub holder: PlayerSeatV1,
    pub granted_this_turn: bool,
    pub original_card_in_graveyard: bool,
    pub expired: bool,
}
pub fn facts_v1(state: &GameState) -> Result<Vec<PermissionV1>, RlContractError> {
    crate::zul_ashur_v1::validate_state(state).map_err(RlContractError)?;
    let mut facts = Vec::new();
    for e in &state.engine.event_history {
        let CommittedEvent::ZulAshurInstalledV1 { installation: i } = e else {
            continue;
        };
        let Some(CommittedEvent::ZulAshurRemovedV1 { removal: r }) =
            state.engine.event_history.get(i.removal as usize)
        else {
            return Err(RlContractError(
                "Zul public installation lost original removal".into(),
            ));
        };
        let s = r
            .item
            .v4
            .ability_source_contract
            .ok_or_else(|| RlContractError("Zul original source absent".into()))?;
        let StackTargetContractV4::Object {
            object,
            card_def,
            owner,
            controller,
            zone,
            zone_change_count,
            ..
        } = i.target
        else {
            return Err(RlContractError(
                "Zul original graveyard target absent".into(),
            ));
        };
        let expired=state.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::ZulAshurExpiredV1{expiry} if expiry.installation==i.index));
        facts.push(PermissionV1 {
            source_at_activation: CardStableRefV1 {
                arena_id: s.source.0,
                card_db_id: s.card_def,
                owner: s.owner.into(),
                controller: s.controller.into(),
                zone: s.zone,
                zone_change_count: s.zone_change_count,
            },
            original_target: CardStableRefV1 {
                arena_id: object.0,
                card_db_id: card_def,
                owner: owner.into(),
                controller: controller.into(),
                zone,
                zone_change_count,
            },
            holder: i.holder.into(),
            granted_this_turn: !expired
                && i.turn == state.turn_key_v1()
                && i.active_player == state.active_player,
            original_card_in_graveyard: state.objects.try_get(object).is_some_and(|o| {
                o.zone == zone
                    && o.zone_change_count == zone_change_count
                    && o.owner == owner
                    && o.card_def == card_def
            }),
            expired,
        });
    }
    Ok(facts)
}
