use core::iter::FusedIterator;

use crate::ColorFormat;
use crate::format::minimum_stride_for_bits;

/// Open identifier for the decoded sample arrangement of an IMAGE surface.
///
/// The layout describes component packing and plane subsampling. Color
/// primaries, transfer, matrix, range, and chroma siting are carried by
/// [`super::ColorDescription`] instead.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SampleLayout(u16);

impl SampleLayout {
    /// No fixed sample layout in a container display hint.
    /// This sentinel cannot describe a decoded surface.
    pub const NONE: Self = Self(0x00ff);

    pub const I1: Self = Self(0x0010);
    pub const I2: Self = Self(0x0011);
    pub const I4: Self = Self(0x0012);
    pub const I8: Self = Self(0x0013);

    pub const A1: Self = Self(0x0020);
    pub const A2: Self = Self(0x0021);
    pub const A4: Self = Self(0x0022);
    pub const A8: Self = Self(0x0023);

    pub const L8: Self = Self(0x0030);

    pub const RGB565: Self = Self(0x0040);
    pub const RGB565_SWAPPED: Self = Self(0x0041);
    pub const RGB565_A8: Self = Self(0x0042);
    pub const RGB888: Self = Self(0x0050);
    pub const XRGB8888: Self = Self(0x0060);
    pub const RGBA8888: Self = Self(0x0061);
    pub const BGRA8888: Self = Self(0x0062);

    /// 8-bit 4:2:0 planes ordered Y, U, V.
    pub const I420: Self = Self(0x0100);
    /// 8-bit 4:2:0 planes ordered Y, V, U.
    pub const YV12: Self = Self(0x0101);
    /// 8-bit 4:2:0 planes ordered Y, interleaved UV.
    pub const NV12: Self = Self(0x0110);
    /// 8-bit 4:2:0 planes ordered Y, interleaved VU.
    pub const NV21: Self = Self(0x0111);
    /// 10 meaningful bits in 16-bit Y samples and 32-bit interleaved UV pairs.
    pub const P010: Self = Self(0x0112);
    /// 16-bit Y samples and 32-bit interleaved UV pairs.
    pub const P016: Self = Self(0x0113);

    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }

    pub const fn from_color_format(format: ColorFormat) -> Self {
        Self(format.to_u8() as u16)
    }

    /// Returns the scalar color format when this layout is representable by
    /// the original MIRX packed/indexed vocabulary.
    pub const fn color_format(self) -> Option<ColorFormat> {
        if self.0 > u8::MAX as u16 {
            return None;
        }
        ColorFormat::from_u8(self.0 as u8)
    }

    pub const fn is_known(self) -> bool {
        self.color_format().is_some()
            || matches!(
                self,
                Self::I420 | Self::YV12 | Self::NV12 | Self::NV21 | Self::P010 | Self::P016
            )
    }

    pub const fn is_alpha(self) -> bool {
        matches!(self, Self::A1 | Self::A2 | Self::A4 | Self::A8)
    }

    pub const fn is_indexed(self) -> bool {
        matches!(self, Self::I1 | Self::I2 | Self::I4 | Self::I8)
    }

    pub const fn is_yuv(self) -> bool {
        matches!(
            self,
            Self::I420 | Self::YV12 | Self::NV12 | Self::NV21 | Self::P010 | Self::P016
        )
    }

    /// Number of entries required in the separate COLOR_TABLE section.
    pub const fn color_table_entries(self) -> Option<u32> {
        match self.color_format() {
            Some(format) => format.palette_entries(),
            None => None,
        }
    }

    /// Number of decoded sample planes, excluding an indexed color table.
    pub const fn plane_count(self) -> Option<u8> {
        if self.color_format().is_some() {
            return Some(if self.0 == Self::RGB565_A8.0 { 2 } else { 1 });
        }
        match self {
            Self::I420 | Self::YV12 => Some(3),
            Self::NV12 | Self::NV21 | Self::P010 | Self::P016 => Some(2),
            _ => None,
        }
    }

    /// Derives one logical plane from the surface dimensions.
    ///
    /// Unknown layouts and plane indices outside the known layout return
    /// `None`. Odd 4:2:0 dimensions round chroma planes upward.
    pub const fn plane_geometry(
        self,
        surface_width: u32,
        surface_height: u32,
        plane_index: u8,
    ) -> Option<PlaneGeometry> {
        if let Some(format) = self.color_format() {
            if self.0 == Self::RGB565_A8.0 && plane_index == 1 {
                return Some(PlaneGeometry::new(
                    PlaneRole::ALPHA,
                    surface_width,
                    surface_height,
                    8,
                    0,
                    0,
                ));
            }
            if plane_index != 0 {
                return None;
            }
            let role = if self.is_indexed() {
                PlaneRole::INDEX
            } else if self.is_alpha() {
                PlaneRole::ALPHA
            } else if self.0 == Self::L8.0 {
                PlaneRole::LUMA
            } else {
                PlaneRole::PACKED_COLOR
            };
            return Some(PlaneGeometry::new(
                role,
                surface_width,
                surface_height,
                format.bits_per_pixel(),
                0,
                0,
            ));
        }

        let chroma_width = surface_width.div_ceil(2);
        let chroma_height = surface_height.div_ceil(2);
        match (self, plane_index) {
            (Self::I420 | Self::YV12 | Self::NV12 | Self::NV21, 0) => Some(PlaneGeometry::new(
                PlaneRole::LUMA,
                surface_width,
                surface_height,
                8,
                0,
                0,
            )),
            (Self::P010, 0) => Some(PlaneGeometry::new(
                PlaneRole::LUMA,
                surface_width,
                surface_height,
                16,
                0,
                0,
            )),
            (Self::P016, 0) => Some(PlaneGeometry::new(
                PlaneRole::LUMA,
                surface_width,
                surface_height,
                16,
                0,
                0,
            )),
            (Self::I420, 1) | (Self::YV12, 2) => Some(PlaneGeometry::new(
                PlaneRole::CHROMA_U,
                chroma_width,
                chroma_height,
                8,
                1,
                1,
            )),
            (Self::I420, 2) | (Self::YV12, 1) => Some(PlaneGeometry::new(
                PlaneRole::CHROMA_V,
                chroma_width,
                chroma_height,
                8,
                1,
                1,
            )),
            (Self::NV12, 1) => Some(PlaneGeometry::new(
                PlaneRole::CHROMA_UV,
                chroma_width,
                chroma_height,
                16,
                1,
                1,
            )),
            (Self::NV21, 1) => Some(PlaneGeometry::new(
                PlaneRole::CHROMA_VU,
                chroma_width,
                chroma_height,
                16,
                1,
                1,
            )),
            (Self::P010 | Self::P016, 1) => Some(PlaneGeometry::new(
                PlaneRole::CHROMA_UV,
                chroma_width,
                chroma_height,
                32,
                1,
                1,
            )),
            _ => None,
        }
    }

    pub const fn planes(self, width: u32, height: u32) -> PlaneGeometries {
        PlaneGeometries {
            layout: self,
            width,
            height,
            front: 0,
            back: match self.plane_count() {
                Some(count) => count,
                None => 0,
            },
        }
    }
}

