//! Pauper meta wave 2, Task 3: the five utility lands and mana artifacts
//! Urzatron rounds out its manabase with -- Bojuka Bog, Conduit Pylons,
//! Expedition Map, Bonder's Ornament, and Barrels of Blasting Jelly.
//!
//! The bounded rules oracle is the Mage fork at `72a08a3b`:
//! `BojukaBog.java` blob `681976c4d054c43804426a99ffe9bde19c9bc82d`,
//! `ConduitPylons.java` blob `abedb87ee8d4f313bfda191fa88b9ff7088507fe`,
//! `ExpeditionMap.java` blob `dc7a64ba4e8f8473eec8d38d77f53626fb662884`,
//! `BondersOrnament.java` blob `8e36188ee81e41f2c15ef02290234261229d77fe`,
//! `BarrelsOfBlastingJelly.java` blob
//! `9fdd9763dbbb2bfeab2ddc4f81b82540a3a64c16`. Together they establish:
//! Bojuka Bog enters tapped and, on ETB, exiles all cards from target
//! player's graveyard, then taps for {B}; Conduit Pylons is a Desert that
//! surveils 1 on ETB, taps for {C} free, or (paying {1}) for one mana of any
//! color; Expedition Map is `{2}, {T}, Sacrifice`: search for any land card,
//! reveal it, put it into hand, then shuffle; Bonder's Ornament taps for one
//! mana of any color and, for `{4}, {T}`, has each player who controls a
//! permanent named Bonder's Ornament draw a card (a per-player battlefield
//! check, independent of who activates); Barrels of Blasting Jelly's `{1}:
//! Add one mana of any color` has no tap symbol and is capped once each
//! turn, and its `{5}, {T}, Sacrifice` deals 5 damage to target creature.

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::rl::{legal_action_candidates_v1, ActionSemanticV1};
use mtg_kernel::state::{Counters, GameObject, GameState, Step, Target, Zone};
use mtg_kernel::surface_v2::SurfaceDecision;

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

// Copied verbatim from `tests/pauper_meta_w1_creatures.rs:34-62`.
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
        GameState::new_from_libraries(&p0_defs, &p1_defs, card_name, 0x4c_41_4e_44_53_5f_52_4b);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

/// Copied from `tests/pauper_meta_w1_creatures.rs:83-105`, extended to also
/// drive the generic resumable-effect option surface (`Decision::
/// ChooseEffectOption`) is left to callers: unlike `ChooseEffectTargets`/
/// `ChooseEffectBoolean`, an option's meaning is card-specific, so this
/// helper stops there instead of guessing an answer.
fn pass_until_stack_len(state: &mut GameState, wanted: usize) -> Decision {
    for _ in 0..64 {
        let decision = engine::advance_until_decision(state);
        if state.stack.len() <= wanted {
            return decision;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::ChooseEffectTargets { legal_targets, .. } => {
                let target = *legal_targets
                    .first()
                    .expect("a legal target remains to select");
                engine::step(state, Action::ChooseEffectTarget(target)).unwrap();
            }
            Decision::ChooseEffectBoolean { .. } => {
                engine::step(state, Action::ChooseEffectBoolean(false)).unwrap();
            }
            Decision::GameOver { .. } | Decision::Halted { .. } => return decision,
            other => panic!("unexpected decision while resolving the stack: {other:?}"),
        }
    }
    panic!("stack did not reach length {wanted}")
}

fn pass_until_stack_empty(state: &mut GameState) -> Decision {
    pass_until_stack_len(state, 0)
}

/// Passes at every `CastSpellOrPass` decision until a different decision (or
/// a terminal one) is reached. Copied from `pauper_meta_w1_creatures.rs`.
fn pass_until_next_decision(state: &mut GameState) -> Decision {
    loop {
        let decision = engine::advance_until_decision(state);
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => return other,
        }
    }
}

