//! Eternal wave S1c: native rules tests for the Vintage-only sideboard cards
//! (Argentum Masticore, Hullbreacher, Wurmcoil Engine and the rest of the
//! wave's cards).
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardDef, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, CastMode, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

/// A first main phase with P0 active and holding priority. Each library
/// holds `library` in order, top first.
fn game(library: &[&str]) -> GameState {
    let ids: Vec<u16> = library.iter().map(|name| id(name)).collect();
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x4539_4221_0001,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    state
}

fn plains() -> GameState {
    game(&["Plains"; 20])
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = id(name);
    let object = state.objects.push(GameObject {
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
        Zone::Hand => seat.hand.push(object),
        Zone::Battlefield => seat.battlefield.push(object),
        Zone::Graveyard => seat.graveyard.push(object),
        Zone::Exile => state.exile.push(object),
        Zone::Library => seat.library.insert(0, object),
        _ => panic!("helper zone"),
    }
    object
}

fn pool(state: &mut GameState, player: PlayerId, colored: &[ManaColor], colorless: u8) {
    let pool = &mut state.players[player.index()].mana_pool;
    for color in colored {
        pool[color.pool_index()] += 1;
    }
    pool[ManaColor::C.pool_index()] += colorless;
}

fn zone(state: &GameState, object: ObjectId) -> Zone {
    state.objects.get(object).zone
}

fn named(state: &GameState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    [P0, P1]
        .into_iter()
        .flat_map(|seat| state.players[seat.index()].battlefield.iter().copied())
        .filter(|&object| {
            let live = state.objects.get(object);
            live.controller == player && CARD_DEFS[live.card_def as usize].name == name
        })
        .collect()
}

fn power_toughness(state: &GameState, object: ObjectId) -> (i32, i32) {
    (
        engine::effective_power(state, object),
        engine::effective_toughness(state, object),
    )
}

fn keyword(state: &GameState, object: ObjectId, keyword: Keywords) -> bool {
    engine::has_effective_keyword(state, object, keyword)
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(
        !matches!(decision, Decision::Halted { .. }),
        "{decision:?}\nstack: {:#?}\npending triggers: {:#?}",
        state.stack,
        state.engine.pending_triggers
    );
    decision
}

fn act(state: &mut GameState, action: Action) {
    engine::step(state, action.clone()).unwrap_or_else(|error| panic!("{action:?}: {error}"));
}

/// Passes priority and orders triggers until the stack and pending triggers
/// are empty, returning the first other decision if one interrupts.
fn settle(state: &mut GameState) -> Option<Decision> {
    for _ in 0..200 {
        match next(state) {
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return None
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
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

/// The first legal answer to any choice: the first target or option,
/// finishing a selection that may finish, declining a Boolean or cost.
fn first_answer(decision: Decision) -> Action {
    match decision {
        Decision::ChooseTargets { legal_targets, .. } => Action::ChooseTarget(legal_targets[0]),
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            Action::ChooseEffectTarget(legal_targets[0])
        }
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(0),
        Decision::ChooseEffectBoolean { .. } => Action::ChooseEffectBoolean(false),
        Decision::ChooseCostTargets { candidates, .. } => Action::ChooseCostTarget(candidates[0]),
        Decision::ChooseCastMode { options, .. } => Action::ChooseCastMode(options[0]),
        Decision::ChooseKicker { .. } => Action::ChooseKicker(false),
        Decision::ChooseSpellMode { legal_modes, .. } => Action::ChooseSpellMode(legal_modes[0]),
        Decision::ChooseTriggerMode { legal_modes, .. } => {
            Action::ChooseTriggerMode(legal_modes[0])
        }
        Decision::ChooseOptionalCost { .. } => {
            Action::ChooseOptionalCost(OptionalCostChoice::Decline)
        }
        Decision::Discard { count, choices, .. } => {
            Action::Discard(choices.into_iter().take(count as usize).collect())
        }
        other => panic!("unexpected choice: {other:?}"),
    }
}

/// Settles, answering every choice with `answer`.
fn settle_with(state: &mut GameState, mut answer: impl FnMut(&GameState, Decision) -> Action) {
    while let Some(decision) = settle(state) {
        let action = answer(state, decision);
        act(state, action);
    }
}

fn settle_first(state: &mut GameState) {
    settle_with(state, |_, decision| first_answer(decision));
}

fn priority(state: &mut GameState) -> (Vec<ObjectId>, Vec<(ObjectId, u8)>) {
    match next(state) {
        Decision::CastSpellOrPass {
            castable_spells,
            activatable_abilities,
            ..
        } => (castable_spells, activatable_abilities),
        other => panic!("expected priority, got {other:?}"),
    }
}

fn castable(state: &mut GameState, spell: ObjectId) -> bool {
    priority(state).0.contains(&spell)
}

fn activatable(state: &mut GameState, source: ObjectId, index: u8) -> bool {
    priority(state).1.contains(&(source, index))
}

/// Casts `spell`, answering each target prompt from `targets` in order.
fn cast(state: &mut GameState, spell: ObjectId, targets: &[Target]) {
    assert!(
        castable(state, spell),
        "{} not castable",
        state.objects.get(spell).name
    );
    act(state, Action::CastSpell(spell));
    for target in targets {
        choose_target(state, *target);
    }
}

fn choose_target(state: &mut GameState, target: Target) {
    match next(state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(
                legal_targets.contains(&target),
                "{target:?} not in {legal_targets:?}"
            );
            act(state, Action::ChooseTarget(target));
        }
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert!(
                legal_targets.contains(&target),
                "{target:?} not in {legal_targets:?}"
            );
            act(state, Action::ChooseEffectTarget(target));
        }
        other => panic!("expected targets, got {other:?}"),
    }
}

/// The targets `spell` may choose first.
fn cast_targets(state: &GameState, spell: ObjectId) -> Vec<Target> {
    let mut probe = state.clone();
    assert!(castable(&mut probe, spell));
    act(&mut probe, Action::CastSpell(spell));
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. } => legal_targets,
        other => panic!("expected targets, got {other:?}"),
    }
}

/// Activates `source`'s ability `index`, answering each target prompt.
fn activate(state: &mut GameState, source: ObjectId, index: u8, targets: &[Target]) {
    assert!(
        activatable(state, source, index),
        "{} ability {index} not activatable",
        state.objects.get(source).name
    );
    act(state, Action::ActivateAbility(source, index));
    for target in targets {
        choose_target(state, *target);
    }
}