impl From<ColorFormat> for SampleLayout {
    fn from(format: ColorFormat) -> Self {
        Self::from_color_format(format)
    }
}

impl From<u16> for SampleLayout {
    fn from(value: u16) -> Self {
        Self::new(value)
    }
}

impl From<SampleLayout> for u16 {
    fn from(value: SampleLayout) -> Self {
        value.raw()
    }
}

/// Open semantic role for one decoded plane.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlaneRole(u8);

impl PlaneRole {
    pub const PACKED_COLOR: Self = Self(1);
    pub const INDEX: Self = Self(2);
    pub const ALPHA: Self = Self(3);
    pub const LUMA: Self = Self(4);
    pub const CHROMA_U: Self = Self(5);
    pub const CHROMA_V: Self = Self(6);
    pub const CHROMA_UV: Self = Self(7);
    pub const CHROMA_VU: Self = Self(8);

    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u8 {
        self.0
    }
}

/// Logical geometry derived for one decoded plane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaneGeometry {
    role: PlaneRole,
    width: u32,
    height: u32,
    bits_per_element: u8,
    subsample_x_log2: u8,
    subsample_y_log2: u8,
}

impl PlaneGeometry {
    const fn new(
        role: PlaneRole,
        width: u32,
        height: u32,
        bits_per_element: u8,
        subsample_x_log2: u8,
        subsample_y_log2: u8,
    ) -> Self {
        Self {
            role,
            width,
            height,
            bits_per_element,
            subsample_x_log2,
            subsample_y_log2,
        }
    }

    pub const fn role(self) -> PlaneRole {
        self.role
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }

    /// Stored bits in one plane element.
    ///
    /// An interleaved UV element contains both chroma components, so NV12 uses
    /// 16 bits and P010/P016 use 32 bits here.
    pub const fn bits_per_element(self) -> u8 {
        self.bits_per_element
    }

    pub const fn subsample_x_log2(self) -> u8 {
        self.subsample_x_log2
    }

    pub const fn subsample_y_log2(self) -> u8 {
        self.subsample_y_log2
    }

