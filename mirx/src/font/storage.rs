#![doc = include_str!("../../docs/glyph-storage.md")]

use super::{GlyphMap, GlyphPacking};
use crate::image::{
    ColorDescription, PlaneMemoryError, PlaneMemoryLayout, RawImageAsset, Region, RegionMemoryPlan,
    SampleLayout, SurfaceCopyError, SurfaceDescriptor, SurfacePlanError, SurfaceRequirements,
    SurfaceView,
};

/// Borrowed scalar glyph samples with shared physical storage rules.
///
/// GlyphMajor stores independently padded cells; Atlas2D stores one shared
/// plane. Lookup retains source bytes without decoding or copying a map.
#[derive(Clone, Copy, Debug)]
pub struct RawGlyphs<'map, 'data> {
    map: GlyphMap<'map>,
    surface: SurfaceDescriptor,
    memory: PlaneMemoryLayout,
    cell_step: u64,
    data: &'data [u8],
}

impl<'map, 'data> RawGlyphs<'map, 'data> {
    pub const fn builder(map: GlyphMap<'map>, layout: SampleLayout) -> RawGlyphBuilder<'map> {
        RawGlyphBuilder {
            map,
            layout,
            memory: None,
        }
    }

    pub const fn map(self) -> GlyphMap<'map> {
        self.map
    }

    /// Shared cell allocation for GlyphMajor, or whole-atlas allocation for Atlas2D.
    pub const fn memory_layout(self) -> PlaneMemoryLayout {
        self.memory
    }

    pub const fn sample_layout(self) -> SampleLayout {
        self.surface.sample_layout()
    }

    /// Exact bound including initial and inter-cell gaps, without a trailing cell gap.
    pub const fn byte_len(self) -> u32 {
        self.data.len() as u32
    }

    /// The complete stored range, including declared cell and row padding.
    pub const fn as_bytes(self) -> &'data [u8] {
        self.data
    }

    pub fn len(self) -> usize {
        self.map.len()
    }

    pub fn is_empty(self) -> bool {
        self.map.is_empty()
    }

    /// Resolves one glyph in constant time without retaining the map's lifetime.
    pub fn get(self, index: usize) -> Option<GlyphRaster<'data>> {
        let mapped = self.map.get(index)?;
        let (offset, region) = match self.map.packing() {
            GlyphPacking::GlyphMajor => (
                (u64::from(self.memory.data_offset()) + index as u64 * self.cell_step) as u32,
                self.surface
                    .region(0, 0, self.surface.width(), self.surface.height())
                    .expect("validated cell extent"),
            ),
            GlyphPacking::Atlas2D => (self.memory.data_offset(), mapped),
        };
        let memory = PlaneMemoryLayout::builder(self.surface.plane(0).expect("scalar glyph plane"))
            .with_allocation_extent(
                self.memory.allocation_width(),
                self.memory.allocation_height(),
            )
            .with_stride(self.memory.stride())
            .with_data_offset(offset)
            .with_alignment(self.memory.required_alignment())
            .with_flags(self.memory.flags())
            .build()
            .expect("validated repeated allocation");
        let planes = [memory.bytes(self.data).expect("validated glyph DATA range")];
        let storage = RawImageAsset::new(self.surface, &planes)
            .with_memory_layouts(&[memory])
            .view()
            .expect("validated scalar glyph storage");
        Some(GlyphRaster { storage, region })
    }

    /// Checks real source addresses separately from declared file placement.
    /// Misaligned input remains available for CPU reads and explicit transfer.
    pub fn data_addresses_are_aligned(self) -> bool {
        self.data.is_empty() || self.memory.runtime_address_is_aligned(self.data)
    }

    /// Checks the group's complete address range and first stored plane.
    /// Derived cell steps preserve that alignment for every subsequent glyph.
    pub const fn file_address_is_aligned(self, data_offset: u32) -> bool {
        data_offset.checked_add(self.byte_len()).is_some()
            && (self.data.is_empty() || self.memory.file_address_is_aligned(data_offset))
    }
}

