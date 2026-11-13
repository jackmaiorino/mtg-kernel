//! Paid Portent scheduling and original-spell delayed draw ownership.
#![cfg(feature = "eternal-fixtures")]
use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;
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

fn cast_jace(state: &mut GameState, player: PlayerId) -> ObjectId {
    state.active_player = player;
    state.priority_player = player;
    let jace = put(state, player, "Jace, Wielder of Mysteries", Zone::Hand);
    pool(
        state,
        player,
        &[ManaColor::U, ManaColor::U, ManaColor::U],
        1,
    );
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&jace))
    );
    act(state, Action::CastSpell(jace));
    settled(state);
    jace
}

fn restored(state: &GameState) -> GameState {
    let restored: GameState = serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
    assert_eq!(state.state_hash(), restored.state_hash());
    mtg_kernel::effect::validate_pending_effect_choice(&restored).unwrap();
    restored
}

fn cast(
    state: &mut GameState,
    actor: PlayerId,
    name: &str,
    target: Option<PlayerId>,
    colored: &[ManaColor],
    colorless: u8,
) -> ObjectId {
    let card = put(state, actor, name, Zone::Hand);
    pool(state, actor, colored, colorless);
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { player, castable_spells, .. } if player == actor && castable_spells.contains(&card))
    );
    act(state, Action::CastSpell(card));
    for _ in 0..12 {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                let target = Target::Player(target.expect("printed target"));
                assert!(legal_targets.contains(&target));
                act(state, Action::ChooseTarget(target));
            }
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::CastSpellOrPass { .. } => {
                assert!(state.engine.pending_cast.is_none());
                assert!(state.stack.iter().any(|row| row.source == card
                    && !row.is_copy
                    && row
                        .v4
                        .source_contract
                        .is_some_and(|source| source.spell_cast_origin.is_some())));
                assert_eq!(zone(state, card), Zone::Stack);
                return card;
            }
            other => panic!("cast {name}: {other:?}"),
        }
    }
    panic!("paid cast did not finish");
}
fn portent(state: &mut GameState, actor: PlayerId) -> ObjectId {
    cast(
        state,
        actor,
        "Portent",
        Some(actor.opponent()),
        &[ManaColor::U],
        0,
    )
}
fn finish_parent(state: &mut GameState, shuffle: bool) {
    for _ in 0..24 {
        match settle(state) {
            None => return,
            Some(Decision::ChooseEffectTargets {
                can_finish: true, ..
            }) => act(state, Action::FinishEffectSelection),
            Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
                act(state, Action::ChooseEffectTarget(legal_targets[0]))
            }
            Some(Decision::ChooseEffectOption {
                option_count: 2, ..
            }) => act(state, Action::ChooseEffectOption(u16::from(!shuffle))),
            other => panic!("Portent parent: {other:?}"),
        }
    }
    panic!("Portent parent did not finish");
}
fn next_upkeep(state: &mut GameState, caster: PlayerId) {
    for _ in 0..160 {
        let decision = next(state);
        if state.active_player == caster.opponent() && state.step == Step::Upkeep {
            assert!(!state.stack.is_empty() || !state.engine.pending_triggers.is_empty());
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::DeclareAttackers { .. } => act(state, Action::DeclareAttackers(vec![])),
            Decision::DeclareBlockers { .. } => act(state, Action::DeclareBlockers(vec![])),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => panic!("next upkeep: {other:?}"),
        }
    }
    panic!("next upkeep absent");
}
fn entries(value: &serde_json::Value, key: &str) -> usize {
    match value {
        serde_json::Value::Object(map) => {
            usize::from(map.contains_key(key))
                + map.values().map(|v| entries(v, key)).sum::<usize>()
        }
        serde_json::Value::Array(rows) => rows.iter().map(|v| entries(v, key)).sum(),
        _ => 0,
    }
}
fn receipt_count(state: &GameState, key: &str) -> usize {
    entries(
        &serde_json::to_value(&state.engine.event_history).unwrap(),
        key,
    )
}
fn alter(
    value: &mut serde_json::Value,
    variant: &str,
    field: &str,
    replacement: serde_json::Value,
) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(row) = map.get_mut(variant) {
                row[field] = replacement;
                return true;
            }
            map.values_mut()
                .any(|v| alter(v, variant, field, replacement.clone()))
        }
        serde_json::Value::Array(rows) => rows
            .iter_mut()
            .any(|v| alter(v, variant, field, replacement.clone())),
        _ => false,
    }
}
#[test]
fn paid_portent_both_seats_shuffle_branches_restore_and_original_caster_next_upkeep() {
    for actor in [P0, P1] {
        for shuffle in [false, true] {
            let mut state = plains();
            state.active_player = actor;
            state.priority_player = actor;
            let original_library = state.players[actor.opponent().index()].library.clone();
            let source = portent(&mut state, actor);
            let paid_source = state
                .stack
                .iter()
                .find(|row| row.source == source)
                .unwrap()
                .v4
                .source_contract
                .unwrap();
            assert!(
                matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { player, .. }) if player == actor)
            );
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                finish_parent(branch, shuffle);
                assert_eq!(zone(branch, source), Zone::Graveyard);
                assert!(branch.players.iter().all(|seat| seat.hand.is_empty()));
                assert_eq!(receipt_count(branch, "Scheduled"), 1);
                assert_eq!(receipt_count(branch, "Queued"), 0);
                if !shuffle {
                    assert_eq!(
                        branch.players[actor.opponent().index()].library,
                        original_library
                    );
                }
                next_upkeep(branch, actor);
                let row = branch
                    .stack
                    .iter()
                    .find(|row| row.source == source)
                    .unwrap();
                assert_eq!(row.controller, actor);
                assert_eq!(row.v4.source_contract, Some(paid_source));
                assert!(row.v4.ability_source_contract.is_none());
                assert!(mtg_kernel::rl::observe_v2(
                    branch,
                    &mtg_kernel::surface_v2::HarnessSurfaceV2::default(),
                    actor,
                    0
                )
                .is_ok());
                mtg_kernel::rl::observe_policy_v5(
                    branch,
                    &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
                    actor,
                    0,
                    0,
                    0,
                    1,
                )
                .unwrap();
                settled(branch);
                assert!(mtg_kernel::rl::observe_v2(
                    branch,
                    &mtg_kernel::surface_v2::HarnessSurfaceV2::default(),
                    actor,
                    0
                )
                .is_ok());
                assert_eq!(branch.players[actor.index()].hand.len(), 1);
                assert!(branch.players[actor.opponent().index()].hand.is_empty());
                assert_eq!(branch.step, Step::Upkeep);
                assert_eq!(receipt_count(branch, "Started"), 1);
                assert_eq!(receipt_count(branch, "Completed"), 1);
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }
}
/// Misdirection: "Change the target of target spell with a single target."
/// A paid Portent it retargets still resolves: it looks at and orders the
/// new target's library, may shuffle it, and schedules its caster's draw.
/// A restored retarget binding still resumes; a forged one fails closed.
#[test]
fn paid_portent_retargeted_by_misdirection_looks_at_the_new_target() {
    for actor in [P0, P1] {
        for shuffle in [false, true] {
            let mut state = plains();
            state.active_player = actor;
            state.priority_player = actor;
            let mut own_top: Vec<ObjectId> = state.players[actor.index()]
                .library
                .iter()
                .copied()
                .take(3)
                .collect();
            own_top.sort();
            let opponent_library = state.players[actor.opponent().index()].library.clone();
            let source = portent(&mut state, actor);
            let misdirection = put(&mut state, actor, "Misdirection", Zone::Hand);
            let pitched = put(&mut state, actor, "Brainstorm", Zone::Hand);
            assert!(
                matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&misdirection))
            );
            act(&mut state, Action::CastSpell(misdirection));
            for _ in 0..8 {
                match next(&mut state) {
                    Decision::ChooseTargets { legal_targets, .. } => {
                        assert!(legal_targets.contains(&Target::Object(source)));
                        act(&mut state, Action::ChooseTarget(Target::Object(source)));
                    }
                    Decision::ChooseCostTargets { candidates, .. } => {
                        assert!(candidates.contains(&pitched));
                        act(&mut state, Action::ChooseCostTarget(pitched));
                    }
                    Decision::CastSpellOrPass { .. } => break,
                    other => panic!("Misdirection cast: {other:?}"),
                }
            }
            assert_eq!(zone(&state, pitched), Zone::Exile);
            match settle(&mut state) {
                Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
                    assert_eq!(legal_targets, vec![Target::Player(actor)]);
                    act(
                        &mut state,
                        Action::ChooseEffectTarget(Target::Player(actor)),
                    );
                }
                other => panic!("Misdirection's new target: {other:?}"),
            }
            // The answer applies as Misdirection finishes resolving; the
            // retargeted Portent then waits on the stack.
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            assert_eq!(zone(&state, misdirection), Zone::Graveyard);
            assert_eq!(receipt_count(&state, "Retargeted"), 1);
            let row = state.stack.iter().find(|row| row.source == source).unwrap();
            assert_eq!(row.targets, vec![Target::Player(actor)]);
            let mut replay = restored(&state);
            let mut json = serde_json::to_value(&state).unwrap();
            assert!(alter(
                &mut json["engine"]["event_history"],
                "Retargeted",
                "paid_at",
                999999.into()
            ));
            let mut forged: GameState = serde_json::from_value(json).unwrap();
            let mut halted = false;
            for _ in 0..8 {
                match engine::advance_until_decision(&mut forged) {
                    Decision::Halted { .. } => {
                        halted = true;
                        break;
                    }
                    Decision::CastSpellOrPass { .. } => {
                        let _ = engine::step(&mut forged, Action::Pass);
                    }
                    other => panic!("forged retarget binding resolved: {other:?}"),
                }
            }
            assert!(halted, "a forged retarget binding fails closed");
            for branch in [&mut state, &mut replay] {
                match settle(branch) {
                    Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
                        let mut looked: Vec<ObjectId> = legal_targets
                            .iter()
                            .map(|target| match *target {
                                Target::Object(card) => card,
                                other => panic!("Portent looked at {other:?}"),
                            })
                            .collect();
                        looked.sort();
                        assert_eq!(looked, own_top);
                    }
                    other => panic!("Portent's look: {other:?}"),
                }
                finish_parent(branch, shuffle);
                assert_eq!(zone(branch, source), Zone::Graveyard);
                assert_eq!(receipt_count(branch, "Scheduled"), 1);
                assert_eq!(
                    branch.players[actor.opponent().index()].library,
                    opponent_library
                );
                next_upkeep(branch, actor);
                settled(branch);
                assert_eq!(branch.players[actor.index()].hand.len(), 1);
                assert!(branch.players[actor.opponent().index()].hand.is_empty());
                assert_eq!(receipt_count(branch, "Completed"), 1);
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }
}
#[test]
fn paid_portent_jace_hull_without_dredge_departed_source_and_terminal_stop() {
    for actor in [P0, P1] {
        for win in [false, true] {
            let mut state = game(&[]);
            for _ in 0..4 {
                put(&mut state, actor.opponent(), "Plains", Zone::Library);
            }
            let jace = cast_jace(&mut state, actor);
            put(
                &mut state,
                actor.opponent(),
                "Hullbreacher",
                Zone::Battlefield,
            );
            put(&mut state, actor, "Quantum Riddler", Zone::Battlefield);
            let source = portent(&mut state, actor);
            finish_parent(&mut state, false);
            assert_eq!(zone(&state, source), Zone::Graveyard);
            // Native zone-event control after the actual spell resolved. This
            // verifies later source movement, not a claimed shipped exile spell.
            mtg_kernel::event::propose_and_commit(
                &mut state,
                mtg_kernel::event::ProposedEvent::zone_change(source, Zone::Exile),
            );
            next_upkeep(&mut state, actor);
            let prompt = settle(&mut state).expect("Jace/Hull competing replacements");
            assert!(
                matches!(&prompt, Decision::ChooseEffectTargets { player, legal_targets, .. } if *player == actor && legal_targets == &[Target::Object(jace)])
            );
            let mut replay = restored(&state);
            assert_eq!(next(&mut replay), prompt);
            for branch in [&mut state, &mut replay] {
                if win {
                    act(branch, Action::ChooseEffectTarget(Target::Object(jace)));
                    assert!(
                        matches!(next(branch), Decision::GameOver { winner: Some(player) } if player == actor)
                    );
                    assert!(named(branch, actor.opponent(), "Treasure Token").is_empty());
                    assert_eq!(
                        receipt_count(branch, "Completed"),
                        1,
                        "terminal first attempt stops the Riddler tail"
                    );
                } else {
                    act(branch, Action::FinishEffectSelection);
                    assert!(matches!(
                        settle(branch),
                        Some(Decision::ChooseEffectTargets { .. })
                    ));
                    act(branch, Action::FinishEffectSelection);
                    settled(branch);
                    assert_eq!(named(branch, actor.opponent(), "Treasure Token").len(), 2);
                    assert!(branch
                        .players
                        .iter()
                        .all(|seat| !seat.has_lost && !seat.drew_from_empty));
                    assert_eq!(receipt_count(branch, "Completed"), 2);
                }
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }
}
#[test]
fn paid_portent_dredge_hull_and_riddler_expand_once_with_restored_progress() {
    for actor in [P0, P1] {
        let mut state = plains();
        state.active_player = actor;
        state.priority_player = actor;
        put(
            &mut state,
            actor.opponent(),
            "Hullbreacher",
            Zone::Battlefield,
        );
        put(&mut state, actor, "Quantum Riddler", Zone::Battlefield);
        let thug = put(&mut state, actor, "Golgari Thug", Zone::Graveyard);
        portent(&mut state, actor);
        finish_parent(&mut state, true);
        next_upkeep(&mut state, actor);
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { player, legal_targets, .. }) if player == actor && legal_targets == [Target::Object(thug)])
        );
        let mut replay = restored(&state);
        for branch in [&mut state, &mut replay] {
            act(branch, Action::ChooseEffectTarget(Target::Object(thug)));
            settled(branch);
            assert_eq!(zone(branch, thug), Zone::Hand);
            assert_eq!(branch.players[actor.index()].library.len(), 16);
            assert_eq!(named(branch, actor.opponent(), "Treasure Token").len(), 1);
            assert_eq!(receipt_count(branch, "StartedWithRiddlerV1"), 1);
            assert_eq!(receipt_count(branch, "Completed"), 2);
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}
#[test]
fn paid_portent_restore_rejects_rebound_source_count_progress_and_scheduling_proof() {
    for actor in [P0, P1] {
        let mut state = game(&[]);
        for _ in 0..3 {
            put(&mut state, actor.opponent(), "Plains", Zone::Library);
        }
        cast_jace(&mut state, actor);
        put(
            &mut state,
            actor.opponent(),
            "Hullbreacher",
            Zone::Battlefield,
        );
        portent(&mut state, actor);
        finish_parent(&mut state, false);
        next_upkeep(&mut state, actor);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectTargets { .. })
        ));
        restored(&state);
        for mutation in 0..5 {
            let mut forged = state.clone();
            let pending = forged.engine.pending_effect.as_mut().unwrap();
            match mutation {
                0 => pending.frames.clear(),
                1 => pending.ctx.controller = actor.opponent(),
                2 => {
                    pending
                        .resolving_item
                        .v4
                        .source_contract
                        .as_mut()
                        .unwrap()
                        .zone_change_count += 1
                }
                3 => {
                    pending
                        .resolving_item
                        .v4
                        .source_contract
                        .as_mut()
                        .unwrap()
                        .controller = actor.opponent()
                }
                _ => {
                    pending.resolving_item.inline_effect =
                        Some(mtg_kernel::effect::EffectOp::DrawCards {
                            player: mtg_kernel::effect::PlayerRef::Controller,
                            count: 1,
                        })
                }
            }
            assert!(
                mtg_kernel::effect::validate_pending_effect_choice(&forged).is_err(),
                "mutation {mutation}"
            );
        }
        for (variant, field) in [
            ("Started", "total"),
            ("LookCompleted", "look_at"),
            ("ShuffleCompleted", "look_at"),
            ("Scheduled", "paid_at"),
            ("Queued", "scheduled_at"),
            ("Published", "queued_at"),
        ] {
            let mut json = serde_json::to_value(&state).unwrap();
            assert!(alter(
                &mut json["engine"]["event_history"],
                variant,
                field,
                999999.into()
            ));
            let forged: GameState = serde_json::from_value(json).unwrap();
            assert!(
                mtg_kernel::effect::validate_pending_effect_choice(&forged).is_err(),
                "{variant}.{field}"
            );
        }
        let mut json = serde_json::to_value(&state).unwrap();
        json["eternal_owned_draw_progress_v1"]["completed"] = 1.into();
        let forged: GameState = serde_json::from_value(json).unwrap();
        assert!(mtg_kernel::effect::validate_pending_effect_choice(&forged).is_err());
    }
}
#[test]
fn native_copied_portent_preserves_legacy_schedule_and_ordinary_draw() {
    for actor in [P0, P1] {
        for shuffle in [false, true] {
            let mut state = plains();
            state.active_player = actor;
            state.priority_player = actor;
            // This registered Standard-prefix printing is in the Eternal
            // catalog. Portent is its first instant/sorcery cast this turn.
            cast(
                &mut state,
                actor,
                "Chandra, Hope's Beacon",
                None,
                &[ManaColor::R, ManaColor::R],
                4,
            );
            settled(&mut state);
            portent(&mut state, actor);
            finish_parent(&mut state, shuffle);
            assert_eq!(
                receipt_count(&state, "Scheduled"),
                1,
                "only the physical original acquires the new owner"
            );
            assert_eq!(
                state.eternal_v1.as_ref().unwrap().p1a.portent_draws.len(),
                1,
                "the genuine native copy retains the predecessor record"
            );
            assert!(state.players[actor.index()].hand.is_empty());
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                next_upkeep(branch, actor);
                settled(branch);
                assert_eq!(branch.players[actor.index()].hand.len(), 2);
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }
}

#[test]
fn paid_portent_parent_cannot_skip_look_shuffle_or_replay_completed_cast() {
    use mtg_kernel::effect::{self, EffectFrame, EffectOp};
    use mtg_kernel::eternal_cards_v1::EternalOpV1;
    use mtg_kernel::eternal_premodern_p1a_v1::P1aOpV1;
    use mtg_kernel::standard_cards_v1::StandardOpV1;
    let leaf = |op| EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P1aV1(op)));
    for actor in [P0, P1] {
        let mut state = plains();
        state.active_player = actor;
        state.priority_player = actor;
        portent(&mut state, actor);
        let mut borrowed_live = state.clone();
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectTargets { .. })
        ));
        let original = state.engine.pending_effect.clone().unwrap();
        assert!(effect::begin_resumable_resolution(
            &mtg_kernel::eternal_premodern_p1a_v1::portent(),
            &original.ctx,
            original.resolving_item.clone(),
            &mut borrowed_live,
        )
        .is_err());
        assert_eq!(receipt_count(&borrowed_live, "Look"), 0);

        for mutation in 0..3 {
            let mut forged = state.clone();
            let pending = forged.engine.pending_effect.as_mut().unwrap();
            match mutation {
                0 => pending.frames.clear(),
                1 => pending.ctx.targets = vec![Target::Player(actor)],
                _ => {
                    pending.frames = vec![EffectFrame::Program {
                        op: leaf(P1aOpV1::PortentDrawV1),
                        path: vec![2],
                    }]
                }
            }
            assert!(effect::validate_pending_effect_choice(&forged).is_err());
        }
        let mut skipped = state.clone();
        let pending = skipped.engine.pending_effect.as_mut().unwrap();
        pending.choice = None;
        pending.frames = vec![EffectFrame::Program {
            op: leaf(P1aOpV1::PortentDrawV1),
            path: vec![2],
        }];
        assert!(effect::resume_resumable_resolution(&mut skipped).is_err());
        assert_eq!(receipt_count(&skipped, "Scheduled"), 0);
        let mut skipped = state.clone();
        skipped.engine.pending_effect = None;
        let borrowed_tail = EffectOp::Sequence(vec![
            leaf(P1aOpV1::NoEffectV1),
            leaf(P1aOpV1::NoEffectV1),
            leaf(P1aOpV1::PortentDrawV1),
        ]);
        assert!(effect::begin_resumable_resolution(
            &borrowed_tail,
            &original.ctx,
            original.resolving_item.clone(),
            &mut skipped
        )
        .is_err());
        assert_eq!(receipt_count(&skipped, "Scheduled"), 0);
        for raw in [
            leaf(P1aOpV1::PortentDrawV1),
            leaf(P1aOpV1::PortentUpkeepBoundV1 { queued_at: 0 }),
        ] {
            let mut forged = state.clone();
            forged.engine.pending_effect = None;
            effect::execute(&raw, &original.ctx, &mut forged);
            assert!(forged.engine.halted.is_some());
            assert_eq!(receipt_count(&forged, "Scheduled"), 0);
        }
        finish_parent(&mut state, false);
        assert_eq!(receipt_count(&state, "Scheduled"), 1);
        let mut replay = state.clone();
        assert!(effect::begin_resumable_resolution(
            &mtg_kernel::eternal_premodern_p1a_v1::portent(),
            &original.ctx,
            original.resolving_item.clone(),
            &mut replay
        )
        .is_err());
        assert_eq!(receipt_count(&replay, "Scheduled"), 1);
    }
}
#[test]
fn paid_portent_empty_or_single_card_look_schedules_after_both_shuffle_branches() {
    for actor in [P0, P1] {
        for count in 0..2 {
            for shuffle in [false, true] {
                let mut state = game(&[]);
                state.active_player = actor;
                state.priority_player = actor;
                for _ in 0..4 {
                    put(&mut state, actor, "Plains", Zone::Library);
                }
                for _ in 0..count {
                    put(&mut state, actor.opponent(), "Plains", Zone::Library);
                }
                portent(&mut state, actor);
                assert!(matches!(
                    settle(&mut state),
                    Some(Decision::ChooseEffectOption {
                        option_count: 2,
                        ..
                    })
                ));
                let mut replay = restored(&state);
                for branch in [&mut state, &mut replay] {
                    finish_parent(branch, shuffle);
                    assert!(branch.players[actor.index()].hand.is_empty());
                    next_upkeep(branch, actor);
                    settled(branch);
                    assert_eq!(branch.players[actor.index()].hand.len(), 1);
                }
                assert_eq!(state.state_hash(), replay.state_hash());
            }
        }
    }
}

