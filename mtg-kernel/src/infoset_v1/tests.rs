//! Smoke and determinism checks of the export on engine games. The AC1 to
//! AC3 harnesses live in `harness`.

use super::*;
use crate::ids::PlayerId;
use crate::rl::PlayerSeatV1;
use crate::rl_session::{FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1};
use crate::runtime_decks::runtime_deck_by_id;
use crate::state::SplitMix64;

/// Writes `value` to `W1_DUMP_DIR/<name>.json` when that variable is set
/// (diagnostics for failing harness cases).
pub(crate) fn dump(name: &str, value: &impl serde::Serialize) {
    if let Ok(dir) = std::env::var("W1_DUMP_DIR") {
        let path = std::path::Path::new(&dir).join(format!("{name}.json"));
        let _ = std::fs::write(path, serde_json::to_vec(value).unwrap_or_default());
    }
}

/// Up to `limit` paths at which two values differ.
pub(crate) fn diffs(
    a: &serde_json::Value,
    b: &serde_json::Value,
    path: String,
    limit: usize,
) -> Vec<String> {
    use serde_json::Value;
    let mut out = Vec::new();
    match (a, b) {
        _ if a == b => {}
        (Value::Object(x), Value::Object(y)) => {
            let keys: std::collections::BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            for k in keys {
                if out.len() >= limit {
                    break;
                }
                match (x.get(k), y.get(k)) {
                    (Some(p), Some(q)) => {
                        out.extend(diffs(p, q, format!("{path}.{k}"), limit - out.len()))
                    }
                    (p, q) => out.push(format!("{path}.{k}: {p:?} vs {q:?}")),
                }
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                if out.len() >= limit {
                    break;
                }
                out.extend(diffs(p, q, format!("{path}[{i}]"), limit - out.len()));
            }
        }
        _ => out.push(
            format!("{path}: {} vs {}", a, b)
                .chars()
                .take(300)
                .collect(),
        ),
    }
    out
}

/// The path of the first difference between two values.
pub(crate) fn first_diff(a: &serde_json::Value, b: &serde_json::Value, path: String) -> String {
    use serde_json::Value;
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys()) {
                match (x.get(k), y.get(k)) {
                    (Some(p), Some(q)) if p == q => {}
                    (Some(p), Some(q)) => return first_diff(p, q, format!("{path}.{k}")),
                    (p, q) => return format!("{path}.{k}: {p:?} vs {q:?}"),
                }
            }
            path
        }
        (Value::Array(x), Value::Array(y)) => {
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                if p != q {
                    return first_diff(p, q, format!("{path}[{i}]"));
                }
            }
            format!("{path}: lengths {} vs {}", x.len(), y.len())
        }
        _ => format!("{path}: {a} vs {b}"),
    }
}

pub(crate) fn decklists(s: &FastActorSessionV1) -> [Vec<u16>; 2] {
    let ids = s.deck_ids_v1();
    [0, 1].map(|i| {
        runtime_deck_by_id(&ids[i])
            .expect("registered deck")
            .card_ids
            .to_vec()
    })
}

pub(crate) fn export_at(
    s: &FastActorSessionV1,
    viewer: PlayerId,
    options: ExportOptionsV1,
) -> Result<InfosetV1, ExportErrorV1> {
    let lists = decklists(s);
    let state = s.game_state();
    export_v1(
        &ExportInputV1 {
            state,
            journal: state.observation_journal.notes(),
            viewer,
            decklists: [&lists[0], &lists[1]],
            marks: Some(&s.infoset_marks_v1(viewer)),
        },
        options,
    )
}

fn seat(p: PlayerSeatV1) -> PlayerId {
    match p {
        PlayerSeatV1::P0 => PlayerId::P0,
        PlayerSeatV1::P1 => PlayerId::P1,
    }
}

#[test]
fn paid_ninjutsu_keeps_the_waiting_hand_source_pristine_for_opponent_infoset() {
    use crate::card_def::{card_id_by_name, CARD_DEFS};
    use crate::engine::{advance_until_decision, step, Action, CostKind, Decision};
    use crate::event::{propose_and_commit, ProposedEvent};
    use crate::mana::ManaColor;
    use crate::state::{GameState, Step, Zone};

    for actor in [PlayerId::P0, PlayerId::P1] {
        for (ninja_name, blue_mana) in [("Ninja of the Deep Hours", 2), ("Moon-Circuit Hacker", 1)]
        {
            let viewer = actor.opponent();
            let actor_cards = [ninja_name, "Faerie Miscreant", "Spellstutter Sprite"]
                .map(|name| card_id_by_name(name).unwrap());
            let viewer_cards = [card_id_by_name("Snap").unwrap()];
            let lists = [PlayerId::P0, PlayerId::P1].map(|player| {
                if player == actor {
                    actor_cards.as_slice()
                } else {
                    viewer_cards.as_slice()
                }
            });
            let mut state = GameState::new_from_libraries_with_starting_player_v1(
                lists[0],
                lists[1],
                |id| CARD_DEFS[usize::from(id)].name.to_string(),
                1,
                actor,
            );
            state.engine.spellbench_neutral_v1.choices = true;
            let ninja = state.players[actor.index()].library[0];
            let attacker = state.players[actor.index()].library[1];
            let other_attacker = state.players[actor.index()].library[2];
            let snap = state.players[viewer.index()].library[0];
            propose_and_commit(&mut state, ProposedEvent::draw(actor));
            propose_and_commit(&mut state, ProposedEvent::draw(viewer));
            for attacking in [attacker, other_attacker] {
                propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change_to_battlefield_tapped(attacking),
                );
            }
            state.step = Step::DeclareBlockers;
            state.engine.combat.attackers_declared = true;
            state.engine.combat.blockers_declared = true;
            state.engine.combat.attackers = vec![attacker, other_attacker];
            state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = blue_mana;
            state.players[viewer.index()].mana_pool[ManaColor::U.pool_index()] = 2;

            step(&mut state, Action::ActivateAbility(ninja, 0)).unwrap();
            assert!(matches!(
                advance_until_decision(&mut state),
                Decision::ChooseCostTargets {
                    player,
                    source,
                    cost_kind: CostKind::ReturnPermanentsToHand,
                    remaining: 1,
                    candidates,
                } if player == actor && source == ninja && candidates.contains(&attacker)
            ));
            step(&mut state, Action::ChooseCostTarget(attacker)).unwrap();
            assert!(matches!(
                advance_until_decision(&mut state),
                Decision::CastSpellOrPass { player, .. } if player == actor
            ));
            assert_eq!(state.objects.get(attacker).zone, Zone::Hand);
            assert_eq!(state.objects.get(ninja).zone, Zone::Hand);
            assert_eq!(
                state.players[actor.index()].mana_pool[ManaColor::U.pool_index()],
                0
            );
            let source = state
                .stack
                .last()
                .unwrap()
                .v4
                .ability_source_contract
                .unwrap();
            assert_eq!(source.source, ninja);
            assert_eq!(source.zone, Zone::Hand);
            assert_eq!(source.card_def, actor_cards[0]);

            step(&mut state, Action::Pass).unwrap();
            assert!(matches!(
                advance_until_decision(&mut state),
                Decision::CastSpellOrPass { player, castable_spells, .. }
                    if player == viewer && castable_spells.contains(&snap)
            ));
            let result = export_v1(
                &ExportInputV1 {
                    state: &state,
                    journal: state.observation_journal.notes(),
                    viewer,
                    decklists: lists,
                    marks: None,
                },
                ExportOptionsV1::default(),
            );
            // The hidden-hand reference can still safely refuse. It must not
            // turn this real response decision into a fatal engine fault.
            if let Err(ExportErrorV1::Fault { code, detail }) = result {
                panic!("{ninja_name} in seat {actor:?}: {code:?} {detail}");
            }
            assert!(state
                .objects
                .get(ninja)
                .v4
                .ability_uses_this_turn
                .is_empty());

            step(&mut state, Action::Pass).unwrap();
            assert!(matches!(
                advance_until_decision(&mut state),
                Decision::CastSpellOrPass { player, .. } if player == actor
            ));
            assert!(state.stack.is_empty());
            assert_eq!(state.objects.get(ninja).zone, Zone::Battlefield);
            assert!(state.objects.get(ninja).tapped);
            assert!(state.engine.combat.attackers.contains(&ninja));
        }
    }
}

/// One export mode under test: v1 with default options, v2, v3 or reveal v4.
type ExportModeV1 = fn(&ExportInputV1<'_>) -> Result<InfosetV1, ExportErrorV1>;

fn export_v1_default(input: &ExportInputV1<'_>) -> Result<InfosetV1, ExportErrorV1> {
    export_v1(input, ExportOptionsV1::default())
}

/// Every export mode, by name.
const EXPORT_MODES: [(&str, ExportModeV1); 4] = [
    ("v1", export_v1_default),
    ("v2", export_v2),
    ("v3", export_v3),
    ("reveal_v4", export_reveal_v4),
];

/// The export input of `viewer` at `state`, without decision marks.
fn token_input<'a>(
    state: &'a crate::state::GameState,
    viewer: PlayerId,
    lists: &'a [Vec<u16>; 2],
) -> ExportInputV1<'a> {
    ExportInputV1 {
        state,
        journal: state.observation_journal.notes(),
        viewer,
        decklists: [&lists[0], &lists[1]],
        marks: None,
    }
}

/// A two-player game whose `owner` holds Faerie Miscreant, Spellstutter
/// Sprite and Snap and whose other player holds Snap and two Islands, with
/// `starting` taking the first turn and the reveal journal on (reveal v4
/// reads it).
fn token_game(owner: PlayerId, starting: PlayerId) -> (crate::state::GameState, [Vec<u16>; 2]) {
    use crate::card_def::{card_id_by_name, CARD_DEFS};
    let ids = |names: [&str; 3]| -> Vec<u16> {
        names
            .iter()
            .map(|name| card_id_by_name(name).unwrap())
            .collect()
    };
    let mut lists = [
        ids(["Snap", "Island", "Island"]),
        ids(["Snap", "Island", "Island"]),
    ];
    lists[owner.index()] = ids(["Faerie Miscreant", "Spellstutter Sprite", "Snap"]);
    let mut state = crate::state::GameState::new_from_libraries_with_starting_player_v1(
        &lists[0],
        &lists[1],
        |id| CARD_DEFS[usize::from(id)].name.to_string(),
        1,
        starting,
    );
    state.reveal_meaning_v3.enable();
    (state, lists)
}

/// Creates a Human Soldier token for `owner` on the battlefield.
fn create_token(state: &mut crate::state::GameState, owner: PlayerId) -> crate::ids::ObjectId {
    use crate::card_def::card_id_by_name;
    use crate::event::{propose_and_commit, ProposedEvent};
    let token_def = card_id_by_name("Human Soldier Token").unwrap();
    propose_and_commit(state, ProposedEvent::create_token(token_def, owner));
    let token = *state.players[owner.index()].battlefield.last().unwrap();
    assert!(state.objects.get(token).v4.is_token);
    token
}

