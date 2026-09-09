//! Focused rules coverage for six pauper-meta wave 1 spells that reuse
//! existing kernel machinery: Terminate, Ancient Grudge, Artful Dodge,
//! Abandon Attachments, Acorn Harvest, and the Squirrel Token it creates.
//!
//! The bounded rules oracle is the Mage fork at `72a08a3b`:
//! `Terminate.java` blob `ceba2c65200523c48b3a6607710be613bac44e60`,
//! `AncientGrudge.java` blob `2a8fb1204b0231b915164b5b35a61edc63702902`,
//! `ArtfulDodge.java` blob `9a21bd56283f0692c9d26cf9787e4c1f5b2e0aab`,
//! `AbandonAttachments.java` blob
//! `d3a447913d00cf86dc26315c622a00ab205ff93e`, `AcornHarvest.java` blob
//! `d1a81aa82f997d3298077bfa36010dbad0aa9878`, and `SquirrelToken.java`
//! blob `2faded2af2ecff67b4d16ca6ed8a589081bc94ff`. Together they establish:
//! Terminate destroys target creature (it "can't be regenerated", which has
//! no kernel equivalent since the engine models no regeneration -- so this
//! is a plain destroy); Ancient Grudge destroys target artifact with
//! flashback {G}; Artful Dodge makes target creature unblockable this turn
//! with flashback {U}; Abandon Attachments ({1}{U/R}) may discard a card to
//! draw two; Acorn Harvest creates two 1/1 green Squirrel tokens with
//! flashback {1}{G} plus pay 3 life.

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::state::{Counters, GameObject, GameState, Step, Target, Zone};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

// Copied verbatim from `tests/deep_analysis.rs:24-60`.
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

/// Mirrors `deep_analysis.rs`'s `ready_deep` without the Deep Analysis
/// object: Main1, P0 active with priority, both libraries as given.
fn ready_main1(p0_library: &[&str], p1_library: &[&str]) -> GameState {
    let p0_defs = p0_library.iter().map(|name| card_id(name)).collect::<Vec<_>>();
    let p1_defs = p1_library.iter().map(|name| card_id(name)).collect::<Vec<_>>();
    let mut state =
        GameState::new_from_libraries(&p0_defs, &p1_defs, card_name, 0x4445_4550_414E_414C);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

/// `pass_until_stack_len(state, 0)`, copied from `deep_analysis.rs:126-140`.
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

/// Passes at every `CastSpellOrPass` decision until a different decision
/// (or a terminal one) is reached -- used to drive a spell whose resolution
/// itself asks a decision (e.g. `Decision::ChooseOptionalCost`) before the
/// stack empties.
fn pass_until_next_decision(state: &mut GameState) -> Decision {
    loop {
        let decision = engine::advance_until_decision(state);
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => return other,
        }
    }
}

#[test]
fn terminate_destroys_target_creature() {
    let mut state = ready_main1(&["Island"; 8], &["Island"; 8]);
    let terminate = put_object(&mut state, PlayerId::P0, "Terminate", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Swamp", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
    let victim = put_object(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);

    let offer = engine::advance_until_decision(&mut state);
    assert!(matches!(
        offer,
        Decision::CastSpellOrPass { ref castable_spells, .. } if castable_spells.contains(&terminate)
    ));
    engine::step(&mut state, Action::CastSpell(terminate)).unwrap();
    match engine::advance_until_decision(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(victim)));
        }
        other => panic!("{other:?}"),
    }
    engine::step(&mut state, Action::ChooseTarget(Target::Object(victim))).unwrap();
    pass_until_stack_empty(&mut state);
    assert_eq!(state.objects[victim].zone, Zone::Graveyard);
    assert_eq!(state.objects[terminate].zone, Zone::Graveyard);
}

