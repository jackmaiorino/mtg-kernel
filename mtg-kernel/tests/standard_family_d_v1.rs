//! MageZero Standard family D: newer set keywords (prowess, valiant, speed,
//! warp, offspring, plot, crime and friends) in the Standard catalog.
#![cfg(feature = "standard-magezero-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, Keywords, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::standard_keywords_v1::speed;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, SpeedV1, Step, Target, Zone};

/// A main-phase state with `library` cards in both libraries, priority to P0.
fn ready_with(library: &str) -> GameState {
    let card = card_id_by_name(library).unwrap();
    let name = library.to_owned();
    let mut state =
        GameState::new_from_libraries(&[card; 30], &[card; 30], move |_| name.clone(), 0x4644);
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state.step = Step::Main1;
    state
}

fn ready() -> GameState {
    ready_with("Mountain")
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    let seat = &mut state.players[player.index()];
    match zone {
        Zone::Hand => seat.hand.push(id),
        Zone::Battlefield => seat.battlefield.push(id),
        Zone::Graveyard => seat.graveyard.push(id),
        _ => panic!("helper zone"),
    }
    id
}

fn add_mana(state: &mut GameState, player: PlayerId, colored: &[ManaColor], generic: u8) {
    let pool = &mut state.players[player.index()].mana_pool;
    for color in colored {
        pool[color.pool_index()] += 1;
    }
    pool[5] += generic;
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

/// Passes priority and orders triggers until the stack and pending triggers
/// are empty, returning the first other decision if one interrupts.
fn settle(state: &mut GameState) -> Option<Decision> {
    for _ in 0..100 {
        match next(state) {
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return None
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => return Some(other),
        }
    }
    panic!("did not settle");
}

fn settled(state: &mut GameState) {
    if let Some(other) = settle(state) {
        panic!("unexpected choice: {other:?}");
    }
}

/// Casts `spell`, answering each target prompt from `targets` in order.
fn cast(state: &mut GameState, spell: ObjectId, targets: &[Target]) {
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell)),
        "{} not castable",
        state.objects.get(spell).name
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    for target in targets {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(
                    legal_targets.contains(target),
                    "{target:?} not in {legal_targets:?}"
                );
                engine::step(state, Action::ChooseTarget(*target)).unwrap();
            }
            other => panic!("expected targets, got {other:?}"),
        }
    }
}

fn power_toughness(state: &GameState, id: ObjectId) -> (i32, i32) {
    (
        engine::effective_power(state, id),
        engine::effective_toughness(state, id),
    )
}

#[test]
fn family_d_cards_are_fully_supported() {
    for name in [
        "Emberheart Challenger",
        "Nova Hellkite",
        "Full Bore",
        "Iridescent Vinelasher",
        "Iridescent Vinelasher Offspring Token",
        "Aloe Alchemist",
        "Forsaken Miner",
        "Chrome Host Seedshark",
        "Incubator Token",
        "Monastery Swiftspear",
        "Heartfire Hero",
        "Slickshot Show-Off",
        "Sanguine Evangelist",
        "Bat Token",
        "Darkstar Augur",
        "Darkstar Augur Offspring Token",
        "Ruin-Lurker Bat",
        "Pawpatch Recruit",
        "Pawpatch Recruit Offspring Token",
        "Manifold Mouse",
        "Manifold Mouse Offspring Token",
        "Yotian Frontliner",
        "Cori-Steel Cutter",
        "Monk Token",
        "White Insect Token",
        "Phantom Interference",
        "Spirit Token",
    ] {
        let id = card_id_by_name(name).unwrap_or_else(|| panic!("{name} missing"));
        assert_eq!(
            CARD_DEFS[id as usize].capability,
            CardCapability::Full,
            "{name}"
        );
    }
}

#[test]
fn incomplete_keyword_cards_remain_partial() {
    for name in [
        "Enduring Curiosity",
        "Enduring Innocence",
        "Overlord of the Mistmoors",
        "Axebane Ferox",
        "Brutal Cathar",
        "Burnout Bashtronaut",
        "Graveyard Trespasser",
        "Hopeful Initiate",
        "Knight-Errant of Eos",
        "Make Disappear",
    ] {
        let id = card_id_by_name(name).expect(name);
        assert_eq!(
            CARD_DEFS[id as usize].capability,
            CardCapability::Partial,
            "{name}"
        );
    }
}

#[test]
fn flourishing_bloom_kin_capability_awaits_combined_validation() {
    let id = card_id_by_name("Flourishing Bloom-Kin").expect("Flourishing Bloom-Kin");
    assert_eq!(CARD_DEFS[id as usize].capability, CardCapability::Partial);
}

#[test]
fn emberheart_challenger_has_prowess_and_valiant() {
    let mut state = ready();
    let challenger = put(
        &mut state,
        PlayerId::P0,
        "Emberheart Challenger",
        Zone::Battlefield,
    );
    assert!(engine::has_effective_keyword(
        &state,
        challenger,
        Keywords::HASTE
    ));
    let first = put(&mut state, PlayerId::P0, "Burst Lightning", Zone::Hand);
    let second = put(&mut state, PlayerId::P0, "Burst Lightning", Zone::Hand);
    let library = state.players[0].library.len();

    // Targeting it with your own noncreature spell triggers both prowess and
    // valiant; prowess resolves first, so the 2 damage doesn't kill it.
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 0);
    cast(&mut state, first, &[Target::Object(challenger)]);
    settled(&mut state);
    assert_eq!(state.objects.get(challenger).zone, Zone::Battlefield);
    assert_eq!(power_toughness(&state, challenger), (3, 3));
    assert_eq!(state.objects.get(challenger).damage, 2);
    assert_eq!(state.players[0].library.len(), library - 1);
    let exiled = state.exile.clone();
    assert_eq!(exiled.len(), 1);
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { land_drops, .. } if land_drops.contains(&exiled[0]))
    );

    // The second targeting this turn is not the first: only prowess.
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 0);
    cast(&mut state, second, &[Target::Player(PlayerId::P1)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, challenger), (4, 4));
    assert_eq!(state.exile.len(), 1);
    assert_eq!(state.players[1].life, 18);
}

#[test]
fn valiant_ignores_opponents_targeting() {
    let mut state = ready();
    let challenger = put(
        &mut state,
        PlayerId::P0,
        "Emberheart Challenger",
        Zone::Battlefield,
    );
    let burst = put(&mut state, PlayerId::P1, "Burst Lightning", Zone::Hand);
    state.priority_player = PlayerId::P1;
    add_mana(&mut state, PlayerId::P1, &[ManaColor::R], 0);
    cast(&mut state, burst, &[Target::Object(challenger)]);
    settled(&mut state);
    assert_eq!(state.objects.get(challenger).zone, Zone::Graveyard);
    assert!(state.exile.is_empty());
}

fn burn(state: &mut GameState, player: PlayerId, target: Target) {
    let burst = put(state, player, "Burst Lightning", Zone::Hand);
    add_mana(state, player, &[ManaColor::R], 0);
    cast(state, burst, &[target]);
    settled(state);
}

fn burn_with_ward(state: &mut GameState, player: PlayerId, target: Target, answer: Action) {
    state.priority_player = player;
    let spell = put(state, player, "Burst Lightning", Zone::Hand);
    add_mana(state, player, &[ManaColor::R], 0);
    cast(state, spell, &[target]);
    assert!(matches!(
        settle(state),
        Some(Decision::ChooseEffectTargets { .. } | Decision::ChooseEffectBoolean { .. })
    ));
    engine::step(state, answer).unwrap();
    settled(state);
}

#[test]
fn burnout_bashtronaut_starts_and_raises_speed_once_per_turn() {
    let mut state = ready();
    let bashtronaut = put(&mut state, PlayerId::P0, "Burnout Bashtronaut", Zone::Hand);
    assert_eq!(speed(&state, PlayerId::P0), 0);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 0);
    cast(&mut state, bashtronaut, &[]);
    settled(&mut state);
    assert!(engine::has_effective_keyword(
        &state,
        bashtronaut,
        Keywords::MENACE
    ));
    assert_eq!(speed(&state, PlayerId::P0), 1);
    assert_eq!(speed(&state, PlayerId::P1), 0);

    burn(&mut state, PlayerId::P0, Target::Player(PlayerId::P1));
    assert_eq!(speed(&state, PlayerId::P0), 2);
    burn(&mut state, PlayerId::P0, Target::Player(PlayerId::P1));
    assert_eq!(speed(&state, PlayerId::P0), 2, "only once each turn");

    // An opponent losing life during the opponent's own turn doesn't count.
    state.turn += 1;
    state.active_player = PlayerId::P1;
    state.priority_player = PlayerId::P1;
    burn(&mut state, PlayerId::P1, Target::Player(PlayerId::P1));
    assert_eq!(speed(&state, PlayerId::P0), 2);
}

#[test]
fn burnout_bashtronaut_pumps_and_has_double_strike_at_max_speed() {
    let mut state = ready();
    let bashtronaut = put(
        &mut state,
        PlayerId::P0,
        "Burnout Bashtronaut",
        Zone::Battlefield,
    );
    mtg_kernel::trigger::sba_fixed_point(&mut state);
    assert_eq!(speed(&state, PlayerId::P0), 1);
    assert!(!engine::has_effective_keyword(
        &state,
        bashtronaut,
        Keywords::DOUBLE_STRIKE
    ));
    state.speed_v1 = Some(SpeedV1 {
        sources: [None; 2],
        speeds: [4, 0],
        last_increase: None,
    });
    assert!(engine::has_effective_keyword(
        &state,
        bashtronaut,
        Keywords::DOUBLE_STRIKE
    ));
    burn(&mut state, PlayerId::P0, Target::Player(PlayerId::P1));
    assert_eq!(speed(&state, PlayerId::P0), 4, "speed can't exceed four");

    add_mana(&mut state, PlayerId::P0, &[], 2);
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { activatable_abilities, .. } if activatable_abilities.contains(&(bashtronaut, 0)))
    );
    engine::step(&mut state, Action::ActivateAbility(bashtronaut, 0)).unwrap();
    settled(&mut state);
    assert_eq!(power_toughness(&state, bashtronaut), (2, 1));
}

/// Passes priority (attacking and blocking with nothing) until `until` holds
/// at a priority decision.
fn pass_until(state: &mut GameState, until: impl Fn(&GameState) -> bool) {
    for _ in 0..500 {
        match next(state) {
            Decision::CastSpellOrPass { .. } if until(state) => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::DeclareAttackers { .. } => {
                engine::step(state, Action::DeclareAttackers(vec![])).unwrap()
            }
            Decision::DeclareBlockers { .. } => {
                engine::step(state, Action::DeclareBlockers(vec![])).unwrap()
            }
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    panic!("condition never reached");
}

fn castable(state: &mut GameState, id: ObjectId) -> bool {
    matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&id))
}

#[test]
fn nova_hellkite_warps_in_then_returns_from_exile_on_a_later_turn() {
    let mut state = ready();
    let hellkite = put(&mut state, PlayerId::P0, "Nova Hellkite", Zone::Hand);
    let victim = put(
        &mut state,
        PlayerId::P1,
        "Burnout Bashtronaut",
        Zone::Battlefield,
    );
    let full_bore = put(&mut state, PlayerId::P0, "Full Bore", Zone::Hand);

    // Only the warp cost is affordable, so it is chosen without a prompt.
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 2);
    cast(&mut state, hellkite, &[]);
    // The enters trigger targets the only opposing creature.
    let Some(Decision::ChooseTargets { legal_targets, .. }) = settle(&mut state) else {
        panic!("expected the enters trigger's target");
    };
    assert_eq!(legal_targets, vec![Target::Object(victim)]);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(victim))).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(hellkite).zone, Zone::Battlefield);
    assert!(state.objects.get(hellkite).v4.warped_v1);
    assert_eq!(state.objects.get(victim).zone, Zone::Graveyard);

    // Full Bore sees the warp cast: +3/+2, trample and haste.
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 0);
    cast(&mut state, full_bore, &[Target::Object(hellkite)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, hellkite), (7, 7));
    assert!(engine::has_effective_keyword(
        &state,
        hellkite,
        Keywords::TRAMPLE
    ));

    // At the beginning of the end step it is exiled with a later-turn permission.
    pass_until(&mut state, |s| s.step == Step::End && s.stack.is_empty());
    assert_eq!(state.objects.get(hellkite).zone, Zone::Exile);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R, ManaColor::R], 3);
    assert!(!castable(&mut state, hellkite), "not this turn");
    state.players[0].mana_pool = [0; 6];

    pass_until(&mut state, |s| {
        s.active_player == PlayerId::P0 && s.step == Step::Main1
    });
    // From exile it is cast for its normal cost only; warp needs the hand.
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 2);
    assert!(!castable(&mut state, hellkite));
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 0);
    assert!(!castable(&mut state, hellkite));
    add_mana(&mut state, PlayerId::P0, &[], 1);
    cast(&mut state, hellkite, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(hellkite).zone, Zone::Battlefield);
    assert!(!state.objects.get(hellkite).v4.warped_v1);
    pass_until(&mut state, |s| s.step == Step::End && s.stack.is_empty());
    assert_eq!(state.objects.get(hellkite).zone, Zone::Battlefield);
}