/// The currently offered mana colors for `source`'s printed mana abilities,
/// via the public RL projection (`engine::available_mana_ability_choices`
/// itself is `pub(crate)`). Copied in shape from `tests/spy_mana.rs`'s
/// `mana_candidates`, cloning `state` to get a `CastSpellOrPass` decision
/// without mutating the caller's state.
fn mana_colors(state: &GameState, source: ObjectId) -> Vec<ManaColor> {
    let mut decision_state = state.clone();
    let decision = engine::advance_until_decision(&mut decision_state);
    assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
    let mut colors: Vec<ManaColor> = legal_action_candidates_v1(&SurfaceDecision::Decision(decision), state)
        .expect("legal action projection")
        .into_iter()
        .filter_map(|candidate| match candidate.record.semantic {
            ActionSemanticV1::ActivateManaAbility {
                source: card,
                mana_choice: Some(color),
                ..
            } if card.arena_id == source.0 => Some(color),
            _ => None,
        })
        .collect();
    colors.sort_by_key(|color| *color as u8);
    colors.dedup();
    colors
}

/// Copied from `tests/spy_combo_core.rs`'s `advance_to_targeted_etb`: passes
/// through ordinary priority until the pending trigger whose source is
/// `source` asks for its announced target.
fn advance_to_targeted_etb(state: &mut GameState, source: ObjectId) -> Decision {
    for _ in 0..16 {
        let decision = engine::advance_until_decision(state);
        match decision {
            Decision::ChooseTargets { spell, .. } if spell == source => return decision,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision before targeted ETB: {other:?}"),
        }
    }
    panic!("targeted ETB was not offered")
}

// ---------------------------------------------------------------------
// Bojuka Bog
// ---------------------------------------------------------------------

fn bojuka_bog_state(p0_graveyard: usize, p1_graveyard: usize) -> (GameState, ObjectId) {
    let mut state = ready_main1(&["Swamp"; 7], &["Swamp"; 7]);
    for _ in 0..p0_graveyard {
        put_object(&mut state, PlayerId::P0, "Faerie Seer", Zone::Graveyard);
    }
    for _ in 0..p1_graveyard {
        put_object(&mut state, PlayerId::P1, "Faerie Seer", Zone::Graveyard);
    }
    let bog = put_object(&mut state, PlayerId::P0, "Bojuka Bog", Zone::Hand);
    (state, bog)
}

// covers: Bojuka Bog: enters_tapped, etb_exiles_the_targeted_players_graveyard, target_choice_selects_between_either_player
#[test]
fn bojuka_bog_enters_tapped_and_exiles_a_targeted_graveyard() {
    // Choosing P1 empties P1's three-card graveyard into exile, leaving
    // P0's two-card graveyard untouched.
    let (mut state, bog) = bojuka_bog_state(2, 3);
    engine::step(&mut state, Action::PlayLand(bog)).unwrap();
    assert!(
        state.objects.get(bog).tapped,
        "Bojuka Bog enters the battlefield tapped"
    );
    let target = advance_to_targeted_etb(&mut state, bog);
    let Decision::ChooseTargets { legal_targets, .. } = &target else {
        panic!("expected ChooseTargets, got {target:?}");
    };
    assert_eq!(
        legal_targets,
        &vec![Target::Player(PlayerId::P0), Target::Player(PlayerId::P1)]
    );
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();
    pass_until_stack_empty(&mut state);
    assert_eq!(state.players[1].graveyard.len(), 0, "P1's graveyard is exiled");
    assert_eq!(
        state.players[0].graveyard.len(),
        2,
        "P0's graveyard is untouched"
    );
    assert_eq!(state.exile.len(), 3);

    // Choosing P0 instead empties P0's graveyard, leaving P1's untouched:
    // the target choice, not a fixed player, controls which graveyard is
    // exiled.
    let (mut other, bog2) = bojuka_bog_state(2, 3);
    engine::step(&mut other, Action::PlayLand(bog2)).unwrap();
    advance_to_targeted_etb(&mut other, bog2);
    engine::step(
        &mut other,
        Action::ChooseTarget(Target::Player(PlayerId::P0)),
    )
    .unwrap();
    pass_until_stack_empty(&mut other);
    assert_eq!(other.players[0].graveyard.len(), 0, "P0's graveyard is exiled");
    assert_eq!(
        other.players[1].graveyard.len(),
        3,
        "P1's graveyard is untouched"
    );
    assert_eq!(other.exile.len(), 2);
}

