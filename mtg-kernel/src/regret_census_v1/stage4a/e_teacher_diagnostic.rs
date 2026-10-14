//! Teacher diagnostic (collab LANES/search-policy-target-audit-20261012,
//! DESIGN.md "Teacher diagnostic"): at one fixed probe root, the frozen
//! teacher's bound E choice against the parent's sampled raw choice, each
//! continued by identical frozen plain policies on fresh lawful worlds.
//!
//! For world index `w` of probe `p` (seeds from `seed(p, w, role)`, a
//! namespace separate from E's own search seeds):
//! 1. Raw choice: the parent's deployment sampler at the actual replayed root,
//!    seat streams reset to roles `root-raw/0` and `root-raw/1`. It is drawn
//!    before, and independently of, the world sample.
//! 2. World: one `world::sample` draw (role `world`) from the actual root with
//!    E's public prior conditioned on the same visible history
//!    (`e_supported::public_prior`). A rejection is recorded and counted; it
//!    is never retried or replaced.
//! 3. Branch A plays `teacher_live`, branch B plays the raw choice, each on a
//!    clone of that world. Both then continue with the plain policies (parent
//!    for the focal seat, the episode's frozen opponent for the other). Before
//!    each branch both policies' seat streams are reset to the same roles
//!    (`continuation/focal/<seat>`, `continuation/opponent/<seat>`), so the
//!    k-th draw of a role comes from the same stream position in either
//!    branch. Forced decisions take their only action without a draw.
//!
//! Each suffix runs to a natural terminal, a non-natural engine end, or the
//! transition cap (the root action counts against it). Search and resets are
//! off. An instrument fault (an engine step error or a sampled world whose
//! root decision or visible menu differs from the actual root) fails the
//! whole diagnostic closed. Nothing here is filtered by outcome.

use super::e_supported::{public_prior, VisibleFrame};
use super::labels::{spy_defs, SpyDefs};
use super::play::{act, acting, apply, decision, Counters, Meter, PlayErr};
use super::tree::raw_decision;
use super::world::{sample, SAMPLER_VERSION};
use crate::ids::PlayerId;
use crate::rl::{PlayerSeatV1, TerminalClassificationV1};
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub(crate) const METHOD: &str = "e-teacher-diagnostic-v1-20261012";
pub(crate) const FORMAL_WORLDS: u64 = 8;
pub(crate) const FORMAL_MAX_TRANSITIONS: u64 = 2048;

/// Seed for one role of world `world` at probe `probe_id`: SHA-256 of the
/// compact JSON key, first eight digest bytes little-endian, top bit cleared.
pub(crate) fn seed(probe_id: &str, world: u64, role: &str) -> u64 {
    let key = json!({"namespace":METHOD,"probe_id":probe_id,"role":role,"world":world});
    let digest = Sha256::digest(serde_json::to_vec(&key).expect("seed JSON"));
    u64::from_le_bytes(digest[..8].try_into().expect("eight bytes")) & i64::MAX as u64
}

fn seat_seeds(probe_id: &str, world: u64, role: &str) -> [u64; 2] {
    [
        seed(probe_id, world, &format!("{role}/0")),
        seed(probe_id, world, &format!("{role}/1")),
    ]
}

/// Win, draw and loss counts of natural endings.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub(crate) struct Wdl {
    pub(crate) win: u64,
    pub(crate) draw: u64,
    pub(crate) loss: u64,
}

/// Every attempted unit of work, and the paired signed result with its
/// worst/best bounds over unresolved (rejected, truncated or non-natural)
/// suffixes.
#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct Totals {
    pub(crate) probes: u64,
    pub(crate) worlds_attempted: u64,
    pub(crate) worlds_rejected: u64,
    pub(crate) suffixes_attempted: u64,
    pub(crate) suffixes_truncated: u64,
    pub(crate) suffixes_nonnatural: u64,
    pub(crate) transitions: u64,
    pub(crate) inference_calls: u64,
    pub(crate) root_raw_draws: u64,
    pub(crate) first_actions_coincide: u64,
    pub(crate) teacher: Wdl,
    pub(crate) raw: Wdl,
    /// Worlds where both suffixes ended naturally.
    pub(crate) paired_resolved: u64,
    /// Sum over resolved pairs of teacher result minus raw result.
    pub(crate) paired_signed_sum: i64,
    /// Sum over every attempted world of the least and greatest possible
    /// teacher-minus-raw result (an unresolved suffix ranges over -1..1; a
    /// rejected world over -2..2).
    pub(crate) paired_signed_lower: i64,
    pub(crate) paired_signed_upper: i64,
}

