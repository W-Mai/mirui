use alloc::vec::Vec;
use core::alloc::Layout;
use mirui::core::cache::{Handle, handle::CacheEntry};

use crate::format;

pub use format::{GeneratedError, GeneratedLevel};

/// Validated, shared application-generated sprite geometry.
#[derive(Clone)]
pub struct GeneratedStackAsset(Handle<format::GeneratedStack>);

impl GeneratedStackAsset {
    /// Takes ownership of the planes and builds the layer bounds within the
    /// supplied storage budget. Caller-owned generator scratch is separate.
    pub fn new(
        extent: [u16; 3],
        pivot_q8: [i32; 3],
        spacing_q8: i32,
        palette: Vec<[u8; 4]>,
        levels: Vec<GeneratedLevel>,
        budget: usize,
    ) -> Result<Self, GeneratedError> {
        let budget = budget
            .checked_sub(Self::shared_entry_bytes())
            .ok_or(GeneratedError::Budget)?;
        let data =
            format::GeneratedStack::new(extent, pivot_q8, spacing_q8, palette, levels, budget)?;
        Ok(Self(Handle::new(data)))
    }

    /// Bytes charged to this asset, counted once across all cloned handles.
    pub fn storage_bytes(&self) -> usize {
        Self::shared_entry_bytes() + self.0.storage_bytes()
    }

    pub(crate) fn data(&self) -> &format::GeneratedStack {
        self.0.get()
    }

    fn shared_entry_bytes() -> usize {
        Layout::new::<[usize; 2]>()
            .extend(Layout::new::<CacheEntry<format::GeneratedStack>>())
            .expect("generated stack handle layout")
            .0
            .pad_to_align()
            .size()
    }
}

impl PartialEq for GeneratedStackAsset {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

impl Eq for GeneratedStackAsset {}
