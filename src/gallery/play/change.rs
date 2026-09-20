#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ChangeSet(u8);

impl ChangeSet {
    pub(crate) const NONE: Self = Self(0);
    pub(crate) const MODEL: Self = Self(1 << 0);
    pub(crate) const VISUAL: Self = Self(1 << 1);
    pub(crate) const LAYOUT: Self = Self(1 << 2);
    pub(crate) const PERSISTENCE: Self = Self(1 << 3);

    pub(crate) const fn contains(self, change: Self) -> bool {
        self.0 & change.0 == change.0
    }

    pub(crate) const fn union(self, change: Self) -> Self {
        Self(self.0 | change.0)
    }
}

impl core::ops::BitOr for ChangeSet {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_compose_without_losing_independent_domains() {
        let changes = ChangeSet::MODEL | ChangeSet::VISUAL;
        assert!(changes.contains(ChangeSet::MODEL));
        assert!(changes.contains(ChangeSet::VISUAL));
        assert!(!changes.contains(ChangeSet::LAYOUT));
        assert!(!changes.contains(ChangeSet::PERSISTENCE));
        assert_eq!(ChangeSet::NONE, ChangeSet::default());
    }
}
