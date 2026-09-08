use super::{GroupPlanes, SurfaceDescriptor, TileGrid, UnitGroup};

/// Remaining bounded operations for allocation-free surface coverage validation.
///
/// One operation is a group/pair visit, selected-unit visit, or spatial query.
/// Indexed queries have the bounded lookup costs of UnitSelection. No budget
/// exhaustion is treated as valid coverage; callers can retry with more work.
/// Constant-space encoded group validation also charges its documented
/// conservative record/index re-resolution costs through this budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoverageBudget {
    remaining: u64,
}

impl CoverageBudget {
    pub const fn new(operations: u64) -> Self {
        Self {
            remaining: operations,
        }
    }
    pub const fn remaining(self) -> u64 {
        self.remaining
    }
    fn spend(&mut self) -> Result<(), CoverageError> {
        self.spend_many(1)
    }
    pub(crate) fn spend_many(&mut self, operations: u64) -> Result<(), CoverageError> {
        self.remaining = self
            .remaining
            .checked_sub(operations)
            .ok_or(CoverageError::BudgetExceeded)?;
        Ok(())
    }
}

impl SurfaceDescriptor {
    /// Checks exact plane coverage with O(1) memory and explicit work limits.
    /// This does not validate checksums, coding support or reference availability.
    pub fn validate_coverage(
        self,
        groups: &[UnitGroup<'_>],
        budget: &mut CoverageBudget,
    ) -> Result<(), CoverageError> {
        self.validate_coverage_by(groups.len(), |index, _| Ok(groups[index]), budget)
    }

    /// Checks that every group targets this surface and no selected units overlap.
    ///
    /// Unlike [`Self::validate_coverage`], holes are permitted. This is the
    /// coverage rule for a frame that retains unchanged samples from a previous
    /// canvas.
    pub fn validate_disjoint_coverage(
        self,
        groups: &[UnitGroup<'_>],
        budget: &mut CoverageBudget,
    ) -> Result<(), CoverageError> {
        self.validate_disjoint_coverage_by(groups.len(), |index, _| Ok(groups[index]), budget)
    }

    pub(crate) fn validate_coverage_by<'a>(
        self,
        count: usize,
        group_at: impl Fn(usize, &mut CoverageBudget) -> Result<UnitGroup<'a>, CoverageError>,
        budget: &mut CoverageBudget,
    ) -> Result<(), CoverageError> {
        for index in 0..count {
            budget.spend()?;
            let group = group_at(index, budget)?;
            if group.surface() != self {
                return Err(CoverageError::SurfaceMismatch);
            }
        }
        for plane in 0..self.plane_count() {
            let geometry = self.plane(plane).unwrap();
            let expected = u64::from(geometry.width()) * u64::from(geometry.height());
            let mut actual = 0u64;
            for index in 0..count {
                budget.spend()?;
                let group = group_at(index, budget)?;
                actual = actual
                    .checked_add(group.covered_area(plane, budget)?)
                    .ok_or(CoverageError::AreaOverflow)?;
            }
            if actual != expected {
                return Err(CoverageError::AreaMismatch {
                    plane,
                    expected,
                    actual,
                });
            }
            for index in 0..count {
                let group = group_at(index, budget)?;
                for other in index + 1..count {
                    if group.overlaps(group_at(other, budget)?, plane, budget)? {
                        return Err(CoverageError::Overlap { plane });
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn validate_disjoint_coverage_by<'a>(
        self,
        count: usize,
        group_at: impl Fn(usize, &mut CoverageBudget) -> Result<UnitGroup<'a>, CoverageError>,
        budget: &mut CoverageBudget,
    ) -> Result<(), CoverageError> {
        for index in 0..count {
            budget.spend()?;
            let group = group_at(index, budget)?;
            if group.surface() != self {
                return Err(CoverageError::SurfaceMismatch);
            }
        }
        for plane in 0..self.plane_count() {
            for index in 0..count {
                let group = group_at(index, budget)?;
                for other in index + 1..count {
                    if group.overlaps(group_at(other, budget)?, plane, budget)? {
                        return Err(CoverageError::Overlap { plane });
                    }
                }
            }
        }
        Ok(())
    }
}

impl UnitGroup<'_> {
    /// Reports overlap in one plane, including groups using different tile sizes.
    pub fn overlaps(
        self,
        other: Self,
        plane: u8,
        budget: &mut CoverageBudget,
    ) -> Result<bool, CoverageError> {
        budget.spend()?;
        if self.surface() != other.surface() {
            return Err(CoverageError::SurfaceMismatch);
        }
        if plane >= self.surface().plane_count() {
            return Err(CoverageError::InvalidPlane(plane));
        }
        if self.is_empty() || other.is_empty() {
            return Ok(false);
        }
        let (Some(grid), Some(other_grid)) = (self.plane_grid(plane), other.plane_grid(plane))
        else {
            return Ok(false);
        };
        if self.len() == grid.len() || other.len() == other_grid.len() {
            return Ok(true);
        }
        let (small, small_grid, large, large_grid) = if self.len() <= other.len() {
            (self, grid, other, other_grid)
        } else {
            (other, other_grid, self, grid)
        };
        if small_grid == large_grid {
            for cell in small.selection().iter() {
                budget.spend()?;
                if large.selection().contains(cell) {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        for cell in small.selection().iter() {
            budget.spend()?;
            let region = small_grid.get(cell as usize).unwrap();
            let first_row = region.y() / large_grid.tile_height();
            let end_row = region.bottom().div_ceil(large_grid.tile_height());
            if (end_row - first_row) as usize > large.len() {
                for other_cell in large.selection().iter() {
                    budget.spend()?;
                    if region.intersects(large_grid.get(other_cell as usize).unwrap()) {
                        return Ok(true);
                    }
                }
            } else {
                let first_column = region.x() / large_grid.tile_width();
                let end_column = region.right().div_ceil(large_grid.tile_width());
                for row in first_row..end_row {
                    budget.spend()?;
                    let start = row * large_grid.columns() + first_column;
                    let end = row * large_grid.columns() + end_column;
                    if large.selection().range(start..end).unwrap().len() != 0 {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    fn plane_grid(self, plane: u8) -> Option<TileGrid> {
        if !self.planes().contains(plane) {
            return None;
        }
        let geometry = self.surface().plane(plane)?;
        let first = self.grid().get(0)?;
        let first = match self.planes() {
            GroupPlanes::Joint(_) => first.for_plane(self.surface(), plane).ok()?,
            GroupPlanes::Plane(_) => first,
        };
        TileGrid::new(
            geometry.width(),
            geometry.height(),
            first.width(),
            first.height(),
        )
        .ok()
    }

    fn covered_area(self, plane: u8, budget: &mut CoverageBudget) -> Result<u64, CoverageError> {
        let Some(grid) = self.plane_grid(plane) else {
            return Ok(0);
        };
        if self.len() == grid.len() {
            return Ok(u64::from(grid.width()) * u64::from(grid.height()));
        }
        let mut area = 0u64;
        for cell in self.selection().iter() {
            budget.spend()?;
            let region = grid.get(cell as usize).unwrap();
            area += u64::from(region.width()) * u64::from(region.height());
        }
        Ok(area)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CoverageError {
    BudgetExceeded,
    SurfaceMismatch,
    InvalidPlane(u8),
    AreaOverflow,
    AreaMismatch {
        plane: u8,
        expected: u64,
        actual: u64,
    },
    Overlap {
        plane: u8,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        coding::CodingId,
        image::{ColorDescription, Region, SampleLayout},
        media::{CodingRecord, UnitSelection, UnitSelectionEncoding},
    };

    fn coding() -> CodingRecord<'static> {
        CodingRecord::new(CodingId::new(19), 1, &[])
    }

    #[test]
    fn matching_area_cannot_hide_an_overlap_and_hole() {
        let surface =
            SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let left = [0, 0, 0, 0, 1, 0, 0, 0];
        let wrong = [1, 0, 0, 0, 3, 0, 0, 0];
        let right = [2, 0, 0, 0, 3, 0, 0, 0];
        let make = |bytes| {
            UnitGroup::builder(surface, coding(), &[1; 2])
                .with_tiles(1, 1)
                .with_selection(UnitSelection::list(4, bytes).unwrap())
                .build()
                .unwrap()
        };
        assert_eq!(
            surface.validate_coverage(&[make(&left), make(&wrong)], &mut CoverageBudget::new(100)),
            Err(CoverageError::Overlap { plane: 0 })
        );
        assert_eq!(
            surface.validate_coverage(&[make(&left), make(&right)], &mut CoverageBudget::new(100)),
            Ok(())
        );
        assert_eq!(
            surface.validate_coverage(&[make(&left)], &mut CoverageBudget::new(100)),
            Err(CoverageError::AreaMismatch {
                plane: 0,
                expected: 4,
                actual: 2
            })
        );
        assert_eq!(
            surface.validate_disjoint_coverage(&[make(&left)], &mut CoverageBudget::new(100)),
            Ok(())
        );
        assert!(
            !Region::new(0, 0, 1, 1)
                .unwrap()
                .intersects(Region::new(1, 0, 1, 1).unwrap())
        );
        assert!(
            !Region::new(0, 0, 0, 1)
                .unwrap()
                .intersects(Region::new(0, 0, 1, 1).unwrap())
        );
    }

    #[test]
    fn different_grids_and_joint_planar_groups_partition_exactly() {
        let surface =
            SurfaceDescriptor::new(6, 4, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let left = [0, 0, 0, 0, 2, 0, 0, 0];
        let mut right = [0; 48];
        UnitSelectionEncoding::List
            .encode_into(
                24,
                &[3, 4, 5, 9, 10, 11, 15, 16, 17, 21, 22, 23],
                &mut right,
            )
            .unwrap();
        let groups = [
            UnitGroup::builder(surface, coding(), &[1; 2])
                .with_tiles(3, 2)
                .with_selection(UnitSelection::list(4, &left).unwrap())
                .build()
                .unwrap(),
            UnitGroup::builder(surface, coding(), &[1; 12])
                .with_tiles(1, 1)
                .with_selection(UnitSelection::list(24, &right).unwrap())
                .build()
                .unwrap(),
        ];
        assert_eq!(
            surface.validate_coverage(&groups, &mut CoverageBudget::new(100)),
            Ok(())
        );
        let yuv = SurfaceDescriptor::new(
            5,
            3,
            SampleLayout::NV12,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap();
        let planar = [
            UnitGroup::builder(yuv, coding(), &[1])
                .with_planes(GroupPlanes::Joint(1))
                .build()
                .unwrap(),
            UnitGroup::builder(yuv, coding(), &[1; 6])
                .with_tiles(1, 1)
                .with_planes(GroupPlanes::Plane(1))
                .build()
                .unwrap(),
        ];
        assert_eq!(
            yuv.validate_coverage(&planar, &mut CoverageBudget::new(100)),
            Ok(())
        );
        assert_eq!(
            planar[0].overlaps(planar[1], 0, &mut CoverageBudget::new(10)),
            Ok(false)
        );
        assert_eq!(
            planar[0].overlaps(planar[1], 2, &mut CoverageBudget::new(10)),
            Err(CoverageError::InvalidPlane(2))
        );
    }

    #[test]
    fn huge_sparse_grid_uses_selected_units_instead_of_billions_of_rows() {
        let surface =
            SurfaceDescriptor::new(1, u32::MAX, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let first = [0; 4];
        let last = (u32::MAX - 1).to_le_bytes();
        let upper = UnitGroup::builder(surface, coding(), &[1])
            .with_tiles(1, 1 << 31)
            .with_selection(UnitSelection::list(2, &first).unwrap())
            .build()
            .unwrap();
        let lower = UnitGroup::builder(surface, coding(), &[1])
            .with_tiles(1, 1)
            .with_selection(UnitSelection::list(u32::MAX, &last).unwrap())
            .build()
            .unwrap();
        let mut budget = CoverageBudget::new(3);
        assert_eq!(upper.overlaps(lower, 0, &mut budget), Ok(false));
        assert_eq!(budget.remaining(), 0);
        assert_eq!(
            upper.overlaps(lower, 0, &mut budget),
            Err(CoverageError::BudgetExceeded)
        );
        let whole = UnitGroup::builder(surface, coding(), &[1]).build().unwrap();
        assert_eq!(
            surface.validate_coverage(&[whole], &mut CoverageBudget::new(2)),
            Ok(())
        );
    }

    #[test]
    fn budget_exhaustion_and_surface_mismatch_never_pass_validation() {
        let surface =
            SurfaceDescriptor::new(1, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let group = UnitGroup::builder(surface, coding(), &[1]).build().unwrap();
        let mut budget = CoverageBudget::new(1);
        assert_eq!(
            surface.validate_coverage(&[group], &mut budget),
            Err(CoverageError::BudgetExceeded)
        );
        assert_eq!(budget.remaining(), 0);
        assert_eq!(
            surface.validate_coverage(&[group], &mut CoverageBudget::new(2)),
            Ok(())
        );
        let empty = SurfaceDescriptor::new(0, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        assert_eq!(
            empty.validate_coverage(&[], &mut CoverageBudget::new(0)),
            Ok(())
        );
        assert_eq!(
            empty.validate_coverage(&[group], &mut CoverageBudget::new(10)),
            Err(CoverageError::SurfaceMismatch)
        );
        let huge =
            SurfaceDescriptor::new(u32::MAX, u32::MAX, SampleLayout::A8, ColorDescription::NONE)
                .unwrap();
        let huge_group = UnitGroup::builder(huge, coding(), &[1]).build().unwrap();
        assert_eq!(
            huge.validate_coverage(&[huge_group, huge_group], &mut CoverageBudget::new(10)),
            Err(CoverageError::AreaOverflow)
        );
    }
}
