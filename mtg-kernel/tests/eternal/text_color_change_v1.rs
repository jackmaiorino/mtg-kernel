//! Step C1 of Issue #460: Layer-3 color-word text replacement infrastructure,
//! audited 305-card Premodern table, all 38 color-word sites across 16 modules,
//! Schema 5 observation field, `choose_text_color_word_v1` action semantics,
//! consumer refusals, and byte-identity conditions (`docs/design/
//! premodern_shapeshifter_alter_reality_v1.md`, §§4.1–4.6, 5.2).
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, mana_color_mask, CardDef, CARD_DEFS};
use mtg_kernel::effect::{
    EffectContinuation, EffectOp, EffectOptionChoicePurpose, ExecCtx, PendingEffectChoice,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice, UnsupportedMechanic};
use mtg_kernel::eternal_sideboard_s1d_v1::CircleColorV1;
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::infoset_v1::{export_v2, ExportErrorV1, ExportInputV1, RefusalCodeV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::policy_surface_v5::{PolicyDecisionV5, PolicySurfaceV5};
use mtg_kernel::rl::{
    enable_public_shroud_v1, observe_policy_v5, observe_premodern_ordered_v1, observe_v1,
    policy_legal_action_candidates_v5, ActionSemanticV1, PlayerSeatV1, TargetRefV1,
};
use mtg_kernel::state::{
    FaceDownV1, GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone,
};
use mtg_kernel::surface::SurfaceDecision;
use mtg_kernel::surface_v2::HarnessSurfaceV2;
use mtg_kernel::text_color_change_v1::{
    self, TextColorWordStageV1, AUDITED_COLOR_WORD_CARDS_V1, AUDITED_POOL_TOKENS_V1,
    COLOR_WORD_CARDS_V1, COST_ONLY_COLOR_WORD_CARDS_V1, NO_COLOR_WORD_CARDS_V1,
};
use std::collections::BTreeSet;
use std::path::PathBuf;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("missing card definition: {name}"))
}

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn islands() -> GameState {
    let ids = vec![id("Island"); 20];
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |card_id| CARD_DEFS[card_id as usize].object_name.into(),
        0x4331_0000_0001,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    state
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
        _ => panic!("unsupported helper zone: {zone:?}"),
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

fn life(state: &GameState, player: PlayerId) -> i32 {
    state.players[player.index()].life
}

fn controls(state: &GameState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    let want = id(name);
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| state.objects.get(object).card_def == want)
        .collect()
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(
        !matches!(decision, Decision::Halted { .. }),
        "unexpected halt: {decision:?}\nstack: {:#?}\npending triggers: {:#?}",
        state.stack,
        state.engine.pending_triggers
    );
    decision
}

fn act(state: &mut GameState, action: Action) {
    engine::step(state, action.clone()).unwrap_or_else(|error| panic!("{action:?}: {error}"));
}

fn settle(state: &mut GameState) -> Option<Decision> {
    for _ in 0..200 {
        match next(state) {
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return None;
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => return Some(other),
        }
    }
    panic!("did not settle");
}

fn settled(state: &mut GameState) {
    if let Some(other) = settle(state) {
        panic!("unexpected choice while settling: {other:?}");
    }
}

fn settle_with(state: &mut GameState, mut answer: impl FnMut(&GameState, Decision) -> Action) {
    while let Some(decision) = settle(state) {
        let action = answer(state, decision);
        act(state, action);
    }
}

fn first_answer(decision: Decision) -> Action {
    match decision {
        Decision::ChooseTargets {
            legal_targets,
            can_finish: true,
            ..
        } if legal_targets.is_empty() => Action::FinishEffectSelection,
        Decision::ChooseTargets { legal_targets, .. } => Action::ChooseTarget(legal_targets[0]),
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            Action::ChooseEffectTarget(legal_targets[0])
        }
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(0),
        Decision::ChooseEffectBoolean { .. } => Action::ChooseEffectBoolean(false),
        Decision::ChooseKicker { .. } => Action::ChooseKicker(false),
        Decision::ChooseSpellMode { legal_modes, .. } => Action::ChooseSpellMode(legal_modes[0]),
        Decision::ChooseOptionalCost { .. } => {
            Action::ChooseOptionalCost(OptionalCostChoice::Decline)
        }
        Decision::Discard { count, choices, .. } => {
            Action::Discard(choices.into_iter().take(count as usize).collect())
        }
        other => panic!("unexpected choice: {other:?}"),
    }
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

fn activatable(state: &mut GameState, source: ObjectId, ability: u8) -> bool {
    priority(state).1.contains(&(source, ability))
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
        other => panic!("expected targets, got {other:?}"),
    }
}

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

fn cast_mode(state: &mut GameState, spell: ObjectId, mode: u8, targets: &[Target]) {
    assert!(
        castable(state, spell),
        "{} not castable",
        state.objects.get(spell).name
    );
    act(state, Action::CastSpell(spell));
    match next(state) {
        Decision::ChooseSpellMode { legal_modes, .. } => {
            assert!(legal_modes.contains(&mode), "{mode} not in {legal_modes:?}");
            act(state, Action::ChooseSpellMode(mode));
        }
        other => panic!("expected spell mode, got {other:?}"),
    }
    for target in targets {
        choose_target(state, *target);
    }
}

fn activate(state: &mut GameState, source: ObjectId, ability: u8, targets: &[Target]) {
    assert!(
        activatable(state, source, ability),
        "{} ability {ability} not activatable",
        state.objects.get(source).name
    );
    act(state, Action::ActivateAbility(source, ability));
    for target in targets {
        choose_target(state, *target);
    }
}

fn ability_targets(state: &GameState, source: ObjectId, ability: u8) -> Vec<Target> {
    let mut probe = state.clone();
    assert!(activatable(&mut probe, source, ability));
    act(&mut probe, Action::ActivateAbility(source, ability));
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. } => legal_targets,
        other => panic!("expected targets, got {other:?}"),
    }
}

fn hit(state: &mut GameState, source: ObjectId, target: Target, amount: i32) {
    event::propose_and_commit(state, ProposedEvent::damage(source, target, amount));
}

fn bury(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
}

fn announce_x(state: &mut GameState, x: u16) {
    match next(state) {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert!(x < option_count, "X {x} of {option_count}");
            act(state, Action::ChooseEffectOption(x));
        }
        other => panic!("expected X announcement, got {other:?}"),
    }
}

fn pass_until(state: &mut GameState, stop: impl Fn(&GameState) -> bool) {
    for _ in 0..2000 {
        let decision = next(state);
        if stop(state) {
            return;
        }
        let action = match decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::DeclareAttackers { .. } => Action::DeclareAttackers(Vec::new()),
            Decision::DeclareBlockers { .. } => Action::DeclareBlockers(Vec::new()),
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            other => first_answer(other),
        };
        act(state, action);
    }
    panic!("condition never held");
}

fn to_main_phase_of(state: &mut GameState, player: PlayerId) {
    let now = (state.turn, state.active_player);
    pass_until(state, |state| {
        (state.turn, state.active_player) != now
            && state.active_player == player
            && state.step == Step::Main1
    });
}

fn to_next_upkeep_trigger(state: &mut GameState) {
    let now = (state.turn, state.active_player);
    pass_until(state, |state| {
        (state.turn, state.active_player) != now
            && state.step == Step::Upkeep
            && !state.stack.is_empty()
    });
}

fn eligible_attackers(state: &mut GameState) -> Vec<ObjectId> {
    pass_until(state, |state| state.step == Step::DeclareAttackers);
    match next(state) {
        Decision::DeclareAttackers { eligible, .. } => eligible,
        other => panic!("expected attackers, got {other:?}"),
    }
}

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
    panic!("blockers never offered");
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn change(state: &mut GameState, target: ObjectId, from: ManaColor, to: ManaColor) {
    assert!(
        text_color_change_v1::apply_change(state, target, target, from, to),
        "expected audited target {}",
        state.objects.get(target).name
    );
}

// ---- Audit table completeness & refusal --------------------------------------------