#[test]
fn nova_hellkite_offers_both_costs_when_both_are_affordable() {
    let mut state = ready();
    let hellkite = put(&mut state, PlayerId::P0, "Nova Hellkite", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R, ManaColor::R], 3);
    assert!(castable(&mut state, hellkite));
    engine::step(&mut state, Action::CastSpell(hellkite)).unwrap();
    let Decision::ChooseCastMode { options, .. } = next(&mut state) else {
        panic!("expected a cast mode choice");
    };
    assert_eq!(options.len(), 2);
    engine::step(&mut state, Action::ChooseCastMode(engine::CastMode::Normal)).unwrap();
    settled(&mut state);
    assert!(!state.objects.get(hellkite).v4.warped_v1);
    // A normally cast Hellkite stays, and Full Bore grants it no trample.
    let full_bore = put(&mut state, PlayerId::P0, "Full Bore", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 0);
    cast(&mut state, full_bore, &[Target::Object(hellkite)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, hellkite), (7, 7));
    assert!(!engine::has_effective_keyword(
        &state,
        hellkite,
        Keywords::TRAMPLE
    ));
}

fn cast_vinelasher(state: &mut GameState, offspring: bool) -> ObjectId {
    let vinelasher = put(state, PlayerId::P0, "Iridescent Vinelasher", Zone::Hand);
    add_mana(state, PlayerId::P0, &[ManaColor::B], 2);
    assert!(castable(state, vinelasher));
    engine::step(state, Action::CastSpell(vinelasher)).unwrap();
    assert!(matches!(next(state), Decision::ChooseKicker { spell, .. } if spell == vinelasher));
    engine::step(state, Action::ChooseKicker(offspring)).unwrap();
    settled(state);
    state.players[0].mana_pool = [0; 6];
    vinelasher
}

fn controlled_vinelashers(state: &GameState) -> Vec<ObjectId> {
    state.players[0]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| state.objects.get(id).name == "Iridescent Vinelasher")
        .collect()
}

#[test]
fn iridescent_vinelasher_offspring_makes_a_one_one_copy_only_when_paid() {
    let mut state = ready();
    let vinelasher = cast_vinelasher(&mut state, false);
    assert_eq!(controlled_vinelashers(&state), vec![vinelasher]);

    let mut state = ready();
    let vinelasher = cast_vinelasher(&mut state, true);
    let both = controlled_vinelashers(&state);
    assert_eq!(both.len(), 2);
    let token = *both.iter().find(|&&id| id != vinelasher).unwrap();
    assert_eq!(power_toughness(&state, vinelasher), (1, 2));
    assert_eq!(power_toughness(&state, token), (1, 1));
    assert!(CARD_DEFS[state.objects.get(token).card_def as usize].is_token);
}

#[test]
fn iridescent_vinelasher_landfall_pings_target_opponent_from_each_copy() {
    let mut state = ready();
    cast_vinelasher(&mut state, true);
    let land = put(&mut state, PlayerId::P0, "Mountain", Zone::Hand);
    engine::step(&mut state, Action::PlayLand(land)).unwrap();
    // Two landfall triggers, each targeting the only opponent.
    for _ in 0..2 {
        match settle(&mut state) {
            Some(Decision::ChooseTargets { legal_targets, .. }) => {
                assert_eq!(legal_targets, vec![Target::Player(PlayerId::P1)]);
                engine::step(
                    &mut state,
                    Action::ChooseTarget(Target::Player(PlayerId::P1)),
                )
                .unwrap();
            }
            other => panic!("expected a landfall target, got {other:?}"),
        }
    }
    settled(&mut state);
    assert_eq!(state.players[1].life, 18);
    assert_eq!(state.players[0].life, 20);

    // An opponent's land doesn't trigger it.
    state.turn += 1;
    state.active_player = PlayerId::P1;
    state.priority_player = PlayerId::P1;
    let land = put(&mut state, PlayerId::P1, "Mountain", Zone::Hand);
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    engine::step(&mut state, Action::PlayLand(land)).unwrap();
    settled(&mut state);
    assert_eq!(state.players[1].life, 18);
}

#[test]
fn aloe_alchemist_pumps_target_creature_when_it_becomes_plotted() {
    let mut state = ready();
    let bashtronaut = put(
        &mut state,
        PlayerId::P0,
        "Burnout Bashtronaut",
        Zone::Battlefield,
    );
    let alchemist = put(&mut state, PlayerId::P0, "Aloe Alchemist", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::G], 1);
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { plot_actions, .. } if plot_actions.contains(&alchemist))
    );
    engine::step(&mut state, Action::PlotSpell(alchemist)).unwrap();
    assert_eq!(state.objects.get(alchemist).zone, Zone::Exile);
    let Some(Decision::ChooseTargets { legal_targets, .. }) = settle(&mut state) else {
        panic!("expected the plotted trigger's target");
    };
    assert_eq!(legal_targets, vec![Target::Object(bashtronaut)]);
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Object(bashtronaut)),
    )
    .unwrap();
    settled(&mut state);
    assert_eq!(power_toughness(&state, bashtronaut), (4, 3));
    assert!(engine::has_effective_keyword(
        &state,
        bashtronaut,
        Keywords::TRAMPLE
    ));

    // Cast later for free from exile; entering doesn't trigger it again.
    assert!(
        !castable(&mut state, alchemist),
        "not the turn it was plotted"
    );
    let plotted_turn = state.turn;
    pass_until(&mut state, |s| {
        s.turn > plotted_turn && s.active_player == PlayerId::P0 && s.step == Step::Main1
    });
    cast(&mut state, alchemist, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(alchemist).zone, Zone::Battlefield);
    assert!(engine::has_effective_keyword(
        &state,
        alchemist,
        Keywords::TRAMPLE
    ));
    assert_eq!(power_toughness(&state, bashtronaut), (1, 1));
}

/// Answers the next optional-payment prompt, then settles.
fn answer_payment(state: &mut GameState, pay: bool) {
    match settle(state) {
        Some(Decision::ChooseEffectBoolean { player, .. }) => {
            assert_eq!(player, PlayerId::P0);
            engine::step(state, Action::ChooseEffectBoolean(pay)).unwrap();
        }
        other => panic!("expected an optional payment, got {other:?}"),
    }
    settled(state);
}

#[test]
fn forsaken_miner_returns_when_you_commit_a_crime_and_pay() {
    let mut state = ready();
    let miner = put(&mut state, PlayerId::P0, "Forsaken Miner", Zone::Graveyard);
    let own = put(
        &mut state,
        PlayerId::P0,
        "Burnout Bashtronaut",
        Zone::Battlefield,
    );

    // Targeting your own creature is no crime.
    burn(&mut state, PlayerId::P0, Target::Object(own));
    assert_eq!(state.objects.get(miner).zone, Zone::Graveyard);

    // Targeting the opponent is; declining leaves it in the graveyard.
    let burst = put(&mut state, PlayerId::P0, "Burst Lightning", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R, ManaColor::B], 0);
    cast(&mut state, burst, &[Target::Player(PlayerId::P1)]);
    answer_payment(&mut state, false);
    assert_eq!(state.objects.get(miner).zone, Zone::Graveyard);

    // Targeting an opposing creature with B open: pay and it returns.
    let theirs = put(
        &mut state,
        PlayerId::P1,
        "Burnout Bashtronaut",
        Zone::Battlefield,
    );
    let burst = put(&mut state, PlayerId::P0, "Burst Lightning", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 0);
    cast(&mut state, burst, &[Target::Object(theirs)]);
    answer_payment(&mut state, true);
    assert_eq!(state.objects.get(miner).zone, Zone::Battlefield);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

#[test]
fn opponents_crimes_do_not_return_forsaken_miner() {
    let mut state = ready();
    let miner = put(&mut state, PlayerId::P0, "Forsaken Miner", Zone::Graveyard);
    state.priority_player = PlayerId::P1;
    add_mana(&mut state, PlayerId::P0, &[ManaColor::B], 0);
    let burst = put(&mut state, PlayerId::P1, "Burst Lightning", Zone::Hand);
    add_mana(&mut state, PlayerId::P1, &[ManaColor::R], 0);
    cast(&mut state, burst, &[Target::Player(PlayerId::P0)]);
    settled(&mut state);
    assert_eq!(state.objects.get(miner).zone, Zone::Graveyard);
}

#[test]
fn forsaken_miner_cant_block() {
    let mut state = ready();
    let miner = put(
        &mut state,
        PlayerId::P1,
        "Forsaken Miner",
        Zone::Battlefield,
    );
    let attacker = put(
        &mut state,
        PlayerId::P0,
        "Emberheart Challenger",
        Zone::Battlefield,
    );
    let other = put(
        &mut state,
        PlayerId::P1,
        "Burnout Bashtronaut",
        Zone::Battlefield,
    );
    pass_until_blocks(&mut state, attacker, |blockers| {
        assert!(!blockers.contains(&miner));
        assert!(blockers.contains(&other));
    });
}

fn pass_until_blocks(state: &mut GameState, attacker: ObjectId, check: impl Fn(&[ObjectId])) {
    for _ in 0..50 {
        match next(state) {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::DeclareAttackers { .. } => {
                engine::step(state, Action::DeclareAttackers(vec![attacker])).unwrap()
            }
            Decision::DeclareBlockers { legal_blockers, .. } => {
                let (_, blockers) = legal_blockers
                    .iter()
                    .find(|(id, _)| *id == attacker)
                    .expect("the attacker is listed");
                check(blockers);
                return;
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    panic!("no block step");
}

#[test]
fn axebane_ferox_ward_counters_unless_evidence_is_collected() {
    let mut state = ready();
    let ferox = put(&mut state, PlayerId::P0, "Axebane Ferox", Zone::Battlefield);
    for keyword in [Keywords::DEATHTOUCH, Keywords::HASTE] {
        assert!(engine::has_effective_keyword(&state, ferox, keyword));
    }
    state.priority_player = PlayerId::P1;

    // Three mana value of evidence is not enough: the spell is countered.
    let hellkite = put(&mut state, PlayerId::P1, "Nova Hellkite", Zone::Graveyard);
    let first = put(&mut state, PlayerId::P1, "Burst Lightning", Zone::Hand);
    add_mana(&mut state, PlayerId::P1, &[ManaColor::R], 0);
    let graveyard = state.players[1].graveyard.clone();
    state.players[1].graveyard.clear();
    let bashtronauts: Vec<_> = (0..3)
        .map(|_| {
            put(
                &mut state,
                PlayerId::P1,
                "Burnout Bashtronaut",
                Zone::Graveyard,
            )
        })
        .collect();
    cast(&mut state, first, &[Target::Object(ferox)]);
    settled(&mut state);
    assert_eq!(state.objects.get(ferox).damage, 0, "countered by ward");
    assert_eq!(state.objects.get(first).zone, Zone::Graveyard);
    for id in &bashtronauts {
        assert_eq!(state.objects.get(*id).zone, Zone::Graveyard);
    }

    // With the five-drop back the payer can pay, and exiles the smallest
    // total that reaches four: the three one-drops and the first Burst
    // (total 4) rather than Hellkite (total 5).
    state.players[1].graveyard.extend(graveyard);
    state.priority_player = PlayerId::P1;
    let second = put(&mut state, PlayerId::P1, "Burst Lightning", Zone::Hand);
    add_mana(&mut state, PlayerId::P1, &[ManaColor::R], 0);
    cast(&mut state, second, &[Target::Object(ferox)]);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets {
            player: PlayerId::P1,
            can_finish: true,
            ..
        })
    ));
    for card in bashtronauts.iter().chain(std::iter::once(&first)) {
        engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(*card)),
        )
        .unwrap();
    }
    engine::step(&mut state, Action::FinishEffectSelection).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(ferox).damage, 2);
    let exiled: Vec<_> = state.exile.clone();
    assert_eq!(exiled.len(), 4, "{exiled:?}");
    assert_eq!(state.objects.get(hellkite).zone, Zone::Graveyard);
}

#[test]
fn axebane_ferox_ward_ignores_its_controllers_spells() {
    let mut state = ready();
    let ferox = put(&mut state, PlayerId::P0, "Axebane Ferox", Zone::Battlefield);
    burn(&mut state, PlayerId::P0, Target::Object(ferox));
    assert_eq!(state.objects.get(ferox).damage, 2);
}

