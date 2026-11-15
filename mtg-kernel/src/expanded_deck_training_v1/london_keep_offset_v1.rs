//! Pregame keep offset for London openings (`ExpandedEpisodeV1::london_keep_offset`).
//!
//! An opt-in start aid for learning London mulligans from a policy whose
//! pregame rows are untrained: an offset is subtracted from the logit of
//! every "take a mulligan" row of a London announcement before the row is
//! sampled, and from the same row inside the update's log-softmax, so the
//! policy that plays is the policy that is trained (on-policy). The offset is
//! one positive scalar (the same for every announcement) or seven numbers, one
//! per number of mulligans the acting seat has already taken, 0 to 6
//! ([`LondonKeepOffsetV1`]); a scalar is the vector with seven equal entries.
//! Nothing else changes:
//!
//! - the network, its parameters, its optimizer state and every recorded raw
//!   logit stay as they are; the offset is never a parameter and never saved;
//! - keep rows, bottom rows and every in-game row are untouched;
//! - the entry used depends on nothing but the acting seat's own count of
//!   mulligans already taken, read from the announcement's own action tensor
//!   ([`own_mulligans_taken_v1`]); there is no deck, card or hand-quality term;
//! - `None` (the default) skips the offset entirely, so an episode without it
//!   collects, validates and trains bit for bit as before.
//!
//! The rows an offset adjusts are drawn with the unclamped softmax sampler
//! (`unclamped-softmax-f64-icdf-u53-v1`, one seat-stream draw per decision as
//! always), so the draw follows the softmax of the offset logits that the
//! update's log-softmax uses, whatever sampler the policy's other rows use
//! (the legacy sampler quantizes gaps and clamps them at 16 nats). Every other
//! row of the episode keeps its sampler. A mulligan row of an episode that
//! carries the field is adjusted even when its entry is 0 (it subtracts 0.0).
//!
//! The episode field holds f64 numbers each exactly an f32 value
//! ([`offset_f32_v1`]); the trainer narrows them once, where they are used.
//! A batch must carry one value, compared as seven f32 bit patterns.
//!
//! A schedule (its start values, shape and end update) belongs to the caller
//! that writes each episode (the chains tools); the trainer applies whatever
//! value an episode carries. A decay to zero is an episode with `None`.
//!
//! The mulligan row is found from the FlatV7 tensors alone, so collection,
//! record validation and the update agree without any other record:
//! pregame stage (state column 89) set, action kind column 15
//! (`london_opening`) set and the VALUE flag (action column 69) set. FlatV8
//! appends its include column after these, so the columns are the same.

use crate::native_flat_tensorizer_v2::NATIVE_FLAT_ACTION_FEATURE_DIM_V2;
use serde::{Deserialize, Serialize};

/// FlatV7 state column of the London pregame stage (engine-stage slot 9).
pub(crate) const PREGAME_STATE_COLUMN_V1: usize = 89;
/// FlatV7 action-kind column `london_opening` (V4 `choose_effect_number`).
pub(crate) const LONDON_OPENING_ACTION_COLUMN_V1: usize = 15;
/// FlatV7 action VALUE flag column: 1.0 on the mulligan row of an announcement.
pub(crate) const MULLIGAN_FLAG_ACTION_COLUMN_V1: usize = 69;
/// FlatV7 action column `selected_count`: on every row of a London
/// announcement it is the acting seat's own count of mulligans already taken
/// (`london.mulligans_taken[0]`, actor-relative self), scaled by 1/16. Column
/// 66 carries the other seat's count and is never read.
pub(crate) const OWN_MULLIGANS_ACTION_COLUMN_V1: usize = 63;
/// Number of mulligan counts an offset vector covers: 0 to 6 already taken (a
/// seat that has taken seven keeps zero cards without a decision).
pub(crate) const LONDON_OFFSET_COUNTS_V1: usize = 7;

/// Largest accepted offset. A real start value is a few tens of nats (a
/// probability of `e^-104` already underflows the f32 softmax); the ceiling
/// only keeps `raw - offset` finite for every finite raw logit, so the f64
/// sampler and the f32 update log-softmax see the same finite row.
pub(crate) const MAX_OFFSET_V1: f32 = 1.0e6;

/// An episode's `london_keep_offset`: one positive number for every
/// announcement (the only form before the per-count vector), or seven
/// numbers, one per number of mulligans the acting seat has already taken.
/// Absent stays absent, so every older byte still parses and writes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LondonKeepOffsetV1 {
    Scalar(f64),
    PerCount([f64; LONDON_OFFSET_COUNTS_V1]),
}