/// Plays on until `stop` holds before a decision: passing priority, not
/// attacking or blocking, ordering triggers as given and answering any
/// other choice with `answer` (or the first legal answer).
fn run_until(
    state: &mut GameState,
    stop: impl Fn(&GameState) -> bool,
    mut answer: impl FnMut(&GameState, &Decision) -> Option<Action>,
) {
    for _ in 0..2000 {
        let decision = next(state);
        if stop(state) {
            return;
        }
        if let Some(action) = answer(state, &decision) {
            act(state, action);
            continue;
        }
        let action = match decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::DeclareAttackers { .. } => Action::DeclareAttackers(Vec::new()),
            Decision::DeclareBlockers { .. } => Action::DeclareBlockers(Vec::new()),
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            Decision::ChooseCombatDamageRange { .. } => {
                Action::ChooseCombatDamageRange { upper_half: true }
            }
            Decision::ChooseAttackTarget {
                player, candidates, ..
            } => Action::ChooseAttackTarget(
                if candidates.contains(&Target::Player(player.opponent())) {
                    Target::Player(player.opponent())
                } else {
                    candidates[0]
                },
            ),
            other => first_answer(other),
        };
        act(state, action);
    }
    panic!("condition never held");
}

/// Plays on with first answers until `stop` holds.
fn pass_until(state: &mut GameState, stop: impl Fn(&GameState) -> bool) {
    run_until(state, stop, |_, _| None);
}

/// P0 attacks with `attacker`; the creatures P1 may block it with.
fn legal_blockers(state: &mut GameState, attacker: ObjectId) -> Vec<ObjectId> {
    pass_until(state, |state| state.step == Step::DeclareAttackers);
    act(state, Action::DeclareAttackers(vec![attacker]));
    for _ in 0..50 {
        match next(state) {
            Decision::DeclareBlockers { legal_blockers, .. } => {
                return legal_blockers
                    .into_iter()
                    .find(|(candidate, _)| *candidate == attacker)
                    .map(|(_, blockers)| blockers)
                    .unwrap_or_default();
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => panic!("expected blockers, got {other:?}"),
        }
    }
    panic!("no declare blockers decision");
}

fn hand_contains(state: &GameState, player: PlayerId, object: ObjectId) -> bool {
    state.players[player.index()].hand.contains(&object)
}

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
}

const S1C_CARDS: [&str; 19] = [
    "Argentum Masticore",
    "Dark Petition",
    "Feed the Swarm",
    "Hullbreacher",
    "Hurkyl's Recall",
    "Light of Day",
    "Lithomantic Barrage",
    "Long Goodbye",
    "Mystical Dispute",
    "Rebuild",
    "Ravenous Trap",
    "Snapback",
    "Spell Snare",
    "Sphinx of the Steel Wind",
    "Urborg, Tomb of Yawgmoth",
    "Virtue's Ruin",
    "Wurmcoil Engine",
    "Deathtouch Wurm Token",
    "Lifelink Wurm Token",
];

fn eligible_attackers(state: &mut GameState) -> Vec<ObjectId> {
    pass_until(state, |state| state.step == Step::DeclareAttackers);
    match next(state) {
        Decision::DeclareAttackers { eligible, .. } => eligible,
        other => panic!("expected attackers, got {other:?}"),
    }
}

#[test]
fn s1c_cards_keep_their_support_labels() {
    for name in S1C_CARDS {
        assert_eq!(
            def(name).capability,
            crate::eternal_catalog_v1::expected_capability(name),
            "{name}"
        );
    }
}

#[test]
fn spell_snare_targets_only_mana_value_two_spells() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Dauthi Voidwalker", Zone::Hand);
    let snare = put(&mut state, P0, "Spell Snare", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::B, ManaColor::B, ManaColor::U],
        0,
    );
    cast(&mut state, inspector, &[]);
    assert!(
        cast_targets(&state, snare).contains(&Target::Object(inspector)),
        "a mana value 2 spell"
    );
    cast(&mut state, snare, &[Target::Object(inspector)]);
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Graveyard);
    assert_eq!(zone(&state, snare), Zone::Graveyard);

    let mut state = plains();
    let breacher = put(&mut state, P0, "Hullbreacher", Zone::Hand);
    let snare = put(&mut state, P0, "Spell Snare", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 2);
    cast(&mut state, breacher, &[]);
    assert!(
        !castable(&mut state, snare),
        "a mana value 3 spell is not a legal target"
    );
}

#[test]
fn hurkyls_recall_returns_only_the_targeted_players_artifacts() {
    let mut state = plains();
    let theirs = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let mine = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let recall = put(&mut state, P0, "Hurkyl's Recall", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, recall, &[Target::Player(P1)]);
    settled(&mut state);
    assert!(hand_contains(&state, P1, theirs));
    assert_eq!(zone(&state, mine), Zone::Battlefield);
}

#[test]
fn rebuild_returns_every_artifact_and_cycles() {
    let mut state = plains();
    let theirs = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let mine = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let rebuild = put(&mut state, P0, "Rebuild", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, rebuild, &[]);
    settled(&mut state);
    assert!(hand_contains(&state, P1, theirs));
    assert!(hand_contains(&state, P0, mine));

    let mut state = plains();
    let rebuild = put(&mut state, P0, "Rebuild", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    let hand = state.players[0].hand.len();
    activate(&mut state, rebuild, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, rebuild), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), hand, "one card out, one drawn");
}

/// P1 casts `spell` (paid with `colored` and `colorless` mana) at instant
/// speed after P0 passes, then passes so that P0 holds priority with the
/// spell on the stack.
fn opponent_casts(
    state: &mut GameState,
    spell: ObjectId,
    colored: &[ManaColor],
    colorless: u8,
    targets: &[Target],
) {
    next(state);
    act(state, Action::Pass);
    pool(state, P1, colored, colorless);
    cast(state, spell, targets);
    next(state);
    act(state, Action::Pass);
    next(state);
}

#[test]
fn mystical_dispute_costs_two_less_only_against_a_blue_spell() {
    // Against a blue spell it costs {U}.
    let mut state = plains();
    let breacher = put(&mut state, P1, "Hullbreacher", Zone::Hand);
    let dispute = put(&mut state, P0, "Mystical Dispute", Zone::Hand);
    opponent_casts(&mut state, breacher, &[ManaColor::U], 2, &[]);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, dispute, &[Target::Object(breacher)]);
    settle_first(&mut state);
    assert_eq!(
        zone(&state, breacher),
        Zone::Graveyard,
        "countered unless {{3}}"
    );

    // Against a nonblue spell it costs the full {2}{U}.
    let mut state = plains();
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let dispute = put(&mut state, P0, "Mystical Dispute", Zone::Hand);
    opponent_casts(&mut state, bolt, &[ManaColor::R], 0, &[Target::Player(P0)]);
    pool(&mut state, P0, &[ManaColor::U], 0);
    assert!(!castable(&mut state, dispute), "{{U}} is not enough");
    pool(&mut state, P0, &[], 2);
    assert!(castable(&mut state, dispute), "{{2}}{{U}}");
}