#[test]
fn terminate_has_no_legal_target_without_a_creature() {
    // Only lands on both sides: Terminate's `TargetSpec::Creature` prefix
    // can never complete, so `is_castable_now` (via
    // `viable_printed_spell_modes`) excludes it from `castable_spells`
    // entirely -- same convention as Cast Down's own definitions test
    // (`tests/pauper_interaction_creatures_v1.rs`), which asserts the
    // generated program shape rather than a runtime "no targets" prompt.
    let mut state = ready_main1(&["Island"; 8], &["Island"; 8]);
    let terminate = put_object(&mut state, PlayerId::P0, "Terminate", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Swamp", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);

    let offer = engine::advance_until_decision(&mut state);
    match offer {
        Decision::CastSpellOrPass { ref castable_spells, .. } => {
            assert!(
                !castable_spells.contains(&terminate),
                "Terminate has no legal creature target and must not be offered as castable"
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(
        engine::step(&mut state, Action::CastSpell(terminate)).is_err(),
        "casting Terminate directly must also be rejected with no legal target"
    );
}

#[test]
fn ancient_grudge_destroys_an_artifact_and_flashes_back_for_g() {
    let mut state = ready_main1(&["Island"; 8], &["Island"; 8]);
    let grudge = put_object(&mut state, PlayerId::P0, "Ancient Grudge", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    let victim = put_object(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);

    let offer = engine::advance_until_decision(&mut state);
    assert!(matches!(
        offer,
        Decision::CastSpellOrPass { ref castable_spells, .. } if castable_spells.contains(&grudge)
    ));
    engine::step(&mut state, Action::CastSpell(grudge)).unwrap();
    match engine::advance_until_decision(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(victim)));
        }
        other => panic!("{other:?}"),
    }
    engine::step(&mut state, Action::ChooseTarget(Target::Object(victim))).unwrap();
    pass_until_stack_empty(&mut state);
    assert_eq!(state.objects[victim].zone, Zone::Graveyard);
    assert_eq!(state.objects[grudge].zone, Zone::Graveyard);

    // Flashback {G}, from the graveyard, with exactly one untapped Forest.
    let victim2 = put_object(&mut state, PlayerId::P1, "Myr Enforcer", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    let offer2 = engine::advance_until_decision(&mut state);
    assert!(
        matches!(
            offer2,
            Decision::CastSpellOrPass { ref castable_spells, .. } if castable_spells.contains(&grudge)
        ),
        "flashback should be offered from the graveyard with G available"
    );
    engine::step(&mut state, Action::CastSpell(grudge)).unwrap();
    match engine::advance_until_decision(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(victim2)));
        }
        other => panic!("{other:?}"),
    }
    engine::step(&mut state, Action::ChooseTarget(Target::Object(victim2))).unwrap();
    pass_until_stack_empty(&mut state);
    assert_eq!(state.objects[victim2].zone, Zone::Graveyard);
    assert_eq!(
        state.objects[grudge].zone,
        Zone::Exile,
        "a resolved flashback cast exiles the card instead of returning it to the graveyard"
    );
}

/// Advances through steps that need no player decision (passing every
/// `CastSpellOrPass`) until reaching a decision this test must act on.
fn advance_ignoring_priority(state: &mut GameState) -> Decision {
    loop {
        let decision = engine::advance_until_decision(state);
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => return other,
        }
    }
}

