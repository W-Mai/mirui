use super::SampleLayout;

macro_rules! open_color_value {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(u8);

        impl $name {
            pub const fn new(value: u8) -> Self {
                Self(value)
            }

            pub const fn raw(self) -> u8 {
                self.0
            }
        }

        impl From<u8> for $name {
            fn from(value: u8) -> Self {
                Self::new(value)
            }
        }

        impl From<$name> for u8 {
            fn from(value: $name) -> Self {
                value.raw()
            }
        }
    };
}

open_color_value!(/// Open color-primary identifier.
                  ColorPrimaries);

impl ColorPrimaries {
    pub const UNSPECIFIED: Self = Self(0);
    pub const BT709: Self = Self(1);
    pub const BT470_M: Self = Self(2);
    pub const BT470_BG: Self = Self(3);
    pub const BT601_525: Self = Self(4);
    pub const BT601_625: Self = Self(5);
    pub const BT2020: Self = Self(6);
    pub const DISPLAY_P3: Self = Self(7);
    pub const ADOBE_RGB: Self = Self(8);
}

open_color_value!(/// Open transfer-function identifier.
                  TransferFunction);

impl TransferFunction {
    pub const UNSPECIFIED: Self = Self(0);
    pub const LINEAR: Self = Self(1);
    pub const SRGB: Self = Self(2);
    pub const GAMMA_22: Self = Self(3);
    pub const BT709: Self = Self(4);
    pub const PQ: Self = Self(5);
    pub const HLG: Self = Self(6);
}

open_color_value!(/// Open color-matrix identifier.
                  ColorMatrix);

impl ColorMatrix {
    pub const UNSPECIFIED: Self = Self(0);
    pub const IDENTITY: Self = Self(1);
    pub const BT601: Self = Self(2);
    pub const BT709: Self = Self(3);
    pub const BT2020_NCL: Self = Self(4);
    pub const YCGCO: Self = Self(5);
}

open_color_value!(/// Open signal-range identifier.
                  ColorRange);

impl ColorRange {
    pub const UNSPECIFIED: Self = Self(0);
    pub const FULL: Self = Self(1);
    pub const LIMITED: Self = Self(2);
}

open_color_value!(/// Open chroma-siting identifier.
                  ChromaSiting);

impl ChromaSiting {
    pub const NONE: Self = Self(0);
    pub const LEFT: Self = Self(1);
    pub const CENTER: Self = Self(2);
    pub const TOP_LEFT: Self = Self(3);
    pub const TOP: Self = Self(4);
    pub const BOTTOM_LEFT: Self = Self(5);
    pub const BOTTOM: Self = Self(6);
}

/// Color interpretation independent of sample packing and byte coding.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ColorDescription {
    primaries: ColorPrimaries,
    transfer: TransferFunction,
    matrix: ColorMatrix,
    range: ColorRange,
    chroma_siting: ChromaSiting,
    profile_id: u16,
}

impl ColorDescription {
    /// No color channels. This is the canonical description for alpha-only
    /// layouts.
    pub const NONE: Self = Self::new(
        ColorPrimaries::UNSPECIFIED,
        TransferFunction::UNSPECIFIED,
        ColorMatrix::UNSPECIFIED,
        ColorRange::UNSPECIFIED,
        ChromaSiting::NONE,
        0,
    );

    /// Full-range sRGB samples with BT.709 primaries.
    pub const SRGB: Self = Self::new(
        ColorPrimaries::BT709,
        TransferFunction::SRGB,
        ColorMatrix::IDENTITY,
        ColorRange::FULL,
        ChromaSiting::NONE,
        0,
    );

    /// Limited-range BT.709 YUV with left-sited 4:2:0 chroma.
    pub const BT709_YUV_LIMITED: Self = Self::new(
        ColorPrimaries::BT709,
        TransferFunction::BT709,
        ColorMatrix::BT709,
        ColorRange::LIMITED,
        ChromaSiting::LEFT,
        0,
    );

    pub const fn new(
        primaries: ColorPrimaries,
        transfer: TransferFunction,
        matrix: ColorMatrix,
        range: ColorRange,
        chroma_siting: ChromaSiting,
        profile_id: u16,
    ) -> Self {
        Self {
            primaries,
            transfer,
            matrix,
            range,
            chroma_siting,
            profile_id,
        }
    }

    pub const fn primaries(self) -> ColorPrimaries {
        self.primaries
    }

    pub const fn transfer(self) -> TransferFunction {
        self.transfer
    }

    pub const fn matrix(self) -> ColorMatrix {
        self.matrix
    }

    pub const fn range(self) -> ColorRange {
        self.range
    }

    pub const fn chroma_siting(self) -> ChromaSiting {
        self.chroma_siting
    }

    pub const fn profile_id(self) -> u16 {
        self.profile_id
    }

    pub const fn with_profile_id(mut self, profile_id: u16) -> Self {
        self.profile_id = profile_id;
        self
    }

