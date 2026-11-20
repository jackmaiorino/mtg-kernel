//! Per-seat priority-window rules for training (priority windows S0).
//!
//! `HarnessSurfaceV2` in `HarnessV2` mode passes for a player at every
//! priority window in upkeep, draw, beginning of combat, combat damage, end
//! of combat and the end step, after the player's own fresh stack item, and
//! after a player's one combat action per round. A seat on an action-window
//! rule is handed such a window instead whenever it has something to do
//! there ([`offer_v1`]). The HarnessV2 bookkeeping still runs in full for
//! every seat, so the surface columns of the model input keep their trained
//! meaning, and a seat that passes at every such window plays today's game.
//!
//! [`SeatPriorityWindowsV1::ActionWindowsV1`] also replaces today's rule for
//! mana-only windows (a window whose only options besides Pass are mana
//! abilities): it is offered only when tapping could make something castable
//! ([`mana_enables_action_v1`]), in every step.
//!
//! The mana test searches the activations autopay does not make (the engine's
//! own autopay source list decides), each run by the engine on a cloned
//! state.
//!
//! Hand-blind: every test reads only the acting player's own legal options as
//! the rules engine lists them. Nothing here is a judgment about what is
//! worth casting.

use serde::{Deserialize, Serialize};

use crate::engine::{self, Action, Decision};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{GameState, Step};

/// One seat's priority-window rule. The episode field is absent for today's
/// surface; `HarnessV2` is the same thing spelled out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SeatPriorityWindowsV1 {
    /// Today's surface exactly.
    #[default]
    #[serde(rename = "harness-v2")]
    HarnessV2,
    /// A window wherever the seat has a real action, in every step and while
    /// holding priority, plus mana-only windows that pass the mana test.
    #[serde(rename = "action-windows-v1")]
    ActionWindowsV1,
    /// [`Self::ActionWindowsV1`]'s test, applied only in the end step: for
    /// the non-active player, or for either player while the stack is not
    /// empty. Every other window is HarnessV2's.
    #[serde(rename = "action-windows-end-step-v1")]
    ActionWindowsEndStepV1,
}

impl SeatPriorityWindowsV1 {
    pub const fn is_harness_v2(self) -> bool {
        matches!(self, Self::HarnessV2)
    }
}

/// The candidate lists of one raw `Decision::CastSpellOrPass`.
#[derive(Clone, Copy)]
pub struct PriorityOptionsV1<'a> {
    pub player: PlayerId,
    pub castable_spells: &'a [ObjectId],
    pub mana_abilities: &'a [ObjectId],
    pub land_drops: &'a [ObjectId],
    pub activatable_abilities: &'a [(ObjectId, u8)],
    pub plot_actions: &'a [ObjectId],
}

impl PriorityOptionsV1<'_> {
    /// A spell, a non-mana activated ability, a land drop or a plot.
    pub fn has_real_action(&self) -> bool {
        !self.castable_spells.is_empty()
            || !self.activatable_abilities.is_empty()
            || !self.land_drops.is_empty()
            || !self.plot_actions.is_empty()
    }

    pub fn is_mana_only(&self) -> bool {
        !self.has_real_action() && !self.mana_abilities.is_empty()
    }
}

/// Simulated activations allowed per window. Reaching it offers the window.
pub const MANA_TEST_ACTIVATION_CAP_V1: u32 = 64;

/// The mana test's outcome, with its cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ManaTestV1 {
    pub enables: bool,
    pub cap_reached: bool,
    pub activations: u32,
    /// Hand-only sources considered at the root.
    pub hand_only_sources: u32,
}

/// Whether the HarnessV2 window that `rule` would otherwise pass for
/// `options.player` is handed to the seat. `state` is the live state at the
/// pending decision; it is only read.
pub fn offer_v1(
    rule: SeatPriorityWindowsV1,
    state: &GameState,
    options: &PriorityOptionsV1,
) -> bool {
    match rule {
        SeatPriorityWindowsV1::HarnessV2 => false,
        SeatPriorityWindowsV1::ActionWindowsV1 => has_action_or_enabling_mana_v1(state, options),
        SeatPriorityWindowsV1::ActionWindowsEndStepV1 => {
            end_step_scope_v1(state, options.player)
                && has_action_or_enabling_mana_v1(state, options)
        }
    }
}