// covers: Bojuka Bog: taps_for_black
#[test]
fn bojuka_bog_taps_for_black_the_turn_after() {
    let (mut state, bog) = bojuka_bog_state(0, 0);
    engine::step(&mut state, Action::PlayLand(bog)).unwrap();
    advance_to_targeted_etb(&mut state, bog);
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P0)),
    )
    .unwrap();
    pass_until_stack_empty(&mut state);
    assert!(state.objects.get(bog).tapped, "still tapped from entering");

    // The turn after: untapped by the untap step, it taps for {B}.
    state.objects.get_mut(bog).tapped = false;
    let choices = mana_colors(&state, bog);
    assert_eq!(choices, vec![ManaColor::B]);
    engine::step(&mut state, Action::ActivateManaAbility(bog)).unwrap();
    assert!(state.objects.get(bog).tapped);
    assert_eq!(state.players[0].mana_pool[ManaColor::B.pool_index()], 1);
}

// ---------------------------------------------------------------------
// Conduit Pylons
// ---------------------------------------------------------------------

fn conduit_pylons_library() -> Vec<&'static str> {
    vec![
        "Forest", "Island", "Mountain", "Swamp", "Forest", "Island", "Mountain", "Swamp",
    ]
}

// covers: Conduit Pylons: etb_surveil_keep_on_top, etb_surveil_put_into_graveyard
#[test]
fn conduit_pylons_surveils_one_on_entry() {
    // Keep on top: the library's top card is unchanged and nothing moves to
    // the graveyard.
    let mut kept = ready_main1(&conduit_pylons_library(), &["Forest"; 8]);
    let top_before = kept.players[0].library[0];
    let pylons = put_object(&mut kept, PlayerId::P0, "Conduit Pylons", Zone::Hand);
    engine::step(&mut kept, Action::PlayLand(pylons)).unwrap();
    let decision = pass_until_next_decision(&mut kept);
    let Decision::ChooseEffectOption {
        option_count,
        source,
        ..
    } = decision
    else {
        panic!("expected ChooseEffectOption, got {decision:?}");
    };
    assert_eq!(option_count, 2, "keep on top or put into the graveyard");
    assert_eq!(source, pylons);
    engine::step(&mut kept, Action::ChooseEffectOption(0)).unwrap();
    pass_until_stack_empty(&mut kept);
    assert_eq!(
        kept.players[0].library.first().copied(),
        Some(top_before),
        "kept on top: library order is unchanged"
    );
    assert!(kept.players[0].graveyard.is_empty());

    // Put into the graveyard: the same card that was on top now sits in the
    // graveyard, and the library's new top is the next card down.
    let mut milled = ready_main1(&conduit_pylons_library(), &["Forest"; 8]);
    let top_before2 = milled.players[0].library[0];
    let pylons2 = put_object(&mut milled, PlayerId::P0, "Conduit Pylons", Zone::Hand);
    engine::step(&mut milled, Action::PlayLand(pylons2)).unwrap();
    pass_until_next_decision(&mut milled);
    engine::step(&mut milled, Action::ChooseEffectOption(1)).unwrap();
    pass_until_stack_empty(&mut milled);
    assert_eq!(
        milled.players[0].graveyard,
        vec![top_before2],
        "put into the graveyard"
    );
    assert_ne!(milled.players[0].library.first().copied(), Some(top_before2));
}

