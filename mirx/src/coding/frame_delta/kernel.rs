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

/// x86-64 SSE2 frame residual execution.
///
/// SSE2 is part of the x86-64 baseline. Unaligned loads and stores keep input
/// and output alignment independent; scalar tails preserve exact modulo-256
/// behavior.
#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Sse2FrameDelta;

#[cfg(target_arch = "x86_64")]
impl Sse2FrameDelta {
    fn literal_vectors(output: &mut [u8], residuals: &[u8]) -> usize {
        use core::arch::x86_64::{_mm_add_epi8, _mm_loadu_si128, _mm_storeu_si128};

        let vectors = output.len() / 16;
        for index in 0..vectors {
            let offset = index * 16;
            // SAFETY: each unaligned vector lies inside equal-length slices;
            // x86-64 guarantees SSE2 support.
            unsafe {
                let value = _mm_loadu_si128(output.as_ptr().add(offset).cast());
                let residual = _mm_loadu_si128(residuals.as_ptr().add(offset).cast());
                _mm_storeu_si128(
                    output.as_mut_ptr().add(offset).cast(),
                    _mm_add_epi8(value, residual),
                );
            }
        }
        vectors * 16
    }

    fn repeat_vectors(output: &mut [u8], residual: u8) -> usize {
        use core::arch::x86_64::{_mm_add_epi8, _mm_loadu_si128, _mm_set1_epi8, _mm_storeu_si128};

        let vectors = output.len() / 16;
        // The signed argument preserves the residual byte's bit pattern.
        let residual = unsafe { _mm_set1_epi8(residual as i8) };
        for index in 0..vectors {
            let offset = index * 16;
            // SAFETY: each unaligned vector lies inside output; x86-64
            // guarantees SSE2 support.
            unsafe {
                let value = _mm_loadu_si128(output.as_ptr().add(offset).cast());
                _mm_storeu_si128(
                    output.as_mut_ptr().add(offset).cast(),
                    _mm_add_epi8(value, residual),
                );
            }
        }
        vectors * 16
    }

    fn pattern_vectors(output: &mut [u8], residuals: &[u8; 16]) -> usize {
        use core::arch::x86_64::{_mm_add_epi8, _mm_loadu_si128, _mm_storeu_si128};

        let vectors = output.len() / 16;
        // SAFETY: residuals contains one complete unaligned vector.
        let residual = unsafe { _mm_loadu_si128(residuals.as_ptr().cast()) };
        for index in 0..vectors {
            let offset = index * 16;
            // SAFETY: each unaligned vector lies inside output; x86-64
            // guarantees SSE2 support.
            unsafe {
                let value = _mm_loadu_si128(output.as_ptr().add(offset).cast());
                _mm_storeu_si128(
                    output.as_mut_ptr().add(offset).cast(),
                    _mm_add_epi8(value, residual),
                );
            }
        }
        vectors * 16
    }
}

#[cfg(target_arch = "x86_64")]
impl FrameDeltaKernel for Sse2FrameDelta {
    fn add_literals(&mut self, output: &mut [u8], residuals: &[u8]) {
        debug_assert_eq!(output.len(), residuals.len());
        let done = Self::literal_vectors(output, residuals);
        ScalarFrameDelta.add_literals(&mut output[done..], &residuals[done..]);
    }

    fn add_repeat(&mut self, output: &mut [u8], residual: u8) {
        let done = Self::repeat_vectors(output, residual);
        ScalarFrameDelta.add_repeat(&mut output[done..], residual);
    }

    fn add_pattern(&mut self, output: &mut [u8], residuals: &[u8]) {
        debug_assert!(!residuals.is_empty());
        if 16 % residuals.len() != 0 {
            ScalarFrameDelta.add_pattern(output, residuals);
            return;
        }
        let mut lane = [0; 16];
        for (index, value) in lane.iter_mut().enumerate() {
            *value = residuals[index % residuals.len()];
        }
        let done = Self::pattern_vectors(output, &lane);
        ScalarFrameDelta.add_pattern(&mut output[done..], residuals);
    }
}

