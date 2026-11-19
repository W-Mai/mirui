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
    MissingObject,
    NotHeightField,
    Tilted,
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
    materials: Cow<'static, [u8]>,
    colors: &'static [Color],
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
            levels,
            materials,
            colors: &[],
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

    pub fn grid(&self) -> TerrainGrid {
        self.grid
    }
    pub fn levels(&self) -> &[u8] {
        &self.levels
    }
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
        let i = self.grid.address(self.grid.cell_at(x, z)?)?;
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
        levels[i] = value.level;
        materials[i] = value.material;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }
}