fn attack_with(state: &mut GameState, attackers: Vec<ObjectId>) {
    pass_until(state, |s| s.step == Step::BeginCombat);
    for _ in 0..20 {
        match next(state) {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::DeclareAttackers { .. } => {
                engine::step(state, Action::DeclareAttackers(attackers)).unwrap();
                return;
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    panic!("no attack declaration");
}

#[test]
fn hopeful_initiate_trains_only_beside_a_stronger_attacker() {
    let mut state = ready();
    let initiate = put(
        &mut state,
        PlayerId::P0,
        "Hopeful Initiate",
        Zone::Battlefield,
    );
    let challenger = put(
        &mut state,
        PlayerId::P0,
        "Emberheart Challenger",
        Zone::Battlefield,
    );
    attack_with(&mut state, vec![initiate, challenger]);
    settled(&mut state);
    assert_eq!(state.objects.get(initiate).counters.plus1_plus1, 1);
    assert_eq!(state.objects.get(challenger).counters.plus1_plus1, 0);

    // Alone (or beside an equal-power creature) it doesn't train.
    let mut state = ready();
    let initiate = put(
        &mut state,
        PlayerId::P0,
        "Hopeful Initiate",
        Zone::Battlefield,
    );
    attack_with(&mut state, vec![initiate]);
    settled(&mut state);
    assert_eq!(state.objects.get(initiate).counters.plus1_plus1, 0);
}

#[test]
fn hopeful_initiate_removes_two_counters_to_destroy_an_artifact() {
    let mut state = ready();
    let initiate = put(
        &mut state,
        PlayerId::P0,
        "Hopeful Initiate",
        Zone::Battlefield,
    );
    let challenger = put(
        &mut state,
        PlayerId::P0,
        "Emberheart Challenger",
        Zone::Battlefield,
    );
    let wellspring = put(
        &mut state,
        PlayerId::P1,
        "Ichor Wellspring",
        Zone::Battlefield,
    );
    state.objects.get_mut(initiate).counters.plus1_plus1 = 1;
    add_mana(&mut state, PlayerId::P0, &[ManaColor::W], 2);
    let offered = |state: &mut GameState| matches!(next(state), Decision::CastSpellOrPass { activatable_abilities, .. } if activatable_abilities.contains(&(initiate, 0)));
    assert!(!offered(&mut state), "only one counter among creatures");

    state.objects.get_mut(challenger).counters.plus1_plus1 = 2;
    assert!(offered(&mut state));
    engine::step(&mut state, Action::ActivateAbility(initiate, 0)).unwrap();
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(wellspring)]);
            engine::step(&mut state, Action::ChooseTarget(Target::Object(wellspring))).unwrap();
        }
        other => panic!("expected a target, got {other:?}"),
    }
    assert!(matches!(
        next(&mut state),
        Decision::ChooseCostTargets { remaining: 2, .. }
    ));
    engine::step(&mut state, Action::ChooseCostTarget(challenger)).unwrap();
    assert!(matches!(
        next(&mut state),
        Decision::ChooseCostTargets { remaining: 1, .. }
    ));
    engine::step(&mut state, Action::ChooseCostTarget(challenger)).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(wellspring).zone, Zone::Graveyard);
    // Both counters can come from the same chosen creature.
    assert_eq!(state.objects.get(challenger).counters.plus1_plus1, 0);
    assert_eq!(state.objects.get(initiate).counters.plus1_plus1, 1);
}

fn incubators(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| state.objects.get(id).name == "Incubator Token")
        .collect()
}

#[test]
fn chrome_host_seedshark_incubates_noncreature_spells_mana_value() {
    let mut state = ready();
    let seedshark = put(
        &mut state,
        PlayerId::P0,
        "Chrome Host Seedshark",
        Zone::Battlefield,
    );
    assert!(engine::has_effective_keyword(
        &state,
        seedshark,
        Keywords::FLYING
    ));

    // A creature spell doesn't trigger it.
    let bashtronaut = put(&mut state, PlayerId::P0, "Burnout Bashtronaut", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 0);
    cast(&mut state, bashtronaut, &[]);
    settled(&mut state);
    assert!(incubators(&state, PlayerId::P0).is_empty());

    // Full Bore (mana value 1) makes an Incubator with one counter.
    let full_bore = put(&mut state, PlayerId::P0, "Full Bore", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 0);
    cast(&mut state, full_bore, &[Target::Object(seedshark)]);
    settled(&mut state);
    let [incubator] = incubators(&state, PlayerId::P0)[..] else {
        panic!("expected one Incubator");
    };
    assert_eq!(state.objects.get(incubator).counters.plus1_plus1, 1);
    assert!(!engine::object_has_type(
        &state,
        incubator,
        mtg_kernel::card_def::CardType::Creature
    ));

    // {2}: it transforms into a 1/1 Phyrexian artifact creature, once.
    add_mana(&mut state, PlayerId::P0, &[], 4);
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { activatable_abilities, .. } if activatable_abilities.contains(&(incubator, 0)))
    );
    engine::step(&mut state, Action::ActivateAbility(incubator, 0)).unwrap();
    settled(&mut state);
    assert!(engine::object_has_type(
        &state,
        incubator,
        mtg_kernel::card_def::CardType::Creature
    ));
    assert_eq!(power_toughness(&state, incubator), (1, 1));
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { activatable_abilities, .. } if !activatable_abilities.contains(&(incubator, 0)))
    );
}

#[test]
fn brutal_cathar_exiles_until_it_leaves_and_makes_it_day() {
    use mtg_kernel::state::DayNightV1;
    let mut state = ready();
    let victim = put(
        &mut state,
        PlayerId::P1,
        "Emberheart Challenger",
        Zone::Battlefield,
    );
    let cathar = put(&mut state, PlayerId::P0, "Brutal Cathar", Zone::Hand);
    assert_eq!(state.day_night_v1, None);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::W], 2);
    cast(&mut state, cathar, &[]);
    let Some(Decision::ChooseTargets { legal_targets, .. }) = settle(&mut state) else {
        panic!("expected the enters trigger's target");
    };
    assert_eq!(legal_targets, vec![Target::Object(victim)]);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(victim))).unwrap();
    settled(&mut state);
    assert_eq!(state.day_night_v1, Some(DayNightV1::Day));
    assert_eq!(state.objects.get(victim).zone, Zone::Exile);

    // Burning it away returns the exiled creature.
    burn(&mut state, PlayerId::P0, Target::Object(cathar));
    let victim_now = state.players[1]
        .battlefield
        .iter()
        .copied()
        .find(|&id| state.objects.get(id).name == "Emberheart Challenger");
    assert_eq!(state.objects.get(cathar).zone, Zone::Graveyard);
    assert!(victim_now.is_some(), "the exiled creature came back");
}

#[test]
fn brutal_cathar_becomes_moonrage_brute_at_night_and_back_by_day() {
    use mtg_kernel::state::DayNightV1;
    let mut state = ready();
    let cathar = put(&mut state, PlayerId::P0, "Brutal Cathar", Zone::Battlefield);
    mtg_kernel::trigger::sba_fixed_point(&mut state);
    assert_eq!(state.day_night_v1, Some(DayNightV1::Day));

    // P0 casts nothing this turn, so it becomes night as P1's turn begins.
    pass_until(&mut state, |s| {
        s.active_player == PlayerId::P1 && s.step == Step::Main1
    });
    assert_eq!(state.day_night_v1, Some(DayNightV1::Night));
    assert_eq!(state.objects.get(cathar).v4.face_index, 1);
    assert_eq!(state.objects.get(cathar).name, "Moonrage Brute");
    assert_eq!(power_toughness(&state, cathar), (3, 3));
    assert!(engine::has_effective_keyword(
        &state,
        cathar,
        Keywords::FIRST_STRIKE
    ));

    // Ward—pay 3 life: P1 pays and the spell resolves.
    burn_with_ward(
        &mut state,
        PlayerId::P1,
        Target::Object(cathar),
        Action::ChooseEffectBoolean(true),
    );
    assert_eq!(state.players[1].life, 17);
    assert_eq!(state.objects.get(cathar).damage, 2);

    // At 3 life P1 may decline the legal but lethal payment.
    state.players[1].life = 3;
    burn_with_ward(
        &mut state,
        PlayerId::P1,
        Target::Object(cathar),
        Action::ChooseEffectBoolean(false),
    );
    assert_eq!(state.players[1].life, 3);
    assert_eq!(state.objects.get(cathar).damage, 2);

    // P1 cast two spells, so it becomes day as P0's turn begins; the
    // transform back into Brutal Cathar exiles a creature again.
    let victim = put(
        &mut state,
        PlayerId::P1,
        "Emberheart Challenger",
        Zone::Battlefield,
    );
    let mut state_after = state.clone();
    for _ in 0..200 {
        match next(&mut state_after) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert_eq!(legal_targets, vec![Target::Object(victim)]);
                engine::step(
                    &mut state_after,
                    Action::ChooseTarget(Target::Object(victim)),
                )
                .unwrap();
                break;
            }
            Decision::CastSpellOrPass { .. } => {
                engine::step(&mut state_after, Action::Pass).unwrap()
            }
            Decision::DeclareAttackers { .. } => {
                engine::step(&mut state_after, Action::DeclareAttackers(vec![])).unwrap()
            }
            Decision::DeclareBlockers { .. } => {
                engine::step(&mut state_after, Action::DeclareBlockers(vec![])).unwrap()
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    settled(&mut state_after);
    assert_eq!(state_after.active_player, PlayerId::P0);
    assert_eq!(state_after.day_night_v1, Some(DayNightV1::Day));
    assert_eq!(state_after.objects.get(cathar).v4.face_index, 0);
    assert_eq!(state_after.objects.get(victim).zone, Zone::Exile);
}

#[test]
fn brutal_cathar_enters_transformed_at_night() {
    use mtg_kernel::state::DayNightV1;
    let mut state = ready();
    state.day_night_v1 = Some(DayNightV1::Night);
    put(
        &mut state,
        PlayerId::P1,
        "Emberheart Challenger",
        Zone::Battlefield,
    );
    let cathar = put(&mut state, PlayerId::P0, "Brutal Cathar", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::W], 2);
    cast(&mut state, cathar, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(cathar).name, "Moonrage Brute");
}

/// Puts `names` on top of `player`'s library, first name on top.
fn stack_library(state: &mut GameState, player: PlayerId, names: &[&str]) -> Vec<ObjectId> {
    let ids: Vec<ObjectId> = names
        .iter()
        .map(|name| {
            let id = put(state, player, name, Zone::Hand);
            state.players[player.index()]
                .hand
                .retain(|&card| card != id);
            state.objects.get_mut(id).zone = Zone::Library;
            id
        })
        .collect();
    let library = &mut state.players[player.index()].library;
    for (index, &id) in ids.iter().enumerate() {
        library.insert(index, id);
    }
    ids
}

#[test]
fn knight_errant_of_eos_convokes_and_takes_creatures_up_to_the_count() {
    let mut state = ready();
    let initiate = put(
        &mut state,
        PlayerId::P0,
        "Hopeful Initiate",
        Zone::Battlefield,
    );
    let challenger = put(
        &mut state,
        PlayerId::P0,
        "Emberheart Challenger",
        Zone::Battlefield,
    );
    let top = stack_library(
        &mut state,
        PlayerId::P0,
        &[
            "Burnout Bashtronaut",
            "Nova Hellkite",
            "Emberheart Challenger",
            "Mountain",
            "Iridescent Vinelasher",
        ],
    );
    let knight = put(&mut state, PlayerId::P0, "Knight-Errant of Eos", Zone::Hand);
    // Three mana plus two creatures (the white Initiate pays {W}).
    add_mana(&mut state, PlayerId::P0, &[], 3);
    cast(&mut state, knight, &[]);
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectTargets {
            can_finish: false,
            ..
        }
    ));
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(initiate)),
    )
    .unwrap();
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectTargets {
            can_finish: false,
            ..
        }
    ));
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(challenger)),
    )
    .unwrap();
    engine::step(&mut state, Action::FinishEffectSelection).unwrap();
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets {
            can_finish: true,
            ..
        })
    ));
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(top[2])),
    )
    .unwrap();
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(top[4])),
    )
    .unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(knight).zone, Zone::Battlefield);
    assert!(state.objects.get(initiate).tapped);
    assert!(state.objects.get(challenger).tapped);
    assert_eq!(state.objects.get(knight).v4.convoked_creatures_v1, 2);
    // The controller chooses which two qualifying creatures, regardless of library order.
    let hand = &state.players[0].hand;
    assert!(hand.contains(&top[2]) && hand.contains(&top[4]));
    assert!(!hand.contains(&top[0]) && !hand.contains(&top[1]));
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

#[test]
fn knight_errant_of_eos_offers_convoke_or_mana_when_both_work() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Hopeful Initiate",
        Zone::Battlefield,
    );
    let knight = put(&mut state, PlayerId::P0, "Knight-Errant of Eos", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::W], 4);
    assert!(castable(&mut state, knight));
    engine::step(&mut state, Action::CastSpell(knight)).unwrap();
    let Decision::ChooseCastMode { options, .. } = next(&mut state) else {
        panic!("expected a cast mode choice");
    };
    assert_eq!(options.len(), 2);
    engine::step(&mut state, Action::ChooseCastMode(engine::CastMode::Normal)).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(knight).v4.convoked_creatures_v1, 0);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

