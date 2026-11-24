//! Premodern wave P3i (Step C2 of Issue #460): **Alter Reality** ({1}{U},
//! Instant, Flashback {1}{U}), `StandardTargetV1::SpellOrPermanentV1`,
//! Schema 5 `ChooseTextColorWordV1` + `text_color_changes`, the ruled CR 613.8
//! Volrath's Shapeshifter x Alter Reality interaction (§4.7), and full Schema 5
//! games with Alter Reality in both seats (§5.2).
#![cfg(feature = "eternal-fixtures")]

use crate::eternal_catalog_v1::{play_observed, stress_deck, Policy};
use mtg_kernel::card_def::{card_id_by_name, CardCapability, CardDef, TargetSpec, CARD_DEFS};
use mtg_kernel::engine::{self, Action, CastMode, Decision, RuntimeRulesV1, UnsupportedMechanic};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::policy_surface_v5::{PolicyDecisionV5, PolicySurfaceV5};
use mtg_kernel::rl::{
    enable_public_shroud_v1, observe_policy_v5, policy_legal_action_candidates_v5,
    ActionSemanticV1, PlayerSeatV1, TargetRefV1,
};
use mtg_kernel::standard_cards_v1::StandardTargetV1;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface::SurfaceDecision;
use mtg_kernel::text_color_change_v1::{self, TextColorWordStageV1, COLOR_WORDS};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const ALTER_REALITY: &str = "Alter Reality";
const VOLRATH: &str = "Volrath's Shapeshifter";

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("missing card definition: {name}"))
}

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn islands_for(active: PlayerId) -> GameState {
    let ids = vec![id("Island"); 20];
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |card_id| CARD_DEFS[card_id as usize].object_name.into(),
        0x5033_6900_0001,
    );
    state.active_player = active;
    state.priority_player = active;
    state.step = Step::Main1;
    state
}

fn ordered_islands_for(active: PlayerId) -> GameState {
    let mut state = islands_for(active);
    state.engine.runtime_rules = RuntimeRulesV1::OWNER_ORDERED_GRAVEYARD_V1;
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

fn priority_castable(state: &mut GameState) -> Vec<ObjectId> {
    match next(state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => castable_spells,
        other => panic!("expected priority, got {other:?}"),
    }
}

fn castable(state: &mut GameState, spell: ObjectId) -> bool {
    priority_castable(state).contains(&spell)
}

fn spell_targets(state: &GameState, spell: ObjectId) -> Vec<Target> {
    let mut probe = state.clone();
    if !castable(&mut probe, spell) {
        return Vec::new();
    }
    act(&mut probe, Action::CastSpell(spell));
    loop {
        match next(&mut probe) {
            Decision::ChooseCastMode { options, .. } => {
                let mode = if options.contains(&CastMode::Normal) {
                    CastMode::Normal
                } else {
                    options[0]
                };
                act(&mut probe, Action::ChooseCastMode(mode));
            }
            Decision::ChooseTargets { legal_targets, .. } => return legal_targets,
            other => panic!("expected ChooseTargets, got {other:?}"),
        }
    }
}

fn cast_with_targets(state: &mut GameState, spell: ObjectId, targets: &[Target]) {
    assert!(
        castable(state, spell),
        "{} not castable",
        state.objects.get(spell).name
    );
    act(state, Action::CastSpell(spell));
    for &target in targets {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(
                    legal_targets.contains(&target),
                    "{target:?} not in {legal_targets:?}"
                );
                act(state, Action::ChooseTarget(target));
            }
            other => panic!("expected ChooseTargets, got {other:?}"),
        }
    }
    let _ = next(state);
}

fn cast_mode_with_targets(state: &mut GameState, spell: ObjectId, mode: u8, targets: &[Target]) {
    assert!(
        castable(state, spell),
        "{} not castable",
        state.objects.get(spell).name
    );
    act(state, Action::CastSpell(spell));
    if let Decision::ChooseSpellMode { legal_modes, .. } = next(state) {
        assert!(legal_modes.contains(&mode), "{mode} not in {legal_modes:?}");
        act(state, Action::ChooseSpellMode(mode));
    }
    for &target in targets {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(
                    legal_targets.contains(&target),
                    "{target:?} not in {legal_targets:?}"
                );
                act(state, Action::ChooseTarget(target));
            }
            other => panic!("expected ChooseTargets, got {other:?}"),
        }
    }
    let _ = next(state);
}

fn from_option_index(from: ManaColor) -> u16 {
    COLOR_WORDS
        .iter()
        .position(|&color| color == from)
        .expect("color word") as u16
}

fn to_option_index(from: ManaColor, to: ManaColor) -> u16 {
    text_color_change_v1::choice_colors(TextColorWordStageV1::To, Some(from))
        .expect("valid from color")
        .into_iter()
        .position(|color| color == to)
        .expect("distinct replacement color") as u16
}

