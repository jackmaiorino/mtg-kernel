//! The sequential backward with the exact parallel backward's kernels.
//!
//! `AccumulateGradientsV1` adds a layer's terms to the weight gradient in the
//! parameter's layout, [output, input]: one pass over every input for every
//! output of every row. Here a hidden-width weight's gradient is accumulated
//! transposed, [input, output], and written back in the parameter's layout
//! after the last substep (`GradientTensorsV1`). A layer row's output
//! gradient stays in registers while the row's inputs are walked, and an
//! input that is zero costs one comparison where leaving its terms out
//! changes no bit. The input gradient keeps its sums in registers and
//! computes only the columns its caller reads (`input_gradient_rows_v1`).
//!
//! Every gradient element, and every input gradient element that is read,
//! receives the terms `AccumulateGradientsV1` gives it, in its order, so every
//! bit is the same. `AccumulateGradientsV1` stays the reference: the other
//! train steps use it, and so does this one under a test's capture.

use super::exact_parallel_backward_v1::{
    add_input_major_layer_v1, input_gradient_rows_v1, GradientTensorsV1,
};
use super::*;

/// The sequential backward's gradient tensors, for `AccumulateTransposedV1`.
/// A test that captures the train step, or asks for the reference, gets
/// every tensor in its own shape, which sends every layer through
/// `linear_backward_into`.
pub(super) fn sequential_gradients_dims_v1<D: NetDimsV1>(
    parameters: &[NativeNamedParameterV1],
) -> GradientTensorsV1 {
    #[cfg(test)]
    if train_step_capture_active_v1() || REFERENCE_FOR_TEST_V1.with(Cell::get) {
        return GradientTensorsV1::in_parameter_shapes_v1::<D>(parameters);
    }
    GradientTensorsV1::zeroed_dims_v1::<D>(parameters)
}

/// The sequential backward: each term is added to `gradients` when computed,
/// a hidden-width weight's transposed.
///
/// The terms of a layer's zero inputs are left out only where that changes
/// no bit:
///
/// - A term is the row's output gradient times the input. With that
///   gradient finite and the input a zero, it is a zero of either sign.
/// - With round-to-nearest, adding a zero changes a sum only if the sum is
///   a negative zero or a signaling NaN. The tensors start at +0.0 and only
///   this sink writes to them. A sum that is not a negative zero never
///   becomes one (two numbers sum to a negative zero only when both are),
///   and arithmetic never yields a signaling NaN. So no element is either
///   while every layer so far was added in the pinned MXCSR state.
/// - That state is read at every layer. Once a layer finds the thread
///   outside it, that layer and every later one add every term, as does a
///   target without that register.
pub(super) struct AccumulateTransposedV1<'g> {
    gradients: &'g mut GradientTensorsV1,
    /// Whether this thread was in the pinned MXCSR state at every layer so
    /// far.
    pinned: bool,
}

impl<'g> AccumulateTransposedV1<'g> {
    /// `gradients` as `sequential_gradients_dims_v1` made them, all +0.0.
    pub(super) fn new_v1(gradients: &'g mut GradientTensorsV1) -> Self {
        Self {
            gradients,
            pinned: true,
        }
    }
}

