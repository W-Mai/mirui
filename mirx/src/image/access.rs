/// Sample access supported by a validated image or frame source.
///
/// Capabilities describe implemented operations, not merely representable
/// execution intents. An unset capability must be treated as unsupported.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct AccessCapabilities(u8);

impl AccessCapabilities {
    const WHOLE: u8 = 1 << 0;
    const ROWS: u8 = 1 << 1;
    const UNITS: u8 = 1 << 2;
    const REGION: u8 = 1 << 3;
    const PROGRESSIVE: u8 = 1 << 4;
    const RANDOM_FRAME: u8 = 1 << 5;
    const DIRECT_BORROW: u8 = 1 << 6;
    const DIRECT_UPLOAD: u8 = 1 << 7;

    pub(crate) const fn raw_surface(has_linear_rows: bool) -> Self {
        let mut bits = Self::DIRECT_BORROW;
        if has_linear_rows {
            bits |= Self::WHOLE | Self::ROWS | Self::REGION;
        }
        Self(bits)
    }

    pub(crate) const fn encoded_image() -> Self {
        Self(Self::WHOLE | Self::REGION)
    }

    pub(crate) const fn encoded_group(has_independent_units: bool) -> Self {
        let mut bits = Self::WHOLE | Self::REGION;
        if has_independent_units {
            bits |= Self::UNITS;
        }
        Self(bits)
    }

    pub(crate) const fn frames() -> Self {
        Self(Self::WHOLE | Self::RANDOM_FRAME)
    }

    pub const fn supports_whole(self) -> bool {
        self.0 & Self::WHOLE != 0
    }

    pub const fn supports_rows(self) -> bool {
        self.0 & Self::ROWS != 0
    }

    pub const fn supports_units(self) -> bool {
        self.0 & Self::UNITS != 0
    }

    pub const fn supports_region(self) -> bool {
        self.0 & Self::REGION != 0
    }

    pub const fn supports_progressive(self) -> bool {
        self.0 & Self::PROGRESSIVE != 0
    }

    pub const fn supports_random_frame(self) -> bool {
        self.0 & Self::RANDOM_FRAME != 0
    }

    pub const fn supports_direct_borrow(self) -> bool {
        self.0 & Self::DIRECT_BORROW != 0
    }

    pub const fn supports_direct_upload(self) -> bool {
        self.0 & Self::DIRECT_UPLOAD != 0
    }
}

#[cfg(test)]
mod tests {
    use super::AccessCapabilities;

    #[test]
    fn constructors_do_not_advertise_unimplemented_execution() {
        let raw = AccessCapabilities::raw_surface(true);
        assert!(raw.supports_whole());
        assert!(raw.supports_rows());
        assert!(raw.supports_region());
        assert!(raw.supports_direct_borrow());
        assert!(!raw.supports_units());
        assert!(!raw.supports_progressive());
        assert!(!raw.supports_random_frame());
        assert!(!raw.supports_direct_upload());

        let opaque = AccessCapabilities::raw_surface(false);
        assert!(opaque.supports_direct_borrow());
        assert!(!opaque.supports_whole());
        assert!(!opaque.supports_rows());
        assert!(!opaque.supports_region());

        let encoded = AccessCapabilities::encoded_group(true);
        assert!(encoded.supports_whole());
        assert!(encoded.supports_units());
        assert!(encoded.supports_region());
        assert!(!encoded.supports_rows());
        assert!(!encoded.supports_direct_borrow());
        assert!(!encoded.supports_direct_upload());

        let frames = AccessCapabilities::frames();
        assert!(frames.supports_whole());
        assert!(frames.supports_random_frame());
        assert!(!frames.supports_region());
    }
}
