//! Focused rules coverage for four pauper-meta wave 1 black/red spells:
//! Suffocating Fumes, Arms of Hadar, Smash to Smithereens, and Raze.
//!
//! The bounded rules oracle is the Mage fork at `72a08a3b`:
//! `SuffocatingFumes.java` blob `47433c982a105374f8bfa18d7d83a3aad64012c4`,
//! `ArmsOfHadar.java` blob `64c51cc3feb6cd71d2dcef78eb3efb097165251c`,
//! `SmashToSmithereens.java` blob `883cac3e0833d12b77a93e478a82eb57b2991004`,
//! `Raze.java` blob `6e6f77295f5be1402e838ced2608da87c62ea492`. Together they
//! establish: Suffocating Fumes gives creatures your opponents control
//! -1/-1 until end of turn, and has cycling {2}; Arms of Hadar gives
//! creatures target player controls -2/-2 until end of turn; Smash to
//! Smithereens destroys target artifact, then deals 3 damage to that
//! artifact's controller -- read via `getFirstTargetPermanentOrLKI`
//! (last known information) for the case where the destroy earlier in
//! *this same resolution* already moved the artifact off the battlefield.
//! A target already gone *before* the spell resolves at all is a separate,
//! ordinary CR 608.2b fizzle (one target referenced twice in the card's own
//! text): neither the destroy nor the damage happens, matching Smash to
//! Smithereens' own Gatherer ruling. Raze's additional cost is "sacrifice a
//! land", and it destroys target land.

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, CostKind, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::state::{Counters, GameObject, GameState, Step, Target, Zone};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

