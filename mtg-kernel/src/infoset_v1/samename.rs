//! Lead ruling 7 (a): a live reference across an unlinked visible move keeps
//! its link (6.5) only while V can tell which copy moved. Two same-name
//! contract sources that leave in one batch of V's transcript refuse R1
//! (`same_name_batch`), in both worlds.

use super::fixtures::{
    activate, cast, force_deal, id, kind, new_game, play_land, record_strict, restart, AtV1,
    IntentV1, ScriptV1,
};
use super::harness::PairReportV1;
use super::tests::export_at;
use super::*;
use crate::ids::PlayerId;
use crate::rl_session::FastActorSessionV1;

/// Activate the `nth` copy of `name` (by arena id, ascending) among the
/// copies whose ability the menu offers.
fn activate_copy(name: &str, nth: usize) -> IntentV1 {
    let d = u64::from(id(name));
    let ours = move |v: &serde_json::Value| {
        kind(v) == "activate_ability" && v["source"]["card_db_id"].as_u64() == Some(d)
    };
    IntentV1::TakeIn(
        format!("activate {name} copy {nth}"),
        Box::new(move |s: &FastActorSessionV1, v| {
            if !ours(v) {
                return false;
            }
            let mut copies: Vec<u64> = s
                .diagnostic_current_action_semantics()
                .unwrap_or_default()
                .iter()
                .map(|x| serde_json::to_value(x).expect("semantics serialize"))
                .filter(|x| ours(x))
                .filter_map(|x| x["source"]["arena_id"].as_u64())
                .collect();
            copies.sort_unstable();
            copies.dedup();
            copies.get(nth).copied() == v["source"]["arena_id"].as_u64()
        }),
    )
}

/// The same-name batch fixture (Wildfire versus Faeries, off-board). V is
/// Faeries (P1). Wildfire casts two Lembas on turns 2 and 3; on turn 4,
/// holding priority, it activates both (each sacrificed as a cost, its
/// shuffle trigger waiting), `first` choosing which copy goes first. V's
/// next records show two Lembas moving from the battlefield to the
/// graveyard in one batch and the waiting items' sources as null: V cannot
/// pair the items with the graveyard copies, so every live contract to
/// either copy refuses until it resolves.
pub(crate) fn same_name_batch(first: usize) -> ScriptV1 {
    let (o, v) = (PlayerId::P0, PlayerId::P1);
    let live = new_game(["Wildfire", "Faeries"], 75, o);
    let mut st = live.game_state().clone();
    force_deal(
        &mut st,
        o,
        &[
            "Mountain", "Swamp", "Swamp", "Forest", "Lembas", "Lembas", "Forest",
        ],
        &["Ichor Wellspring", "Cast Down", "Cast Down", "Cast Down"],
    );
    force_deal(
        &mut st,
        v,
        &[
            "Island",
            "Island",
            "Island",
            "Spellstutter Sprite",
            "Island",
            "Force Spike",
            "Dispel",
        ],
        &["Island", "Island", "Island", "Island"],
    );
    let mut script = ScriptV1::new(restart(&live, st));
    script.push(
        o,
        vec![
            play_land("Mountain"),
            play_land("Swamp"),
            cast("Lembas"),
            play_land("Swamp"),
            cast("Lembas"),
            play_land("Forest"),
            activate_copy("Lembas", first),
            activate("Lembas"),
        ],
    );
    script.keep(o, &["Mountain", "Swamp", "Forest", "Lembas"]);
    script.push(
        v,
        vec![
            play_land("Island"),
            play_land("Island"),
            play_land("Island"),
            play_land("Island"),
        ],
    );
    script
}

/// Each V decision's refusal subclass under P, in order (None: exported).
fn subclasses(first: usize, v: PlayerId) -> Vec<Option<&'static str>> {
    let mut out = Vec::new();
    let mut script = same_name_batch(first);
    script.run(400, 24, &mut |s, p, _| {
        if p == v {
            out.push(
                export_at(s, v, ExportOptionsV1::default())
                    .err()
                    .map(|e| crate::agent_bridge_infoset_v1::export_refusal_subclass_v1(&e)),
            );
        }
    });
    out
}

/// Lead ruling 7 (a), fixture witness: the two Lembas leave in one batch in
/// either order. At V's decisions while their items wait, P refuses R1
/// `same_name_batch` in both worlds, and P agrees across the worlds at every
/// compared decision.
#[test]
fn same_name_batch_refuses_in_both_worlds() {
    let v = PlayerId::P1;
    let runs: Vec<(Vec<AtV1>, ScriptV1)> = [0usize, 1]
        .iter()
        .map(|&first| record_strict(same_name_batch(first), v, 24, &format!("fx-samename-v{first}")))
        .collect();
    let (a, b) = (&runs[0].0, &runs[1].0);
    let report: PairReportV1 = super::fixtures::compare_records(a, b);
    eprintln!("same-name batch, first copy 0 vs 1: {report:?}");
    let (sa, sb) = (subclasses(0, v), subclasses(1, v));
    eprintln!("refusal subclasses at V's decisions: {sa:?} / {sb:?}");
    assert_eq!(report.p_differs, 0, "P differs across the orders: {report:?}");
    assert!(report.compared > 0, "the pair compares decisions: {report:?}");
    let both = sa
        .iter()
        .zip(&sb)
        .take(report.compared as usize)
        .filter(|(x, y)| **x == Some("same_name_batch") && **y == Some("same_name_batch"))
        .count();
    assert!(
        both > 0,
        "R1 same_name_batch refuses in both worlds at a compared decision: {sa:?} / {sb:?}"
    );
}