/// Resolves the top-of-stack Alter Reality by passing priority until its two
/// `ChooseEffectOption` menus appear and answering `from -> to`.
fn resolve_alter_reality(
    state: &mut GameState,
    caster: PlayerId,
    alter: ObjectId,
    from: ManaColor,
    to: ManaColor,
) {
    loop {
        match next(state) {
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::ChooseEffectOption {
                player,
                source,
                option_count,
            } => {
                assert_eq!(player, caster);
                assert_eq!(source, alter);
                assert_eq!(option_count, 5);
                act(state, Action::ChooseEffectOption(from_option_index(from)));
                break;
            }
            other => panic!("expected Alter Reality From choice, got {other:?}"),
        }
    }
    match next(state) {
        Decision::ChooseEffectOption {
            player,
            source,
            option_count,
        } => {
            assert_eq!(player, caster);
            assert_eq!(source, alter);
            assert_eq!(option_count, 4);
            act(state, Action::ChooseEffectOption(to_option_index(from, to)));
        }
        other => panic!("expected Alter Reality To choice, got {other:?}"),
    }
}

/// Puts Alter Reality into `caster`'s hand, gives `{1}{U}`, casts it targeting
/// `target`, resolves it with `from -> to`, and returns the Alter Reality object.
fn cast_and_resolve_alter_reality(
    state: &mut GameState,
    caster: PlayerId,
    target: ObjectId,
    from: ManaColor,
    to: ManaColor,
) -> ObjectId {
    let alter = put(state, caster, ALTER_REALITY, Zone::Hand);
    pool(state, caster, &[ManaColor::U], 1);
    cast_with_targets(state, alter, &[Target::Object(target)]);
    resolve_alter_reality(state, caster, alter, from, to);
    let _ = next(state);
    alter
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn hit(state: &mut GameState, source: ObjectId, target: Target, amount: i32) {
    event::propose_and_commit(state, ProposedEvent::damage(source, target, amount));
}

#[test]
fn alter_reality_definition_and_target_spec_rules() {
    let card = def(ALTER_REALITY);
    assert_eq!(card.capability, CardCapability::Full);
    assert_eq!(card.mana_value, 2);
    assert_eq!(
        card.target_spec,
        TargetSpec::StandardV1(StandardTargetV1::SpellOrPermanentV1)
    );
    assert!(card.flashback.is_some());

    // Empty battlefield and empty stack: Alter Reality cannot target itself (115.5).
    let mut empty = islands_for(P0);
    let alter_empty = put(&mut empty, P0, ALTER_REALITY, Zone::Hand);
    pool(&mut empty, P0, &[ManaColor::U], 1);
    assert!(
        !castable(&mut empty, alter_empty),
        "Alter Reality must not be castable with no other spell or permanent"
    );

    // Mixed battlefield (own/opponent lands & creatures, shroud, pro-blue) + stack spell.
    let mut state = islands_for(P0);
    let own_land = put(&mut state, P0, "Island", Zone::Battlefield);
    let opp_land = put(&mut state, P1, "Mountain", Zone::Battlefield);
    let enchantress = put(&mut state, P1, "Argothian Enchantress", Zone::Battlefield);
    let piledriver = put(&mut state, P1, "Goblin Piledriver", Zone::Battlefield);
    let spell = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast_with_targets(&mut state, spell, &[Target::Player(P1)]);

    let alter = put(&mut state, P0, ALTER_REALITY, Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    let targets = spell_targets(&state, alter);
    assert!(
        targets.contains(&Target::Object(spell)),
        "targets stack spell"
    );
    assert!(
        targets.contains(&Target::Object(own_land)),
        "targets own land"
    );
    assert!(
        targets.contains(&Target::Object(opp_land)),
        "targets opponent land"
    );
    assert!(
        !targets.contains(&Target::Object(alter)),
        "cannot target itself (CR 115.5)"
    );
    assert!(
        !targets.contains(&Target::Object(enchantress)),
        "respects shroud"
    );
    assert!(
        !targets.contains(&Target::Object(piledriver)),
        "respects protection from blue"
    );

    // Flagbearer obligation: opponent's Standard Bearer must be chosen.
    let mut flag = islands_for(P0);
    let bearer = put(&mut flag, P1, "Standard Bearer", Zone::Battlefield);
    let _other = put(&mut flag, P1, "Island", Zone::Battlefield);
    let alter_flag = put(&mut flag, P0, ALTER_REALITY, Zone::Hand);
    pool(&mut flag, P0, &[ManaColor::U], 1);
    assert_eq!(
        spell_targets(&flag, alter_flag),
        vec![Target::Object(bearer)]
    );
}

#[test]
fn alter_reality_permanent_target_full_twenty_pair_menu_and_schema_5_semantics() {
    // Exercise all 20 ordered distinct (from, to) pairs across both seats.
    let mut pairs = 0usize;
    for seat in [P0, P1] {
        for &from in &COLOR_WORDS {
            let tos = text_color_change_v1::choice_colors(TextColorWordStageV1::To, Some(from))
                .expect("valid from color");
            assert_eq!(tos.len(), 4);
            for to in tos {
                let mut state = islands_for(seat);
                let priest = put(&mut state, seat, "Soltari Priest", Zone::Battlefield);
                let alter = cast_and_resolve_alter_reality(&mut state, seat, priest, from, to);
                settled(&mut state);
                assert_eq!(zone(&state, alter), Zone::Graveyard);
                assert_eq!(
                    text_color_change_v1::live_changes_for_object(&state, priest),
                    vec![(from, to)]
                );
                assert_eq!(text_color_change_v1::text_color(&state, priest, from), to);
                pairs += 1;
            }
        }
    }
    assert_eq!(pairs, 40);

    // Schema 5 / PolicySurfaceV5 check with snapshot/restore mid-choice.
    for seat in [P0, P1] {
        let mut state = islands_for(seat);
        enable_public_shroud_v1(&mut state);
        let priest = put(&mut state, seat, "Soltari Priest", Zone::Battlefield);
        let alter = put(&mut state, seat, ALTER_REALITY, Zone::Hand);
        pool(&mut state, seat, &[ManaColor::U], 1);
        cast_with_targets(&mut state, alter, &[Target::Object(priest)]);

        // Pass priority until Alter Reality enters stage From.
        while matches!(next(&mut state), Decision::CastSpellOrPass { .. }) {
            act(&mut state, Action::Pass);
        }

        // Before resolution finishes, text_color_changes is still absent.
        let mut surface = PolicySurfaceV5::new();
        let obs_from = observe_policy_v5(&state, &surface, seat, 0, 0, 0, 1).unwrap();
        assert!(obs_from.projection.text_color_changes.is_none());

        let from_decision = surface.next_decision(&mut state).unwrap();
        assert!(matches!(
            from_decision,
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::ChooseEffectOption {
                option_count: 5,
                ..
            }))
        ));
        let from_actions =
            policy_legal_action_candidates_v5(&from_decision, &surface, &state).unwrap();
        assert_eq!(from_actions.len(), 5);
        for (idx, (&expected_color, candidate)) in
            COLOR_WORDS.iter().zip(from_actions.iter()).enumerate()
        {
            match &candidate.record.semantic {
                ActionSemanticV1::ChooseTextColorWordV1 {
                    actor,
                    source,
                    target,
                    stage,
                    from,
                    color,
                    option_index,
                    option_count,
                } => {
                    assert_eq!(*actor, PlayerSeatV1::from(seat));
                    assert_eq!(source.arena_id, alter.0);
                    assert!(matches!(
                        target,
                        TargetRefV1::Object { object } if object.arena_id == priest.0
                    ));
                    assert_eq!(*stage, TextColorWordStageV1::From);
                    assert_eq!(*from, None);
                    assert_eq!(*color, expected_color);
                    assert_eq!(*option_index, idx as u16);
                    assert_eq!(*option_count, 5);
                }
                other => panic!("expected ChooseTextColorWordV1 From, got {other:?}"),
            }
        }

        // Choose Red (index 3) via PolicySurfaceV5, then snapshot/restore before stage To.
        surface
            .apply(
                &mut state,
                from_actions[usize::from(from_option_index(ManaColor::R))]
                    .policy_action
                    .clone(),
            )
            .unwrap();
        let mut state = restored(&state);
        let to_decision = surface.next_decision(&mut state).unwrap();
        assert!(matches!(
            to_decision,
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::ChooseEffectOption {
                option_count: 4,
                ..
            }))
        ));

        let to_actions = policy_legal_action_candidates_v5(&to_decision, &surface, &state).unwrap();
        let expected_tos = [ManaColor::W, ManaColor::U, ManaColor::B, ManaColor::G];
        assert_eq!(to_actions.len(), 4);
        for (idx, (&expected_color, candidate)) in
            expected_tos.iter().zip(to_actions.iter()).enumerate()
        {
            match &candidate.record.semantic {
                ActionSemanticV1::ChooseTextColorWordV1 {
                    stage,
                    from,
                    color,
                    option_index,
                    option_count,
                    ..
                } => {
                    assert_eq!(*stage, TextColorWordStageV1::To);
                    assert_eq!(*from, Some(ManaColor::R));
                    assert_eq!(*color, expected_color);
                    assert_eq!(*option_index, idx as u16);
                    assert_eq!(*option_count, 4);
                }
                other => panic!("expected ChooseTextColorWordV1 To, got {other:?}"),
            }
        }

        // Choose Green (index 3 in [W, U, B, G]).
        surface
            .apply(
                &mut state,
                to_actions[usize::from(to_option_index(ManaColor::R, ManaColor::G))]
                    .policy_action
                    .clone(),
            )
            .unwrap();
        settled(&mut state);

        // Projection now exposes the recorded change in order.
        let obs_after =
            observe_policy_v5(&state, &PolicySurfaceV5::new(), seat, 0, 0, 0, 2).unwrap();
        let changes = obs_after
            .projection
            .text_color_changes
            .expect("projection exposes text_color_changes");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].object.arena_id, priest.0);
        assert_eq!(changes[0].from, ManaColor::R);
        assert_eq!(changes[0].to, ManaColor::G);

        // Soltari Priest now has protection from green, not red.
        let bolt = put(&mut state, seat.opponent(), "Lightning Bolt", Zone::Hand);
        let rancor = put(&mut state, seat.opponent(), "Rancor", Zone::Hand);
        state.active_player = seat.opponent();
        state.priority_player = seat.opponent();
        pool(
            &mut state,
            seat.opponent(),
            &[ManaColor::R, ManaColor::G],
            0,
        );
        assert!(spell_targets(&state, bolt).contains(&Target::Object(priest)));
        assert!(!spell_targets(&state, rancor).contains(&Target::Object(priest)));
    }
}

