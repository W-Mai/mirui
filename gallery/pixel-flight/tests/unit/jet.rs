use super::*;
#[test]
fn coarse_slices_partition_occupied_cells_without_allocating() {
    assert_eq!(WIDTH % 2, 0);
    assert_eq!(LENGTH % 2, 0);
    assert!(COARSE_VOXELS.len() < VOXELS.len() / 3);
    let mut cells = [0; WIDTH * LENGTH * LAYERS];
    decode(&mut cells);
    let allocations = crate::test_support::tracked_allocations(|| {
        for layer in 0..LAYERS {
            let slice = MODEL.levels()[1].layer(layer).unwrap();
            let mut represented = [false; WIDTH * LENGTH / 4];
            for voxel in slice {
                let (x, y) = (usize::from(voxel.x), usize::from(voxel.y));
                assert!(x < WIDTH && y < LENGTH && x % 2 == 0 && y % 2 == 0);
                assert!(PALETTE.iter().any(|p| p[0] == voxel.color));
                let at = y / 2 * (WIDTH / 2) + x / 2;
                assert!(!represented[at]);
                represented[at] = true;
                assert!(
                    (0..4).any(
                        |i| cells[layer * WIDTH * LENGTH + (y + i / 2) * WIDTH + x + i % 2] != 0
                    )
                );
            }
            for voxel in MODEL.levels()[0].layer(layer).unwrap() {
                assert!(
                    represented[usize::from(voxel.y) / 2 * (WIDTH / 2) + usize::from(voxel.x) / 2]
                );
            }
            assert_eq!(
                slice.is_empty(),
                MODEL.levels()[0].layer(layer).unwrap().is_empty()
            );
        }
    });
    assert_eq!(allocations, 0);
}

#[test]
fn detail_uses_the_projected_cell_bound() {
    for raw in -256..=512 {
        let scale = Fixed::from_ratio(raw, 256);
        let level =
            mirui_sprite_stack::raster::sprites::select_level(&MODEL, scale, Fixed::from_int(3));
        assert_eq!(
            level.step(),
            if scale > Fixed::ZERO && scale <= Fixed::from_ratio(3, 2) {
                2
            } else {
                1
            }
        );
    }
}

#[test]
fn slices_have_complete_valid_palette_coverage() {
    let mut cells = [0; WIDTH * LENGTH * LAYERS];
    decode(&mut cells);
    for i in 0..WIDTH * LENGTH {
        let top = (0..LAYERS)
            .rev()
            .map(|layer| cells[layer * WIDTH * LENGTH + i])
            .find(|value| *value != 0);
        assert_eq!(PROJECTIONS.shadow[i], u8::from(top.is_some()));
        assert_eq!(
            PROJECTIONS.top[i],
            top.map_or(0, |c| 1 + ((c - 1) >> 4).min(2))
        );
    }
    let allocated = crate::test_support::tracked_allocations(|| {
        for (layer, voxels) in slices().iter().enumerate() {
            for voxel in *voxels {
                let i =
                    layer * WIDTH * LENGTH + usize::from(voxel.y) * WIDTH + usize::from(voxel.x);
                assert_eq!(voxel.color, cells[i]);
            }
            assert_eq!(
                voxels.len(),
                cells[layer * WIDTH * LENGTH..(layer + 1) * WIDTH * LENGTH]
                    .iter()
                    .filter(|c| **c != 0)
                    .count()
            );
        }
    });
    assert_eq!(allocated, 0);
    assert!(
        cells
            .iter()
            .all(|c| *c == 0 || PALETTE.iter().any(|p| p[0] == *c))
    );
    assert!(cells.iter().any(|c| *c != 0));
    assert!(cells.contains(&0));
}
