use super::Region;

/// Bounded upload validity for one immutable texture revision.
///
/// Forgetting coverage permits a later upload; it never permits sampling a hole.
#[derive(Default)]
pub(crate) struct UploadCoverage(Option<Region>);

impl UploadCoverage {
    pub(crate) fn contains(&self, area: Region) -> bool {
        area.is_empty() || self.0.is_some_and(|valid| contains(valid, area))
    }

    /// Call only after every byte in the requested rectangle was uploaded.
    pub(crate) fn uploaded(&mut self, area: Region) {
        if area.is_empty() || self.contains(area) {
            return;
        }
        self.0 = Some(match self.0 {
            Some(old) if contains(area, old) => area,
            Some(old)
                if (old.x() == area.x()
                    && old.width() == area.width()
                    && old.y() <= area.bottom()
                    && area.y() <= old.bottom())
                    || (old.y() == area.y()
                        && old.height() == area.height()
                        && old.x() <= area.right()
                        && area.x() <= old.right()) =>
            {
                let x = old.x().min(area.x());
                let y = old.y().min(area.y());
                Region::new(
                    x,
                    y,
                    old.right().max(area.right()) - x,
                    old.bottom().max(area.bottom()) - y,
                )
                .expect("union of checked rectangles")
            }
            _ => area,
        });
    }
}

fn contains(outer: Region, inner: Region) -> bool {
    outer.x() <= inner.x()
        && outer.y() <= inner.y()
        && outer.right() >= inner.right()
        && outer.bottom() >= inner.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_coverage_never_fills_holes() {
        let mut coverage = UploadCoverage::default();
        let mut uploaded = [[false; 8]; 8];
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for (x, y, w, h) in [
                    (0, 0, 2, 2),
                    (4, 0, 2, 2),
                    (2, 0, 2, 2),
                    (2, 2, 4, 2),
                    (0, 4, 8, 2),
                    (0, 6, 8, 2),
                ] {
                    let area = Region::new(x, y, w, h).unwrap();
                    assert!(!coverage.contains(area));
                    coverage.uploaded(area);
                    for row in &mut uploaded[y as usize..(y + h) as usize] {
                        row[x as usize..(x + w) as usize].fill(true);
                    }
                    assert!(coverage.contains(area));
                    for (y, row) in uploaded.iter().enumerate() {
                        for (x, valid) in row.iter().enumerate() {
                            if coverage.contains(Region::new(x as u32, y as u32, 1, 1).unwrap()) {
                                assert!(*valid);
                            }
                        }
                    }
                }
            }),
            0
        );
        let full = Region::new(0, 0, 8, 8).unwrap();
        assert!(!coverage.contains(full));
        coverage.uploaded(full);
        assert!(coverage.contains(full));
    }
}