/// Issue 713: Snap returns an opposing token to its owner's hand and its land
/// untap then poses a decision before the next state-based sweep removes the
/// token. Returns the state at that decision, the decklists and the token.
fn bounced_token_state(
    actor: PlayerId,
) -> (crate::state::GameState, [Vec<u16>; 2], crate::ids::ObjectId) {
    use crate::engine::{advance_until_decision, step, Action, Decision};
    use crate::event::{propose_and_commit, ProposedEvent};
    use crate::mana::ManaColor;
    use crate::state::{Target, Zone};

    let owner = actor.opponent();
    let (mut state, lists) = token_game(owner, actor);
    let snap = state.players[actor.index()].library[0];
    let land = state.players[actor.index()].library[1];
    propose_and_commit(&mut state, ProposedEvent::draw(actor));
    propose_and_commit(&mut state, ProposedEvent::draw(owner));
    propose_and_commit(
        &mut state,
        ProposedEvent::zone_change_to_battlefield_tapped(land),
    );
    let token = create_token(&mut state, owner);
    assert!(state.objects.get(land).tapped);

    let first = advance_until_decision(&mut state);
    assert!(
        matches!(&first, Decision::CastSpellOrPass { player, .. } if *player == actor),
        "{first:?}"
    );
    // Snap is cast from the pool: the land stays tapped for its untap.
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 2;
    step(&mut state, Action::CastSpell(snap)).unwrap();
    let mut guard = 0;
    loop {
        guard += 1;
        assert!(guard < 12, "Snap never reached its land-untap choice");
        match advance_until_decision(&mut state) {
            Decision::ChooseTargets {
                player,
                legal_targets,
                ..
            } if player == actor => {
                assert!(legal_targets.contains(&Target::Object(token)));
                step(&mut state, Action::ChooseTarget(Target::Object(token))).unwrap();
            }
            Decision::ChooseEffectTargets { player, .. } if player == actor => break,
            Decision::CastSpellOrPass { .. } => step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected decision before the land untap: {other:?}"),
        }
    }
    // The token sits in its owner's hand; no sweep has removed it yet.
    assert_eq!(state.objects.get(token).zone, Zone::Hand);
    assert!(state.players[owner.index()].hand.contains(&token));
    (state, lists, token)
}

/// Exports the bounced-token decision of [`bounced_token_state`] under
/// `export` for both seats as the Snap player and both viewers, then the
/// next decision after the state-based sweep.
fn check_bounced_token(mode: &str, export: ExportModeV1) {
    use crate::engine::{advance_until_decision, step, Action, Decision};

    for actor in [PlayerId::P0, PlayerId::P1] {
        let owner = actor.opponent();
        let (mut state, lists, token) = bounced_token_state(actor);
        for viewer in [actor, owner] {
            let at = format!("{mode}, actor {actor:?}, viewer {viewer:?}");
            let exported = export(&token_input(&state, viewer, &lists))
                .unwrap_or_else(|error| panic!("{at}: {error:?}"));
            let hand = &exported.state.players[owner.index()].hand;
            assert_eq!(
                hand.len(),
                state.players[owner.index()].hand.len(),
                "{at}: the exported hand keeps its size"
            );
            let tokens_in_hand = hand
                .iter()
                .filter(|id| exported.state.objects.get(**id).v4.is_token)
                .count();
            assert_eq!(tokens_in_hand, 1, "{at}: the token is placed by handle");
            let placed = *hand
                .iter()
                .find(|id| exported.state.objects.get(**id).v4.is_token)
                .unwrap();
            assert!(
                matches!(exported.handles[placed.0 as usize], HandleV1::C { .. }),
                "{at}: the token has a tracked handle"
            );
            assert_eq!(
                exported.state.objects.get(placed).card_def,
                state.objects.get(token).card_def,
                "{at}"
            );
            if viewer != owner {
                // No hand knowledge row names the public token.
                for row in exported.state.known_hand_cards(viewer, owner) {
                    assert!(!exported.state.objects.get(row.object).v4.is_token, "{at}");
                }
            }
            for player in [PlayerId::P0, PlayerId::P1] {
                assert_eq!(
                    exported.state.players[player.index()].library.len(),
                    state.players[player.index()].library.len(),
                    "{at}"
                );
            }
        }

        // After the next state-based sweep the token is gone and the export
        // does not change shape for either seat.
        step(&mut state, Action::FinishEffectSelection).unwrap();
        assert!(matches!(
            advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        assert!(!state.players[owner.index()].hand.contains(&token));
        for viewer in [actor, owner] {
            let at = format!("{mode}, actor {actor:?}, viewer {viewer:?}, after the sweep");
            let exported = export(&token_input(&state, viewer, &lists))
                .unwrap_or_else(|error| panic!("{at}: {error:?}"));
            assert_eq!(
                exported.state.players[owner.index()].hand.len(),
                state.players[owner.index()].hand.len(),
                "{at}"
            );
            // The token is in no exported hand or library vector. (The
            // engine keeps `zone == Hand` on a ceased token, so a viewer's own
            // tracked token object may still carry that zone; the zone
            // vectors are what the export places.)
            for player in [PlayerId::P0, PlayerId::P1] {
                let placed = &exported.state.players[player.index()];
                for id in placed.hand.iter().chain(&placed.library) {
                    assert!(
                        !exported.state.objects.get(*id).v4.is_token,
                        "{at}: a token is still placed for {player:?}"
                    );
                }
            }
        }
    }
}

/// Issue 713: Snap returns an opposing token to its owner's hand and its land
/// untap then poses a decision before the next state-based sweep removes the
/// token. Both seats export that decision.
#[test]
fn bounced_token_in_hand_exports_for_both_seats() {
    check_bounced_token("v1", export_v1_default);
}

/// Issue 713 under the other export modes: they share the tracking and the
/// placement of v1, after v2's event-time visibility checks.
#[test]
fn bounced_token_in_hand_exports_in_every_later_mode() {
    for (mode, export) in &EXPORT_MODES[1..] {
        check_bounced_token(mode, *export);
    }
}

/// Issue 751: the state of [`bounced_token_state`] after the state-based
/// sweep has made the token cease to exist in its owner's hand, after which
/// the owner draws a card, cycles another through the graveyard back to the
/// hand (so the Snap player knows it there), puts that card back on top of
/// the library unidentified, scries 1 (which latches R8 for the Snap player:
/// an informative put-back was scried), shuffles the library (which retires
/// the put-back) and finally moves the last hand card to the graveyard. The
/// owner's hand is then empty. Returns the state and the decklists.
fn ceased_hand_token_then_an_empty_hand(
    actor: PlayerId,
) -> (crate::state::GameState, [Vec<u16>; 2]) {
    use crate::engine::{advance_until_decision, step, Action, Decision};
    use crate::event::{propose_and_commit, ProposedEvent};
    use crate::state::Zone;

    let owner = actor.opponent();
    let (mut state, lists, token) = bounced_token_state(actor);
    step(&mut state, Action::FinishEffectSelection).unwrap();
    assert!(matches!(
        advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(!state.players[owner.index()].hand.contains(&token));
    assert_eq!(state.objects.get(token).zone, Zone::Hand);

    propose_and_commit(&mut state, ProposedEvent::draw(owner));
    let hand = state.players[owner.index()].hand.clone();
    assert_eq!(hand.len(), 2);
    let (known, last) = (hand[0], hand[1]);
    propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(known, Zone::Graveyard),
    );
    propose_and_commit(&mut state, ProposedEvent::zone_change(known, Zone::Hand));
    propose_and_commit(&mut state, ProposedEvent::zone_change(known, Zone::Library));
    assert_eq!(state.players[owner.index()].library[0], known);
    let bound = crate::state::ObjectLinkV4 {
        object: known,
        zone_change_count: state.objects.get(known).zone_change_count,
    };
    state
        .apply_scry_result(owner, &[bound], &[known], &[])
        .unwrap();
    state.shuffle_library(owner).unwrap();
    propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(last, Zone::Graveyard),
    );
    assert!(state.players[owner.index()].hand.is_empty());
    (state, lists)
}

/// Issue 751: a token that ceased to exist in the opponent's hand leaves no
/// phantom card in the Snap player's walk. Once the opponent's hand is empty
/// the R8 latch (set by an informative put-back that was scried) is released,
/// so the harness mutant L12, which reads the latch, does not refuse R8. Every
/// released mode exports both views with the native (empty) hand.
#[test]
fn a_hand_token_that_ceased_releases_the_latch_once_the_hand_is_empty() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let owner = actor.opponent();
        let (state, lists) = ceased_hand_token_then_an_empty_hand(actor);
        let journal = state.observation_journal.notes();
        for (name, projection) in [
            ("v1", super::projection::project_v1(&state, journal, actor)),
            ("v2", super::projection::project_v2(&state, journal, actor)),
        ] {
            assert!(
                !projection.r8_latch,
                "projection {name}, actor {actor:?}: the latch is released"
            );
        }
        let at = format!("L12, actor {actor:?}, viewer {actor:?}");
        let options = ExportOptionsV1 {
            mutant: Some(MutantV1::L12),
        };
        if let Err(error) = export_v1(&token_input(&state, actor, &lists), options) {
            assert!(
                !matches!(error.code_name(), "R8" | "R8b"),
                "{at}: {error:?}"
            );
        }
        for (mode, export) in &EXPORT_MODES {
            for viewer in [actor, owner] {
                let at = format!("{mode}, actor {actor:?}, viewer {viewer:?}");
                let exported = export(&token_input(&state, viewer, &lists))
                    .unwrap_or_else(|error| panic!("{at}: {error:?}"));
                assert!(
                    exported.state.players[owner.index()].hand.is_empty(),
                    "{at}"
                );
            }
        }
    }
}

/// Issue 751: the fix changes only the walk's count of the opponent's hand,
/// which the released export modes do not read. This pins a digest of every
/// released mode's export (or refusal), for both viewers, at every decision
/// of a seeded game in which no token or spell copy ever enters a hand, and
/// at the end of the issue's own scenario in both seats. The digests were
/// recorded on the parent commit of the fix; a deliberate change to the
/// export elsewhere must record them again.
#[test]
fn released_exports_are_unchanged_by_the_hand_token_fix() {
    use crate::event::CommittedEvent;
    use crate::state::Zone;
    use sha2::Digest as _;

    fn fold(hasher: &mut sha2::Sha256, result: &Result<InfosetV1, ExportErrorV1>) {
        match result {
            Ok(x) => {
                hasher.update(b"ok");
                hasher.update((x.bytes.len() as u64).to_le_bytes());
                hasher.update(x.bytes.as_slice());
            }
            Err(e) => {
                let text = format!("{e:?}");
                hasher.update(b"err");
                hasher.update((text.len() as u64).to_le_bytes());
                hasher.update(text.as_bytes());
            }
        }
    }

    let mut game = sha2::Sha256::new();
    let mut decisions = 0;
    play(["CawGates", "Faeries"], 3, 300, |s, _| {
        let state = s.game_state();
        for event in &state.engine.event_history {
            if let CommittedEvent::ZoneChange {
                object,
                to: Zone::Hand,
                ..
            } = event
            {
                let o = state.objects.get(*object);
                assert!(
                    !o.v4.is_token && o.spell_copy_origin.is_none(),
                    "a token or spell copy entered a hand: pick another seed"
                );
            }
        }
        let lists = decklists(s);
        for viewer in [PlayerId::P0, PlayerId::P1] {
            let marks = s.infoset_marks_v1(viewer);
            let input = ExportInputV1 {
                state,
                journal: state.observation_journal.notes(),
                viewer,
                decklists: [&lists[0], &lists[1]],
                marks: Some(&marks),
            };
            for (_, export) in &EXPORT_MODES {
                fold(&mut game, &export(&input));
            }
        }
        decisions += 1;
    });
    let game = format!("{:x}", game.finalize());

    let mut scenario = sha2::Sha256::new();
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, lists) = ceased_hand_token_then_an_empty_hand(actor);
        for viewer in [actor, actor.opponent()] {
            for (_, export) in &EXPORT_MODES {
                fold(&mut scenario, &export(&token_input(&state, viewer, &lists)));
            }
        }
    }
    let scenario = format!("{:x}", scenario.finalize());

    assert!(decisions > 20, "{decisions} decisions");
    assert_eq!(
        (game.as_str(), scenario.as_str()),
        (RELEASED_GAME_DIGEST_751, RELEASED_SCENARIO_DIGEST_751),
        "released export digests ({decisions} decisions)"
    );
}