#[test]
fn monastery_swiftspear_has_haste_and_prowess() {
    let mut state = ready();
    let swiftspear = put(
        &mut state,
        PlayerId::P0,
        "Monastery Swiftspear",
        Zone::Battlefield,
    );
    assert!(engine::has_effective_keyword(
        &state,
        swiftspear,
        Keywords::HASTE
    ));
    burn(&mut state, PlayerId::P0, Target::Player(PlayerId::P1));
    assert_eq!(power_toughness(&state, swiftspear), (2, 3));
}

#[test]
fn heartfire_hero_grows_with_valiant_and_burns_for_its_power_on_death() {
    let mut state = ready();
    let hero = put(
        &mut state,
        PlayerId::P0,
        "Heartfire Hero",
        Zone::Battlefield,
    );
    let full_bore = put(&mut state, PlayerId::P0, "Full Bore", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 0);
    cast(&mut state, full_bore, &[Target::Object(hero)]);
    settled(&mut state);
    assert_eq!(state.objects.get(hero).counters.plus1_plus1, 1);
    assert_eq!(power_toughness(&state, hero), (5, 4));

    // 5/4 now: two Burst Lightnings from the opponent kill it; it deals 5.
    state.priority_player = PlayerId::P1;
    for _ in 0..2 {
        burn(&mut state, PlayerId::P1, Target::Object(hero));
        state.priority_player = PlayerId::P1;
    }
    assert_eq!(state.objects.get(hero).zone, Zone::Graveyard);
    assert_eq!(state.players[1].life, 15);
}

#[test]
fn slickshot_show_off_pumps_on_noncreature_spells_and_can_be_plotted() {
    let mut state = ready();
    let show_off = put(
        &mut state,
        PlayerId::P0,
        "Slickshot Show-Off",
        Zone::Battlefield,
    );
    for keyword in [Keywords::FLYING, Keywords::HASTE] {
        assert!(engine::has_effective_keyword(&state, show_off, keyword));
    }
    burn(&mut state, PlayerId::P0, Target::Player(PlayerId::P1));
    assert_eq!(power_toughness(&state, show_off), (3, 2));
    let second = put(&mut state, PlayerId::P0, "Slickshot Show-Off", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 1);
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { plot_actions, .. } if plot_actions.contains(&second))
    );
}

fn tokens_named(state: &GameState, player: PlayerId, name: &str) -> usize {
    state.players[player.index()]
        .battlefield
        .iter()
        .filter(|&&id| state.objects.get(id).name == name)
        .count()
}

#[test]
fn sanguine_evangelist_makes_bats_and_battle_cries() {
    let mut state = ready();
    let evangelist = put(&mut state, PlayerId::P0, "Sanguine Evangelist", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::W], 2);
    cast(&mut state, evangelist, &[]);
    settled(&mut state);
    assert_eq!(tokens_named(&state, PlayerId::P0, "Bat Token"), 1);
    let swiftspear = put(
        &mut state,
        PlayerId::P0,
        "Monastery Swiftspear",
        Zone::Battlefield,
    );
    state.objects.get_mut(evangelist).summoning_sick = false;
    attack_with(&mut state, vec![evangelist, swiftspear]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, swiftspear), (2, 2));
    assert_eq!(power_toughness(&state, evangelist), (2, 1));
    // Dying makes another Bat.
    let mut state = state.clone();
    state.step = Step::Main1;
    burn(&mut state, PlayerId::P0, Target::Object(evangelist));
    assert_eq!(tokens_named(&state, PlayerId::P0, "Bat Token"), 2);
}

#[test]
fn darkstar_augur_offspring_and_upkeep_reveal() {
    let mut state = ready();
    let augur = put(&mut state, PlayerId::P0, "Darkstar Augur", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::B, ManaColor::B], 2);
    engine::step(&mut state, Action::CastSpell(augur)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseKicker { .. }));
    engine::step(&mut state, Action::ChooseKicker(true)).unwrap();
    settled(&mut state);
    assert_eq!(tokens_named(&state, PlayerId::P0, "Darkstar Augur"), 2);

    // Next upkeep: both reveal a card (a Nova Hellkite, then a Mountain).
    let top = stack_library(&mut state, PlayerId::P0, &["Nova Hellkite", "Mountain"]);
    let turn = state.turn;
    pass_until(&mut state, |s| {
        s.turn > turn && s.active_player == PlayerId::P0 && s.step == Step::Main1
    });
    let hand = &state.players[0].hand;
    assert!(hand.contains(&top[0]) && hand.contains(&top[1]));
    assert_eq!(state.players[0].life, 15);
}

/// Passes until `until` holds, answering any trigger target prompt with its
/// first legal target and recording every other non-priority decision.
fn pass_recording(
    state: &mut GameState,
    until: impl Fn(&GameState) -> bool,
    seen: &mut Vec<Decision>,
) {
    for _ in 0..500 {
        match next(state) {
            Decision::CastSpellOrPass { .. } if until(state) => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::DeclareAttackers { .. } => {
                engine::step(state, Action::DeclareAttackers(vec![])).unwrap()
            }
            Decision::DeclareBlockers { .. } => {
                engine::step(state, Action::DeclareBlockers(vec![])).unwrap()
            }
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected decision: {other:?} (seen {seen:?})"),
        }
    }
    panic!("condition never reached");
}

#[test]
fn ruin_lurker_bat_scries_at_end_step_only_after_descending() {
    let mut state = ready();
    let bat = put(
        &mut state,
        PlayerId::P0,
        "Ruin-Lurker Bat",
        Zone::Battlefield,
    );
    assert!(engine::has_effective_keyword(&state, bat, Keywords::FLYING));
    assert!(engine::has_effective_keyword(
        &state,
        bat,
        Keywords::LIFELINK
    ));
    // No descent this turn: the end step passes with no scry.
    let mut seen = Vec::new();
    pass_recording(
        &mut state,
        |s| s.active_player == PlayerId::P1 && s.step == Step::Main1,
        &mut seen,
    );
    pass_recording(
        &mut state,
        |s| s.active_player == PlayerId::P0 && s.step == Step::Main1,
        &mut seen,
    );

    // A permanent card of ours goes to the graveyard: we descended.
    let frontliner = put(
        &mut state,
        PlayerId::P0,
        "Yotian Frontliner",
        Zone::Battlefield,
    );
    burn(&mut state, PlayerId::P0, Target::Object(frontliner));
    assert_eq!(state.objects.get(frontliner).zone, Zone::Graveyard);
    let scry = loop {
        match next(&mut state) {
            Decision::CastSpellOrPass { .. } => {
                assert!(state.active_player == PlayerId::P0, "no scry this turn");
                engine::step(&mut state, Action::Pass).unwrap();
            }
            Decision::DeclareAttackers { .. } => {
                engine::step(&mut state, Action::DeclareAttackers(vec![])).unwrap()
            }
            other => break other,
        }
    };
    assert_eq!(state.step, Step::End);
    assert!(
        matches!(
            scry,
            Decision::ChooseEffectTargets {
                player: PlayerId::P0,
                ..
            }
        ),
        "{scry:?}"
    );
}

#[test]
fn pawpatch_recruit_counters_another_creature_when_an_opponent_targets_one() {
    let mut state = ready();
    let recruit = put(
        &mut state,
        PlayerId::P0,
        "Pawpatch Recruit",
        Zone::Battlefield,
    );
    let swiftspear = put(
        &mut state,
        PlayerId::P0,
        "Monastery Swiftspear",
        Zone::Battlefield,
    );
    assert!(engine::has_effective_keyword(
        &state,
        recruit,
        Keywords::TRAMPLE
    ));

    // Our own spell targeting our creature does nothing (prowess saves it).
    burn(&mut state, PlayerId::P0, Target::Object(swiftspear));
    assert_eq!(state.objects.get(recruit).counters.plus1_plus1, 0);
    assert_eq!(state.objects.get(swiftspear).zone, Zone::Battlefield);

    // The opponent targets the Swiftspear: the counter can't go on it.
    state.priority_player = PlayerId::P1;
    let burst = put(&mut state, PlayerId::P1, "Burst Lightning", Zone::Hand);
    add_mana(&mut state, PlayerId::P1, &[ManaColor::R], 0);
    cast(&mut state, burst, &[Target::Object(swiftspear)]);
    match settle(&mut state) {
        Some(Decision::ChooseTargets {
            player,
            legal_targets,
            ..
        }) => {
            assert_eq!(player, PlayerId::P0);
            assert_eq!(legal_targets, vec![Target::Object(recruit)]);
            engine::step(&mut state, Action::ChooseTarget(Target::Object(recruit))).unwrap();
        }
        other => panic!("expected the trigger's target, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.objects.get(recruit).counters.plus1_plus1, 1);
    assert_eq!(state.objects.get(swiftspear).zone, Zone::Graveyard);

    // Targeting the Recruit itself with no other creature to grow: the
    // trigger has no legal target and is removed, so nothing prompts.
    state.priority_player = PlayerId::P1;
    let burst = put(&mut state, PlayerId::P1, "Burst Lightning", Zone::Hand);
    add_mana(&mut state, PlayerId::P1, &[ManaColor::R], 0);
    cast(&mut state, burst, &[Target::Object(recruit)]);
    settled(&mut state);
    assert_eq!(state.objects.get(recruit).zone, Zone::Graveyard);
}

#[test]
fn pawpatch_recruit_offspring_copy_is_a_one_one() {
    let mut state = ready();
    let recruit = put(&mut state, PlayerId::P0, "Pawpatch Recruit", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::G], 2);
    engine::step(&mut state, Action::CastSpell(recruit)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseKicker { .. }));
    engine::step(&mut state, Action::ChooseKicker(true)).unwrap();
    settled(&mut state);
    assert_eq!(tokens_named(&state, PlayerId::P0, "Pawpatch Recruit"), 2);
}