/// Whether `rule` takes back a window HarnessV2 would surface: an
/// `ActionWindowsV1` seat's mana-only window that fails the mana test.
pub fn withhold_v1(
    rule: SeatPriorityWindowsV1,
    state: &GameState,
    options: &PriorityOptionsV1,
) -> bool {
    rule == SeatPriorityWindowsV1::ActionWindowsV1
        && options.is_mana_only()
        && !mana_enables_action_v1(state, options.player, options.mana_abilities, false).enables
}

/// [`SeatPriorityWindowsV1::ActionWindowsEndStepV1`]'s scope.
pub fn end_step_scope_v1(state: &GameState, player: PlayerId) -> bool {
    state.step == Step::End && (player != state.active_player || !state.stack.is_empty())
}

fn has_action_or_enabling_mana_v1(state: &GameState, options: &PriorityOptionsV1) -> bool {
    options.has_real_action()
        || (!options.mana_abilities.is_empty()
            && mana_enables_action_v1(state, options.player, options.mana_abilities, false).enables)
}

/// One mana activation the player could make now.
struct ManaActivationV1 {
    action: Action,
}

/// The (source, color) pairs autopay can pay with for `player` right now,
/// read from the engine itself: `mana::gather_sources_for_spell` for a
/// noncreature and for a creature spell (some sources pay creature spells
/// only). An activation of one of these pairs without a cost target adds
/// nothing autopay could not already use, so the mana test skips it; every
/// other activation (a sacrifice or life cost, a paid filtering ability, a
/// cost that taps or sacrifices another object, mana autopay only gives a
/// spell with particular subtypes) is searched. Searching an activation
/// autopay could have made is never wrong, only slower.
///
/// Builds with `eternal-fixtures` skip nothing: their cards add mana on a tap
/// outside the payment planner (Utopia Sprawl, Wild Growth: the eternal hooks
/// of `engine::activate_mana_ability_for`), so a source autopay lists can make
/// more by hand there. The default build has no such hook.
fn autopay_pairs_v1(
    state: &GameState,
    player: PlayerId,
) -> Vec<(ObjectId, crate::mana::ManaColor)> {
    if cfg!(feature = "eternal-fixtures") {
        return Vec::new();
    }
    let mut pairs = Vec::new();
    for creature_spell in [false, true] {
        for source in crate::mana::gather_sources_for_spell(player, state, creature_spell) {
            for &color in &source.choices {
                if !pairs.contains(&(source.id, color)) {
                    pairs.push((source.id, color));
                }
            }
        }
    }
    pairs
}

/// Whether any listed activation of `id` is one autopay cannot make.
fn hand_only_source_v1(
    state: &GameState,
    player: PlayerId,
    id: ObjectId,
    autopay: &[(ObjectId, crate::mana::ManaColor)],
) -> bool {
    engine::available_mana_ability_choices(player, id, state)
        .into_iter()
        .any(|choice| {
            !engine::mana_ability_cost_targets_for_choice(player, id, choice, state).is_empty()
                || !autopay.contains(&(id, choice))
        })
}

fn activations_v1(
    state: &GameState,
    player: PlayerId,
    sources: &[ObjectId],
    all_sources: bool,
) -> Vec<ManaActivationV1> {
    let mut out = Vec::new();
    let mut seen_keys = Vec::new();
    let autopay = if all_sources {
        Vec::new()
    } else {
        autopay_pairs_v1(state, player)
    };
    let mut ordered = sources.to_vec();
    ordered.sort();
    for id in ordered {
        let card = state.objects.get(id).card_def;
        let choices = engine::available_mana_ability_choices(player, id, state);
        for &choice in &choices {
            let targets = engine::mana_ability_cost_targets_for_choice(player, id, choice, state);
            if !all_sources && targets.is_empty() && autopay.contains(&(id, choice)) {
                continue;
            }
            // Sources of one card with no cost target are interchangeable for
            // the mana they make; a cost target is kept by object (two
            // objects of one card can differ, for example in summoning
            // sickness).
            if targets.is_empty() {
                let key = (card, choice, None);
                if !seen_keys.contains(&key) {
                    seen_keys.push(key);
                    let action = if choices.len() == 1 {
                        Action::ActivateManaAbility(id)
                    } else {
                        Action::ActivateManaAbilityChoice(id, choice)
                    };
                    out.push(ManaActivationV1 { action });
                }
            } else {
                for &target in &targets {
                    let key = (card, choice, Some(target));
                    if !seen_keys.contains(&key) {
                        seen_keys.push(key);
                        out.push(ManaActivationV1 {
                            action: Action::ActivateManaAbilityWithCostTarget(id, choice, target),
                        });
                    }
                }
            }
        }
    }
    out
}