#[test]
fn premodern_snapshot_audit_table_is_complete_and_outside_cards_halt() {
    let snapshot_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../data/eternal_v1/meta_snapshot_mtgtop8_premodern_20261101.json");
    let raw = std::fs::read_to_string(&snapshot_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", snapshot_path.display()));
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let mut snapshot_cards = BTreeSet::new();
    for deck in json["formats"]["premodern"].as_array().unwrap() {
        for section in ["mainboard", "sideboard"] {
            for entry in deck[section].as_array().unwrap() {
                let name = entry[1].as_str().unwrap().to_owned();
                snapshot_cards.insert(name);
            }
        }
    }
    assert_eq!(
        snapshot_cards.len(),
        305,
        "Premodern snapshot card count changed"
    );

    let mut audited = BTreeSet::new();
    for name in AUDITED_COLOR_WORD_CARDS_V1
        .into_iter()
        .chain(NO_COLOR_WORD_CARDS_V1)
    {
        assert!(
            audited.insert(name.to_owned()),
            "duplicate entry in audit tables: {name}"
        );
    }
    assert_eq!(audited, snapshot_cards);
    assert_eq!(COLOR_WORD_CARDS_V1.len(), 36);
    assert_eq!(COST_ONLY_COLOR_WORD_CARDS_V1.len(), 5);
    assert_eq!(NO_COLOR_WORD_CARDS_V1.len(), 264);

    // All 9 pool tokens are registered token definitions and audited.
    for token in AUDITED_POOL_TOKENS_V1 {
        assert!(def(token).is_token, "{token} should be a token");
        assert!(text_color_change_v1::card_name_audited(token));
    }

    // Outside-audit card (Circle of Protection: Blue) halts even for from == to.
    for (from, to) in [(ManaColor::U, ManaColor::G), (ManaColor::U, ManaColor::U)] {
        let mut state = islands();
        let source = put(&mut state, P0, "Island", Zone::Battlefield);
        let cop_blue = put(
            &mut state,
            P1,
            "Circle of Protection: Blue",
            Zone::Battlefield,
        );
        assert!(!text_color_change_v1::apply_change(
            &mut state, source, cop_blue, from, to
        ));
        assert_eq!(
            engine::advance_until_decision(&mut state),
            Decision::Halted {
                mechanic: UnsupportedMechanic::TextColorChangeOutsideAuditV1,
                source,
            }
        );
        assert!(state.text_color_changes_v1.is_none());
    }
}

// ---- Core rules: timestamps, zone change, phasing, control change, face-down -------

#[test]
fn core_rules_timestamps_zone_change_phasing_control_change_and_face_down() {
    // 1. Two changes in timestamp order (613.7): B -> G then G -> R maps both B and G to R.
    let mut state = islands();
    let enforcer = put(&mut state, P0, "Mystic Enforcer", Zone::Battlefield);
    change(&mut state, enforcer, ManaColor::B, ManaColor::G);
    change(&mut state, enforcer, ManaColor::G, ManaColor::R);
    assert_eq!(
        text_color_change_v1::text_color(&state, enforcer, ManaColor::B),
        ManaColor::R
    );
    assert_eq!(
        text_color_change_v1::text_color(&state, enforcer, ManaColor::G),
        ManaColor::R
    );

    // Reverse order: G -> R then B -> G maps B to G and G to R.
    let mut state2 = islands();
    let enforcer2 = put(&mut state2, P0, "Mystic Enforcer", Zone::Battlefield);
    change(&mut state2, enforcer2, ManaColor::G, ManaColor::R);
    change(&mut state2, enforcer2, ManaColor::B, ManaColor::G);
    assert_eq!(
        text_color_change_v1::text_color(&state2, enforcer2, ManaColor::B),
        ManaColor::G
    );
    assert_eq!(
        text_color_change_v1::text_color(&state2, enforcer2, ManaColor::G),
        ManaColor::R
    );

    // 2. Control change preserves the record (same incarnation on battlefield).
    state.objects.get_mut(enforcer).controller = P1;
    state.players[0].battlefield.retain(|&id| id != enforcer);
    state.players[1].battlefield.push(enforcer);
    assert_eq!(
        text_color_change_v1::text_color(&state, enforcer, ManaColor::B),
        ManaColor::R
    );

    // 3. Zone change removes the record and returns field to None.
    bury(&mut state, enforcer);
    assert!(state.text_color_changes_v1.is_none());
    assert_eq!(
        text_color_change_v1::text_color(&state, enforcer, ManaColor::B),
        ManaColor::B
    );

    // 4. Phasing (Vision Charm mode 2) keeps the record across phase-out / phase-in.
    let mut state = islands();
    let processor = put(&mut state, P0, "Phyrexian Processor", Zone::Hand);
    pool(&mut state, P0, &[], 4);
    cast(&mut state, processor, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption { .. }) => act(&mut state, Action::ChooseEffectOption(3)),
        other => panic!("expected Processor life choice, got {other:?}"),
    }
    settled(&mut state);
    change(&mut state, processor, ManaColor::B, ManaColor::G);

    let charm = put(&mut state, P0, "Vision Charm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast_mode(&mut state, charm, 2, &[Target::Object(processor)]);
    settled(&mut state);
    assert_eq!(zone(&state, processor), Zone::Command);
    assert!(state.text_color_changes_v1.is_some());

    // Advance to P0's next main phase: Processor phases back in keeping B -> G.
    to_main_phase_of(&mut state, P0);
    assert_eq!(zone(&state, processor), Zone::Battlefield);
    assert_eq!(
        text_color_change_v1::text_color(&state, processor, ManaColor::B),
        ManaColor::G
    );
    pool(&mut state, P0, &[], 4);
    activate(&mut state, processor, 0, &[]);
    settled(&mut state);
    let minions = controls(&state, P0, "Phyrexian Minion Token");
    assert_eq!(minions.len(), 1);
    assert_eq!(
        engine::object_color_mask(&state, minions[0]),
        mana_color_mask(ManaColor::G)
    );

    // 5. Face-down permanent accepted without reading identity; turning face up runs audit.
    let mut state = islands();
    let angel = put(&mut state, P0, "Exalted Angel", Zone::Battlefield);
    state.objects.get_mut(angel).v4.face_down_v1 = Some(FaceDownV1 {
        disguised: true,
        lookers: 1,
        hidden_by: None,
        hidden_by_source: None,
    });
    assert!(text_color_change_v1::apply_change(
        &mut state,
        angel,
        angel,
        ManaColor::W,
        ManaColor::U
    ));
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    act(&mut state, Action::TurnFaceUp(angel));
    assert!(state.engine.halted.is_none());
    assert_eq!(
        text_color_change_v1::text_color(&state, angel, ManaColor::W),
        ManaColor::U
    );

    // Face-down unaudited card (Circle of Protection: Blue) accepted while face down,
    // halts with TextColorChangeOutsideAuditV1 when turned face up.
    let mut state = islands();
    let cop_blue = put(
        &mut state,
        P0,
        "Circle of Protection: Blue",
        Zone::Battlefield,
    );
    state.objects.get_mut(cop_blue).v4.face_down_v1 = Some(FaceDownV1 {
        disguised: true,
        lookers: 1,
        hidden_by: None,
        hidden_by_source: None,
    });
    assert!(text_color_change_v1::apply_change(
        &mut state,
        cop_blue,
        cop_blue,
        ManaColor::U,
        ManaColor::G
    ));
    assert!(state.engine.halted.is_none());
    // Clear face_down_v1 to simulate turning face up: audit runs and halts.
    state.objects.get_mut(cop_blue).v4.face_down_v1 = None;
    assert!(!text_color_change_v1::on_turned_face_up(
        &mut state, cop_blue
    ));
    assert_eq!(
        state.engine.halted,
        Some((UnsupportedMechanic::TextColorChangeOutsideAuditV1, cop_blue))
    );
}

// ---- Storage, captures, snapshot restore, and §4.6 byte-identity conditions --------

#[test]
fn storage_captures_snapshot_restore_and_section_4_6_conditions() {
    // §4.6 #1: Schema 5 observation of a game without Alter Reality has no `text_color_changes` key.
    let mut state = islands();
    enable_public_shroud_v1(&mut state);
    put(&mut state, P0, "Goblin Piledriver", Zone::Battlefield);
    let obs = observe_policy_v5(&state, &PolicySurfaceV5::new(), P0, 0, 0, 0, 1).unwrap();
    assert!(obs.projection.text_color_changes.is_none());
    let obs_json = serde_json::to_value(&obs).unwrap();
    assert!(
        obs_json["projection"].get("text_color_changes").is_none(),
        "empty text_color_changes must be omitted from Schema 5 JSON"
    );

    // §4.6 #2: A state with a stack item, a pending trigger, and no text_color_changes_v1
    // omits `"text_color_changes_v1"` in JSON, round-trips identically, and matches baseline hashes.
    let mut base = islands();
    let dryad = put(&mut base, P0, "Quirion Dryad", Zone::Battlefield);
    let bolt = put(&mut base, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut base, P0, &[ManaColor::R], 0);
    cast(&mut base, bolt, &[Target::Player(P1)]);
    // Put a second spell on stack so both a stack spell and a triggered ability exist cleanly.
    let _ = dryad;
    assert!(!base.stack.is_empty());
    assert!(base.text_color_changes_v1.is_none());
    let raw_json = serde_json::to_string(&base).unwrap();
    assert!(
        !raw_json.contains("text_color_changes_v1"),
        "None text_color_changes_v1 must not serialize"
    );
    let roundtrip = restored(&base);
    assert_eq!(roundtrip.state_hash(), base.state_hash());
    assert_eq!(
        roundtrip.diagnostic_state_hash(),
        base.diagnostic_state_hash()
    );

    // §4.6 #3: A state whose only change was removed returns to None and hashes identically
    // to a state that never had a change (with matching next_effect_timestamp).
    let mut clean = islands();
    let clean_target = put(&mut clean, P0, "Goblin Piledriver", Zone::Battlefield);
    let mut dirty = clean.clone();
    change(&mut dirty, clean_target, ManaColor::U, ManaColor::G);
    assert!(dirty.text_color_changes_v1.is_some());
    bury(&mut clean, clean_target);
    bury(&mut dirty, clean_target);
    assert!(dirty.text_color_changes_v1.is_none());
    clean.next_effect_timestamp = dirty.next_effect_timestamp;
    assert_eq!(dirty.state_hash(), clean.state_hash());
    assert_eq!(dirty.diagnostic_state_hash(), clean.diagnostic_state_hash());
    assert_eq!(
        serde_json::to_vec(&dirty).unwrap(),
        serde_json::to_vec(&clean).unwrap()
    );

    // Captures: unchanged source writes NO capture; changed source writes capture,
    // keeps it across snapshot/restore and later source changes (113.7a), and drops on counter/resolve.
    let mut state = islands();
    let acolyte = put(&mut state, P0, "Crimson Acolyte", Zone::Battlefield);
    let bear = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W], 0);
    activate(&mut state, acolyte, 0, &[Target::Object(bear)]);
    assert!(
        state.text_color_changes_v1.is_none(),
        "unchanged source must write no capture"
    );
    let first_item = state.stack.last().unwrap().v4.stack_item_id;
    // Changing the source AFTER an unchanged ability went on stack does not alter that ability.
    change(&mut state, acolyte, ManaColor::R, ManaColor::G);
    assert!(!text_color_change_v1::has_capture(&state, first_item));
    settled(&mut state);
    // Unchanged ability granted protection from red (not green)!
    let fanatic = put(&mut state, P1, "Mogg Fanatic", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    assert!(engine::damage_is_prevented_by_protection(
        &state,
        fanatic,
        Target::Object(bear)
    ));
    assert!(!engine::damage_is_prevented_by_protection(
        &state,
        elves,
        Target::Object(bear)
    ));

    // Now activate the changed Acolyte: capture written, survives snapshot/restore, removed when countered.
    pool(&mut state, P0, &[ManaColor::W], 0);
    activate(&mut state, acolyte, 0, &[Target::Object(bear)]);
    let captured_item = state.stack.last().unwrap().v4.stack_item_id;
    assert!(text_color_change_v1::has_capture(&state, captured_item));
    let restored_state = restored(&state);
    assert!(text_color_change_v1::has_capture(
        &restored_state,
        captured_item
    ));
    assert_eq!(restored_state.state_hash(), state.state_hash());

    // Fizzle: bury target before resolution; capture must be removed on fizzle.
    bury(&mut state, bear);
    settled(&mut state);
    assert!(!text_color_change_v1::has_capture(&state, captured_item));
}

