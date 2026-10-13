//! Conservative continuation guidance after commuting basic-land taps.
//!
//! Incoming history nodes and their execution-support counts stay separate.
//! A pool contains only fresh natural-terminal samples, never replayed wins.
//! Sharing ends at the tap run's endpoint: no history after casting, targeting,
//! passing, an opponent transition, or a forced action is normalized.

use super::play::{acting, PlayErr};
use super::tree::{raw_decision, Canon, Key, Node};
use crate::card_def::CARD_DEFS;
use crate::rl_session::{FastActorDecisionKindV1, FastActorDecisionV1, FastActorSessionV1};
use crate::state::{Counters, Step};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

const MAX_BYTES: usize = 64 << 20;
const MAX_CANDIDATES: usize = 65_536;

#[derive(Default)]
pub(crate) struct Sharing {
    groups: HashMap<Key, Group>,
    candidates: HashMap<Key, Key>,
    bytes: usize,
    probes: u64,
    duplicate_views: u64,
    rejected_duplicate_views: u64,
    eligible_endpoints: u64,
    cross_history_endpoints: u64,
    guidance_hits: u64,
    guidance_changes: u64,
    fresh_samples: u64,
    duplicate_samples: u64,
    capacity_rejections: u64,
    rejected: BTreeMap<&'static str, u64>,
    seconds: f64,
}

struct Group {
    identity: Vec<u8>,
    histories: HashSet<Key>,
    n: Vec<u64>,
    wins: Vec<u64>,
    // Simulations arrive monotonically within this one search. Remembering
    // the last ID is enough to refuse repeat or out-of-order sample credit.
    last_sample: Option<u64>,
}

#[derive(Default)]
pub(crate) struct Cursor {
    run: Option<TapRun>,
    pub(crate) path: Vec<(Key, Key, usize)>,
}

struct TapRun {
    anchor: Key,
    start: Vec<u8>,
    taps: Vec<Vec<u8>>,
    last_transition: u64,
}

pub(crate) struct Probe {
    history: Key,
    view: Vec<u8>,
    menu: Vec<Value>,
    safe: bool,
    pub(crate) group: Option<Key>,
}

fn hash(parts: &[&[u8]]) -> Key {
    let mut h = Sha256::new();
    for p in parts {
        h.update((p.len() as u64).to_le_bytes());
        h.update(p);
    }
    h.finalize().into()
}

fn basic(name: &str) -> bool {
    matches!(name, "Plains" | "Island" | "Swamp" | "Mountain" | "Forest")
}

/// Reads public battlefield cards and public pending/priority status only.
/// In particular, no hidden object, library order, sampled deck identity,
/// opponent private hand, or private RNG enters either a key or a guard.
fn guard(s: &FastActorSessionV1, d: FastActorDecisionV1) -> Result<(), &'static str> {
    let st = s.game_state();
    let e = &st.engine;
    if e.pending_cast.is_some()
        || e.pending_activation.is_some()
        || e.pending_discard.is_some()
        || e.pending_optional_cost.is_some()
        || e.pending_optional_cost_sacrifice.is_some()
        || e.pending_spell_copy.is_some()
        || e.pending_effect.is_some()
        || e.pending_land_play.is_some()
        || e.pending_kicked_source.is_some()
        || !e.pending_triggers.is_empty()
        || st.pending_legend_rule_v1.is_some()
        || st.legend_pending_v1.as_ref().is_some_and(|v| !v.is_empty())
        || st
            .wildwood_waiting_v1
            .as_ref()
            .is_some_and(|v| !v.is_empty())
        || st.raid_waiting_v1.as_ref().is_some_and(|v| !v.is_empty())
    {
        return Err("pending_context");
    }
    if d.decision_kind != FastActorDecisionKindV1::Surface
        || d.substep_index != 0
        || d.substep_count != 1
        || !matches!(st.step, Step::Main1 | Step::Main2)
        || st.priority_player != acting(&d)
        || !st.stack.is_empty()
    {
        return Err("priority_context");
    }
    if !e.active_replacements.is_empty()
        || !e.until_end_of_turn.is_empty()
        || !e.until_next_turn_keywords.is_empty()
        || st.initiative.is_some()
        || st.monarch.is_some()
        || !st.exile.is_empty()
        || !st.command.is_empty()
        || st
            .players
            .iter()
            .any(|p| !p.graveyard.is_empty() || !p.restricted_mana_pool.0.is_empty())
    {
        return Err("effects_or_public_zones");
    }
    for id in st.players.iter().flat_map(|p| &p.battlefield) {
        let o = st.objects.get(*id);
        // Test face-down status before reading the concealed definition.
        if o.v4.face_down_v1.is_some() || !basic(CARD_DEFS[o.card_def as usize].name) {
            return Err("nonbasic_board");
        }
        if o.damage != 0
            || o.counters != Counters::default()
            || !o.attachments.is_empty()
            || o.v4.attached_to.is_some()
            || o.v4.chosen_color.is_some()
            || o.v4.face_index != 0
            || o.v4.is_token
            || o.v4.skip_next_untap
        {
            return Err("modified_land");
        }
    }
    Ok(())
}