/// See [`released_exports_are_unchanged_by_the_hand_token_fix`].
const RELEASED_GAME_DIGEST_751: &str =
    "5937bf54620f8f8fab8eb3c9adc409dec1973e0fb7703495a41ba26d401f2d11";
const RELEASED_SCENARIO_DIGEST_751: &str =
    "0df1e20f3da03335e7a2cf95041155481805aec83a05402f17ff4aafd83328b2";

/// A token of `owner` put on top of `owner`'s library, where no state-based
/// sweep has removed it yet. Returns the state, the decklists and the token.
fn library_token_state(
    owner: PlayerId,
) -> (crate::state::GameState, [Vec<u16>; 2], crate::ids::ObjectId) {
    use crate::event::{propose_and_commit, ProposedEvent};
    use crate::state::Zone;

    let (mut state, lists) = token_game(owner, owner);
    let token = create_token(&mut state, owner);
    propose_and_commit(&mut state, ProposedEvent::zone_change(token, Zone::Library));
    assert!(state.players[owner.index()].library.contains(&token));
    assert_eq!(state.objects.get(token).zone, Zone::Library);
    (state, lists, token)
}

/// The exported library of `owner` and the position in it of the one token,
/// after checking that it is the only token there and has a tracked handle.
fn library_token_position(exported: &InfosetV1, owner: PlayerId, at: &str) -> usize {
    let library = &exported.state.players[owner.index()].library;
    let positions: Vec<usize> = (0..library.len())
        .filter(|&i| exported.state.objects.get(library[i]).v4.is_token)
        .collect();
    assert_eq!(positions.len(), 1, "{at}: one token in the library");
    assert!(
        matches!(
            exported.handles[library[positions[0]].0 as usize],
            HandleV1::C { .. }
        ),
        "{at}: the token has a tracked handle"
    );
    positions[0]
}

/// Issue 713, library side: a token that sits in a library list is placed by
/// handle in both seats' exports, at its position (V saw where it went), and
/// takes no hidden slot.
fn check_library_token(mode: &str, export: ExportModeV1) {
    for owner in [PlayerId::P0, PlayerId::P1] {
        let (state, lists, token) = library_token_state(owner);
        let position = state.players[owner.index()]
            .library
            .iter()
            .position(|id| *id == token)
            .unwrap();
        for viewer in [owner, owner.opponent()] {
            let at = format!("{mode}, owner {owner:?}, viewer {viewer:?}");
            let exported = export(&token_input(&state, viewer, &lists))
                .unwrap_or_else(|error| panic!("{at}: {error:?}"));
            assert_eq!(
                exported.state.players[owner.index()].library.len(),
                state.players[owner.index()].library.len(),
                "{at}"
            );
            assert_eq!(
                library_token_position(&exported, owner, &at),
                position,
                "{at}: the token keeps its known position"
            );
        }
    }
}

#[test]
fn token_in_a_library_exports_for_both_seats() {
    check_library_token("v1", export_v1_default);
}

#[test]
fn token_in_a_library_exports_in_every_later_mode() {
    for (mode, export) in &EXPORT_MODES[1..] {
        check_library_token(mode, *export);
    }
}

/// Issue 713, library position: the token's library is shuffled before the
/// sweep, so neither viewer can know where the shuffle put it. Every mode
/// places it at the top-most position the viewer cannot rule out, whatever
/// its true position, and two worlds that differ only in that position
/// export the same bytes. A card then put on top of the library, at a public
/// position, moves the token down one. No engine path is known to shuffle a
/// library that holds a token, so the test builds the state directly.
#[test]
fn shuffled_library_token_exports_no_true_position() {
    use crate::event::{propose_and_commit, ProposedEvent};
    use crate::state::Zone;

    for owner in [PlayerId::P0, PlayerId::P1] {
        let (mut state, lists, token) = library_token_state(owner);
        // A real card leaves the library in public view, to return later.
        let card = *state.players[owner.index()]
            .library
            .iter()
            .find(|id| **id != token)
            .unwrap();
        propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(card, Zone::Graveyard),
        );
        state.shuffle_library(owner).unwrap();
        let position = |s: &crate::state::GameState| {
            s.players[owner.index()]
                .library
                .iter()
                .position(|id| *id == token)
                .unwrap()
        };
        // The other world: the shuffle put the token elsewhere.
        let mut other = state.clone();
        let true_position = position(&other);
        let swap_with = if true_position == 0 { 1 } else { 0 };
        other.players[owner.index()]
            .library
            .swap(true_position, swap_with);
        assert_ne!(position(&state), position(&other));

        let check = |worlds: [&crate::state::GameState; 2], expected: usize, phase: &str| {
            for (mode, export) in &EXPORT_MODES {
                for viewer in [owner, owner.opponent()] {
                    let at = format!("{mode}, owner {owner:?}, viewer {viewer:?}, {phase}");
                    let [a, b] = worlds.map(|world| {
                        export(&token_input(world, viewer, &lists))
                            .unwrap_or_else(|error| panic!("{at}: {error:?}"))
                    });
                    assert_eq!(library_token_position(&a, owner, &at), expected, "{at}");
                    assert_eq!(library_token_position(&b, owner, &at), expected, "{at}");
                    assert_eq!(
                        a.bytes, b.bytes,
                        "{at}: the export does not show where the shuffle put the token"
                    );
                }
            }
        };
        check([&state, &other], 0, "after the shuffle");

        for world in [&mut state, &mut other] {
            propose_and_commit(world, ProposedEvent::zone_change(card, Zone::Library));
            assert_eq!(world.players[owner.index()].library[0], card);
        }
        assert_ne!(position(&state), position(&other));
        check([&state, &other], 1, "after a card is put on top");
    }
}

/// Creates a token for `owner`, puts it on top of `owner`'s library and runs
/// the state-based sweep, which makes it cease to exist there: the engine
/// drops it from the library list with a removal note and no event, and
/// leaves its zone as it was. Returns the token.
fn ceased_library_token(
    state: &mut crate::state::GameState,
    owner: PlayerId,
) -> crate::ids::ObjectId {
    use crate::event::{propose_and_commit, ProposedEvent};
    use crate::state::Zone;

    let token = create_token(state, owner);
    propose_and_commit(state, ProposedEvent::zone_change(token, Zone::Library));
    assert!(state.players[owner.index()].library.contains(&token));
    crate::trigger::sba_fixed_point(state);
    assert!(!state.players[owner.index()].library.contains(&token));
    assert_eq!(state.objects.get(token).zone, Zone::Library);
    token
}

/// `owner` scries 1 and puts the top card of their library on the bottom.
/// Returns that card.
fn scry_top_to_bottom(
    state: &mut crate::state::GameState,
    owner: PlayerId,
) -> crate::ids::ObjectId {
    let top = state.players[owner.index()].library[0];
    let bound = crate::state::ObjectLinkV4 {
        object: top,
        zone_change_count: state.objects.get(top).zone_change_count,
    };
    state
        .apply_scry_result(owner, &[bound], &[], &[top])
        .unwrap();
    top
}

/// Issue 744: a token put into its owner's library ceases to exist at the
/// next state-based sweep. The projection stops counting it there, so when
/// the owner then scries a card to the bottom, the owner's library fact sits
/// at the true last position, and every mode exports both views with no
/// token placed.
#[test]
fn ceased_library_token_leaves_the_library_count() {
    for owner in [PlayerId::P0, PlayerId::P1] {
        let (mut state, lists) = token_game(owner, owner);
        ceased_library_token(&mut state, owner);
        let scried = scry_top_to_bottom(&mut state, owner);
        check_scried_to_the_bottom(&state, &lists, owner, scried, "a scry after the cease");
    }
}

/// Exports `state` under every mode for both viewers and checks that no
/// export refuses, that `owner`'s exported library has its native length and
/// no token, and that the owner knows a card at the last position, where
/// `scried` (a card the owner scried to the bottom) natively sits.
fn check_scried_to_the_bottom(
    state: &crate::state::GameState,
    lists: &[Vec<u16>; 2],
    owner: PlayerId,
    scried: crate::ids::ObjectId,
    phase: &str,
) {
    let length = state.players[owner.index()].library.len();
    assert_eq!(state.players[owner.index()].library[length - 1], scried);
    for (mode, export) in &EXPORT_MODES {
        for viewer in [owner, owner.opponent()] {
            let at = format!("{mode}, owner {owner:?}, viewer {viewer:?}, {phase}");
            let exported = export(&token_input(state, viewer, lists))
                .unwrap_or_else(|error| panic!("{at}: {error:?}"));
            let library = &exported.state.players[owner.index()].library;
            assert_eq!(library.len(), length, "{at}");
            assert!(
                library
                    .iter()
                    .all(|id| !exported.state.objects.get(*id).v4.is_token),
                "{at}: no token is placed"
            );
            if viewer == owner {
                let rows = exported.state.known_library_cards(owner, owner);
                assert!(
                    rows.iter().any(|row| row.position as usize == length - 1),
                    "{at}: the scried card is known at the bottom"
                );
            }
        }
    }
}

/// Issue 744: an ordinary removal from a library (a card leaves it with its
/// own zone change) while a token sits there is claimed by that event, so the
/// token stays counted until it ceases. The owner then scries 2, keeping the
/// token on top and putting the other card on the bottom, and the token
/// ceases: the owner's fact for the bottom card must sit at the true last
/// position. If the ordinary removal took the token out of the count, the
/// scry would read a library one card short and the fact would be one
/// position too high.
#[test]
fn an_ordinary_library_removal_keeps_the_token_counted_until_it_ceases() {
    use crate::event::{propose_and_commit, ProposedEvent};
    use crate::state::Zone;

    for owner in [PlayerId::P0, PlayerId::P1] {
        let (mut state, lists) = token_game(owner, owner);
        let token = create_token(&mut state, owner);
        propose_and_commit(&mut state, ProposedEvent::zone_change(token, Zone::Library));
        assert_eq!(state.players[owner.index()].library[0], token);
        let card = state.players[owner.index()].library[2];
        propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(card, Zone::Graveyard),
        );
        let second = state.players[owner.index()].library[1];
        let bound = |id: crate::ids::ObjectId| crate::state::ObjectLinkV4 {
            object: id,
            zone_change_count: state.objects.get(id).zone_change_count,
        };
        let prefix = [bound(token), bound(second)];
        state
            .apply_scry_result(owner, &prefix, &[token], &[second])
            .unwrap();
        crate::trigger::sba_fixed_point(&mut state);
        assert!(!state.players[owner.index()].library.contains(&token));
        let phase = "an ordinary removal, a scry of 2, then the cease";
        check_scried_to_the_bottom(&state, &lists, owner, second, phase);
    }
}

/// A token of `owner` in `owner`'s library after a real card of that library
/// has gone to the graveyard and the library was shuffled, with no sweep
/// since. When `ceased` is set, another token of `owner` has first ceased to
/// exist in that library. Returns the state, the decklists and the token.
fn shuffled_library_token_state(
    owner: PlayerId,
    ceased: bool,
) -> (crate::state::GameState, [Vec<u16>; 2], crate::ids::ObjectId) {
    use crate::event::{propose_and_commit, ProposedEvent};
    use crate::state::Zone;

    let (mut state, lists) = token_game(owner, owner);
    if ceased {
        ceased_library_token(&mut state, owner);
    }
    let token = create_token(&mut state, owner);
    propose_and_commit(&mut state, ProposedEvent::zone_change(token, Zone::Library));
    let card = *state.players[owner.index()]
        .library
        .iter()
        .find(|id| **id != token)
        .unwrap();
    propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(card, Zone::Graveyard),
    );
    state.shuffle_library(owner).unwrap();
    assert_eq!(state.players[owner.index()].library.len(), 3);
    (state, lists, token)
}

