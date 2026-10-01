//! One in-memory diagnostic successor, never an alternative training objective.
use super::*;

impl NativePolicyValueTrainStateV1 {
    pub(crate) fn diagnostic_zero_gradient_successor_v1(
        &self,
    ) -> Result<Self, NativePolicyTrainErrorV1> {
        let before = self.snapshot_v1()?;
        let gradients = before
            .parameters
            .iter()
            .map(|p| vec![0.0; p.values.len()])
            .collect::<Vec<_>>();
        let age = before
            .adam_step
            .checked_add(1)
            .ok_or(NativePolicyTrainErrorV1::AdamStepOverflow)?;
        let (mut parameters, mut first, mut second) = adam_update(
            &before.parameters,
            &gradients,
            &self.first_moments,
            &self.second_moments,
            age,
            0.0001,
        )?;
        parameters[SCORER_SECOND_BIAS].values[0] = f32::from_bits(before.scorer_bias_anchor_bits);
        first[SCORER_SECOND_BIAS][0] = 0.0;
        second[SCORER_SECOND_BIAS][0] = 0.0;
        let after = NativePolicyValueTrainSnapshotV1 {
            first_moments: named_state_snapshot(&parameters, &first),
            second_moments: named_state_snapshot(&parameters, &second),
            parameters,
            adam_step: age,
            scorer_bias_anchor_bits: before.scorer_bias_anchor_bits,
        };
        Self::from_snapshot_v1(self.model_v1().clone(), &after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> NativePolicyValueTrainStateV1 {
        NativePolicyValueTrainStateV1::new_v1(
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn carryover_probe_zero_moments_preserve_parameters_and_parent() {
        let original = state();
        let before = original.snapshot_v1().unwrap();
        let successor = original
            .diagnostic_zero_gradient_successor_v1()
            .unwrap()
            .snapshot_v1()
            .unwrap();
        assert_eq!(successor.parameters, before.parameters);
        assert_eq!(successor.first_moments, before.first_moments);
        assert_eq!(successor.second_moments, before.second_moments);
        assert_eq!(successor.adam_step, 1);
        assert_eq!(original.snapshot_v1().unwrap(), before);
    }

    #[test]
    fn carryover_probe_warm_moments_move_only_active_parameter_and_replay() {
        let template = state();
        let mut before = template.snapshot_v1().unwrap();
        before.adam_step = 32400;
        before.first_moments[SCORER_FIRST_WEIGHT].values[0] = 0.125;
        before.second_moments[SCORER_FIRST_WEIGHT].values[0] = 0.25;
        let original =
            NativePolicyValueTrainStateV1::from_snapshot_v1(template.model_v1().clone(), &before)
                .unwrap();
        let a = original
            .diagnostic_zero_gradient_successor_v1()
            .unwrap()
            .snapshot_v1()
            .unwrap();
        let b = original
            .diagnostic_zero_gradient_successor_v1()
            .unwrap()
            .snapshot_v1()
            .unwrap();
        assert_eq!(a, b);
        assert_eq!(a.adam_step, 32401);
        assert_ne!(
            a.parameters[SCORER_FIRST_WEIGHT].values[0].to_bits(),
            before.parameters[SCORER_FIRST_WEIGHT].values[0].to_bits()
        );
        assert!(
            a.first_moments[SCORER_FIRST_WEIGHT].values[0] > 0.0
                && a.first_moments[SCORER_FIRST_WEIGHT].values[0] < 0.125
        );
        let changed = a
            .parameters
            .iter()
            .zip(&before.parameters)
            .flat_map(|(a, b)| a.values.iter().zip(&b.values))
            .filter(|(a, b)| a.to_bits() != b.to_bits())
            .count();
        assert_eq!(changed, 1);
        assert_eq!(original.snapshot_v1().unwrap(), before);
    }
}