#[test]
fn alter_reality_spell_target_400_7a_carryover_and_timestamp_order() {
    // 1) Spell target on the stack: change Red Elemental Blast's "blue" to "red",
    // making its Island target illegal on resolution (CR 608.2b).
    let mut state = islands_for(P0);
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    let reb = put(&mut state, P0, "Red Elemental Blast", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    // Mode 1 = destroy target blue permanent.
    cast_mode_with_targets(&mut state, reb, 1, &[Target::Object(island)]);
    // Pass P0 priority to P1 so P1 responds with Alter Reality targeting REB.
    act(&mut state, Action::Pass);
    let _alter = cast_and_resolve_alter_reality(&mut state, P1, reb, ManaColor::U, ManaColor::R);
    settled(&mut state);
    assert_eq!(
        zone(&state, island),
        Zone::Battlefield,
        "Island survives because REB now requires a red permanent"
    );
    assert_eq!(zone(&state, reb), Zone::Graveyard);
    assert!(
        state.text_color_changes_v1.is_none(),
        "REB's change ended when REB left the stack"
    );

    // 2) CR 400.7a permanent-spell carry-over + CR 613.7 timestamp chaining.
    let mut state = islands_for(P0);
    enable_public_shroud_v1(&mut state);
    let priest = put(&mut state, P0, "Soltari Priest", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 0);
    cast_with_targets(&mut state, priest, &[]);
    assert_eq!(zone(&state, priest), Zone::Stack);

    // Alter Reality #1 targets Soltari Priest on the stack: R -> G.
    let _alter1 =
        cast_and_resolve_alter_reality(&mut state, P0, priest, ManaColor::R, ManaColor::G);
    settled(&mut state);
    assert_eq!(zone(&state, priest), Zone::Battlefield);
    assert_eq!(
        text_color_change_v1::live_changes_for_object(&state, priest),
        vec![(ManaColor::R, ManaColor::G)]
    );

    // Alter Reality #2 targets the battlefield Soltari Priest: G -> W (timestamp order 613.7).
    let _alter2 =
        cast_and_resolve_alter_reality(&mut state, P0, priest, ManaColor::G, ManaColor::W);
    settled(&mut state);
    assert_eq!(
        text_color_change_v1::live_changes_for_object(&state, priest),
        vec![(ManaColor::R, ManaColor::G), (ManaColor::G, ManaColor::W)]
    );
    let obs = observe_policy_v5(&state, &PolicySurfaceV5::new(), P0, 0, 0, 0, 1).unwrap();
    let changes = obs.projection.text_color_changes.unwrap();
    assert_eq!(
        changes
            .iter()
            .map(|entry| (entry.from, entry.to))
            .collect::<Vec<_>>(),
        vec![(ManaColor::R, ManaColor::G), (ManaColor::G, ManaColor::W)]
    );

    // Now Soltari Priest has protection from white (Swords blocked, Rancor legal).
    let swords = put(&mut state, P1, "Swords to Plowshares", Zone::Hand);
    let rancor = put(&mut state, P1, "Rancor", Zone::Hand);
    state.active_player = P1;
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::W, ManaColor::G], 0);
    assert!(!spell_targets(&state, swords).contains(&Target::Object(priest)));
    assert!(spell_targets(&state, rancor).contains(&Target::Object(priest)));
}

