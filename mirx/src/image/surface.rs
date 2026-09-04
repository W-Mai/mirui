use super::{
    ChromaSiting, ColorDescription, ColorDescriptionError, ColorMatrix, ColorPrimaries, ColorRange,
    PlaneGeometries, PlaneGeometry, SampleLayout, TransferFunction,
};
use crate::wire::{read_u16_le, read_u32_le, write_u16_le, write_u32_le};

pub const SURFACE_RECORD_LEN: usize = 32;

/// Open flags attached to one decoded surface descriptor.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct SurfaceFlags(u16);

impl SurfaceFlags {
    pub const NONE: Self = Self(0);

    pub const fn from_bits_retain(bits: u16) -> Self {
        Self(bits)
    }

    pub const fn bits(self) -> u16 {
        self.0
    }
}

/// Logical decoded IMAGE surface.
///
/// Plane count, roles, dimensions, sample depth, and subsampling are derived
/// from `sample_layout`; they are not duplicated in this descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceDescriptor {
    width: u32,
    height: u32,
    sample_layout: SampleLayout,
    color: ColorDescription,
    flags: SurfaceFlags,
    pixel_aspect_num: u16,
    pixel_aspect_den: u16,
}

impl SurfaceDescriptor {
    pub fn new(
        width: u32,
        height: u32,
        sample_layout: SampleLayout,
        color: ColorDescription,
    ) -> Result<Self, SurfaceError> {
        color
            .validate_for(sample_layout)
            .map_err(SurfaceError::Color)?;
        Ok(Self {
            width,
            height,
            sample_layout,
            color,
            flags: SurfaceFlags::NONE,
            pixel_aspect_num: 1,
            pixel_aspect_den: 1,
        })
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }

