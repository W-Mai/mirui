use core::iter::FusedIterator;

use super::{DecodeUnitRef, Region, RegionError, TileGrid, UnitGroup};
use crate::media::UnitSelection;

impl<'a> UnitGroup<'a> {
    /// Selects complete units intersecting a region in this group's grid space.
    ///
    /// Joint groups use surface coordinates; planar groups use plane elements.
    /// Queries check bounds without clipping. Sparse rank jumps skip absent rows
    /// and columns; no unit array, sample decode or integrity scan is performed.
    /// Returned units may extend beyond the requested rectangle.
    ///
    /// ```
    /// use mirx::{image::{ColorDescription, SampleLayout, SurfaceDescriptor, UnitGroup},
    ///     media::CodingRecord};
    /// let surface = SurfaceDescriptor::new(8, 2, SampleLayout::A8, ColorDescription::NONE).unwrap();
    /// let group = UnitGroup::builder(surface, CodingRecord::RAW, &[0; 16])
    ///     .with_tiles(2, 1).build().unwrap();
    /// let units = group.units_in(surface.region(3, 0, 2, 2).unwrap()).unwrap();
    /// assert_eq!(units.work_bound(), 9);
    /// assert_eq!(units.map(|unit| unit.cell()).collect::<Vec<_>>(), [1, 2, 5, 6]);
    /// ```
    pub fn units_in(self, region: Region) -> Result<RegionUnits<'a>, RegionError> {
        Ok(RegionUnits {
            group: self,
            cells: RegionCells::new(self.grid(), self.selection(), region)?,
        })
    }
}

/// Borrowed units in stored order, with bounded sparse spatial selection.
///
/// Size hints are upper bounds, not exact counts. Skipping returned units is
/// sequential; rank jumps avoid scanning unrelated cells and empty grid rows.
#[derive(Clone, Debug)]
pub struct RegionUnits<'a> {
    group: UnitGroup<'a>,
    cells: RegionCells<'a>,
}

impl RegionUnits<'_> {
    /// Conservative selection-probe bound for a full traversal after construction.
    ///
    /// One probe performs a rank/select range lookup and may resolve a unit.
    /// Each lookup retains the index's bounded binary-search/checkpoint cost.
    /// Syntax, checksums, output and decoding are not included in this measure.
    /// The bound does not decrease as the iterator is consumed.
    pub const fn work_bound(&self) -> u64 {
        self.cells.work_bound
    }
}

impl<'a> Iterator for RegionUnits<'a> {
    type Item = DecodeUnitRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.cells
            .next()
            .map(|cell| self.group.cell(cell).expect("selected grid cell"))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.cells.size_hint()
    }
}

impl FusedIterator for RegionUnits<'_> {}

#[derive(Clone, Debug)]
struct RegionCells<'a> {
    selection: UnitSelection<'a>,
    columns: u32,
    first_column: u32,
    end_column: u32,
    next_cell: u32,
    end_cell: u32,
    upper: usize,
    work_bound: u64,
    #[cfg(test)]
    probes: u64,
}

impl<'a> RegionCells<'a> {
    fn new(
        grid: TileGrid,
        selection: UnitSelection<'a>,
        region: Region,
    ) -> Result<Self, RegionError> {
        if region.right() > grid.width() || region.bottom() > grid.height() {
            return Err(RegionError::OutOfBounds);
        }
        let mut cells = Self {
            selection,
            columns: grid.columns(),
            first_column: 0,
            end_column: 0,
            next_cell: 0,
            end_cell: 0,
            upper: 0,
            work_bound: 0,
            #[cfg(test)]
            probes: 0,
        };
        if region.is_empty() || selection.is_empty() {
            return Ok(cells);
        }
        cells.first_column = region.x() / grid.tile_width();
        cells.end_column = region.right().div_ceil(grid.tile_width());
        let first_row = region.y() / grid.tile_height();
        let end_row = region.bottom().div_ceil(grid.tile_height());
        cells.next_cell = first_row * cells.columns + cells.first_column;
        cells.end_cell = (end_row - 1) * cells.columns + cells.end_column;
        let enclosing = selection
            .range(cells.next_cell..cells.end_cell)
            .unwrap()
            .len() as u64;
        let rows = u64::from(end_row - first_row);
        let columns = u64::from(cells.end_column - cells.first_column);
        cells.upper = enclosing.min(rows * columns) as usize;
        if cells.upper == 0 {
            cells.next_cell = cells.end_cell;
        } else {
            cells.work_bound = 1 + cells.upper as u64 + 2 * rows.min(enclosing);
        }
        Ok(cells)
    }
}

impl Iterator for RegionCells<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next_cell < self.end_cell {
            #[cfg(test)]
            {
                self.probes += 1;
            }
            let Some(cell) = self
                .selection
                .range(self.next_cell..self.end_cell)
                .unwrap()
                .next()
            else {
                self.next_cell = self.end_cell;
                break;
            };
            let row = cell / self.columns;
            let column = cell % self.columns;
            if column < self.first_column {
                self.next_cell = row * self.columns + self.first_column;
            } else if column >= self.end_column {
                let next_row_start =
                    (u64::from(row) + 1) * u64::from(self.columns) + u64::from(self.first_column);
                self.next_cell = next_row_start.min(u64::from(self.end_cell)) as u32;
            } else {
                self.next_cell = cell + 1;
                self.upper -= 1;
                return Some(cell);
            }
        }
        self.upper = 0;
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.upper))
    }
}

impl FusedIterator for RegionCells<'_> {}

#[cfg(test)]
mod tests;