#[test]
fn alter_reality_flashback_and_illegal_target_fizzle() {
    // Flashback {1}{U} from graveyard applies the change and exiles Alter Reality.
    let mut state = islands_for(P0);
    let light = put(&mut state, P0, "Light of Day", Zone::Battlefield);
    let alter = put(&mut state, P0, ALTER_REALITY, Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast_with_targets(&mut state, alter, &[Target::Object(light)]);
    resolve_alter_reality(&mut state, P0, alter, ManaColor::B, ManaColor::G);
    settled(&mut state);
    assert_eq!(
        zone(&state, alter),
        Zone::Exile,
        "flashback exiles Alter Reality on resolution (CR 702.34a)"
    );
    assert_eq!(
        text_color_change_v1::live_changes_for_object(&state, light),
        vec![(ManaColor::B, ManaColor::G)]
    );

    // Illegal target on resolution (CR 608.2b): fizzles with zero choices.
    let mut fizzle = islands_for(P0);
    let elves = put(&mut fizzle, P1, "Llanowar Elves", Zone::Battlefield);
    let alter_fb = put(&mut fizzle, P0, ALTER_REALITY, Zone::Graveyard);
    let swords = put(&mut fizzle, P0, "Swords to Plowshares", Zone::Hand);
    pool(&mut fizzle, P0, &[ManaColor::U, ManaColor::W], 1);
    cast_with_targets(&mut fizzle, alter_fb, &[Target::Object(elves)]);
    cast_with_targets(&mut fizzle, swords, &[Target::Object(elves)]);
    // Settling must prompt NO ChooseEffectOption (fizzle skips all choices).
    settled(&mut fizzle);
    assert_eq!(zone(&fizzle, elves), Zone::Exile);
    assert_eq!(
        zone(&fizzle, alter_fb),
        Zone::Exile,
        "fizzled flashback spell is still exiled"
    );
    assert!(fizzle.text_color_changes_v1.is_none());
}

#[test]
fn alter_reality_unaudited_target_halts_and_face_down_target_accepted() {
    // 1) Target outside the 305-card Premodern audit halts with
    // UnsupportedMechanic::TextColorChangeOutsideAuditV1 (source == alter).
    let mut state = islands_for(P0);
    let cop_blue = put(
        &mut state,
        P1,
        "Circle of Protection: Blue",
        Zone::Battlefield,
    );
    let alter = put(&mut state, P0, ALTER_REALITY, Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast_with_targets(&mut state, alter, &[Target::Object(cop_blue)]);
    resolve_alter_reality(&mut state, P0, alter, ManaColor::U, ManaColor::R);
    let decision = engine::advance_until_decision(&mut state);
    assert_eq!(
        decision,
        Decision::Halted {
            mechanic: UnsupportedMechanic::TextColorChangeOutsideAuditV1,
            source: alter,
        }
    );
    assert_eq!(
        engine::advance_until_decision(&mut state),
        Decision::Halted {
            mechanic: UnsupportedMechanic::TextColorChangeOutsideAuditV1,
            source: alter,
        }
    );

    // 2) Face-down morph target is accepted without leaking its identity, and
    // keeps the recorded change when turned face up (CR 708.8).
    let mut morph_state = islands_for(P0);
    enable_public_shroud_v1(&mut morph_state);
    let angel = put(&mut morph_state, P0, "Exalted Angel", Zone::Hand);
    pool(&mut morph_state, P0, &[], 3);
    assert!(castable(&mut morph_state, angel));
    act(&mut morph_state, Action::CastSpell(angel));
    match next(&mut morph_state) {
        Decision::ChooseCastMode { options, .. } => {
            assert!(options.contains(&CastMode::Alternative));
            act(
                &mut morph_state,
                Action::ChooseCastMode(CastMode::Alternative),
            );
        }
        other => panic!("expected morph cast mode choice, got {other:?}"),
    }
    settled(&mut morph_state);
    assert!(morph_state.objects.get(angel).v4.face_down_v1.is_some());

    let _alter =
        cast_and_resolve_alter_reality(&mut morph_state, P0, angel, ManaColor::B, ManaColor::G);
    settled(&mut morph_state);

    // Opponent's Schema 5 projection sees the neutral sentinel, never Exalted Angel.
    let opp_obs = observe_policy_v5(&morph_state, &PolicySurfaceV5::new(), P1, 0, 0, 0, 1).unwrap();
    let changes = opp_obs.projection.text_color_changes.unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].object.arena_id, angel.0);
    assert_eq!(changes[0].object.card_db_id, id("Face-down card"));
    assert_ne!(changes[0].object.card_db_id, id("Exalted Angel"));
    assert_eq!(
        (changes[0].from, changes[0].to),
        (ManaColor::B, ManaColor::G)
    );

    // Turn face up for {2}{W}{W}: audited (no halt) and change stays recorded.
    pool(&mut morph_state, P0, &[ManaColor::W, ManaColor::W], 2);
    act(&mut morph_state, Action::ActivateAbility(angel, 255));
    settled(&mut morph_state);
    assert!(morph_state.objects.get(angel).v4.face_down_v1.is_none());
    assert_eq!(
        text_color_change_v1::live_changes_for_object(&morph_state, angel),
        vec![(ManaColor::B, ManaColor::G)]
    );

    // 3) No-word card records the change cleanly with no effect.
    let mut no_word = islands_for(P0);
    let island = put(&mut no_word, P0, "Island", Zone::Battlefield);
    let _alter =
        cast_and_resolve_alter_reality(&mut no_word, P0, island, ManaColor::B, ManaColor::W);
    settled(&mut no_word);
    assert_eq!(
        text_color_change_v1::live_changes_for_object(&no_word, island),
        vec![(ManaColor::B, ManaColor::W)]
    );
}