#[test]
fn long_goodbye_destroys_a_small_creature_but_not_a_big_one() {
    let mut state = plains();
    let small = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let big = put(&mut state, P1, "Wurmcoil Engine", Zone::Battlefield);
    let goodbye = put(&mut state, P0, "Long Goodbye", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    let targets = cast_targets(&state, goodbye);
    assert!(targets.contains(&Target::Object(small)));
    assert!(!targets.contains(&Target::Object(big)), "mana value 6");
    cast(&mut state, goodbye, &[Target::Object(small)]);
    settled(&mut state);
    assert_eq!(zone(&state, small), Zone::Graveyard);
    assert!(def("Long Goodbye").spell_cannot_be_countered);
}

#[test]
fn snapback_exiles_a_blue_card_to_bounce_a_creature() {
    let mut state = plains();
    let victim = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let fodder = put(&mut state, P0, "Spell Snare", Zone::Hand);
    let snapback = put(&mut state, P0, "Snapback", Zone::Hand);
    assert!(castable(&mut state, snapback), "no mana needed");
    act(&mut state, Action::CastSpell(snapback));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { options, .. } => {
            assert!(options.contains(&CastMode::Alternative));
            Action::ChooseCastMode(CastMode::Alternative)
        }
        Decision::ChooseCostTargets { candidates, .. } => {
            assert_eq!(candidates, vec![fodder]);
            Action::ChooseCostTarget(fodder)
        }
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(victim)),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, fodder), Zone::Exile);
    assert!(hand_contains(&state, P1, victim));
}

#[test]
fn hullbreacher_replaces_all_but_the_draw_step_draw() {
    let mut state = plains();
    put(&mut state, P1, "Hullbreacher", Zone::Battlefield);
    let before = state.players[0].hand.len();
    event::propose_and_commit(&mut state, ProposedEvent::draw(P0));
    event::propose_and_commit(&mut state, ProposedEvent::draw(P0));
    assert_eq!(state.players[0].hand.len(), before, "both draws replaced");
    assert_eq!(named(&state, P1, "Treasure Token").len(), 2);

    // The draw step's own draw is not replaced.
    let mut state = plains();
    put(&mut state, P1, "Hullbreacher", Zone::Battlefield);
    pass_until(&mut state, |state| {
        state.turn >= 3 && state.active_player == P0 && state.step == Step::Main1
    });
    assert!(named(&state, P1, "Treasure Token").is_empty());
}

#[test]
fn hullbreacher_and_dredge_compete_for_the_monarchs_draw() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = plains();
        state.active_player = actor;
        state.priority_player = actor;
        put(&mut state, opponent, "Hullbreacher", Zone::Battlefield);
        let sentinels = put(&mut state, actor, "Palace Sentinels", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::W], 3);
        cast(&mut state, sentinels, &[]);
        settled(&mut state);
        assert_eq!(state.monarch, Some(actor));
        assert_eq!(zone(&state, sentinels), Zone::Battlefield);
        let thug = put(&mut state, actor, "Golgari Thug", Zone::Graveyard);
        let library = state.players[actor.index()].library.clone();

        pass_until(&mut state, |state| state.step == Step::End);
        let prompt = settle(&mut state).expect("the monarch's actual draw");
        assert!(matches!(
            &prompt,
            Decision::ChooseEffectTargets {
                player, source, min_targets: 0, max_targets: 1, legal_targets, can_finish: true, ..
            } if *player == actor && *source == sentinels && legal_targets.as_slice() == [Target::Object(thug)]
        ));
        assert!(named(&state, opponent, "Treasure Token").is_empty());
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(prompt, next(&mut restored));
        let mut stale = restored.clone();
        stale.objects.get_mut(thug).zone_change_count += 1;
        assert!(mtg_kernel::effect::validate_pending_effect_choice(&stale).is_err());

        let mut treasure = state.clone();
        act(&mut treasure, Action::FinishEffectSelection);
        settled(&mut treasure);
        assert_eq!(named(&treasure, opponent, "Treasure Token").len(), 1);
        assert_eq!(zone(&treasure, thug), Zone::Graveyard);
        assert_eq!(treasure.players[actor.index()].library, library);
        assert_eq!(treasure.players[actor.index()].draws_this_turn, 0);

        for branch in [&mut state, &mut restored] {
            act(branch, Action::ChooseEffectTarget(Target::Object(thug)));
            settled(branch);
            assert_eq!(zone(branch, thug), Zone::Hand);
            assert!(library[..4]
                .iter()
                .all(|&card| zone(branch, card) == Zone::Graveyard));
            assert_eq!(branch.players[actor.index()].library, library[4..]);
            assert_eq!(branch.players[actor.index()].draws_this_turn, 0);
            assert!(named(branch, opponent, "Treasure Token").is_empty());
        }
        assert_eq!(state.state_hash(), restored.state_hash());
    }
}

#[test]
fn hullbreacher_and_dredge_compete_for_bauble_upkeep_draws_both_seats() {
    use mtg_kernel::effect::{EffectTargetSelectionPurpose, PendingEffectChoice};

    for actor in [P0, P1] {
        for name in ["Mishra's Bauble", "Urza's Bauble"] {
            let opponent = actor.opponent();
            let mut state = plains();
            state.active_player = actor;
            state.priority_player = actor;
            let bauble = put(&mut state, actor, name, Zone::Hand);
            let hull = put(&mut state, opponent, "Hullbreacher", Zone::Hand);
            let hidden = put(&mut state, opponent, "Lightning Bolt", Zone::Hand);
            let thug = put(&mut state, actor, "Golgari Thug", Zone::Graveyard);
            cast(&mut state, bauble, &[]);
            settled(&mut state);
            let bauble_generation = state.objects.get(bauble).zone_change_count;
            assert_eq!(zone(&state, bauble), Zone::Battlefield);
            act(&mut state, Action::Pass);
            assert!(
                matches!(next(&mut state), Decision::CastSpellOrPass { player, .. }
                if player == opponent)
            );
            pool(&mut state, opponent, &[ManaColor::U], 2);
            cast(&mut state, hull, &[]);
            settled(&mut state);
            assert_eq!(zone(&state, hull), Zone::Battlefield);
            assert_eq!(state.players[opponent.index()].mana_pool, [0; 6]);

            let top = state.players[opponent.index()].library[0];
            activate(&mut state, bauble, 0, &[Target::Player(opponent)]);
            settled(&mut state);
            assert_eq!(zone(&state, bauble), Zone::Graveyard);
            let delayed = &state.eternal_v1.as_ref().unwrap().upkeep_draws;
            assert_eq!(delayed.len(), 1);
            assert_eq!(delayed[0].controller, actor);
            assert_eq!(delayed[0].source.source, bauble);
            assert_eq!(delayed[0].source.zone_change_count, bauble_generation);
            if name == "Mishra's Bauble" {
                assert!(state
                    .known_library_cards(actor, opponent)
                    .iter()
                    .any(|entry| { entry.position == 0 && entry.object == top }));
            } else {
                assert!(state
                    .known_hand_cards(actor, opponent)
                    .iter()
                    .any(|entry| entry.object == hidden));
            }
            assert!(named(&state, opponent, "Treasure Token").is_empty());
            let library = state.players[actor.index()].library.clone();
            pass_until(&mut state, |state| {
                state.active_player == opponent && state.step == Step::Upkeep
            });
            let prompt = settle(&mut state).expect("the Bauble's actual delayed draw");
            assert!(matches!(
                &prompt,
                Decision::ChooseEffectTargets {
                    player, source, min_targets: 0, max_targets: 1, legal_targets, can_finish: true, ..
                } if *player == actor && *source == bauble && legal_targets.as_slice() == [Target::Object(thug)]
            ));
            let suspended = state.engine.pending_effect.as_ref().unwrap();
            assert_eq!(suspended.resolving_item.controller, actor);
            assert_eq!(
                suspended
                    .resolving_item
                    .v4
                    .ability_source_contract
                    .unwrap()
                    .zone_change_count,
                bauble_generation
            );
            assert!(named(&state, opponent, "Treasure Token").is_empty());
            assert_eq!(state.players[actor.index()].library, library);

            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(prompt, next(&mut restored));
            let mut forged = restored.clone();
            let Some(PendingEffectChoice::SelectTargets {
                player,
                purpose: EffectTargetSelectionPurpose::EternalV1 { choice },
                ..
            }) = forged
                .engine
                .pending_effect
                .as_mut()
                .unwrap()
                .choice
                .as_mut()
            else {
                panic!("the native Bauble draw must own an Eternal choice");
            };
            *player = opponent;
            choice.player = opponent;
            assert_eq!(
                mtg_kernel::effect::validate_pending_effect_choice(&forged).unwrap_err(),
                "Bauble draw changed its native replacement choice",
            );
            let before = forged.state_hash();
            assert!(engine::step(&mut forged, Action::FinishEffectSelection).is_err());
            assert_eq!(forged.state_hash(), before);

            let mut stale = restored.clone();
            stale.objects.get_mut(thug).zone_change_count += 1;
            assert!(mtg_kernel::effect::validate_pending_effect_choice(&stale).is_err());

            act(&mut restored, Action::FinishEffectSelection);
            settled(&mut restored);
            assert_eq!(named(&restored, opponent, "Treasure Token").len(), 1);
            assert_eq!(zone(&restored, thug), Zone::Graveyard);
            assert_eq!(restored.players[actor.index()].library, library);
            assert_eq!(restored.players[actor.index()].draws_this_turn, 0);

            act(&mut state, Action::ChooseEffectTarget(Target::Object(thug)));
            let mut answered: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut answered] {
                settled(branch);
                assert_eq!(zone(branch, thug), Zone::Hand);
                assert!(library[..4]
                    .iter()
                    .all(|&card| zone(branch, card) == Zone::Graveyard));
                assert_eq!(branch.players[actor.index()].library, library[4..]);
                assert_eq!(branch.players[actor.index()].draws_this_turn, 0);
                assert!(named(branch, opponent, "Treasure Token").is_empty());
                assert_eq!(branch.step, Step::Upkeep);
            }
            assert_eq!(state.state_hash(), answered.state_hash());
        }
    }
}