impl LondonKeepOffsetV1 {
    /// The validated offsets the trainer uses. A scalar `s` becomes seven
    /// entries equal to `s`, so both forms behave bit for bit alike.
    pub(crate) fn resolve_v1(self) -> Result<OffsetVectorV1, String> {
        match self {
            Self::Scalar(value) => Ok(OffsetVectorV1::scalar(offset_f32_v1(value)?)),
            Self::PerCount(values) => {
                let mut entries = [0.0f32; LONDON_OFFSET_COUNTS_V1];
                for (entry, value) in entries.iter_mut().zip(values) {
                    *entry = vector_entry_f32_v1(value)?;
                }
                require_some_positive_v1(&entries)?;
                Ok(OffsetVectorV1(entries))
            }
        }
    }
}

/// The offsets in force for one update or one collected episode: entry `k` is
/// subtracted from the mulligan row of an announcement whose acting seat has
/// already taken `k` mulligans. Every entry is finite, at least 0 and at most
/// [`MAX_OFFSET_V1`]; at least one is above 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OffsetVectorV1([f32; LONDON_OFFSET_COUNTS_V1]);

impl OffsetVectorV1 {
    /// Seven equal entries (the scalar form).
    pub(crate) const fn scalar(value: f32) -> Self {
        Self([value; LONDON_OFFSET_COUNTS_V1])
    }

    /// Seven distinct entries, for callers that already hold f32 values.
    #[cfg(test)]
    pub(crate) const fn per_count(entries: [f32; LONDON_OFFSET_COUNTS_V1]) -> Self {
        Self(entries)
    }

    /// The entries' bit patterns, the value a batch of episodes must agree on.
    pub(crate) fn bits(&self) -> [u32; LONDON_OFFSET_COUNTS_V1] {
        self.0.map(f32::to_bits)
    }

    fn entry(&self, count: usize) -> f32 {
        self.0[count]
    }

    fn validate(&self) -> Result<(), String> {
        for entry in self.0 {
            validate_vector_entry_v1(entry)?;
        }
        require_some_positive_v1(&self.0)
    }
}

/// Checks a vector entry: finite, not negative (not -0.0 either, so one value
/// has one set of bits) and at most [`MAX_OFFSET_V1`]. Zero is allowed.
fn validate_vector_entry_v1(entry: f32) -> Result<(), String> {
    if entry.is_finite() && entry >= 0.0 && !entry.is_sign_negative() && entry <= MAX_OFFSET_V1 {
        Ok(())
    } else {
        Err("london_keep_offset entries must be finite, at least 0 and at most 1e6".into())
    }
}

fn require_some_positive_v1(entries: &[f32]) -> Result<(), String> {
    if entries.iter().any(|entry| *entry > 0.0) {
        Ok(())
    } else {
        Err("london_keep_offset needs at least one entry above 0".into())
    }
}

/// One vector entry as an f32: an f64 that is exactly an f32 value in range
/// (zero allowed, unlike the scalar).
fn vector_entry_f32_v1(recorded: f64) -> Result<f32, String> {
    let narrow = recorded as f32;
    validate_vector_entry_v1(narrow)?;
    if f64::from(narrow) != recorded {
        return Err("london_keep_offset must be exactly an f32 value".into());
    }
    Ok(narrow)
}

/// Checks a scalar offset value: finite, strictly positive and at most
/// [`MAX_OFFSET_V1`].
pub(crate) fn validate_offset_v1(offset: f32) -> Result<(), String> {
    if offset.is_finite() && offset > 0.0 && offset <= MAX_OFFSET_V1 {
        Ok(())
    } else {
        Err("london_keep_offset must be finite, positive and at most 1e6".into())
    }
}