// ---- Sites 1–8, 10 (Protection masks) & Iridescent Angel ---------------------------

#[test]
fn all_protection_mask_sites_and_iridescent_angel_never_mapped() {
    let mut state = islands();
    // Color sources for probing protection from each of W, U, B, R, G.
    let src_w = put(&mut state, P1, "Savannah Lions", Zone::Battlefield);
    let src_u = put(&mut state, P1, "Volrath's Shapeshifter", Zone::Battlefield);
    let src_b = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let src_r = put(&mut state, P1, "Mogg Fanatic", Zone::Battlefield);
    let src_g = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let src_of = |c: ManaColor| match c {
        ManaColor::W => src_w,
        ManaColor::U => src_u,
        ManaColor::B => src_b,
        ManaColor::R => src_r,
        ManaColor::G => src_g,
        ManaColor::C => unreachable!(),
    };
    let protected_from = |state: &GameState, host: ObjectId, c: ManaColor| {
        engine::damage_is_prevented_by_protection(state, src_of(c), Target::Object(host))
    };

    // Sites 1–5, 7, 8, 10: single-color protection creatures.
    let single_cases = [
        ("Goblin Piledriver", ManaColor::U, ManaColor::W),
        ("Knight of Stromgald", ManaColor::W, ManaColor::G),
        ("Mystic Enforcer", ManaColor::B, ManaColor::R),
        ("Soltari Monk", ManaColor::B, ManaColor::U),
        ("Soltari Priest", ManaColor::R, ManaColor::G),
        ("Karmic Guide", ManaColor::B, ManaColor::U),
        ("Crimson Acolyte", ManaColor::R, ManaColor::B),
        ("Ihsan's Shade", ManaColor::W, ManaColor::R),
    ];
    for (name, printed, new_color) in single_cases {
        let host = put(&mut state, P0, name, Zone::Battlefield);
        assert!(
            protected_from(&state, host, printed),
            "{name} printed protection"
        );
        assert!(
            !protected_from(&state, host, new_color),
            "{name} before change"
        );
        change(&mut state, host, printed, new_color);
        assert!(
            !protected_from(&state, host, printed),
            "{name} old protection removed"
        );
        assert!(
            protected_from(&state, host, new_color),
            "{name} new protection applies"
        );
    }

    // Site 6: Akroma, Angel of Wrath (protection from black and from red).
    let akroma = put(&mut state, P0, "Akroma, Angel of Wrath", Zone::Battlefield);
    assert!(protected_from(&state, akroma, ManaColor::B));
    assert!(protected_from(&state, akroma, ManaColor::R));
    change(&mut state, akroma, ManaColor::B, ManaColor::G);
    assert!(!protected_from(&state, akroma, ManaColor::B));
    assert!(protected_from(&state, akroma, ManaColor::G));
    assert!(protected_from(&state, akroma, ManaColor::R));
    change(&mut state, akroma, ManaColor::R, ManaColor::W);
    assert!(!protected_from(&state, akroma, ManaColor::R));
    assert!(protected_from(&state, akroma, ManaColor::W));
    assert!(protected_from(&state, akroma, ManaColor::G));

    // Full-text branches on Volrath's Shapeshifter: Akroma, Karmic Guide, Ihsan's Shade,
    // and Iridescent Angel (never mapped!).
    let shapeshifter = put(&mut state, P0, "Volrath's Shapeshifter", Zone::Battlefield);
    change(&mut state, shapeshifter, ManaColor::B, ManaColor::G);
    put(&mut state, P0, "Akroma, Angel of Wrath", Zone::Graveyard);
    assert!(!protected_from(&state, shapeshifter, ManaColor::B));
    assert!(protected_from(&state, shapeshifter, ManaColor::G));
    assert!(protected_from(&state, shapeshifter, ManaColor::R));

    put(&mut state, P0, "Karmic Guide", Zone::Graveyard);
    assert!(!protected_from(&state, shapeshifter, ManaColor::B));
    assert!(protected_from(&state, shapeshifter, ManaColor::G));

    put(&mut state, P0, "Ihsan's Shade", Zone::Graveyard);
    assert!(protected_from(&state, shapeshifter, ManaColor::W));
    assert!(!protected_from(&state, shapeshifter, ManaColor::G));
    change(&mut state, shapeshifter, ManaColor::W, ManaColor::U);
    assert!(!protected_from(&state, shapeshifter, ManaColor::W));
    assert!(protected_from(&state, shapeshifter, ManaColor::U));

    // Iridescent Angel: "protection from all colors" is NEVER mapped, both physical & Shapeshifter.
    let iri = put(&mut state, P0, "Iridescent Angel", Zone::Battlefield);
    change(&mut state, iri, ManaColor::B, ManaColor::G);
    put(&mut state, P0, "Iridescent Angel", Zone::Graveyard);
    for color in [
        ManaColor::W,
        ManaColor::U,
        ManaColor::B,
        ManaColor::R,
        ManaColor::G,
    ] {
        assert!(
            protected_from(&state, iri, color),
            "physical Iridescent Angel keeps protection from {color:?}"
        );
        assert!(
            protected_from(&state, shapeshifter, color),
            "Shapeshifter with Iridescent Angel keeps protection from {color:?}"
        );
    }
}