impl Totals {
    pub(crate) fn add(&mut self, o: &Self) {
        self.probes += o.probes;
        self.worlds_attempted += o.worlds_attempted;
        self.worlds_rejected += o.worlds_rejected;
        self.suffixes_attempted += o.suffixes_attempted;
        self.suffixes_truncated += o.suffixes_truncated;
        self.suffixes_nonnatural += o.suffixes_nonnatural;
        self.transitions += o.transitions;
        self.inference_calls += o.inference_calls;
        self.root_raw_draws += o.root_raw_draws;
        self.first_actions_coincide += o.first_actions_coincide;
        for (a, b) in [(&mut self.teacher, &o.teacher), (&mut self.raw, &o.raw)] {
            a.win += b.win;
            a.draw += b.draw;
            a.loss += b.loss;
        }
        self.paired_resolved += o.paired_resolved;
        self.paired_signed_sum += o.paired_signed_sum;
        self.paired_signed_lower += o.paired_signed_lower;
        self.paired_signed_upper += o.paired_signed_upper;
    }

    fn count(&mut self, branch: &Branch, teacher: bool) {
        self.suffixes_attempted += 1;
        self.transitions += branch.transitions;
        self.inference_calls += branch.inference_calls;
        self.suffixes_truncated += u64::from(branch.truncated);
        self.suffixes_nonnatural += u64::from(branch.end == "nonnatural");
        let wdl = if teacher {
            &mut self.teacher
        } else {
            &mut self.raw
        };
        match branch.result {
            Some(1) => wdl.win += 1,
            Some(0) => wdl.draw += 1,
            Some(_) => wdl.loss += 1,
            None => {}
        }
    }
}

/// One suffix: the applied root action, then plain play.
#[derive(Clone, Debug, Serialize)]
struct Branch {
    /// `win`, `draw`, `loss`, `nonnatural` or `truncated`.
    end: &'static str,
    /// Focal result of a natural ending: +1, 0 or -1.
    result: Option<i8>,
    truncated: bool,
    transitions: u64,
    inference_calls: u64,
    /// The first action actually applied at the root.
    first_action: u32,
    /// SHA-256 over every applied (step, actor seat, action) of the suffix.
    actions_sha256: String,
    final_state_hash: String,
}

fn seat_byte(seat: PlayerSeatV1) -> u8 {
    match seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
}

#[allow(clippy::too_many_arguments, reason = "explicit policy roles and seeds")]
fn branch(
    world: &FastActorSessionV1,
    first: u32,
    focal: PlayerId,
    parent: &mut FrozenPlayPolicyV1,
    opponent: &mut FrozenPlayPolicyV1,
    seeds: &([u64; 2], [u64; 2]),
    cap: u64,
    defs: &SpyDefs,
) -> Result<Branch, String> {
    let mut s = world.clone();
    parent.reset_sampling_v1(seeds.0);
    opponent.reset_sampling_v1(seeds.1);
    let mut meter = Meter::new(cap);
    let mut counters = Counters::default();
    let mut hash = Sha256::new();
    let mut next = Some(first);
    let mut applied_first = None;
    let truncated = loop {
        let Some(d) = decision(&s) else {
            break false;
        };
        let a = match next.take() {
            Some(a) => a,
            None if acting(&d) == focal => {
                act(parent, &s, &d, &mut counters).map_err(|e| format!("{e:?}"))?
            }
            None => act(opponent, &s, &d, &mut counters).map_err(|e| format!("{e:?}"))?,
        };
        match apply(&mut s, &d, a, &mut meter, focal, None, defs) {
            Ok(()) => {}
            Err(PlayErr::Truncated) => break true,
            Err(PlayErr::Fault(e)) => return Err(format!("teacher diagnostic fault: {e}")),
        }
        applied_first.get_or_insert(a);
        hash.update(d.step.to_le_bytes());
        hash.update([seat_byte(d.acting_player)]);
        hash.update(a.to_le_bytes());
    };
    let (end, result) = match (truncated, s.current_response()) {
        (true, _) => ("truncated", None),
        (false, FastActorResponseV1::Terminal(t)) => {
            if t.terminal_classification != TerminalClassificationV1::Natural {
                ("nonnatural", None)
            } else {
                match t.winner {
                    None => ("draw", Some(0)),
                    Some(w) if w == PlayerSeatV1::from(focal) => ("win", Some(1)),
                    Some(_) => ("loss", Some(-1)),
                }
            }
        }
        (false, FastActorResponseV1::Decision(_)) => unreachable!("loop ends at a terminal"),
    };
    Ok(Branch {
        end,
        result,
        truncated,
        transitions: meter.spent,
        inference_calls: counters.inference,
        first_action: applied_first.ok_or("teacher diagnostic root action was not applied")?,
        actions_sha256: super::hex(&hash.finalize()),
        final_state_hash: format!("{:016x}", s.diagnostic_state_hash()),
    })
}

fn range(branch: &Branch) -> (i64, i64) {
    branch
        .result
        .map_or((-1, 1), |r| (i64::from(r), i64::from(r)))
}

