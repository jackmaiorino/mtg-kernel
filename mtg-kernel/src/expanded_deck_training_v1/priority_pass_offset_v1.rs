//! Pass offset at new priority windows (`ExpandedEpisodeV1::priority_pass_offset`).
//!
//! An opt-in, hand-blind learning aid for a fork that starts playing with
//! full priority windows (`priority_action_windows_v1`): one constant is added
//! to the logit of the Pass row of every new window before the row is
//! sampled, and to the same row inside the update's log-softmax, so the
//! policy that plays is the policy that is trained (on-policy). A large
//! constant starts the fork near today's passing behavior; the caller decays
//! it (the schedule belongs to whoever writes each episode; the trainer
//! applies whatever value an episode carries, and `None` is no aid).
//!
//! A new window is a decision the surface hands to a seat whose rule is
//! `action-windows-v1` or `action-windows-end-step-v1` at a point where
//! HarnessV2 would have forced a pass (the surface reports the HarnessV2
//! suppression reason, `HarnessSurfaceV2::engine_window_v1`), end-step
//! windows included. Windows HarnessV2 already offers, mulligan rows, a
//! HarnessV2 seat's decisions and every other row are untouched. The constant
//! is the same for every hand, deck, card, window class and seat; nothing
//! here reads a card or a hand.
//!
//! Nothing else changes: the network, its parameters, its optimizer state and
//! every recorded raw logit stay as they are; the offset is never a parameter.
//! Adjusted rows are drawn with the unclamped softmax sampler
//! (`unclamped-softmax-f64-icdf-u53-v1`, one seat-stream draw per decision
//! as always), as the London keep offset's rows are.
//!
//! Row mark. The decision tensors do not say whether a priority window is a
//! new one (the HarnessV2 bookkeeping, and so the surface columns, run in
//! full for every seat), so each adjusted decision record carries
//! `pass_offset_action`: the index of its Pass row. It is present only on
//! adjusted decisions and omitted otherwise, so a trajectory without the
//! field keeps every byte. Collection writes it from the live session
//! ([`crate::rl_session::FastActorSessionV1::priority_pass_window_v1`]); the
//! update reads it per substep, in batch order. Record validation trusts no
//! mark: a trajectory whose episode carries the field is replayed through a
//! fresh session with its recorded actions, and every row's mark must equal
//! what the session reports at that decision (`validate_pass_offset_marks_v1`
//! in the parent module), so a record can neither claim a row that was not a
//! new window nor omit one that was. The sampler re-draw then checks every
//! row against the logits it was drawn from.
//!
//! The episode field is an f64 that is exactly an f32 value, finite, at
//! least 0 (not -0.0) and at most [`MAX_PASS_OFFSET_V1`]; zero still marks
//! and adjusts (it adds 0.0). A batch must carry one value, compared as f32
//! bits.

/// Largest accepted offset. It only keeps `raw + offset` finite for every
/// finite raw logit, so the f64 sampler and the f32 update log-softmax see
/// the same finite row.
pub(crate) const MAX_PASS_OFFSET_V1: f32 = 1.0e6;

/// The offset in force for one update's step: the constant and, per learner
/// substep in batch order (group by group, substep by substep), the Pass row
/// it adjusts or `None`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PassOffsetMarksV1 {
    pub(crate) offset: f32,
    pub(crate) rows: Vec<Option<u32>>,
}

/// Checks an offset value: finite, not negative (not -0.0 either, so one
/// value has one set of bits) and at most [`MAX_PASS_OFFSET_V1`].
pub(crate) fn validate_pass_offset_v1(offset: f32) -> Result<(), String> {
    if offset.is_finite()
        && offset >= 0.0
        && !offset.is_sign_negative()
        && offset <= MAX_PASS_OFFSET_V1
    {
        Ok(())
    } else {
        Err("priority_pass_offset must be finite, at least 0 and at most 1e6".into())
    }
}

/// The f32 an episode's recorded offset stands for. Stored as an f64 that
/// must be exactly an f32 value, so a trajectory's JSON reads back to the
/// same number (as `london_keep_offset` is).
pub(crate) fn pass_offset_f32_v1(recorded: f64) -> Result<f32, String> {
    let narrow = recorded as f32;
    validate_pass_offset_v1(narrow)?;
    if f64::from(narrow) != recorded {
        return Err("priority_pass_offset must be exactly an f32 value".into());
    }
    Ok(narrow)
}

/// The logits an adjusted decision samples from and trains on: a copy of
/// `logits` with `offset` added to row `pass_row`.
pub(crate) fn pass_offset_logits_v1(
    logits: &[f32],
    pass_row: u32,
    offset: f32,
) -> Result<Vec<f32>, String> {
    validate_pass_offset_v1(offset)?;
    let row = pass_row as usize;
    if row >= logits.len() {
        return Err("priority pass offset row outside the decision's actions".into());
    }
    let mut adjusted = logits.to_vec();
    adjusted[row] += offset;
    if !adjusted[row].is_finite() {
        return Err("priority pass offset left a non-finite Pass logit".into());
    }
    Ok(adjusted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_offset_changes_only_the_pass_logit() {
        let raw = [0.25f32, -3.5, 1.0];
        let adjusted = pass_offset_logits_v1(&raw, 1, 2.0).unwrap();
        assert_eq!(adjusted[0].to_bits(), raw[0].to_bits());
        assert_eq!(adjusted[1].to_bits(), (-3.5f32 + 2.0).to_bits());
        assert_eq!(adjusted[2].to_bits(), raw[2].to_bits());
        // Zero adds 0.0 and changes no bit.
        let zero = pass_offset_logits_v1(&raw, 2, 0.0).unwrap();
        assert_eq!(
            zero.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
            raw.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
        );
        assert!(pass_offset_logits_v1(&raw, 3, 1.0).is_err());
        // The ceiling keeps every finite raw logit finite.
        assert!(pass_offset_logits_v1(&[3.0e38, 0.0], 0, MAX_PASS_OFFSET_V1).is_ok());
        assert!(pass_offset_logits_v1(&[0.0, 1.0], 0, f32::MAX).is_err());
    }

    #[test]
    fn values_are_checked_like_the_keep_offset() {
        for good in [0.0, 1.0, 6.5, 1.0e6] {
            assert_eq!(pass_offset_f32_v1(good).unwrap() as f64, good);
        }
        for bad in [
            -1.0,
            -0.0,
            f64::NAN,
            f64::INFINITY,
            1.0e6 + 1.0,
            // 0.1 is not an f32 value held in an f64.
            0.1,
        ] {
            assert!(pass_offset_f32_v1(bad).is_err(), "{bad}");
        }
        assert!(validate_pass_offset_v1(-0.0).is_err());
        assert!(validate_pass_offset_v1(f32::NAN).is_err());
    }
}