// ---- Site 9: Crimson Acolyte {W} grant ---------------------------------------------

#[test]
fn crimson_acolyte_grant_captures_changed_color_and_persists_until_end_of_turn() {
    let mut state = islands();
    let acolyte = put(&mut state, P0, "Crimson Acolyte", Zone::Battlefield);
    let target = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    let green_src = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let blue_src = put(&mut state, P1, "Volrath's Shapeshifter", Zone::Battlefield);
    let red_src = put(&mut state, P1, "Mogg Fanatic", Zone::Battlefield);

    change(&mut state, acolyte, ManaColor::R, ManaColor::G);
    pool(&mut state, P0, &[ManaColor::W], 0);
    activate(&mut state, acolyte, 0, &[Target::Object(target)]);
    // Change Acolyte again while ability is on stack: captured green governs (CR 113.7a).
    change(&mut state, acolyte, ManaColor::G, ManaColor::U);
    settled(&mut state);

    assert!(engine::damage_is_prevented_by_protection(
        &state,
        green_src,
        Target::Object(target)
    ));
    assert!(!engine::damage_is_prevented_by_protection(
        &state,
        blue_src,
        Target::Object(target)
    ));
    assert!(!engine::damage_is_prevented_by_protection(
        &state,
        red_src,
        Target::Object(target)
    ));
}

// ---- Sites 11 & 12: Absolute Law & Light of Day (per-instance) ---------------------

#[test]
fn absolute_law_and_light_of_day_evaluate_per_instance() {
    // Site 11: Absolute Law (single changed + two Laws with different colors).
    let mut state = islands();
    let law1 = put(&mut state, P0, "Absolute Law", Zone::Battlefield);
    let creature = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    let red_src = put(&mut state, P1, "Mogg Fanatic", Zone::Battlefield);
    let blue_src = put(&mut state, P1, "Volrath's Shapeshifter", Zone::Battlefield);
    change(&mut state, law1, ManaColor::R, ManaColor::U);
    assert!(engine::damage_is_prevented_by_protection(
        &state,
        blue_src,
        Target::Object(creature)
    ));
    assert!(!engine::damage_is_prevented_by_protection(
        &state,
        red_src,
        Target::Object(creature)
    ));
    // Add a second unchanged Absolute Law (red): creature now has protection from BOTH red and blue.
    put(&mut state, P1, "Absolute Law", Zone::Battlefield);
    assert!(engine::damage_is_prevented_by_protection(
        &state,
        blue_src,
        Target::Object(creature)
    ));
    assert!(engine::damage_is_prevented_by_protection(
        &state,
        red_src,
        Target::Object(creature)
    ));

    // Site 12: Light of Day (two instances with different colors ban both attacking & blocking).
    let mut state = islands();
    let black = put(&mut state, P0, "Hypnotic Specter", Zone::Battlefield);
    let green = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let white = put(&mut state, P0, "Savannah Lions", Zone::Battlefield);
    put(&mut state, P1, "Light of Day", Zone::Battlefield);
    let lod2 = put(&mut state, P1, "Light of Day", Zone::Battlefield);
    change(&mut state, lod2, ManaColor::B, ManaColor::G);
    let attackers = eligible_attackers(&mut state);
    assert!(attackers.contains(&white));
    assert!(
        !attackers.contains(&black),
        "unchanged Light of Day bans black"
    );
    assert!(
        !attackers.contains(&green),
        "changed Light of Day bans green"
    );

    // Blocking check: single changed Light of Day (B -> G) lets black block and bans green.
    let mut state = islands();
    let attacker = put(&mut state, P0, "Savannah Lions", Zone::Battlefield);
    let black_blocker = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let green_blocker = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let lod = put(&mut state, P1, "Light of Day", Zone::Battlefield);
    change(&mut state, lod, ManaColor::B, ManaColor::G);
    let blockers = legal_blockers(&mut state, attacker);
    assert!(blockers.contains(&black_blocker));
    assert!(!blockers.contains(&green_blocker));
}

// ---- Sites 13–17: Sphere of Law, Chill, Gloom (x2), Warmth -------------------------

#[test]
fn sphere_of_law_chill_gloom_and_warmth_evaluate_per_instance() {
    // Site 13: Sphere of Law — two Spheres (one red, one changed red -> blue).
    let mut state = islands();
    put(&mut state, P1, "Sphere of Law", Zone::Battlefield);
    let sphere2 = put(&mut state, P1, "Sphere of Law", Zone::Battlefield);
    change(&mut state, sphere2, ManaColor::R, ManaColor::U);
    let red_src = put(&mut state, P0, "Mogg Fanatic", Zone::Battlefield);
    let blue_src = put(&mut state, P0, "Volrath's Shapeshifter", Zone::Battlefield);
    let green_src = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);

    hit(&mut state, red_src, Target::Player(P1), 3);
    assert_eq!(life(&state, P1), 19, "1 Sphere matches red -> prevent 2");
    hit(&mut state, blue_src, Target::Player(P1), 3);
    assert_eq!(life(&state, P1), 18, "1 Sphere matches blue -> prevent 2");
    hit(&mut state, green_src, Target::Player(P1), 3);
    assert_eq!(life(&state, P1), 15, "0 Spheres match green -> prevent 0");

    // Site 14: Chill — two Chills (one red, one changed red -> blue).
    let mut state = islands();
    put(&mut state, P1, "Chill", Zone::Battlefield);
    let chill2 = put(&mut state, P1, "Chill", Zone::Battlefield);
    change(&mut state, chill2, ManaColor::R, ManaColor::U);
    let red_spell = put(&mut state, P0, "Mogg Fanatic", Zone::Hand); // {R} + {2} = {2}{R}
    let blue_spell = put(&mut state, P0, "Vision Charm", Zone::Hand); // {U} + {2} = {2}{U}
    let green_spell = put(&mut state, P0, "Llanowar Elves", Zone::Hand); // {G} + {0}
    pool(
        &mut state,
        P0,
        &[ManaColor::R, ManaColor::U, ManaColor::G],
        0,
    );
    assert!(castable(&mut state, green_spell));
    assert!(!castable(&mut state, red_spell), "needs +2 from red Chill");
    assert!(
        !castable(&mut state, blue_spell),
        "needs +2 from blue Chill"
    );
    pool(&mut state, P0, &[], 2);
    assert!(castable(&mut state, red_spell));
    assert!(castable(&mut state, blue_spell));

    // Site 15: Gloom spell tax — two Glooms (one white, one changed white -> black).
    let mut state = islands();
    put(&mut state, P1, "Gloom", Zone::Battlefield);
    let gloom2 = put(&mut state, P1, "Gloom", Zone::Battlefield);
    change(&mut state, gloom2, ManaColor::W, ManaColor::B);
    let white_spell = put(&mut state, P0, "Savannah Lions", Zone::Hand); // {W} + {3}
    let black_spell = put(&mut state, P0, "Dark Ritual", Zone::Hand); // {B} + {3}
    let green_spell = put(&mut state, P0, "Llanowar Elves", Zone::Hand); // {G} + {0}
    pool(
        &mut state,
        P0,
        &[ManaColor::W, ManaColor::B, ManaColor::G],
        2,
    );
    assert!(castable(&mut state, green_spell));
    assert!(!castable(&mut state, white_spell));
    assert!(!castable(&mut state, black_spell));
    pool(&mut state, P0, &[], 1);
    assert!(castable(&mut state, white_spell));
    assert!(castable(&mut state, black_spell));

    // Site 16: Gloom activation tax on enchantments — changed white -> red.
    let mut state = islands();
    let gloom = put(&mut state, P1, "Gloom", Zone::Battlefield);
    change(&mut state, gloom, ManaColor::W, ManaColor::R);
    let cleansing = put(&mut state, P0, "Seal of Cleansing", Zone::Battlefield); // white enchantment, {0} sac
    let seal_fire = put(&mut state, P0, "Seal of Fire", Zone::Battlefield); // red enchantment, {0} sac
    assert!(
        activatable(&mut state, cleansing, 0),
        "white enchantment is no longer taxed by red Gloom"
    );
    assert!(
        !activatable(&mut state, seal_fire, 0),
        "red enchantment now costs +3"
    );
    pool(&mut state, P0, &[], 3);
    assert!(activatable(&mut state, seal_fire, 0));

    // Site 17: Warmth — two Warmths (one red, one changed red -> green).
    let mut state = islands();
    put(&mut state, P1, "Warmth", Zone::Battlefield);
    let warmth2 = put(&mut state, P1, "Warmth", Zone::Battlefield);
    change(&mut state, warmth2, ManaColor::R, ManaColor::G);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    let fanatic = put(&mut state, P0, "Mogg Fanatic", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::R], 0);
    cast(&mut state, elves, &[]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 22, "only the green Warmth triggered (+2)");
    cast(&mut state, fanatic, &[]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 24, "only the red Warmth triggered (+2)");
}

