use mirui::types::Fixed;
use mirui_sprite_stack::{HeightField, Pose, StackVisual, TerrainCell, TerrainGrid};

use super::assets::{BEACON, CARGO, Material, SceneTone, VENT, bridge_model};
use crate::world::{CELL_SIZE, Dock, Harbor};

pub(super) const MAX_PROPS: usize = crate::world::MAX_DOCKS * 2;

pub(super) fn water(harbor: &Harbor, tone: SceneTone) -> StackVisual {
    let grid = TerrainGrid::finite([1; 2], Fixed::from_int(harbor.extent())).expect("harbor grid");
    let field = HeightField::generate(grid, tone.palette().terrain, |_| TerrainCell {
        level: 0,
        material: Material::Water as u8,
    })
    .expect("water surface");
    StackVisual::new(field).at(Pose::at(0, -3, 0))
}

fn quay(harbor: &Harbor, dock: Dock, freight: bool, detail: usize, tone: SceneTone) -> StackVisual {
    let subdivisions = if detail == 0 { 1 } else { 2 };
    let [width, depth] = core::array::from_fn(|i| (dock.end[i] - dock.min[i]) * subdivisions);
    let cell = CELL_SIZE / subdivisions as i32;
    let grid = TerrainGrid::finite([width as u16, depth as u16], Fixed::from_int(cell))
        .expect("quay grid");
    let field = HeightField::generate(grid, tone.palette().terrain, |[x, z]| {
        let [x, z] = [x as usize, z as usize];
        let [col, row] = [
            dock.min[0] + x / subdivisions,
            dock.min[1] + z / subdivisions,
        ];
        let blocked = harbor.layout.blocked(col, row);
        let material = if !harbor.layout.on_land(col, row) {
            Material::Slip
        } else if blocked {
            if detail != 0 && x.is_multiple_of(subdivisions) {
                Material::RoofEdge
            } else {
                Material::Roof
            }
        } else if x == 0 || z == 0 || x + 1 == width || z + 1 == depth {
            Material::Curb
        } else if freight {
            if detail != 0 && z % 4 == 1 && x % 4 < 2 {
                Material::Paint
            } else {
                Material::Freight
            }
        } else if detail != 0 && (x.is_multiple_of(6) || (z + 3 * (x / 6 % 2)).is_multiple_of(8)) {
            Material::Joint
        } else {
            match variation(col, row, harbor.phase) % 9 {
                0 | 1 => Material::Patina,
                2 => Material::Aggregate,
                _ => Material::Concrete,
            }
        };
        TerrainCell {
            level: if blocked { 4 } else { 0 },
            material: material as u8,
        }
    })
    .expect("quay surface");
    StackVisual::new(field).at(Pose::at(
        dock.min[0] as i32 * CELL_SIZE,
        0,
        dock.min[1] as i32 * CELL_SIZE,
    ))
}

pub(super) fn mount(
    harbor: &Harbor,
    detail: usize,
    tone: SceneTone,
    mut place: impl FnMut(StackVisual),
) {
    let palette = &tone.palette().normal;
    for (slot, dock) in harbor.docks.into_iter().enumerate() {
        let Some(dock) = dock else { continue };
        place(quay(harbor, dock, slot >= 2, detail, tone));
        // A roof fixture and a harbor light in the outer corner of each quay.
        let outer = |i| if dock.min[i] == 0 { 0 } else { dock.end[i] - 1 };
        let [x, z] = [outer(0), outer(1)];
        if !harbor.layout.blocked(x, z) {
            continue;
        }
        let center = |cell| cell as i32 * CELL_SIZE + CELL_SIZE / 2;
        let asset = if variation(x, z, harbor.phase).is_multiple_of(2) {
            &VENT[detail]
        } else {
            &CARGO[detail]
        };
        place(
            StackVisual::new(asset)
                .at(Pose::at(center(x), 4, center(z)))
                .with_palette(palette),
        );
        if detail != 0 {
            let outer = |i| {
                if dock.min[i] == 0 {
                    1
                } else {
                    dock.end[i] as i32 * CELL_SIZE - 1
                }
            };
            place(
                StackVisual::new(&BEACON[detail])
                    .at(Pose::at(outer(0), 0, outer(1)))
                    .with_palette(palette),
            );
        }
    }
    for bridge in harbor.bridges.into_iter().flatten() {
        let [x, z] = bridge.center().point;
        let mut pose = Pose::at(x, 0, z);
        if bridge.axis == 0 {
            pose.yaw = Fixed::from_int(90);
        }
        place(
            StackVisual::new(bridge_model(bridge, detail))
                .at(pose)
                .with_palette(palette),
        );
    }
}

fn variation(x: usize, z: usize, seed: u16) -> u32 {
    let hash = (x as u32).wrapping_mul(0x9e37_79b9)
        ^ (z as u32).wrapping_mul(0x85eb_ca6b)
        ^ u32::from(seed);
    (hash ^ (hash >> 16)).wrapping_mul(0x7feb_352d) >> 20
}
