//! M4: the learned cube drafter and deck builder for the eight-seat Vintage
//! Cube draft of [`crate::cube_draft_v1`] (v1).
//!
//! Project law, binding every part of this module: the only reward is the
//! terminal game result (win +1, loss -1, natural draw 0); no heuristic
//! priors, land-count aids, ratings, pick orders or human draft data; a card
//! is its registry id plus its row of the engine feature table, never its
//! text.
//!
//! [`card_index`] (task D1) turns the engine's cube feature table into the
//! fixed per-card inputs the model reads: the card index, the scaled feature
//! rows and the dense fields.

pub mod card_index;