#[test]
fn dredging_the_turn_based_draw_exempts_only_the_first_actual_draw() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = plains();
        state.turn = 2;
        state.active_player = actor;
        state.priority_player = actor;
        state.step = Step::Upkeep;
        put(&mut state, opponent, "Hullbreacher", Zone::Battlefield);
        put(&mut state, opponent, "Orcish Bowmasters", Zone::Battlefield);
        let imp = put(&mut state, actor, "Stinkweed Imp", Zone::Graveyard);

        pass_until(&mut state, |state| state.step == Step::Draw);
        assert!(matches!(
            next(&mut state),
            Decision::ChooseEffectOption { player, source, option_count: 2 }
                if player == actor && source == imp
        ));
        act(&mut state, Action::ChooseEffectOption(1));
        assert_eq!(zone(&state, imp), Zone::Hand);
        assert_eq!(state.players[actor.index()].draws_this_turn, 0);
        let top = state.players[actor.index()].library[0];
        let second = state.players[actor.index()].library[1];

        for (card, expected_zone, treasures) in [(top, Zone::Hand, 0), (second, Zone::Library, 1)] {
            let spell = put(&mut state, actor, "Opt", Zone::Hand);
            pool(&mut state, actor, &[ManaColor::U], 0);
            cast(&mut state, spell, &[]);
            assert!(matches!(
                settle(&mut state),
                Some(Decision::ChooseEffectTargets { player, source, legal_targets, can_finish: true, .. })
                    if player == actor && source == spell && legal_targets == [Target::Object(card)]
            ));
            act(&mut state, Action::FinishEffectSelection);
            settled(&mut state);
            assert_eq!(state.step, Step::Draw);
            assert_eq!(zone(&state, card), expected_zone);
            assert_eq!(state.players[actor.index()].draws_this_turn, 1);
            assert_eq!(named(&state, opponent, "Treasure Token").len(), treasures);
            assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        }
    }
}

#[test]
fn hullbreacher_and_dredge_compete_for_bonders_ornament_draws() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = plains();
        state.active_player = actor;
        state.priority_player = actor;
        put(&mut state, opponent, "Hullbreacher", Zone::Battlefield);
        let ornament = put(&mut state, actor, "Bonder's Ornament", Zone::Battlefield);
        put(&mut state, opponent, "Bonder's Ornament", Zone::Battlefield);
        let thug = put(&mut state, actor, "Golgari Thug", Zone::Graveyard);
        let opponent_top = state.players[opponent.index()].library[0];
        pool(&mut state, actor, &[], 4);
        activate(&mut state, ornament, 0, &[]);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectTargets { player, source, legal_targets, .. })
                if player == actor && source == ornament && legal_targets == [Target::Object(thug)]
        ));
        assert!(state.objects.get(ornament).tapped);
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        assert_eq!(
            zone(&state, opponent_top),
            Zone::Library,
            "active player chooses first"
        );
        act(&mut state, Action::ChooseEffectTarget(Target::Object(thug)));
        settled(&mut state);
        assert_eq!(zone(&state, thug), Zone::Hand);
        assert_eq!(state.players[actor.index()].library.len(), 16);
        assert_eq!(zone(&state, opponent_top), Zone::Hand);
        assert!(named(&state, opponent, "Treasure Token").is_empty());
    }
}

#[test]
fn discard_draw_can_dredge_the_card_it_just_discarded() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = plains();
        state.active_player = actor;
        state.priority_player = actor;
        put(&mut state, opponent, "Hullbreacher", Zone::Battlefield);
        let thug = put(&mut state, actor, "Golgari Thug", Zone::Hand);
        let tersa = put(&mut state, actor, "Tersa Lightshatter", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::R], 2);
        cast(&mut state, tersa, &[]);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectTargets { player, source, legal_targets, .. })
                if player == actor && source == tersa && legal_targets == [Target::Object(thug)]
        ));
        act(&mut state, Action::ChooseEffectTarget(Target::Object(thug)));
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectTargets { player, source, legal_targets, .. })
                if player == actor && source == tersa && legal_targets == [Target::Object(thug)]
        ));
        assert_eq!(zone(&state, thug), Zone::Graveyard);
        act(&mut state, Action::ChooseEffectTarget(Target::Object(thug)));
        settled(&mut state);
        assert_eq!(zone(&state, thug), Zone::Hand);
        assert_eq!(state.players[actor.index()].library.len(), 16);
        assert!(named(&state, opponent, "Treasure Token").is_empty());
    }
}

