//! Portable device validity; the renderer and its operations remain owner-local.

use crate::render::{RenderError, budget::Allocation, target::TargetStorage};
use core::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

pub(crate) struct DeviceStatus {
    #[cfg(target_has_atomic = "64")]
    generation: core::sync::atomic::AtomicU64,
    #[cfg(not(target_has_atomic = "64"))]
    generation: crate::core::threading::Gate<u64>,
    failed: AtomicBool,
    charge: OnceLock<Allocation>,
}

impl DeviceStatus {
    pub(crate) fn new() -> Self {
        Self {
            #[cfg(target_has_atomic = "64")]
            generation: core::sync::atomic::AtomicU64::new(0),
            #[cfg(not(target_has_atomic = "64"))]
            generation: crate::core::threading::Gate::new(0),
            failed: AtomicBool::new(false),
            charge: OnceLock::new(),
        }
    }

    pub(crate) fn generation(&self) -> u64 {
        #[cfg(target_has_atomic = "64")]
        {
            self.generation.load(Ordering::Acquire)
        }
        #[cfg(not(target_has_atomic = "64"))]
        {
            self.generation
                .with(|value| *value)
                .expect("bounded generation access")
        }
    }

    pub(crate) fn check(&self) -> Result<(), RenderError> {
        if self.failed.load(Ordering::Acquire) {
            Err(RenderError::BackendFailure)
        } else {
            Ok(())
        }
    }

    pub(crate) fn check_generation(&self, generation: u64) -> Result<(), RenderError> {
        self.check()?;
        if self.generation() != generation {
            Err(RenderError::InvalidTexture)
        } else {
            Ok(())
        }
    }

    pub(crate) fn poison(&self) {
        self.failed.store(true, Ordering::Release);
    }

    pub(crate) fn reset(&self) {
        #[cfg(target_has_atomic = "64")]
        self.generation.fetch_add(1, Ordering::AcqRel);
        #[cfg(not(target_has_atomic = "64"))]
        self.generation
            .with(|value| *value = value.wrapping_add(1))
            .expect("bounded generation access");
        self.failed.store(false, Ordering::Release);
    }

    pub(crate) fn metadata_bytes(&self) -> usize {
        if self.charge.get().is_some() {
            0
        } else {
            core::mem::size_of::<Self>() + 2 * core::mem::size_of::<usize>()
        }
    }

    pub(crate) fn retain_storage(&self, storage: &TargetStorage) -> Result<(), RenderError> {
        let bytes = self.metadata_bytes();
        if bytes == 0 {
            return Ok(());
        }
        // Called only by target admission on the device owner thread.
        self.charge
            .set(storage.split_control(bytes)?)
            .map_err(|_| RenderError::InvalidScope)
    }
}