// ---- Site 18: Dystopia -------------------------------------------------------------

#[test]
fn dystopia_upkeep_captures_green_and_white_words() {
    let mut state = islands();
    let dystopia = put(&mut state, P0, "Dystopia", Zone::Battlefield);
    // P1 controls a red creature, a white creature, a green creature, and a blue creature.
    let red_c = put(&mut state, P1, "Mogg Fanatic", Zone::Battlefield);
    let white_c = put(&mut state, P1, "Savannah Lions", Zone::Battlefield);
    let green_c = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let blue_c = put(&mut state, P1, "Volrath's Shapeshifter", Zone::Battlefield);

    // Change green -> red before P1's upkeep. Trigger captures (green->red, white->white).
    change(&mut state, dystopia, ManaColor::G, ManaColor::R);
    to_next_upkeep_trigger(&mut state);
    assert_eq!(state.active_player, P1);
    // Change white -> blue while trigger is already on the stack: capture ignores it (CR 113.7a).
    change(&mut state, dystopia, ManaColor::W, ManaColor::U);

    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets {
            player,
            legal_targets,
            ..
        }) => {
            assert_eq!(player, P1);
            assert!(legal_targets.contains(&Target::Object(red_c)));
            assert!(legal_targets.contains(&Target::Object(white_c)));
            assert!(
                !legal_targets.contains(&Target::Object(green_c)),
                "green was replaced with red"
            );
            assert!(
                !legal_targets.contains(&Target::Object(blue_c)),
                "late white -> blue change must not reach captured upkeep trigger"
            );
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(red_c)),
            );
        }
        other => panic!("expected Dystopia sacrifice choice, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(zone(&state, red_c), Zone::Graveyard);
}

// ---- Site 19: Circle of Protection: Red --------------------------------------------

#[test]
fn circle_of_protection_red_changed_to_green_prevents_live_and_departed_damage() {
    let mut state = islands();
    let cop = put(
        &mut state,
        P0,
        "Circle of Protection: Red",
        Zone::Battlefield,
    );
    let red_src = put(&mut state, P1, "Mogg Fanatic", Zone::Battlefield);
    let green_src = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);

    change(&mut state, cop, ManaColor::R, ManaColor::G);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, cop, 0, &[]);
    // Changing CoP again while activation is on stack leaves captured green intact.
    change(&mut state, cop, ManaColor::G, ManaColor::W);
    // Resolve CoP choice: only green_src (not red_src) is offered.
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption { option_count, .. }) => {
            assert_eq!(option_count, 1, "only the green source should be offered");
            act(&mut state, Action::ChooseEffectOption(0));
        }
        other => panic!("expected Circle source choice, got {other:?}"),
    }
    settled(&mut state);
    let shield = state.eternal_v1.as_ref().unwrap().s1d.cop_shields[0];
    assert_eq!(shield.color, CircleColorV1::Green);
    assert_eq!(shield.source.object, green_src);

    // Green source damage is prevented; red source damage is not.
    hit(&mut state, green_src, Target::Player(P0), 2);
    assert_eq!(life(&state, P0), 20, "green shield prevented green damage");
    hit(&mut state, red_src, Target::Player(P0), 2);
    assert_eq!(life(&state, P0), 18, "red damage not prevented");

    // Departed green source is recorded by note_departure when CoP: Red is changed to green.
    let mut state = islands();
    let cop = put(
        &mut state,
        P0,
        "Circle of Protection: Red",
        Zone::Battlefield,
    );
    let green_src = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    change(&mut state, cop, ManaColor::R, ManaColor::G);
    bury(&mut state, green_src);
    assert!(
        state.eternal_v1.as_ref().is_some_and(|e| e
            .s1d
            .departed_sources
            .iter()
            .any(|d| d.source.object == green_src)),
        "departed green source must be recorded while CoP: Red is changed to green"
    );
}

// ---- Site 20: Quirion Dryad --------------------------------------------------------

#[test]
fn quirion_dryad_trigger_follows_changed_color_words() {
    let mut state = islands();
    let dryad = put(&mut state, P0, "Quirion Dryad", Zone::Battlefield);
    change(&mut state, dryad, ManaColor::W, ManaColor::G);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    let lions = put(&mut state, P0, "Savannah Lions", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::W], 0);

    cast(&mut state, elves, &[]);
    settled(&mut state);
    assert_eq!(
        state.objects.get(dryad).counters.plus1_plus1,
        1,
        "green spell now triggers Quirion Dryad"
    );

    cast(&mut state, lions, &[]);
    settled(&mut state);
    assert_eq!(
        state.objects.get(dryad).counters.plus1_plus1,
        1,
        "white spell no longer triggers Quirion Dryad"
    );
}

// ---- Sites 21–25 & 27: Blasts, Pyroblast, Hydroblast, Snuff Out, Anarchy -----------