// covers: Conduit Pylons: free_ability_taps_for_colorless, paid_ability_needs_another_source_and_yields_chosen_color
#[test]
fn conduit_pylons_taps_for_colorless_free_and_any_color_for_one() {
    // With no other mana source, only the free colorless ability is
    // payable.
    let mut solo = ready_main1(&["Forest"; 8], &["Forest"; 8]);
    let pylons = put_object(&mut solo, PlayerId::P0, "Conduit Pylons", Zone::Battlefield);
    let choices = mana_colors(&solo, pylons);
    assert_eq!(
        choices,
        vec![ManaColor::C],
        "the paid any-color ability is not offered without another payable source"
    );
    engine::step(&mut solo, Action::ActivateManaAbility(pylons)).unwrap();
    assert!(solo.objects.get(pylons).tapped);
    assert_eq!(solo.players[0].mana_pool[ManaColor::C.pool_index()], 1);

    // With one other payable source (a floating {C}), the paid ability
    // offers a chosen color for {1}.
    let mut paid = ready_main1(&["Forest"; 8], &["Forest"; 8]);
    let pylons2 = put_object(&mut paid, PlayerId::P0, "Conduit Pylons", Zone::Battlefield);
    paid.players[0].mana_pool[ManaColor::C.pool_index()] = 1;
    let choices2 = mana_colors(&paid, pylons2);
    assert!(choices2.contains(&ManaColor::C), "the free ability stays offered");
    for color in [ManaColor::W, ManaColor::U, ManaColor::B, ManaColor::R, ManaColor::G] {
        assert!(choices2.contains(&color), "{color:?} is offered by the paid ability");
    }
    engine::step(
        &mut paid,
        Action::ActivateManaAbilityChoice(pylons2, ManaColor::U),
    )
    .unwrap();
    assert!(paid.objects.get(pylons2).tapped);
    assert_eq!(paid.players[0].mana_pool[ManaColor::U.pool_index()], 1);
    assert_eq!(
        paid.players[0].mana_pool[ManaColor::C.pool_index()],
        0,
        "the {{1}} cost was paid from the floating colorless mana"
    );
}

// ---------------------------------------------------------------------
// Expedition Map
// ---------------------------------------------------------------------

// covers: Expedition Map: fetches_a_basic_land, nonbasic_land_is_an_equally_legal_candidate, sacrificed_after_activation, library_is_shuffled
#[test]
fn expedition_map_fetches_any_land_to_hand() {
    let mut state = ready_main1(
        &[
            "Island",
            "Bojuka Bog",
            "Forest",
            "Mountain",
            "Swamp",
            "Island",
            "Mountain",
            "Forest",
        ],
        &["Forest"; 8],
    );
    let library_before = state.players[0].library.clone();
    let map = put_object(&mut state, PlayerId::P0, "Expedition Map", Zone::Battlefield);
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 2;

    engine::step(&mut state, Action::ActivateAbility(map, 0)).unwrap();
    let decision = pass_until_next_decision(&mut state);
    let Decision::ChooseEffectTargets {
        legal_targets,
        min_targets,
        max_targets,
        ..
    } = &decision
    else {
        panic!("expected ChooseEffectTargets, got {decision:?}");
    };
    assert_eq!((*min_targets, *max_targets), (0, 1));
    let forest = library_before
        .iter()
        .copied()
        .find(|&id| state.objects.get(id).card_def == card_id("Forest"))
        .expect("a Forest is in the library");
    let bojuka_bog = library_before
        .iter()
        .copied()
        .find(|&id| state.objects.get(id).card_def == card_id("Bojuka Bog"))
        .expect("Bojuka Bog is in the library");
    assert!(
        legal_targets.contains(&Target::Object(forest)),
        "a basic land is a legal candidate"
    );
    assert!(
        legal_targets.contains(&Target::Object(bojuka_bog)),
        "a nonbasic land is an equally legal candidate"
    );

    engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(forest))).unwrap();
    pass_until_stack_empty(&mut state);
    assert!(state.players[0].hand.contains(&forest), "found card reaches hand");
    assert_eq!(
        state.objects.get(map).zone,
        Zone::Graveyard,
        "Expedition Map is sacrificed"
    );
    let remaining_before: std::collections::HashSet<ObjectId> = library_before
        .iter()
        .copied()
        .filter(|&id| id != forest)
        .collect();
    let remaining_after: std::collections::HashSet<ObjectId> =
        state.players[0].library.iter().copied().collect();
    assert_eq!(
        remaining_before, remaining_after,
        "the library still holds exactly the un-fetched cards"
    );
    assert_eq!(state.players[0].library.len(), library_before.len() - 1);
    assert_ne!(
        state.players[0].library,
        library_before
            .iter()
            .copied()
            .filter(|&id| id != forest)
            .collect::<Vec<_>>(),
        "the library is shuffled, not left in removal order"
    );
}

