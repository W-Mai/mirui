use super::Region;

/// Bounded upload validity for one immutable texture revision.
///
/// Forgetting coverage permits a later upload; it never permits sampling a hole.
#[derive(Clone, Copy, Default)]
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
#[path = "../../../tests/unit/render/texture/coverage.rs"]
mod tests;
