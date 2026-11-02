//! W5's search-target recipe `w5-xact-v1` (collab LANES/lead/W5-DESIGN.md
//! v1.3 section 3): the pure parts the collection hook and the update share.
//! It generalizes EX1's (`ex1_v1`) and keeps EX1's target, `ex1-target-v1`,
//! unchanged.
//!
//! - **Eligibility:** a learner decision with at least two candidates, at
//!   least two of them not mana abilities (W1's rule).
//! - **Placement:** one draw per learner seat-episode,
//!   `k = 1 + floor(128 u)` with `u` from
//!   `SHA256(json(["w5/placement", chain, update, seed, episode_slot]))`; the
//!   k-th eligible decision is searched if the episode reaches it.
//! - **Acting:** with a target, the played action is drawn from q by the
//!   inverse CDF at `u` from `SHA256(json(["w5/act", chain, update, seed,
//!   episode_slot]))`, independent of the placement draw.
//!
//! The Python mirror is `tools/w5/xact_ref.py`.

use crate::ex1_v1::{ex1_hash_u64_v1, ex1_uniform_v1};
use crate::rl::ActionSemanticV1;

pub const W5_XACT_VERSION_V1: &str = "w5-xact-v1";
/// Indices above this are never searched (truncated coverage).
pub const W5_PLACEMENT_MAX_V1: u64 = 128;
/// The paired Adam-step comparison runs on updates whose global index is a
/// multiple of this.
pub const W5_TELEMETRY_STRIDE_V1: u64 = 16;
/// The largest admitted search budget B (raw selection transitions).
pub const W5_BUDGET_MAX_V1: u64 = 1_048_576;

/// W1's eligibility: at least two candidates that are not mana abilities
/// (which implies at least two candidates).
pub fn w5_eligible_v1(menu: &[ActionSemanticV1]) -> bool {
    menu.iter()
        .filter(|a| !matches!(a, ActionSemanticV1::ActivateManaAbility { .. }))
        .count()
        >= 2
}

/// The placement hash of one learner seat-episode.
pub fn w5_placement_hash_v1(chain: &str, update: u64, seed: u64, slot: u64) -> u64 {
    ex1_hash_u64_v1("w5/placement", chain, update, seed, slot)
}

/// `k = 1 + floor(128 u)`: the 1-based eligible index to search. With `u`
/// the hash's top 53 bits over `2^53`, `floor(128 u)` is the hash's top 7
/// bits, so every index in 1..=128 has probability exactly 1/128.
pub fn w5_placement_index_v1(hash: u64) -> u64 {
    1 + (hash >> 57)
}

/// The acting draw's hash of one learner seat-episode.
pub fn w5_act_hash_v1(chain: &str, update: u64, seed: u64, slot: u64) -> u64 {
    ex1_hash_u64_v1("w5/act", chain, update, seed, slot)
}

/// The action played from `q`: the first menu index whose running sum
/// (binary64, menu order) exceeds `u`. Should rounding leave the total at or
/// below `u`, the last index with positive mass. `Err` when `q` has no
/// positive mass or a non-finite or negative entry.
pub fn w5_sample_from_q_v1(q: &[f64], u: f64) -> Result<usize, String> {
    if q.iter().any(|p| !p.is_finite() || *p < 0.0) {
        return Err("w5 acting: q has a negative or non-finite entry".into());
    }
    let last = q
        .iter()
        .rposition(|p| *p > 0.0)
        .ok_or("w5 acting: q has no positive mass")?;
    let mut total = 0.0_f64;
    for (a, p) in q.iter().enumerate() {
        total += p;
        if *p > 0.0 && u < total {
            return Ok(a);
        }
    }
    Ok(last)
}

/// The played action at a target row: q's draw at the episode's acting hash.
pub fn w5_played_action_v1(
    q: &[f64],
    chain: &str,
    update: u64,
    seed: u64,
    slot: u64,
) -> Result<usize, String> {
    w5_sample_from_q_v1(q, ex1_uniform_v1(w5_act_hash_v1(chain, update, seed, slot)))
}