/// Exports `worlds` for `viewer` under every mode, checks that the token of
/// `owner` sits at `expected[w]` in world `w` and, when `same_bytes` is set,
/// that the two exports are equal.
fn check_library_token_worlds(
    worlds: [&crate::state::GameState; 2],
    lists: &[Vec<u16>; 2],
    owner: PlayerId,
    viewer: PlayerId,
    expected: [usize; 2],
    same_bytes: bool,
    phase: &str,
) {
    for (mode, export) in &EXPORT_MODES {
        let at = format!("{mode}, owner {owner:?}, viewer {viewer:?}, {phase}");
        let [a, b] = worlds.map(|world| {
            export(&token_input(world, viewer, lists))
                .unwrap_or_else(|error| panic!("{at}: {error:?}"))
        });
        assert_eq!(library_token_position(&a, owner, &at), expected[0], "{at}");
        assert_eq!(library_token_position(&b, owner, &at), expected[1], "{at}");
        if same_bytes {
            assert_eq!(
                a.bytes, b.bytes,
                "{at}: the export shows which world the token is in"
            );
        }
    }
}

/// Issue 744, owed by the review of issue 713 (finding 4 a): after a shuffle
/// of a library that holds a token, its owner looks at the top card and it is
/// not the token. In two worlds that differ only in where the shuffle put the
/// token below the top, the owner's view places it at position 1 and the
/// opponent's view at position 0, with equal bytes for each viewer.
#[test]
fn a_look_at_the_top_after_a_shuffle_rules_out_only_the_top() {
    for owner in [PlayerId::P0, PlayerId::P1] {
        let (mut state, lists, token) = shuffled_library_token_state(owner, false);
        let position = |s: &crate::state::GameState| {
            s.players[owner.index()]
                .library
                .iter()
                .position(|id| *id == token)
                .unwrap()
        };
        if position(&state) == 0 {
            state.players[owner.index()].library.swap(0, 1);
        }
        let mut other = state.clone();
        let p = position(&other);
        other.players[owner.index()]
            .library
            .swap(p, if p == 1 { 2 } else { 1 });
        for world in [&mut state, &mut other] {
            world.reveal_library_top(owner, owner, 1);
        }
        assert_ne!(position(&state), position(&other));
        let worlds = [&state, &other];
        let phase = "after a look at the top";
        check_library_token_worlds(worlds, &lists, owner, owner, [1, 1], true, phase);
        let opponent = owner.opponent();
        check_library_token_worlds(worlds, &lists, owner, opponent, [0, 0], true, phase);
    }
}

/// Issue 744, owed by the review of issue 713 (finding 4 b): after a shuffle
/// of a library that holds a token, its owner scries 1 to the bottom. In one
/// world the token was on top and goes to the bottom; in the other it was
/// second and another card goes. The owner sees the token at the last
/// position in the first world and on top in the second; the opponent cannot
/// tell the worlds apart and gets position 0 in both, with equal bytes. With
/// a token that ceased to exist in that library before, the length the
/// projection reads must not count it (issue 744), or the bottom position is
/// one too deep.
#[test]
fn a_scry_to_the_bottom_after_a_shuffle_shows_the_token_only_to_its_owner() {
    for ceased in [false, true] {
        for owner in [PlayerId::P0, PlayerId::P1] {
            let (mut state, lists, token) = shuffled_library_token_state(owner, ceased);
            let position = |s: &crate::state::GameState| {
                s.players[owner.index()]
                    .library
                    .iter()
                    .position(|id| *id == token)
                    .unwrap()
            };
            let p = position(&state);
            state.players[owner.index()].library.swap(p, 0);
            let mut other = state.clone();
            other.players[owner.index()].library.swap(0, 1);
            for world in [&mut state, &mut other] {
                scry_top_to_bottom(world, owner);
            }
            assert_eq!(position(&state), 2);
            assert_eq!(position(&other), 0);
            let worlds = [&state, &other];
            let phase = format!("after a scry to the bottom, ceased token {ceased}");
            check_library_token_worlds(worlds, &lists, owner, owner, [2, 0], false, &phase);
            let opponent = owner.opponent();
            check_library_token_worlds(worlds, &lists, owner, opponent, [0, 0], true, &phase);
        }
    }
}

/// Issue 713, reveal v4: the reveal minima name hidden hand cards, so they
/// are bounded by the hidden hand. A token in the hand is public and takes
/// no hidden slot, so it does not count toward the bound.
#[test]
fn reveal_v4_minima_are_bounded_by_the_hidden_hand_beside_a_token() {
    use crate::event::{propose_and_commit, ProposedEvent};
    use crate::state::Zone;

    for owner in [PlayerId::P0, PlayerId::P1] {
        let viewer = owner.opponent();
        let world = |draws: usize, token: bool| {
            let (mut state, lists) = token_game(owner, owner);
            for _ in 0..draws {
                propose_and_commit(&mut state, ProposedEvent::draw(owner));
            }
            for k in 0..draws {
                let shown = state.players[owner.index()].hand[k];
                state.reveal_hand_card(viewer, owner, shown).unwrap();
            }
            if token {
                let token = create_token(&mut state, owner);
                propose_and_commit(&mut state, ProposedEvent::zone_change(token, Zone::Hand));
                assert!(state.players[owner.index()].hand.contains(&token));
            }
            (state, lists)
        };
        // Two revealed hidden cards beside the token: the minima fill the
        // hidden hand exactly and the export names both.
        let (state, lists) = world(2, true);
        let at = format!("owner {owner:?}");
        let exported = export_reveal_v4(&token_input(&state, viewer, &lists))
            .unwrap_or_else(|error| panic!("{at}: {error:?}"));
        assert_eq!(exported.state.players[owner.index()].hand.len(), 3, "{at}");
        assert_eq!(
            exported.state.known_hand_cards(viewer, owner).len(),
            2,
            "{at}"
        );
        // Minima of three names (another transcript) against two hidden
        // cards and the token: the bound refuses as soon as it reads them.
        let (named, _) = world(3, false);
        let result = export_reveal_v4_from_journal(
            &token_input(&state, viewer, &lists),
            &named.reveal_meaning_v3,
        );
        assert_eq!(
            result.err(),
            Some(ExportErrorV1::Refused {
                code: RefusalCodeV1::R8,
                detail: "reveal minima exceed the public hand size".to_string(),
            }),
            "{at}"
        );
    }
}

/// Plays a seeded uniform-random game and calls `at` at every decision with
/// at least two candidates.
pub(crate) fn play(
    decks: [&str; 2],
    seed: u64,
    max_decisions: usize,
    mut at: impl FnMut(&FastActorSessionV1, PlayerId),
) {
    let mut s = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
        seed,
        seed,
        2_000,
        20_000,
        [decks[0].to_string(), decks[1].to_string()],
    )
    .expect("reset");
    let mut rng = SplitMix64::seed(seed ^ 0x9e37);
    for _ in 0..max_decisions {
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            return;
        };
        if d.legal_action_count >= 2 {
            at(&s, seat(d.acting_player));
        }
        let a = (rng.next_u64() % u64::from(d.legal_action_count)) as u32;
        s.step(d.episode_id, d.step, a).expect("legal step");
    }
}

#[test]
fn export_is_deterministic_and_imports_byte_for_byte() {
    let mut tally = RefusalTallyV1::default();
    for (decks, seed) in [
        (["CawGates", "Faeries"], 3u64),
        (["Wildfire", "Wildfire"], 5),
    ] {
        play(decks, seed, 400, |s, v| {
            let a = export_at(s, v, ExportOptionsV1::default());
            let b = export_at(s, v, ExportOptionsV1::default());
            match (&a, &b) {
                (Ok(a), Ok(b)) => {
                    assert_eq!(a.bytes, b.bytes, "export is a function of its input");
                    let imported = FastActorSessionV1::import_infoset_v1(s, a.state.clone(), 0);
                    let again: crate::state::GameState =
                        serde_json::from_slice(&serde_json::to_vec(imported.game_state()).unwrap())
                            .unwrap();
                    let (x, y) = (
                        serde_json::to_value(&again).unwrap(),
                        serde_json::to_value(&a.state).unwrap(),
                    );
                    assert!(
                        x == y,
                        "the imported state serializes to the exported bytes; first difference at {}",
                        first_diff(&x, &y, String::new())
                    );
                }
                (Err(x), Err(y)) => assert_eq!(x, y),
                _ => panic!("presence differs between two exports of one state"),
            }
            if let Err(e) = &a {
                let seen = tally.by_code.get(e.code_name()).copied().unwrap_or(0);
                if seen < 3 {
                    eprintln!("refusal {}: {e:?}", e.code_name());
                }
            }
            tally.record(&a.as_ref().map(|_| ()).map_err(Clone::clone));
        });
    }
    eprintln!(
        "infoset smoke tally: {}",
        serde_json::to_string(&tally).unwrap()
    );
    assert!(tally.exported > 0, "some decisions export");
}

/// Astra r1 P1: an unidentified put-back never names the card, even when V
/// had it revealed: two worlds that return a revealed card and an unknown
/// one in either order must give the same tombstones and the same export.
#[test]
fn unidentified_put_back_of_a_revealed_card_is_never_named() {
    use crate::event::{propose_and_commit, ProposedEvent};
    use crate::state::Zone;
    let live = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
        41,
        41,
        2_000,
        20_000,
        ["CawGates".to_string(), "Faeries".to_string()],
    )
    .expect("reset");
    let viewer = PlayerId::P0;
    let opp = viewer.opponent();
    let base = live.game_state().clone();
    let hand = base.players[opp.index()].hand.clone();
    assert!(hand.len() >= 2);
    let worlds: Vec<crate::state::GameState> = [(hand[0], hand[1]), (hand[1], hand[0])]
        .into_iter()
        .map(|(first, second)| {
            let mut st = base.clone();
            // V sees hand[0] revealed in both worlds; the opponent then
            // returns two cards unidentified, in either order.
            st.reveal_hand_card(viewer, opp, hand[0]).expect("reveal");
            propose_and_commit(&mut st, ProposedEvent::zone_change(first, Zone::Library));
            propose_and_commit(&mut st, ProposedEvent::zone_change(second, Zone::Library));
            st
        })
        .collect();
    let projections: Vec<_> = worlds
        .iter()
        .map(|st| super::projection::project_v1(st, st.observation_journal.notes(), viewer))
        .collect();
    let n = worlds[0].engine.event_history.len();
    for e in [n - 2, n - 1] {
        for p in &projections {
            assert!(
                p.event_observed[e].is_empty(),
                "an unidentified put-back is observed at event {e}"
            );
        }
    }
}