#[test]
fn manifold_mouse_grants_a_mouse_double_strike_or_trample_each_combat() {
    let mut state = ready();
    let mouse = put(
        &mut state,
        PlayerId::P0,
        "Manifold Mouse",
        Zone::Battlefield,
    );
    put(
        &mut state,
        PlayerId::P0,
        "Monastery Swiftspear",
        Zone::Battlefield,
    );
    let mut prompted = false;
    for _ in 0..20 {
        match next(&mut state) {
            Decision::CastSpellOrPass { .. } if state.step == Step::BeginCombat && prompted => {
                break
            }
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            Decision::ChooseTargets { legal_targets, .. } => {
                assert_eq!(state.step, Step::BeginCombat);
                assert_eq!(legal_targets, vec![Target::Object(mouse)]);
                engine::step(&mut state, Action::ChooseTarget(Target::Object(mouse))).unwrap();
            }
            Decision::ChooseEffectOption { .. } => {
                prompted = true;
                engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert!(prompted);
    assert!(engine::has_effective_keyword(
        &state,
        mouse,
        Keywords::TRAMPLE
    ));
    assert!(!engine::has_effective_keyword(
        &state,
        mouse,
        Keywords::DOUBLE_STRIKE
    ));
}

#[test]
fn manifold_mouse_does_not_trigger_on_the_opponents_turn() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P1,
        "Manifold Mouse",
        Zone::Battlefield,
    );
    let mut seen = Vec::new();
    pass_recording(&mut state, |s| s.step == Step::Main2, &mut seen);
}

#[test]
fn yotian_frontliner_pumps_another_attacker_and_unearths() {
    let mut state = ready();
    let frontliner = put(
        &mut state,
        PlayerId::P0,
        "Yotian Frontliner",
        Zone::Battlefield,
    );
    let swiftspear = put(
        &mut state,
        PlayerId::P0,
        "Monastery Swiftspear",
        Zone::Battlefield,
    );
    attack_with(&mut state, vec![frontliner]);
    match settle(&mut state) {
        Some(Decision::ChooseTargets { legal_targets, .. }) => {
            assert_eq!(legal_targets, vec![Target::Object(swiftspear)]);
            engine::step(&mut state, Action::ChooseTarget(Target::Object(swiftspear))).unwrap();
        }
        other => panic!("expected the attack trigger's target, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(power_toughness(&state, swiftspear), (2, 3));
}

#[test]
fn yotian_frontliner_unearth_returns_with_haste_then_exiles() {
    let mut state = ready();
    let frontliner = put(
        &mut state,
        PlayerId::P0,
        "Yotian Frontliner",
        Zone::Graveyard,
    );
    let offered = |state: &mut GameState| matches!(next(state), Decision::CastSpellOrPass { activatable_abilities, .. } if activatable_abilities.contains(&(frontliner, 0)));
    assert!(!offered(&mut state), "no mana");
    add_mana(&mut state, PlayerId::P0, &[ManaColor::W], 0);
    assert!(offered(&mut state));
    engine::step(&mut state, Action::ActivateAbility(frontliner, 0)).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(frontliner).zone, Zone::Battlefield);
    assert!(engine::has_effective_keyword(
        &state,
        frontliner,
        Keywords::HASTE
    ));

    // Exiled at the beginning of the end step, with no way to cast it.
    let mut seen = Vec::new();
    pass_recording(&mut state, |s| s.step == Step::End, &mut seen);
    settled(&mut state);
    assert_eq!(state.objects.get(frontliner).zone, Zone::Exile);
    assert!(!state
        .engine
        .exile_play_permissions
        .iter()
        .any(|permission| permission.object == frontliner));
}

#[test]
fn unearthed_yotian_frontliner_is_exiled_instead_of_dying() {
    let mut state = ready();
    let frontliner = put(
        &mut state,
        PlayerId::P0,
        "Yotian Frontliner",
        Zone::Graveyard,
    );
    add_mana(&mut state, PlayerId::P0, &[ManaColor::W], 0);
    engine::step(&mut state, Action::ActivateAbility(frontliner, 0)).unwrap();
    settled(&mut state);
    burn(&mut state, PlayerId::P0, Target::Object(frontliner));
    assert_eq!(state.objects.get(frontliner).zone, Zone::Exile);
}

#[test]
fn unearth_is_sorcery_speed() {
    let mut state = ready();
    let frontliner = put(
        &mut state,
        PlayerId::P0,
        "Yotian Frontliner",
        Zone::Graveyard,
    );
    add_mana(&mut state, PlayerId::P0, &[ManaColor::W], 0);
    state.step = Step::Upkeep;
    assert!(
        !matches!(next(&mut state), Decision::CastSpellOrPass { activatable_abilities, .. } if activatable_abilities.contains(&(frontliner, 0)))
    );
}

#[test]
fn cori_steel_cutter_flurry_makes_a_monk_and_may_attach_to_it() {
    for attach in [true, false] {
        let mut state = ready();
        let cutter = put(
            &mut state,
            PlayerId::P0,
            "Cori-Steel Cutter",
            Zone::Battlefield,
        );
        burn(&mut state, PlayerId::P0, Target::Player(PlayerId::P1));
        assert_eq!(tokens_named(&state, PlayerId::P0, "Monk Token"), 0);
        let burst = put(&mut state, PlayerId::P0, "Burst Lightning", Zone::Hand);
        add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 0);
        cast(&mut state, burst, &[Target::Player(PlayerId::P1)]);
        match settle(&mut state) {
            Some(Decision::ChooseEffectOption { .. }) => engine::step(
                &mut state,
                Action::ChooseEffectOption(if attach { 0 } else { 1 }),
            )
            .unwrap(),
            other => panic!("expected the attach choice, got {other:?}"),
        }
        settled(&mut state);
        let monk = *state.players[0]
            .battlefield
            .iter()
            .find(|&&id| state.objects.get(id).name == "Monk Token")
            .expect("a Monk");
        let attached = state
            .objects
            .get(cutter)
            .v4
            .attached_to
            .map(|link| link.object);
        if attach {
            assert_eq!(attached, Some(monk));
            assert_eq!(power_toughness(&state, monk), (2, 2));
            assert!(engine::has_effective_keyword(
                &state,
                monk,
                Keywords::TRAMPLE
            ));
            assert!(engine::has_effective_keyword(&state, monk, Keywords::HASTE));
        } else {
            assert_eq!(attached, None);
            assert_eq!(power_toughness(&state, monk), (1, 1));
        }

        // A third spell is not the second: no new Monk, but the Monk's
        // prowess triggers.
        burn(&mut state, PlayerId::P0, Target::Player(PlayerId::P1));
        assert_eq!(tokens_named(&state, PlayerId::P0, "Monk Token"), 1);
        let base = if attach { 2 } else { 1 };
        assert_eq!(power_toughness(&state, monk), (base + 1, base + 1));
    }
}

#[test]
fn cori_steel_cutter_equips_for_one_and_red() {
    let mut state = ready();
    let cutter = put(
        &mut state,
        PlayerId::P0,
        "Cori-Steel Cutter",
        Zone::Battlefield,
    );
    let mouse = put(
        &mut state,
        PlayerId::P0,
        "Manifold Mouse",
        Zone::Battlefield,
    );
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 1);
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { activatable_abilities, .. } if activatable_abilities.contains(&(cutter, 0)))
    );
    engine::step(&mut state, Action::ActivateAbility(cutter, 0)).unwrap();
    match next(&mut state) {
        Decision::ChooseTargets { .. } => {
            engine::step(&mut state, Action::ChooseTarget(Target::Object(mouse))).unwrap()
        }
        other => panic!("expected a target, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(power_toughness(&state, mouse), (2, 3));
}

/// Answers a trigger's "up to" target prompts with `targets`, then finishes.
fn choose_up_to(state: &mut GameState, targets: &[Target]) {
    for target in targets {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(
                    legal_targets.contains(target),
                    "{target:?} not in {legal_targets:?}"
                );
                engine::step(state, Action::ChooseTarget(*target)).unwrap();
            }
            other => panic!("expected targets, got {other:?}"),
        }
    }
    if let Decision::ChooseTargets { can_finish, .. } = next(state) {
        assert!(can_finish);
        engine::step(state, Action::FinishEffectSelection).unwrap();
    }
}

fn cast_trespasser(state: &mut GameState) -> ObjectId {
    let trespasser = put(state, PlayerId::P0, "Graveyard Trespasser", Zone::Hand);
    add_mana(state, PlayerId::P0, &[ManaColor::B], 2);
    assert!(castable(state, trespasser));
    engine::step(state, Action::CastSpell(trespasser)).unwrap();
    // Pass priority until the spell resolves and its trigger asks.
    for _ in 0..10 {
        if !matches!(next(state), Decision::CastSpellOrPass { .. }) {
            break;
        }
        engine::step(state, Action::Pass).unwrap();
    }
    trespasser
}

#[test]
fn graveyard_trespasser_exiles_a_graveyard_card_and_drains_for_a_creature() {
    let mut state = ready();
    let creature = put(
        &mut state,
        PlayerId::P1,
        "Monastery Swiftspear",
        Zone::Graveyard,
    );
    let land = put(&mut state, PlayerId::P1, "Mountain", Zone::Graveyard);
    let trespasser = cast_trespasser(&mut state);
    match next(&mut state) {
        Decision::ChooseTargets {
            legal_targets,
            can_finish,
            ..
        } => {
            assert!(can_finish);
            assert!(legal_targets.contains(&Target::Object(creature)));
            assert!(legal_targets.contains(&Target::Object(land)));
        }
        other => panic!("expected the enter trigger's targets, got {other:?}"),
    }
    choose_up_to(&mut state, &[Target::Object(creature)]);
    settled(&mut state);
    assert_eq!(state.objects.get(trespasser).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(creature).zone, Zone::Exile);
    assert_eq!((state.players[0].life, state.players[1].life), (21, 19));

    // A land exiled drains nothing; choosing no target is legal.
    let mut state = ready();
    let land = put(&mut state, PlayerId::P1, "Mountain", Zone::Graveyard);
    cast_trespasser(&mut state);
    choose_up_to(&mut state, &[Target::Object(land)]);
    settled(&mut state);
    assert_eq!(state.objects.get(land).zone, Zone::Exile);
    assert_eq!((state.players[0].life, state.players[1].life), (20, 20));

    let mut state = ready();
    put(&mut state, PlayerId::P1, "Mountain", Zone::Graveyard);
    cast_trespasser(&mut state);
    choose_up_to(&mut state, &[]);
    settled(&mut state);
    assert_eq!(state.exile.len(), 0);
}

#[test]
fn graveyard_trespasser_ward_lets_opponents_choose_the_discarded_card() {
    let mut state = ready();
    let trespasser = put(
        &mut state,
        PlayerId::P1,
        "Graveyard Trespasser",
        Zone::Battlefield,
    );
    let hellkite = put(&mut state, PlayerId::P0, "Nova Hellkite", Zone::Hand);
    let mountain = put(&mut state, PlayerId::P0, "Mountain", Zone::Hand);
    burn_with_ward(
        &mut state,
        PlayerId::P0,
        Target::Object(trespasser),
        Action::ChooseEffectTarget(Target::Object(hellkite)),
    );
    assert_eq!(state.objects.get(mountain).zone, Zone::Hand);
    assert_eq!(state.objects.get(hellkite).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(trespasser).damage, 2);

    // With only the Hellkite left it is discarded too; then an empty hand
    // can't pay and the spell is countered.
    burn_with_ward(
        &mut state,
        PlayerId::P0,
        Target::Object(trespasser),
        Action::ChooseEffectTarget(Target::Object(mountain)),
    );
    assert_eq!(state.objects.get(hellkite).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(trespasser).zone, Zone::Graveyard);
    let mut state = ready();
    let trespasser = put(
        &mut state,
        PlayerId::P1,
        "Graveyard Trespasser",
        Zone::Battlefield,
    );
    burn(&mut state, PlayerId::P0, Target::Object(trespasser));
    assert_eq!(state.objects.get(trespasser).damage, 0);
}

#[test]
fn graveyard_trespasser_enters_as_glutton_at_night_and_exiles_two() {
    use mtg_kernel::state::DayNightV1;
    let mut state = ready();
    state.day_night_v1 = Some(DayNightV1::Night);
    let first = put(
        &mut state,
        PlayerId::P1,
        "Monastery Swiftspear",
        Zone::Graveyard,
    );
    let second = put(&mut state, PlayerId::P1, "Heartfire Hero", Zone::Graveyard);
    let trespasser = cast_trespasser(&mut state);
    assert_eq!(state.objects.get(trespasser).name, "Graveyard Glutton");
    assert_eq!(power_toughness(&state, trespasser), (4, 4));
    choose_up_to(&mut state, &[Target::Object(first), Target::Object(second)]);
    settled(&mut state);
    assert_eq!(state.objects.get(first).zone, Zone::Exile);
    assert_eq!(state.objects.get(second).zone, Zone::Exile);
    assert_eq!((state.players[0].life, state.players[1].life), (22, 18));
}

#[test]
fn overlord_of_the_mistmoors_impends_then_becomes_a_creature() {
    let mut state = ready();
    let overlord = put(
        &mut state,
        PlayerId::P0,
        "Overlord of the Mistmoors",
        Zone::Hand,
    );
    // Only the impending cost is affordable.
    add_mana(&mut state, PlayerId::P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut state, overlord, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(overlord).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(overlord).v4.time_counters_v1, 4);
    assert_eq!(tokens_named(&state, PlayerId::P0, "White Insect Token"), 2);
    assert!(!engine::object_has_type(
        &state,
        overlord,
        mtg_kernel::card_def::CardType::Creature
    ));
    assert!(engine::object_has_type(
        &state,
        overlord,
        mtg_kernel::card_def::CardType::Enchantment
    ));

    // It can't attack yet; one counter comes off at each of our end steps.
    state.objects.get_mut(overlord).summoning_sick = false;
    pass_until(&mut state, |s| s.step == Step::BeginCombat);
    match next(&mut state) {
        Decision::DeclareAttackers { .. } | Decision::CastSpellOrPass { .. } => {}
        other => panic!("{other:?}"),
    }
    let turn = state.turn;
    pass_until(&mut state, |s| {
        s.turn > turn && s.active_player == PlayerId::P0 && s.step == Step::Main1
    });
    assert_eq!(state.objects.get(overlord).v4.time_counters_v1, 3);
    for _ in 0..3 {
        let turn = state.turn;
        pass_until(&mut state, |s| {
            s.turn > turn && s.active_player == PlayerId::P0 && s.step == Step::Main1
        });
    }
    assert_eq!(state.objects.get(overlord).v4.time_counters_v1, 0);
    assert!(engine::object_has_type(
        &state,
        overlord,
        mtg_kernel::card_def::CardType::Creature
    ));
    assert_eq!(power_toughness(&state, overlord), (6, 6));

    // Now a 6/6, its attack makes two more Insects.
    attack_with(&mut state, vec![overlord]);
    settled(&mut state);
    assert_eq!(tokens_named(&state, PlayerId::P0, "White Insect Token"), 4);
}