/// The mana test: is there a sequence of mana activations of hand-only
/// sources (`all_sources`: of every listed source, a measurement variant)
/// after which the engine lists a castable spell or a non-mana activated
/// ability for `player`? Each step is the engine's own: the activation is
/// applied with `engine::step` to a clone of the state and the next decision
/// is computed with `engine::advance_until_decision`. Identical sources
/// (same card, same color, same cost target card) are tried once per node;
/// states already seen are skipped; past [`MANA_TEST_ACTIVATION_CAP_V1`]
/// activations the answer is "enables" (the window is offered).
pub fn mana_enables_action_v1(
    state: &GameState,
    player: PlayerId,
    mana_abilities: &[ObjectId],
    all_sources: bool,
) -> ManaTestV1 {
    let root = activations_v1(state, player, mana_abilities, all_sources);
    let autopay = autopay_pairs_v1(state, player);
    let mut test = ManaTestV1 {
        hand_only_sources: mana_abilities
            .iter()
            .filter(|&&id| all_sources || hand_only_source_v1(state, player, id, &autopay))
            .count() as u32,
        ..ManaTestV1::default()
    };
    if root.is_empty() {
        return test;
    }
    let mut seen = vec![state.state_hash()];
    test.enables = search_v1(state, player, root, all_sources, &mut seen, &mut test);
    test
}

