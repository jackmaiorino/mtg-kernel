//! Live definition-owned combat statics. No saved state or historical aliases.

use crate::card_def::{CardType, CombatStaticRulesDef, CARD_DEFS};
use crate::ids::ObjectId;
use crate::state::{GameState, Zone};

pub(crate) fn active(state: &GameState, source: ObjectId) -> CombatStaticRulesDef {
    let Some(object) = state.objects.try_get(source) else {
        return CombatStaticRulesDef::NONE;
    };
    let Some(def) = CARD_DEFS.get(usize::from(object.card_def)) else {
        return CombatStaticRulesDef::NONE;
    };
    if def.combat_static_rules == CombatStaticRulesDef::NONE {
        return CombatStaticRulesDef::NONE;
    }
    if object.zone != Zone::Battlefield
        || !state.players[object.controller.index()]
            .battlefield
            .contains(&source)
        || object.v4.face_index != 0
        || !def.is_executable()
        || !crate::engine::object_has_type(state, source, CardType::Creature)
        || !crate::continuous_characteristics_v1::printed_abilities_active(state, source)
    {
        return CombatStaticRulesDef::NONE;
    }
    def.combat_static_rules
}

pub(crate) fn requires_successor(state: &GameState) -> bool {
    state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .any(|source| active(state, *source) != CombatStaticRulesDef::NONE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::{put, ready_state};

    #[test]
    fn registered_absence_query_is_readonly_and_preserves_restored_state() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state();
            state.active_player = actor;
            let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let before = serde_json::to_vec(&state).unwrap();
            let hash = state.diagnostic_state_hash();
            assert_eq!(active(&state, source), CombatStaticRulesDef::NONE);
            assert!(!requires_successor(&state));
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            assert_eq!(state.diagnostic_state_hash(), hash);
            let restored: GameState = serde_json::from_slice(&before).unwrap();
            assert_eq!(active(&restored, source), CombatStaticRulesDef::NONE);
            assert_eq!(restored.diagnostic_state_hash(), hash);
        }
    }
}