    pub(super) const fn with_extent(mut self, width: u32, height: u32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub const fn sample_layout(self) -> SampleLayout {
        self.sample_layout
    }

    pub const fn color(self) -> ColorDescription {
        self.color
    }

    pub const fn flags(self) -> SurfaceFlags {
        self.flags
    }

    pub const fn pixel_aspect(self) -> (u16, u16) {
        (self.pixel_aspect_num, self.pixel_aspect_den)
    }

    pub const fn with_flags(mut self, flags: SurfaceFlags) -> Self {
        self.flags = flags;
        self
    }

    pub fn with_pixel_aspect(
        mut self,
        numerator: u16,
        denominator: u16,
    ) -> Result<Self, SurfaceError> {
        if numerator == 0 || denominator == 0 {
            return Err(SurfaceError::InvalidPixelAspect {
                numerator,
                denominator,
            });
        }
        self.pixel_aspect_num = numerator;
        self.pixel_aspect_den = denominator;
        Ok(self)
    }

    pub const fn plane_count(self) -> u8 {
        match self.sample_layout.plane_count() {
            Some(count) => count,
            None => 0,
        }
    }

    pub const fn plane(self, index: u8) -> Option<PlaneGeometry> {
        self.sample_layout
            .plane_geometry(self.width, self.height, index)
    }

    pub const fn planes(self) -> PlaneGeometries {
        self.sample_layout.planes(self.width, self.height)
    }

    /// Decodes one fixed-width SURFACE section record.
    ///
    /// An all-zero color tuple expands to the canonical layout default for
    /// alpha-only and RGB-like surfaces. YUV surfaces have no implicit color
    /// default and reject the same tuple.
    pub fn from_record(bytes: &[u8]) -> Result<Self, SurfaceRecordError> {
        if bytes.len() < SURFACE_RECORD_LEN {
            return Err(SurfaceRecordError::Truncated {
                needed: SURFACE_RECORD_LEN,
                available: bytes.len(),
            });
        }
        for offset in [17, 24, 25, 26, 27, 28, 29, 30, 31] {
            if bytes[offset] != 0 {
                return Err(SurfaceRecordError::ReservedNonZero { offset });
            }
        }

        let width = read_u32_le(bytes, 0).expect("complete surface record");
        let height = read_u32_le(bytes, 4).expect("complete surface record");
        let sample_layout =
            SampleLayout::new(read_u16_le(bytes, 8).expect("complete surface record"));
        let flags = SurfaceFlags::from_bits_retain(
            read_u16_le(bytes, 10).expect("complete surface record"),
        );
        let profile_id = read_u16_le(bytes, 18).expect("complete surface record");
        let stored_color = ColorDescription::new(
            ColorPrimaries::new(bytes[13]),
            TransferFunction::new(bytes[14]),
            ColorMatrix::new(bytes[15]),
            ColorRange::new(bytes[12]),
            ChromaSiting::new(bytes[16]),
            profile_id,
        );
        let color = if color_tuple_is_zero(stored_color) {
            default_color(sample_layout)
        } else {
            stored_color
        };
        let mut surface = Self::new(width, height, sample_layout, color)
            .map_err(SurfaceRecordError::InvalidSurface)?
            .with_flags(flags);

        let numerator = read_u16_le(bytes, 20).expect("complete surface record");
        let denominator = read_u16_le(bytes, 22).expect("complete surface record");
        if numerator != 0 || denominator != 0 {
            surface = surface
                .with_pixel_aspect(numerator, denominator)
                .map_err(SurfaceRecordError::InvalidSurface)?;
        }
        Ok(surface)
    }

    /// Encodes the canonical 32-byte SURFACE section record into `out`.
    ///
    /// Unused bytes in `out` are preserved. Common alpha and sRGB color
    /// descriptions plus square pixels use their zero-valued wire defaults.
    pub fn encode_record_into(&self, out: &mut [u8]) -> Result<usize, SurfaceRecordError> {
        if out.len() < SURFACE_RECORD_LEN {
            return Err(SurfaceRecordError::BufferTooSmall {
                needed: SURFACE_RECORD_LEN,
                available: out.len(),
            });
        }

        let mut record = [0; SURFACE_RECORD_LEN];
        write_u32_le(&mut record, 0, self.width);
        write_u32_le(&mut record, 4, self.height);
        write_u16_le(&mut record, 8, self.sample_layout.raw());
        write_u16_le(&mut record, 10, self.flags.bits());

        if self.color != default_color(self.sample_layout) {
            record[12] = self.color.range().raw();
            record[13] = self.color.primaries().raw();
            record[14] = self.color.transfer().raw();
            record[15] = self.color.matrix().raw();
            record[16] = self.color.chroma_siting().raw();
            write_u16_le(&mut record, 18, self.color.profile_id());
        }
        if self.pixel_aspect() != (1, 1) {
            write_u16_le(&mut record, 20, self.pixel_aspect_num);
            write_u16_le(&mut record, 22, self.pixel_aspect_den);
        }
        out[..SURFACE_RECORD_LEN].copy_from_slice(&record);
        Ok(SURFACE_RECORD_LEN)
    }
}

fn color_tuple_is_zero(color: ColorDescription) -> bool {
    color.primaries().raw() == 0
        && color.transfer().raw() == 0
        && color.matrix().raw() == 0
        && color.range().raw() == 0
        && color.chroma_siting().raw() == 0
        && color.profile_id() == 0
}

fn default_color(layout: SampleLayout) -> ColorDescription {
    if layout.is_alpha() || layout.is_yuv() {
        ColorDescription::NONE
    } else {
        ColorDescription::SRGB
    }
}

/// Invalid logical surface description.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SurfaceError {
    Color(ColorDescriptionError),
    InvalidPixelAspect { numerator: u16, denominator: u16 },
}

