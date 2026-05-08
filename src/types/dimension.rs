use super::Fixed;

/// Dimension specification for layout properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dimension {
    /// Fixed pixel value
    Px(Fixed),
    /// Percentage of parent size (0-100 stored as Fixed)
    Percent(Fixed),
    /// Determined by layout algorithm
    Auto,
    /// Sized to fit content
    Content,
}

impl Dimension {
    /// Resolve this dimension to a concrete Fixed value given the parent's size.
    /// - Px: returns the value directly
    /// - Percent: parent_size * percent / 100
    /// - Auto/Content: returns None (caller must handle)
    #[inline]
    pub fn resolve(self, parent_size: Fixed) -> Option<Fixed> {
        match self {
            Self::Px(v) => Some(v),
            Self::Percent(pct) => Some(parent_size * pct / Fixed::from_int(100)),
            Self::Auto | Self::Content => None,
        }
    }
}

impl Default for Dimension {
    fn default() -> Self {
        Self::Auto
    }
}

impl From<i32> for Dimension {
    #[inline]
    fn from(v: i32) -> Self {
        Self::Px(Fixed::from_int(v))
    }
}

impl From<u16> for Dimension {
    #[inline]
    fn from(v: u16) -> Self {
        Self::Px(Fixed::from_int(v as i32))
    }
}

impl From<Fixed> for Dimension {
    #[inline]
    fn from(v: Fixed) -> Self {
        Self::Px(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_px() {
        let d = Dimension::Px(Fixed::from_int(50));
        assert_eq!(d.resolve(Fixed::from_int(200)), Some(Fixed::from_int(50)));
    }

    #[test]
    fn resolve_percent() {
        let d = Dimension::Percent(Fixed::from_int(50));
        let result = d.resolve(Fixed::from_int(200)).unwrap();
        assert_eq!(result.to_int(), 100);
    }

    #[test]
    fn resolve_auto() {
        assert_eq!(Dimension::Auto.resolve(Fixed::from_int(200)), None);
    }

    #[test]
    fn from_i32() {
        let d: Dimension = 100.into();
        assert_eq!(d, Dimension::Px(Fixed::from_int(100)));
    }
}