// ---------------------------------------------------------------------
// Bonder's Ornament
// ---------------------------------------------------------------------

// covers: Bonder's Ornament: taps_for_any_color, controller_alone_draws_one, both_players_controlling_the_name_both_draw, only_the_opponent_controls_it_and_only_they_draw
#[test]
fn bonders_ornament_taps_for_any_color_and_draws_for_four() {
    // The controller taps for any color.
    let mut mana_check = ready_main1(&["Forest"; 8], &["Forest"; 8]);
    let ornament = put_object(
        &mut mana_check,
        PlayerId::P0,
        "Bonder's Ornament",
        Zone::Battlefield,
    );
    let choices = mana_colors(&mana_check, ornament);
    for color in [ManaColor::W, ManaColor::U, ManaColor::B, ManaColor::R, ManaColor::G] {
        assert!(choices.contains(&color));
    }

    // Only the controller has one: only they draw.
    let mut solo = ready_main1(&["Forest"; 8], &["Forest"; 8]);
    let solo_ornament = put_object(&mut solo, PlayerId::P0, "Bonder's Ornament", Zone::Battlefield);
    solo.players[0].mana_pool[ManaColor::C.pool_index()] = 4;
    let p0_before = solo.players[0].hand.len();
    let p1_before = solo.players[1].hand.len();
    engine::step(&mut solo, Action::ActivateAbility(solo_ornament, 0)).unwrap();
    pass_until_stack_empty(&mut solo);
    assert_eq!(solo.players[0].hand.len(), p0_before + 1);
    assert_eq!(solo.players[1].hand.len(), p1_before);

    // Both players control a Bonder's Ornament: both draw.
    let mut both = ready_main1(&["Forest"; 8], &["Forest"; 8]);
    let mine = put_object(&mut both, PlayerId::P0, "Bonder's Ornament", Zone::Battlefield);
    put_object(&mut both, PlayerId::P1, "Bonder's Ornament", Zone::Battlefield);
    both.players[0].mana_pool[ManaColor::C.pool_index()] = 4;
    let p0_before2 = both.players[0].hand.len();
    let p1_before2 = both.players[1].hand.len();
    engine::step(&mut both, Action::ActivateAbility(mine, 0)).unwrap();
    pass_until_stack_empty(&mut both);
    assert_eq!(both.players[0].hand.len(), p0_before2 + 1);
    assert_eq!(both.players[1].hand.len(), p1_before2 + 1);

    // Only the opponent controls one, and the activating player's own is
    // gone: the Java's per-player battlefield check means only the
    // opponent draws, even though the opponent didn't activate.
    let mut opponent_only = ready_main1(&["Forest"; 8], &["Forest"; 8]);
    let theirs = put_object(
        &mut opponent_only,
        PlayerId::P1,
        "Bonder's Ornament",
        Zone::Battlefield,
    );
    opponent_only.players[1].mana_pool[ManaColor::C.pool_index()] = 4;
    opponent_only.active_player = PlayerId::P1;
    opponent_only.priority_player = PlayerId::P1;
    let p0_before3 = opponent_only.players[0].hand.len();
    let p1_before3 = opponent_only.players[1].hand.len();
    engine::step(&mut opponent_only, Action::ActivateAbility(theirs, 0)).unwrap();
    pass_until_stack_empty(&mut opponent_only);
    assert_eq!(
        opponent_only.players[0].hand.len(),
        p0_before3,
        "P0 controls no Bonder's Ornament, so P0 does not draw"
    );
    assert_eq!(opponent_only.players[1].hand.len(), p1_before3 + 1);
}