/// The f32 an episode's recorded scalar offset stands for. The field is stored
/// as an f64 that must be exactly an f32 value: a trajectory's JSON then reads
/// back to the same number (an f32 field would be written in its shortest f32
/// form, which does not parse back to the widened f32 the run's own
/// comparison builds, so a resumed run would refuse its saved episodes).
pub(crate) fn offset_f32_v1(recorded: f64) -> Result<f32, String> {
    let narrow = recorded as f32;
    validate_offset_v1(narrow)?;
    if f64::from(narrow) != recorded {
        return Err("london_keep_offset must be exactly an f32 value".into());
    }
    Ok(narrow)
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
    if !action_features.len().is_multiple_of(action_count) {
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

/// The acting seat's own count of mulligans already taken, from the action
/// rows of a London announcement. Every row carries the same `selected_count`
/// (column 63), and it must be exactly `k / 16` for an integer `k` in 0 to 6.
/// Column 66, the other seat's count, is never read.
pub(crate) fn own_mulligans_taken_v1(
    action_features: &[f32],
    action_count: usize,
) -> Result<usize, String> {
    if action_count == 0 || !action_features.len().is_multiple_of(action_count) {
        return Err("action features do not divide into the action rows".into());
    }
    let width = action_features.len() / action_count;
    if width <= OWN_MULLIGANS_ACTION_COLUMN_V1 {
        return Err("action rows narrower than the FlatV7 contract".into());
    }
    let first = action_features[OWN_MULLIGANS_ACTION_COLUMN_V1];
    for row in 1..action_count {
        let value = action_features[row * width + OWN_MULLIGANS_ACTION_COLUMN_V1];
        if value.to_bits() != first.to_bits() {
            return Err("london announcement rows disagree on the own mulligan count".into());
        }
    }
    (0..LONDON_OFFSET_COUNTS_V1)
        .find(|count| (*count as f64 / 16.0) as f32 == first)
        .ok_or_else(|| "london announcement own mulligan count is not 0 to 6".to_string())
}

/// The logits the offset policy samples from and trains on: a copy of `raw`
/// with the offset entry for the acting seat's own mulligan count subtracted
/// from each mulligan row. Returns `None` when the decision has no mulligan
/// row, so the caller keeps using `raw` unchanged. A zero entry still returns
/// the adjusted copy (it subtracts 0.0), so the row is drawn from the same
/// sampler as every other adjusted row.
pub(crate) fn offset_logits_v1(
    raw: &[f32],
    state: &[f32],
    action_features: &[f32],
    offset: &OffsetVectorV1,
) -> Result<Option<Vec<f32>>, String> {
    offset.validate()?;
    let rows = mulligan_rows_v1(state, action_features, raw.len())?;
    if rows.is_empty() {
        return Ok(None);
    }
    let subtracted = offset.entry(own_mulligans_taken_v1(action_features, raw.len())?);
    let mut adjusted = raw.to_vec();
    for row in rows {
        adjusted[row] -= subtracted;
        if !adjusted[row].is_finite() {
            return Err("london keep offset left a non-finite mulligan logit".into());
        }
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

    /// A keep/mulligan announcement whose acting seat has taken `own`
    /// mulligans and whose other seat has taken `other`, laid out as the
    /// tensorizer writes it (column 63 own count, column 66 the other's).
    fn announcement(own: u8, other: u8, width: usize) -> Vec<f32> {
        let mut features = rows(&[(1.0, 0.0), (1.0, 1.0)], width);
        for row in 0..2 {
            features[row * width + OWN_MULLIGANS_ACTION_COLUMN_V1] = (f64::from(own) / 16.0) as f32;
            features[row * width + 66] = (f64::from(other) / 16.0) as f32;
        }
        features
    }

    fn pregame_state(set: bool) -> Vec<f32> {
        let mut state = vec![0.0; 219];
        state[PREGAME_STATE_COLUMN_V1] = if set { 1.0 } else { 0.0 };
        state
    }

    fn vector(values: [f64; 7]) -> LondonKeepOffsetV1 {
        LondonKeepOffsetV1::PerCount(values)
    }

    #[test]
    fn finds_only_the_announcement_mulligan_row() {
        for width in [195, 196] {
            let announce = rows(&[(1.0, 0.0), (1.0, 1.0)], width);
            assert_eq!(
                mulligan_rows_v1(&pregame_state(true), &announce, 2).unwrap(),
                vec![1]
            );
            // In-game (stage column clear): never, whatever the columns hold.
            assert!(mulligan_rows_v1(&pregame_state(false), &announce, 2)
                .unwrap()
                .is_empty());
            // Bottom rows: kind set, flag clear.
            let bottom = rows(&[(1.0, 0.0), (1.0, 0.0), (1.0, 0.0)], width);
            assert!(mulligan_rows_v1(&pregame_state(true), &bottom, 3)
                .unwrap()
                .is_empty());
        }
    }

    #[test]
    fn offset_changes_only_the_mulligan_logit() {
        let announce = rows(&[(1.0, 0.0), (1.0, 1.0)], 195);
        let raw = [0.25f32, 3.5];
        let adjusted = offset_logits_v1(
            &raw,
            &pregame_state(true),
            &announce,
            &OffsetVectorV1::scalar(2.0),
        )
        .unwrap()
        .unwrap();
        assert_eq!(adjusted[0].to_bits(), raw[0].to_bits());
        assert_eq!(adjusted[1].to_bits(), (3.5f32 - 2.0).to_bits());
        assert!(offset_logits_v1(
            &raw,
            &pregame_state(false),
            &announce,
            &OffsetVectorV1::scalar(2.0)
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn refuses_bad_offsets_and_shapes() {
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(validate_offset_v1(bad).is_err());
        }
        assert!(validate_offset_v1(4.0).is_ok());
        assert!(validate_offset_v1(MAX_OFFSET_V1).is_ok());
        for too_big in [f32::MAX, MAX_OFFSET_V1 * 2.0] {
            assert!(validate_offset_v1(too_big).is_err());
        }
        // An accepted offset cannot overflow a finite raw logit, and the
        // check also runs inside the adjustment itself.
        let announce = rows(&[(1.0, 0.0), (1.0, 1.0)], 195);
        assert!(offset_logits_v1(
            &[1.0, 0.5],
            &pregame_state(true),
            &announce,
            &OffsetVectorV1::scalar(f32::MAX)
        )
        .is_err());
        assert!(offset_logits_v1(
            &[1.0, -3.0e38],
            &pregame_state(true),
            &announce,
            &OffsetVectorV1::scalar(MAX_OFFSET_V1)
        )
        .is_ok());
        let two = rows(&[(1.0, 1.0), (1.0, 1.0)], 195);
        assert!(mulligan_rows_v1(&pregame_state(true), &two, 2).is_err());
        assert!(mulligan_rows_v1(&pregame_state(true), &[0.0; 10], 3).is_err());
    }

    #[test]
    fn the_entry_follows_the_acting_seats_own_count_and_never_the_others() {
        let entries = [9.0f32, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0];
        let offset = OffsetVectorV1::per_count(entries);
        let raw = [0.5f32, 20.0];
        for own in 0..7u8 {
            // The other seat's count is anything but `own`: columns 63 and 66
            // differ in most real announcements, so a read of column 66 would
            // pick another entry.
            for other in [0u8, 3, 6] {
                let features = announcement(own, other, 195);
                let adjusted = offset_logits_v1(&raw, &pregame_state(true), &features, &offset)
                    .unwrap()
                    .unwrap();
                assert_eq!(adjusted[0].to_bits(), raw[0].to_bits());
                assert_eq!(
                    adjusted[1].to_bits(),
                    (raw[1] - entries[usize::from(own)]).to_bits(),
                    "own {own} other {other}"
                );
            }
        }
        assert_eq!(
            own_mulligans_taken_v1(&announcement(4, 1, 196), 2).unwrap(),
            4
        );
    }

    #[test]
    fn a_zero_entry_in_the_middle_still_adjusts_and_changes_no_bit() {
        let offset = OffsetVectorV1::per_count([4.0, 0.0, 6.0, 0.0, 0.0, 2.0, 1.0]);
        let raw = [-0.75f32, 1.25];
        let features = announcement(1, 5, 195);
        let adjusted = offset_logits_v1(&raw, &pregame_state(true), &features, &offset)
            .unwrap()
            .expect("an entry of zero still returns the adjusted copy");
        assert_eq!(adjusted[0].to_bits(), raw[0].to_bits());
        assert_eq!(adjusted[1].to_bits(), raw[1].to_bits());
        // The neighbours do change.
        let before = announcement(0, 5, 195);
        let after = announcement(2, 5, 195);
        for (features, expected) in [(before, 4.0f32), (after, 6.0)] {
            let adjusted = offset_logits_v1(&raw, &pregame_state(true), &features, &offset)
                .unwrap()
                .unwrap();
            assert_eq!(adjusted[1].to_bits(), (raw[1] - expected).to_bits());
        }
        // Keep and bottom decisions and in-game decisions stay untouched.
        assert!(offset_logits_v1(
            &raw,
            &pregame_state(false),
            &announcement(1, 5, 195),
            &offset
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn a_scalar_and_seven_equal_entries_are_the_same_offset_bit_for_bit() {
        let scalar = LondonKeepOffsetV1::Scalar(15.25).resolve_v1().unwrap();
        let spread = vector([15.25; 7]).resolve_v1().unwrap();
        assert_eq!(scalar, spread);
        assert_eq!(scalar.bits(), spread.bits());
        let raw = [0.1f32, 2.2];
        for own in 0..7u8 {
            let features = announcement(own, 2, 195);
            let a = offset_logits_v1(&raw, &pregame_state(true), &features, &scalar).unwrap();
            let b = offset_logits_v1(&raw, &pregame_state(true), &features, &spread).unwrap();
            assert_eq!(
                a.unwrap().iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                b.unwrap().iter().map(|x| x.to_bits()).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn the_own_count_must_be_the_same_on_every_row_and_one_of_zero_to_six() {
        // The keep row and the mulligan row disagree.
        let mut split = announcement(2, 2, 195);
        split[195 + OWN_MULLIGANS_ACTION_COLUMN_V1] = 3.0 / 16.0;
        assert!(own_mulligans_taken_v1(&split, 2).is_err());
        // Seven taken has no announcement; a count between steps is not one.
        for bad in [7.0f32 / 16.0, 0.03, -0.0625, 1.0, f32::NAN] {
            let mut features = announcement(0, 0, 195);
            for row in 0..2 {
                features[row * 195 + OWN_MULLIGANS_ACTION_COLUMN_V1] = bad;
            }
            assert!(own_mulligans_taken_v1(&features, 2).is_err(), "{bad}");
            assert!(offset_logits_v1(
                &[0.0, 1.0],
                &pregame_state(true),
                &features,
                &OffsetVectorV1::scalar(3.0)
            )
            .is_err());
        }
        assert!(own_mulligans_taken_v1(&[0.0; 10], 3).is_err());
        assert!(own_mulligans_taken_v1(&[0.0; 40], 2).is_err());
    }

    #[test]
    fn the_vector_validates_each_entry_and_needs_one_positive() {
        assert!(vector([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.5])
            .resolve_v1()
            .is_ok());
        assert!(vector([1.0e6; 7]).resolve_v1().is_ok());
        for bad in [
            [0.0; 7],
            [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0],
            [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -0.0],
            [1.0, f64::NAN, 1.0, 1.0, 1.0, 1.0, 1.0],
            [1.0, 1.0, f64::INFINITY, 1.0, 1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, 1.0e6 + 1.0, 1.0, 1.0, 1.0],
            // 0.1 is not an f32 value held in an f64.
            [1.0, 1.0, 1.0, 1.0, 0.1, 1.0, 1.0],
        ] {
            assert!(vector(bad).resolve_v1().is_err(), "{bad:?}");
        }
        // The scalar keeps its own rule: strictly positive.
        assert!(LondonKeepOffsetV1::Scalar(0.0).resolve_v1().is_err());
        assert!(LondonKeepOffsetV1::Scalar(15.3).resolve_v1().is_err());
        assert!(LondonKeepOffsetV1::Scalar(6.5).resolve_v1().is_ok());
    }

    #[test]
    fn the_field_reads_and_writes_both_forms_and_nothing_else() {
        let scalar: LondonKeepOffsetV1 = serde_json::from_str("6.5").unwrap();
        assert_eq!(scalar, LondonKeepOffsetV1::Scalar(6.5));
        assert_eq!(serde_json::to_string(&scalar).unwrap(), "6.5");
        let whole: LondonKeepOffsetV1 = serde_json::from_str("6").unwrap();
        assert_eq!(whole, LondonKeepOffsetV1::Scalar(6.0));
        let per_count: LondonKeepOffsetV1 =
            serde_json::from_str("[1.5,0.0,2.5,3.0,4.0,5.0,6.0]").unwrap();
        assert_eq!(per_count, vector([1.5, 0.0, 2.5, 3.0, 4.0, 5.0, 6.0]));
        let written = serde_json::to_value(per_count).unwrap();
        assert_eq!(
            written,
            serde_json::json!([1.5, 0.0, 2.5, 3.0, 4.0, 5.0, 6.0])
        );
        for bad in [
            "[]",
            "[1.0,2.0,3.0,4.0,5.0,6.0]",
            "[1.0,2.0,3.0,4.0,5.0,6.0,7.0,8.0]",
            "\"6.5\"",
            "null",
            "{}",
        ] {
            assert!(
                serde_json::from_str::<LondonKeepOffsetV1>(bad).is_err(),
                "{bad}"
            );
        }
    }
}