#[test]
fn paid_portent_pending_order_sources_remain_original_after_hidden_return() {
    use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
    use mtg_kernel::rl::{legal_action_candidates_v5, observe_policy_v5, ActionSemanticV1};
    for actor in [P0, P1] {
        let mut state = plains();
        state.active_player = actor;
        state.priority_player = actor;
        let mut sources = Vec::new();
        for _ in 0..2 {
            let source = portent(&mut state, actor);
            let contract = state
                .stack
                .iter()
                .find(|row| row.source == source)
                .unwrap()
                .v4
                .source_contract
                .unwrap();
            finish_parent(&mut state, false);
            // Native zone-event control after a real paid and completed spell.
            // This is not a claim that Portent itself returns to its owner's hand.
            mtg_kernel::event::propose_and_commit(
                &mut state,
                mtg_kernel::event::ProposedEvent::zone_change(source, Zone::Hand),
            );
            sources.push(contract);
        }
        next_upkeep(&mut state, actor);
        assert_eq!(state.engine.pending_triggers.len(), 2);
        let mut replay = restored(&state);
        for branch in [&mut state, &mut replay] {
            let mut surface = PolicySurfaceV5::new();
            let decision = surface.next_decision(branch).unwrap();
            let observation = observe_policy_v5(branch, &surface, actor, 0, 0, 0, 1).unwrap();
            let pending = &observation
                .projection
                .surface
                .engine_context
                .pending_triggers;
            assert_eq!(pending.len(), 2);
            for (pending, original) in pending.iter().zip(&sources) {
                let source = pending
                    .source
                    .as_ref()
                    .expect("authenticated original source");
                assert_eq!(source.arena_id, original.source.0);
                assert_eq!(source.zone, Zone::Stack);
                assert_eq!(source.zone_change_count, original.zone_change_count);
                assert_eq!(source.controller, actor.into());
            }
            let choices = legal_action_candidates_v5(&decision, branch).unwrap();
            assert_eq!(choices.len(), 2);
            for choice in &choices {
                let ActionSemanticV1::OrderTriggers {
                    pending_sources, ..
                } = &choice.record.semantic
                else {
                    panic!("unexpected action");
                };
                assert_eq!(
                    pending_sources,
                    &pending
                        .iter()
                        .map(|row| row.source.clone().expect("authenticated original source"))
                        .collect::<Vec<_>>()
                );
            }
            let mut tampered = branch.clone();
            tampered.engine.pending_triggers[0].controller = actor.opponent();
            assert!(observe_policy_v5(&tampered, &surface, actor, 0, 0, 0, 1).is_err());
            assert!(legal_action_candidates_v5(&decision, &tampered).is_err());
            surface
                .apply(branch, choices[0].policy_action.clone())
                .unwrap();
            settled(branch);
            assert_eq!(branch.players[actor.index()].hand.len(), 4);
            assert_eq!(receipt_count(branch, "Completed"), 2);
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}
