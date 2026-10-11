//! Immediate multi-color mana bundles. No saved state or historical capture.
use crate::{
    engine,
    ids::PlayerId,
    mana::ManaColor,
    state::{GameState, Zone},
};

pub(crate) fn colors(state: &GameState, player: PlayerId) -> Vec<ManaColor> {
    let mask = state
        .objects
        .iter()
        .filter(|(_, object)| object.zone == Zone::Battlefield && object.controller == player)
        .fold(0, |mask, (id, _)| {
            mask | engine::object_color_mask(state, id)
        });
    [
        ManaColor::W,
        ManaColor::U,
        ManaColor::B,
        ManaColor::R,
        ManaColor::G,
    ]
    .into_iter()
    .filter(|color| mask & (1 << color.pool_index()) != 0)
    .collect()
}

pub(crate) fn fits(state: &GameState, player: PlayerId) -> bool {
    colors(state, player)
        .iter()
        .all(|color| state.players[player.index()].mana_pool[color.pool_index()] < u8::MAX)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::policy_observation_v6::tests::{put, ready_state};

    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn actual_source(player: PlayerId) -> (GameState, crate::ids::ObjectId) {
        use crate::engine::{Action, Decision};
        let mut state = ready_state();
        state.active_player = player;
        state.priority_player = player;
        state.step = crate::state::Step::Main1;
        for seat in [PlayerId::P0, PlayerId::P1] {
            for _ in 0..20 {
                put(&mut state, seat, "Forest", Zone::Library);
            }
        }
        let source = put(&mut state, player, "Bloom Tender", Zone::Hand);
        state.players[player.index()].mana_pool = [0, 0, 0, 0, 1, 1];
        engine::advance_until_decision(&mut state);
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        engine::advance_until_decision(&mut state);
        assert!(state.stack.iter().any(|item| item.source == source));
        for _ in 0..160 {
            let decision = engine::advance_until_decision(&mut state);
            if state.objects.get(source).zone == Zone::Battlefield
                && !state.objects.get(source).summoning_sick
                && state.step == crate::state::Step::Main1
                && matches!(decision,Decision::CastSpellOrPass{player:actor,..} if actor==player)
            {
                return (state, source);
            }
            let action = match decision {
                Decision::CastSpellOrPass { .. } => Action::Pass,
                Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
                Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
                Decision::OrderTriggers { pending, .. } => {
                    Action::OrderTriggers((0..pending.len()).collect())
                }
                other => panic!("actual Bloom preparation {other:?}"),
            };
            engine::step(&mut state, action).unwrap();
        }
        panic!("actual Bloom cast/maturation did not complete");
    }

    #[test]
    fn bundle_reads_current_control_and_effective_permanent_colors_only() {
        for player in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state();
            let green = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
            let red = put(
                &mut state,
                player.opponent(),
                "Guttersnipe",
                Zone::Battlefield,
            );
            put(&mut state, player, "Counterspell", Zone::Hand);
            put(&mut state, player, "Forest", Zone::Battlefield);
            assert_eq!(colors(&state, player), vec![ManaColor::G]);
            // A typed physical color/control operand, not an invented activation.
            state.objects.get_mut(red).controller = player;
            state.objects.get_mut(red).v4.effective_color_mask = 1 | 2 | 4 | 8;
            assert_eq!(
                colors(&state, player),
                vec![
                    ManaColor::W,
                    ManaColor::U,
                    ManaColor::B,
                    ManaColor::R,
                    ManaColor::G
                ]
            );
            state.players[player.index()].mana_pool[ManaColor::U.pool_index()] = 255;
            assert!(!fits(&state, player));
            state.objects.get_mut(red).zone = Zone::Graveyard;
            assert!(fits(&state, player));
            state.objects.get_mut(green).v4.effective_color_mask = 1 << ManaColor::C.pool_index();
            assert!(colors(&state, player).is_empty());
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(colors(&state, player), colors(&restored, player));
        }
    }
}