/// W-pin-1 shape on recorded S1 roots (Wildfire, Lembas shuffled into its
/// owner's library while its Food ability waits): the pin is placed as a
/// `U:` occupant with Lembas's definition on the canonical epoch path, and
/// the waiting contract maps to epoch 1 (design 6.5 case B or C).
#[test]
fn lembas_waiting_source_is_pinned_on_the_canonical_epoch_path() {
    use crate::runtime_decks::RUNTIME_DECKS;
    for fixture in [
        include_str!("../regret_census_v1/stage4a/world/fixtures/lembas_root_o2_v0_f2_s0.json"),
        include_str!("../regret_census_v1/stage4a/world/fixtures/lembas_root_o2_v0_f3_s0.json"),
        include_str!("../regret_census_v1/stage4a/world/fixtures/lembas_root_o2_v0_f5_s1.json"),
    ] {
        let record: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let state: crate::state::GameState =
            serde_json::from_value(record["state"].clone()).unwrap();
        let viewer = PlayerId(record["actor"].as_u64().unwrap() as u8);
        // The recorded games' decks, matched by their full card multisets.
        let lists: Vec<Vec<u16>> = [PlayerId::P0, PlayerId::P1]
            .iter()
            .map(|owner| {
                let mut cards: Vec<u16> = state
                    .objects
                    .iter()
                    .filter(|(_, o)| {
                        o.owner == *owner && !o.v4.is_token && o.spell_copy_origin.is_none()
                    })
                    .map(|(_, o)| o.card_def)
                    .collect();
                cards.sort_unstable();
                RUNTIME_DECKS
                    .iter()
                    .map(|d| {
                        let mut ids = d.card_ids.to_vec();
                        ids.sort_unstable();
                        ids
                    })
                    .find(|ids| *ids == cards)
                    .expect("a registered deck")
            })
            .collect();
        let export = export_v1(
            &ExportInputV1 {
                state: &state,
                journal: &[],
                viewer,
                decklists: [&lists[0], &lists[1]],
                marks: Some(&every_event_marks(&state, &[])),
            },
            ExportOptionsV1::default(),
        );
        let x = match export {
            Ok(x) => x,
            Err(e) => panic!("{}: {e:?}", record["case"]),
        };
        let lembas = crate::card_def::card_id_by_name("Lembas").unwrap();
        let pinned: Vec<(usize, &crate::state::GameObject)> = x
            .state
            .objects
            .iter()
            .filter(|(id, _)| matches!(x.handles[id.0 as usize], HandleV1::U { pinned: true, .. }))
            .map(|(id, o)| (id.0 as usize, o))
            .collect();
        assert_eq!(pinned.len(), 1, "{}", record["case"]);
        let (pin_id, pin) = pinned[0];
        assert_eq!(pin.card_def, lembas);
        let item = x
            .state
            .stack
            .iter()
            .find(|i| i.source.0 as usize == pin_id)
            .expect("the waiting ability names the pinned occupant");
        let contract = item.v4.ability_source_contract.expect("contract");
        assert_eq!(contract.zone_change_count, 1, "the contract epoch is p0");
        assert!(
            [3, 4].contains(&pin.zone_change_count),
            "{}: pinned generation {} is phi(e_last) + h",
            record["case"],
            pin.zone_change_count
        );
    }
}

/// Decision marks for a recorded state without its session (lead ruling 7
/// (a)): a V decision after every committed event, so each event is its own
/// batch. For fixture states that test predicates other than the batches;
/// without marks the export treats the whole stream as one batch.
pub(crate) fn every_event_marks(
    state: &crate::state::GameState,
    journal: &[crate::state::ObservationNoteV1],
) -> Vec<(usize, usize)> {
    (0..=state.engine.event_history.len())
        .map(|k| (k, journal.iter().filter(|n| n.history_len <= k).count()))
        .collect()
}

/// The recorded decks of a fixture state, matched by full card multisets.
fn fixture_lists(state: &crate::state::GameState) -> Vec<Vec<u16>> {
    use crate::runtime_decks::RUNTIME_DECKS;
    [PlayerId::P0, PlayerId::P1]
        .iter()
        .map(|owner| {
            let mut cards: Vec<u16> = state
                .objects
                .iter()
                .filter(|(_, o)| {
                    o.owner == *owner && !o.v4.is_token && o.spell_copy_origin.is_none()
                })
                .map(|(_, o)| o.card_def)
                .collect();
            cards.sort_unstable();
            RUNTIME_DECKS
                .iter()
                .map(|d| {
                    let mut ids = d.card_ids.to_vec();
                    ids.sort_unstable();
                    ids
                })
                .find(|ids| *ids == cards)
                .expect("a registered deck")
        })
        .collect()
}

/// Ruling 41's Wildfire join (mutant L13): Lembas A waits as a pinned Food
/// source in its owner's library; an unidentified draw and a public discard
/// of a Lembas follow. Whether the discarded Lembas is A or another copy B,
/// R1b refuses in both worlds (name-level, public events only); v6's
/// identity test (L13) refuses only when it is A.
#[test]
fn wildfire_join_refuses_in_both_worlds_and_l13_differs() {
    use crate::event::{propose_and_commit, ProposedEvent};
    use crate::state::Zone;
    let record: serde_json::Value = serde_json::from_str(include_str!(
        "../regret_census_v1/stage4a/world/fixtures/lembas_root_o2_v0_f2_s0.json"
    ))
    .unwrap();
    let base: crate::state::GameState = serde_json::from_value(record["state"].clone()).unwrap();
    let viewer = PlayerId(record["actor"].as_u64().unwrap() as u8);
    let lists = fixture_lists(&base);
    let lembas = crate::card_def::card_id_by_name("Lembas").unwrap();
    let pinned = super::export::contract_pins(&base);
    let a = *pinned
        .iter()
        .find(|id| base.objects.get(**id).card_def == lembas)
        .expect("a pinned Lembas");
    let owner = base.objects.get(a).owner;
    let Some(b) = base.players[owner.index()]
        .library
        .iter()
        .copied()
        .find(|id| *id != a && base.objects.get(*id).card_def == lembas)
    else {
        eprintln!("fixture has no second hidden Lembas; join fixture vacuous here");
        return;
    };
    let run = |which: crate::ids::ObjectId, mutant: Option<MutantV1>| {
        let mut st = base.clone();
        propose_and_commit(&mut st, ProposedEvent::zone_change(which, Zone::Hand));
        propose_and_commit(&mut st, ProposedEvent::zone_change(which, Zone::Graveyard));
        export_v1(
            &ExportInputV1 {
                state: &st,
                journal: st.observation_journal.notes(),
                viewer,
                decklists: [&lists[0], &lists[1]],
                marks: Some(&every_event_marks(&st, st.observation_journal.notes())),
            },
            ExportOptionsV1 { mutant },
        )
        .map(|x| x.bytes)
        .map_err(|e| e.code_name().to_string())
    };
    assert_eq!(run(a, None), Err("R1b".to_string()));
    assert_eq!(run(b, None), Err("R1b".to_string()));
    let (la, lb) = (run(a, Some(MutantV1::L13)), run(b, Some(MutantV1::L13)));
    assert_ne!(
        la.is_ok(),
        lb.is_ok(),
        "L13 differs in presence: {la:?} vs {lb:?}"
    );
}

/// The public outcome a continuation reaches: life, zone sizes and public
/// zone contents by definition.
fn public_summary(s: &FastActorSessionV1) -> serde_json::Value {
    let st = s.game_state();
    let defs = |ids: &[crate::ids::ObjectId]| {
        let mut d: Vec<u16> = ids.iter().map(|id| st.objects.get(*id).card_def).collect();
        d.sort_unstable();
        d
    };
    serde_json::json!([0, 1].map(|p| {
        let pl = &st.players[p];
        serde_json::json!({"life": pl.life, "hand": pl.hand.len(), "library": pl.library.len(),
            "battlefield": defs(&pl.battlefield), "graveyard": defs(&pl.graveyard)})
    }))
}

/// Events committed since `from`, up to the next turn, with every object
/// named by its definition.
pub(crate) fn public_events(s: &FastActorSessionV1, from: usize) -> serde_json::Value {
    let st = s.game_state();
    let events: Vec<_> = st.engine.event_history[from.min(st.engine.event_history.len())..]
        .iter()
        .take_while(|e| !matches!(e, crate::event::CommittedEvent::UpkeepBegan { .. }))
        .cloned()
        .collect();
    let mut v = super::rewrite::tagged_value(&events);
    fn name(v: &mut serde_json::Value, st: &crate::state::GameState) {
        match v {
            serde_json::Value::String(_) => {
                if let Some(id) = super::rewrite::parse_tag(v) {
                    let o = st.objects.get(id);
                    *v = if super::projection::is_public(o.zone) {
                        serde_json::json!({ "def": o.card_def })
                    } else {
                        serde_json::json!("hidden")
                    };
                }
            }
            serde_json::Value::Array(xs) => xs.iter_mut().for_each(|x| name(x, st)),
            serde_json::Value::Object(m) => {
                m.retain(|k, _| k != "zone_change_count" && !k.ends_with("_zone_change_count"));
                m.values_mut().for_each(|x| name(x, st));
            }
            _ => {}
        }
    }
    name(&mut v, st);
    v
}

/// Passes priority until the stack is shorter than `below` (the waiting
/// item resolved) or `limit` decisions pass; other decisions take index 0.
pub(crate) fn resolve_waiting(s: &mut FastActorSessionV1, below: usize, limit: usize) {
    for _ in 0..limit {
        if s.game_state().stack.len() < below {
            return;
        }
        let FastActorResponseV1::Decision(d) = s.current_response() else {
            return;
        };
        let menu = s.diagnostic_current_action_semantics().unwrap_or_default();
        let pass = menu
            .iter()
            .position(|m| matches!(m, crate::rl::ActionSemanticV1::Pass { .. }))
            .unwrap_or(0);
        s.step(d.episode_id, d.step, pass as u32).expect("step");
    }
}

/// AC3-L (design 6.5): from the recorded Lembas roots, the waiting Food
/// ability resolves to the same public outcome in the export and in the
/// true state.
#[test]
fn ac3_l_lembas_continuations_match_the_true_state() {
    for fixture in [
        include_str!("../regret_census_v1/stage4a/world/fixtures/lembas_root_o2_v0_f2_s0.json"),
        include_str!("../regret_census_v1/stage4a/world/fixtures/lembas_root_o2_v0_f3_s0.json"),
        include_str!("../regret_census_v1/stage4a/world/fixtures/lembas_root_o2_v0_f5_s1.json"),
    ] {
        let record: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let state: crate::state::GameState =
            serde_json::from_value(record["state"].clone()).unwrap();
        let viewer = PlayerId(record["actor"].as_u64().unwrap() as u8);
        let lists = fixture_lists(&state);
        let x = export_v1(
            &ExportInputV1 {
                state: &state,
                journal: &[],
                viewer,
                decklists: [&lists[0], &lists[1]],
                marks: Some(&every_event_marks(&state, &[])),
            },
            ExportOptionsV1::default(),
        )
        .unwrap_or_else(|e| panic!("{}: {e:?}", record["case"]));
        // The pinned Lembas, natively and as its canonical occupant.
        let pin_native = super::export::contract_pins(&state)
            .into_iter()
            .find(|id| matches!(state.objects.get(*id).zone, crate::state::Zone::Library))
            .expect("a pinned Lembas in the library");
        let pin_canon = x
            .handles
            .iter()
            .position(|h| matches!(h, HandleV1::U { pinned: true, .. }))
            .map(|i| crate::ids::ObjectId(i as u32))
            .expect("the pinned occupant");
        // The true state edited to the canonical placement (design 6.5):
        // the pin moves to its canonical library slot.
        let owner = state.objects.get(pin_native).owner;
        let slot = x.state.players[owner.index()]
            .library
            .iter()
            .position(|id| *id == pin_canon)
            .expect("the pin is in the canonical library");
        let mut edited = state.clone();
        let lib = &mut edited.players[owner.index()].library;
        let at = lib.iter().position(|id| *id == pin_native).unwrap();
        lib.swap(at, slot);
        let live = FastActorSessionV1::from_v3_fixture_state(edited);
        let mut truth = live.clone();
        let mut canon = FastActorSessionV1::import_infoset_v1(&live, x.state.clone(), 0);
        let waiting = |s: &FastActorSessionV1, src: crate::ids::ObjectId| {
            s.game_state().stack.iter().any(|i| i.source == src)
        };
        assert!(waiting(&truth, pin_native) && waiting(&canon, pin_canon));
        let root = |s: &FastActorSessionV1| match s.current_response() {
            FastActorResponseV1::Decision(d) => Some((
                d.acting_player,
                d.legal_action_count,
                serde_json::to_string(&s.diagnostic_current_action_semantics()).unwrap(),
            )),
            FastActorResponseV1::Terminal(_) => None,
        };
        let (rt, rc) = (root(&truth), root(&canon));
        assert_eq!(
            rt.as_ref().map(|r| (r.0, r.1)),
            rc.as_ref().map(|r| (r.0, r.1)),
            "{}: the imported root decision differs: {rt:?} vs {rc:?}",
            record["case"]
        );
        // Both pass until the Lembas item resolves. Each world's committed
        // events since the root, with objects named by definition, must
        // agree up to the next turn: later flow can depend on the hidden
        // hands the export refilled, the Lembas outcome cannot.
        let mut resolved = false;
        for _ in 0..60 {
            if !waiting(&truth, pin_native) {
                assert!(
                    !waiting(&canon, pin_canon),
                    "{}: the Lembas item resolved in only one world",
                    record["case"]
                );
                resolved = true;
                break;
            }
            resolve_waiting(&mut truth, 0, 1);
            resolve_waiting(&mut canon, 0, 1);
        }
        let from = state.engine.event_history.len();
        let (et, ec) = (public_events(&truth, from), public_events(&canon, from));
        assert!(
            et == ec,
            "{}: the Lembas continuation differs at {}",
            record["case"],
            first_diff(&et, &ec, String::new())
        );
        assert!(
            resolved,
            "{}: the Lembas item did not resolve",
            record["case"]
        );
    }
}