/// All worlds of one probe. `root` is the actual replayed session at the
/// probe's learner row and `history` its actor-visible prefix (E's).
#[allow(
    clippy::too_many_arguments,
    reason = "explicit probe, roles and limits"
)]
pub(crate) fn probe(
    probe_id: &str,
    root: &FastActorSessionV1,
    history: &[VisibleFrame],
    teacher_live: usize,
    parent: &FrozenPlayPolicyV1,
    opponent: &FrozenPlayPolicyV1,
    prior_indices: &[usize],
    worlds: u64,
    max_transitions: u64,
) -> Result<(Value, Totals), String> {
    let d = decision(root).ok_or("teacher diagnostic root is terminal")?;
    let width = d.legal_action_count as usize;
    if width < 2 || teacher_live >= width {
        return Err("teacher diagnostic root is forced or teacher_live is outside it".into());
    }
    let focal = acting(&d);
    let defs = spy_defs();
    let prior = public_prior(root, d, history, prior_indices)?;
    let (_, root_menu) = raw_decision(root, d).map_err(|e| format!("{e:?}"))?;
    let mut drawer = parent.fork_for_collection_v3()?;
    let mut focal_policy = parent.fork_for_collection_v3()?;
    let mut opponent_policy = opponent.fork_for_collection_v3()?;
    let mut totals = Totals {
        probes: 1,
        ..Totals::default()
    };
    let mut rows = Vec::new();
    for w in 0..worlds {
        drawer.reset_sampling_v1(seat_seeds(probe_id, w, "root-raw"));
        let raw = drawer.select_fast_session_v1(root)?;
        totals.root_raw_draws += 1;
        totals.inference_calls += 1;
        totals.worlds_attempted += 1;
        let coincide = raw as usize == teacher_live;
        totals.first_actions_coincide += u64::from(coincide);
        let world_seed = seed(probe_id, w, "world");
        let mut row = json!({"world":w,"world_seed":world_seed,"teacher_live":teacher_live,
            "raw_choice":raw,"first_actions_coincide":coincide,
            "raw_choice_semantics":root_menu[raw as usize]});
        let sampled = match sample(root, world_seed, &prior) {
            Err(e) => {
                totals.worlds_rejected += 1;
                totals.paired_signed_lower -= 2;
                totals.paired_signed_upper += 2;
                row["rejected"] = json!(true);
                row["rejection"] = json!(e.chars().take(200).collect::<String>());
                rows.push(row);
                continue;
            }
            Ok(x) => x,
        };
        let world = sampled.world;
        let wd = decision(&world).ok_or("teacher diagnostic sampled world is terminal")?;
        let (_, world_menu) = raw_decision(&world, wd).map_err(|e| format!("{e:?}"))?;
        if wd.step != d.step
            || wd.physical_decision_id != d.physical_decision_id
            || wd.substep_index != d.substep_index
            || wd.legal_action_count != d.legal_action_count
            || acting(&wd) != focal
            || world_menu != root_menu
        {
            return Err(
                "teacher diagnostic sampled world root differs from the actual root".into(),
            );
        }
        let continuation = (
            seat_seeds(probe_id, w, "continuation/focal"),
            seat_seeds(probe_id, w, "continuation/opponent"),
        );
        let mut run = |first| {
            branch(
                &world,
                first,
                focal,
                &mut focal_policy,
                &mut opponent_policy,
                &continuation,
                max_transitions,
                &defs,
            )
        };
        let a = run(teacher_live as u32)?;
        let b = run(raw)?;
        totals.count(&a, true);
        totals.count(&b, false);
        if let (Some(x), Some(y)) = (a.result, b.result) {
            totals.paired_resolved += 1;
            totals.paired_signed_sum += i64::from(x) - i64::from(y);
        }
        let ((alo, ahi), (blo, bhi)) = (range(&a), range(&b));
        totals.paired_signed_lower += alo - bhi;
        totals.paired_signed_upper += ahi - blo;
        row["rejected"] = json!(false);
        row["prior_deck"] = json!(prior.ids()[sampled.prior_deck]);
        row["world_state_hash"] = json!(format!("{:016x}", world.diagnostic_state_hash()));
        row["teacher"] = serde_json::to_value(&a).map_err(|e| e.to_string())?;
        row["raw"] = serde_json::to_value(&b).map_err(|e| e.to_string())?;
        rows.push(row);
    }
    let summary = json!({"probe_id":probe_id,"teacher_live":teacher_live,
        "teacher_live_semantics":root_menu[teacher_live],"legal_width":width,
        "focal_seat":focal.index(),"sampler":SAMPLER_VERSION,"public_prior":prior.ids(),
        "worlds":rows,"totals":totals});
    Ok((summary, totals))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn e_teacher_diagnostic_seeds_are_separated_by_role_world_probe_and_from_e() {
        let a = seed("p", 0, "world");
        assert_eq!(a, seed("p", 0, "world"));
        assert_ne!(a, seed("p", 1, "world"));
        assert_ne!(a, seed("q", 0, "world"));
        assert_ne!(a, seed("p", 0, "root-raw/0"));
        let e = super::super::seeds::RootSeeds {
            model: super::super::e_supported::METHOD.into(),
            root: "p".into(),
        };
        assert_ne!(
            a,
            e.get(super::super::seeds::Purpose::Select, "", 0, b"world")
        );
        assert!(a <= i64::MAX as u64);
    }
}
