//! Offline bounded combat diagnostic. No live action selection or training labels.
//!
//! Bounds are root-player terminal returns under the explored public continuation.
//! Unexplored choices retain [-1, 1]; they never silently become draws or losses.
//! The information checks are conservative engineering boundaries, not a general
//! theorem that arbitrary engine effects cannot consult hidden information.
use super::*;
use crate::state::{GameState, Step};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
struct Bounds {
    lower: i8,
    upper: i8,
}
impl Bounds {
    const UNKNOWN: Self = Self {
        lower: -1,
        upper: 1,
    };
    fn terminal(value: i8) -> Self {
        Self {
            lower: value,
            upper: value,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize)]
struct Tree {
    bounds: Bounds,
    reason: &'static str,
    actor: Option<crate::rl::PlayerSeatV1>,
    branches: Vec<(u32, Tree)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    terminal: Option<Value>,
}
impl Tree {
    fn unknown(reason: &'static str) -> Self {
        Self {
            bounds: Bounds::UNKNOWN,
            reason,
            actor: None,
            branches: Vec::new(),
            terminal: None,
        }
    }
}

fn combat(action: &ActionSemanticV1) -> bool {
    matches!(
        action,
        ActionSemanticV1::DeclareAttackers { .. }
            | ActionSemanticV1::DeclareBlockersForAttacker { .. }
            | ActionSemanticV1::ChooseAttackerInclusion { .. }
            | ActionSemanticV1::ChooseBlockerInclusion { .. }
    )
}

fn same_information(initial: &GameState, after: &GameState, actor: usize) -> bool {
    initial.turn == after.turn
        && initial.library_knowledge == after.library_knowledge
        && after.players[1 - actor].hand.is_empty()
        && (0..2).all(|i| {
            initial.players[i].library == after.players[i].library
                && initial.players[i].draws_this_turn == after.players[i].draws_this_turn
        })
}

fn explore(
    session: &FastActorSessionV1,
    initial: &GameState,
    root_actor: crate::rl::PlayerSeatV1,
    depth: u32,
    remaining: &mut u32,
) -> Result<Tree, String> {
    if !same_information(initial, session.game_state(), seat(root_actor) as usize) {
        return Ok(Tree::unknown("information_boundary"));
    }
    let decision = match session.current_response() {
        FastActorResponseV1::Terminal(t) => {
            if t.terminal_classification != TerminalClassificationV1::Natural {
                return Ok(Tree::unknown("non_natural_terminal"));
            }
            let value = if t.winner == Some(root_actor) {
                1
            } else if t.winner.is_some() {
                -1
            } else {
                0
            };
            return Ok(Tree {
                bounds: Bounds::terminal(value),
                reason: "natural_terminal",
                actor: None,
                branches: Vec::new(),
                terminal: Some(json!(t)),
            });
        }
        FastActorResponseV1::Decision(d) => d,
    };
    if !matches!(
        session.game_state().step,
        Step::DeclareAttackers | Step::DeclareBlockers | Step::CombatDamage | Step::EndCombat
    ) {
        return Ok(Tree::unknown("combat_window_finished"));
    }
    if depth == 0 {
        return Ok(Tree::unknown("depth_limit"));
    }
    let (_, actions) = PairedBo1PolicyInputV1::new(session, decision).diagnostic_visible_v4()?;
    ensure(
        actions.len() == decision.legal_action_count as usize && !actions.is_empty(),
        "combat menu differs",
    )?;
    if actions.len() > 32 {
        return Ok(Tree::unknown("menu_limit"));
    }
    let mut branches = Vec::new();
    for (index, action) in actions.iter().enumerate() {
        let child = if !combat(action) && !matches!(action, ActionSemanticV1::Pass { .. }) {
            Tree::unknown("unsupported_action")
        } else if *remaining == 0 {
            Tree::unknown("node_limit")
        } else {
            *remaining -= 1;
            let mut branch = session.clone();
            branch
                .step(decision.episode_id, decision.step, index as u32)
                .map_err(err)?;
            explore(&branch, initial, root_actor, depth - 1, remaining)?
        };
        branches.push((index as u32, child));
    }
    let maximizing = decision.acting_player == root_actor;
    let lower = branches.iter().map(|(_, t)| t.bounds.lower);
    let upper = branches.iter().map(|(_, t)| t.bounds.upper);
    let bounds = Bounds {
        lower: if maximizing {
            lower.max().unwrap()
        } else {
            lower.min().unwrap()
        },
        upper: if maximizing {
            upper.max().unwrap()
        } else {
            upper.min().unwrap()
        },
    };
    Ok(Tree {
        bounds,
        reason: if maximizing {
            "own_choice"
        } else {
            "opponent_choice"
        },
        actor: Some(decision.acting_player),
        branches,
        terminal: None,
    })
}

/// A shadow analysis only. The live source is borrowed and never advanced.
/// No policy logits, value predictions or actual hidden-card identities select branches.
pub(crate) fn audit_public_combat_v1(
    session: &FastActorSessionV1,
    root: crate::rl_session::FastActorDecisionV1,
    depth: u32,
    nodes_per_action: u32,
) -> Result<Value, String> {
    ensure(
        session.current_response() == FastActorResponseV1::Decision(root),
        "combat root binding differs",
    )?;
    ensure(
        (1..=64).contains(&depth) && (1..=2048).contains(&nodes_per_action),
        "combat diagnostic budget outside bounds",
    )?;
    if !(2..=32).contains(&root.legal_action_count) {
        return Ok(json!({"status":"menu_outside_bounds"}));
    }
    let (visible, actions) = PairedBo1PolicyInputV1::new(session, root).diagnostic_visible_v4()?;
    if !actions.iter().all(combat) {
        return Ok(json!({"status":"not_combat_menu"}));
    }
    let initial = session.game_state();
    let actor = seat(root.acting_player) as usize;
    if !initial.players[1 - actor].hand.is_empty() {
        return Ok(json!({"status":"opponent_hand_nonempty"}));
    }
    if !initial.stack.is_empty() {
        return Ok(json!({"status":"stack_nonempty"}));
    }
    if !matches!(initial.step, Step::DeclareAttackers | Step::DeclareBlockers) {
        return Ok(json!({"status":"outside_combat_declaration"}));
    }
    let mut results = Vec::new();
    for index in 0..root.legal_action_count {
        let mut branch = session.clone();
        branch
            .step(root.episode_id, root.step, index)
            .map_err(err)?;
        let mut remaining = nodes_per_action - 1;
        let tree = explore(
            &branch,
            initial,
            root.acting_player,
            depth - 1,
            &mut remaining,
        )?;
        results.push(json!({"index":index,"transitions":nodes_per_action-remaining,"tree":tree}));
    }
    ensure(
        session.current_response() == FastActorResponseV1::Decision(root),
        "combat diagnostic mutated root",
    )?;
    Ok(
        json!({"schema":"public-combat-shadow/v1","status":"audited","visible":visible,"actions":actions,
        "depth":depth,"nodes_per_action":nodes_per_action,"outcomes":results,
        "non_claim":"Bounded offline combat diagnostic, not a deployed policy or validated natural-game training dataset. Unknown branches retain both possible terminal extremes."}),
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::card_def::card_id_by_name;
    use crate::engine::{self, Decision};
    use crate::state::{Counters, GameObject, ObjectStateV4, Zone};

    pub(crate) fn put(
        state: &mut GameState,
        owner: PlayerId,
        name: &str,
        zone: Zone,
    ) -> crate::ids::ObjectId {
        let card_def =
            card_id_by_name(name).unwrap_or_else(|| panic!("missing fixture card {name}"));
        let id = state.objects.push(GameObject {
            card_def,
            name: name.into(),
            owner,
            controller: owner,
            zone,
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
        match zone {
            Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
            Zone::Library => state.players[owner.index()].library.push(id),
            Zone::Hand => state.players[owner.index()].hand.push(id),
            _ => unreachable!(),
        }
        id
    }

    pub(crate) fn position(
        actor: u8,
        blocker: bool,
        hidden_variant: bool,
        hidden_hand: bool,
    ) -> FastActorSessionV1 {
        position_with(
            actor,
            blocker,
            hidden_variant,
            hidden_hand,
            "Myr Enforcer",
            1,
            1,
        )
    }

    fn position_with(
        actor: u8,
        blocker: bool,
        hidden_variant: bool,
        hidden_hand: bool,
        attacker_name: &str,
        opponent_life: i32,
        attacker_count: usize,
    ) -> FastActorSessionV1 {
        let actor = PlayerId(actor);
        let opponent = PlayerId(1 - actor.0);
        let mut state = GameState::new_from_libraries(&[], &[], crate::rl::card_name, 91_001);
        state.step = Step::DeclareAttackers;
        state.active_player = actor;
        state.priority_player = actor;
        state.players[opponent.index()].life = opponent_life;
        for _ in 0..attacker_count {
            put(&mut state, actor, attacker_name, Zone::Battlefield);
        }
        if blocker {
            put(&mut state, opponent, "Sacred Cat", Zone::Battlefield);
        }
        for owner in [PlayerId::P0, PlayerId::P1] {
            for name in if hidden_variant {
                ["Mountain", "Forest", "Swamp"]
            } else {
                ["Island", "Forest", "Mountain"]
            } {
                put(&mut state, owner, name, Zone::Library);
            }
            if hidden_variant {
                state.players[owner.index()].library.reverse();
            }
        }
        if hidden_hand {
            put(&mut state, opponent, "Island", Zone::Hand);
        }
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::DeclareAttackers { .. }
        ));
        FastActorSessionV1::from_public_terminal_fixture_v1(state)
    }

    fn root(s: &FastActorSessionV1) -> crate::rl_session::FastActorDecisionV1 {
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            panic!("no combat root")
        };
        d
    }

    #[test]
    fn public_combat_terminal_attack_replays_and_ignores_unknown_library_cards() {
        for actor in 0..2 {
            let mut first = None;
            for hidden in [false, true] {
                let s = position(actor, false, hidden, false);
                let d = root(&s);
                let before = PairedBo1PolicyInputV1::new(&s, d)
                    .diagnostic_visible_v4()
                    .unwrap();
                let a = audit_public_combat_v1(&s, d, 32, 512).unwrap();
                assert_eq!(a["status"], "audited");
                let outcomes = a["outcomes"].as_array().unwrap();
                assert!(
                    outcomes.iter().any(|o| o["tree"]["bounds"]["lower"] == 1),
                    "{a}"
                );
                assert!(
                    outcomes
                        .iter()
                        .any(|o| o["tree"]["bounds"]["lower"] == -1
                            && o["tree"]["bounds"]["upper"] == 1),
                    "{a}"
                );
                assert_eq!(a, audit_public_combat_v1(&s, d, 32, 512).unwrap());
                assert_eq!(
                    before,
                    PairedBo1PolicyInputV1::new(&s, d)
                        .diagnostic_visible_v4()
                        .unwrap()
                );
                if let Some(prior) = &first {
                    assert_eq!(prior, &a);
                } else {
                    first = Some(a);
                }
            }
        }
    }

    #[test]
    fn public_combat_opponent_block_prevents_cooperative_win_certificate() {
        for actor in 0..2 {
            let s = position(actor, true, false, false);
            let a = audit_public_combat_v1(&s, root(&s), 32, 512).unwrap();
            assert_eq!(a["status"], "audited");
            assert!(
                a["outcomes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|o| o["tree"]["bounds"]["lower"] != 1),
                "{a}"
            );
        }
    }

    #[test]
    fn public_combat_truncation_and_nonempty_hand_never_certify() {
        for actor in 0..2 {
            let s = position_with(actor, false, false, false, "Myr Enforcer", 1, 2);
            let a = audit_public_combat_v1(&s, root(&s), 1, 1).unwrap();
            assert!(
                a["outcomes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(
                        |o| o["tree"]["bounds"]["lower"] == -1 && o["tree"]["bounds"]["upper"] == 1
                    ),
                "{a}"
            );
            let hidden = position(actor, false, false, true);
            assert_eq!(
                audit_public_combat_v1(&hidden, root(&hidden), 32, 512).unwrap()["status"],
                "opponent_hand_nonempty"
            );
        }
    }

    #[test]
    fn public_combat_defensive_root_distinguishes_certain_loss_from_unresolved() {
        for actor in 0..2 {
            let own = PlayerId(actor);
            let opponent = PlayerId(1 - actor);
            let mut state = GameState::new_from_libraries(&[], &[], crate::rl::card_name, 91_002);
            state.step = Step::DeclareBlockers;
            state.active_player = opponent;
            state.priority_player = own;
            state.players[own.index()].life = 1;
            let attacker = put(&mut state, opponent, "Myr Enforcer", Zone::Battlefield);
            put(&mut state, own, "Myr Enforcer", Zone::Battlefield);
            state.objects.get_mut(attacker).tapped = true;
            state.engine.combat.attackers_declared = true;
            state.engine.combat.attackers = vec![attacker];
            for owner in [own, opponent] {
                for name in ["Forest", "Island", "Mountain"] {
                    put(&mut state, owner, name, Zone::Library);
                }
            }
            let s = FastActorSessionV1::from_public_terminal_fixture_v1(state);
            let a = audit_public_combat_v1(&s, root(&s), 32, 512).unwrap();
            assert_eq!(a["status"], "audited", "{a}");
            let outcomes = a["outcomes"].as_array().unwrap();
            assert!(
                outcomes
                    .iter()
                    .any(|o| o["tree"]["bounds"]["lower"] == -1
                        && o["tree"]["bounds"]["upper"] == -1),
                "{a}"
            );
            assert!(
                outcomes.iter().any(
                    |o| o["tree"]["bounds"]["lower"] == -1 && o["tree"]["bounds"]["upper"] == 1
                ),
                "{a}"
            );
        }
    }

    #[test]
    fn public_combat_draw_trigger_abstains_and_actual_draw_crosses_boundary() {
        for actor in 0..2 {
            for hidden in [false, true] {
                let s = position_with(
                    actor,
                    false,
                    hidden,
                    false,
                    "Ninja of the Deep Hours",
                    20,
                    1,
                );
                let d = root(&s);
                let a = audit_public_combat_v1(&s, d, 32, 512).unwrap();
                assert!(a.to_string().contains("unsupported_action"), "{a}");
                assert!(
                    a["outcomes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|o| o["tree"]["bounds"]["lower"] == -1
                            && o["tree"]["bounds"]["upper"] == 1),
                    "{a}"
                );
                // Explicit test-only traversal of the unsupported optional draw.
                // Production diagnostic must stop before this decision.
                let mut branch = s.clone();
                let mut drew = false;
                for _ in 0..64 {
                    let decision = root(&branch);
                    let (_, actions) = PairedBo1PolicyInputV1::new(&branch, decision)
                        .diagnostic_visible_v4()
                        .unwrap();
                    let draw = actions.iter().position(|a| {
                        matches!(
                            a,
                            ActionSemanticV1::ChooseEffectOption {
                                option_index: 1,
                                ..
                            }
                        )
                    });
                    let selected = draw
                        .or_else(|| {
                            actions.iter().position(|a| {
                                matches!(
                                    a,
                                    ActionSemanticV1::ChooseAttackerInclusion { include: true, .. }
                                        | ActionSemanticV1::Pass { .. }
                                )
                            })
                        })
                        .expect("declared draw control path");
                    branch
                        .step(decision.episode_id, decision.step, selected as u32)
                        .unwrap();
                    if draw.is_some() {
                        drew = true;
                        break;
                    }
                }
                assert!(drew);
                assert_eq!(
                    branch.game_state().players[actor as usize].library.len() + 1,
                    s.game_state().players[actor as usize].library.len()
                );
                let tree = explore(&branch, s.game_state(), d.acting_player, 32, &mut 512).unwrap();
                assert_eq!(tree.reason, "information_boundary");
                assert_eq!(tree.bounds, Bounds::UNKNOWN);
            }
        }
    }
}
