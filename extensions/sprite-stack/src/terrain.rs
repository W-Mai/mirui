use alloc::{borrow::Cow, vec::Vec};
use mirui::types::{Color, Fixed, Fixed64};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerrainError {
    InvalidGrid,
    InvalidPlanes,
    InvalidMaterial,
    Allocation,
    OutsideGrid,
    ReadOnly,
}

/// X/Z cells, with positive Y pointing up. Repeated grids wrap independently on each axis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerrainGrid {
    pub(crate) size: [u16; 2],
    pub(crate) cell_size: Fixed,
    pub(crate) repeats: bool,
    base: Fixed,
    step: Fixed,
}

impl TerrainGrid {
    pub fn finite(size: [u16; 2], cell_size: Fixed) -> Result<Self, TerrainError> {
        Self::new(size, cell_size, false)
    }

    pub fn repeating(size: [u16; 2], cell_size: Fixed) -> Result<Self, TerrainError> {
        Self::new(size, cell_size, true)
    }

    fn new(size: [u16; 2], cell_size: Fixed, repeats: bool) -> Result<Self, TerrainError> {
        if size.contains(&0)
            || !cell_size.is_positive()
            || Fixed64::from(cell_size) * i64::from(size[0].max(size[1]))
                > Fixed64::from(Fixed::MAX)
        {
            return Err(TerrainError::InvalidGrid);
        }
        Ok(Self {
            size,
            cell_size,
            repeats,
            base: Fixed::ZERO,
            step: Fixed::ONE,
        })
    }

    pub fn with_elevation(
        mut self,
        base: Fixed,
        level_height: Fixed,
    ) -> Result<Self, TerrainError> {
        if !level_height.is_positive()
            || Fixed64::from(base) + Fixed64::from(level_height) * 255 > Fixed64::from(Fixed::MAX)
        {
            return Err(TerrainError::InvalidGrid);
        }
        self.base = base;
        self.step = level_height;
        Ok(self)
    }

    pub fn size(self) -> [u16; 2] {
        self.size
    }
    pub fn cell_size(self) -> Fixed {
        self.cell_size
    }
    pub fn elevation(self, level: u8) -> Fixed {
        (Fixed64::from(self.base) + Fixed64::from(self.step) * i64::from(level)).into()
    }

    /// Addresses the cell containing the world coordinate, including negative fractions.
    pub fn cell_at(self, x: Fixed, z: Fixed) -> Option<[u16; 2]> {
        let step = (Fixed64::from(self.cell_size) * 256).to_int();
        let axis = |v: Fixed, n: u16| {
            let cell = (Fixed64::from(v) * 256).to_int().div_euclid(step);
            if self.repeats {
                Some(cell.rem_euclid(i64::from(n)) as u16)
            } else if (0..i64::from(n)).contains(&cell) {
                Some(cell as u16)
            } else {
                None
            }
        };
        Some([axis(x, self.size[0])?, axis(z, self.size[1])?])
    }

