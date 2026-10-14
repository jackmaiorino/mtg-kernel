//! Interchangeable choices (opt-in, `S4A_INTERCHANGE=1` in `s4a-budget`).
//!
//! The engine offers one legal action per object, so visibly identical copies
//! (two Mountains in hand, two untapped Mountains, two Blood tokens) are
//! separate candidates. E's canonical edges number such copies by engine
//! order, so each copy is its own edge with its own subtree: visits and
//! natural backups split between choices that lead to the same game, the
//! untried rule spends simulations trying every copy, and an execution that
//! plays a different copy than the one searched leaves the tree.
//!
//! Two live candidates are interchangeable when one exchange of two handles
//! that tie in a declared-unordered list (`canon2::tie_classes`) maps the
//! acting player's whole prepared observation onto itself (unordered lists as
//! multisets, incarnation counters relative to the smallest shown for the
//! same object) and the menu onto itself, taking one candidate to the other.
//! Orbits are the transitive closure (a composition of such exchanges is
//! again a symmetry of the view and the menu).
//!
//! The acting view does not show what the opponent was shown of the actor's
//! own cards (a card revealed on top of the library and then drawn, a looked
//! at hand), so a copy the opponent knows differs from one it does not even
//! when the views tie. Such copies never merge: `opponent_known` lists them
//! from the engine's knowledge records. The actor saw those reveals happen,
//! so this reads nothing hidden from it.

use super::canon2::{as_multisets, swap_handles, tie_classes};
use serde_json::Value;
use std::collections::HashMap;

/// One exchange that merged two candidates: live indices `a` < `b` and the
/// exchanged handles `x`, `y` (`b` is `a` with `x` and `y` exchanged).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Merge {
    pub(crate) a: usize,
    pub(crate) b: usize,
    pub(crate) x: u64,
    pub(crate) y: u64,
}

fn min_zcc(v: &Value, out: &mut HashMap<u64, u64>) {
    match v {
        Value::Object(m) => {
            if let (Some(id), Some(z)) = (
                m.get("arena_id").and_then(Value::as_u64),
                m.get("zone_change_count").and_then(Value::as_u64),
            ) {
                let e = out.entry(id).or_insert(z);
                *e = (*e).min(z);
            }
            m.values().for_each(|x| min_zcc(x, out));
        }
        Value::Array(a) => a.iter().for_each(|x| min_zcc(x, out)),
        _ => {}
    }
}

/// Incarnation counters become offsets from the smallest shown for the same
/// object (absolute counters differ between copies with different pasts).
fn relative(v: &mut Value, min: &HashMap<u64, u64>) {
    match v {
        Value::Object(m) => {
            if let Some(id) = m.get("arena_id").and_then(Value::as_u64) {
                if let Some(z) = m.get("zone_change_count").and_then(Value::as_u64) {
                    let base = min.get(&id).copied().unwrap_or(z);
                    m.insert("zone_change_count".into(), Value::from(z - base));
                }
            }
            m.values_mut().for_each(|x| relative(x, min));
        }
        Value::Array(a) => a.iter_mut().for_each(|x| relative(x, min)),
        _ => {}
    }
}

fn find(parent: &mut [usize], i: usize) -> usize {
    let mut r = i;
    while parent[r] != r {
        r = parent[r];
    }
    parent[i] = r;
    r
}

/// Handles of the actor's cards that the opponent has been shown (hand and
/// library knowledge records).
pub(crate) fn opponent_known(
    s: &crate::rl_session::FastActorSessionV1,
    actor: crate::ids::PlayerId,
) -> Vec<u64> {
    let st = s.game_state();
    let opp = actor.opponent();
    st.known_hand_cards(opp, actor)
        .iter()
        .map(|e| u64::from(e.object.0))
        .chain(
            st.known_library_cards(opp, actor)
                .iter()
                .map(|e| u64::from(e.object.0)),
        )
        .collect()
}

