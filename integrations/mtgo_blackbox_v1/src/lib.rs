//! Offline contract for a player-visible MTGO adapter.
//!
//! This crate intentionally contains no capture, process inspection, network
//! inspection, accessibility enumeration, or input implementation.

#![forbid(unsafe_code)]

mod action_resolution;
mod calibration_trace;
mod capture_contract;
mod competitive_lifecycle;
mod contract;
mod dxgi_artifact;
mod mock;
mod model_scoring;
mod offline_bottoming;
mod offline_mulligan_ladder;
mod offline_perception;
mod offline_pregame_episode;
mod reconstruction_audit;
mod validation;
mod visible_history;

pub use action_resolution::*;
pub use calibration_trace::*;
pub use capture_contract::*;
pub use competitive_lifecycle::*;
pub use contract::*;
pub use dxgi_artifact::*;
pub use mock::*;
pub use model_scoring::*;
pub use offline_bottoming::*;
pub use offline_mulligan_ladder::*;
pub use offline_perception::*;
pub use offline_pregame_episode::*;
pub use reconstruction_audit::*;
pub use validation::*;
pub use visible_history::*;