    pub fn address(self, cell: [u16; 2]) -> Option<usize> {
        (cell[0] < self.size[0] && cell[1] < self.size[1])
            .then(|| usize::from(cell[1]) * usize::from(self.size[0]) + usize::from(cell[0]))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerrainCell {
    pub level: u8,
    pub material: u8,
}

/// Two byte planes. Moving an owned plane into a scene retains its allocation.
#[mirui::model]
pub struct HeightField {
    grid: TerrainGrid,
    levels: Cow<'static, [u8]>,
    max_level: u8,
    materials: Cow<'static, [u8]>,
    colors: &'static [Color],
    coordinate_map: Option<fn([Fixed; 2]) -> [Fixed; 2]>,
    revision: u64,
}

impl HeightField {
    pub fn from_planes(
        grid: TerrainGrid,
        levels: impl Into<Cow<'static, [u8]>>,
        materials: impl Into<Cow<'static, [u8]>>,
    ) -> Result<Self, TerrainError> {
        let (levels, materials) = (levels.into(), materials.into());
        let len = usize::from(grid.size[0]) * usize::from(grid.size[1]);
        if levels.len() != len || materials.len() != len {
            return Err(TerrainError::InvalidPlanes);
        }
        Ok(Self {
            grid,
            max_level: levels.iter().copied().max().unwrap_or(0),
            levels,
            materials,
            colors: &[],
            coordinate_map: None,
            revision: 0,
        })
    }

    /// Maps each index to a surface color. Without a table, the material index is its grayscale intensity.
    pub fn with_palette(mut self, colors: &'static [Color]) -> Result<Self, TerrainError> {
        if colors.is_empty()
            || colors.len() > 256
            || self
                .materials
                .iter()
                .any(|m| usize::from(*m) >= colors.len())
        {
            return Err(TerrainError::InvalidMaterial);
        }
        self.colors = colors;
        Ok(self)
    }

    pub fn generate(
        grid: TerrainGrid,
        colors: &'static [Color],
        mut cell: impl FnMut([i32; 2]) -> TerrainCell,
    ) -> Result<Self, TerrainError> {
        let len = usize::from(grid.size[0]) * usize::from(grid.size[1]);
        let mut levels = Vec::new();
        let mut materials = Vec::new();
        levels
            .try_reserve_exact(len)
            .map_err(|_| TerrainError::Allocation)?;
        materials
            .try_reserve_exact(len)
            .map_err(|_| TerrainError::Allocation)?;
        for row in 0..grid.size[1] {
            for col in 0..grid.size[0] {
                let value = cell([i32::from(col), i32::from(row)]);
                levels.push(value.level);
                materials.push(value.material);
            }
        }
        Self::from_planes(grid, levels, materials)?.with_palette(colors)
    }

    /// Maps object-local X/Z into the plane coordinates before cell lookup.
    pub fn with_coordinate_map(mut self, map: fn([Fixed; 2]) -> [Fixed; 2]) -> Self {
        self.coordinate_map = Some(map);
        self
    }

    /// Returns the shared plane index for a local point. Height and material may be sampled independently.
    pub fn address_at(&self, x: Fixed, z: Fixed) -> Option<usize> {
        let [x, z] = self.coordinate_map.map_or([x, z], |map| map([x, z]));
        self.grid.address(self.grid.cell_at(x, z)?)
    }

    pub fn grid(&self) -> TerrainGrid {
        self.grid
    }

    pub(crate) fn height_bounds(&self) -> [Fixed; 2] {
        [self.grid.elevation(0), self.grid.elevation(self.max_level)]
    }

    pub(crate) fn has_finite_local_bounds(&self) -> bool {
        self.coordinate_map.is_none() && !self.grid.repeats
    }
    #[inline]
    pub fn levels(&self) -> &[u8] {
        &self.levels
    }
    #[inline]
    pub fn materials(&self) -> &[u8] {
        &self.materials
    }
    pub fn color(&self, material: u8) -> Color {
        self.colors
            .get(usize::from(material))
            .copied()
            .unwrap_or(Color::rgb(material, material, material))
    }
    pub fn height_at(&self, x: Fixed, z: Fixed) -> Option<Fixed> {
        let i = self.address_at(x, z)?;
        Some(self.grid.elevation(self.levels[i]))
    }
}

#[mirui::model]
impl HeightField {
    #[observe]
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// Edits owned planes. Borrowed flash planes remain read-only.
    pub fn set_cell(&mut self, cell: [u16; 2], value: TerrainCell) -> Result<(), TerrainError> {
        let i = self.grid.address(cell).ok_or(TerrainError::OutsideGrid)?;
        if !self.colors.is_empty() && usize::from(value.material) >= self.colors.len() {
            return Err(TerrainError::InvalidMaterial);
        }
        if self.levels[i] == value.level && self.materials[i] == value.material {
            return Ok(());
        }
        let (Cow::Owned(levels), Cow::Owned(materials)) = (&mut self.levels, &mut self.materials)
        else {
            return Err(TerrainError::ReadOnly);
        };
        let previous = core::mem::replace(&mut levels[i], value.level);
        if value.level > self.max_level {
            self.max_level = value.level;
        } else if previous == self.max_level && previous > value.level {
            self.max_level = levels.iter().copied().max().unwrap_or(0);
        }
        materials[i] = value.material;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mirui::{core::model::ModelHandle, prelude::App};

    #[test]
    fn shared_height_bounds_follow_successful_plane_changes() {
        static COLORS: [Color; 1] = [Color::rgb(1, 2, 3)];
        let grid = TerrainGrid::repeating([2, 1], Fixed::ONE)
            .unwrap()
            .with_elevation((-1).into(), Fixed::HALF)
            .unwrap();
        assert!(matches!(
            HeightField::from_planes(grid, &[][..], &[][..]),
            Err(TerrainError::InvalidPlanes)
        ));
        let field = HeightField::from_planes(grid, alloc::vec![2, 2], alloc::vec![0, 0])
            .unwrap()
            .with_palette(&COLORS)
            .unwrap();
        let mut app = App::headless(8, 8);
        let handle = app.add_model(field).finish();
        let copy = handle.clone();
        let bounds = || copy.read(HeightField::height_bounds);
        let cell = |level| TerrainCell { level, material: 0 };
        assert_eq!(bounds(), [(-1).into(), Fixed::ZERO]);
        handle.set_cell([0, 0], cell(8)).unwrap();
        assert_eq!(bounds(), [(-1).into(), 3.into()]);
        assert_eq!(
            handle.set_cell(
                [0, 0],
                TerrainCell {
                    level: 0,
                    material: 1
                }
            ),
            Err(TerrainError::InvalidMaterial)
        );
        assert_eq!(
            handle.set_cell([2, 0], cell(255)),
            Err(TerrainError::OutsideGrid)
        );
        assert_eq!(bounds(), [(-1).into(), 3.into()]);
        handle.set_cell([0, 0], cell(0)).unwrap();
        assert_eq!(bounds(), [(-1).into(), Fixed::ZERO]);
        handle.set_cell([1, 0], cell(0)).unwrap();
        assert_eq!(bounds(), [(-1).into(); 2]);
        let mut flash = HeightField::from_planes(grid, &[4, 4][..], &[0, 0][..]).unwrap();
        assert_eq!(
            flash.set_cell([0, 0], cell(255)),
            Err(TerrainError::ReadOnly)
        );
        assert_eq!(flash.height_bounds(), [(-1).into(), Fixed::ONE]);
    }
}