/// Whether the update at global index `update` runs the paired Adam-step
/// comparison.
pub fn w5_telemetry_update_v1(update: u64) -> bool {
    update % W5_TELEMETRY_STRIDE_V1 == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::{CardStableRefV1, PlayerSeatV1};
    use crate::state::Zone;

    fn card() -> CardStableRefV1 {
        CardStableRefV1 {
            arena_id: 1,
            card_db_id: 2,
            owner: PlayerSeatV1::P0,
            controller: PlayerSeatV1::P0,
            zone: Zone::Hand,
            zone_change_count: 0,
        }
    }

    #[test]
    fn eligibility_counts_non_mana_candidates() {
        use ActionSemanticV1 as A;
        let pass = A::Pass {
            actor: PlayerSeatV1::P0,
        };
        let cast = A::CastSpell {
            actor: PlayerSeatV1::P0,
            source: card(),
        };
        let mana = A::ActivateManaAbility {
            actor: PlayerSeatV1::P0,
            source: card(),
            mana_choice: None,
            cost_target: None,
        };
        assert!(w5_eligible_v1(&[pass.clone(), cast.clone()]));
        assert!(w5_eligible_v1(&[mana.clone(), pass.clone(), cast.clone()]));
        assert!(!w5_eligible_v1(&[mana.clone(), pass.clone()]));
        assert!(!w5_eligible_v1(&[mana.clone(), mana.clone(), cast]));
        assert!(!w5_eligible_v1(&[pass]));
        assert!(!w5_eligible_v1(&[]));
    }

    /// The bit form equals the stated `1 + floor(128 u)` and spans 1..=128.
    #[test]
    fn placement_index_is_one_plus_floor_128u() {
        for hash in [
            0,
            1,
            u64::MAX,
            1 << 57,
            (1 << 57) - 1,
            0x8000_0000_0000_0000,
        ] {
            let u = ex1_uniform_v1(hash);
            assert_eq!(w5_placement_index_v1(hash), 1 + (128.0 * u).floor() as u64);
        }
        assert_eq!(w5_placement_index_v1(0), 1);
        assert_eq!(w5_placement_index_v1(u64::MAX), W5_PLACEMENT_MAX_V1);
        // A known draw, mirrored by tools/w5/xact_ref.py.
        let hash = w5_placement_hash_v1("w5/chain1", 7, 42, 3);
        assert_eq!(hash, ex1_hash_u64_v1("w5/placement", "w5/chain1", 7, 42, 3));
        assert_eq!(hash, 5_962_464_029_930_671_119);
        assert_eq!(w5_placement_index_v1(hash), 42);
        let act = w5_act_hash_v1("w5/chain1", 7, 42, 3);
        assert_eq!(act, 15_846_080_588_647_116_512);
        assert_eq!(ex1_uniform_v1(act), 0.8590177499795791);
    }

    #[test]
    fn sampling_is_the_inverse_cdf_in_menu_order() {
        let q = [0.25, 0.0, 0.5, 0.25];
        assert_eq!(w5_sample_from_q_v1(&q, 0.0), Ok(0));
        assert_eq!(w5_sample_from_q_v1(&q, 0.2499), Ok(0));
        assert_eq!(w5_sample_from_q_v1(&q, 0.25), Ok(2));
        assert_eq!(w5_sample_from_q_v1(&q, 0.7499), Ok(2));
        assert_eq!(w5_sample_from_q_v1(&q, 0.75), Ok(3));
        // Rounding below u: the last positive index.
        assert_eq!(w5_sample_from_q_v1(&[0.5, 0.25, 0.0], 0.9), Ok(1));
        assert!(w5_sample_from_q_v1(&[0.0, 0.0], 0.1).is_err());
        assert!(w5_sample_from_q_v1(&[f64::NAN, 1.0], 0.1).is_err());
        assert!(w5_sample_from_q_v1(&[-0.1, 1.1], 0.1).is_err());
    }

    #[test]
    fn telemetry_runs_one_update_in_sixteen() {
        let runs: Vec<u64> = (0..64).filter(|&u| w5_telemetry_update_v1(u)).collect();
        assert_eq!(runs, vec![0, 16, 32, 48]);
    }
}