// ---------------------------------------------------------------------
// Barrels of Blasting Jelly
// ---------------------------------------------------------------------

// covers: Barrels of Blasting Jelly: tapless_ability_offered_then_capped_once_per_turn, cap_resets_next_turn
#[test]
fn barrels_of_blasting_jelly_adds_any_color_once_per_turn() {
    let mut state = ready_main1(&["Forest"; 8], &["Forest"; 8]);
    let barrels = put_object(
        &mut state,
        PlayerId::P0,
        "Barrels of Blasting Jelly",
        Zone::Battlefield,
    );
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 2;

    let choices = mana_colors(&state, barrels);
    for color in [ManaColor::W, ManaColor::U, ManaColor::B, ManaColor::R, ManaColor::G] {
        assert!(choices.contains(&color), "offered before use this turn");
    }

    engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(barrels, ManaColor::U),
    )
    .unwrap();
    assert!(
        !state.objects.get(barrels).tapped,
        "the ability has no tap symbol"
    );
    assert_eq!(state.players[0].mana_pool[ManaColor::U.pool_index()], 1);
    assert_eq!(
        state.players[0].mana_pool[ManaColor::C.pool_index()],
        1,
        "the {{1}} cost was paid, one colorless left floating"
    );

    let after_use = mana_colors(&state, barrels);
    assert!(
        after_use.is_empty(),
        "capped at once per turn: no colors remain offered"
    );
    assert!(engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(barrels, ManaColor::W)
    )
    .is_err());

    // The cap resets next turn (`ability_uses_this_turn` clears in the
    // Step::Untap entry handler, engine.rs's `run_step_entry_action` around
    // line 9961, reached here by walking from Cleanup; same mechanism
    // `spy_mana.rs`'s Wall of Roots test drives).
    state.step = Step::Cleanup;
    state.active_player = PlayerId::P1;
    state.priority_player = PlayerId::P1;
    let _ = engine::advance_until_decision(&mut state);
    assert!(state.objects.get(barrels).v4.ability_uses_this_turn.is_empty());
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    // Every step transition empties floating mana pools (601.2h-adjacent
    // cleanup, driven by the same transition this test just walked
    // through); refloat the {1} cost so the reset is visible through the
    // color offer, not masked by an unrelated unpaid cost.
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 1;
    let reset_choices = mana_colors(&state, barrels);
    assert!(
        reset_choices.contains(&ManaColor::W),
        "the per-turn cap reset for the new turn"
    );
}

// covers: Barrels of Blasting Jelly: damage_ability_deals_five_to_target_creature, sacrificed_after_the_damage_ability
#[test]
fn barrels_of_blasting_jelly_deals_five_to_a_creature() {
    let mut state = ready_main1(&["Forest"; 8], &["Forest"; 8]);
    let barrels = put_object(
        &mut state,
        PlayerId::P0,
        "Barrels of Blasting Jelly",
        Zone::Battlefield,
    );
    // Murmuring Mystic: 1/5, a five-toughness creature killed exactly by 5
    // damage.
    let creature = put_object(&mut state, PlayerId::P1, "Murmuring Mystic", Zone::Battlefield);
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 5;

    engine::step(&mut state, Action::ActivateAbility(barrels, 0)).unwrap();
    let decision = pass_until_next_decision(&mut state);
    let Decision::ChooseTargets { legal_targets, .. } = &decision else {
        panic!("expected ChooseTargets, got {decision:?}");
    };
    assert_eq!(legal_targets, &vec![Target::Object(creature)]);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(creature))).unwrap();
    pass_until_stack_empty(&mut state);

    assert_eq!(
        state.objects.get(creature).zone,
        Zone::Graveyard,
        "5 damage kills the five-toughness creature"
    );
    assert_eq!(
        state.objects.get(barrels).zone,
        Zone::Graveyard,
        "Barrels of Blasting Jelly is sacrificed as part of its own cost"
    );
}