    /// Validates the color interpretation required by a known sample layout.
    pub fn validate_for(self, layout: SampleLayout) -> Result<(), ColorDescriptionError> {
        if !layout.is_known() {
            return Err(ColorDescriptionError::UnknownSampleLayout(layout));
        }
        if layout.is_alpha() {
            return if self == Self::NONE {
                Ok(())
            } else {
                Err(ColorDescriptionError::UnexpectedColorChannels)
            };
        }
        if self.primaries == ColorPrimaries::UNSPECIFIED {
            return Err(ColorDescriptionError::MissingPrimaries);
        }
        if self.transfer == TransferFunction::UNSPECIFIED {
            return Err(ColorDescriptionError::MissingTransfer);
        }
        if self.range == ColorRange::UNSPECIFIED {
            return Err(ColorDescriptionError::MissingRange);
        }

        if layout.is_yuv() {
            if self.matrix == ColorMatrix::UNSPECIFIED || self.matrix == ColorMatrix::IDENTITY {
                return Err(ColorDescriptionError::MissingYuvMatrix);
            }
            if self.chroma_siting == ChromaSiting::NONE {
                return Err(ColorDescriptionError::MissingChromaSiting);
            }
        } else {
            if self.matrix != ColorMatrix::IDENTITY {
                return Err(ColorDescriptionError::UnexpectedColorMatrix);
            }
            if self.range != ColorRange::FULL {
                return Err(ColorDescriptionError::UnexpectedColorRange);
            }
            if self.chroma_siting != ChromaSiting::NONE {
                return Err(ColorDescriptionError::UnexpectedChromaSiting);
            }
        }
        Ok(())
    }
}

/// Invalid pairing of a color description and decoded sample layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ColorDescriptionError {
    UnknownSampleLayout(SampleLayout),
    UnexpectedColorChannels,
    MissingPrimaries,
    MissingTransfer,
    MissingRange,
    MissingYuvMatrix,
    MissingChromaSiting,
    UnexpectedColorMatrix,
    UnexpectedColorRange,
    UnexpectedChromaSiting,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_rgb_and_indexed_layouts_use_explicit_srgb() {
        for layout in [
            SampleLayout::RGB565,
            SampleLayout::RGB888,
            SampleLayout::RGBA8888,
            SampleLayout::I4,
            SampleLayout::L8,
        ] {
            assert_eq!(ColorDescription::SRGB.validate_for(layout), Ok(()));
        }
        assert_eq!(
            ColorDescription::NONE.validate_for(SampleLayout::RGBA8888),
            Err(ColorDescriptionError::MissingPrimaries)
        );
    }

    #[test]
    fn alpha_layouts_have_no_color_interpretation() {
        assert_eq!(
            ColorDescription::NONE.validate_for(SampleLayout::A4),
            Ok(())
        );
        assert_eq!(
            ColorDescription::SRGB.validate_for(SampleLayout::A8),
            Err(ColorDescriptionError::UnexpectedColorChannels)
        );
    }

    #[test]
    fn yuv_never_infers_matrix_range_or_chroma_siting() {
        assert_eq!(
            ColorDescription::SRGB.validate_for(SampleLayout::NV12),
            Err(ColorDescriptionError::MissingYuvMatrix)
        );

        let missing_range = ColorDescription::new(
            ColorPrimaries::BT709,
            TransferFunction::BT709,
            ColorMatrix::BT709,
            ColorRange::UNSPECIFIED,
            ChromaSiting::LEFT,
            0,
        );
        assert_eq!(
            missing_range.validate_for(SampleLayout::I420),
            Err(ColorDescriptionError::MissingRange)
        );

        let missing_siting = ColorDescription::new(
            ColorPrimaries::BT709,
            TransferFunction::BT709,
            ColorMatrix::BT709,
            ColorRange::LIMITED,
            ChromaSiting::NONE,
            0,
        );
        assert_eq!(
            missing_siting.validate_for(SampleLayout::P010),
            Err(ColorDescriptionError::MissingChromaSiting)
        );
        assert_eq!(
            ColorDescription::BT709_YUV_LIMITED.validate_for(SampleLayout::NV21),
            Ok(())
        );
    }

    #[test]
    fn open_values_and_profile_id_round_trip() {
        let description = ColorDescription::new(
            ColorPrimaries::new(0x81),
            TransferFunction::new(0x82),
            ColorMatrix::new(0x83),
            ColorRange::new(0x84),
            ChromaSiting::new(0x85),
            0xbeef,
        );
        assert_eq!(description.primaries().raw(), 0x81);
        assert_eq!(description.transfer().raw(), 0x82);
        assert_eq!(description.matrix().raw(), 0x83);
        assert_eq!(description.range().raw(), 0x84);
        assert_eq!(description.chroma_siting().raw(), 0x85);
        assert_eq!(description.profile_id(), 0xbeef);
    }

    #[test]
    fn unknown_layout_is_preservable_but_not_typed() {
        let unknown = SampleLayout::new(0x8001);
        assert_eq!(
            ColorDescription::SRGB.validate_for(unknown),
            Err(ColorDescriptionError::UnknownSampleLayout(unknown))
        );
    }
}