#[test]
fn impending_overlord_cannot_attack_or_block() {
    let mut state = ready();
    let overlord = put(
        &mut state,
        PlayerId::P0,
        "Overlord of the Mistmoors",
        Zone::Battlefield,
    );
    let mouse = put(
        &mut state,
        PlayerId::P0,
        "Manifold Mouse",
        Zone::Battlefield,
    );
    state.objects.get_mut(overlord).v4.time_counters_v1 = 2;
    loop {
        match next(&mut state) {
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            Decision::ChooseTargets { .. } => {
                engine::step(&mut state, Action::ChooseTarget(Target::Object(mouse))).unwrap()
            }
            Decision::ChooseEffectOption { .. } => {
                engine::step(&mut state, Action::ChooseEffectOption(0)).unwrap()
            }
            Decision::DeclareAttackers { eligible, .. } => {
                assert_eq!(eligible, vec![mouse]);
                break;
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }

    // On the opponent's turn it can't block either.
    let mut state = ready();
    let overlord = put(
        &mut state,
        PlayerId::P0,
        "Overlord of the Mistmoors",
        Zone::Battlefield,
    );
    state.objects.get_mut(overlord).v4.time_counters_v1 = 2;
    let attacker = put(
        &mut state,
        PlayerId::P1,
        "Monastery Swiftspear",
        Zone::Battlefield,
    );
    pass_until(&mut state, |s| {
        s.active_player == PlayerId::P1 && s.step == Step::Main1
    });
    pass_until_blocks(&mut state, attacker, |blockers| {
        assert!(!blockers.contains(&overlord))
    });
}

#[test]
fn enduring_innocence_returns_as_an_enchantment_and_draws_once_a_turn() {
    use mtg_kernel::card_def::CardType;
    let mut state = ready();
    let innocence = put(
        &mut state,
        PlayerId::P0,
        "Enduring Innocence",
        Zone::Battlefield,
    );
    let hand = state.players[0].hand.len();
    // Two small creatures enter: one card, once this turn.
    for _ in 0..2 {
        let frontliner = put(&mut state, PlayerId::P0, "Yotian Frontliner", Zone::Hand);
        add_mana(&mut state, PlayerId::P0, &[], 1);
        cast(&mut state, frontliner, &[]);
        settled(&mut state);
    }
    assert_eq!(state.players[0].hand.len(), hand + 1);

    // It dies as a creature and comes back as a noncreature enchantment
    // that still draws.
    burn(&mut state, PlayerId::P0, Target::Object(innocence));
    assert_eq!(state.objects.get(innocence).zone, Zone::Battlefield);
    assert!(state.objects.get(innocence).v4.enduring_enchantment_v1);
    assert!(!engine::object_has_type(
        &state,
        innocence,
        CardType::Creature
    ));
    assert!(engine::object_has_type(
        &state,
        innocence,
        CardType::Enchantment
    ));
    let turn = state.turn;
    pass_until(&mut state, |s| {
        s.turn > turn && s.active_player == PlayerId::P0 && s.step == Step::Main1
    });
    let hand = state.players[0].hand.len();
    let frontliner = put(&mut state, PlayerId::P0, "Yotian Frontliner", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[], 1);
    cast(&mut state, frontliner, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), hand + 1);

    // As an enchantment it is no longer a creature target, and destroyed
    // that way it stays in the graveyard.
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(innocence, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(state.objects.get(innocence).zone, Zone::Graveyard);
}

#[test]
fn enduring_curiosity_draws_for_each_creature_connecting() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Enduring Curiosity",
        Zone::Battlefield,
    );
    let first = put(
        &mut state,
        PlayerId::P0,
        "Monastery Swiftspear",
        Zone::Battlefield,
    );
    let second = put(
        &mut state,
        PlayerId::P0,
        "Monastery Swiftspear",
        Zone::Battlefield,
    );
    let hand = state.players[0].hand.len();
    attack_with(&mut state, vec![first, second]);
    pass_until(&mut state, |s| s.step == Step::Main2);
    assert_eq!(state.players[0].hand.len(), hand + 2);
    assert_eq!(state.players[1].life, 18);
}

/// P1 casts Burst Lightning at P0, then P0 answers with Make Disappear,
/// paying casualty when `casualty` is set.
fn make_disappear_a_burn(state: &mut GameState, casualty: bool) -> (ObjectId, ObjectId) {
    let fodder = put(state, PlayerId::P0, "Yotian Frontliner", Zone::Battlefield);
    let burst = make_disappear_a_burn_with(state, casualty);
    (fodder, burst)
}

/// P1 casts Burst Lightning at P0; P0 responds with Make Disappear,
/// choosing casualty or not. Returns the Burst Lightning.
fn make_disappear_a_burn_with(state: &mut GameState, casualty: bool) -> ObjectId {
    let counter = put(state, PlayerId::P0, "Make Disappear", Zone::Hand);
    state.priority_player = PlayerId::P1;
    let burst = put(state, PlayerId::P1, "Burst Lightning", Zone::Hand);
    add_mana(state, PlayerId::P1, &[ManaColor::R], 0);
    cast(state, burst, &[Target::Player(PlayerId::P0)]);
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass {
            player: PlayerId::P1,
            ..
        }
    ));
    engine::step(state, Action::Pass).unwrap();
    add_mana(state, PlayerId::P0, &[ManaColor::U], 1);
    assert!(castable(state, counter));
    engine::step(state, Action::CastSpell(counter)).unwrap();
    for _ in 0..5 {
        match next(state) {
            Decision::ChooseEffectOption { .. } => {
                engine::step(state, Action::ChooseEffectOption(u16::from(casualty))).unwrap()
            }
            Decision::ChooseTargets { .. } => {
                engine::step(state, Action::ChooseTarget(Target::Object(burst))).unwrap()
            }
            Decision::CastSpellOrPass { .. } => break,
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    burst
}

#[test]
fn make_disappear_casualty_can_sacrifice_a_transformed_incubator() {
    let mut state = ready();
    let incubator = put(
        &mut state,
        PlayerId::P0,
        "Incubator Token",
        Zone::Battlefield,
    );
    state.objects.get_mut(incubator).counters.plus1_plus1 = 1;
    add_mana(&mut state, PlayerId::P0, &[], 2);
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { activatable_abilities, .. } if activatable_abilities.contains(&(incubator, 0)))
    );
    engine::step(&mut state, Action::ActivateAbility(incubator, 0)).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(incubator).v4.face_index, 1);
    assert_eq!(power_toughness(&state, incubator), (1, 1));

    let burst = make_disappear_a_burn_with(&mut state, true);
    assert!(state.engine.halted.is_none());
    assert_ne!(state.objects.get(incubator).zone, Zone::Battlefield);
    assert_eq!(state.stack.len(), 3);
    assert!(!state.stack[2].is_copy);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets { .. })
    ));
    engine::step(&mut state, Action::FinishEffectSelection).unwrap();
    next(&mut state);
    assert!(state.stack[2].is_copy);
    assert_eq!(state.stack[2].targets, vec![Target::Object(burst)]);
}

#[test]
fn make_disappear_with_casualty_copies_itself() {
    let mut state = ready();
    let (fodder, burst) = make_disappear_a_burn(&mut state, true);
    assert_eq!(state.objects.get(fodder).zone, Zone::Graveyard);
    assert_eq!(state.stack.len(), 3);
    assert!(!state.stack[2].is_copy);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets { .. })
    ));
    engine::step(&mut state, Action::FinishEffectSelection).unwrap();
    next(&mut state);
    assert!(state.stack[2].is_copy);
    assert_eq!(state.stack[2].targets, vec![Target::Object(burst)]);

    // P1 pays {2} for the copy but can't pay again for the original.
    add_mana(&mut state, PlayerId::P1, &[], 2);
    for _ in 0..20 {
        match next(&mut state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            Decision::ChooseEffectBoolean { player, .. } => {
                assert_eq!(player, PlayerId::P1);
                engine::step(&mut state, Action::ChooseEffectBoolean(true)).unwrap()
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert_eq!(state.objects.get(burst).zone, Zone::Graveyard);
    assert_eq!(state.players[0].life, 20);
}

#[test]
fn make_disappear_without_casualty_is_a_single_counter() {
    let mut state = ready();
    let (fodder, _) = make_disappear_a_burn(&mut state, false);
    assert_eq!(state.objects.get(fodder).zone, Zone::Battlefield);
    assert_eq!(state.stack.len(), 2);
    add_mana(&mut state, PlayerId::P1, &[], 2);
    for _ in 0..20 {
        match next(&mut state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            Decision::ChooseEffectBoolean { .. } => {
                engine::step(&mut state, Action::ChooseEffectBoolean(true)).unwrap()
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert_eq!(state.players[0].life, 18);
}

/// P1 casts Burst Lightning at P0, then P0 gets priority with Phantom
/// Interference in hand and `generic` extra mana beside its {U}.
fn phantom_interference_facing_a_burn(state: &mut GameState, generic: u8) -> (ObjectId, ObjectId) {
    let spree = put(state, PlayerId::P0, "Phantom Interference", Zone::Hand);
    state.priority_player = PlayerId::P1;
    let burst = put(state, PlayerId::P1, "Burst Lightning", Zone::Hand);
    add_mana(state, PlayerId::P1, &[ManaColor::R], 0);
    cast(state, burst, &[Target::Player(PlayerId::P0)]);
    next(state);
    engine::step(state, Action::Pass).unwrap();
    add_mana(state, PlayerId::P0, &[ManaColor::U], generic);
    (spree, burst)
}

fn resolve_stack_declining_payments(state: &mut GameState) {
    for _ in 0..20 {
        match next(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::ChooseEffectBoolean { .. } => {
                engine::step(state, Action::ChooseEffectBoolean(false)).unwrap()
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    panic!("stack never emptied");
}

#[test]
fn phantom_interference_spree_offers_every_affordable_mode_set() {
    let mut state = ready();
    let (spree, burst) = phantom_interference_facing_a_burn(&mut state, 4);
    assert!(castable(&mut state, spree));
    engine::step(&mut state, Action::CastSpell(spree)).unwrap();
    match next(&mut state) {
        Decision::ChooseSpellMode { legal_modes, .. } => assert_eq!(legal_modes, vec![0, 1, 2]),
        other => panic!("expected a mode choice, got {other:?}"),
    }
    engine::step(&mut state, Action::ChooseSpellMode(2)).unwrap();
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(burst)])
        }
        other => panic!("expected spell targets, got {other:?}"),
    }
    engine::step(&mut state, Action::ChooseTarget(Target::Object(burst))).unwrap();
    resolve_stack_declining_payments(&mut state);
    assert_eq!(state.objects.get(burst).zone, Zone::Graveyard);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    assert_eq!(state.players[0].life, 20);
    assert_eq!(tokens_named(&state, PlayerId::P0, "Spirit Token"), 1);
}

#[test]
fn phantom_interference_counter_mode_costs_one_more() {
    let mut state = ready();
    let (spree, burst) = phantom_interference_facing_a_burn(&mut state, 1);
    engine::step(&mut state, Action::CastSpell(spree)).unwrap();
    // Only the {U}+{1} counter mode is affordable, so it is chosen silently.
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(burst))).unwrap();
    resolve_stack_declining_payments(&mut state);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    assert_eq!(state.players[0].life, 20);
    assert_eq!(tokens_named(&state, PlayerId::P0, "Spirit Token"), 0);
}

#[test]
fn phantom_interference_spirit_mode_needs_no_spell_to_target() {
    let mut state = ready();
    let spree = put(&mut state, PlayerId::P0, "Phantom Interference", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::U], 1);
    assert!(
        !castable(&mut state, spree),
        "{{U}}+{{1}} with no spell to counter"
    );
    add_mana(&mut state, PlayerId::P0, &[], 2);
    assert!(castable(&mut state, spree));
    engine::step(&mut state, Action::CastSpell(spree)).unwrap();
    resolve_stack_declining_payments(&mut state);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    let spirit = state.players[0]
        .battlefield
        .iter()
        .copied()
        .find(|&id| CARD_DEFS[state.objects.get(id).card_def as usize].name == "Spirit Token")
        .expect("spirit token");
    assert_eq!(power_toughness(&state, spirit), (2, 2));
}

#[test]
fn flourishing_bloom_kin_counts_forests_you_control() {
    let mut state = ready();
    let bloom = put(
        &mut state,
        PlayerId::P0,
        "Flourishing Bloom-Kin",
        Zone::Battlefield,
    );
    put(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    put(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    put(&mut state, PlayerId::P1, "Forest", Zone::Battlefield);
    assert_eq!(power_toughness(&state, bloom), (2, 2));
    put(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    assert_eq!(power_toughness(&state, bloom), (3, 3));
}

#[test]
fn flourishing_bloom_kin_dies_without_forests() {
    let mut state = ready();
    let bloom = put(
        &mut state,
        PlayerId::P0,
        "Flourishing Bloom-Kin",
        Zone::Hand,
    );
    add_mana(&mut state, PlayerId::P0, &[ManaColor::G], 1);
    cast(&mut state, bloom, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(bloom).zone, Zone::Graveyard);
}

#[test]
fn incubator_two_pending_activations_transform_once_without_halting() {
    let mut state = ready();
    let incubator = put(
        &mut state,
        PlayerId::P0,
        "Incubator Token",
        Zone::Battlefield,
    );
    state.objects.get_mut(incubator).counters.plus1_plus1 = 2;
    add_mana(&mut state, PlayerId::P0, &[], 4);

    for _ in 0..2 {
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { activatable_abilities, .. } if activatable_abilities.contains(&(incubator, 0)))
        );
        engine::step(&mut state, Action::ActivateAbility(incubator, 0)).unwrap();
    }
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(state.stack.len(), 2, "both activations are still pending");
    assert_eq!(state.players[0].mana_pool[5], 0);
    settled(&mut state);

    assert!(state.engine.halted.is_none());
    assert_eq!(state.objects.get(incubator).v4.face_index, 1);
    assert_eq!(state.objects.get(incubator).name, "Phyrexian Token");
    assert_eq!(power_toughness(&state, incubator), (2, 2));
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { activatable_abilities, .. } if !activatable_abilities.contains(&(incubator, 0)))
    );
}