#[test]
fn empty_library_draw_chooses_between_jace_and_hullbreacher() {
    use mtg_kernel::effect::{EffectTargetSelectionPurpose, PendingEffectChoice};

    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(&[]);
        state.active_player = actor;
        state.priority_player = actor;
        put(&mut state, opponent, "Hullbreacher", Zone::Battlefield);
        let jace = put(&mut state, actor, "Jace, Wielder of Mysteries", Zone::Hand);
        pool(
            &mut state,
            actor,
            &[ManaColor::U, ManaColor::U, ManaColor::U],
            1,
        );
        cast(&mut state, jace, &[]);
        settled(&mut state);
        let spell = put(&mut state, actor, "Opt", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::U], 0);
        cast(&mut state, spell, &[]);
        let prompt = settle(&mut state).expect("empty-library replacement choice");
        assert!(matches!(
            &prompt,
            Decision::ChooseEffectTargets {
                player, source, legal_targets, min_targets: 0, max_targets: 1, can_finish: true, ..
            } if *player == actor && *source == spell && legal_targets.as_slice() == [Target::Object(jace)]
        ));
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        assert!(named(&state, opponent, "Treasure Token").is_empty());
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(prompt, next(&mut restored));

        let mut forged = restored.clone();
        let Some(PendingEffectChoice::SelectTargets {
            path,
            purpose: EffectTargetSelectionPurpose::EternalV1 { choice },
            ..
        }) = forged
            .engine
            .pending_effect
            .as_mut()
            .and_then(|pending| pending.choice.as_mut())
        else {
            panic!("missing restored replacement choice");
        };
        path.push(0);
        choice.path = path.clone();
        assert!(mtg_kernel::effect::validate_pending_effect_choice(&forged).is_err());

        let mut treasure = state.clone();
        act(&mut treasure, Action::FinishEffectSelection);
        settled(&mut treasure);
        assert_eq!(named(&treasure, opponent, "Treasure Token").len(), 1);
        assert!(treasure
            .players
            .iter()
            .all(|player| !player.has_lost && !player.drew_from_empty));

        for branch in [&mut state, &mut restored] {
            act(branch, Action::ChooseEffectTarget(Target::Object(jace)));
            assert!(
                matches!(next(branch), Decision::GameOver { winner: Some(player) } if player == actor)
            );
            assert!(!branch.players[actor.index()].drew_from_empty);
            assert!(named(branch, opponent, "Treasure Token").is_empty());
        }
        assert_eq!(state.state_hash(), restored.state_hash());
    }
}

fn announce_flame(
    state: &mut GameState,
    caster: PlayerId,
    drawer: PlayerId,
    other_target: Option<ObjectId>,
    mode: u8,
) -> ObjectId {
    put(state, caster, "Snapcaster Mage", Zone::Battlefield);
    let flame = put(state, caster, "Flame of Anor", Zone::Hand);
    pool(state, caster, &[ManaColor::U, ManaColor::R], 1);
    assert!(castable(state, flame));
    act(state, Action::CastSpell(flame));
    assert!(
        matches!(next(state), Decision::ChooseSpellMode { legal_modes, .. }
        if legal_modes.contains(&mode))
    );
    act(state, Action::ChooseSpellMode(mode));
    choose_target(state, Target::Player(drawer));
    if let Some(other_target) = other_target {
        choose_target(state, Target::Object(other_target));
    }
    flame
}

fn paid_riddler(state: &mut GameState, drawer: PlayerId, hullbreacher: bool, copies: usize) {
    state.active_player = drawer;
    state.priority_player = drawer;
    if hullbreacher {
        act(state, Action::Pass);
        let opponent = drawer.opponent();
        let hull = put(state, opponent, "Hullbreacher", Zone::Hand);
        pool(state, opponent, &[ManaColor::U], 2);
        cast(state, hull, &[]);
        settled(state);
        assert_eq!(zone(state, hull), Zone::Battlefield);
    }
    for _ in 0..copies {
        let riddler = put(state, drawer, "Quantum Riddler", Zone::Hand);
        pool(state, drawer, &[ManaColor::U], 1);
        cast(state, riddler, &[]);
        settled(state);
        assert_eq!(zone(state, riddler), Zone::Battlefield);
    }
}

#[test]
fn paid_gush_applies_riddler_once_before_individual_draw_replacements_both_seats() {
    for drawer in [P0, P1] {
        for (hull, riddlers) in [(false, 1), (true, 1), (true, 2)] {
            let mut state = plains();
            paid_riddler(&mut state, drawer, hull, riddlers);
            if !hull {
                let plains = state.players[drawer.index()].hand[0];
                act(&mut state, Action::PlayLand(plains));
                settled(&mut state);
            }
            let hand_before = state.players[drawer.index()].hand.len();
            assert_eq!(hand_before, if hull { 0 } else { 1 });
            let treasures_before = named(&state, drawer.opponent(), "Treasure Token").len();
            let library_before = state.players[drawer.index()].library.len();
            let gush = put(&mut state, drawer, "Gush", Zone::Hand);
            pool(&mut state, drawer, &[ManaColor::U], 4);
            cast(&mut state, gush, &[]);
            settled(&mut state);
            assert_eq!(zone(&state, gush), Zone::Graveyard);
            assert_eq!(state.players[drawer.index()].mana_pool, [0; 6]);
            let attempts = 2 + riddlers;
            let drawn = if hull { 0 } else { attempts };
            assert_eq!(
                state.players[drawer.index()].hand.len(),
                hand_before + drawn
            );
            assert_eq!(
                state.players[drawer.index()].library.len(),
                library_before - drawn
            );
            assert_eq!(
                named(&state, drawer.opponent(), "Treasure Token").len(),
                treasures_before + attempts - drawn
            );
        }
    }
}

