/// Power-of-two byte alignment used by memory and storage contracts.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ByteAlignment(u32);

impl ByteAlignment {
    pub const ONE: Self = Self(1);

    /// Creates a validated byte alignment.
    ///
    /// MIRX stores alignments as a base-two exponent, so the largest
    /// representable alignment is 2^31 bytes.
    pub const fn new(bytes: u32) -> Result<Self, InvalidByteAlignment> {
        if bytes == 0 {
            return Err(InvalidByteAlignment::Zero);
        }
        if !bytes.is_power_of_two() {
            return Err(InvalidByteAlignment::NotPowerOfTwo { bytes });
        }
        Ok(Self(bytes))
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    pub const fn max(self, other: Self) -> Self {
        if self.0 >= other.0 { self } else { other }
    }

    pub const fn log2(self) -> u8 {
        self.0.trailing_zeros() as u8
    }
}

impl Default for ByteAlignment {
    fn default() -> Self {
        Self::ONE
    }
}

impl TryFrom<u32> for ByteAlignment {
    type Error = InvalidByteAlignment;

    fn try_from(bytes: u32) -> Result<Self, Self::Error> {
        Self::new(bytes)
    }
}

impl From<ByteAlignment> for u32 {
    fn from(alignment: ByteAlignment) -> Self {
        alignment.get()
    }
}

impl PartialEq<u32> for ByteAlignment {
    fn eq(&self, other: &u32) -> bool {
        self.0 == *other
    }
}

impl PartialEq<ByteAlignment> for u32 {
    fn eq(&self, other: &ByteAlignment) -> bool {
        *self == other.0
    }
}

impl core::fmt::Display for ByteAlignment {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Reason a byte alignment cannot be represented by MIRX.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum InvalidByteAlignment {
    Zero,
    NotPowerOfTwo { bytes: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_complete_wire_range() {
        assert_eq!(ByteAlignment::new(1), Ok(ByteAlignment::ONE));
        assert_eq!(ByteAlignment::new(64).unwrap().get(), 64);
        assert_eq!(ByteAlignment::new(1 << 31).unwrap().log2(), 31);
    }

    #[test]
    fn rejects_invalid_values_at_construction() {
        assert_eq!(ByteAlignment::new(0), Err(InvalidByteAlignment::Zero));
        assert_eq!(
            ByteAlignment::new(48),
            Err(InvalidByteAlignment::NotPowerOfTwo { bytes: 48 })
        );
    }
}