/// Sol r2 P1: a reveal opens its span after the events before it. The
/// opponent holds A, draws a same-name B unidentified, then reveals A or
/// B: V's histories are identical, so the exports must be byte-equal (the
/// draw stays a tombstone in both).
#[test]
fn a_reveal_never_links_the_unidentified_draw_before_it() {
    use crate::event::{propose_and_commit, ProposedEvent};
    use crate::state::Zone;
    let live = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
        43,
        43,
        2_000,
        20_000,
        ["Faeries".to_string(), "Faeries".to_string()],
    )
    .expect("reset");
    let viewer = PlayerId::P0;
    let opp = viewer.opponent();
    let base = live.game_state().clone();
    let lists = decklists(&live);
    let hand = base.players[opp.index()].hand.clone();
    let library = base.players[opp.index()].library.clone();
    let Some((a, b)) = hand.iter().find_map(|&a| {
        let def = base.objects.get(a).card_def;
        library
            .iter()
            .copied()
            .find(|&b| base.objects.get(b).card_def == def)
            .map(|b| (a, b))
    }) else {
        eprintln!("no same-name pair across hand and library at this seed");
        return;
    };
    let run = |revealed: crate::ids::ObjectId| {
        let mut st = base.clone();
        propose_and_commit(&mut st, ProposedEvent::zone_change(b, Zone::Hand));
        st.reveal_hand_card(viewer, opp, revealed).expect("reveal");
        propose_and_commit(&mut st, ProposedEvent::life_loss(opp, 1));
        export_v1(
            &ExportInputV1 {
                state: &st,
                journal: st.observation_journal.notes(),
                viewer,
                decklists: [&lists[0], &lists[1]],
                marks: None,
            },
            ExportOptionsV1::default(),
        )
        .map(|x| x.bytes)
        .map_err(|e| e.code_name().to_string())
    };
    assert_eq!(
        run(a),
        run(b),
        "revealing A or B must export the same bytes"
    );
}

/// R5 regression (bridge, Wildfire seed 6): the opponent's hand holds a
/// revealed copy A and an unrevealed copy B of one card; B leaves publicly.
/// Formerly V's fact about A stayed and A kept its `C:` handle. Amendment
/// 43.7 (lead ruling 12): A is not tracked in the hand, and B's departure
/// ends V's fact of the name, as the adapter does; the engine's row for A is
/// allowed, so the export is not refused R5. The name is kept for
/// traceability.
#[test]
fn a_public_departure_of_another_copy_keeps_the_revealed_copy_tracked() {
    use crate::event::{propose_and_commit, ProposedEvent};
    use crate::state::Zone;
    let viewer = PlayerId::P0;
    let opp = viewer.opponent();
    for seed in 50..200u64 {
        let live = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
            seed,
            seed,
            2_000,
            20_000,
            ["Wildfire".to_string(), "Wildfire".to_string()],
        )
        .expect("reset");
        let base = live.game_state().clone();
        let hand = base.players[opp.index()].hand.clone();
        let Some((a, b)) = hand.iter().enumerate().find_map(|(i, &a)| {
            hand[i + 1..]
                .iter()
                .find(|&&b| base.objects.get(b).card_def == base.objects.get(a).card_def)
                .map(|&b| (a, b))
        }) else {
            continue;
        };
        let lists = decklists(&live);
        let mut st = base.clone();
        st.reveal_hand_card(viewer, opp, a).expect("reveal");
        propose_and_commit(&mut st, ProposedEvent::zone_change(b, Zone::Graveyard));
        assert!(
            st.known_hand_cards(viewer, opp)
                .iter()
                .any(|k| k.object == a),
            "the engine still knows the revealed copy"
        );
        let x = export_v1(
            &ExportInputV1 {
                state: &st,
                journal: st.observation_journal.notes(),
                viewer,
                decklists: [&lists[0], &lists[1]],
                marks: None,
            },
            ExportOptionsV1::default(),
        )
        .unwrap_or_else(|e| panic!("seed {seed}: {e:?}"));
        // Amendment 43.7: the revealed copy is not tracked in the hand, and
        // B's identified departure ends V's only fact of the name (the
        // adapter's rule: V cannot tell which copy left), so no known-name
        // occupant of it remains; the engine's row for A is allowed (R5).
        assert!(
            !x.tracked_natives.contains(&a),
            "no opponent-hand copy is tracked"
        );
        let known = x.state.hand_knowledge[viewer.index()][opp.index()]
            .iter()
            .filter(|k| x.state.objects.get(k.object).card_def == st.objects.get(a).card_def)
            .count();
        assert_eq!(
            known, 0,
            "the fact ended with the departure of a copy of its name"
        );
        return;
    }
    panic!("no seed deals the opponent two copies of one card");
}

/// Issue #531: the same shape under the engine's opt-in name-level rule.
/// B's public departure ends V's row of A too, so the engine's rows name
/// what the transcript reducer knows (43.8's L), by name.
#[test]
fn name_level_rows_agree_with_the_transcript_reducer_after_a_same_name_departure() {
    use crate::event::{propose_and_commit, ProposedEvent};
    use crate::state::Zone;
    let viewer = PlayerId::P0;
    let opp = viewer.opponent();
    for seed in 50..200u64 {
        let live = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
            seed,
            seed,
            2_000,
            20_000,
            ["Wildfire".to_string(), "Wildfire".to_string()],
        )
        .expect("reset");
        let base = live.game_state().clone();
        let hand = base.players[opp.index()].hand.clone();
        let Some((a, b)) = hand.iter().enumerate().find_map(|(i, &a)| {
            hand[i + 1..]
                .iter()
                .find(|&&b| base.objects.get(b).card_def == base.objects.get(a).card_def)
                .map(|&b| (a, b))
        }) else {
            continue;
        };
        let mut st = base.clone();
        st.enable_hand_departures_by_name_v1();
        st.reveal_hand_card(viewer, opp, a).expect("reveal");
        propose_and_commit(&mut st, ProposedEvent::zone_change(b, Zone::Graveyard));
        let mut native: Vec<u16> = st
            .known_hand_cards(viewer, opp)
            .iter()
            .map(|k| st.objects.get(k.object).card_def)
            .collect();
        native.sort_unstable();
        let mut reducer: Vec<u16> =
            super::projection::project_v1(&st, st.observation_journal.notes(), viewer)
                .hand_facts
                .iter()
                .filter(|f| f.known)
                .map(|f| f.card_def)
                .collect();
        reducer.sort_unstable();
        assert!(native.is_empty(), "seed {seed}: the row of A ended");
        assert_eq!(native, reducer, "seed {seed}");
        return;
    }
    panic!("no seed deals the opponent two copies of one card");
}