#[test]
fn paid_gush_with_riddler_resumes_the_original_fixed_count_both_seats() {
    for drawer in [P0, P1] {
        let opponent = drawer.opponent();
        let mut state = plains();
        paid_riddler(&mut state, drawer, true, 1);
        let treasures_before = named(&state, opponent, "Treasure Token").len();
        let thug = put(&mut state, drawer, "Golgari Thug", Zone::Graveyard);
        let gush = put(&mut state, drawer, "Gush", Zone::Hand);
        pool(&mut state, drawer, &[ManaColor::U], 4);
        cast(&mut state, gush, &[]);
        let first = settle(&mut state).expect("Dredge on the first of three attempts");
        assert!(matches!(&first, Decision::ChooseEffectTargets {
            player, source, legal_targets, can_finish: true, ..
        } if *player == drawer && *source == gush
            && legal_targets.as_slice() == [Target::Object(thug)]));
        assert_eq!(state.players[drawer.index()].mana_pool, [0; 6]);
        let first_snapshot = serde_json::to_value(&state).unwrap();

        act(&mut state, Action::FinishEffectSelection);
        let second = settle(&mut state).expect("Dredge on the second of three attempts");
        assert_eq!(
            named(&state, opponent, "Treasure Token").len(),
            treasures_before + 1
        );
        let second_snapshot = serde_json::to_value(&state).unwrap();
        let mut skipped = first_snapshot.clone();
        skipped["eternal_owned_draw_progress_v1"] =
            second_snapshot["eternal_owned_draw_progress_v1"].clone();
        skipped["engine"]["pending_effect"] = second_snapshot["engine"]["pending_effect"].clone();
        let skipped: GameState = serde_json::from_value(skipped).unwrap();
        assert_eq!(
            mtg_kernel::effect::validate_pending_effect_choice(&skipped).unwrap_err(),
            "Eternal draw changed its native completed attempts",
        );

        for corrupt_start in [false, true] {
            let mut removed = first_snapshot.clone();
            let progress = removed
                .as_object_mut()
                .unwrap()
                .remove("eternal_owned_draw_progress_v1")
                .unwrap();
            if corrupt_start {
                let started_at = progress["started_at"].as_u64().unwrap() as usize;
                let started = &mut removed["engine"]["event_history"][started_at]
                    ["EternalOwnedDrawV1"]["journal"]["StartedWithRiddlerV1"];
                assert_eq!(started["extra"], 1);
                started["extra"] = serde_json::json!(0);
            }
            let removed: GameState = serde_json::from_value(removed).unwrap();
            assert_eq!(
                mtg_kernel::effect::validate_pending_effect_choice(&removed).unwrap_err(),
                "Eternal draw already started its original instruction",
            );
        }

        let mut restored: GameState = serde_json::from_value(second_snapshot).unwrap();
        assert_eq!(next(&mut restored), second);
        act(&mut state, Action::FinishEffectSelection);
        assert!(settle(&mut state).is_some(), "Dredge on the third attempt");
        act(&mut state, Action::FinishEffectSelection);
        settled(&mut state);
        assert_eq!(
            named(&state, opponent, "Treasure Token").len(),
            treasures_before + 3
        );
        assert_eq!(restored.players[drawer.index()].library.len(), 20);

        act(
            &mut restored,
            Action::ChooseEffectTarget(Target::Object(thug)),
        );
        let mut answered: GameState =
            serde_json::from_slice(&serde_json::to_vec(&restored).unwrap()).unwrap();
        for branch in [&mut restored, &mut answered] {
            settled(branch);
            assert_eq!(zone(branch, gush), Zone::Graveyard);
            assert_eq!(zone(branch, thug), Zone::Hand);
            assert_eq!(branch.players[drawer.index()].library.len(), 16);
            assert_eq!(
                named(branch, opponent, "Treasure Token").len(),
                treasures_before + 2
            );
        }
        assert_eq!(restored.state_hash(), answered.state_hash());
    }
}

#[test]
fn paid_riddler_entry_keeps_its_second_attempt_after_dredge_both_seats() {
    for drawer in [P0, P1] {
        let mut state = plains();
        state.active_player = drawer;
        state.priority_player = drawer;
        let thug = put(&mut state, drawer, "Golgari Thug", Zone::Graveyard);
        let riddler = put(&mut state, drawer, "Quantum Riddler", Zone::Hand);
        pool(&mut state, drawer, &[ManaColor::U], 1);
        cast(&mut state, riddler, &[]);
        let prompt = settle(&mut state).expect("Riddler's first attempt can dredge");
        assert!(matches!(prompt, Decision::ChooseEffectTargets {
            player, source, legal_targets, can_finish: true, ..
        } if player == drawer && source == riddler && legal_targets == [Target::Object(thug)]));
        assert_eq!(zone(&state, riddler), Zone::Battlefield);
        act(&mut state, Action::ChooseEffectTarget(Target::Object(thug)));
        settled(&mut state);
        assert_eq!(state.players[drawer.index()].hand.len(), 2);
        assert_eq!(state.players[drawer.index()].library.len(), 15);
        assert_eq!(zone(&state, thug), Zone::Hand);
    }
}

#[test]
fn paid_flame_riddler_freezes_three_individual_attempts_across_restore_both_seats() {
    use mtg_kernel::event::CommittedEvent;

    for drawer in [P0, P1] {
        let caster = drawer.opponent();
        let mut state = plains();
        paid_riddler(&mut state, drawer, true, 1);
        let treasures_before = named(&state, caster, "Treasure Token").len();
        assert_eq!(
            treasures_before, 2,
            "Riddler's paid entry draws one plus one"
        );
        let dredger = put(&mut state, drawer, "Golgari Thug", Zone::Graveyard);
        let ring = put(&mut state, drawer, "Sol Ring", Zone::Battlefield);
        act(&mut state, Action::Pass);
        let flame = announce_flame(&mut state, caster, drawer, Some(ring), 3);
        let first = settle(&mut state).expect("dredge before Flame's first draw");
        assert!(matches!(&first, Decision::ChooseEffectTargets {
            player, source, legal_targets, can_finish: true, ..
        } if *player == drawer && *source == flame
            && legal_targets.as_slice() == [Target::Object(dredger)]));

        for downgrade in [false, true] {
            let mut forged = state.clone();
            let changed = forged.engine.event_history.iter_mut().any(|event| {
                if let CommittedEvent::EternalOwnedDrawV1 { journal } = event {
                    let mut row = serde_json::to_value(**journal).unwrap();
                    if let Some(started) = row.get_mut("StartedWithRiddlerV1") {
                        assert_eq!(started["extra"], 1);
                        if downgrade {
                            row = serde_json::json!({ "Started": started["started"].take() });
                        } else {
                            started["extra"] = serde_json::json!(2);
                        }
                        **journal = serde_json::from_value(row).unwrap();
                        return true;
                    }
                }
                false
            });
            assert!(changed);
            assert!(mtg_kernel::effect::validate_pending_effect_choice(&forged)
                .unwrap_err()
                .contains("original Riddler adjustment"));
        }

        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(next(&mut restored), first);
        act(
            &mut restored,
            Action::ChooseEffectTarget(Target::Object(dredger)),
        );
        let mut answered: GameState =
            serde_json::from_slice(&serde_json::to_vec(&restored).unwrap()).unwrap();
        for branch in [&mut restored, &mut answered] {
            settled(branch);
            assert_eq!(zone(branch, dredger), Zone::Hand);
            assert_eq!(branch.players[drawer.index()].library.len(), 16);
            assert_eq!(
                named(branch, caster, "Treasure Token").len(),
                treasures_before + 2
            );
            assert_eq!(zone(branch, ring), Zone::Graveyard);
        }
        assert_eq!(restored.state_hash(), answered.state_hash());

        for completed in 1..=3 {
            act(&mut state, Action::FinishEffectSelection);
            let prompt = settle(&mut state);
            assert_eq!(prompt.as_ref(), (completed < 3).then_some(&first));
            assert_eq!(
                named(&state, caster, "Treasure Token").len(),
                treasures_before + completed
            );
            assert_eq!(
                zone(&state, ring),
                if completed < 3 {
                    Zone::Battlefield
                } else {
                    Zone::Graveyard
                }
            );
        }
    }
}