/// Orbit id per live candidate (the smallest live index in its orbit) of a
/// prepared observation and menu, with the merging exchanges. Handles in
/// `known` (`opponent_known`) never exchange.
pub(crate) fn orbits(obs: &Value, menu: &[Value], known: &[u64]) -> (Vec<usize>, Vec<Merge>) {
    let mut parent: Vec<usize> = (0..menu.len()).collect();
    let mut merges = Vec::new();
    let classes = tie_classes(obs);
    if classes.is_empty() || menu.len() < 2 {
        return (parent, merges);
    }
    let mut min = HashMap::new();
    min_zcc(obs, &mut min);
    menu.iter().for_each(|m| min_zcc(m, &mut min));
    let mut rel_obs = obs.clone();
    relative(&mut rel_obs, &min);
    let rel_menu: Vec<Value> = menu
        .iter()
        .map(|m| {
            let mut m = m.clone();
            relative(&mut m, &min);
            m
        })
        .collect();
    let base = as_multisets(&rel_obs);
    for c in &classes {
        for (k, &x) in c.members.iter().enumerate() {
            for &y in &c.members[k + 1..] {
                if known.contains(&x) || known.contains(&y) {
                    continue;
                }
                let mut o = rel_obs.clone();
                swap_handles(&mut o, x, y);
                if as_multisets(&o) != base {
                    continue;
                }
                // The exchange must map the menu onto itself.
                let mut image = Vec::with_capacity(rel_menu.len());
                for m in &rel_menu {
                    let mut s = m.clone();
                    swap_handles(&mut s, x, y);
                    match rel_menu.iter().position(|n| *n == s) {
                        Some(j) => image.push(j),
                        None => break,
                    }
                }
                if image.len() != rel_menu.len() {
                    continue;
                }
                for (a, &b) in image.iter().enumerate() {
                    if a < b {
                        merges.push(Merge { a, b, x, y });
                        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                        if ra != rb {
                            parent[ra.max(rb)] = ra.min(rb);
                        }
                    }
                }
            }
        }
    }
    let ids = (0..menu.len()).map(|i| find(&mut parent, i)).collect();
    (ids, merges)
}

/// Lists whose order follows hand order (oldest first) or engine handles,
/// not the game: which of two copies was used changes only their order.
#[cfg(test)]
const ENGINE_ORDER_LISTS: [&str; 2] = ["remaining_choices", "paid_cost_refs"];

