use super::texture::{AlphaMode, ColorFormat, TexBuf, Texture};

/// Invalid or insufficient storage for a render plane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaneError {
    InvalidAlignment,
    InvalidStride { minimum: usize, actual: usize },
    MisalignedStride { alignment: usize },
    MisalignedAddress { alignment: usize },
    Overflow,
    InsufficientCapacity { required: usize, available: usize },
}

/// Hardware constraints on the first byte and the start of each row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaneRequirements {
    address_alignment: usize,
    stride_alignment: usize,
}

impl PlaneRequirements {
    /// Unconstrained CPU-accessible storage.
    pub const CPU: Self = Self {
        address_alignment: 1,
        stride_alignment: 1,
    };

    pub fn new(address_alignment: usize, stride_alignment: usize) -> Result<Self, PlaneError> {
        if !address_alignment.is_power_of_two() || !stride_alignment.is_power_of_two() {
            return Err(PlaneError::InvalidAlignment);
        }
        Ok(Self {
            address_alignment,
            stride_alignment,
        })
    }

    pub const fn address_alignment(self) -> usize {
        self.address_alignment
    }

    pub const fn stride_alignment(self) -> usize {
        self.stride_alignment
    }
}

/// Checked dimensions, format, row stride and byte requirement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaneLayout {
    width: u16,
    height: u16,
    format: ColorFormat,
    stride_bytes: usize,
    required_bytes: usize,
    requirements: PlaneRequirements,
}

impl PlaneLayout {
    /// Use the minimum packed row stride with no extra alignment.
    pub fn packed(width: u16, height: u16, format: ColorFormat) -> Result<Self, PlaneError> {
        let stride_bytes = usize::from(width)
            .checked_mul(format.bytes_per_pixel())
            .ok_or(PlaneError::Overflow)?;
        Self::new(width, height, format, stride_bytes, PlaneRequirements::CPU)
    }

    pub fn new(
        width: u16,
        height: u16,
        format: ColorFormat,
        stride_bytes: usize,
        requirements: PlaneRequirements,
    ) -> Result<Self, PlaneError> {
        let minimum = usize::from(width)
            .checked_mul(format.bytes_per_pixel())
            .ok_or(PlaneError::Overflow)?;
        if stride_bytes < minimum {
            return Err(PlaneError::InvalidStride {
                minimum,
                actual: stride_bytes,
            });
        }
        if stride_bytes % requirements.stride_alignment != 0 {
            return Err(PlaneError::MisalignedStride {
                alignment: requirements.stride_alignment,
            });
        }
        let required_bytes = stride_bytes
            .checked_mul(usize::from(height))
            .ok_or(PlaneError::Overflow)?;
        Ok(Self {
            width,
            height,
            format,
            stride_bytes,
            required_bytes,
            requirements,
        })
    }

    pub const fn width(self) -> u16 {
        self.width
    }

    pub const fn height(self) -> u16 {
        self.height
    }

    pub const fn stride_bytes(self) -> usize {
        self.stride_bytes
    }

    pub const fn required_bytes(self) -> usize {
        self.required_bytes
    }

    /// Validate an actual buffer before exposing it as a render plane.
    pub fn bind<'a>(self, bytes: &'a mut [u8]) -> Result<AlignedPlane<'a>, PlaneError> {
        if bytes.len() < self.required_bytes {
            return Err(PlaneError::InsufficientCapacity {
                required: self.required_bytes,
                available: bytes.len(),
            });
        }
        if self.required_bytes != 0
            && (bytes.as_ptr() as usize) % self.requirements.address_alignment != 0
        {
            return Err(PlaneError::MisalignedAddress {
                alignment: self.requirements.address_alignment,
            });
        }
        Ok(AlignedPlane {
            bytes,
            layout: self,
        })
    }
}

/// A caller-owned plane whose address, stride and capacity match its layout.
pub struct AlignedPlane<'a> {
    bytes: &'a mut [u8],
    layout: PlaneLayout,
}

impl AlignedPlane<'_> {
    pub const fn layout(&self) -> PlaneLayout {
        self.layout
    }

    /// Return only the bytes described by the validated layout.
    pub fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.bytes[..self.layout.required_bytes]
    }

    /// Borrow the plane as a texture without copying its padded rows.
    pub fn texture(&mut self) -> Texture<'_> {
        let layout = self.layout;
        Texture {
            buf: TexBuf::Mut(self.bytes_mut()),
            width: layout.width,
            height: layout.height,
            format: layout.format,
            stride: layout.stride_bytes,
            alpha_mode: AlphaMode::Opaque,
            cache_revision: 0,
            transient: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligned_borrowed_plane_preserves_padded_stride() {
        let requirements = PlaneRequirements::new(64, 64).unwrap();
        let layout = PlaneLayout::new(17, 2, ColorFormat::RGBA8888, 128, requirements).unwrap();
        assert_eq!(layout.required_bytes(), 256);
        let mut storage = [0u8; 320];
        let offset = (64 - (storage.as_ptr() as usize % 64)) % 64;
        let mut plane = layout.bind(&mut storage[offset..offset + 256]).unwrap();
        let texture = plane.texture();
        assert_eq!(texture.stride, 128);
        assert_eq!(texture.buf.as_slice().len(), 256);
        assert!(texture.transient);
    }

    #[test]
    fn rejects_bad_stride_address_and_capacity() {
        let requirements = PlaneRequirements::new(64, 64).unwrap();
        assert_eq!(
            PlaneLayout::new(17, 2, ColorFormat::RGBA8888, 64, requirements),
            Err(PlaneError::InvalidStride {
                minimum: 68,
                actual: 64,
            })
        );
        assert_eq!(
            PlaneLayout::new(17, 2, ColorFormat::RGBA8888, 72, requirements),
            Err(PlaneError::MisalignedStride { alignment: 64 })
        );
        let layout = PlaneLayout::new(17, 2, ColorFormat::RGBA8888, 128, requirements).unwrap();
        let mut storage = [0u8; 320];
        let offset = (64 - (storage.as_ptr() as usize % 64)) % 64;
        assert!(matches!(
            layout.bind(&mut storage[offset..offset + 255]),
            Err(PlaneError::InsufficientCapacity {
                required: 256,
                available: 255
            })
        ));
        assert!(matches!(
            layout.bind(&mut storage[offset + 1..offset + 257]),
            Err(PlaneError::MisalignedAddress { alignment: 64 })
        ));
    }

    #[test]
    fn rejects_invalid_alignment_and_overflow() {
        assert_eq!(
            PlaneRequirements::new(0, 64),
            Err(PlaneError::InvalidAlignment)
        );
        assert_eq!(
            PlaneRequirements::new(64, 3),
            Err(PlaneError::InvalidAlignment)
        );
        let requirements = PlaneRequirements::new(1, 1).unwrap();
        assert_eq!(
            PlaneLayout::new(1, 2, ColorFormat::RGBA8888, usize::MAX, requirements),
            Err(PlaneError::Overflow)
        );
    }
}
