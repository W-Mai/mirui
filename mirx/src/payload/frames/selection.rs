use crate::coding::CodingId;

/// Storage relationship of one encoded frame candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameStorage {
    /// No groups or DATA; the retained canvas is displayed unchanged.
    Omitted,
    /// Complete independently decodable surface coverage.
    Keyframe,
    /// Changed regions applied to the retained canvas.
    Sparse,
    /// Previous-frame residuals applied to the retained canvas.
    Delta,
}

impl FrameStorage {
    pub const fn is_independent(self) -> bool {
        matches!(self, Self::Keyframe)
    }

    const fn preference(self) -> u8 {
        match self {
            Self::Keyframe => 0,
            Self::Omitted => 1,
            Self::Sparse => 2,
            Self::Delta => 3,
        }
    }
}

/// One already-sized frame representation offered to [`FrameSelector`].
///
/// `stored_bytes` is the candidate's complete incremental contribution,
/// including group records, indexes, alignment padding, DATA, and any setup
/// metadata not already active in the sequence. Comparing only codec bodies is
/// not sufficient.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameCandidate {
    storage: FrameStorage,
    coding: Option<CodingId>,
    stored_bytes: u64,
    decode_work: u64,
    workspace_bytes: u64,
    lossless: bool,
}

impl FrameCandidate {
    pub const fn omitted() -> Self {
        Self {
            storage: FrameStorage::Omitted,
            coding: None,
            stored_bytes: 0,
            decode_work: 0,
            workspace_bytes: 0,
            lossless: true,
        }
    }

    pub const fn keyframe(coding: CodingId, stored_bytes: u64) -> Self {
        Self::new(FrameStorage::Keyframe, coding, stored_bytes)
    }

    pub const fn sparse(coding: CodingId, stored_bytes: u64) -> Self {
        Self::new(FrameStorage::Sparse, coding, stored_bytes)
    }

    pub const fn delta(stored_bytes: u64) -> Self {
        Self::new(FrameStorage::Delta, CodingId::FRAME_DELTA, stored_bytes)
    }

    const fn new(storage: FrameStorage, coding: CodingId, stored_bytes: u64) -> Self {
        Self {
            storage,
            coding: Some(coding),
            stored_bytes,
            decode_work: 0,
            workspace_bytes: 0,
            lossless: true,
        }
    }

    pub const fn with_decode_cost(mut self, work: u64, workspace_bytes: u64) -> Self {
        self.decode_work = work;
        self.workspace_bytes = workspace_bytes;
        self
    }

    pub const fn lossy(mut self) -> Self {
        self.lossless = false;
        self
    }

    pub const fn storage(self) -> FrameStorage {
        self.storage
    }

    pub const fn coding(self) -> Option<CodingId> {
        self.coding
    }

    pub const fn stored_bytes(self) -> u64 {
        self.stored_bytes
    }

    pub const fn decode_work(self) -> u64 {
        self.decode_work
    }

    pub const fn workspace_bytes(self) -> u64 {
        self.workspace_bytes
    }

    pub const fn is_lossless(self) -> bool {
        self.lossless
    }

    fn selection_key(self) -> (u64, bool, u64, u64, u8, u16) {
        (
            self.stored_bytes,
            !self.storage.is_independent(),
            self.decode_work,
            self.workspace_bytes,
            self.storage.preference(),
            self.coding.map_or(u16::MAX, CodingId::raw),
        )
    }

    fn is_better_than(self, current: Self) -> bool {
        self.selection_key() < current.selection_key()
    }
}

/// Resource and recovery constraints applied before comparing candidates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FramePolicy {
    max_delta_frames: u16,
    max_stored_bytes: u64,
    max_decode_work: u64,
    max_workspace_bytes: u64,
    allow_lossy: bool,
}

impl FramePolicy {
    pub const fn new(max_delta_frames: u16) -> Self {
        Self {
            max_delta_frames,
            max_stored_bytes: u64::MAX,
            max_decode_work: u64::MAX,
            max_workspace_bytes: u64::MAX,
            allow_lossy: false,
        }
    }

    pub const fn allow_lossy(mut self) -> Self {
        self.allow_lossy = true;
        self
    }