/// Exact raw actor information and live menu. Only physical step counters
/// and their derived projection hash are removed; handles, incarnation
/// counters, timestamps, all ordered lists and surface context remain exact.
fn view(mut obs: Value, menu: &[Value], cn: &Canon) -> Result<Vec<u8>, PlayErr> {
    if let Value::Object(m) = &mut obs {
        for k in [
            "step_index",
            "physical_decision_id",
            "visible_projection_hash",
        ] {
            m.remove(k);
        }
    }
    serde_json::to_vec(&(obs, menu, &cn.obs, &cn.menu, &cn.edges))
        .map_err(|e| PlayErr::Fault(e.to_string()))
}

impl Sharing {
    pub(crate) fn probe(
        &mut self,
        cursor: &mut Cursor,
        s: &FastActorSessionV1,
        d: FastActorDecisionV1,
        cn: &Canon,
        history: Key,
        depth: u32,
        transitions: u64,
    ) -> Result<Probe, PlayErr> {
        let started = Instant::now();
        self.probes += 1;
        let (obs, menu) = raw_decision(s, d)?;
        let view = view(obs, &menu, cn)?;
        let view_key = hash(&[b"stage4a-sharing-visible-v1", &view]);
        let duplicate_view = self
            .candidates
            .get(&view_key)
            .is_some_and(|first| *first != history);
        if self.candidates.contains_key(&view_key) {
            self.duplicate_views += u64::from(duplicate_view);
        } else if self.candidates.len() < MAX_CANDIDATES {
            self.candidates.insert(view_key, history);
        }
        let safe = match guard(s, d) {
            Ok(()) => true,
            Err(reason) => {
                *self.rejected.entry(reason).or_default() += 1;
                cursor.run = None;
                false
            }
        };
        // A forced or opponent transition must break a run even if no policy
        // call occurred. Exact consecutive transition positions enforce it.
        if cursor
            .run
            .as_ref()
            .is_some_and(|r| r.last_transition != transitions)
        {
            *self.rejected.entry("intervening_transition").or_default() += 1;
            cursor.run = None;
        }
        let mut group = None;
        if let Some(run) = cursor.run.as_ref().filter(|r| safe && r.taps.len() >= 2) {
            self.eligible_endpoints += 1;
            let mut taps = run.taps.clone();
            taps.sort();
            let identity =
                serde_json::to_vec(&(run.anchor, &run.start, taps, depth, transitions, &view))
                    .map_err(|e| PlayErr::Fault(e.to_string()))?;
            let key = hash(&[b"stage4a-basic-tap-endpoint-v1", &identity]);
            let g = match self.groups.entry(key) {
                std::collections::hash_map::Entry::Occupied(entry)
                    if entry.get().identity != identity =>
                {
                    return Err(PlayErr::Fault("continuation-sharing key collision".into()));
                }
                std::collections::hash_map::Entry::Occupied(entry) => {
                    let g = entry.into_mut();
                    if !g.histories.contains(&history) {
                        if self.bytes + 64 <= MAX_BYTES {
                            g.histories.insert(history);
                            self.bytes += 64;
                            self.cross_history_endpoints += 1;
                        } else {
                            self.capacity_rejections += 1;
                        }
                    }
                    Some(g)
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    let bytes = identity.len() + cn.edges.len() * 16 + 256;
                    if self.bytes + bytes <= MAX_BYTES {
                        self.bytes += bytes;
                        Some(entry.insert(Group {
                            identity,
                            histories: HashSet::from([history]),
                            n: vec![0; cn.edges.len()],
                            wins: vec![0; cn.edges.len()],
                            last_sample: None,
                        }))
                    } else {
                        self.capacity_rejections += 1;
                        None
                    }
                }
            };
            if g.is_some_and(|g| g.histories.contains(&history)) {
                group = Some(key);
            }
        }
        self.rejected_duplicate_views += u64::from(duplicate_view && group.is_none());
        self.seconds += started.elapsed().as_secs_f64();
        Ok(Probe {
            history,
            view,
            menu,
            safe,
            group,
        })
    }