/// Design §4.7 & §5.2 ruled interaction (CR 613.8): Alter Reality applies after
/// Volrath's Shapeshifter's Layer-3 full-text effect and acts on whichever
/// donor's text is currently active, in both seats.
#[test]
fn volrath_shapeshifter_and_alter_reality_cr_613_8_both_seats() {
    for seat in [P0, P1] {
        let opp = seat.opponent();
        let mut state = ordered_islands_for(seat);

        // Opponent has a green source (Wild Mongrel) and a black source
        // (Hypnotic Specter) to verify protection damage prevention.
        let mongrel = put(&mut state, opp, "Wild Mongrel", Zone::Battlefield);
        let specter = put(&mut state, opp, "Hypnotic Specter", Zone::Battlefield);

        // Cast Volrath's Shapeshifter with Akroma, Angel of Wrath on top of
        // controller's graveyard (protection from black and from red).
        let akroma = put(&mut state, seat, "Akroma, Angel of Wrath", Zone::Graveyard);
        let shifter = put(&mut state, seat, VOLRATH, Zone::Hand);
        pool(&mut state, seat, &[ManaColor::U, ManaColor::U], 1);
        cast_with_targets(&mut state, shifter, &[]);
        settled(&mut state);
        assert_eq!(zone(&state, shifter), Zone::Battlefield);

        // Opponent (Replenish) casts real Alter Reality changing "black" ->
        // "green" on the Shapeshifter (so Alter Reality goes to opponent's
        // graveyard and Akroma stays on top of seat's graveyard).
        act(&mut state, Action::Pass);
        let alter1 =
            cast_and_resolve_alter_reality(&mut state, opp, shifter, ManaColor::B, ManaColor::G);
        settled(&mut state);
        assert_eq!(zone(&state, alter1), Zone::Graveyard);
        assert_eq!(state.players[seat.index()].graveyard.last(), Some(&akroma));

        // Akroma's text now gives protection from green and from red (NOT black):
        // - green damage prevented; black damage NOT prevented
        // - green spell (Rancor) cannot target; red spell (Bolt) cannot target;
        //   black spell (Snuff Out) CAN target (Akroma is white, nonblack).
        {
            let mut dmg_probe = state.clone();
            hit(&mut dmg_probe, mongrel, Target::Object(shifter), 2);
            assert_eq!(
                dmg_probe.objects.get(shifter).damage,
                0,
                "seat={seat:?}: green damage must be prevented under Akroma (B->G)"
            );
            hit(&mut dmg_probe, specter, Target::Object(shifter), 2);
            assert_eq!(
                dmg_probe.objects.get(shifter).damage,
                2,
                "seat={seat:?}: black damage must NOT be prevented under Akroma (B->G)"
            );

            let mut tgt_probe = state.clone();
            tgt_probe.active_player = opp;
            tgt_probe.priority_player = opp;
            let rancor = put(&mut tgt_probe, opp, "Rancor", Zone::Hand);
            let bolt = put(&mut tgt_probe, opp, "Lightning Bolt", Zone::Hand);
            let snuff = put(&mut tgt_probe, opp, "Snuff Out", Zone::Hand);
            pool(
                &mut tgt_probe,
                opp,
                &[ManaColor::G, ManaColor::R, ManaColor::B],
                3,
            );
            assert!(
                !spell_targets(&tgt_probe, rancor).contains(&Target::Object(shifter)),
                "seat={seat:?}: green spell cannot target Akroma-Shapeshifter (B->G)"
            );
            assert!(
                !spell_targets(&tgt_probe, bolt).contains(&Target::Object(shifter)),
                "seat={seat:?}: red spell still cannot target Akroma-Shapeshifter"
            );
            assert!(
                spell_targets(&tgt_probe, snuff).contains(&Target::Object(shifter)),
                "seat={seat:?}: black spell can now target Akroma-Shapeshifter (B->G)"
            );
        }

        // Top becomes Karmic Guide (protection from black) -> protection from green.
        let _guide = put(&mut state, seat, "Karmic Guide", Zone::Graveyard);
        // Snapshot/restore in the middle preserves the change and protection.
        let mut state = restored(&state);
        assert_eq!(
            text_color_change_v1::live_changes_for_object(&state, shifter),
            vec![(ManaColor::B, ManaColor::G)]
        );
        {
            let mut dmg_probe = state.clone();
            hit(&mut dmg_probe, mongrel, Target::Object(shifter), 1);
            assert_eq!(
                dmg_probe.objects.get(shifter).damage,
                0,
                "seat={seat:?}: Karmic Guide donor has protection from green"
            );
            hit(&mut dmg_probe, specter, Target::Object(shifter), 1);
            assert_eq!(
                dmg_probe.objects.get(shifter).damage,
                1,
                "seat={seat:?}: Karmic Guide donor lost protection from black"
            );

            let mut tgt_probe = state.clone();
            tgt_probe.active_player = opp;
            tgt_probe.priority_player = opp;
            let rancor = put(&mut tgt_probe, opp, "Rancor", Zone::Hand);
            let snuff = put(&mut tgt_probe, opp, "Snuff Out", Zone::Hand);
            pool(&mut tgt_probe, opp, &[ManaColor::G, ManaColor::B], 3);
            assert!(!spell_targets(&tgt_probe, rancor).contains(&Target::Object(shifter)));
            assert!(spell_targets(&tgt_probe, snuff).contains(&Target::Object(shifter)));
        }

        // Top becomes Ihsan's Shade (protection from white) -> untouched by B->G.
        let _shade = put(&mut state, seat, "Ihsan's Shade", Zone::Graveyard);
        {
            let mut tgt_probe = state.clone();
            tgt_probe.active_player = opp;
            tgt_probe.priority_player = opp;
            let swords = put(&mut tgt_probe, opp, "Swords to Plowshares", Zone::Hand);
            let rancor = put(&mut tgt_probe, opp, "Rancor", Zone::Hand);
            pool(&mut tgt_probe, opp, &[ManaColor::W, ManaColor::G], 0);
            assert!(
                !spell_targets(&tgt_probe, swords).contains(&Target::Object(shifter)),
                "seat={seat:?}: Ihsan's Shade keeps protection from white"
            );
            assert!(
                spell_targets(&tgt_probe, rancor).contains(&Target::Object(shifter)),
                "seat={seat:?}: Ihsan's Shade has no protection from green"
            );
        }

        // Top becomes Iridescent Angel ("protection from each color", no color word)
        // -> still protection from all five colors (W, U, B, R, G).
        let _iridescent = put(&mut state, seat, "Iridescent Angel", Zone::Graveyard);
        {
            let mut tgt_probe = state.clone();
            tgt_probe.active_player = opp;
            tgt_probe.priority_player = opp;
            let swords = put(&mut tgt_probe, opp, "Swords to Plowshares", Zone::Hand);
            let chain = put(&mut tgt_probe, opp, "Chain of Vapor", Zone::Hand);
            let snuff = put(&mut tgt_probe, opp, "Snuff Out", Zone::Hand);
            let bolt = put(&mut tgt_probe, opp, "Lightning Bolt", Zone::Hand);
            let rancor = put(&mut tgt_probe, opp, "Rancor", Zone::Hand);
            pool(
                &mut tgt_probe,
                opp,
                &[
                    ManaColor::W,
                    ManaColor::U,
                    ManaColor::B,
                    ManaColor::R,
                    ManaColor::G,
                ],
                3,
            );
            for spell in [swords, chain, snuff, bolt, rancor] {
                assert!(
                    !spell_targets(&tgt_probe, spell).contains(&Target::Object(shifter)),
                    "seat={seat:?}: Iridescent Angel keeps protection from all five colors"
                );
            }
        }

        // Top becomes a noncreature card -> no protection, change stays recorded.
        let _land_top = put(&mut state, seat, "Island", Zone::Graveyard);
        assert_eq!(
            text_color_change_v1::live_changes_for_object(&state, shifter),
            vec![(ManaColor::B, ManaColor::G)]
        );
        {
            let mut tgt_probe = state.clone();
            tgt_probe.active_player = opp;
            tgt_probe.priority_player = opp;
            let swords = put(&mut tgt_probe, opp, "Swords to Plowshares", Zone::Hand);
            let rancor = put(&mut tgt_probe, opp, "Rancor", Zone::Hand);
            pool(&mut tgt_probe, opp, &[ManaColor::W, ManaColor::G], 0);
            assert!(spell_targets(&tgt_probe, swords).contains(&Target::Object(shifter)));
            assert!(spell_targets(&tgt_probe, rancor).contains(&Target::Object(shifter)));
        }

        // Second Alter Reality (G -> U) in timestamp order (613.7): Karmic Guide
        // now gets protection from blue (not green).
        let guide2 = put(&mut state, seat, "Karmic Guide", Zone::Graveyard);
        act(&mut state, Action::Pass);
        let _alter2 =
            cast_and_resolve_alter_reality(&mut state, opp, shifter, ManaColor::G, ManaColor::U);
        settled(&mut state);
        assert_eq!(state.players[seat.index()].graveyard.last(), Some(&guide2));
        assert_eq!(
            text_color_change_v1::live_changes_for_object(&state, shifter),
            vec![(ManaColor::B, ManaColor::G), (ManaColor::G, ManaColor::U)]
        );
        {
            let mut tgt_probe = state.clone();
            tgt_probe.active_player = opp;
            tgt_probe.priority_player = opp;
            let chain = put(&mut tgt_probe, opp, "Chain of Vapor", Zone::Hand);
            let rancor = put(&mut tgt_probe, opp, "Rancor", Zone::Hand);
            pool(&mut tgt_probe, opp, &[ManaColor::U, ManaColor::G], 0);
            assert!(
                !spell_targets(&tgt_probe, chain).contains(&Target::Object(shifter)),
                "seat={seat:?}: Karmic Guide donor now has protection from blue (B->G->U)"
            );
            assert!(
                spell_targets(&tgt_probe, rancor).contains(&Target::Object(shifter)),
                "seat={seat:?}: Karmic Guide donor no longer has protection from green"
            );
        }

        // Shapeshifter leaving the battlefield ends the change.
        let swords = put(&mut state, seat, "Swords to Plowshares", Zone::Hand);
        pool(&mut state, seat, &[ManaColor::W], 0);
        cast_with_targets(&mut state, swords, &[Target::Object(shifter)]);
        settled(&mut state);
        assert_eq!(zone(&state, shifter), Zone::Exile);
        assert!(text_color_change_v1::live_changes_for_object(&state, shifter).is_empty());
        assert!(state.text_color_changes_v1.is_none());

        // CR 400.7a: Alter Reality on a Shapeshifter spell carries over to the
        // battlefield permanent and immediately acts on its donor's text.
        let mut spell_state = ordered_islands_for(seat);
        let akroma_spell = put(
            &mut spell_state,
            seat,
            "Akroma, Angel of Wrath",
            Zone::Graveyard,
        );
        let shifter_spell = put(&mut spell_state, seat, VOLRATH, Zone::Hand);
        pool(&mut spell_state, seat, &[ManaColor::U, ManaColor::U], 1);
        cast_with_targets(&mut spell_state, shifter_spell, &[]);
        assert_eq!(zone(&spell_state, shifter_spell), Zone::Stack);
        act(&mut spell_state, Action::Pass);
        let _alter_spell = cast_and_resolve_alter_reality(
            &mut spell_state,
            opp,
            shifter_spell,
            ManaColor::B,
            ManaColor::G,
        );
        settled(&mut spell_state);
        assert_eq!(zone(&spell_state, shifter_spell), Zone::Battlefield);
        assert_eq!(
            spell_state.players[seat.index()].graveyard.last(),
            Some(&akroma_spell)
        );
        assert_eq!(
            text_color_change_v1::live_changes_for_object(&spell_state, shifter_spell),
            vec![(ManaColor::B, ManaColor::G)]
        );
        spell_state.active_player = opp;
        spell_state.priority_player = opp;
        let rancor = put(&mut spell_state, opp, "Rancor", Zone::Hand);
        let snuff = put(&mut spell_state, opp, "Snuff Out", Zone::Hand);
        pool(&mut spell_state, opp, &[ManaColor::G, ManaColor::B], 3);
        assert!(!spell_targets(&spell_state, rancor).contains(&Target::Object(shifter_spell)));
        assert!(spell_targets(&spell_state, snuff).contains(&Target::Object(shifter_spell)));
    }
}

