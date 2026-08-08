//! Offline contract for a player-visible MTGO adapter.
//!
//! This crate intentionally contains no capture, process inspection, network
//! inspection, accessibility enumeration, or input implementation.

#![forbid(unsafe_code)]

mod contract;
mod mock;
mod validation;

pub use contract::*;
pub use mock::*;
pub use validation::*;