    /// Returns pooled guidance only after a second incoming history exists.
    /// The transient Node copy is used solely for UCT selection. The stored
    /// node, frozen choice and eight-backup support check remain untouched.
    pub(crate) fn select(
        &mut self,
        probe: &Probe,
        node: &Node,
        urgency: Option<f64>,
    ) -> Option<usize> {
        let g = self.groups.get(&probe.group?)?;
        if g.histories.len() < 2 || g.n.iter().all(|&n| n == 0) {
            return None;
        }
        let mut guidance = node.clone();
        guidance.visits = g.n.iter().sum();
        guidance.n.clone_from(&g.n);
        guidance.wins.clone_from(&g.wins);
        let selected = guidance.select_with(urgency);
        self.guidance_hits += 1;
        self.guidance_changes += u64::from(selected != node.select_with(urgency));
        Some(selected)
    }

    /// Once per group per fresh completed simulation, even if a malformed
    /// cyclic path revisits it on another edge. No backup is copied into any
    /// incoming node; `Tree::backup` independently credits the actual path.
    pub(crate) fn backup(&mut self, cursor: &Cursor, sample: u64, win: bool) {
        let started = Instant::now();
        for &(key, _, edge) in &cursor.path {
            let g = self.groups.get_mut(&key).expect("probed sharing group");
            if g.last_sample.is_some_and(|last| sample <= last) {
                self.duplicate_samples += 1;
                continue;
            }
            g.last_sample = Some(sample);
            g.n[edge] += 1;
            g.wins[edge] += u64::from(win);
            self.fresh_samples += 1;
        }
        self.seconds += started.elapsed().as_secs_f64();
    }

    pub(crate) fn json(&self) -> Value {
        json!({"contract":"commuting-basic-tap-endpoints-v1","probes":self.probes,
            "duplicate_visible_views":self.duplicate_views,"eligible_endpoints":self.eligible_endpoints,
            "unproven_duplicate_views":self.rejected_duplicate_views,
            "cross_history_endpoints":self.cross_history_endpoints,"guidance_hits":self.guidance_hits,
            "guidance_changes":self.guidance_changes,"fresh_pool_samples":self.fresh_samples,
            "duplicate_sample_rejections":self.duplicate_samples,"capacity_rejections":self.capacity_rejections,
            "rejected":self.rejected,"groups":self.groups.len(),"estimated_pool_bytes":self.bytes,
            "candidate_views":self.candidates.len(),"pool_byte_cap":MAX_BYTES,"seconds":self.seconds,
            "cached_terminal_samples":0,"local_support_increments_from_reuse":0})
    }
}

