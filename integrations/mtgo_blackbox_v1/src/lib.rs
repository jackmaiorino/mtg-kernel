//! Offline contract for a player-visible MTGO adapter.
//!
//! This crate intentionally contains no capture, process inspection, network
//! inspection, accessibility enumeration, or input implementation.

#![forbid(unsafe_code)]

mod action_resolution;
mod calibration_trace;
mod card_correspondence;
mod capture_contract;
mod competitive_lifecycle;
mod contract;
mod dxgi_artifact;
mod first_main_kernel_coverage;
mod first_main_kernel_context;
mod first_main_legal_actions;
mod first_main_reconstruction;
mod mock;
mod model_scoring;
mod object_incarnation;
mod offline_bottom_six_initial;
mod offline_bottom_six_reflow;
mod offline_bottom_six_state;
mod offline_bottom_six_state_v2;
mod offline_bottom_six_state_v3;
mod offline_bottoming;
mod offline_first_main;
mod offline_mulligan_ladder;
mod offline_perception;
mod offline_play_land_hand_reflow;
mod offline_pregame_episode;
mod offline_visible_card_identity;
mod reconstruction_audit;
mod validation;
mod visible_history;

pub use action_resolution::*;
pub use calibration_trace::*;
pub use card_correspondence::*;
pub use capture_contract::*;
pub use competitive_lifecycle::*;
pub use contract::*;
pub use dxgi_artifact::*;
pub use first_main_kernel_coverage::*;
pub use first_main_kernel_context::*;
pub use first_main_legal_actions::*;
pub use first_main_reconstruction::*;
pub use mock::*;
pub use model_scoring::*;
pub use object_incarnation::*;
pub use offline_bottom_six_initial::*;
pub use offline_bottom_six_reflow::*;
pub use offline_bottom_six_state::*;
pub use offline_bottom_six_state_v2::*;
pub use offline_bottom_six_state_v3::*;
pub use offline_bottoming::*;
pub use offline_first_main::*;
pub use offline_mulligan_ladder::*;
pub use offline_perception::*;
pub use offline_play_land_hand_reflow::*;
pub use offline_pregame_episode::*;
pub use offline_visible_card_identity::*;
pub use reconstruction_audit::*;
pub use validation::*;
pub use visible_history::*;