/// Amendment 43.1: at every combat inclusion point (attackers, and blockers
/// inside the per-attacker reshape) the payload carries the live scan, and
/// the import presents the same point: the canonical row keys pair one to
/// one with the live menu's, and walking both sessions through the rest of
/// the viewer's points by paired rows keeps them paired.
#[test]
fn combat_scan_payloads_import_at_the_live_scan_point() {
    use crate::rl::ActionSemanticV1;
    let (mut attack, mut block, mut mid_scan, mut walked, mut refused) =
        (0u32, 0u32, 0u32, 0u32, 0u32);
    // Design 13 AC3 witnesses: declarations that open with at least two
    // candidates (several possible attackers, or several possible blockers
    // for one attacker).
    let (mut multi_attack, mut multi_block) = (0u32, 0u32);
    let mut state_refusals = 0u32;
    for (decks, seed) in [
        (["Burn", "Burn"], 41u64),
        (["Wildfire", "Elves"], 42),
        (["CawGates", "Faeries"], 43),
        (["Affinity", "Rally"], 44),
    ] {
        play(decks, seed, 800, |s, v| {
            let sem = s.diagnostic_current_action_semantics().unwrap_or_default();
            let kind = if sem
                .iter()
                .any(|a| matches!(a, ActionSemanticV1::ChooseAttackerInclusion { .. }))
            {
                crate::policy_surface_v5::CombatScanKindV1::Attackers
            } else if sem
                .iter()
                .any(|a| matches!(a, ActionSemanticV1::ChooseBlockerInclusion { .. }))
            {
                crate::policy_surface_v5::CombatScanKindV1::Blockers
            } else {
                return;
            };
            let Ok(x) = export_at(s, v, ExportOptionsV1::default()) else {
                return;
            };
            let ids = super::order::canonical_ids_v1(&x);
            let Some(payload) = PayloadV1::new(&x, s, v, 0) else {
                refused += 1;
                return;
            };
            let combat = payload
                .surface
                .combat
                .clone()
                .expect("a combat point's payload carries the scan");
            assert_eq!(combat.kind, kind, "{decks:?}: scan kind");
            let multi = combat.cursor == 0 && combat.candidates.len() >= 2;
            match kind {
                crate::policy_surface_v5::CombatScanKindV1::Attackers => {
                    attack += 1;
                    multi_attack += u32::from(multi);
                }
                crate::policy_surface_v5::CombatScanKindV1::Blockers => {
                    block += 1;
                    multi_block += u32::from(multi);
                }
            }
            // AC3 at the combat root: the imported menu and view, and a
            // random rollout to the end with zero engine faults.
            super::harness::ac3_check(s, &x, 7)
                .unwrap_or_else(|e| panic!("{decks:?}: AC3 at a combat root: {e}"));
            if combat.cursor > 0 {
                mid_scan += 1;
            }
            let back = PayloadV1::from_bytes(&payload.to_bytes()).expect("round trip");
            assert_eq!(back, payload, "the payload round-trips");
            let mut imported = payload.import().expect("a combat payload imports");
            let imported_ids = super::order::imported_ids_v1(&payload.handles);
            let mut live = s.clone();
            let root_turn_step = (s.game_state().turn, s.game_state().step);
            let inclusion = |x: &FastActorSessionV1| {
                x.diagnostic_current_action_semantics()
                    .unwrap_or_default()
                    .iter()
                    .any(|a| {
                        matches!(
                            a,
                            ActionSemanticV1::ChooseAttackerInclusion { .. }
                                | ActionSemanticV1::ChooseBlockerInclusion { .. }
                        )
                    })
            };
            // The import's state check refuses a record the surface would
            // not present there: a foreign or untracked candidate, the
            // other player.
            let state: crate::state::GameState =
                serde_json::from_value(payload.state.clone()).expect("payload state");
            combat
                .validate_in_state(&state, &payload.handles)
                .expect("the live record passes the state check");
            let last = combat.candidates.len() - 1;
            let swapped = |o: crate::ids::ObjectId| {
                let mut bad = combat.clone();
                bad.candidates[last] = o;
                bad
            };
            let index = |pred: &dyn Fn(&str) -> bool| {
                (0..payload.handles.len())
                    .map(|n| crate::ids::ObjectId(n as u32))
                    .find(|o| {
                        pred(&payload.handles[o.0 as usize]) && !combat.candidates.contains(o)
                    })
            };
            if last >= combat.cursor {
                if let Some(o) = index(&|h| h.starts_with("C:")) {
                    assert!(swapped(o)
                        .validate_in_state(&state, &payload.handles)
                        .is_err());
                    state_refusals += 1;
                }
                if let Some(o) = index(&|h| !h.starts_with("C:")) {
                    assert!(swapped(o)
                        .validate_in_state(&state, &payload.handles)
                        .is_err());
                    state_refusals += 1;
                }
            }
            let mut other = combat.clone();
            other.player = other.player.opponent();
            assert!(other.validate_in_state(&state, &payload.handles).is_err());
            // The rest of the declaration: every remaining inclusion point
            // (later candidates, and for blocks later attackers' scans).
            let mut left = false;
            for depth in 0..256usize {
                // A terminal session has left the declaration (the game can
                // end right after one, before the next decision).
                let decision = |x: &FastActorSessionV1| match x.current_response() {
                    FastActorResponseV1::Decision(d) => Some(d),
                    FastActorResponseV1::Terminal(t) => {
                        assert_ne!(
                            t.terminal_classification,
                            crate::rl::TerminalClassificationV1::Halted,
                            "{decks:?}: a combat walk halted: {}",
                            t.terminal_reason
                        );
                        None
                    }
                };
                let (d, e) = (decision(&live), decision(&imported));
                // The same declaration: the viewer's inclusion points in the
                // root's turn and combat step (afterwards the worlds' hidden
                // cards may lead them apart).
                let same = |x: &FastActorSessionV1, d: Option<FastActorDecisionV1>| {
                    let st = x.game_state();
                    d.is_some_and(|d| seat(d.acting_player) == v)
                        && inclusion(x)
                        && (st.turn, st.step) == root_turn_step
                };
                let (in_live, in_imported) = (same(&live, d), same(&imported, e));
                if in_live != in_imported {
                    let forms = |x: &FastActorSessionV1| {
                        x.diagnostic_current_action_semantics()
                            .unwrap_or_default()
                            .iter()
                            .map(|a| serde_json::to_string(a).unwrap())
                            .collect::<Vec<_>>()
                    };
                    panic!(
                        "{decks:?}: depth {depth}: scans end apart (live {in_live}, imported {in_imported}); root combat {combat:?}; live actor {:?} menu {:?}; imported actor {:?} menu {:?}",
                        d.map(|d| d.acting_player),
                        forms(&live),
                        e.map(|e| e.acting_player),
                        forms(&imported)
                    );
                }
                if !in_live {
                    left = true;
                    break;
                }
                let (Some(d), Some(e)) = (d, e) else {
                    unreachable!("both sessions are at the viewer's inclusion point")
                };
                let a = super::order::row_keys_v1(&live, Some(&ids)).expect("live keys");
                let b = super::order::row_keys_v1(&imported, Some(&imported_ids))
                    .expect("imported keys");
                if depth == 0 {
                    assert_eq!(a, b, "{decks:?}: the import presents the live scan point");
                }
                let pairs = super::order::live_rows_v1(&a, &b).unwrap_or_else(|| {
                    panic!("{decks:?}: depth {depth}: keys do not pair: {a:?} vs {b:?}")
                });
                let i = depth % b.len();
                imported
                    .step(e.episode_id, e.step, i as u32)
                    .expect("imported step");
                live.step(d.episode_id, d.step, pairs[i])
                    .expect("live step");
                walked += 1;
            }
            assert!(
                left,
                "{decks:?}: the declaration did not end within its bound"
            );
        });
    }
    eprintln!(
        "combat payloads: {attack} attacker points ({multi_attack} multi-attacker roots), {block} blocker points ({multi_block} multi-blocker roots), {mid_scan} mid-scan, {walked} paired steps, {refused} refused, {state_refusals} tampered records refused"
    );
    assert!(state_refusals > 0, "tampered records reach the state check");
    assert!(
        multi_attack >= 5 && multi_block >= 5,
        "design 13: 5 multi-attacker and 5 multi-blocker witnesses"
    );
    assert!(attack > 0 && block > 0, "both scan kinds exported");
    assert!(mid_scan > 0, "a scan past its first candidate exported");
    assert_eq!(refused, 0, "no combat point refuses");
}

#[test]
fn combat_context_validation_fails_closed() {
    use crate::ids::ObjectId;
    use crate::policy_surface_v5::{CombatContextV1, CombatScanKindV1};
    let ok = CombatContextV1 {
        kind: CombatScanKindV1::Attackers,
        player: PlayerId::P0,
        attacker: None,
        candidates: vec![ObjectId(3), ObjectId(5)],
        cursor: 1,
        selected: vec![ObjectId(3)],
        blockers: None,
    };
    assert!(ok.validate(10).is_ok());
    let bad = |f: &dyn Fn(&mut CombatContextV1)| {
        let mut c = ok.clone();
        f(&mut c);
        c.validate(10).is_err()
    };
    assert!(
        bad(&|c| c.candidates[1] = ObjectId(10)),
        "outside the arena"
    );
    assert!(bad(&|c| c.cursor = 2), "exhausted scan");
    assert!(
        bad(&|c| c.selected = vec![ObjectId(5)]),
        "selection past the cursor"
    );
    assert!(
        bad(&|c| c.candidates[1] = ObjectId(3)),
        "repeated candidate"
    );
    assert!(
        bad(&|c| c.kind = CombatScanKindV1::Blockers),
        "blocker scan without a reshape"
    );
    assert!(bad(&|c| c.player = PlayerId(2)), "player");
    let three = CombatContextV1 {
        candidates: vec![ObjectId(3), ObjectId(5), ObjectId(7)],
        cursor: 2,
        selected: vec![ObjectId(3), ObjectId(5)],
        ..ok.clone()
    };
    assert!(three.validate(10).is_ok());
    for selected in [vec![3, 3], vec![5, 3]] {
        let mut c = three.clone();
        c.selected = selected.iter().map(|x| ObjectId(*x)).collect();
        assert!(c.validate(10).is_err(), "selection {selected:?}");
    }
    let renamed = ok
        .rename(|o| Some(ObjectId(o.0 + 1)))
        .expect("every object renames");
    assert_eq!(renamed.candidates, vec![ObjectId(4), ObjectId(6)]);
    assert!(
        ok.rename(|o| (o.0 != 5).then_some(o)).is_none(),
        "an unmapped object refuses"
    );
}

/// Amendment 43.1 part 2: at every viewer discard point the payload carries
/// the discard record and imports at the same pick (the row keys pair one to
/// one), AC3 holds there, tampered records are refused against the real
/// state, and walking both sessions through the rest of the discard by
/// paired rows keeps them paired.
#[test]
fn discard_payloads_import_at_the_live_discard_point() {
    use crate::ids::ObjectId;
    use crate::policy_surface_v5::DiscardContextV1;
    use crate::rl::ActionSemanticV1;
    let (mut roots, mut mid, mut walked, mut refused, mut tampered) =
        (0u32, 0u32, 0u32, 0u32, 0u32);
    let discarding = |x: &FastActorSessionV1| {
        x.diagnostic_current_action_semantics()
            .unwrap_or_default()
            .iter()
            .any(|a| matches!(a, ActionSemanticV1::Discard { .. }))
    };
    for (decks, seed) in [
        (["Wildfire", "Faeries"], 51u64),
        (["CawGates", "Burn"], 52),
        (["Faeries", "Elves"], 53),
        (["Affinity", "Wildfire"], 54),
        (["Burn", "CawGates"], 55),
        (["Rally", "Faeries"], 56),
        (["Elves", "Affinity"], 57),
        (["Wildfire", "Wildfire"], 58),
    ] {
        play(decks, seed, 1_000, |s, v| {
            if !discarding(s) {
                return;
            }
            let Ok(x) = export_at(s, v, ExportOptionsV1::default()) else {
                return;
            };
            let ids = super::order::canonical_ids_v1(&x);
            let Some(payload) = PayloadV1::new(&x, s, v, 0) else {
                refused += 1;
                return;
            };
            let discard = payload
                .surface
                .discard
                .clone()
                .expect("a discard point's payload carries the record");
            assert!(payload.surface.combat.is_none(), "{decks:?}: one record");
            roots += 1;
            if !discard.chosen.is_empty() {
                mid += 1;
            }
            let back = PayloadV1::from_bytes(&payload.to_bytes()).expect("round trip");
            assert_eq!(back, payload, "the payload round-trips");
            let mut imported = payload.import().expect("a discard payload imports");
            let imported_ids = super::order::imported_ids_v1(&payload.handles);
            super::harness::ac3_check(s, &x, 7)
                .unwrap_or_else(|e| panic!("{decks:?}: AC3 at a discard root: {e}"));
            // The import's state check refuses a record the surface would
            // not present there.
            let state: crate::state::GameState =
                serde_json::from_value(payload.state.clone()).expect("payload state");
            discard
                .validate_in_state(&state, &payload.handles, v)
                .expect("the live record passes the state check");
            let bad = |f: &dyn Fn(&mut DiscardContextV1)| {
                let mut d = discard.clone();
                f(&mut d);
                d.validate_in_state(&state, &payload.handles, v).is_err()
            };
            assert!(bad(&|d| d.remaining_needed += 1), "{decks:?}: count");
            assert!(
                bad(&|d| d.player = d.player.opponent()),
                "{decks:?}: player"
            );
            let foreign = (0..payload.handles.len())
                .map(|n| ObjectId(n as u32))
                .find(|o| !state.players[v.index()].hand.contains(o));
            if let Some(o) = foreign {
                assert!(
                    bad(&|d| {
                        d.chosen.push(o);
                        d.remaining_needed = d.remaining_needed.saturating_sub(1).max(1);
                    }),
                    "{decks:?}: a pick outside the hand"
                );
                tampered += 1;
            }
            tampered += 2;
            // The rest of the discard: every remaining pick of this
            // obligation (the viewer's discard points in the root's turn and
            // step).
            let mut live = s.clone();
            let root = (s.game_state().turn, s.game_state().step);
            let mut left = false;
            for depth in 0..64usize {
                let decision = |x: &FastActorSessionV1| match x.current_response() {
                    FastActorResponseV1::Decision(d) => Some(d),
                    FastActorResponseV1::Terminal(t) => {
                        assert_ne!(
                            t.terminal_classification,
                            crate::rl::TerminalClassificationV1::Halted,
                            "{decks:?}: a discard walk halted: {}",
                            t.terminal_reason
                        );
                        None
                    }
                };
                let (d, e) = (decision(&live), decision(&imported));
                let same = |x: &FastActorSessionV1, d: Option<FastActorDecisionV1>| {
                    let st = x.game_state();
                    d.is_some_and(|d| seat(d.acting_player) == v)
                        && discarding(x)
                        && (st.turn, st.step) == root
                };
                let (in_live, in_imported) = (same(&live, d), same(&imported, e));
                assert_eq!(
                    in_live, in_imported,
                    "{decks:?}: depth {depth}: discards end apart"
                );
                if !in_live {
                    left = true;
                    break;
                }
                let (Some(d), Some(e)) = (d, e) else {
                    unreachable!("both sessions are at the viewer's discard point")
                };
                let a = super::order::row_keys_v1(&live, Some(&ids)).expect("live keys");
                let b = super::order::row_keys_v1(&imported, Some(&imported_ids))
                    .expect("imported keys");
                let pairs = super::order::live_rows_v1(&a, &b).unwrap_or_else(|| {
                    panic!("{decks:?}: depth {depth}: keys do not pair: {a:?} vs {b:?}")
                });
                let i = depth % b.len();
                imported
                    .step(e.episode_id, e.step, i as u32)
                    .expect("imported step");
                live.step(d.episode_id, d.step, pairs[i])
                    .expect("live step");
                walked += 1;
            }
            assert!(left, "{decks:?}: the discard did not end within its bound");
        });
    }
    eprintln!(
        "discard payloads: {roots} discard points ({mid} past the first pick), {walked} paired steps, {refused} refused, {tampered} tampered records refused"
    );
    assert_eq!(refused, 0, "no discard point refuses");
    assert!(roots >= 5, "discard witnesses");
    assert!(mid > 0, "a discard past its first pick exported");
}