#[test]
fn elemental_blasts_pyroblast_hydroblast_snuff_out_and_anarchy_on_stack() {
    // Site 21: Blue Elemental Blast — permanent mode & spell mode fizzle when changed on stack.
    let mut state = islands();
    let fanatic = put(&mut state, P1, "Mogg Fanatic", Zone::Battlefield);
    let beb = put(&mut state, P0, "Blue Elemental Blast", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast_mode(&mut state, beb, 1, &[Target::Object(fanatic)]);
    change(&mut state, beb, ManaColor::R, ManaColor::G);
    settled(&mut state);
    assert_eq!(zone(&state, fanatic), Zone::Battlefield, "BEB fizzled");

    let mut state = islands();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let beb = put(&mut state, P1, "Blue Elemental Blast", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    pool(&mut state, P1, &[ManaColor::U], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    let bolt_item = state.stack.last().unwrap().v4.stack_item_id;
    act(&mut state, Action::Pass); // P0 passes -> P1 gets priority
    cast_mode(&mut state, beb, 0, &[Target::StackItem(bolt_item)]);
    change(&mut state, beb, ManaColor::R, ManaColor::G);
    settled(&mut state);
    assert_eq!(life(&state, P1), 17, "BEB fizzled so Bolt resolved");

    // Site 22: Red Elemental Blast — permanent mode & spell mode fizzle when changed on stack.
    let mut state = islands();
    let shapeshifter = put(&mut state, P1, "Volrath's Shapeshifter", Zone::Battlefield);
    let reb = put(&mut state, P0, "Red Elemental Blast", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast_mode(&mut state, reb, 1, &[Target::Object(shapeshifter)]);
    change(&mut state, reb, ManaColor::U, ManaColor::G);
    settled(&mut state);
    assert_eq!(zone(&state, shapeshifter), Zone::Battlefield, "REB fizzled");

    let mut state = islands();
    let charm = put(&mut state, P0, "Vision Charm", Zone::Hand);
    let reb = put(&mut state, P1, "Red Elemental Blast", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast_mode(&mut state, charm, 0, &[Target::Player(P1)]);
    let charm_item = state.stack.last().unwrap().v4.stack_item_id;
    act(&mut state, Action::Pass);
    cast_mode(&mut state, reb, 0, &[Target::StackItem(charm_item)]);
    change(&mut state, reb, ManaColor::U, ManaColor::G);
    settled(&mut state);
    assert_eq!(
        state.players[1].graveyard.len(),
        4,
        "REB fizzled so Vision Charm milled 4"
    );

    // Site 23: Pyroblast — resolves and does nothing to blue target after U -> G; destroys green target.
    let mut state = islands();
    let blue_perm = put(&mut state, P1, "Volrath's Shapeshifter", Zone::Battlefield);
    let green_perm = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let pyro1 = put(&mut state, P0, "Pyroblast", Zone::Hand);
    let pyro2 = put(&mut state, P0, "Pyroblast", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 0);
    cast_mode(&mut state, pyro1, 1, &[Target::Object(blue_perm)]);
    change(&mut state, pyro1, ManaColor::U, ManaColor::G);
    settled(&mut state);
    assert_eq!(
        zone(&state, blue_perm),
        Zone::Battlefield,
        "Pyroblast does nothing to non-green permanent"
    );
    cast_mode(&mut state, pyro2, 1, &[Target::Object(green_perm)]);
    change(&mut state, pyro2, ManaColor::U, ManaColor::G);
    settled(&mut state);
    assert_eq!(
        zone(&state, green_perm),
        Zone::Graveyard,
        "Pyroblast destroys green permanent"
    );

    // Site 24: Hydroblast — resolves and does nothing to red target after R -> G; destroys green target.
    let mut state = islands();
    let red_perm = put(&mut state, P1, "Mogg Fanatic", Zone::Battlefield);
    let green_perm = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let hydro1 = put(&mut state, P0, "Hydroblast", Zone::Hand);
    let hydro2 = put(&mut state, P0, "Hydroblast", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    cast_mode(&mut state, hydro1, 1, &[Target::Object(red_perm)]);
    change(&mut state, hydro1, ManaColor::R, ManaColor::G);
    settled(&mut state);
    assert_eq!(zone(&state, red_perm), Zone::Battlefield);
    cast_mode(&mut state, hydro2, 1, &[Target::Object(green_perm)]);
    change(&mut state, hydro2, ManaColor::R, ManaColor::G);
    settled(&mut state);
    assert_eq!(zone(&state, green_perm), Zone::Graveyard);

    // Site 25: Snuff Out — changed B -> G on stack makes green target illegal (fizzles).
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let snuff = put(&mut state, P0, "Snuff Out", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 3);
    cast(&mut state, snuff, &[Target::Object(elves)]);
    change(&mut state, snuff, ManaColor::B, ManaColor::G);
    settled(&mut state);
    assert_eq!(
        zone(&state, elves),
        Zone::Battlefield,
        "Snuff Out fizzled because target is green"
    );

    // Site 27: Anarchy — changed W -> G on stack destroys green permanents instead of white.
    let mut state = islands();
    let lions = put(&mut state, P1, "Savannah Lions", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let anarchy = put(&mut state, P0, "Anarchy", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 2);
    cast(&mut state, anarchy, &[]);
    change(&mut state, anarchy, ManaColor::W, ManaColor::G);
    settled(&mut state);
    assert_eq!(zone(&state, lions), Zone::Battlefield);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
}

// ---- Site 26: Plaguebearer ---------------------------------------------------------

#[test]
fn plaguebearer_targeting_and_captured_resolution_recheck() {
    let mut state = islands();
    let plaguebearer = put(&mut state, P0, "Plaguebearer", Zone::Battlefield);
    let lions = put(&mut state, P1, "Savannah Lions", Zone::Battlefield); // white, MV 1
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield); // black, MV 3
    let fanatic = put(&mut state, P1, "Mogg Fanatic", Zone::Battlefield); // red, MV 1

    // Change Plaguebearer B -> W: now "nonwhite creature with mana value X".
    change(&mut state, plaguebearer, ManaColor::B, ManaColor::W);
    pool(&mut state, P0, &[ManaColor::B], 6);
    assert_eq!(
        ability_targets(&state, plaguebearer, 1),
        vec![Target::Object(fanatic)],
        "white Lions excluded, red Fanatic legal"
    );
    assert_eq!(
        ability_targets(&state, plaguebearer, 3),
        vec![Target::Object(specter)],
        "black Specter is now legal for nonwhite Plaguebearer"
    );

    // Activate targeting black Specter (captures B -> W), then change Plaguebearer back W -> B
    // while ability is on stack: resolution recheck still uses captured W and destroys Specter!
    activate(&mut state, plaguebearer, 3, &[Target::Object(specter)]);
    change(&mut state, plaguebearer, ManaColor::W, ManaColor::B);
    settled(&mut state);
    assert_eq!(zone(&state, specter), Zone::Graveyard);
}

// ---- Sites 28–36: All 9 token creators & Layer-5 overrides -------------------------

#[test]
fn all_nine_token_creators_apply_changed_color_and_layer_5_overrides() {
    let blue = mana_color_mask(ManaColor::U);
    let red = mana_color_mask(ManaColor::R);
    let black = mana_color_mask(ManaColor::B);
    let white = mana_color_mask(ManaColor::W);
    let green = mana_color_mask(ManaColor::G);

    // Site 28: Call of the Herd (hard cast + flashback) + token departure + Painter's Servant Layer 5.
    let mut state = islands();
    let herd = put(&mut state, P0, "Call of the Herd", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, herd, &[]);
    change(&mut state, herd, ManaColor::G, ManaColor::U);
    settled(&mut state);
    let elephants = controls(&state, P0, "Elephant Token");
    assert_eq!(elephants.len(), 1);
    assert_eq!(engine::object_color_mask(&state, elephants[0]), blue);

    // Flashback from graveyard changed G -> R creates a red Elephant token.
    assert_eq!(zone(&state, herd), Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::G], 3);
    cast(&mut state, herd, &[]);
    change(&mut state, herd, ManaColor::G, ManaColor::R);
    settled(&mut state);
    let elephants = controls(&state, P0, "Elephant Token");
    assert_eq!(elephants.len(), 2);
    assert_eq!(engine::object_color_mask(&state, elephants[1]), red);

    // Layer-5 Painter's Servant adds white on top of the changed blue token.
    let painter = put(&mut state, P0, "Painter's Servant", Zone::Battlefield);
    state.objects.get_mut(painter).v4.chosen_color = Some(ManaColor::W);
    assert_eq!(
        engine::object_color_mask(&state, elephants[0]),
        blue | white
    );

    // Removing both tokens clears token_colors and returns text_color_changes_v1 to None.
    bury(&mut state, elephants[0]);
    bury(&mut state, elephants[1]);
    assert!(state.text_color_changes_v1.is_none());

    // Site 29: Siege-Gang Commander (400.7a permanent-spell carry-over -> ETB trigger capture).
    let mut state = islands();
    let siege = put(&mut state, P0, "Siege-Gang Commander", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 3);
    cast(&mut state, siege, &[]);
    change(&mut state, siege, ManaColor::R, ManaColor::U);
    settled(&mut state);
    assert_eq!(
        text_color_change_v1::text_color(&state, siege, ManaColor::R),
        ManaColor::U,
        "400.7a carry-over keeps change on permanent"
    );
    let goblins = controls(&state, P0, "Goblin Token");
    assert_eq!(goblins.len(), 3);
    for goblin in goblins {
        assert_eq!(engine::object_color_mask(&state, goblin), blue);
    }

    // Site 30: Decree of Justice — hard cast changed W -> B creates black Angel; cycling stays white.
    let mut state = islands();
    let decree = put(&mut state, P0, "Decree of Justice", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 4); // X = 1
    act(&mut state, Action::CastSpell(decree));
    announce_x(&mut state, 1);
    change(&mut state, decree, ManaColor::W, ManaColor::B);
    settled(&mut state);
    let angels = controls(&state, P0, "Angel Token");
    assert_eq!(angels.len(), 1);
    assert_eq!(engine::object_color_mask(&state, angels[0]), black);

    let mut state = islands();
    let decree = put(&mut state, P0, "Decree of Justice", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 3);
    activate(&mut state, decree, 0, &[]); // cycle {2}{W}, pay {1}
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption { .. }) => act(&mut state, Action::ChooseEffectOption(1)),
        other => panic!("expected cycling X choice, got {other:?}"),
    }
    settled(&mut state);
    let soldiers = controls(&state, P0, "Soldier Token");
    assert_eq!(soldiers.len(), 1);
    assert_eq!(engine::object_color_mask(&state, soldiers[0]), white);
    assert!(state.text_color_changes_v1.is_none());

    // Site 31: Chatter of the Squirrel (spell G -> R).
    let mut state = islands();
    let chatter = put(&mut state, P0, "Chatter of the Squirrel", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, chatter, &[]);
    change(&mut state, chatter, ManaColor::G, ManaColor::R);
    settled(&mut state);
    let squirrels = controls(&state, P0, "Squirrel Token");
    assert_eq!(squirrels.len(), 1);
    assert_eq!(engine::object_color_mask(&state, squirrels[0]), red);

    // Site 32: Deranged Hermit (400.7a spell G -> W -> 4 white Squirrels).
    let mut state = islands();
    let hermit = put(&mut state, P0, "Deranged Hermit", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 3);
    cast(&mut state, hermit, &[]);
    change(&mut state, hermit, ManaColor::G, ManaColor::W);
    settled(&mut state);
    let squirrels = controls(&state, P0, "Squirrel Token");
    assert_eq!(squirrels.len(), 4);
    for sq in squirrels {
        assert_eq!(engine::object_color_mask(&state, sq), white);
    }

    // Site 33: Caller of the Claw (400.7a spell G -> U -> blue Bear token).
    let mut state = islands();
    let victim = put(&mut state, P0, "Mogg Fanatic", Zone::Battlefield);
    bury(&mut state, victim);
    let caller = put(&mut state, P0, "Caller of the Claw", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, caller, &[]);
    change(&mut state, caller, ManaColor::G, ManaColor::U);
    settled(&mut state);
    let bears = controls(&state, P0, "Bear Token");
    assert_eq!(bears.len(), 1);
    assert_eq!(engine::object_color_mask(&state, bears[0]), blue);

    // Site 34: Verdant Force (upkeep trigger capture G -> B -> black Saproling).
    let mut state = islands();
    let force = put(&mut state, P0, "Verdant Force", Zone::Battlefield);
    change(&mut state, force, ManaColor::G, ManaColor::B);
    to_next_upkeep_trigger(&mut state);
    settled(&mut state);
    let saprolings = controls(&state, P0, "Saproling Token");
    assert_eq!(saprolings.len(), 1);
    assert_eq!(engine::object_color_mask(&state, saprolings[0]), black);

    // Site 35: Zombie Infestation (activation capture B -> W -> white Zombie).
    let mut state = islands();
    let infestation = put(&mut state, P0, "Zombie Infestation", Zone::Battlefield);
    put(&mut state, P0, "Island", Zone::Hand);
    put(&mut state, P0, "Island", Zone::Hand);
    change(&mut state, infestation, ManaColor::B, ManaColor::W);
    activate(&mut state, infestation, 0, &[]);
    settle_with(&mut state, |_, decision| first_answer(decision));
    let zombies = controls(&state, P0, "Zombie Token");
    assert_eq!(zombies.len(), 1);
    assert_eq!(engine::object_color_mask(&state, zombies[0]), white);

    // Site 36: Phyrexian Processor (activation capture B -> G -> green Phyrexian Minion).
    let mut state = islands();
    let processor = put(&mut state, P0, "Phyrexian Processor", Zone::Hand);
    pool(&mut state, P0, &[], 4);
    cast(&mut state, processor, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption { .. }) => act(&mut state, Action::ChooseEffectOption(4)),
        other => panic!("expected Processor payment, got {other:?}"),
    }
    settled(&mut state);
    change(&mut state, processor, ManaColor::B, ManaColor::G);
    pool(&mut state, P0, &[], 4);
    activate(&mut state, processor, 0, &[]);
    // Changing again on stack leaves captured green intact.
    change(&mut state, processor, ManaColor::G, ManaColor::R);
    settled(&mut state);
    let minions = controls(&state, P0, "Phyrexian Minion Token");
    assert_eq!(minions.len(), 1);
    assert_eq!(engine::object_color_mask(&state, minions[0]), green);
}

// ---- Sites 37–38: Treetop Village & Ghitu Encampment -------------------------------

#[test]
fn land_animations_fix_color_at_resolution_and_clear_at_cleanup() {
    for (land_name, printed_color, mana_needed) in [
        ("Treetop Village", ManaColor::G, (vec![ManaColor::G], 1)),
        ("Ghitu Encampment", ManaColor::R, (vec![ManaColor::R], 1)),
    ] {
        // 1. Unchanged land writes NO animation_colors entry and keeps printed color.
        let mut state = islands();
        let land = put(&mut state, P0, land_name, Zone::Battlefield);
        pool(&mut state, P0, &mana_needed.0, mana_needed.1);
        activate(&mut state, land, 0, &[]);
        settled(&mut state);
        assert!(
            state.text_color_changes_v1.is_none(),
            "{land_name}: unchanged animation must write nothing"
        );
        assert_eq!(
            engine::object_color_mask(&state, land),
            mana_color_mask(printed_color)
        );

        // 2. Changed to blue before activation -> becomes blue; changing again same turn
        //    leaves the resolved animation blue (CR 611.2c); cleanup drops animation_colors.
        let mut state = islands();
        let land = put(&mut state, P0, land_name, Zone::Battlefield);
        change(&mut state, land, printed_color, ManaColor::U);
        pool(&mut state, P0, &mana_needed.0, mana_needed.1);
        activate(&mut state, land, 0, &[]);
        settled(&mut state);
        assert_eq!(
            engine::object_color_mask(&state, land),
            mana_color_mask(ManaColor::U),
            "{land_name} animated as blue"
        );
        assert_eq!(
            state
                .text_color_changes_v1
                .as_ref()
                .unwrap()
                .animation_colors
                .len(),
            1
        );

        // Changing the land's text again the same turn does not recolor the resolved animation.
        change(&mut state, land, ManaColor::U, ManaColor::W);
        assert_eq!(
            engine::object_color_mask(&state, land),
            mana_color_mask(ManaColor::U),
            "{land_name} resolved animation stays blue (611.2c)"
        );

        // Passing to next turn's main phase runs cleanup and removes animation_colors.
        to_main_phase_of(&mut state, P1);
        assert!(
            state
                .text_color_changes_v1
                .as_ref()
                .unwrap()
                .animation_colors
                .is_empty(),
            "{land_name} animation_colors cleared at cleanup"
        );
        assert_eq!(
            engine::object_color_mask(&state, land),
            0,
            "{land_name} is colorless land again after cleanup"
        );
    }
}

// ---- Cost-only (all 5) & sample no-word cards --------------------------------------

#[test]
fn cost_only_and_no_word_cards_record_change_with_no_effect() {
    let mut state = islands();
    for name in COST_ONLY_COLOR_WORD_CARDS_V1 {
        let obj = put(&mut state, P0, name, Zone::Battlefield);
        change(&mut state, obj, ManaColor::B, ManaColor::G);
        assert_eq!(
            text_color_change_v1::live_changes_for_object(&state, obj),
            vec![(ManaColor::B, ManaColor::G)]
        );
    }
    for name in [
        "Black Vise",
        "Avatar of Woe",
        "Flooded Strand",
        "Mother of Runes",
        "Lurking Skirge",
    ] {
        let obj = put(&mut state, P0, name, Zone::Battlefield);
        change(&mut state, obj, ManaColor::B, ManaColor::U);
        assert_eq!(
            text_color_change_v1::live_changes_for_object(&state, obj),
            vec![(ManaColor::B, ManaColor::U)]
        );
    }
    // Avatar of Woe still has fear (black is reminder text, not rules text: CR 702.36).
    let mut state = islands();
    let avatar = put(&mut state, P0, "Avatar of Woe", Zone::Battlefield);
    let black_blocker = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let green_blocker = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    change(&mut state, avatar, ManaColor::B, ManaColor::G);
    let blockers = legal_blockers(&mut state, avatar);
    assert!(blockers.contains(&black_blocker));
    assert!(!blockers.contains(&green_blocker));
}

// ---- Schema 5 projection, choose_text_color_word_v1 actions, and consumer refusals -

#[test]
fn schema_5_projection_ordering_face_down_masking_actions_and_refusals() {
    let mut state = islands();
    enable_public_shroud_v1(&mut state);

    // Seat 0 battlefield: p0_first (face-up, 2 timestamped changes), p0_face_down (face-down).
    let p0_first = put(&mut state, P0, "Goblin Piledriver", Zone::Battlefield);
    let p0_face_down = put(&mut state, P0, "Exalted Angel", Zone::Battlefield);
    state.objects.get_mut(p0_face_down).v4.face_down_v1 = Some(FaceDownV1 {
        disguised: true,
        lookers: 1,
        hidden_by: None,
        hidden_by_source: None,
    });
    // Seat 1 battlefield: p1_perm.
    let p1_perm = put(&mut state, P1, "Absolute Law", Zone::Battlefield);
    // Stack spells: bottom spell then top spell.
    let stack_bottom = put(&mut state, P0, "Call of the Herd", Zone::Hand);
    let stack_top = put(&mut state, P0, "Hydroblast", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::U], 2);
    cast(&mut state, stack_bottom, &[]);
    cast_mode(&mut state, stack_top, 1, &[Target::Object(p1_perm)]);

    // Apply changes in scrambled order so projection ordering is genuinely tested.
    change(&mut state, stack_top, ManaColor::R, ManaColor::G);
    change(&mut state, p1_perm, ManaColor::R, ManaColor::B);
    change(&mut state, p0_first, ManaColor::U, ManaColor::G);
    change(&mut state, stack_bottom, ManaColor::G, ManaColor::W);
    change(&mut state, p0_face_down, ManaColor::W, ManaColor::U);
    change(&mut state, p0_first, ManaColor::G, ManaColor::R);

    let obs = observe_policy_v5(&state, &PolicySurfaceV5::new(), P1, 0, 0, 0, 1).unwrap();
    let rows = obs
        .projection
        .text_color_changes
        .as_ref()
        .expect("Schema 5 exposes text_color_changes");
    let got: Vec<(u32, ManaColor, ManaColor)> = rows
        .iter()
        .map(|row| (row.object.arena_id, row.from, row.to))
        .collect();
    assert_eq!(
        got,
        vec![
            // Seat 0 battlefield: p0_first in timestamp order, then p0_face_down.
            (p0_first.0, ManaColor::U, ManaColor::G),
            (p0_first.0, ManaColor::G, ManaColor::R),
            (p0_face_down.0, ManaColor::W, ManaColor::U),
            // Seat 1 battlefield.
            (p1_perm.0, ManaColor::R, ManaColor::B),
            // Stack bottom to top.
            (stack_bottom.0, ManaColor::G, ManaColor::W),
            (stack_top.0, ManaColor::R, ManaColor::G),
        ]
    );
    // Face-down permanent's reference uses the neutral sentinel, NEVER Exalted Angel's card_db_id.
    assert_eq!(rows[2].object.card_db_id, id("Face-down creature"));
    assert_ne!(rows[2].object.card_db_id, id("Exalted Angel"));

    // Non-Schema-5 consumers must refuse a state with text_color_changes_v1:
    // 1. observe_v1
    assert!(observe_v1(&state, P0, 0).is_err());
    // 2. Schema 4 (public_shroud_v1 == false)
    let mut schema4 = state.clone();
    schema4.public_shroud_v1 = false;
    assert!(observe_policy_v5(&schema4, &PolicySurfaceV5::new(), P0, 0, 0, 0, 1).is_err());
    // 3. Frozen PremodernOrderedV1Artifact
    assert!(observe_premodern_ordered_v1(&state, &HarnessSurfaceV2::new(), P0, 0).is_err());
    // 4. infoset_v1::export_v2 refuses with RefusalCodeV1::R3
    let deck = vec![id("Island"); 20];
    let export_err = export_v2(&ExportInputV1 {
        state: &state,
        journal: state.observation_journal.notes(),
        viewer: P0,
        decklists: [&deck, &deck],
        marks: None,
    })
    .unwrap_err();
    assert!(
        matches!(
            export_err,
            ExportErrorV1::Refused {
                code: RefusalCodeV1::R3,
                ..
            }
        ),
        "expected R3 refusal, got {export_err:?}"
    );

    // ActionSemanticV1::ChooseTextColorWordV1 (`choose_text_color_word_v1`) for both stages.
    for (stage, from, expected_colors) in [
        (
            TextColorWordStageV1::From,
            None,
            vec![
                ManaColor::W,
                ManaColor::U,
                ManaColor::B,
                ManaColor::R,
                ManaColor::G,
            ],
        ),
        (
            TextColorWordStageV1::To,
            Some(ManaColor::B),
            vec![ManaColor::W, ManaColor::U, ManaColor::R, ManaColor::G],
        ),
    ] {
        let mut choice_state = islands();
        enable_public_shroud_v1(&mut choice_state);
        let perm = put(&mut choice_state, P1, "Absolute Law", Zone::Battlefield);
        let spell = put(&mut choice_state, P0, "Hydroblast", Zone::Hand);
        pool(&mut choice_state, P0, &[ManaColor::U], 0);
        cast_mode(&mut choice_state, spell, 1, &[Target::Object(perm)]);
        let resolving_item = choice_state.stack.pop().unwrap();
        assert_eq!(resolving_item.kind, StackItemKind::Spell);
        let mut ctx = ExecCtx::no_targets(spell, P0);
        ctx.stack_item_id = Some(resolving_item.v4.stack_item_id);
        ctx.targets = vec![Target::Object(perm)];
        choice_state.engine.pending_effect = Some(EffectContinuation {
            resolving_item,
            ctx,
            frames: Vec::new(),
            choice: Some(PendingEffectChoice::ChooseOption {
                player: P0,
                path: Vec::new(),
                options: vec![EffectOp::Sequence(Vec::new()); expected_colors.len()],
                purpose: EffectOptionChoicePurpose::ChooseTextColorWordV1 { stage, from },
            }),
            answered_choice_guard: None,
        });

        // Schema 5 observes it; Schema 4, observe_v1, PremodernOrdered, and infoset refuse it.
        observe_policy_v5(&choice_state, &PolicySurfaceV5::new(), P0, 0, 0, 0, 1).unwrap();
        assert!(observe_v1(&choice_state, P0, 0).is_err());
        let mut s4 = choice_state.clone();
        s4.public_shroud_v1 = false;
        assert!(observe_policy_v5(&s4, &PolicySurfaceV5::new(), P0, 0, 0, 0, 1).is_err());
        assert!(
            observe_premodern_ordered_v1(&choice_state, &HarnessSurfaceV2::new(), P0, 0).is_err()
        );
        let err = export_v2(&ExportInputV1 {
            state: &choice_state,
            journal: choice_state.observation_journal.notes(),
            viewer: P0,
            decklists: [&deck, &deck],
            marks: None,
        })
        .unwrap_err();
        assert!(
            matches!(
                err,
                ExportErrorV1::Refused {
                    code: RefusalCodeV1::R3,
                    ..
                }
            ),
            "expected R3 refusal, got {err:?}"
        );

        // Legal action semantics emit `choose_text_color_word_v1` with exact fields.
        let mut surface = PolicySurfaceV5::new();
        let decision = surface.next_decision(&mut choice_state).unwrap();
        assert!(matches!(
            decision,
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::ChooseEffectOption {
                player: P0,
                ..
            }))
        ));
        let candidates =
            policy_legal_action_candidates_v5(&decision, &surface, &choice_state).unwrap();
        assert_eq!(candidates.len(), expected_colors.len());
        for (idx, (candidate, expected_color)) in
            candidates.iter().zip(&expected_colors).enumerate()
        {
            let serialized = serde_json::to_value(&candidate.record.semantic).unwrap();
            assert_eq!(serialized["action_kind"], "choose_text_color_word_v1");
            match &candidate.record.semantic {
                ActionSemanticV1::ChooseTextColorWordV1 {
                    actor,
                    source,
                    target,
                    stage: actual_stage,
                    from: actual_from,
                    color,
                    option_index,
                    option_count,
                } => {
                    assert_eq!(*actor, PlayerSeatV1::P0);
                    assert_eq!(source.arena_id, spell.0);
                    assert!(matches!(
                        target,
                        TargetRefV1::Object { object } if object.arena_id == perm.0
                    ));
                    assert_eq!(*actual_stage, stage);
                    assert_eq!(*actual_from, from);
                    assert_eq!(*color, *expected_color);
                    assert_eq!(usize::from(*option_index), idx);
                    assert_eq!(usize::from(*option_count), expected_colors.len());
                }
                other => panic!("expected ChooseTextColorWordV1, got {other:?}"),
            }
        }
        // Answering with ChooseEffectOption(0) succeeds cleanly.
        surface
            .apply(&mut choice_state, candidates[0].policy_action.clone())
            .unwrap();
    }
}