    pub const fn with_max_stored_bytes(mut self, bytes: u64) -> Self {
        self.max_stored_bytes = bytes;
        self
    }

    pub const fn with_max_decode_work(mut self, work: u64) -> Self {
        self.max_decode_work = work;
        self
    }

    pub const fn with_max_workspace_bytes(mut self, bytes: u64) -> Self {
        self.max_workspace_bytes = bytes;
        self
    }

    pub const fn max_delta_frames(self) -> u16 {
        self.max_delta_frames
    }

    pub const fn allows_lossy(self) -> bool {
        self.allow_lossy
    }
}

/// Stateful, allocation-free frame candidate selector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameSelector {
    policy: FramePolicy,
    frame: u32,
    delta_frames: u16,
}

impl FrameSelector {
    pub const fn new(policy: FramePolicy) -> Self {
        Self {
            policy,
            frame: 0,
            delta_frames: 0,
        }
    }

    pub const fn frame(&self) -> u32 {
        self.frame
    }

    pub const fn delta_frames(&self) -> u16 {
        self.delta_frames
    }

    pub const fn policy(&self) -> FramePolicy {
        self.policy
    }

    /// Explains whether a candidate can be selected for the current frame.
    pub const fn admission(&self, candidate: FrameCandidate) -> Result<(), CandidateRejection> {
        if self.frame == 0 && !candidate.storage.is_independent() {
            return Err(CandidateRejection::FirstFrameNeedsKeyframe);
        }
        if !candidate.storage.is_independent() && self.delta_frames >= self.policy.max_delta_frames
        {
            return Err(CandidateRejection::DeltaBound {
                limit: self.policy.max_delta_frames,
            });
        }
        if !candidate.lossless && !self.policy.allow_lossy {
            return Err(CandidateRejection::LossyNotAllowed);
        }
        if candidate.stored_bytes > self.policy.max_stored_bytes {
            return Err(CandidateRejection::StoredBytes {
                needed: candidate.stored_bytes,
                limit: self.policy.max_stored_bytes,
            });
        }
        if candidate.decode_work > self.policy.max_decode_work {
            return Err(CandidateRejection::DecodeWork {
                needed: candidate.decode_work,
                limit: self.policy.max_decode_work,
            });
        }
        if candidate.workspace_bytes > self.policy.max_workspace_bytes {
            return Err(CandidateRejection::Workspace {
                needed: candidate.workspace_bytes,
                limit: self.policy.max_workspace_bytes,
            });
        }
        Ok(())
    }

    /// Selects one admissible candidate and advances sequence state atomically.
    ///
    /// Complete stored bytes win first. Ties prefer independent recovery, then
    /// lower decode work, lower workspace, and a stable storage/profile order.
    /// Failure leaves the selector unchanged.
    pub fn select(
        &mut self,
        candidates: &[FrameCandidate],
    ) -> Result<FrameChoice, FrameSelectionError> {
        let mut selected = None;
        for &candidate in candidates {
            if self.admission(candidate).is_err() {
                continue;
            }
            if selected.is_none_or(|current| candidate.is_better_than(current)) {
                selected = Some(candidate);
            }
        }
        let candidate = selected.ok_or(FrameSelectionError::NoCandidate { frame: self.frame })?;
        let delta_frames = if candidate.storage.is_independent() {
            0
        } else {
            self.delta_frames + 1
        };
        let choice = FrameChoice {
            frame: self.frame,
            candidate,
            delta_frames,
        };
        self.frame = self
            .frame
            .checked_add(1)
            .ok_or(FrameSelectionError::FrameOverflow)?;
        self.delta_frames = delta_frames;
        Ok(choice)
    }
}

/// Selected frame representation and the resulting recovery distance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameChoice {
    frame: u32,
    candidate: FrameCandidate,
    delta_frames: u16,
}

impl FrameChoice {
    pub const fn frame(self) -> u32 {
        self.frame
    }

    pub const fn candidate(self) -> FrameCandidate {
        self.candidate
    }

    pub const fn delta_frames(self) -> u16 {
        self.delta_frames
    }
}

