use crate::render::RenderError;

#[derive(Default)]
pub(super) struct BufferRotation {
    pub drawing: usize,
    scanout: usize,
    pub pending: bool,
}

impl BufferRotation {
    pub fn prepare(&mut self, count: usize) -> Result<(), RenderError> {
        if self.pending {
            return Err(RenderError::SurfaceUnavailable);
        }
        if count > 1 && self.drawing == self.scanout {
            self.drawing = (self.drawing + 1) % count;
        }
        Ok(())
    }

    pub fn complete(&mut self) {
        self.scanout = self.drawing;
        self.pending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_flip_never_releases_a_buffer_without_completion() {
        for count in 2..=4 {
            let mut state = BufferRotation::default();
            for _ in 0..8 {
                state.prepare(count).unwrap();
                assert_ne!(state.drawing, state.scanout);
                let drawing = state.drawing;
                state.pending = true;
                for _ in 0..3 {
                    assert_eq!(state.prepare(count), Err(RenderError::SurfaceUnavailable));
                    assert_eq!(state.drawing, drawing);
                    assert!(state.pending);
                }
                state.complete();
                assert_eq!(
                    state.drawing, drawing,
                    "the completed slot remains the mirror source"
                );
                assert_eq!(state.scanout, drawing);
            }
        }
    }

    #[test]
    fn failed_submission_keeps_its_writable_slot_and_single_buffer_stays_direct() {
        let mut state = BufferRotation::default();
        state.prepare(2).unwrap();
        let drawing = state.drawing;
        state.prepare(2).unwrap();
        assert_eq!(state.drawing, drawing);
        assert_eq!(state.scanout, 0);

        let mut direct = BufferRotation::default();
        for _ in 0..4 {
            direct.prepare(1).unwrap();
            assert_eq!(direct.drawing, 0);
            direct.complete();
        }
    }
}
