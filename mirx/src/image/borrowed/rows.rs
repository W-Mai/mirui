use core::iter::FusedIterator;

use super::SurfacePlane;

/// Failure while interpreting physical plane storage as logical samples.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PlaneAccessError {
    UnsupportedFlags(u16),
}

impl<'a> SurfacePlane<'a> {
    /// Borrows one logical row, excluding byte-stride padding.
    ///
    /// The index uses this plane's geometry, including chroma subsampling.
    /// Unused low bits in the final byte are retained for sub-byte layouts.
    /// Out-of-range indices return `None`; unknown storage flags are errors.
    pub fn row(self, index: u32) -> Result<Option<&'a [u8]>, PlaneAccessError> {
        Ok(self.rows()?.nth(index as usize))
    }

    /// Iterates over logical rows without allocation or address-alignment claims.
    ///
    /// Allocation-only rows and byte-stride padding are excluded. Zero-width
    /// planes yield empty slices; zero-height planes yield no rows.
    pub fn rows(self) -> Result<PlaneRows<'a>, PlaneAccessError> {
        let flags = self.memory().flags().bits();
        if flags != 0 {
            return Err(PlaneAccessError::UnsupportedFlags(flags));
        }
        Ok(PlaneRows {
            bytes: self.bytes(),
            stride: self.memory().stride() as usize,
            row_len: self
                .geometry()
                .minimum_stride()
                .expect("validated plane geometry") as usize,
            front: 0,
            back: self.geometry().height(),
        })
    }
}

/// Exact-size, double-ended iteration over a RAW plane's logical rows.
///
/// Indexed skips, counting, and selecting the last row take constant time.
#[derive(Clone, Debug)]
pub struct PlaneRows<'a> {
    bytes: &'a [u8],
    stride: usize,
    row_len: usize,
    front: u32,
    back: u32,
}

impl<'a> PlaneRows<'a> {
    fn row_at(&self, index: u32) -> &'a [u8] {
        let start = index as usize * self.stride;
        &self.bytes[start..start + self.row_len]
    }
}

impl<'a> Iterator for PlaneRows<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let index = self.front;
        self.front += 1;
        Some(self.row_at(index))
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        if n >= self.len() {
            self.front = self.back;
            return None;
        }
        self.front += n as u32;
        self.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }

    fn count(self) -> usize {
        self.len()
    }

    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }
}

impl DoubleEndedIterator for PlaneRows<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        Some(self.row_at(self.back))
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        if n >= self.len() {
            self.front = self.back;
            return None;
        }
        self.back -= n as u32;
        self.next_back()
    }
}

impl ExactSizeIterator for PlaneRows<'_> {
    fn len(&self) -> usize {
        (self.back - self.front) as usize
    }
}