/// AArch64 NEON frame residual execution.
///
/// AArch64 always includes Advanced SIMD. Unaligned slice addresses are valid
/// for the load/store instructions used here; scalar tails preserve exact
/// modulo-256 behavior.
#[cfg(target_arch = "aarch64")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NeonFrameDelta;

#[cfg(target_arch = "aarch64")]
impl NeonFrameDelta {
    fn literal_vectors(output: &mut [u8], residuals: &[u8]) -> usize {
        use core::arch::aarch64::{vaddq_u8, vld1q_u8, vst1q_u8};

        let vectors = output.len() / 16;
        for index in 0..vectors {
            let offset = index * 16;
            // SAFETY: each vector lies inside equal-length slices. AArch64
            // vector loads and stores permit unaligned addresses.
            unsafe {
                let value = vld1q_u8(output.as_ptr().add(offset));
                let residual = vld1q_u8(residuals.as_ptr().add(offset));
                vst1q_u8(output.as_mut_ptr().add(offset), vaddq_u8(value, residual));
            }
        }
        vectors * 16
    }

    fn repeat_vectors(output: &mut [u8], residual: u8) -> usize {
        use core::arch::aarch64::{vaddq_u8, vdupq_n_u8, vld1q_u8, vst1q_u8};

        let vectors = output.len() / 16;
        // SAFETY: constructing a vector has no memory precondition.
        let residual = unsafe { vdupq_n_u8(residual) };
        for index in 0..vectors {
            let offset = index * 16;
            // SAFETY: each vector lies inside output. AArch64 vector loads and
            // stores permit unaligned addresses.
            unsafe {
                let value = vld1q_u8(output.as_ptr().add(offset));
                vst1q_u8(output.as_mut_ptr().add(offset), vaddq_u8(value, residual));
            }
        }
        vectors * 16
    }

    fn pattern_vectors(output: &mut [u8], residuals: &[u8; 16]) -> usize {
        use core::arch::aarch64::{vaddq_u8, vld1q_u8, vst1q_u8};

        let vectors = output.len() / 16;
        // SAFETY: residuals contains one complete vector.
        let residual = unsafe { vld1q_u8(residuals.as_ptr()) };
        for index in 0..vectors {
            let offset = index * 16;
            // SAFETY: each vector lies inside output. AArch64 vector loads and
            // stores permit unaligned addresses.
            unsafe {
                let value = vld1q_u8(output.as_ptr().add(offset));
                vst1q_u8(output.as_mut_ptr().add(offset), vaddq_u8(value, residual));
            }
        }
        vectors * 16
    }
}

#[cfg(target_arch = "aarch64")]
impl FrameDeltaKernel for NeonFrameDelta {
    fn add_literals(&mut self, output: &mut [u8], residuals: &[u8]) {
        debug_assert_eq!(output.len(), residuals.len());
        let done = Self::literal_vectors(output, residuals);
        ScalarFrameDelta.add_literals(&mut output[done..], &residuals[done..]);
    }

    fn add_repeat(&mut self, output: &mut [u8], residual: u8) {
        let done = Self::repeat_vectors(output, residual);
        ScalarFrameDelta.add_repeat(&mut output[done..], residual);
    }

    fn add_pattern(&mut self, output: &mut [u8], residuals: &[u8]) {
        debug_assert!(!residuals.is_empty());
        if 16 % residuals.len() != 0 {
            ScalarFrameDelta.add_pattern(output, residuals);
            return;
        }
        let mut lane = [0; 16];
        for (index, value) in lane.iter_mut().enumerate() {
            *value = residuals[index % residuals.len()];
        }
        let done = Self::pattern_vectors(output, &lane);
        ScalarFrameDelta.add_pattern(&mut output[done..], residuals);
    }
}