#[cfg(test)]
fn sort_engine_order_lists(v: &mut Value) {
    match v {
        Value::Object(m) => {
            for (k, x) in m.iter_mut() {
                sort_engine_order_lists(x);
                if ENGINE_ORDER_LISTS.contains(&k.as_str()) {
                    if let Value::Array(a) = x {
                        a.sort_by_cached_key(|e| e.to_string());
                    }
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(sort_engine_order_lists),
        _ => {}
    }
}

/// Whether view `b` is view `a` with handles `x` and `y` exchanged (each
/// view with relative incarnation counters; unordered lists, engine-order
/// lists and the menu as multisets). Test support for the engine check in
/// `tests.rs`.
#[cfg(test)]
pub(crate) fn same_up_to_exchange(
    a: (&Value, &[Value]),
    b: (&Value, &[Value]),
    x: u64,
    y: u64,
) -> bool {
    let norm = |obs: &Value, menu: &[Value], swap: bool| {
        let mut min = HashMap::new();
        min_zcc(obs, &mut min);
        menu.iter().for_each(|m| min_zcc(m, &mut min));
        let mut o = obs.clone();
        relative(&mut o, &min);
        let mut ms: Vec<String> = menu
            .iter()
            .map(|m| {
                let mut m = m.clone();
                relative(&mut m, &min);
                if swap {
                    swap_handles(&mut m, x, y);
                }
                m.to_string()
            })
            .collect();
        ms.sort_unstable();
        if swap {
            swap_handles(&mut o, x, y);
        }
        sort_engine_order_lists(&mut o);
        (as_multisets(&o), ms)
    };
    let (l, r) = (norm(a.0, a.1, false), norm(b.0, b.1, true));
    if l != r && std::env::var("INTERCHANGE_DEBUG").is_ok() {
        fn diff(p: &str, l: &Value, r: &Value) {
            match (l, r) {
                (Value::Object(a), Value::Object(b)) => {
                    for (k, v) in a {
                        match b.get(k) {
                            Some(w) => diff(&format!("{p}.{k}"), v, w),
                            None => eprintln!("{p}.{k}: only left"),
                        }
                    }
                    b.keys()
                        .filter(|k| !a.contains_key(*k))
                        .for_each(|k| eprintln!("{p}.{k}: only right"));
                }
                (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
                    for (i, (v, w)) in a.iter().zip(b).enumerate() {
                        diff(&format!("{p}[{i}]"), v, w);
                    }
                }
                _ if l != r => eprintln!("{p}: {l} != {r}"),
                _ => {}
            }
        }
        diff("obs", &l.0, &r.0);
        if l.1 != r.1 {
            eprintln!("menus differ:\n{:?}\n{:?}", l.1, r.1);
        }
    }
    l == r
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn card(id: u64, card: u64, zone: &str, zcc: u64) -> Value {
        json!({"stable":{"arena_id":id,"card_db_id":card,"owner":"p0","controller":"p0",
            "zone":zone,"zone_change_count":zcc},"tapped":false,"attachments":[]})
    }

    fn reference(id: u64, card: u64, zone: &str, zcc: u64) -> Value {
        json!({"arena_id":id,"card_db_id":card,"owner":"p0","controller":"p0","zone":zone,"zone_change_count":zcc})
    }

    fn play(id: u64, zcc: u64) -> Value {
        json!({"action_kind":"play_land","actor":"p0","source":reference(id, 7, "Hand", zcc)})
    }

    fn tap(id: u64, zcc: u64) -> Value {
        json!({"action_kind":"activate_mana_ability","actor":"p0","source":reference(id, 7, "Battlefield", zcc),"mana_choice":"R"})
    }

    fn view(hand: Vec<Value>, bf: Vec<Value>, known: Vec<Value>) -> Value {
        json!({"projection":{"battlefield":[bf, []],"stack":[]},"own_hand":hand,
            "known_hand_cards":[known, []]})
    }

    const PASS: &str = r#"{"action_kind":"pass","actor":"p0"}"#;

    fn pass() -> Value {
        serde_json::from_str(PASS).unwrap()
    }

    /// Two identical Mountains in hand (different engine pasts) and two
    /// identical untapped Mountains merge; the spell and pass stay alone.
    #[test]
    fn identical_copies_merge_into_one_orbit() {
        let obs = view(
            vec![
                card(11, 7, "Hand", 1),
                card(30, 9, "Hand", 1),
                card(12, 7, "Hand", 4),
            ],
            vec![card(20, 7, "Battlefield", 2), card(21, 7, "Battlefield", 2)],
            vec![],
        );
        let menu = vec![
            play(11, 1),
            json!({"action_kind":"cast_spell","actor":"p0","source":reference(30, 9, "Hand", 1)}),
            tap(20, 2),
            play(12, 4),
            tap(21, 2),
            pass(),
        ];
        let (ids, merges) = orbits(&obs, &menu, &[]);
        assert_eq!(ids, vec![0, 1, 2, 0, 2, 5]);
        assert!(merges.contains(&Merge {
            a: 0,
            b: 3,
            x: 11,
            y: 12
        }));
    }

    /// Visible differences keep copies apart: a tapped land, a copy the
    /// opponent has seen, a different card, or a stack target naming one.
    #[test]
    fn visible_differences_keep_copies_apart() {
        let lone = |ids: &[usize]| ids.iter().enumerate().all(|(i, &o)| i == o);
        // Tapped versus untapped.
        let obs = view(
            vec![],
            vec![card(20, 7, "Battlefield", 2), card(21, 7, "Battlefield", 2)],
            vec![],
        );
        let mut obs_tapped = obs.clone();
        obs_tapped["projection"]["battlefield"][0][1]["tapped"] = json!(true);
        assert!(lone(&orbits(&obs_tapped, &[tap(20, 2), pass()], &[]).0));
        // The opponent knows one copy.
        let hand = vec![card(11, 7, "Hand", 1), card(12, 7, "Hand", 1)];
        let seen = view(hand.clone(), vec![], vec![card(12, 7, "Hand", 1)]);
        assert!(lone(
            &orbits(&seen, &[play(11, 1), play(12, 1), pass()], &[]).0
        ));
        // A different card.
        let other = view(
            vec![card(11, 7, "Hand", 1), card(12, 8, "Hand", 1)],
            vec![],
            vec![],
        );
        assert!(lone(
            &orbits(&other, &[play(11, 1), play(12, 1), pass()], &[]).0
        ));
        // A stack item targets one copy.
        let mut targeted = view(hand, vec![], vec![]);
        targeted["projection"]["stack"] = json!([{"source":reference(40, 3, "Stack", 0),
            "targets":[{"object":reference(12, 7, "Hand", 1),"target_kind":"object"}]}]);
        assert!(lone(
            &orbits(&targeted, &[play(11, 1), play(12, 1), pass()], &[]).0
        ));
        // Sanity: without the target they merge.
        let plain = view(
            vec![card(11, 7, "Hand", 1), card(12, 7, "Hand", 1)],
            vec![],
            vec![],
        );
        assert_eq!(
            orbits(&plain, &[play(11, 1), play(12, 1), pass()], &[]).0,
            vec![0, 0, 2]
        );
        // The opponent was shown one copy (not in the acting view).
        assert!(lone(
            &orbits(&plain, &[play(11, 1), play(12, 1), pass()], &[12]).0
        ));
    }

    /// An exchange that is a symmetry of the view but not of the menu (a
    /// candidate names one copy with no counterpart) merges nothing.
    #[test]
    fn asymmetric_menu_merges_nothing() {
        let obs = view(
            vec![card(11, 7, "Hand", 1), card(12, 7, "Hand", 1)],
            vec![],
            vec![],
        );
        let (ids, merges) = orbits(&obs, &[play(11, 1), pass()], &[]);
        assert_eq!(ids, vec![0, 1]);
        assert!(merges.is_empty());
    }
}