#[test]
fn paid_flame_of_anor_draws_for_its_target_then_resolves_other_mode_both_seats() {
    use mtg_kernel::state::StackTargetContractV4;

    for caster in [P0, P1] {
        let drawer = caster.opponent();
        for (name, mode) in [
            (None, 0),
            (Some("Sol Ring"), 3),
            (Some("Llanowar Elves"), 4),
        ] {
            let mut state = plains();
            state.active_player = caster;
            state.priority_player = caster;
            let hullbreacher = put(&mut state, caster, "Hullbreacher", Zone::Hand);
            pool(&mut state, caster, &[ManaColor::U], 2);
            cast(&mut state, hullbreacher, &[]);
            settled(&mut state);
            let other = name.map(|name| put(&mut state, drawer, name, Zone::Battlefield));
            let other_zone = |state: &GameState| other.map(|object| zone(state, object));
            let dredger = put(&mut state, drawer, "Golgari Thug", Zone::Graveyard);
            let flame = announce_flame(&mut state, caster, drawer, other, mode);
            let first = settle(&mut state).expect("target player chooses the first replacement");
            assert!(matches!(&first, Decision::ChooseEffectTargets {
                player, source, legal_targets, can_finish: true, ..
            } if *player == drawer && *source == flame
                && legal_targets.as_slice() == [Target::Object(dredger)]));
            assert_eq!(state.players[caster.index()].mana_pool, [0; 6]);
            assert_eq!(other_zone(&state), other.map(|_| Zone::Battlefield));
            let mut forged = state.clone();
            let item = forged.stack.last_mut().unwrap();
            assert_eq!(item.source, flame);
            item.targets[0] = Target::Player(caster);
            item.v4.target_contracts[0] = StackTargetContractV4::Player(caster);
            let pending = forged.engine.pending_effect.as_mut().unwrap();
            pending.resolving_item = item.clone();
            pending.ctx.targets[0] = item.targets[0];
            pending.ctx.target_contracts[0] = item.v4.target_contracts[0];
            assert!(mtg_kernel::effect::validate_pending_effect_choice(&forged)
                .unwrap_err()
                .contains("lost its native start"));
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(next(&mut restored), first);
            act(
                &mut restored,
                Action::ChooseEffectTarget(Target::Object(dredger)),
            );
            restored = serde_json::from_slice(&serde_json::to_vec(&restored).unwrap()).unwrap();
            settled(&mut restored);
            assert_eq!(zone(&restored, dredger), Zone::Hand);
            assert_eq!(restored.players[drawer.index()].library.len(), 16);
            assert_eq!(named(&restored, caster, "Treasure Token").len(), 1);
            assert_eq!(other_zone(&restored), other.map(|_| Zone::Graveyard));

            act(&mut state, Action::FinishEffectSelection);
            assert_eq!(settle(&mut state), Some(first));
            assert_eq!(other_zone(&state), other.map(|_| Zone::Battlefield));
            assert_eq!(named(&state, caster, "Treasure Token").len(), 1);
            act(&mut state, Action::FinishEffectSelection);
            settled(&mut state);
            assert_eq!(zone(&state, dredger), Zone::Graveyard);
            assert_eq!(state.players[drawer.index()].library.len(), 20);
            assert_eq!(named(&state, caster, "Treasure Token").len(), 2);
            assert_eq!(other_zone(&state), other.map(|_| Zone::Graveyard));
        }
    }
}

#[test]
fn paid_flame_of_anor_stops_other_mode_when_its_draw_target_wins_both_seats() {
    for drawer in [P0, P1] {
        let caster = drawer.opponent();
        let mut state = game(&[]);
        state.active_player = drawer;
        state.priority_player = drawer;
        put(&mut state, caster, "Hullbreacher", Zone::Battlefield);
        let jace = put(&mut state, drawer, "Jace, Wielder of Mysteries", Zone::Hand);
        pool(&mut state, drawer, &[ManaColor::U; 3], 1);
        cast(&mut state, jace, &[]);
        settled(&mut state);
        act(&mut state, Action::Pass);
        let ring = put(&mut state, drawer, "Sol Ring", Zone::Battlefield);
        let flame = announce_flame(&mut state, caster, drawer, Some(ring), 3);
        let first = settle(&mut state).expect("Jace competes with Hullbreacher for Flame's draw");
        assert!(matches!(&first, Decision::ChooseEffectTargets {
            player, source, legal_targets, can_finish: true, ..
        } if *player == drawer && *source == flame
            && legal_targets.as_slice() == [Target::Object(jace)]));
        act(&mut state, Action::FinishEffectSelection);
        assert_eq!(settle(&mut state), Some(first));
        assert_eq!(named(&state, caster, "Treasure Token").len(), 1);
        assert_eq!(zone(&state, ring), Zone::Battlefield);
        act(&mut state, Action::ChooseEffectTarget(Target::Object(jace)));
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut restored] {
            assert_eq!(
                next(branch),
                Decision::GameOver {
                    winner: Some(drawer)
                }
            );
            assert_eq!(named(branch, caster, "Treasure Token").len(), 1);
            assert_eq!(zone(branch, ring), Zone::Battlefield);
        }
        assert_eq!(restored.state_hash(), state.state_hash());
    }
}

#[test]
fn paid_flame_of_anor_ignores_caster_dredge_and_departed_other_target_both_seats() {
    use mtg_kernel::event::CommittedEvent;

    for caster in [P0, P1] {
        let drawer = caster.opponent();
        let mut state = plains();
        state.active_player = caster;
        state.priority_player = caster;
        let ring = put(&mut state, drawer, "Sol Ring", Zone::Battlefield);
        let bounce = put(&mut state, drawer, "Echoing Truth", Zone::Hand);
        let unrelated = put(&mut state, caster, "Golgari Thug", Zone::Graveyard);
        announce_flame(&mut state, caster, drawer, Some(ring), 3);
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == caster)
        );
        act(&mut state, Action::Pass);
        pool(&mut state, drawer, &[ManaColor::U], 1);
        cast(&mut state, bounce, &[Target::Object(ring)]);
        settled(&mut state);
        assert_eq!(zone(&state, ring), Zone::Hand);
        assert_eq!(zone(&state, unrelated), Zone::Graveyard);
        assert_eq!(state.players[drawer.index()].library.len(), 18);
        assert_eq!(state.players[drawer.index()].hand.len(), 3);
        assert!(!state
            .engine
            .event_history
            .iter()
            .any(|event| matches!(event, CommittedEvent::EternalOwnedDrawV1 { .. })));
    }
}