fn search_v1(
    state: &GameState,
    player: PlayerId,
    activations: Vec<ManaActivationV1>,
    all_sources: bool,
    seen: &mut Vec<u64>,
    test: &mut ManaTestV1,
) -> bool {
    for activation in activations {
        if test.activations >= MANA_TEST_ACTIVATION_CAP_V1 {
            test.cap_reached = true;
            return true;
        }
        test.activations += 1;
        let mut next = state.clone();
        if engine::step(&mut next, activation.action).is_err() {
            continue;
        }
        match engine::advance_until_decision(&mut next) {
            Decision::CastSpellOrPass {
                player: asked,
                castable_spells,
                mana_abilities,
                activatable_abilities,
                ..
            } if asked == player => {
                if !castable_spells.is_empty() || !activatable_abilities.is_empty() {
                    return true;
                }
                let hash = next.state_hash();
                if seen.contains(&hash) {
                    continue;
                }
                seen.push(hash);
                let deeper = activations_v1(&next, player, &mana_abilities, all_sources);
                if !deeper.is_empty() && search_v1(&next, player, deeper, all_sources, seen, test) {
                    return true;
                }
            }
            // A mana ability that hands the game to anything other than this
            // player's next priority window (a trigger, a choice, the game
            // ending) is something the player can do: offer the window.
            _ => return true,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_def;
    use crate::state::Zone;
    use crate::surface::{SuppressionReason, SurfaceDecision};
    use crate::surface_v2::{HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode};

    fn game() -> GameState {
        GameState::new_from_libraries(&[], &[], |c| format!("card-{c}"), 1)
    }

    fn put(
        state: &mut GameState,
        player: PlayerId,
        name: &str,
        zone: crate::state::Zone,
    ) -> ObjectId {
        let card =
            card_def::card_id_by_name(name).unwrap_or_else(|| panic!("{name} not in CARD_DEFS"));
        let id = state.objects.push(crate::state::GameObject {
            card_def: card,
            name: name.to_string(),
            owner: player,
            controller: player,
            zone,
            tapped: false,
            summoning_sick: false,
            damage: 0,
            counters: Default::default(),
            attachments: Vec::new(),
            v4: crate::state::ObjectStateV4::from_card_def(card),
            spell_copy_origin: None,
            plotted_turn: None,
            zone_change_count: 0,
        });
        match zone {
            Zone::Battlefield => state.players[player.index()].battlefield.push(id),
            Zone::Hand => state.players[player.index()].hand.push(id),
            other => panic!("fixture zone {other:?}"),
        }
        id
    }

    /// `player` holds priority in `step` of `active`'s turn.
    fn at(state: &mut GameState, step: Step, active: PlayerId, player: PlayerId) {
        state.step = step;
        state.active_player = active;
        state.priority_player = player;
    }

    fn surface(rules: [SeatPriorityWindowsV1; 2]) -> HarnessSurfaceV2 {
        let mut surface = HarnessSurfaceV2::new();
        surface.set_action_window_seats_v1(rules).unwrap();
        surface
    }

    const P0_ACTS: [SeatPriorityWindowsV1; 2] = [
        SeatPriorityWindowsV1::ActionWindowsV1,
        SeatPriorityWindowsV1::HarnessV2,
    ];
    const P0_END_STEP: [SeatPriorityWindowsV1; 2] = [
        SeatPriorityWindowsV1::ActionWindowsEndStepV1,
        SeatPriorityWindowsV1::HarnessV2,
    ];

    /// The player asked, when the decision is a priority window in `step_now`.
    fn asked(decision: &SurfaceDecision, step_now: Step, state: &GameState) -> Option<PlayerId> {
        match decision {
            SurfaceDecision::Decision(Decision::CastSpellOrPass { player, .. })
                if state.step == step_now =>
            {
                Some(*player)
            }
            _ => None,
        }
    }

    #[test]
    fn an_instant_in_upkeep_is_offered_to_an_action_seat_only() {
        let fixture = || {
            let mut state = game();
            put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
            put(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
            at(&mut state, Step::Upkeep, PlayerId::P0, PlayerId::P0);
            state
        };
        let mut state = fixture();
        let mut ruled = surface(P0_ACTS);
        let decision = ruled.next_decision(&mut state);
        assert_eq!(asked(&decision, Step::Upkeep, &state), Some(PlayerId::P0));
        assert_eq!(ruled.engine_window_v1(), Some(SuppressionReason::StepGated));
        assert!(ruled.suppressions().is_empty());

        let mut state = fixture();
        let mut plain = HarnessSurfaceV2::new();
        let decision = plain.next_decision(&mut state);
        assert_eq!(asked(&decision, Step::Upkeep, &state), None);
        assert_eq!(plain.suppressions()[0].reason, SuppressionReason::StepGated);
    }

    #[test]
    fn a_gated_window_with_only_lands_to_tap_is_passed_for_an_action_seat() {
        let mut state = game();
        let mountain = put(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
        at(&mut state, Step::Upkeep, PlayerId::P0, PlayerId::P0);
        let test = mana_enables_action_v1(&state, PlayerId::P0, &[mountain], false);
        if !cfg!(feature = "eternal-fixtures") {
            assert_eq!(test.hand_only_sources, 0);
        }
        assert!(!test.enables);
        let mut ruled = surface(P0_ACTS);
        let decision = ruled.next_decision(&mut state);
        assert_eq!(asked(&decision, Step::Upkeep, &state), None);
        assert_eq!(ruled.suppressions()[0].reason, SuppressionReason::StepGated);
    }

    #[test]
    fn the_end_step_rule_offers_the_opponents_end_step_only() {
        let fixture = |step: Step, active: PlayerId| {
            let mut state = game();
            put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
            put(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
            at(&mut state, step, active, PlayerId::P0);
            state
        };
        let mut state = fixture(Step::End, PlayerId::P1);
        let mut ruled = surface(P0_END_STEP);
        let decision = ruled.next_decision(&mut state);
        assert_eq!(asked(&decision, Step::End, &state), Some(PlayerId::P0));
        assert_eq!(ruled.engine_window_v1(), Some(SuppressionReason::StepGated));
        for (step, active) in [(Step::End, PlayerId::P0), (Step::Upkeep, PlayerId::P0)] {
            let mut state = fixture(step, active);
            let mut ruled = surface(P0_END_STEP);
            let decision = ruled.next_decision(&mut state);
            assert_eq!(asked(&decision, step, &state), None, "{step:?}");
            assert_eq!(ruled.suppressions()[0].reason, SuppressionReason::StepGated);
        }
    }

    #[test]
    fn a_hand_only_source_that_can_pay_a_spell_passes_the_mana_test() {
        let mut state = game();
        put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
        let petal = put(&mut state, PlayerId::P0, "Lotus Petal", Zone::Battlefield);
        at(&mut state, Step::Main1, PlayerId::P0, PlayerId::P0);
        let test = mana_enables_action_v1(&state, PlayerId::P0, &[petal], false);
        assert_eq!(test.hand_only_sources, 1, "Lotus Petal pays only by hand");
        assert!(test.enables);
        assert!(!test.cap_reached);
        assert!(test.activations >= 1);

        // The surface keeps this mana-only main-phase window, as today.
        let mut ruled = surface(P0_ACTS);
        let decision = ruled.next_decision(&mut state);
        assert_eq!(asked(&decision, Step::Main1, &state), Some(PlayerId::P0));
        assert!(ruled.suppressions().is_empty());
    }

    #[test]
    fn a_hand_only_source_in_upkeep_is_offered_when_it_pays_an_instant() {
        let mut state = game();
        put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
        put(&mut state, PlayerId::P0, "Lotus Petal", Zone::Battlefield);
        at(&mut state, Step::Upkeep, PlayerId::P0, PlayerId::P0);
        let mut ruled = surface(P0_ACTS);
        let decision = ruled.next_decision(&mut state);
        assert_eq!(asked(&decision, Step::Upkeep, &state), Some(PlayerId::P0));
        assert_eq!(ruled.engine_window_v1(), Some(SuppressionReason::StepGated));
    }

    #[test]
    fn a_mana_only_window_that_enables_nothing_is_passed_for_an_action_seat() {
        let fixture = || {
            let mut state = game();
            put(&mut state, PlayerId::P0, "Lotus Petal", Zone::Battlefield);
            at(&mut state, Step::Main1, PlayerId::P0, PlayerId::P0);
            state
        };
        let mut state = fixture();
        let petal = state.players[0].battlefield[0];
        assert!(!mana_enables_action_v1(&state, PlayerId::P0, &[petal], false).enables);
        let mut ruled = surface(P0_ACTS);
        ruled.next_decision(&mut state);
        let first = &ruled.suppressions()[0];
        assert_eq!(first.reason, SuppressionReason::NoRealOption);
        assert!(
            first.auto_action.contains("mana test failed"),
            "{}",
            first.auto_action
        );

        // Today's surface asks it; so does the end-step rule (main phases
        // are today's there).
        for rules in [[SeatPriorityWindowsV1::HarnessV2; 2], P0_END_STEP] {
            let mut state = fixture();
            let mut other = surface(rules);
            let decision = other.next_decision(&mut state);
            assert_eq!(asked(&decision, Step::Main1, &state), Some(PlayerId::P0));
            assert!(other.suppressions().is_empty());
        }
    }

    #[test]
    fn rules_are_refused_with_engine_windows_and_beside_a_flagged_seat() {
        let mut engine = HarnessSurfaceV2::new_with_priority_mode_v1(
            PriorityModeV1::EngineWindowsV1,
            SuppressionAuditMode::Off,
        );
        assert!(engine.set_action_window_seats_v1(P0_ACTS).is_err());
        assert!(engine
            .set_action_window_seats_v1([SeatPriorityWindowsV1::HarnessV2; 2])
            .is_ok());

        let mut flagged = HarnessSurfaceV2::new();
        flagged.set_engine_window_seats_v1([true, false]).unwrap();
        assert!(flagged.set_action_window_seats_v1(P0_ACTS).is_err());
        assert!(flagged
            .set_action_window_seats_v1([
                SeatPriorityWindowsV1::HarnessV2,
                SeatPriorityWindowsV1::ActionWindowsV1,
            ])
            .is_ok());
        assert!(flagged.set_engine_window_seats_v1([false, true]).is_err());
    }

    #[test]
    fn rule_names_are_the_config_strings() {
        for (rule, name) in [
            (SeatPriorityWindowsV1::HarnessV2, "\"harness-v2\""),
            (
                SeatPriorityWindowsV1::ActionWindowsV1,
                "\"action-windows-v1\"",
            ),
            (
                SeatPriorityWindowsV1::ActionWindowsEndStepV1,
                "\"action-windows-end-step-v1\"",
            ),
        ] {
            assert_eq!(serde_json::to_string(&rule).unwrap(), name);
            assert_eq!(
                serde_json::from_str::<SeatPriorityWindowsV1>(name).unwrap(),
                rule
            );
        }
    }

    /// Review finding 1: a land whose primary ability autopay uses can still
    /// have a paid ability that only a hand activation reaches (Heap Gate's
    /// colored mana); the mana test must try it.
    #[test]
    fn a_paid_ability_of_an_automatic_land_is_tried_by_the_mana_test() {
        let mut state = game();
        put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
        let gate = put(&mut state, PlayerId::P0, "Heap Gate", Zone::Battlefield);
        let island = put(&mut state, PlayerId::P0, "Island", Zone::Battlefield);
        at(&mut state, Step::Main1, PlayerId::P0, PlayerId::P0);
        let test = mana_enables_action_v1(&state, PlayerId::P0, &[gate, island], false);
        assert!(test.enables, "{test:?}");
        let mut ruled = surface(P0_ACTS);
        let decision = ruled.next_decision(&mut state);
        assert_eq!(asked(&decision, Step::Main1, &state), Some(PlayerId::P0));
        assert!(ruled.suppressions().is_empty());
    }

    /// Review finding 2: two objects of one card are not interchangeable as
    /// cost targets. Saruli Caretaker taps another creature as its cost; only
    /// tapping the summoning-sick Elf leaves the other Elf to pay, in either
    /// object order.
    #[test]
    fn cost_targets_of_one_card_are_tried_by_object() {
        for sick_first in [false, true] {
            let mut state = game();
            put(&mut state, PlayerId::P0, "Winding Way", Zone::Hand);
            let caretaker = put(
                &mut state,
                PlayerId::P0,
                "Saruli Caretaker",
                Zone::Battlefield,
            );
            let first = put(
                &mut state,
                PlayerId::P0,
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let second = put(
                &mut state,
                PlayerId::P0,
                "Llanowar Elves",
                Zone::Battlefield,
            );
            state
                .objects
                .get_mut(if sick_first { first } else { second })
                .summoning_sick = true;
            at(&mut state, Step::Main1, PlayerId::P0, PlayerId::P0);
            let test =
                mana_enables_action_v1(&state, PlayerId::P0, &[caretaker, first, second], false);
            assert!(test.enables, "sick first {sick_first}: {test:?}");
        }
    }

    /// Review round 3: activations autopay already makes are skipped, read
    /// from the engine's own autopay source list; an inert board of plain
    /// mana sources costs no simulated activation.
    #[test]
    fn an_inert_board_of_autopay_sources_searches_nothing() {
        let mut state = game();
        let mut sources = Vec::new();
        for name in [
            "Mountain",
            "Island",
            "Forest",
            "Llanowar Elves",
            "Heap Gate",
        ] {
            sources.push(put(&mut state, PlayerId::P0, name, Zone::Battlefield));
        }
        at(&mut state, Step::Main1, PlayerId::P0, PlayerId::P0);
        let test = mana_enables_action_v1(&state, PlayerId::P0, &sources, false);
        assert!(!test.enables, "{test:?}");
        assert!(!test.cap_reached, "{test:?}");
        // Only Heap Gate's paid colored ability is outside autopay.
        if !cfg!(feature = "eternal-fixtures") {
            assert_eq!(test.hand_only_sources, 1, "{test:?}");
            assert!(test.activations <= 5, "{test:?}");
        }
    }
}
