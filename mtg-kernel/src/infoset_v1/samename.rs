//! Lead ruling 7 (a): a live reference across an unlinked visible move keeps
//! its link (6.5) only while V can tell which copy moved. Two same-name
//! contract sources that leave in one batch of V's transcript refuse R1
//! (`same_name_batch`), in both worlds.

use super::fixtures::{
    cast, force_deal, gate, id, kind, new_game, pick, play_land, record_strict, restart, AtV1,
    IntentV1, ScriptV1,
};
use super::harness::PairReportV1;
use super::tests::export_at;
use super::*;
use crate::ids::PlayerId;
use crate::rl_session::FastActorSessionV1;
use crate::state::GameState;

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

const SHAMAN: &str = "Krark-Clan Shaman";

/// Waiting Shaman abilities.
fn shaman_items(st: &GameState) -> usize {
    let shaman = id(SHAMAN);
    st.stack
        .iter()
        .filter(|i| st.objects.get(i.source).card_def == shaman)
        .count()
}

/// The same-name batch fixture (Wildfire versus Faeries, off-board). V is
/// Faeries (P1). Wildfire casts two Krark-Clan Shamans and two Ichor
/// Wellsprings. On turn 4 it activates Shaman copy `first` (sacrificing a
/// Wellspring); once the Wellspring's draw trigger resolved, with that
/// ability still waiting, it activates the other Shaman. The second ability
/// resolves first: 1 damage to each creature without flying kills both
/// Shamans at once, one batch of V's transcript. The first ability waits
/// with its source contract naming one of the two graveyard Shamans, and V
/// cannot pair it with either: every V decision while it waits refuses R1
/// `same_name_batch`. (The engine passes priority after an activation, so
/// two sacrifices with priority held, the first design, never share a
/// batch.)
pub(crate) fn same_name_batch(first: usize) -> ScriptV1 {
    let (o, v) = (PlayerId::P0, PlayerId::P1);
    let live = new_game(["Wildfire", "Faeries"], 75, o);
    let mut st = live.game_state().clone();
    force_deal(
        &mut st,
        o,
        &[
            "Mountain",
            "Krark-Clan Shaman",
            "Mountain",
            "Krark-Clan Shaman",
            "Swamp",
            "Ichor Wellspring",
            "Ichor Wellspring",
        ],
        &["Swamp", "Cast Down", "Cast Down", "Forest", "Cast Down"],
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
            cast(SHAMAN),
            play_land("Mountain"),
            cast(SHAMAN),
            play_land("Swamp"),
            cast("Ichor Wellspring"),
            play_land("Swamp"),
            cast("Ichor Wellspring"),
            activate_copy(SHAMAN, first),
            pick("Ichor Wellspring"),
            gate("the first Wellspring trigger resolved", |st: &GameState| {
                st.stack.len() == 1 && shaman_items(st) == 1
            }),
            activate_copy(SHAMAN, 1 - first),
            pick("Ichor Wellspring"),
        ],
    );
    script.keep(o, &["Mountain", "Swamp", SHAMAN, "Ichor Wellspring"]);
    script.push(
        v,
        vec![
            play_land("Island"),
            play_land("Island"),
            play_land("Island"),
        ],
    );
    script
}

/// Each V decision's refusal subclass under P, in order (None: exported),
/// with the number of Shamans in the graveyard and of waiting Shaman
/// abilities, and a diagnostic line per V decision.
type AtDecisionV1 = (Option<&'static str>, usize, usize);

fn subclasses(first: usize, v: PlayerId) -> (Vec<AtDecisionV1>, Vec<String>) {
    let shaman = id(SHAMAN);
    let mut out = Vec::new();
    let mut lines = Vec::new();
    let mut script = same_name_batch(first);
    script.run(400, 12, &mut |s, p, step| {
        if p == v {
            let st = s.game_state();
            let count = |zone: &[crate::ids::ObjectId]| {
                zone.iter()
                    .filter(|x| st.objects.get(**x).card_def == shaman)
                    .count()
            };
            let sub = export_at(s, v, ExportOptionsV1::default())
                .err()
                .map(|e| crate::agent_bridge_infoset_v1::export_refusal_subclass_v1(&e));
            let graveyard = count(&st.players[0].graveyard);
            lines.push(format!(
                "V decision {} (step {step}, turn {}): stack {}, Shaman items {}, Shamans battlefield {} graveyard {graveyard}; P {sub:?}",
                out.len(),
                st.turn,
                st.stack.len(),
                shaman_items(st),
                count(&st.players[0].battlefield),
            ));
            out.push((sub, graveyard, shaman_items(st)));
        }
    });
    lines.extend(
        script
            .trace
            .iter()
            .enumerate()
            .filter(|(_, t)| t.contains("activate Krark"))
            .map(|(i, t)| format!("step {i}: {}", t.chars().take(160).collect::<String>())),
    );
    (out, lines)
}

/// Lead ruling 7 (a), fixture witness: the two Shamans leave in one batch
/// with the first Shaman's ability waiting, whichever copy activated first.
/// At V's decisions while it waits, P refuses R1 `same_name_batch` in both
/// worlds, and P agrees across the worlds at every compared decision.
#[test]
fn same_name_batch_refuses_in_both_worlds() {
    let v = PlayerId::P1;
    let runs: Vec<(Vec<AtV1>, ScriptV1)> = [0usize, 1]
        .iter()
        .map(|&first| {
            record_strict(
                same_name_batch(first),
                v,
                12,
                &format!("fx-samename-v{first}"),
            )
        })
        .collect();
    let (a, b) = (&runs[0].0, &runs[1].0);
    let report: PairReportV1 = super::fixtures::compare_records(a, b);
    eprintln!("same-name batch, first copy 0 vs 1: {report:?}");
    let ((sa, la), (sb, lb)) = (subclasses(0, v), subclasses(1, v));
    eprintln!("same-name batch, first copy 0:\n{}", la.join("\n"));
    eprintln!("same-name batch, first copy 1:\n{}", lb.join("\n"));
    assert_eq!(
        report.p_differs, 0,
        "P differs across the orders: {report:?}"
    );
    assert!(
        report.compared > 0,
        "the pair compares decisions: {report:?}"
    );
    // The V decisions with both Shamans in the graveyard and an ability
    // still waiting: the batch happened, and each refuses in both worlds.
    let after = |x: &[AtDecisionV1]| -> Vec<Option<&'static str>> {
        x.iter()
            .filter(|(_, g, items)| *g == 2 && *items > 0)
            .map(|(sub, _, _)| *sub)
            .collect()
    };
    let (ra, rb) = (after(&sa), after(&sb));
    assert!(
        !ra.is_empty() && ra.len() == rb.len(),
        "both worlds reach V decisions after the batch with the ability waiting: {sa:?} / {sb:?}"
    );
    assert!(
        ra.iter().chain(&rb).all(|s| *s == Some("same_name_batch")),
        "R1 same_name_batch refuses in both worlds while the ability waits: {sa:?} / {sb:?}"
    );
    // Before the batch, the waiting ability's single source keeps 6.5's
    // link: V decisions with one Shaman ability waiting and both Shamans on
    // the battlefield export.
    let before = |x: &[AtDecisionV1]| {
        x.iter()
            .any(|(sub, g, items)| *g == 0 && *items == 1 && sub.is_none())
    };
    assert!(
        before(&sa) && before(&sb),
        "a single waiting source exports before the batch: {sa:?} / {sb:?}"
    );
}
