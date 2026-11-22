use super::texture::{ColorFormat, Texture};

/// Invalid or insufficient storage for a render plane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaneError {
    EmptyDimensions,
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

    pub const fn new(
        address_alignment: usize,
        stride_alignment: usize,
    ) -> Result<Self, PlaneError> {
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

    pub fn validate_address(self, bytes: &[u8]) -> Result<(), PlaneError> {
        if !bytes.is_empty() && (bytes.as_ptr() as usize) % self.address_alignment != 0 {
            return Err(PlaneError::MisalignedAddress {
                alignment: self.address_alignment,
            });
        }
        Ok(())
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
    pub const fn packed(width: u16, height: u16, format: ColorFormat) -> Result<Self, PlaneError> {
        Self::aligned(width, height, format, PlaneRequirements::CPU)
    }

    pub const fn aligned(
        width: u16,
        height: u16,
        format: ColorFormat,
        requirements: PlaneRequirements,
    ) -> Result<Self, PlaneError> {
        let minimum = Self::row_bytes(width, format);
        let mask = requirements.stride_alignment - 1;
        let Some(padded) = minimum.checked_add(mask) else {
            return Err(PlaneError::Overflow);
        };
        let stride_bytes = padded & !mask;
        Self::new(width, height, format, stride_bytes, requirements)
    }

    pub const fn new(
        width: u16,
        height: u16,
        format: ColorFormat,
        stride_bytes: usize,
        requirements: PlaneRequirements,
    ) -> Result<Self, PlaneError> {
        if width == 0 || height == 0 {
            return Err(PlaneError::EmptyDimensions);
        }
        let minimum = Self::row_bytes(width, format);
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
        let Some(required_bytes) = stride_bytes.checked_mul(height as usize) else {
            return Err(PlaneError::Overflow);
        };
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

    pub const fn format(self) -> ColorFormat {
        self.format
    }

    pub const fn requirements(self) -> PlaneRequirements {
        self.requirements
    }

    pub const fn required_bytes(self) -> usize {
        self.required_bytes
    }

    /// Bytes through the last pixel, excluding trailing padding in the final row.
    /// Render workspaces require [`Self::required_bytes`] instead.
    pub const fn minimum_bytes(self) -> usize {
        self.required_bytes - self.stride_bytes + Self::row_bytes(self.width, self.format)
    }

    const fn row_bytes(width: u16, format: ColorFormat) -> usize {
        width as usize * format.bytes_per_pixel()
    }

    /// Validate an actual buffer before exposing it as a render plane.
    pub fn bind<'a>(self, bytes: &'a mut [u8]) -> Result<AlignedPlane<'a>, PlaneError> {
        if bytes.len() < self.required_bytes {
            return Err(PlaneError::InsufficientCapacity {
                required: self.required_bytes,
                available: bytes.len(),
            });
        }
        self.requirements
            .validate_address(&bytes[..self.required_bytes])?;
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
        Texture::from_plane(self)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/render/scratch.rs"]
mod tests;