impl FusedIterator for PlaneRows<'_> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ColorFormat;
    use crate::image::{
        ColorDescription, PlaneMemoryFlags, PlaneMemoryLayout, RawImageAsset, SampleLayout,
        SurfaceDescriptor, SurfaceRequirements,
    };
    use alloc::vec::Vec;

    #[test]
    fn every_sample_layout_borrows_logical_rows_without_storage_padding() {
        let formats = [
            ColorFormat::I1,
            ColorFormat::I2,
            ColorFormat::I4,
            ColorFormat::I8,
            ColorFormat::A1,
            ColorFormat::A2,
            ColorFormat::A4,
            ColorFormat::A8,
            ColorFormat::L8,
            ColorFormat::RGB565,
            ColorFormat::RGB565Swapped,
            ColorFormat::RGB565A8,
            ColorFormat::RGB888,
            ColorFormat::XRGB8888,
            ColorFormat::RGBA8888,
            ColorFormat::BGRA8888,
        ];
        let yuv = [
            SampleLayout::I420,
            SampleLayout::YV12,
            SampleLayout::NV12,
            SampleLayout::NV21,
            SampleLayout::P010,
            SampleLayout::P016,
        ];
        for layout in formats
            .into_iter()
            .map(SampleLayout::from_color_format)
            .chain(yuv)
        {
            let color = if layout.is_alpha() {
                ColorDescription::NONE
            } else if layout.is_yuv() {
                ColorDescription::BT709_YUV_LIMITED
            } else {
                ColorDescription::SRGB
            };
            let surface = SurfaceDescriptor::new(5, 5, layout, color).unwrap();
            let plan = surface
                .memory_plan(
                    SurfaceRequirements::new()
                        .with_width_multiple(8)
                        .with_height_multiple(8)
                        .with_stride_multiple(8),
                )
                .unwrap();
            let memory: Vec<_> = plan.planes().collect();
            let buffers: Vec<Vec<u8>> = memory
                .iter()
                .map(|memory| {
                    (0..memory.byte_len())
                        .map(|index| (index as u8).wrapping_mul(17).wrapping_add(0xab))
                        .collect()
                })
                .collect();
            let planes: Vec<_> = buffers.iter().map(Vec::as_slice).collect();
            let palette = [0; 1024];
            let mut asset = RawImageAsset::new(surface, &planes).with_memory_layouts(&memory);
            if let Some(count) = layout.color_table_entries() {
                asset = asset.with_color_table(&palette[..count as usize * 4]);
            }
            let view = asset.view().unwrap();
            for plane in view.planes() {
                let height = plane.geometry().height() as usize;
                let row_len = plane.geometry().minimum_stride().unwrap() as usize;
                assert_eq!(plane.rows().unwrap().len(), height);
                for (index, row) in plane.rows().unwrap().enumerate() {
                    let start = index * plane.memory().stride() as usize;
                    assert_eq!(row.len(), row_len);
                    assert_eq!(row, &plane.bytes()[start..start + row_len]);
                    assert_eq!(row.as_ptr(), plane.bytes()[start..].as_ptr());
                    assert_eq!(plane.row(index as u32).unwrap(), Some(row));
                }
                assert_eq!(plane.row(height as u32).unwrap(), None);
                assert_eq!(plane.row(u32::MAX).unwrap(), None);
                let mut rows = plane.rows().unwrap();
                assert_eq!(rows.next(), plane.row(0).unwrap());
                assert_eq!(rows.next_back(), plane.row((height - 1) as u32).unwrap());
                assert_eq!(rows.next(), plane.row(1).unwrap());
                assert_eq!(plane.rows().unwrap().nth(1), plane.row(1).unwrap());
                assert_eq!(rows.len(), height - 3);
                assert_eq!(
                    plane.rows().unwrap().nth_back(1),
                    plane.row((height - 2) as u32).unwrap()
                );
                assert_eq!(
                    plane.rows().unwrap().last(),
                    plane.row((height - 1) as u32).unwrap()
                );
                assert_eq!(plane.rows().unwrap().count(), height);
                assert_eq!(plane.rows().unwrap().rev().count(), height);
            }
        }
    }

    #[test]
    fn empty_and_extreme_geometry_keeps_iteration_bounded() {
        for (width, height) in [(0, 0), (0, u32::MAX), (u32::MAX, 0)] {
            let surface =
                SurfaceDescriptor::new(width, height, SampleLayout::A8, ColorDescription::NONE)
                    .unwrap();
            let plane = RawImageAsset::new(surface, &[&[]])
                .view()
                .unwrap()
                .plane(0)
                .unwrap();
            let rows = plane.rows().unwrap();
            assert_eq!(rows.len(), height as usize);
            assert_eq!(rows.clone().count(), height as usize);
            if height != 0 {
                assert_eq!(plane.row(height - 1).unwrap(), Some(&[][..]));
                assert_eq!(rows.clone().last(), Some(&[][..]));
                assert_eq!(rows.clone().nth((height - 1) as usize), Some(&[][..]));
                assert_eq!(rows.clone().nth_back((height - 1) as usize), Some(&[][..]));
            }
            for reverse in [false, true] {
                let mut rows = rows.clone();
                assert_eq!(
                    if reverse {
                        rows.nth_back(usize::MAX)
                    } else {
                        rows.nth(usize::MAX)
                    },
                    None
                );
                assert_eq!(rows.len(), 0);
                assert_eq!(rows.size_hint(), (0, Some(0)));
                assert_eq!(rows.next(), None);
                assert_eq!(rows.next_back(), None);
            }
        }
    }

    #[test]
    fn unknown_storage_flags_cannot_be_interpreted_as_linear_rows() {
        let surface =
            SurfaceDescriptor::new(1, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let memory = [PlaneMemoryLayout::builder(surface.plane(0).unwrap())
            .with_flags(PlaneMemoryFlags::from_bits_retain(0x80))
            .build()
            .unwrap()];
        let plane = RawImageAsset::new(surface, &[&[0]])
            .with_memory_layouts(&memory)
            .view()
            .unwrap()
            .plane(0)
            .unwrap();
        assert_eq!(
            plane.rows().unwrap_err(),
            PlaneAccessError::UnsupportedFlags(0x80)
        );
        assert_eq!(plane.row(0), Err(PlaneAccessError::UnsupportedFlags(0x80)));
        assert_eq!(
            plane.row(u32::MAX),
            Err(PlaneAccessError::UnsupportedFlags(0x80))
        );
    }
}