/// Failure while decoding or encoding a SURFACE section record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SurfaceRecordError {
    Truncated { needed: usize, available: usize },
    BufferTooSmall { needed: usize, available: usize },
    ReservedNonZero { offset: usize },
    InvalidSurface(SurfaceError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(surface: SurfaceDescriptor) -> [u8; SURFACE_RECORD_LEN] {
        let mut bytes = [0xa5; SURFACE_RECORD_LEN];
        assert_eq!(
            surface.encode_record_into(&mut bytes),
            Ok(SURFACE_RECORD_LEN)
        );
        bytes
    }

    #[test]
    fn descriptor_derives_planes_without_storing_a_count() {
        let surface = SurfaceDescriptor::new(
            319,
            181,
            SampleLayout::NV12,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap();
        assert_eq!(surface.plane_count(), 2);
        assert_eq!(surface.planes().len(), 2);
        assert_eq!(
            (
                surface.plane(0).unwrap().width(),
                surface.plane(0).unwrap().height()
            ),
            (319, 181)
        );
        assert_eq!(
            (
                surface.plane(1).unwrap().width(),
                surface.plane(1).unwrap().height()
            ),
            (160, 91)
        );
    }

    #[test]
    fn common_srgb_and_square_pixels_use_zero_wire_defaults() {
        let surface =
            SurfaceDescriptor::new(48, 32, SampleLayout::RGBA8888, ColorDescription::SRGB).unwrap();
        let bytes = encode(surface);
        assert_eq!(&bytes[12..24], &[0; 12]);
        assert_eq!(SurfaceDescriptor::from_record(&bytes), Ok(surface));
    }

    #[test]
    fn explicit_yuv_color_and_pixel_aspect_round_trip() {
        let surface = SurfaceDescriptor::new(
            720,
            480,
            SampleLayout::I420,
            ColorDescription::BT709_YUV_LIMITED.with_profile_id(7),
        )
        .unwrap()
        .with_flags(SurfaceFlags::from_bits_retain(0xa501))
        .with_pixel_aspect(8, 9)
        .unwrap();
        let bytes = encode(surface);
        assert_eq!(bytes[12], ColorRange::LIMITED.raw());
        assert_eq!(bytes[15], ColorMatrix::BT709.raw());
        assert_eq!(bytes[16], ChromaSiting::LEFT.raw());
        assert_eq!(SurfaceDescriptor::from_record(&bytes), Ok(surface));
    }

    #[test]
    fn yuv_record_cannot_omit_color_interpretation() {
        let mut bytes = [0; SURFACE_RECORD_LEN];
        write_u32_le(&mut bytes, 0, 16);
        write_u32_le(&mut bytes, 4, 16);
        write_u16_le(&mut bytes, 8, SampleLayout::NV12.raw());
        assert_eq!(
            SurfaceDescriptor::from_record(&bytes),
            Err(SurfaceRecordError::InvalidSurface(SurfaceError::Color(
                ColorDescriptionError::MissingPrimaries
            )))
        );
    }

    #[test]
    fn unknown_layout_and_bad_aspect_are_rejected_from_typed_surfaces() {
        assert_eq!(
            SurfaceDescriptor::new(1, 1, SampleLayout::new(0x8001), ColorDescription::SRGB,),
            Err(SurfaceError::Color(
                ColorDescriptionError::UnknownSampleLayout(SampleLayout::new(0x8001))
            ))
        );

        let surface =
            SurfaceDescriptor::new(1, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        assert_eq!(
            surface.with_pixel_aspect(1, 0),
            Err(SurfaceError::InvalidPixelAspect {
                numerator: 1,
                denominator: 0,
            })
        );
    }

    #[test]
    fn record_boundaries_reserved_bytes_and_output_atomicity_are_checked() {
        let surface =
            SurfaceDescriptor::new(2, 2, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let bytes = encode(surface);
        for available in 0..SURFACE_RECORD_LEN {
            assert_eq!(
                SurfaceDescriptor::from_record(&bytes[..available]),
                Err(SurfaceRecordError::Truncated {
                    needed: SURFACE_RECORD_LEN,
                    available,
                })
            );
        }

        for offset in [17, 24, 25, 26, 27, 28, 29, 30, 31] {
            let mut reserved = bytes;
            reserved[offset] = 1;
            assert_eq!(
                SurfaceDescriptor::from_record(&reserved),
                Err(SurfaceRecordError::ReservedNonZero { offset })
            );
        }

        let mut short = [0xa5; SURFACE_RECORD_LEN - 1];
        assert_eq!(
            surface.encode_record_into(&mut short),
            Err(SurfaceRecordError::BufferTooSmall {
                needed: SURFACE_RECORD_LEN,
                available: SURFACE_RECORD_LEN - 1,
            })
        );
        assert_eq!(short, [0xa5; SURFACE_RECORD_LEN - 1]);
    }
}