// Copied verbatim from `tests/deep_analysis.rs:24-60` / `tests/pauper_meta_w1_spells.rs`.
fn put_object(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id(name);
    let id = state.objects.push(GameObject {
        card_def,
        name: name.to_string(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: mtg_kernel::state::ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Library => state.players[player.index()].library.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        Zone::Exile => state.exile.push(id),
        Zone::Command => state.command.push(id),
        Zone::Stack => panic!("test helper does not construct stack objects"),
    }
    id
}

/// Mirrors `pauper_meta_w1_spells.rs`'s `ready_main1`: Main1, P0 active with
/// priority, both libraries as given.
fn ready_main1(p0_library: &[&str], p1_library: &[&str]) -> GameState {
    let p0_defs = p0_library.iter().map(|name| card_id(name)).collect::<Vec<_>>();
    let p1_defs = p1_library.iter().map(|name| card_id(name)).collect::<Vec<_>>();
    let mut state =
        GameState::new_from_libraries(&p0_defs, &p1_defs, card_name, 0x424c41434b524544);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

/// `pass_until_stack_len(state, 0)`, copied from `pauper_meta_w1_spells.rs`.
fn pass_until_stack_len(state: &mut GameState, wanted: usize) -> Decision {
    for _ in 0..16 {
        let decision = engine::advance_until_decision(state);
        if state.stack.len() <= wanted {
            return decision;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::GameOver { .. } | Decision::Halted { .. } => return decision,
            other => panic!("unexpected decision while resolving the stack: {other:?}"),
        }
    }
    panic!("stack did not reach length {wanted}")
}

fn pass_until_stack_empty(state: &mut GameState) -> Decision {
    pass_until_stack_len(state, 0)
}

/// Advances (declining every attack/block and passing every priority window
/// along the way) until `player`'s *next* Main1, i.e. until `state.turn` has
/// increased past `starting_turn` and the engine is about to offer that
/// player priority in Main1. Leaves the state sitting at that
/// `CastSpellOrPass` decision without consuming it, so the caller can
/// inspect board state immediately.
fn advance_to_players_next_main1(state: &mut GameState, player: PlayerId) {
    let starting_turn = state.turn;
    loop {
        match engine::advance_until_decision(state) {
            Decision::CastSpellOrPass { player: p, .. }
                if state.turn > starting_turn
                    && state.step == Step::Main1
                    && state.active_player == player
                    && p == player =>
            {
                return;
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::DeclareAttackers { .. } => {
                engine::step(state, Action::DeclareAttackers(Vec::new())).unwrap();
            }
            Decision::DeclareBlockers { .. } => {
                engine::step(state, Action::DeclareBlockers(Vec::new())).unwrap();
            }
            other => panic!("unexpected decision advancing turns: {other:?}"),
        }
    }
}

#[test]
fn suffocating_fumes_gives_opponents_creatures_minus_one_until_end_of_turn() {
    let mut state = ready_main1(&["Island"; 8], &["Island"; 8]);
    let fumes = put_object(&mut state, PlayerId::P0, "Suffocating Fumes", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Swamp", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Swamp", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Swamp", Zone::Battlefield);
    let p0_creature = put_object(&mut state, PlayerId::P0, "Ninja of the Deep Hours", Zone::Battlefield);
    let squirrel = put_object(&mut state, PlayerId::P1, "Squirrel Token", Zone::Battlefield);
    let sagu = put_object(&mut state, PlayerId::P1, "Sagu Wildling", Zone::Battlefield);

    let offer = engine::advance_until_decision(&mut state);
    assert!(matches!(
        offer,
        Decision::CastSpellOrPass { ref castable_spells, .. } if castable_spells.contains(&fumes)
    ));
    engine::step(&mut state, Action::CastSpell(fumes)).unwrap();
    pass_until_stack_empty(&mut state);

    assert_eq!(state.objects[fumes].zone, Zone::Graveyard);
    assert_eq!(
        state.objects[squirrel].zone,
        Zone::Graveyard,
        "the 1/1 Squirrel Token dies to -1/-1 (0 toughness state-based action)"
    );
    assert_eq!(state.objects[sagu].zone, Zone::Battlefield);
    assert_eq!(engine::effective_power(&state, sagu), 2);
    assert_eq!(engine::effective_toughness(&state, sagu), 2, "the 3/3 is a 2/2 until end of turn");
    assert_eq!(state.objects[p0_creature].zone, Zone::Battlefield);
    assert_eq!(
        engine::effective_power(&state, p0_creature),
        2,
        "the caster's own 2/2 is unaffected: Suffocating Fumes only hits opponents' creatures"
    );
    assert_eq!(engine::effective_toughness(&state, p0_creature), 2);

    advance_to_players_next_main1(&mut state, PlayerId::P0);
    assert_eq!(
        engine::effective_power(&state, sagu),
        3,
        "the -1/-1 expired at cleanup; the 3/3 is back to 3/3 by P0's next turn"
    );
    assert_eq!(engine::effective_toughness(&state, sagu), 3);
}

#[test]
fn suffocating_fumes_cycles_for_two() {
    let mut state = ready_main1(&["Island"; 8], &["Island"; 8]);
    let fumes = put_object(&mut state, PlayerId::P0, "Suffocating Fumes", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Swamp", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Swamp", Zone::Battlefield);
    let library_before = state.players[0].library.len();
    let hand_before = state.players[0].hand.len();

    let offer = engine::advance_until_decision(&mut state);
    match offer {
        Decision::CastSpellOrPass { ref activatable_abilities, .. } => {
            assert!(
                activatable_abilities.contains(&(fumes, 0)),
                "cycling {{2}} should be offered as an activated ability from hand"
            );
        }
        other => panic!("{other:?}"),
    }
    engine::step(&mut state, Action::ActivateAbility(fumes, 0)).unwrap();
    pass_until_stack_empty(&mut state);

    assert_eq!(
        state.objects[fumes].zone,
        Zone::Graveyard,
        "cycling discards the card itself as part of its cost"
    );
    assert_eq!(
        state.players[0].library.len(),
        library_before - 1,
        "cycling draws exactly one card"
    );
    assert_eq!(
        state.players[0].hand.len(),
        hand_before,
        "one card (Suffocating Fumes) was discarded from hand as a cost and one was drawn back"
    );
}

#[test]
fn arms_of_hadar_shrinks_only_the_targeted_players_creatures() {
    // Targeting P1 shrinks P1's creature and leaves P0's alone.
    let mut state = ready_main1(&["Island"; 8], &["Island"; 8]);
    let hadar = put_object(&mut state, PlayerId::P0, "Arms of Hadar", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Swamp", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Swamp", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Swamp", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Swamp", Zone::Battlefield);
    let p0_creature = put_object(&mut state, PlayerId::P0, "Ninja of the Deep Hours", Zone::Battlefield);
    let p1_creature = put_object(&mut state, PlayerId::P1, "Ninja of the Deep Hours", Zone::Battlefield);

    let offer = engine::advance_until_decision(&mut state);
    assert!(matches!(
        offer,
        Decision::CastSpellOrPass { ref castable_spells, .. } if castable_spells.contains(&hadar)
    ));
    engine::step(&mut state, Action::CastSpell(hadar)).unwrap();
    match engine::advance_until_decision(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Player(PlayerId::P0)));
            assert!(legal_targets.contains(&Target::Player(PlayerId::P1)));
        }
        other => panic!("{other:?}"),
    }
    engine::step(&mut state, Action::ChooseTarget(Target::Player(PlayerId::P1))).unwrap();
    pass_until_stack_empty(&mut state);

    assert_eq!(
        state.objects[p1_creature].zone,
        Zone::Graveyard,
        "P1's 2/2 dies to -2/-2 (0 toughness state-based action)"
    );
    assert_eq!(state.objects[p0_creature].zone, Zone::Battlefield);
    assert_eq!(engine::effective_power(&state, p0_creature), 2);
    assert_eq!(engine::effective_toughness(&state, p0_creature), 2, "P0's 2/2 is unchanged");

    // Targeting P0 instead shrinks P0's creature and leaves P1's alone.
    let mut state2 = ready_main1(&["Island"; 8], &["Island"; 8]);
    let hadar2 = put_object(&mut state2, PlayerId::P0, "Arms of Hadar", Zone::Hand);
    put_object(&mut state2, PlayerId::P0, "Swamp", Zone::Battlefield);
    put_object(&mut state2, PlayerId::P0, "Swamp", Zone::Battlefield);
    put_object(&mut state2, PlayerId::P0, "Swamp", Zone::Battlefield);
    put_object(&mut state2, PlayerId::P0, "Swamp", Zone::Battlefield);
    let p0_creature2 = put_object(&mut state2, PlayerId::P0, "Ninja of the Deep Hours", Zone::Battlefield);
    let p1_creature2 = put_object(&mut state2, PlayerId::P1, "Ninja of the Deep Hours", Zone::Battlefield);

    engine::advance_until_decision(&mut state2);
    engine::step(&mut state2, Action::CastSpell(hadar2)).unwrap();
    engine::advance_until_decision(&mut state2);
    engine::step(&mut state2, Action::ChooseTarget(Target::Player(PlayerId::P0))).unwrap();
    pass_until_stack_empty(&mut state2);

    assert_eq!(
        state2.objects[p0_creature2].zone,
        Zone::Graveyard,
        "targeting P0 this time shrinks P0's own 2/2 instead"
    );
    assert_eq!(state2.objects[p1_creature2].zone, Zone::Battlefield);
    assert_eq!(engine::effective_power(&state2, p1_creature2), 2);
    assert_eq!(engine::effective_toughness(&state2, p1_creature2), 2, "P1's 2/2 is unchanged");
}

#[test]
fn smash_to_smithereens_destroys_the_artifact_and_burns_its_controller() {
    // Baseline: the artifact is still on the battlefield at resolution.
    let mut state = ready_main1(&["Island"; 8], &["Island"; 8]);
    let smash = put_object(&mut state, PlayerId::P0, "Smash to Smithereens", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
    let enforcer = put_object(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);

    let offer = engine::advance_until_decision(&mut state);
    assert!(matches!(
        offer,
        Decision::CastSpellOrPass { ref castable_spells, .. } if castable_spells.contains(&smash)
    ));
    engine::step(&mut state, Action::CastSpell(smash)).unwrap();
    match engine::advance_until_decision(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(enforcer)));
        }
        other => panic!("{other:?}"),
    }
    engine::step(&mut state, Action::ChooseTarget(Target::Object(enforcer))).unwrap();
    pass_until_stack_empty(&mut state);

    assert_eq!(state.objects[enforcer].zone, Zone::Graveyard);
    assert_eq!(state.objects[smash].zone, Zone::Graveyard);
    assert_eq!(state.players[1].life, 17, "3 damage to the destroyed artifact's controller");

    // Last-known-controller case: P1 sacrifices the targeted artifact (its
    // own Nihil Spellbomb) in response, holding priority, before Smash to
    // Smithereens resolves. Smash to Smithereens has exactly one target,
    // referenced twice in its own text (destroy it; damage *its*
    // controller); CR 608.2b makes the whole spell fizzle when that one
    // target is illegal at resolution, so this matches the real card's own
    // Gatherer ruling ("If the target artifact is an illegal target when
    // Smash to Smithereens tries to resolve, neither part of its effect
    // will happen") -- no destroy and no damage, not a last-known-info
    // damage. `stack_targets_still_legal` (engine.rs) is what enforces
    // this fizzle for the kernel, before `EffectOp::execute` ever runs.
    let mut state2 = ready_main1(&["Island"; 8], &["Island"; 8]);
    let smash2 = put_object(&mut state2, PlayerId::P0, "Smash to Smithereens", Zone::Hand);
    put_object(&mut state2, PlayerId::P0, "Mountain", Zone::Battlefield);
    put_object(&mut state2, PlayerId::P0, "Mountain", Zone::Battlefield);
    let spellbomb = put_object(&mut state2, PlayerId::P1, "Nihil Spellbomb", Zone::Battlefield);

    engine::advance_until_decision(&mut state2);
    engine::step(&mut state2, Action::CastSpell(smash2)).unwrap();
    match engine::advance_until_decision(&mut state2) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(spellbomb)));
        }
        other => panic!("{other:?}"),
    }
    engine::step(&mut state2, Action::ChooseTarget(Target::Object(spellbomb))).unwrap();

    // P0 (the caster) gets priority first and passes it to P1.
    match engine::advance_until_decision(&mut state2) {
        Decision::CastSpellOrPass { player: PlayerId::P0, .. } => {
            engine::step(&mut state2, Action::Pass).unwrap();
        }
        other => panic!("expected P0 priority right after casting, got {other:?}"),
    }
    // P1 responds by activating Nihil Spellbomb's own sacrifice ability
    // instead of passing.
    match engine::advance_until_decision(&mut state2) {
        Decision::CastSpellOrPass {
            player: PlayerId::P1,
            ref activatable_abilities,
            ..
        } => {
            assert!(activatable_abilities.contains(&(spellbomb, 0)));
            engine::step(&mut state2, Action::ActivateAbility(spellbomb, 0)).unwrap();
        }
        other => panic!("expected P1 priority with Nihil Spellbomb activatable, got {other:?}"),
    }
    // Target P0's (empty) graveyard, not P1's -- P1's graveyard now holds
    // the just-sacrificed Nihil Spellbomb itself, and exiling *that*
    // graveyard would move it to Exile before this test can observe it
    // sitting in the Graveyard as a plain sacrifice.
    match engine::advance_until_decision(&mut state2) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Player(PlayerId::P0)));
            engine::step(&mut state2, Action::ChooseTarget(Target::Player(PlayerId::P0))).unwrap();
        }
        other => panic!("expected Nihil Spellbomb's own player target, got {other:?}"),
    }
    pass_until_stack_empty(&mut state2);

    assert_eq!(
        state2.objects[spellbomb].zone,
        Zone::Graveyard,
        "Nihil Spellbomb was sacrificed to pay for its own ability, in response"
    );
    assert_eq!(
        state2.objects[smash2].zone,
        Zone::Graveyard,
        "a fizzled spell still resolves as an event and goes to the graveyard"
    );
    assert_eq!(
        state2.players[1].life,
        20,
        "the spell's only target was illegal at resolution, so it fizzles entirely (CR 608.2b): \
         no destroy and no damage, unlike the same-resolution case above"
    );
}