/// G1 (fairness gate) and amendment 43.2: R3 is the opponent's pending
/// library-search plan only; V's own pending search refuses R3b
/// (`own_library_search`), decided by the pending effect's owner.
#[test]
fn r3_refuses_only_the_opponents_library_search() {
    use crate::effect::library_choice_search_v2::plan_future_v3;
    let (mut own, mut own_exported, mut opponent) = (0u32, 0u32, 0u32);
    let mut own_refusals: std::collections::BTreeMap<String, u32> = Default::default();
    for (decks, seed) in [
        (["CawGates", "Faeries"], 61u64),
        (["CawGates", "CawGates"], 62),
        (["Faeries", "CawGates"], 63),
        (["CawGates", "Burn"], 64),
        (["CawGates", "Wildfire"], 65),
        (["Elves", "CawGates"], 66),
    ] {
        play(decks, seed, 1_000, |s, v| {
            let st = s.game_state();
            let own_plan = matches!(plan_future_v3(st, v), Ok(Some(_)));
            let opponent_plan = matches!(plan_future_v3(st, v.opponent()), Ok(Some(_)));
            let x = export_at(s, v, ExportOptionsV1::default());
            if own_plan {
                own += 1;
                match &x {
                    Ok(_) => own_exported += 1,
                    Err(e) => {
                        assert_eq!(
                            e.code_name(),
                            "R3b",
                            "{decks:?}: V's own search refuses R3b"
                        );
                        *own_refusals.entry(e.code_name().to_string()).or_default() += 1;
                    }
                }
            }
            if opponent_plan {
                opponent += 1;
                assert!(
                    matches!(&x, Err(ExportErrorV1::Refused { code: RefusalCodeV1::R3, detail })
                        if detail == super::export::R3_OPPONENT_DETAIL),
                    "{decks:?}: the opponent's plan refuses R3"
                );
            }
        });
    }
    eprintln!(
        "library searches: {own} of V's own ({own_exported} exported, refusals {own_refusals:?}), {opponent} of the opponent's"
    );
    assert!(own > 0, "V's own library searches witnessed");
}

/// Amendment 43.9, cut 2 (R2): a live optional-cost reshape and the madness
/// reprompt exemption travel in the payload instead of refusing. At V's
/// decisions of Burn mirrors (Highway Robbery, Fiery Temper) where either
/// is live, the payload is built, its import passes the fail-closed checks
/// and presents the live decision (same actor and menu size), and AC3's
/// menu bijection and rollout hold from the export.
#[test]
fn optional_cost_and_madness_records_travel_and_import() {
    let mut seen = [0usize; 2];
    for seed in 0..400_u64 {
        play(["Burn", "Burn"], 4_300 + seed, 700, |s, v| {
            let k = match s.payload_refusal_detail_v1(v) {
                "optional_cost_reshape" => 0,
                "madness_reprompt" => 1,
                _ => return,
            };
            let Ok(x) = export_at(s, v, ExportOptionsV1::default()) else {
                return;
            };
            // The madness exemption travels only during its announcement;
            // after it (trigger ordering) the producer still refuses.
            let announcing = s.game_state().engine.pending_cast.is_some();
            let Some(p) = super::payload::PayloadV1::new(&x, s, v, 0) else {
                assert!(
                    k == 1 && !announcing,
                    "seed {seed}: the {k} record does not travel"
                );
                return;
            };
            assert!(
                p.surface.optional_cost.is_some() || p.surface.madness_reprompt.is_some(),
                "seed {seed}: no record"
            );
            let imported = p
                .import()
                .unwrap_or_else(|e| panic!("seed {seed}: import: {e}"));
            let (FastActorResponseV1::Decision(d), FastActorResponseV1::Decision(e)) =
                (s.current_response(), imported.current_response())
            else {
                panic!("seed {seed}: the imported root is not a decision");
            };
            assert_eq!(
                (d.acting_player, d.legal_action_count),
                (e.acting_player, e.legal_action_count),
                "seed {seed}"
            );
            super::harness::ac3_check(s, &x, seed).expect("AC3");
            seen[k] += 1;
        });
        if seen.iter().all(|&n| n >= 4) {
            break;
        }
    }
    assert!(
        seen.iter().all(|&n| n > 0),
        "witnesses (optional cost, madness): {seen:?}"
    );
}

/// Amendment 43.9, cut 1 (`same_name_batch`): the move of the spell being
/// announced is walked after the committed stream, with the committed
/// lengths V's cast decision was presented at; it is the open item, a batch
/// of its own, while every other item keeps the batch it had (bounds over
/// the closed items, so event-less moves after it stay in the closed batch).
/// At every V decision with a pending cast in these mirrors: the open item
/// is the announced spell's move and has its own batch; every closed item
/// keeps the batch the former placement gave it (recomputed here with the
/// open item stamped like the others), the open item excepted; witnesses
/// are decisions where the former placement put a same-name copy of the
/// owner, leaving the same zone, in the open item's batch (the refusal
/// the cut removes).
#[test]
fn the_announced_spell_moves_in_the_open_batch() {
    use super::projection::ProjectionV1;
    use crate::card_def::CARD_DEFS;
    let former = |p: &ProjectionV1, marks: &[(usize, usize)]| -> Vec<usize> {
        let mut out: Vec<usize> = marks
            .iter()
            .map(|&(h, j)| {
                p.stamps
                    .iter()
                    .take_while(|&&(e, n)| e <= h && n <= j)
                    .count()
            })
            .filter(|&b| b < p.stamps.len())
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    };
    let mut pending = 0;
    let mut witnesses = 0;
    let mut closed_checked = 0;
    for (decks, seeds) in [
        (["Burn", "Burn"], 0..30_u64),
        (["Faeries", "Faeries"], 0..30),
        (["Rally", "Rally"], 0..30),
    ] {
        for seed in seeds {
            play(decks, 9_100 + seed, 500, |s, v| {
                let st = s.game_state();
                let Some(cast) = st.engine.pending_cast.as_ref() else {
                    return;
                };
                let p = super::projection::project_v1(st, st.observation_journal.notes(), v);
                let Some(open) = p.open_item else {
                    return;
                };
                pending += 1;
                let marks = s.infoset_marks_v1(v);
                let bounds = p.batch_bounds(Some(&marks));
                let old = former(&p, &marks);
                let old_batch = |item: usize| old.partition_point(|&b| b <= item);
                let end = p.objects[cast.spell.0 as usize]
                    .spans
                    .iter()
                    .filter_map(|x| x.end)
                    .find(|e| e.item == open)
                    .unwrap_or_else(|| {
                        panic!("seed {seed}: the open item is not the spell's move")
                    });
                assert_eq!(p.batch_of(&bounds, open), usize::MAX, "seed {seed}");
                // Batches are runs of consecutive items: every two
                // consecutive closed items keep their former relation.
                // `p.items` is the reconciliation index (`final_item`) when
                // event-less moves follow the announcement.
                let closed: Vec<usize> = (0..=p.items).filter(|&a| a != open).collect();
                for w in closed.windows(2) {
                    assert_eq!(
                        p.batch_of(&bounds, w[0]) == p.batch_of(&bounds, w[1]),
                        old_batch(w[0]) == old_batch(w[1]),
                        "seed {seed}: items {} and {}",
                        w[0],
                        w[1]
                    );
                    closed_checked += 1;
                }
                let name = CARD_DEFS[st.objects.get(cast.spell).card_def as usize].name;
                let owner = st.objects.get(cast.spell).owner;
                if p.objects.iter().enumerate().any(|(i, o)| {
                    let other = st.objects.get(crate::ids::ObjectId(i as u32));
                    i != cast.spell.0 as usize
                        && other.owner == owner
                        && CARD_DEFS[other.card_def as usize].name == name
                        && o.spans.iter().filter_map(|x| x.end).any(|e| {
                            e.from == end.from
                                && e.item != open
                                && old_batch(e.item) == old_batch(open)
                        })
                }) {
                    witnesses += 1;
                }
            });
        }
    }
    assert!(
        pending > 50 && witnesses > 0 && closed_checked > 0,
        "pending casts {pending}, witnesses {witnesses}, closed pairs {closed_checked}"
    );
}

/// Amendment 43.9, cut 1, the placement rule on its own (Sol design review
/// round 2): committed items stamped (1, 0) and (2, 0), the announcement
/// (the open item) and an event-less move after it, both walked at (2, 0),
/// and the reconciliation index after them; marks at (1, 0) and (2, 0). The
/// open item is a batch of its own; the event-less move and the
/// reconciliation index stay in the batch of the committed item before the
/// announcement, as the former placement had them.
#[test]
fn the_open_item_leaves_every_other_batch_alone() {
    let s = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
        5,
        5,
        2_000,
        20_000,
        ["Burn".to_string(), "Burn".to_string()],
    )
    .expect("reset");
    let st = s.game_state();
    let mut p = super::projection::project_v1(st, st.observation_journal.notes(), PlayerId(0));
    p.stamps = vec![(1, 0), (2, 0), (2, 0), (2, 0)];
    p.items = 4;
    let marks = [(1, 0), (2, 0)];
    p.open_item = None;
    let former = p.batch_bounds(Some(&marks));
    let former: Vec<usize> = (0..=4).map(|i| p.batch_of(&former, i)).collect();
    assert_eq!(former, vec![0, 1, 1, 1, 1]);
    p.open_item = Some(2);
    let bounds = p.batch_bounds(Some(&marks));
    let now: Vec<usize> = (0..=4).map(|i| p.batch_of(&bounds, i)).collect();
    assert_eq!(now, vec![0, 1, usize::MAX, 1, 1]);
    // A mark between the event-less move and a later committed item still
    // separates them.
    p.stamps = vec![(1, 0), (2, 0), (2, 0), (2, 0), (3, 0)];
    p.items = 5;
    let bounds = p.batch_bounds(Some(&[(1, 0), (2, 0), (3, 0)]));
    let now: Vec<usize> = (0..=5).map(|i| p.batch_of(&bounds, i)).collect();
    assert_eq!(now, vec![0, 1, usize::MAX, 1, 2, 2]);
}