#[test]
fn wurmcoil_engine_leaves_two_wurms() {
    let mut state = plains();
    let engine_card = put(&mut state, P1, "Wurmcoil Engine", Zone::Battlefield);
    let grapple = put(&mut state, P0, "Grapple with Death", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::G], 1);
    cast(&mut state, grapple, &[Target::Object(engine_card)]);
    settled(&mut state);
    assert_eq!(zone(&state, engine_card), Zone::Graveyard);
    let deathtouch = named(&state, P1, "Deathtouch Wurm Token");
    let lifelink = named(&state, P1, "Lifelink Wurm Token");
    assert_eq!((deathtouch.len(), lifelink.len()), (1, 1));
    assert_eq!(power_toughness(&state, deathtouch[0]), (3, 3));
    assert!(keyword(&state, deathtouch[0], Keywords::DEATHTOUCH));
    assert!(keyword(&state, lifelink[0], Keywords::LIFELINK));
}

#[test]
fn light_of_day_stops_black_creatures_from_attacking_and_blocking() {
    let mut state = plains();
    let black = put(&mut state, P0, "Cauldron Familiar", Zone::Battlefield);
    let white = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    put(&mut state, P1, "Light of Day", Zone::Battlefield);
    let eligible = eligible_attackers(&mut state);
    assert!(eligible.contains(&white));
    assert!(!eligible.contains(&black), "black creatures can't attack");

    let mut state = plains();
    let attacker = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let blocker = put(&mut state, P1, "Cauldron Familiar", Zone::Battlefield);
    assert!(legal_blockers(&mut state, attacker).contains(&blocker));
    let mut state = plains();
    let attacker = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let blocker = put(&mut state, P1, "Cauldron Familiar", Zone::Battlefield);
    put(&mut state, P1, "Light of Day", Zone::Battlefield);
    assert!(
        !legal_blockers(&mut state, attacker).contains(&blocker),
        "black creatures can't block"
    );
}

#[test]
fn dark_petition_adds_mana_only_with_spell_mastery() {
    for (spells, mana_black) in [(2, 3), (1, 0)] {
        let mut state = game(&["Plains", "Island", "Swamp", "Plains", "Plains"]);
        for _ in 0..spells {
            put(&mut state, P0, "Spell Snare", Zone::Graveyard);
        }
        let petition = put(&mut state, P0, "Dark Petition", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 3);
        cast(&mut state, petition, &[]);
        settle_first(&mut state);
        assert_eq!(
            mana(&state, P0, ManaColor::B),
            mana_black,
            "{spells} spells"
        );
    }
}

#[test]
fn rebuild_and_virtues_ruin_hit_what_the_text_says() {
    let mut state = plains();
    let white = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let black = put(&mut state, P1, "Cauldron Familiar", Zone::Battlefield);
    let ruin = put(&mut state, P0, "Virtue's Ruin", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, ruin, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, white), Zone::Graveyard);
    assert_eq!(zone(&state, black), Zone::Battlefield);
}

#[test]
fn feed_the_swarm_costs_life_equal_to_the_mana_value() {
    let mut state = plains();
    let breacher = put(&mut state, P1, "Hullbreacher", Zone::Battlefield);
    let own = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let feed = put(&mut state, P0, "Feed the Swarm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    let targets = cast_targets(&state, feed);
    assert!(targets.contains(&Target::Object(breacher)));
    assert!(
        !targets.contains(&Target::Object(own)),
        "an opponent controls"
    );
    cast(&mut state, feed, &[Target::Object(breacher)]);
    settled(&mut state);
    assert_eq!(zone(&state, breacher), Zone::Graveyard);
    assert_eq!(state.players[0].life, 17);
}

#[test]
fn lithomantic_barrage_deals_five_to_white_and_blue_targets() {
    // Hullbreacher (blue, 3/2) dies to five damage; Dauthi Voidwalker (black,
    // 3/2) survives the single point.
    let mut state = plains();
    let blue = put(&mut state, P1, "Hullbreacher", Zone::Battlefield);
    let barrage = put(&mut state, P0, "Lithomantic Barrage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, barrage, &[Target::Object(blue)]);
    settled(&mut state);
    assert_eq!(zone(&state, blue), Zone::Graveyard);

    let mut state = plains();
    let black = put(&mut state, P1, "Dauthi Voidwalker", Zone::Battlefield);
    let barrage = put(&mut state, P0, "Lithomantic Barrage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, barrage, &[Target::Object(black)]);
    settled(&mut state);
    assert_eq!(zone(&state, black), Zone::Battlefield);
    assert_eq!(state.objects.get(black).damage, 1);
    assert!(def("Lithomantic Barrage").spell_cannot_be_countered);
}

#[test]
fn sphinx_of_the_steel_wind_has_protection_from_red_and_green() {
    let mut state = plains();
    let sphinx = put(
        &mut state,
        P1,
        "Sphinx of the Steel Wind",
        Zone::Battlefield,
    );
    let barrage = put(&mut state, P0, "Lithomantic Barrage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(
        !castable(&mut state, barrage),
        "the Sphinx is the only target and is protected from red"
    );
    assert_eq!(power_toughness(&state, sphinx), (6, 6));
    assert!(keyword(&state, sphinx, Keywords::FLYING));
    assert!(keyword(&state, sphinx, Keywords::LIFELINK));
}

#[test]
fn urborg_makes_every_land_a_swamp() {
    let mut state = plains();
    let urborg = put(
        &mut state,
        P0,
        "Urborg, Tomb of Yawgmoth",
        Zone::Battlefield,
    );
    let plains_land = put(&mut state, P1, "Plains", Zone::Battlefield);
    assert!(engine::has_effective_subtype(
        &state,
        plains_land,
        Subtype::Swamp
    ));
    assert!(engine::has_effective_subtype(
        &state,
        urborg,
        Subtype::Swamp
    ));
    let mut state = plains();
    let plains_land = put(&mut state, P1, "Plains", Zone::Battlefield);
    assert!(!engine::has_effective_subtype(
        &state,
        plains_land,
        Subtype::Swamp
    ));
}

#[test]
fn argentum_masticore_sacrifices_unless_its_controller_discards() {
    // With an empty hand it is sacrificed at the upkeep.
    let mut state = plains();
    let masticore = put(&mut state, P0, "Argentum Masticore", Zone::Battlefield);
    state.players[0].hand.clear();
    pass_until(&mut state, |state| {
        state.turn >= 3 && state.active_player == P0 && state.step == Step::Main1
    });
    assert_eq!(zone(&state, masticore), Zone::Graveyard);
}
#[test]
fn ravenous_trap_is_free_after_three_cards_hit_the_opponents_graveyard() {
    let mut state = plains();
    let trap = put(&mut state, P0, "Ravenous Trap", Zone::Hand);
    assert!(
        !castable(&mut state, trap),
        "no mana and no graveyard cards"
    );
    for _ in 0..3 {
        let card = put(&mut state, P1, "Plains", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(card, Zone::Graveyard),
        );
    }
    let mut state = state;
    assert_eq!(state.players[1].graveyard.len(), 3);
    assert!(castable(&mut state, trap), "three cards this turn");
    act(&mut state, Action::CastSpell(trap));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { options, .. } => {
            assert!(options.contains(&CastMode::Alternative));
            Action::ChooseCastMode(CastMode::Alternative)
        }
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert!(state.players[1].graveyard.is_empty(), "exiled");
    assert_eq!(state.exile.len(), 3);
}