/// Reason one candidate is inadmissible for the current frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CandidateRejection {
    FirstFrameNeedsKeyframe,
    DeltaBound { limit: u16 },
    LossyNotAllowed,
    StoredBytes { needed: u64, limit: u64 },
    DecodeWork { needed: u64, limit: u64 },
    Workspace { needed: u64, limit: u64 },
}

/// Failure to select or advance one frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FrameSelectionError {
    NoCandidate { frame: u32 },
    FrameOverflow,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_frame_and_recovery_bound_force_keyframes() {
        let mut selector = FrameSelector::new(FramePolicy::new(2));
        let delta = FrameCandidate::delta(1);
        assert_eq!(
            selector.admission(delta),
            Err(CandidateRejection::FirstFrameNeedsKeyframe)
        );
        assert_eq!(
            selector.select(&[delta]),
            Err(FrameSelectionError::NoCandidate { frame: 0 })
        );
        assert_eq!((selector.frame(), selector.delta_frames()), (0, 0));

        let keyframe = FrameCandidate::keyframe(CodingId::LZ4, 20);
        assert_eq!(
            selector.select(&[delta, keyframe]).unwrap().candidate(),
            keyframe
        );
        assert_eq!(
            selector.select(&[delta, keyframe]).unwrap().candidate(),
            delta
        );
        assert_eq!(
            selector.select(&[delta, keyframe]).unwrap().candidate(),
            delta
        );
        assert_eq!(
            selector.admission(delta),
            Err(CandidateRejection::DeltaBound { limit: 2 })
        );
        assert_eq!(
            selector.select(&[delta, keyframe]).unwrap().candidate(),
            keyframe
        );
    }

    #[test]
    fn complete_bytes_win_and_ties_improve_recovery_then_runtime_cost() {
        let mut selector = FrameSelector::new(FramePolicy::new(8));
        selector
            .select(&[FrameCandidate::keyframe(CodingId::RAW, 1)])
            .unwrap();
        let cheap_delta = FrameCandidate::delta(9).with_decode_cost(20, 5);
        let expensive_keyframe = FrameCandidate::keyframe(CodingId::RLE, 10).with_decode_cost(1, 1);
        assert_eq!(
            selector
                .select(&[expensive_keyframe, cheap_delta])
                .unwrap()
                .candidate(),
            cheap_delta
        );

        let delta = FrameCandidate::delta(10).with_decode_cost(1, 1);
        assert_eq!(
            selector
                .select(&[delta, expensive_keyframe])
                .unwrap()
                .candidate(),
            expensive_keyframe
        );

        let sparse_high_work = FrameCandidate::sparse(CodingId::PIXEL, 10).with_decode_cost(4, 1);
        let sparse_low_work = FrameCandidate::sparse(CodingId::LZ4, 10).with_decode_cost(3, 2);
        assert_eq!(
            selector
                .select(&[sparse_high_work, sparse_low_work])
                .unwrap()
                .candidate(),
            sparse_low_work
        );
    }

    #[test]
    fn policy_rejections_are_independent_and_visible() {
        let policy = FramePolicy::new(1)
            .with_max_stored_bytes(10)
            .with_max_decode_work(20)
            .with_max_workspace_bytes(30);
        let selector = FrameSelector {
            policy,
            frame: 1,
            delta_frames: 0,
        };
        assert_eq!(
            selector.admission(FrameCandidate::keyframe(CodingId::RAW, 1).lossy()),
            Err(CandidateRejection::LossyNotAllowed)
        );
        assert_eq!(
            selector.admission(FrameCandidate::delta(11)),
            Err(CandidateRejection::StoredBytes {
                needed: 11,
                limit: 10
            })
        );
        assert_eq!(
            selector.admission(FrameCandidate::delta(1).with_decode_cost(21, 1)),
            Err(CandidateRejection::DecodeWork {
                needed: 21,
                limit: 20
            })
        );
        assert_eq!(
            selector.admission(FrameCandidate::delta(1).with_decode_cost(1, 31)),
            Err(CandidateRejection::Workspace {
                needed: 31,
                limit: 30
            })
        );
        assert!(
            FrameSelector::new(policy.allow_lossy())
                .admission(FrameCandidate::keyframe(CodingId::RAW, 1).lossy())
                .is_ok()
        );
    }
}