#[test]
fn battle_cry_bonus_does_not_follow_a_returned_object() {
    let mut state = ready();
    let evangelist = put(
        &mut state,
        PlayerId::P0,
        "Sanguine Evangelist",
        Zone::Battlefield,
    );
    let returned = put(
        &mut state,
        PlayerId::P0,
        "Monastery Swiftspear",
        Zone::Battlefield,
    );
    let unchanged = put(
        &mut state,
        PlayerId::P0,
        "Monastery Swiftspear",
        Zone::Battlefield,
    );
    attack_with(&mut state, vec![evangelist, returned, unchanged]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, returned), (2, 2));
    assert_eq!(power_toughness(&state, unchanged), (2, 2));
    assert_eq!(power_toughness(&state, evangelist), (2, 1));

    let previous_generation = state.objects.get(returned).zone_change_count;
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(returned, Zone::Hand),
    );
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(returned, Zone::Battlefield),
    );
    assert_eq!(
        state.objects.get(returned).zone_change_count,
        previous_generation + 2
    );
    assert_eq!(power_toughness(&state, returned), (1, 2));
    assert_eq!(power_toughness(&state, unchanged), (2, 2));
}

#[test]
fn chrome_host_seedshark_incubates_the_announced_omen_mana_value() {
    let mut state = ready_with("Forest");
    put(
        &mut state,
        PlayerId::P0,
        "Chrome Host Seedshark",
        Zone::Battlefield,
    );
    let sagu = put(&mut state, PlayerId::P0, "Sagu Wildling", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::G], 0);
    // Only Roost Seek ({G}) is affordable, rather than the MV5 creature.
    cast(&mut state, sagu, &[]);
    let search = settle(&mut state).expect("Roost Seek's basic-land search");
    assert!(matches!(search, Decision::ChooseEffectTargets { .. }));
    let [incubator] = incubators(&state, PlayerId::P0)[..] else {
        panic!("expected one Incubator from Roost Seek");
    };
    assert_eq!(state.objects.get(incubator).counters.plus1_plus1, 1);
    engine::step(&mut state, Action::FinishEffectSelection).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(sagu).zone, Zone::Library);
    assert_eq!(state.objects.get(incubator).counters.plus1_plus1, 1);
}

#[test]
fn stolen_enduring_death_triggers_for_its_controller_and_returns_to_its_owner() {
    for name in ["Enduring Curiosity", "Enduring Innocence"] {
        let mut state = ready();
        let enduring = put(&mut state, PlayerId::P0, name, Zone::Battlefield);
        state.players[0].battlefield.retain(|&id| id != enduring);
        state.players[1].battlefield.push(enduring);
        state.objects.get_mut(enduring).controller = PlayerId::P1;

        mtg_kernel::event::propose_and_commit(
            &mut state,
            mtg_kernel::event::ProposedEvent::zone_change(enduring, Zone::Graveyard),
        );
        let graveyard_generation = state.objects.get(enduring).zone_change_count;
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        let trigger = state
            .stack
            .iter()
            .find(|item| {
                item.source == enduring
                    && item.kind == mtg_kernel::state::StackItemKind::TriggeredAbility
            })
            .expect("Enduring's death trigger is on the stack");
        assert_eq!(trigger.controller, PlayerId::P1);
        let contract = trigger
            .v4
            .ability_source_contract
            .expect("graveyard return binding");
        assert_eq!(contract.controller, PlayerId::P1);
        assert_eq!(contract.zone, Zone::Graveyard);
        assert_eq!(contract.zone_change_count, graveyard_generation);

        settled(&mut state);
        let returned = state.objects.get(enduring);
        assert_eq!(returned.zone, Zone::Battlefield);
        assert_eq!(returned.controller, PlayerId::P0);
        assert_eq!(returned.zone_change_count, graveyard_generation + 1);
        assert!(returned.v4.enduring_enchantment_v1);
        assert!(state.players[0].battlefield.contains(&enduring));
        assert!(!state.players[1].battlefield.contains(&enduring));
        assert!(!engine::object_has_type(
            &state,
            enduring,
            mtg_kernel::card_def::CardType::Creature,
        ));
    }
}

#[test]
fn ward_evidence_is_optional_and_incomplete_subsets_cannot_finish() {
    for decline in [false, true] {
        let mut state = ready();
        let ferox = put(&mut state, PlayerId::P1, "Axebane Ferox", Zone::Battlefield);
        let small = put(
            &mut state,
            PlayerId::P0,
            "Burnout Bashtronaut",
            Zone::Graveyard,
        );
        let big = put(&mut state, PlayerId::P0, "Nova Hellkite", Zone::Graveyard);
        let spell = put(&mut state, PlayerId::P0, "Burst Lightning", Zone::Hand);
        add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 0);
        cast(&mut state, spell, &[Target::Object(ferox)]);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectTargets {
                can_finish: true,
                ..
            })
        ));
        if decline {
            engine::step(&mut state, Action::FinishEffectSelection).unwrap();
            settled(&mut state);
            assert_eq!(state.objects.get(ferox).damage, 0);
            assert_eq!(state.objects.get(big).zone, Zone::Graveyard);
        } else {
            engine::step(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(small)),
            )
            .unwrap();
            assert!(matches!(
                next(&mut state),
                Decision::ChooseEffectTargets {
                    can_finish: false,
                    ..
                }
            ));
            let before = state.clone();
            assert!(engine::step(&mut state, Action::FinishEffectSelection).is_err());
            assert_eq!(state, before);
            let encoded = serde_json::to_string(&state).unwrap();
            let mut restored: GameState = serde_json::from_str(&encoded).unwrap();
            for s in [&mut state, &mut restored] {
                engine::step(s, Action::ChooseEffectTarget(Target::Object(big))).unwrap();
                settled(s);
                assert_eq!(s.objects.get(ferox).damage, 2);
                assert_eq!(s.objects.get(small).zone, Zone::Exile);
                assert_eq!(s.objects.get(big).zone, Zone::Exile);
            }
            assert_eq!(state, restored);
        }
    }
}

#[test]
fn moonrage_brute_ward_permits_the_legal_lethal_life_payment() {
    let mut state = ready();
    state.day_night_v1 = Some(mtg_kernel::state::DayNightV1::Night);
    let cathar = put(&mut state, PlayerId::P1, "Brutal Cathar", Zone::Battlefield);
    next(&mut state);
    state.players[0].life = 3;
    let spell = put(&mut state, PlayerId::P0, "Burst Lightning", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 0);
    cast(&mut state, spell, &[Target::Object(cathar)]);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectBoolean {
            player: PlayerId::P0,
            ..
        })
    ));
    engine::step(&mut state, Action::ChooseEffectBoolean(true)).unwrap();
    let _ = engine::advance_until_decision(&mut state);
    assert_eq!(state.players[0].life, 0);
    assert!(state.engine.halted.is_none());
}

#[test]
fn cathar_leaving_before_its_trigger_resolves_does_not_exile_the_target() {
    let mut state = ready();
    let victim = put(
        &mut state,
        PlayerId::P1,
        "Emberheart Challenger",
        Zone::Battlefield,
    );
    let cathar = put(&mut state, PlayerId::P0, "Brutal Cathar", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::W], 2);
    cast(&mut state, cathar, &[]);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseTargets { .. })
    ));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(victim))).unwrap();
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(cathar, Zone::Hand),
    );
    settled(&mut state);
    assert_eq!(state.objects.get(victim).zone, Zone::Battlefield);
    assert!(state.engine.linked_exile_records.is_empty());
}

#[test]
fn cathar_returns_every_linked_card_immediately_when_it_leaves() {
    let mut state = ready();
    let first = put(
        &mut state,
        PlayerId::P1,
        "Emberheart Challenger",
        Zone::Battlefield,
    );
    let second = put(
        &mut state,
        PlayerId::P1,
        "Monastery Swiftspear",
        Zone::Battlefield,
    );
    let cathar = put(&mut state, PlayerId::P0, "Brutal Cathar", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::W], 2);
    cast(&mut state, cathar, &[]);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseTargets { .. })
    ));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(first))).unwrap();
    settled(&mut state);
    state.day_night_v1 = Some(mtg_kernel::state::DayNightV1::Night);
    next(&mut state);
    state.day_night_v1 = Some(mtg_kernel::state::DayNightV1::Day);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseTargets { .. })
    ));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(second))).unwrap();
    settled(&mut state);
    assert_eq!(state.engine.linked_exile_records.len(), 2);
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(cathar, Zone::Hand),
    );
    assert_eq!(state.objects.get(first).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(second).zone, Zone::Battlefield);
    assert!(state.engine.linked_exile_records.is_empty());
    settled(&mut state);
}

#[test]
fn speed_increase_waits_on_the_stack_and_survives_its_original_source() {
    let mut state = ready();
    let source = put(
        &mut state,
        PlayerId::P0,
        "Burnout Bashtronaut",
        Zone::Battlefield,
    );
    next(&mut state);
    assert_eq!(speed(&state, PlayerId::P0), 1);
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(source, Zone::Graveyard),
    );
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::life_loss(PlayerId::P1, 1),
    );
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(speed(&state, PlayerId::P0), 1);
    assert!(state.stack.iter().any(|item| matches!(
        item.inline_effect,
        Some(mtg_kernel::effect::EffectOp::IncreaseSpeed {
            player: PlayerId::P0
        })
    )));
    settled(&mut state);
    assert_eq!(speed(&state, PlayerId::P0), 2);
}

#[test]
fn enduring_and_impending_enchantments_do_not_pay_creature_sacrifice_costs() {
    for name in [
        "Enduring Curiosity",
        "Enduring Innocence",
        "Overlord of the Mistmoors",
    ] {
        let mut state = ready();
        let enchantment = put(&mut state, PlayerId::P0, name, Zone::Battlefield);
        if name.starts_with("Enduring") {
            state
                .objects
                .get_mut(enchantment)
                .v4
                .enduring_enchantment_v1 = true;
        } else {
            state.objects.get_mut(enchantment).v4.time_counters_v1 = 3;
        }
        for _ in 0..2 {
            put(
                &mut state,
                PlayerId::P0,
                "Monastery Swiftspear",
                Zone::Battlefield,
            );
        }
        put(&mut state, PlayerId::P0, "Nova Hellkite", Zone::Graveyard);
        let dread = put(&mut state, PlayerId::P0, "Dread Return", Zone::Graveyard);
        let decision = next(&mut state);
        assert!(!format!("{decision:?}").is_empty());
        assert!(!engine::object_has_type(
            &state,
            enchantment,
            mtg_kernel::card_def::CardType::Creature
        ));
        assert!(
            !matches!(decision, Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&dread))
        );
    }
}

#[test]
fn monstrous_rage_roles_replace_only_the_same_controllers_old_role() {
    let mut state = ready();
    let host = put(
        &mut state,
        PlayerId::P1,
        "Novice Inspector",
        Zone::Battlefield,
    );
    for _ in 0..2 {
        let rage = put(&mut state, PlayerId::P0, "Monstrous Rage", Zone::Hand);
        add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 0);
        cast(&mut state, rage, &[Target::Object(host)]);
        settled(&mut state);
    }
    let roles = state.objects.get(host).attachments.clone();
    assert_eq!(roles.len(), 1);
    assert_eq!(power_toughness(&state, host), (6, 3));
    assert!(engine::has_effective_keyword(
        &state,
        host,
        Keywords::TRAMPLE
    ));
    state.priority_player = PlayerId::P1;
    let rage = put(&mut state, PlayerId::P1, "Monstrous Rage", Zone::Hand);
    add_mana(&mut state, PlayerId::P1, &[ManaColor::R], 0);
    cast(&mut state, rage, &[Target::Object(host)]);
    settled(&mut state);
    assert_eq!(state.objects.get(host).attachments.len(), 2);
    assert_eq!(power_toughness(&state, host), (9, 4));
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(host, Zone::Hand),
    );
    settled(&mut state);
    assert!(state
        .objects
        .iter()
        .all(|(_, object)| object.name != "Monster" || object.zone != Zone::Battlefield));
}

