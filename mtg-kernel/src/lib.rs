//! Experimental, deterministic, resumable game core for a fixed Pauper pool.
//!
//! Scope: exactly the pinned nine-deck Pauper pool (150 unique roster names;
//! 132 currently registered deck cards plus required tokens). The
//! Java XMage engine remains the reference implementation and claim surface.
//! This kernel is intended to reduce rules-engine cost in training and search
//! workloads; an end-to-end training speedup over XMage has not yet been
//! established. Behavior is validated by golden-trace replay equivalence and
//! branch differential testing against the reference. Unsupported mechanics
//! FAIL CLOSED (a deck that needs them cannot be simulated here and must run on
//! the reference engine).
//!
//! Architectural invariants (Sol #84):
//! - Deterministic transitions; no unordered-map iteration anywhere.
//! - Stable arena ids for all game objects; canonical identity across snapshots.
//! - Versioned snapshot/restore boundary; v1 snapshots are full-state clones,
//!   not O(1).
//! - Strict single-session JSONL reset/step boundary; parallelism and batching
//!   are orchestrated outside the Rust process.

pub mod async_rollout;
pub mod async_rollout_v2;
pub mod card_def;
pub mod effect;
pub mod engine;
pub mod event;
pub mod fast_sampler;
pub mod ids;
pub mod mana;
pub mod phase_profile;
pub mod policy_surface_v5;
pub mod rl;
pub mod rl_session;
pub mod runtime_decks;
pub mod snapshot;
pub mod state;
pub mod surface;
pub mod surface_v2;
pub mod trace;
pub mod trigger;

pub const KERNEL_VERSION: &str = "0.0.4-spike";
