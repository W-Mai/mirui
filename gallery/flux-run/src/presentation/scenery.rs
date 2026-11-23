use mirui::types::Fixed;
use mirui_sprite_stack::{HeightField, Pose, StackVisual, TerrainCell, TerrainGrid};

use super::assets::{BEACON, CARGO, GRATE, Material, SceneTone, VENT};
use crate::world::{BRIDGE_START, CELL_SIZE, Harbor, Layout};

pub(super) const MAX_PROPS: usize = 16;

pub(super) fn floor(harbor: &Harbor, detail: usize, tone: SceneTone) -> HeightField {
    let layout = &harbor.layout;
    let subdivisions = if detail == 0 { 1 } else { 2 };
    let side = layout.side() * subdivisions;
    let cell = CELL_SIZE / subdivisions as i32;
    let grid = TerrainGrid::finite([side as u16; 2], Fixed::from_int(cell)).expect("harbor grid");

    HeightField::generate(grid, tone.palette().terrain, |[x, z]| {
        let [x, z] = [x as usize, z as usize];
        let [col, row] = [x / subdivisions, z / subdivisions];
        let elevation = roof(layout, col, row);
        let grain = variation(x, z, harbor.phase);
        let last = side - 1;
        let ramp = || {
            harbor.bridges().into_iter().flatten().any(|bridge| {
                bridge.x / CELL_SIZE == col as i32
                    && [BRIDGE_START, bridge.end]
                        .into_iter()
                        .any(|end| (z as i32 * cell - end).abs() <= 4)
            })
        };
        let material = if elevation > 0 {
            if detail == 0 {
                Material::Roof
            } else if x % subdivisions == 0 {
                Material::RoofEdge
            } else if (z / subdivisions).is_multiple_of(2) {
                Material::RoofPanel
            } else {
                Material::Roof
            }
        } else if x == 0 || x == last {
            if z == 0 || z == last {
                Material::Curb
            } else {
                Material::Water
            }
        } else if z == 0 || z == last || x == 1 || x == last - 1 {
            Material::Curb
        } else if detail != 0 && z.is_multiple_of(2) && ramp() {
            Material::Paint
        } else if layout.freight_row().is_some_and(|start| row >= start) {
            if detail != 0 && z % (subdivisions * 2) == 0 && x % 4 < 2 {
                Material::Paint
            } else {
                Material::Freight
            }
        } else if col == 2 || col == layout.side() - 3 || row == 3 {
            if detail != 0 && x % subdivisions == 0 && z % 4 < 2 {
                Material::Paint
            } else {
                Material::Road
            }
        } else if detail != 0 && (x.is_multiple_of(6) || (z + 3 * (x / 6 % 2)).is_multiple_of(8)) {
            Material::Joint
        } else {
            match grain % 9 {
                0 | 1 => Material::Patina,
                2 => Material::Aggregate,
                _ => Material::Concrete,
            }
        };
        TerrainCell {
            level: elevation,
            material: material as u8,
        }
    })
    .expect("harbor field")
}

pub(super) fn props(
    harbor: &Harbor,
    detail: usize,
    tone: SceneTone,
    mut place: impl FnMut(StackVisual),
) {
    let layout = &harbor.layout;
    let extent = layout.extent();
    let side = layout.side();
    let palette = &tone.palette().normal;
    let mut at = |asset, x, y, z| {
        place(
            StackVisual::new(asset)
                .at(Pose::at(x, y, z))
                .with_palette(palette),
        );
    };

    for z in 1..side - 1 {
        for x in 1..side - 1 {
            let height = roof(layout, x, z);
            if height == 0 || layout.blocked(x - 1, z) || layout.blocked(x, z - 1) {
                continue;
            }
            let center = |cell| cell as i32 * CELL_SIZE + CELL_SIZE / 2;
            let [world_x, world_z] = [center(x), center(z)];
            if harbor.bridges().into_iter().flatten().any(|bridge| {
                (BRIDGE_START..=bridge.end).contains(&world_z) && (world_x - bridge.x).abs() <= 10
            }) {
                continue;
            }
            // Place machinery within the solid roof footprint.
            let asset = if detail == 0 || variation(x, z, harbor.phase).is_multiple_of(2) {
                &VENT[detail]
            } else {
                &CARGO[detail]
            };
            at(asset, world_x, i32::from(height), world_z);
        }
    }

    // Quay fixtures occupy the two-unit rim beyond the ship's legal travel bounds.
    for (x, z) in [(2, 15), (extent - 2, extent - 15)] {
        at(&BEACON[detail], x, 0, z);
    }
    if detail != 0 {
        let offset = i32::from(harbor.phase % 3) * 2;
        for z in [extent / 3, 2 * extent / 3] {
            at(&CARGO[detail], 2, 0, z + offset);
            at(&BEACON[detail], extent - 2, 0, z + offset);
        }
        // Flush grates sit below the skiff and mark the loading lanes.
        for z in [20, extent - 19] {
            at(&GRATE[detail], extent / 2, 0, z);
            at(&GRATE[detail], extent / 2 + 8, 0, z);
        }
    }
}

fn roof(layout: &Layout, x: usize, z: usize) -> u8 {
    if layout.blocked(x, z) {
        if x.is_multiple_of(2) { 4 } else { 5 }
    } else {
        0
    }
}

fn variation(x: usize, z: usize, seed: u16) -> u32 {
    let hash = (x as u32).wrapping_mul(0x9e37_79b9)
        ^ (z as u32).wrapping_mul(0x85eb_ca6b)
        ^ u32::from(seed);
    (hash ^ (hash >> 16)).wrapping_mul(0x7feb_352d) >> 20
}