#[test]
fn casualty_copy_survives_original_countering_and_can_retarget() {
    let mut state = ready();
    let (_, burst) = make_disappear_a_burn(&mut state, true);
    let original = state.stack[1].source;
    assert!(!state.stack[2].is_copy);
    state.priority_player = PlayerId::P1;
    let negate = put(&mut state, PlayerId::P1, "Negate", Zone::Hand);
    add_mana(&mut state, PlayerId::P1, &[ManaColor::U], 1);
    cast(&mut state, negate, &[Target::Object(original)]);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets { .. })
    ));
    assert_eq!(state.objects.get(original).zone, Zone::Graveyard);
    let before = serde_json::to_string(&state).unwrap();
    let mut restored: GameState = serde_json::from_str(&before).unwrap();
    for state in [&mut state, &mut restored] {
        engine::step(state, Action::ChooseEffectTarget(Target::Object(burst))).unwrap();
        settled(state);
        assert_eq!(state.objects.get(burst).zone, Zone::Graveyard);
        assert_eq!(state.players[0].life, 20);
    }
    assert_eq!(state, restored);
}

fn library_card(state: &mut GameState, position: usize, name: &str) -> ObjectId {
    let id = state.players[0].library[position];
    let definition = card_id_by_name(name).unwrap();
    let card = state.objects.get_mut(id);
    card.card_def = definition;
    card.name = CARD_DEFS[definition as usize].object_name.into();
    card.v4 = ObjectStateV4::from_card_def(definition);
    id
}
fn disguised_bloom(state: &mut GameState) -> ObjectId {
    let bloom = put(state, PlayerId::P0, "Flourishing Bloom-Kin", Zone::Hand);
    add_mana(state, PlayerId::P0, &[], 3);
    cast(state, bloom, &[]);
    settled(state);
    assert!(state.objects.get(bloom).v4.face_down_v1.is_some());
    assert_eq!(power_toughness(state, bloom), (2, 2));
    bloom
}
#[test]
fn disguise_masks_all_public_characteristics_and_keeps_private_identity() {
    let mut state = ready();
    let bloom = disguised_bloom(&mut state);
    let surface = mtg_kernel::surface_v2::HarnessSurfaceV2::new();
    let own = mtg_kernel::rl::observe_v2(&state, &surface, PlayerId::P0, 0).unwrap();
    assert_eq!(own.known_face_down_cards.len(), 1);
    assert_eq!(
        own.known_face_down_cards[0].card_db_id,
        card_id_by_name("Flourishing Bloom-Kin").unwrap()
    );
    let opposing = mtg_kernel::rl::observe_v2(&state, &surface, PlayerId::P1, 0).unwrap();
    assert!(opposing.known_face_down_cards.is_empty());
    let json = serde_json::to_string(&opposing).unwrap();
    assert!(!json.contains("Flourishing Bloom-Kin"));
    let mut other = state.clone();
    other.objects.get_mut(bloom).card_def = card_id_by_name("Axebane Ferox").unwrap();
    other.objects.get_mut(bloom).name = "Axebane Ferox".into();
    assert_eq!(
        opposing,
        mtg_kernel::rl::observe_v2(&other, &surface, PlayerId::P1, 0).unwrap()
    );
    assert_eq!(power_toughness(&other, bloom), (2, 2));
    assert!(engine::effective_subtype_ids(&state, bloom).is_empty());
    assert_eq!(engine::object_color_mask(&state, bloom), 0);
}
#[test]
fn disguise_turns_face_up_as_special_action_and_chooses_both_forest_destinations() {
    let mut state = ready_with("Forest");
    put(&mut state, PlayerId::P0, "Forest", Zone::Battlefield);
    let bloom = disguised_bloom(&mut state);
    let first = state.players[0].library[0];
    let second = state.players[0].library[1];
    let spell = put(&mut state, PlayerId::P1, "Burst Lightning", Zone::Hand);
    state.priority_player = PlayerId::P1;
    add_mana(&mut state, PlayerId::P1, &[ManaColor::R], 0);
    cast(&mut state, spell, &[Target::Player(PlayerId::P0)]);
    next(&mut state);
    state.priority_player = PlayerId::P0;
    add_mana(&mut state, PlayerId::P0, &[ManaColor::G], 4);
    let original_len = state.stack.len();
    assert!(
        matches!(next(&mut state),Decision::CastSpellOrPass{activatable_abilities,..} if activatable_abilities.contains(&(bloom,255)))
    );
    let mut policy_state = state.clone();
    let mut surface = mtg_kernel::policy_surface_v5::PolicySurfaceV5::new();
    let decision = surface.next_decision(&mut policy_state).unwrap();
    let choices =
        mtg_kernel::rl::policy_legal_action_candidates_v5(&decision, &surface, &policy_state)
            .unwrap();
    assert!(choices.iter().any(|choice| matches!(
        choice.record.semantic,
        mtg_kernel::rl::ActionSemanticV1::TurnFaceUp { .. }
    )));
    engine::step(&mut state, Action::ActivateAbility(bloom, 255)).unwrap();
    assert!(state.objects.get(bloom).v4.face_down_v1.is_none());
    assert_eq!(state.priority_player, PlayerId::P0);
    assert_eq!(state.stack.len(), original_len + 1);
    assert_eq!(
        state.stack.last().unwrap().kind,
        mtg_kernel::state::StackItemKind::TriggeredAbility
    );
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets { .. })
    ));
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(first)),
    )
    .unwrap();
    next(&mut state);
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(second)),
    )
    .unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(first).zone, Zone::Battlefield);
    assert!(state.objects.get(first).tapped);
    assert_eq!(state.objects.get(second).zone, Zone::Hand);
    assert_eq!(state.players[0].life, 18);
}
fn glyph_on_clue(state: &mut GameState) -> ObjectId {
    let clue = put(state, PlayerId::P0, "Clue Token", Zone::Battlefield);
    let glyph = put(state, PlayerId::P0, "Zoetic Glyph", Zone::Hand);
    add_mana(state, PlayerId::P0, &[ManaColor::U], 2);
    cast(state, glyph, &[Target::Object(clue)]);
    settled(state);
    assert_eq!(power_toughness(state, clue), (5, 4));
    assert!(engine::object_has_type(
        state,
        clue,
        mtg_kernel::card_def::CardType::Artifact
    ));
    assert!(engine::object_has_type(
        state,
        clue,
        mtg_kernel::card_def::CardType::Creature
    ));
    glyph
}
#[test]
fn zoetic_discover_can_decline_or_cast_without_changing_the_unseen_library_tail() {
    for play in [false, true] {
        let mut state = ready();
        let skipped = library_card(&mut state, 0, "Mountain");
        let hit = library_card(&mut state, 1, "Novice Inspector");
        let tail = state.players[0].library[2..].to_vec();
        let glyph = glyph_on_clue(&mut state);
        mtg_kernel::event::propose_and_commit(
            &mut state,
            mtg_kernel::event::ProposedEvent::zone_change(glyph, Zone::Graveyard),
        );
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectBoolean { .. })
        ));
        assert_eq!(state.objects.get(hit).zone, Zone::Exile);
        let encoded = serde_json::to_string(&state).unwrap();
        state = serde_json::from_str(&encoded).unwrap();
        engine::step(&mut state, Action::ChooseEffectBoolean(play)).unwrap();
        settled(&mut state);
        assert_eq!(
            state.objects.get(hit).zone,
            if play { Zone::Battlefield } else { Zone::Hand }
        );
        assert_eq!(&state.players[0].library[..tail.len()], tail.as_slice());
        assert_eq!(state.players[0].library.last(), Some(&skipped));
    }
}
#[test]
fn cage_hideaway_is_private_and_can_play_a_land_after_the_counter_creates_coven() {
    let mut state = ready();
    let hidden = library_card(&mut state, 1, "Forest");
    let cage = put(&mut state, PlayerId::P0, "Collector's Cage", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::W], 1);
    cast(&mut state, cage, &[]);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets { .. })
    ));
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(hidden)),
    )
    .unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(hidden).zone, Zone::Exile);
    let surface = mtg_kernel::surface_v2::HarnessSurfaceV2::new();
    let hidden_view = mtg_kernel::rl::observe_v2(&state, &surface, PlayerId::P1, 0).unwrap();
    assert!(hidden_view.known_face_down_cards.is_empty());
    let mut alternative = state.clone();
    alternative.objects.get_mut(hidden).card_def = card_id_by_name("Axebane Ferox").unwrap();
    alternative.objects.get_mut(hidden).name = "Axebane Ferox".into();
    assert_eq!(
        hidden_view,
        mtg_kernel::rl::observe_v2(&alternative, &surface, PlayerId::P1, 0).unwrap()
    );
    let target = put(
        &mut state,
        PlayerId::P0,
        "Novice Inspector",
        Zone::Battlefield,
    );
    put(
        &mut state,
        PlayerId::P0,
        "Monastery Swiftspear",
        Zone::Battlefield,
    );
    put(
        &mut state,
        PlayerId::P0,
        "Sanguine Evangelist",
        Zone::Battlefield,
    );
    add_mana(&mut state, PlayerId::P0, &[], 1);
    engine::step(&mut state, Action::ActivateAbility(cage, 0)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectBoolean { .. })
    ));
    engine::step(&mut state, Action::ChooseEffectBoolean(true)).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(hidden).zone, Zone::Battlefield);
    assert!(state.objects.get(hidden).v4.face_down_v1.is_none());
    assert_eq!(state.objects.get(target).counters.plus1_plus1, 1);
    assert!(state.objects.get(cage).tapped);
}

#[test]
fn tersa_discard_choice_draws_exactly_zero_one_or_two_and_survives_restore() {
    for count in 0..=2 {
        let mut state = ready();
        let first = put(&mut state, PlayerId::P0, "Mountain", Zone::Hand);
        let second = put(&mut state, PlayerId::P0, "Forest", Zone::Hand);
        let tersa = put(&mut state, PlayerId::P0, "Tersa Lightshatter", Zone::Hand);
        add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 2);
        cast(&mut state, tersa, &[]);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectTargets { .. })
        ));
        assert!(engine::has_effective_keyword(
            &state,
            tersa,
            Keywords::HASTE
        ));
        let library_count = state.players[0].library.len();
        state = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        for card in [first, second].into_iter().take(count) {
            engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(card))).unwrap();
            if count == 2 && card == first {
                next(&mut state);
            }
        }
        if count < 2 {
            engine::step(&mut state, Action::FinishEffectSelection).unwrap();
        }
        settled(&mut state);
        assert_eq!(state.players[0].hand.len(), 2);
        assert_eq!(state.players[0].library.len(), library_count - count);
        assert_eq!(state.players[0].graveyard.len(), count);
    }
}
#[test]
fn tersa_threshold_random_exile_is_replayable_and_requires_normal_mana() {
    let mut state = ready();
    let tersa = put(
        &mut state,
        PlayerId::P0,
        "Tersa Lightshatter",
        Zone::Battlefield,
    );
    for _ in 0..7 {
        put(&mut state, PlayerId::P0, "Burst Lightning", Zone::Graveyard);
    }
    attack_with(&mut state, vec![tersa]);
    let library = state.players[0].library.clone();
    let mut restored: GameState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    for state in [&mut state, &mut restored] {
        settled(state);
        assert_eq!(state.exile.len(), 1);
        assert_eq!(state.players[0].graveyard.len(), 6);
        assert_eq!(state.players[0].library, library);
        let card = state.exile[0];
        state.priority_player = PlayerId::P0;
        assert!(!castable(state, card), "Tersa does not waive the mana cost");
        add_mana(state, PlayerId::P0, &[ManaColor::R], 0);
        cast(state, card, &[Target::Player(PlayerId::P1)]);
        settled(state);
        assert_eq!(state.objects.get(card).zone, Zone::Graveyard);
    }
    assert_eq!(state, restored);
}
#[test]
fn tersa_threshold_rechecks_on_resolution_and_permission_expires_at_cleanup() {
    for remove_card in [false, true] {
        let mut state = ready();
        let tersa = put(
            &mut state,
            PlayerId::P0,
            "Tersa Lightshatter",
            Zone::Battlefield,
        );
        for _ in 0..7 {
            put(&mut state, PlayerId::P0, "Mountain", Zone::Graveyard);
        }
        attack_with(&mut state, vec![tersa]);
        next(&mut state);
        assert!(!state.stack.is_empty());
        if remove_card {
            let card = state.players[0].graveyard[0];
            mtg_kernel::event::propose_and_commit(
                &mut state,
                mtg_kernel::event::ProposedEvent::zone_change(card, Zone::Hand),
            );
        }
        settled(&mut state);
        assert_eq!(state.exile.len(), usize::from(!remove_card));
        if !remove_card {
            assert_eq!(state.engine.exile_play_permissions.len(), 1);
            pass_until(&mut state, |s| {
                s.active_player == PlayerId::P1 && s.step == Step::Main1
            });
            assert!(state.engine.exile_play_permissions.is_empty());
            assert_eq!(state.exile.len(), 1);
        }
    }
}
