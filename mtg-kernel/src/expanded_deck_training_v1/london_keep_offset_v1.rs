//! Pregame keep offset for London openings (`ExpandedEpisodeV1::london_keep_offset`).
//!
//! An opt-in start aid for learning London mulligans from a policy whose
//! pregame rows are untrained: a positive scalar `b` is subtracted from the
//! logit of every "take a mulligan" row of a London announcement before the
//! row is sampled, and from the same row inside the update's log-softmax, so
//! the policy that plays is the policy that is trained (on-policy). Nothing
//! else changes:
//!
//! - the network, its parameters, its optimizer state and every recorded raw
//!   logit stay as they are; the offset is never a parameter and never saved;
//! - keep rows, bottom rows and every in-game row are untouched;
//! - the same `b` applies to every seat, deck, hand and mulligan count; there
//!   is no deck, card or hand-quality term;
//! - `None` (the default) skips the offset entirely, so an episode without it
//!   collects, validates and trains bit for bit as before.
//!
//! A schedule (its start value, shape and end update) belongs to the caller
//! that writes each episode (the chains tools); the trainer applies whatever
//! value an episode carries. A decay to zero is an episode with `None`.
//!
//! The mulligan row is found from the FlatV7 tensors alone, so collection,
//! record validation and the update agree without any other record:
//! pregame stage (state column 89) set, action kind column 15
//! (`london_opening`) set and the VALUE flag (action column 69) set. FlatV8
//! appends its include column after these, so the columns are the same.

use crate::native_flat_tensorizer_v2::NATIVE_FLAT_ACTION_FEATURE_DIM_V2;

/// FlatV7 state column of the London pregame stage (engine-stage slot 9).
pub(crate) const PREGAME_STATE_COLUMN_V1: usize = 89;
/// FlatV7 action-kind column `london_opening` (V4 `choose_effect_number`).
pub(crate) const LONDON_OPENING_ACTION_COLUMN_V1: usize = 15;
/// FlatV7 action VALUE flag column: 1.0 on the mulligan row of an announcement.
pub(crate) const MULLIGAN_FLAG_ACTION_COLUMN_V1: usize = 69;

/// Checks an episode's offset value: finite and strictly positive.
pub(crate) fn validate_offset_v1(offset: f32) -> Result<(), String> {
    if offset.is_finite() && offset > 0.0 {
        Ok(())
    } else {
        Err("london_keep_offset must be finite and positive".into())
    }
}

/// Indices of the mulligan rows of one decision (at most one), from its
/// tensors. Empty for every in-game decision, every bottom decision and
/// every keep row. `action_features` is row-major with `action_count` rows.
pub(crate) fn mulligan_rows_v1(
    state: &[f32],
    action_features: &[f32],
    action_count: usize,
) -> Result<Vec<usize>, String> {
    if state.get(PREGAME_STATE_COLUMN_V1).copied() != Some(1.0) || action_count == 0 {
        return Ok(Vec::new());
    }
    if action_features.len() % action_count != 0 {
        return Err("action features do not divide into the action rows".into());
    }
    let width = action_features.len() / action_count;
    if width < NATIVE_FLAT_ACTION_FEATURE_DIM_V2 {
        return Err("action rows narrower than the FlatV7 contract".into());
    }
    let rows: Vec<usize> = (0..action_count)
        .filter(|&row| {
            let base = row * width;
            action_features[base + LONDON_OPENING_ACTION_COLUMN_V1] == 1.0
                && action_features[base + MULLIGAN_FLAG_ACTION_COLUMN_V1] == 1.0
        })
        .collect();
    if rows.len() > 1 {
        return Err("more than one mulligan row in one announcement".into());
    }
    Ok(rows)
}

/// The logits the offset policy samples from and trains on: a copy of `raw`
/// with `offset` subtracted from each mulligan row. Returns `None` when the
/// decision has no mulligan row, so the caller keeps using `raw` unchanged.
pub(crate) fn offset_logits_v1(
    raw: &[f32],
    state: &[f32],
    action_features: &[f32],
    offset: f32,
) -> Result<Option<Vec<f32>>, String> {
    let rows = mulligan_rows_v1(state, action_features, raw.len())?;
    if rows.is_empty() {
        return Ok(None);
    }
    let mut adjusted = raw.to_vec();
    for row in rows {
        adjusted[row] -= offset;
    }
    Ok(Some(adjusted))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(kinds: &[(f32, f32)], width: usize) -> Vec<f32> {
        let mut features = vec![0.0; kinds.len() * width];
        for (row, (kind, flag)) in kinds.iter().enumerate() {
            features[row * width + LONDON_OPENING_ACTION_COLUMN_V1] = *kind;
            features[row * width + MULLIGAN_FLAG_ACTION_COLUMN_V1] = *flag;
        }
        features
    }

    fn pregame_state(set: bool) -> Vec<f32> {
        let mut state = vec![0.0; 219];
        state[PREGAME_STATE_COLUMN_V1] = if set { 1.0 } else { 0.0 };
        state
    }

    #[test]
    fn finds_only_the_announcement_mulligan_row() {
        for width in [195, 196] {
            let announce = rows(&[(1.0, 0.0), (1.0, 1.0)], width);
            assert_eq!(mulligan_rows_v1(&pregame_state(true), &announce, 2).unwrap(), vec![1]);
            // In-game (stage column clear): never, whatever the columns hold.
            assert!(mulligan_rows_v1(&pregame_state(false), &announce, 2).unwrap().is_empty());
            // Bottom rows: kind set, flag clear.
            let bottom = rows(&[(1.0, 0.0), (1.0, 0.0), (1.0, 0.0)], width);
            assert!(mulligan_rows_v1(&pregame_state(true), &bottom, 3).unwrap().is_empty());
        }
    }

    #[test]
    fn offset_changes_only_the_mulligan_logit() {
        let announce = rows(&[(1.0, 0.0), (1.0, 1.0)], 195);
        let raw = [0.25f32, 3.5];
        let adjusted = offset_logits_v1(&raw, &pregame_state(true), &announce, 2.0)
            .unwrap()
            .unwrap();
        assert_eq!(adjusted[0].to_bits(), raw[0].to_bits());
        assert_eq!(adjusted[1].to_bits(), (3.5f32 - 2.0).to_bits());
        assert!(offset_logits_v1(&raw, &pregame_state(false), &announce, 2.0)
            .unwrap()
            .is_none());
    }

    #[test]
    fn refuses_bad_offsets_and_shapes() {
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(validate_offset_v1(bad).is_err());
        }
        assert!(validate_offset_v1(4.0).is_ok());
        let two = rows(&[(1.0, 1.0), (1.0, 1.0)], 195);
        assert!(mulligan_rows_v1(&pregame_state(true), &two, 2).is_err());
        assert!(mulligan_rows_v1(&pregame_state(true), &[0.0; 10], 3).is_err());
    }
}