/// Builder for borrowed RAW glyph storage with tight physical defaults.
#[derive(Clone, Copy, Debug)]
pub struct RawGlyphBuilder<'map> {
    map: GlyphMap<'map>,
    layout: SampleLayout,
    memory: Option<PlaneMemoryLayout>,
}

impl<'map> RawGlyphBuilder<'map> {
    pub const fn with_memory_layout(mut self, memory: PlaneMemoryLayout) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Checks geometry and the exact DATA span without scanning sample bytes.
    pub fn build<'data>(
        self,
        data: &'data [u8],
    ) -> Result<RawGlyphs<'map, 'data>, GlyphStorageError> {
        if !self.layout.is_alpha() {
            return Err(GlyphStorageError::UnsupportedLayout(self.layout));
        }
        let (width, height) = self
            .map
            .cell_extent()
            .unwrap_or((self.map.width(), self.map.height()));
        let surface = SurfaceDescriptor::new(width, height, self.layout, ColorDescription::NONE)
            .expect("known scalar sample layout");
        let plane = surface.plane(0).expect("scalar glyph plane");
        let memory = match self.memory {
            Some(memory) => {
                memory
                    .validate_for(plane)
                    .map_err(GlyphStorageError::Memory)?;
                memory
            }
            None => PlaneMemoryLayout::tight(plane).map_err(GlyphStorageError::Memory)?,
        };
        // Widen alignment arithmetic so one huge cell need not have a
        // representable next-cell address that no glyph will ever use.
        let alignment = u64::from(memory.required_alignment());
        let cell_step = u64::from(memory.byte_len()).div_ceil(alignment) * alignment;
        let span = match self.map.packing() {
            GlyphPacking::GlyphMajor if self.map.is_empty() => 0,
            GlyphPacking::GlyphMajor => {
                u64::from(memory.data_offset())
                    + (self.map.len() as u64 - 1) * cell_step
                    + u64::from(memory.byte_len())
            }
            GlyphPacking::Atlas2D => u64::from(memory.data_end()),
        };
        let expected = u32::try_from(span).map_err(|_| GlyphStorageError::SizeOverflow)?;
        if data.len() != expected as usize {
            return Err(GlyphStorageError::DataSizeMismatch {
                expected,
                actual: data.len(),
            });
        }
        Ok(RawGlyphs {
            map: self.map,
            surface,
            memory,
            cell_step,
            data,
        })
    }
}

/// One exact glyph region backed by shared decoded surface storage.
///
/// Storage may be a padded cell or the complete atlas. Coordinates remain in
/// that storage's logical sample space, including sub-byte atlas origins.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GlyphRaster<'a> {
    storage: SurfaceView<'a>,
    region: Region,
}

impl<'a> GlyphRaster<'a> {
    pub const fn storage(self) -> SurfaceView<'a> {
        self.storage
    }

    pub const fn region(self) -> Region {
        self.region
    }

    /// Plans aligned cropped output using the shared image geometry contract.
    pub fn memory_plan(
        self,
        requirements: SurfaceRequirements,
    ) -> Result<RegionMemoryPlan, SurfacePlanError> {
        self.storage
            .surface()
            .region_plan(self.region, requirements)
    }

    /// Copies only this glyph, leaving errors atomic and successful padding zero.
    /// The returned samples borrow output independently of source storage.
    pub fn copy_into<'output>(
        self,
        output: &'output mut [u8],
        plan: RegionMemoryPlan,
    ) -> Result<SurfaceView<'output>, SurfaceCopyError> {
        if plan.region() != self.region {
            return Err(SurfaceCopyError::SurfaceMismatch);
        }
        self.storage.copy_region_samples_into(output, plan)?;
        Ok(SurfaceView::from_plan(plan.memory_plan(), output, None))
    }
}

/// Invalid scalar layout, physical allocation or complete group DATA span.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum GlyphStorageError {
    UnsupportedLayout(SampleLayout),
    Memory(PlaneMemoryError),
    SizeOverflow,
    DataSizeMismatch { expected: u32, actual: usize },
}

#[cfg(test)]
mod tests;