#[test]
fn raze_requires_sacrificing_a_land_and_destroys_the_target_land() {
    // With zero lands, Raze is castable neither for mana nor for its
    // "sacrifice a land" additional cost.
    let mut state = ready_main1(&["Island"; 8], &["Island"; 8]);
    let raze = put_object(&mut state, PlayerId::P0, "Raze", Zone::Hand);
    let offer = engine::advance_until_decision(&mut state);
    match offer {
        Decision::CastSpellOrPass { ref castable_spells, .. } => {
            assert!(
                !castable_spells.contains(&raze),
                "no lands at all: neither {{R}} nor the sacrifice cost can be paid"
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(engine::step(&mut state, Action::CastSpell(raze)).is_err());

    // With two lands, Raze can sacrifice one (chosen from the offered
    // candidates) as its additional cost and destroy the other.
    let mut state2 = ready_main1(&["Island"; 8], &["Island"; 8]);
    let raze2 = put_object(&mut state2, PlayerId::P0, "Raze", Zone::Hand);
    let mountain_a = put_object(&mut state2, PlayerId::P0, "Mountain", Zone::Battlefield);
    let mountain_b = put_object(&mut state2, PlayerId::P0, "Mountain", Zone::Battlefield);

    let offer2 = engine::advance_until_decision(&mut state2);
    assert!(matches!(
        offer2,
        Decision::CastSpellOrPass { ref castable_spells, .. } if castable_spells.contains(&raze2)
    ));
    engine::step(&mut state2, Action::CastSpell(raze2)).unwrap();

    let mut targeted = false;
    let mut sacrificed = None;
    loop {
        match engine::advance_until_decision(&mut state2) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(mountain_a)));
                assert!(legal_targets.contains(&Target::Object(mountain_b)));
                engine::step(&mut state2, Action::ChooseTarget(Target::Object(mountain_a))).unwrap();
                targeted = true;
            }
            Decision::ChooseCostTargets {
                cost_kind,
                candidates,
                ..
            } => {
                assert_eq!(cost_kind, CostKind::SacrificeLands);
                assert!(candidates.contains(&mountain_a));
                assert!(candidates.contains(&mountain_b));
                engine::step(&mut state2, Action::ChooseCostTarget(mountain_b)).unwrap();
                sacrificed = Some(mountain_b);
            }
            Decision::CastSpellOrPass { .. } => break,
            other => panic!("unexpected decision while casting Raze: {other:?}"),
        }
    }
    assert!(targeted, "Raze's own target (a land) must be chosen");
    assert_eq!(sacrificed, Some(mountain_b), "the additional cost's land must be chosen");
    pass_until_stack_empty(&mut state2);

    assert_eq!(
        state2.objects[mountain_b].zone,
        Zone::Graveyard,
        "the sacrificed land is in the graveyard"
    );
    assert_eq!(
        state2.objects[mountain_a].zone,
        Zone::Graveyard,
        "the targeted land is destroyed and in the graveyard"
    );
    assert_eq!(state2.objects[raze2].zone, Zone::Graveyard);

    // With exactly one controlled land, Raze is still castable: the mana
    // cost and the additional cost are checked independently by the
    // generic cost machinery (`engine::is_castable_now`/`can_pay_components`),
    // with no reservation between them, so the same land can pay {R} and
    // then, since a sacrifice never checks tapped status, also serve as the
    // one land `CostComponent::SacrificeControlled` needs. This also
    // matches real Magic: tapping a land for mana and then sacrificing
    // that same tapped land is legal. The target is the opponent's land,
    // so a legal target still exists once the caster's only land is spent
    // as the additional cost.
    let mut state3 = ready_main1(&["Island"; 8], &["Island"; 8]);
    let raze3 = put_object(&mut state3, PlayerId::P0, "Raze", Zone::Hand);
    let sole_land = put_object(&mut state3, PlayerId::P0, "Mountain", Zone::Battlefield);
    let opponent_land = put_object(&mut state3, PlayerId::P1, "Mountain", Zone::Battlefield);

    let offer3 = engine::advance_until_decision(&mut state3);
    assert!(
        matches!(
            offer3,
            Decision::CastSpellOrPass { ref castable_spells, .. } if castable_spells.contains(&raze3)
        ),
        "with exactly one land, Raze is still castable: that land pays {{R}} and is then sacrificed"
    );
    engine::step(&mut state3, Action::CastSpell(raze3)).unwrap();

    let mut targeted3 = false;
    loop {
        match engine::advance_until_decision(&mut state3) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(opponent_land)));
                engine::step(&mut state3, Action::ChooseTarget(Target::Object(opponent_land)))
                    .unwrap();
                targeted3 = true;
            }
            Decision::ChooseCostTargets { .. } => {
                panic!(
                    "with only one candidate land, the sacrifice must auto-resolve without a \
                     ChooseCostTargets decision (same convention `drain_pending_cast_or_decide` \
                     uses whenever `candidates.len() <= 1`)"
                );
            }
            Decision::CastSpellOrPass { .. } => break,
            other => panic!("unexpected decision while casting Raze with one land: {other:?}"),
        }
    }
    assert!(targeted3, "Raze's own target (the opponent's land) must be chosen");
    pass_until_stack_empty(&mut state3);

    assert_eq!(
        state3.objects[sole_land].zone,
        Zone::Graveyard,
        "the caster's sole land pays the additional cost, auto-selected as the only candidate"
    );
    assert_eq!(
        state3.objects[opponent_land].zone,
        Zone::Graveyard,
        "the targeted opponent's land is destroyed"
    );
    assert_eq!(state3.objects[raze3].zone, Zone::Graveyard);
}
