/// Execution kernel for validated frame residual blocks.
///
/// Implementations must add every residual with modulo-256 arithmetic. The
/// decode plan supplies equal literal lengths, nonempty patterns, and complete
/// output ranges after syntax and bounds validation.
pub trait FrameDeltaKernel {
    fn add_literals(&mut self, output: &mut [u8], residuals: &[u8]);

    fn add_repeat(&mut self, output: &mut [u8], residual: u8);

    fn add_pattern(&mut self, output: &mut [u8], residuals: &[u8]);
}

/// Portable allocation-free frame residual execution.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScalarFrameDelta;

impl FrameDeltaKernel for ScalarFrameDelta {
    fn add_literals(&mut self, output: &mut [u8], residuals: &[u8]) {
        debug_assert_eq!(output.len(), residuals.len());
        for (output, residual) in output.iter_mut().zip(residuals) {
            *output = output.wrapping_add(*residual);
        }
    }

    fn add_repeat(&mut self, output: &mut [u8], residual: u8) {
        for output in output {
            *output = output.wrapping_add(residual);
        }
    }

    fn add_pattern(&mut self, output: &mut [u8], residuals: &[u8]) {
        debug_assert!(!residuals.is_empty());
        for (offset, output) in output.iter_mut().enumerate() {
            *output = output.wrapping_add(residuals[offset % residuals.len()]);
        }
    }
}

#[cfg(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "wasm32"
))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct PortableFrameDelta;

#[cfg(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "wasm32"
))]
impl PortableFrameDelta {
    pub(super) const LANES: usize = 16;
    pub(super) const MIN_VECTOR_BYTES: usize = Self::LANES * 4;

    fn add_vectors(output: &mut [u8], residuals: impl Fn(usize) -> [u8; Self::LANES]) -> usize {
        use wide::u8x16;

        if output.len() < Self::MIN_VECTOR_BYTES {
            return 0;
        }
        let vector_bytes = output.len() / Self::LANES * Self::LANES;
        for (index, output) in output[..vector_bytes]
            .chunks_exact_mut(Self::LANES)
            .enumerate()
        {
            let output: &mut [u8; Self::LANES] = output
                .try_into()
                .expect("chunks_exact_mut yields one complete vector");
            *output = (u8x16::new(*output) + u8x16::new(residuals(index))).to_array();
        }
        vector_bytes
    }
}

#[cfg(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "wasm32"
))]
impl FrameDeltaKernel for PortableFrameDelta {
    fn add_literals(&mut self, output: &mut [u8], residuals: &[u8]) {
        debug_assert_eq!(output.len(), residuals.len());
        let done = Self::add_vectors(output, |index| {
            residuals[index * Self::LANES..][..Self::LANES]
                .try_into()
                .expect("validated equal-length literal vector")
        });
        ScalarFrameDelta.add_literals(&mut output[done..], &residuals[done..]);
    }

    fn add_repeat(&mut self, output: &mut [u8], residual: u8) {
        let done = Self::add_vectors(output, |_| [residual; Self::LANES]);
        ScalarFrameDelta.add_repeat(&mut output[done..], residual);
    }

    fn add_pattern(&mut self, output: &mut [u8], residuals: &[u8]) {
        debug_assert!(!residuals.is_empty());
        if Self::LANES % residuals.len() != 0 {
            ScalarFrameDelta.add_pattern(output, residuals);
            return;
        }
        let lane = core::array::from_fn(|index| residuals[index % residuals.len()]);
        let done = Self::add_vectors(output, |_| lane);
        ScalarFrameDelta.add_pattern(&mut output[done..], residuals);
    }
}