impl Cursor {
    pub(crate) fn advance(&mut self, probe: Probe, live: u32, edge: usize, transitions: u64) {
        if let Some(group) = probe.group {
            self.path.push((group, probe.history, edge));
        }
        let m = &probe.menu[live as usize];
        let pure_tap = probe.safe
            && m["action_kind"] == "activate_mana_ability"
            && m.get("cost_target").is_none_or(Value::is_null);
        if !pure_tap {
            self.run = None;
            return;
        }
        let tap = serde_json::to_vec(m).expect("JSON action serializes");
        let run = self.run.get_or_insert_with(|| TapRun {
            anchor: probe.history,
            start: probe.view,
            taps: Vec::new(),
            last_transition: transitions,
        });
        if run.taps.contains(&tap) {
            // A basic land cannot be tapped twice without an intervening
            // transition. Refuse an unexpected cycle instead of pooling it.
            self.run = None;
            return;
        }
        run.taps.push(tap);
        run.last_transition = transitions + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::super::tree::{canon, child_key};
    use super::*;
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::rl::ActionSemanticV1;
    use crate::state::Zone;

    fn root() -> FastActorSessionV1 {
        let mut st = ready_state();
        put(&mut st, PlayerId::P0, "Mountain", Zone::Battlefield);
        put(&mut st, PlayerId::P0, "Swamp", Zone::Battlefield);
        put(&mut st, PlayerId::P0, "Lightning Bolt", Zone::Hand);
        for _ in 0..10 {
            put(&mut st, PlayerId::P1, "Island", Zone::Library);
        }
        FastActorSessionV1::from_v3_fixture_state(st)
    }

    fn endpoint(
        pools: &mut Sharing,
        mut session: FastActorSessionV1,
        order: &[&str],
        final_depth: u32,
    ) -> (FastActorSessionV1, Probe, Cursor) {
        let mut cursor = Cursor::default();
        let mut parent = ([0; 32], b"root".to_vec());
        for (i, &name) in order.iter().enumerate() {
            let d = super::super::play::decision(&session).unwrap();
            let cn = canon(&session, d).unwrap();
            let history = child_key(&parent.0, &parent.1, &cn);
            let probe = pools
                .probe(
                    &mut cursor,
                    &session,
                    d,
                    &cn,
                    history,
                    i as u32 + 1,
                    i as u64,
                )
                .unwrap();
            let live = session
                .diagnostic_current_action_semantics()
                .unwrap()
                .iter()
                .position(|s| {
                    matches!(s, ActionSemanticV1::ActivateManaAbility { source, .. }
                    if CARD_DEFS[source.card_db_id as usize].name == name)
                })
                .unwrap();
            let edge = cn.edges.iter().position(|e| *e == cn.menu[live]).unwrap();
            parent = (history, cn.menu[live].clone());
            cursor.advance(probe, live as u32, edge, i as u64);
            session.step(d.episode_id, d.step, live as u32).unwrap();
        }
        let d = super::super::play::decision(&session).unwrap();
        let cn = canon(&session, d).unwrap();
        let history = child_key(&parent.0, &parent.1, &cn);
        let probe = pools
            .probe(
                &mut cursor,
                &session,
                d,
                &cn,
                history,
                final_depth,
                order.len() as u64,
            )
            .unwrap();
        (session, probe, cursor)
    }

    #[test]
    fn different_color_basic_taps_share_only_the_endpoint() {
        let mut pools = Sharing::default();
        let (a, pa, _) = endpoint(&mut pools, root(), &["Mountain", "Swamp"], 3);
        let (b, pb, _) = endpoint(&mut pools, root(), &["Swamp", "Mountain"], 3);
        assert!(pa.group.is_some(), "{:?}", pools.json());
        assert_eq!(pa.group, pb.group);
        assert_ne!(pa.history, pb.history);
        assert_eq!(
            a.game_state().players[0].mana_pool,
            b.game_state().players[0].mana_pool
        );
        assert_eq!(pa.view, pb.view);
        let (_, deeper, _) = endpoint(&mut pools, root(), &["Swamp", "Mountain"], 4);
        assert_ne!(pa.group, deeper.group, "remaining search depth must agree");
        let (_, single, _) = endpoint(&mut pools, root(), &["Mountain"], 2);
        assert!(single.group.is_none());
    }

    #[test]
    fn hidden_world_changes_do_not_condition_the_key_or_guard() {
        let mut pools = Sharing::default();
        let original = root();
        let changed = original.census_edited_clone_v1(|st| {
            let def = CARD_DEFS.iter().position(|c| c.name == "Forest").unwrap() as u16;
            for &id in &st.players[1].library {
                st.objects.get_mut(id).card_def = def;
            }
            st.players[1].library.reverse();
        });
        let (_, a, _) = endpoint(&mut pools, original, &["Mountain", "Swamp"], 3);
        let (_, b, _) = endpoint(&mut pools, changed, &["Swamp", "Mountain"], 3);
        assert_eq!(a.group, b.group);
        assert_eq!(a.view, b.view);
    }

    #[test]
    fn pending_targets_and_nonbasic_boards_are_rejected() {
        let mut pools = Sharing::default();
        let (mut session, _, mut cursor) = endpoint(&mut pools, root(), &["Mountain", "Swamp"], 3);
        let d = super::super::play::decision(&session).unwrap();
        let cast = session
            .diagnostic_current_action_semantics()
            .unwrap()
            .iter()
            .position(|s| matches!(s, ActionSemanticV1::CastSpell { .. }))
            .unwrap();
        session.step(d.episode_id, d.step, cast as u32).unwrap();
        let d = super::super::play::decision(&session).unwrap();
        assert!(session.game_state().engine.pending_cast.is_some());
        let cn = canon(&session, d).unwrap();
        let probe = pools
            .probe(&mut cursor, &session, d, &cn, [8; 32], 4, 3)
            .unwrap();
        assert!(probe.group.is_none());
        assert_eq!(pools.rejected["pending_context"], 1);
        let altered = root().census_edited_clone_v1(|st| {
            put(st, PlayerId::P0, "Balustrade Spy", Zone::Battlefield);
        });
        assert_eq!(
            guard(&altered, super::super::play::decision(&altered).unwrap()),
            Err("nonbasic_board")
        );
    }

    #[test]
    fn pooled_samples_never_fabricate_local_support_or_cycle_credit() {
        let mut pools = Sharing::default();
        let (_, pa, _) = endpoint(&mut pools, root(), &["Mountain", "Swamp"], 3);
        let (s, pb, _) = endpoint(&mut pools, root(), &["Swamp", "Mountain"], 3);
        let key = pa.group.unwrap();
        let cn = canon(&s, super::super::play::decision(&s).unwrap()).unwrap();
        let node = Node::with_perm(
            [0; 32],
            vec![],
            cn.clone(),
            3,
            (0..cn.edges.len()).collect(),
        );
        let cursor = Cursor {
            run: None,
            path: vec![(key, pa.history, 0), (key, pb.history, 1)],
        };
        for sample in 0..8 {
            pools.backup(&cursor, sample, true);
        }
        pools.backup(&cursor, 7, true);
        let group = &pools.groups[&key];
        assert_eq!(group.n[0], 8);
        assert_eq!(group.n[1], 0);
        assert_eq!(pools.fresh_samples, 8);
        assert!(pools.duplicate_samples >= 8);
        assert!(pools.select(&pb, &node, None).is_some());
        assert_eq!(node.n, vec![0; cn.edges.len()]);
        assert_eq!(node.frozen_choice(), None);
    }

    #[test]
    fn interrupted_runs_and_memory_limits_do_not_weaken_equivalence() {
        let mut pools = Sharing::default();
        let (s, _, mut cursor) = endpoint(&mut pools, root(), &["Mountain", "Swamp"], 3);
        let d = super::super::play::decision(&s).unwrap();
        let cn = canon(&s, d).unwrap();
        let interrupted = pools.probe(&mut cursor, &s, d, &cn, [9; 32], 3, 3).unwrap();
        assert!(interrupted.group.is_none());
        assert_eq!(pools.rejected["intervening_transition"], 1);
        let mut full = Sharing {
            bytes: MAX_BYTES,
            ..Sharing::default()
        };
        let (_, p, _) = endpoint(&mut full, root(), &["Mountain", "Swamp"], 3);
        assert!(p.group.is_none());
        assert_eq!(full.capacity_rejections, 1);
    }
}