#[test]
fn artful_dodge_makes_the_target_unblockable_this_turn_and_flashes_back() {
    let mut state = ready_main1(&["Island"; 8], &["Island"; 8]);
    let dodge = put_object(&mut state, PlayerId::P0, "Artful Dodge", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Island", Zone::Battlefield);
    let attacker = put_object(&mut state, PlayerId::P0, "Myr Enforcer", Zone::Battlefield);
    let blocker = put_object(&mut state, PlayerId::P1, "Sacred Cat", Zone::Battlefield);

    let offer = engine::advance_until_decision(&mut state);
    assert!(matches!(
        offer,
        Decision::CastSpellOrPass { ref castable_spells, .. } if castable_spells.contains(&dodge)
    ));
    engine::step(&mut state, Action::CastSpell(dodge)).unwrap();
    match engine::advance_until_decision(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(attacker)));
        }
        other => panic!("{other:?}"),
    }
    engine::step(&mut state, Action::ChooseTarget(Target::Object(attacker))).unwrap();
    pass_until_stack_empty(&mut state);
    assert_eq!(state.objects[dodge].zone, Zone::Graveyard);

    // Turn 1: P0 attacks with the now-unblockable Myr Enforcer.
    loop {
        match advance_ignoring_priority(&mut state) {
            Decision::DeclareAttackers { player, eligible } => {
                if player == PlayerId::P0 {
                    assert!(eligible.contains(&attacker));
                    engine::step(&mut state, Action::DeclareAttackers(vec![attacker])).unwrap();
                    break;
                }
                engine::step(&mut state, Action::DeclareAttackers(Vec::new())).unwrap();
            }
            other => panic!("unexpected decision before turn 1 combat: {other:?}"),
        }
    }
    match advance_ignoring_priority(&mut state) {
        Decision::DeclareBlockers {
            attackers,
            legal_blockers,
            ..
        } => {
            assert_eq!(attackers, vec![attacker]);
            let (_, blockers) = legal_blockers
                .iter()
                .find(|(a, _)| *a == attacker)
                .expect("attacker has a legal_blockers entry");
            assert!(
                blockers.is_empty(),
                "Artful Dodge should make the attacker unblockable this turn: {blockers:?}"
            );
            assert!(
                !blockers.contains(&blocker),
                "the untapped 3/3-analog blocker must not be a legal blocker this turn"
            );
            engine::step(&mut state, Action::DeclareBlockers(Vec::new())).unwrap();
        }
        other => panic!("{other:?}"),
    }

    // Advance through the rest of turn 1, all of P1's turn, and into P0's
    // second combat, declining every other attack/block along the way.
    loop {
        match advance_ignoring_priority(&mut state) {
            Decision::DeclareAttackers { player, eligible } => {
                if player == PlayerId::P0 && eligible.contains(&attacker) {
                    engine::step(&mut state, Action::DeclareAttackers(vec![attacker])).unwrap();
                    break;
                }
                engine::step(&mut state, Action::DeclareAttackers(Vec::new())).unwrap();
            }
            Decision::DeclareBlockers { .. } => {
                engine::step(&mut state, Action::DeclareBlockers(Vec::new())).unwrap();
            }
            other => panic!("unexpected decision advancing to turn 2 combat: {other:?}"),
        }
    }
    match advance_ignoring_priority(&mut state) {
        Decision::DeclareBlockers {
            attackers,
            legal_blockers,
            ..
        } => {
            assert_eq!(attackers, vec![attacker]);
            let (_, blockers) = legal_blockers
                .iter()
                .find(|(a, _)| *a == attacker)
                .expect("attacker has a legal_blockers entry");
            assert_eq!(
                blockers,
                &vec![blocker],
                "Artful Dodge's until-end-of-turn grant must have expired by the next turn"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn abandon_attachments_draws_two_only_if_a_card_is_discarded() {
    for accept in [false, true] {
        let mut state = ready_main1(&["Island"; 8], &["Island"; 8]);
        let abandon = put_object(&mut state, PlayerId::P0, "Abandon Attachments", Zone::Hand);
        let fodder = put_object(&mut state, PlayerId::P0, "Island", Zone::Hand);
        put_object(&mut state, PlayerId::P0, "Island", Zone::Battlefield);
        put_object(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);

        let offer = engine::advance_until_decision(&mut state);
        assert!(matches!(
            offer,
            Decision::CastSpellOrPass { ref castable_spells, .. } if castable_spells.contains(&abandon)
        ));
        engine::step(&mut state, Action::CastSpell(abandon)).unwrap();

        match pass_until_next_decision(&mut state) {
            Decision::ChooseOptionalCost {
                player,
                discard_payable,
                ..
            } => {
                assert_eq!(player, PlayerId::P0);
                assert!(discard_payable, "the lone Island in hand is a legal discard");
            }
            other => panic!("{other:?}"),
        }

        if accept {
            engine::step(
                &mut state,
                Action::ChooseOptionalCost(OptionalCostChoice::Discard),
            )
            .unwrap();
            pass_until_stack_empty(&mut state);
            assert_eq!(
                state.objects[fodder].zone,
                Zone::Graveyard,
                "the discarded Island must leave the hand"
            );
            assert_eq!(
                state.players[0].hand.len(),
                2,
                "two cards drawn after discarding"
            );
        } else {
            engine::step(
                &mut state,
                Action::ChooseOptionalCost(OptionalCostChoice::Decline),
            )
            .unwrap();
            pass_until_stack_empty(&mut state);
            assert_eq!(
                state.objects[fodder].zone,
                Zone::Hand,
                "declining pays nothing, so nothing is discarded"
            );
            assert!(state.players[0].hand.contains(&fodder));
            assert_eq!(state.players[0].hand.len(), 1, "no draw on decline");
        }
        assert_eq!(state.objects[abandon].zone, Zone::Graveyard);
    }
}

#[test]
fn acorn_harvest_creates_two_squirrels_and_flashback_costs_three_life() {
    let mut state = ready_main1(&["Island"; 8], &["Island"; 8]);
    let harvest = put_object(&mut state, PlayerId::P0, "Acorn Harvest", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);

    let offer = engine::advance_until_decision(&mut state);
    assert!(matches!(
        offer,
        Decision::CastSpellOrPass { ref castable_spells, .. } if castable_spells.contains(&harvest)
    ));
    engine::step(&mut state, Action::CastSpell(harvest)).unwrap();
    pass_until_stack_empty(&mut state);
    assert_eq!(state.objects[harvest].zone, Zone::Graveyard);
    assert_eq!(state.players[0].life, 20, "no life cost on the normal cast");

    let squirrels: Vec<ObjectId> = state.players[0]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| state.objects[id].name == "Squirrel Token")
        .collect();
    assert_eq!(squirrels.len(), 2, "Acorn Harvest creates two Squirrel tokens");
    for &squirrel in &squirrels {
        let def = &CARD_DEFS[state.objects[squirrel].card_def as usize];
        assert!(def.is_token, "Squirrel Token must be a real token definition");
        assert_eq!((def.power, def.toughness), (Some(1), Some(1)));
        assert_eq!(def.colors, &[mtg_kernel::mana::ManaColor::G]);
    }

    // Flashback {1}{G} plus pay 3 life, from the graveyard.
    put_object(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Island", Zone::Battlefield);
    let offer2 = engine::advance_until_decision(&mut state);
    assert!(
        matches!(
            offer2,
            Decision::CastSpellOrPass { ref castable_spells, .. } if castable_spells.contains(&harvest)
        ),
        "flashback should be offered at life 20 with {{1}}{{G}} available"
    );
    engine::step(&mut state, Action::CastSpell(harvest)).unwrap();
    pass_until_stack_empty(&mut state);
    assert_eq!(state.players[0].life, 17, "flashback pays 3 life on top of {{1}}{{G}}");
    assert_eq!(
        state.objects[harvest].zone,
        Zone::Exile,
        "a resolved flashback cast exiles the card"
    );
    let squirrels_after: Vec<ObjectId> = state.players[0]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| state.objects[id].name == "Squirrel Token")
        .collect();
    assert_eq!(
        squirrels_after.len(),
        4,
        "the flashback cast creates two more Squirrel tokens"
    );

    // Below the 3-life cost, the flashback's PayLife component is
    // unaffordable and is not offered -- same generic `CostComponent::
    // PayLife` legality Deep Analysis's own flashback exercises in
    // `flashback_life_two_is_illegal_life_three_loses_and_life_four_resolves`
    // (`tests/deep_analysis.rs`): a card in the graveyard that can never
    // legally pay any of its cast methods is excluded from
    // `castable_spells` outright, not merely refused if attempted.
    state.players[0].life = 2;
    let offer3 = engine::advance_until_decision(&mut state);
    match offer3 {
        Decision::CastSpellOrPass { ref castable_spells, .. } => {
            assert!(
                !castable_spells.contains(&harvest),
                "at 2 life, the mandatory 3-life flashback cost can't be paid"
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(engine::step(&mut state, Action::CastSpell(harvest)).is_err());
}