    /// Smallest tightly packed stride for this logical plane.
    pub const fn minimum_stride(self) -> Option<u32> {
        minimum_stride_for_bits(self.width, self.bits_per_element)
    }
}

/// Allocation-free iterator over the planes derived from one sample layout.
#[derive(Clone, Debug)]
pub struct PlaneGeometries {
    layout: SampleLayout,
    width: u32,
    height: u32,
    front: u8,
    back: u8,
}

impl Iterator for PlaneGeometries {
    type Item = PlaneGeometry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let index = self.front;
        self.front += 1;
        self.layout.plane_geometry(self.width, self.height, index)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = usize::from(self.back - self.front);
        (remaining, Some(remaining))
    }
}

impl DoubleEndedIterator for PlaneGeometries {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        self.layout
            .plane_geometry(self.width, self.height, self.back)
    }
}

impl ExactSizeIterator for PlaneGeometries {}
impl FusedIterator for PlaneGeometries {}

#[cfg(test)]
mod tests {
    use super::*;

    const COLOR_FORMATS: [ColorFormat; 16] = [
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

    #[test]
    fn scalar_formats_reuse_the_existing_bpp_and_stride_seam() {
        for format in COLOR_FORMATS {
            let layout = SampleLayout::from(format);
            assert_eq!(layout.color_format(), Some(format));
            let primary = layout.plane_geometry(13, 7, 0).unwrap();
            assert_eq!(primary.bits_per_element(), format.bits_per_pixel());
            assert_eq!(primary.minimum_stride(), format.minimum_stride(13));
        }
    }

    #[test]
    fn indexed_layouts_keep_the_color_table_out_of_the_plane_array() {
        assert_eq!(SampleLayout::I1.color_table_entries(), Some(2));
        assert_eq!(SampleLayout::I4.color_table_entries(), Some(16));
        assert_eq!(SampleLayout::I8.color_table_entries(), Some(256));
        assert_eq!(SampleLayout::I8.plane_count(), Some(1));
        assert_eq!(
            SampleLayout::I8.plane_geometry(19, 11, 0).unwrap().role(),
            PlaneRole::INDEX
        );
        assert_eq!(SampleLayout::I8.plane_geometry(19, 11, 1), None);
    }

    #[test]
    fn odd_yuv420_dimensions_round_chroma_up() {
        let i420: alloc::vec::Vec<_> = SampleLayout::I420.planes(5, 3).collect();
        assert_eq!(i420.len(), 3);
        assert_eq!((i420[0].width(), i420[0].height()), (5, 3));
        assert_eq!((i420[1].width(), i420[1].height()), (3, 2));
        assert_eq!(i420[1].role(), PlaneRole::CHROMA_U);
        assert_eq!(i420[2].role(), PlaneRole::CHROMA_V);
        assert_eq!(i420[1].minimum_stride(), Some(3));

        let yv12: alloc::vec::Vec<_> = SampleLayout::YV12.planes(5, 3).collect();
        assert_eq!(yv12[1].role(), PlaneRole::CHROMA_V);
        assert_eq!(yv12[2].role(), PlaneRole::CHROMA_U);
    }

    #[test]
    fn interleaved_chroma_stride_counts_both_components() {
        let nv12 = SampleLayout::NV12.plane_geometry(5, 3, 1).unwrap();
        assert_eq!((nv12.width(), nv12.height()), (3, 2));
        assert_eq!(nv12.bits_per_element(), 16);
        assert_eq!(nv12.minimum_stride(), Some(6));

        let p010 = SampleLayout::P010.plane_geometry(5, 3, 1).unwrap();
        assert_eq!(p010.bits_per_element(), 32);
        assert_eq!(p010.minimum_stride(), Some(12));
    }

    #[test]
    fn rgb565_a8_derives_two_full_resolution_planes() {
        let planes: alloc::vec::Vec<_> = SampleLayout::RGB565_A8.planes(7, 5).collect();
        assert_eq!(planes.len(), 2);
        assert_eq!(planes[0].role(), PlaneRole::PACKED_COLOR);
        assert_eq!(planes[0].minimum_stride(), Some(14));
        assert_eq!(planes[1].role(), PlaneRole::ALPHA);
        assert_eq!(planes[1].minimum_stride(), Some(7));
    }

    #[test]
    fn unknown_layout_has_no_invented_geometry() {
        let unknown = SampleLayout::new(0x8a51);
        assert!(!unknown.is_known());
        assert_eq!(unknown.plane_count(), None);
        assert_eq!(unknown.plane_geometry(20, 10, 0), None);
        assert!(unknown.planes(20, 10).next().is_none());
    }
}