impl<'t> ReverseGradientSinkV1<'t> for AccumulateTransposedV1<'_> {
    fn linear_v1(
        &mut self,
        parameters: &[NativeNamedParameterV1],
        weight_index: usize,
        bias_index: usize,
        input: &'t [f32],
        rows: usize,
        input_dim: usize,
        output_dim: usize,
        d_output: &[f32],
        d_input: &mut Vec<f32>,
    ) -> Result<(), NativePolicyTrainErrorV1> {
        self.linear_read_from_v1(
            parameters,
            weight_index,
            bias_index,
            input,
            rows,
            input_dim,
            output_dim,
            d_output,
            0,
            d_input,
        )
    }

    /// Computes the input gradient's columns from `first_read` on and leaves
    /// the columns before it zero: nothing reads them.
    fn linear_read_from_v1(
        &mut self,
        parameters: &[NativeNamedParameterV1],
        weight_index: usize,
        bias_index: usize,
        input: &'t [f32],
        rows: usize,
        input_dim: usize,
        output_dim: usize,
        d_output: &[f32],
        first_read: usize,
        d_input: &mut Vec<f32>,
    ) -> Result<(), NativePolicyTrainErrorV1> {
        let GradientTensorsV1 {
            gradients,
            input_major,
            hidden,
        } = &mut *self.gradients;
        if input_major[weight_index].is_empty() {
            // A tensor in its own shape takes the reference's loop.
            return AccumulateGradientsV1(gradients).linear_v1(
                parameters,
                weight_index,
                bias_index,
                input,
                rows,
                input_dim,
                output_dim,
                d_output,
                d_input,
            );
        }
        let weight = &parameters[weight_index].values;
        // `linear_backward_into`'s checks, and the width the transposed
        // tensor was allocated for.
        if input.len() != rows * input_dim
            || d_output.len() != rows * output_dim
            || weight.len() != output_dim * input_dim
            || gradients[weight_index].len() != weight.len()
            || gradients[bias_index].len() != output_dim
            || output_dim != *hidden
        {
            return Err(NativePolicyTrainErrorV1::ParameterManifest);
        }
        let first_read = first_read.min(input_dim);
        resize_zeroed_v1(d_input, rows * input_dim);
        input_gradient_rows_v1(weight, input_dim, output_dim, d_output, first_read, d_input);
        // A test that reads a column this sink did not compute meets a NaN.
        #[cfg(test)]
        if first_read > 0 {
            for d_input_row in d_input.chunks_exact_mut(input_dim) {
                d_input_row[..first_read].fill(f32::NAN);
            }
        }
        // Each bias element receives the rows in the reference's order: from
        // the last row back for the scorer's second layer
        // (`linear_backward_into`), from the first on for every other.
        let bias = &mut gradients[bias_index];
        let mut add_row = |d_output_row: &[f32]| {
            for (sum, d_value) in bias.iter_mut().zip(d_output_row) {
                *sum += *d_value;
            }
        };
        let d_output_rows = d_output.chunks_exact(output_dim);
        if weight_index == SCORER_SECOND_WEIGHT {
            d_output_rows.rev().for_each(&mut add_row);
        } else {
            d_output_rows.for_each(&mut add_row);
        }
        self.pinned &= rounds_to_nearest_v1();
        // No early exit, so the pass vectorizes.
        let pass_zero_inputs = self.pinned
            && d_output
                .iter()
                .fold(true, |finite, value| finite & value.is_finite());
        #[cfg(test)]
        TRANSPOSED_LAYERS_FOR_TEST_V1.with(|layers| {
            let (all, passing) = layers.get();
            layers.set((all + 1, passing + u64::from(pass_zero_inputs)));
        });
        add_input_major_layer_v1(
            *hidden,
            &mut input_major[weight_index],
            input,
            input_dim,
            d_output,
            pass_zero_inputs,
        );
        Ok(())
    }

    fn card_embedding_dims_v1<D: NetDimsV1>(
        &mut self,
        object_card_ids: &'t [usize],
        d_object_input: &[f32],
    ) {
        AccumulateGradientsV1(&mut self.gradients.gradients)
            .card_embedding_dims_v1::<D>(object_card_ids, d_object_input);
    }
}

#[cfg(test)]
thread_local! {
    /// Makes this thread's sequential backward the reference's loops.
    static REFERENCE_FOR_TEST_V1: Cell<bool> = const { Cell::new(false) };
    /// Layers this thread accumulated transposed, and how many of them with
    /// their zero inputs passed over.
    static TRANSPOSED_LAYERS_FOR_TEST_V1: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
}

/// Test-only: runs `run` with this thread's sequential backward on the
/// reference's loops.
#[cfg(test)]
pub(super) fn with_reference_sequential_backward_for_test_v1<R>(run: impl FnOnce() -> R) -> R {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            REFERENCE_FOR_TEST_V1.with(|reference| reference.set(self.0));
        }
    }
    let _restore = Restore(REFERENCE_FOR_TEST_V1.with(|reference| reference.replace(true)));
    run()
}

/// Test-only: the layers this thread has accumulated transposed so far, and
/// how many of them with their zero inputs passed over.
#[cfg(test)]
pub(super) fn transposed_layers_for_test_v1() -> (u64, u64) {
    TRANSPOSED_LAYERS_FOR_TEST_V1.with(Cell::get)
}
