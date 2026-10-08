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
        "Burnout Bashtronaut",
        "Nova Hellkite",
        "Full Bore",
        "Iridescent Vinelasher",
        "Iridescent Vinelasher Offspring Token",
        "Aloe Alchemist",
        "Forsaken Miner",
        "Axebane Ferox",
        "Hopeful Initiate",
        "Chrome Host Seedshark",
        "Incubator Token",
        "Brutal Cathar",
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
    settled(&mut state);
    assert_eq!(state.objects.get(wellspring).zone, Zone::Graveyard);
    // Each counter comes off the creature with the most: the Challenger's
    // first, then (tied at one) the earlier Initiate's.
    assert_eq!(state.objects.get(challenger).counters.plus1_plus1, 1);
    assert_eq!(state.objects.get(initiate).counters.plus1_plus1, 0);
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
    burn(&mut state, PlayerId::P1, Target::Object(cathar));
    assert_eq!(state.players[1].life, 17);
    assert_eq!(state.objects.get(cathar).damage, 2);

    // At 3 life P1 can't pay without dying, so the spell is countered.
    state.players[1].life = 3;
    burn(&mut state, PlayerId::P1, Target::Object(cathar));
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