/// Design §5.2 Games: a blue-white Premodern-pool deck with four Alter
/// Realities in its mainboard plays a Premodern-pool deck of color-word cards
/// through Schema 5 sessions in both seats with ranked and seeded random
/// actions, reaching natural terminals, replaying identically, and observing
/// real `choose_text_color_word_v1` choices and `projection.text_color_changes`.
#[test]
fn p3i_alter_reality_schema_5_games_finish_and_replay_both_seats() {
    let blue_white = stress_deck(
        &[("Island", 12), ("Plains", 12)],
        [
            ALTER_REALITY,
            "Soltari Priest",
            "Soltari Monk",
            "Mother of Runes",
            "Swords to Plowshares",
            "Counterspell",
            "Impulse",
            "Absolute Law",
            "Blue Elemental Blast",
        ],
    );
    let color_word_opponent = stress_deck(
        &[("Mountain", 12), ("Swamp", 12)],
        [
            "Goblin Piledriver",
            "Knight of Stromgald",
            "Red Elemental Blast",
            "Pyroblast",
            "Snuff Out",
            "Plaguebearer",
            "Siege-Gang Commander",
            "Hypnotic Specter",
            "Lightning Bolt",
        ],
    );

    let mut from_choices = 0usize;
    let mut to_choices = 0usize;
    let mut observed_changes = 0usize;

    for (pair, base_seed) in [
        (
            [blue_white.clone(), color_word_opponent.clone()],
            0x5033_6900_u64,
        ),
        ([color_word_opponent, blue_white], 0x5033_6901_u64),
    ] {
        for (seed, policy) in [
            (base_seed, Policy::Ranked),
            (base_seed + 2, Policy::Ranked),
            (base_seed, Policy::Random(base_seed)),
            (base_seed, Policy::Random(base_seed + 16)),
        ] {
            let mut observe = |decision: &serde_json::Value, action: &serde_json::Value| {
                if let Some(changes) =
                    decision["observation"]["projection"]["text_color_changes"].as_array()
                {
                    assert!(
                        !changes.is_empty(),
                        "empty text_color_changes must be omitted"
                    );
                    observed_changes += 1;
                }
                let semantic = &action["semantic"];
                if semantic["action_kind"] == "choose_text_color_word_v1" {
                    match semantic["stage"].as_str() {
                        Some("from") => {
                            assert_eq!(semantic["option_count"], 5);
                            assert!(semantic["from"].is_null());
                            from_choices += 1;
                        }
                        Some("to") => {
                            assert_eq!(semantic["option_count"], 4);
                            assert!(semantic["from"].is_string());
                            assert_ne!(semantic["from"], semantic["color"]);
                            to_choices += 1;
                        }
                        other => panic!("unexpected choose_text_color_word_v1 stage: {other:?}"),
                    }
                }
            };
            let game = play_observed(pair.clone(), None, seed, policy, &mut observe);
            let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
            assert_eq!(game.transcript_sha256, replay.transcript_sha256);
            assert_eq!(game.winner, replay.winner);
        }
    }

    println!(
        "Alter Reality choices: from={from_choices}, to={to_choices}, observed_changes={observed_changes}"
    );
    assert!(
        from_choices > 0 && to_choices > 0,
        "Alter Reality color-word choices must be chosen from the Schema 5 action list: from={from_choices}, to={to_choices}"
    );
    assert_eq!(from_choices, to_choices);
    assert!(
        observed_changes > 0,
        "Schema 5 observation must expose non-empty text_color_changes after Alter Reality resolves"
    );
}
