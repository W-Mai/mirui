use super::*;
use crate::{
    SparseLevel, StackSample,
    raster::{Frame, Workspace},
};
use mirui::types::Fixed64;

const RED: u32 = 0xff00_00ff;
const GREEN: u32 = 0xff00_ff00;
const ROW: [StackSample; 3] = [
    StackSample {
        x: 0,
        y: 0,
        color: 0,
    },
    StackSample {
        x: 1,
        y: 0,
        color: 0,
    },
    StackSample {
        x: 2,
        y: 0,
        color: 0,
    },
];
const BLOCK: [StackSample; 9] = [
    ROW[0], ROW[1], ROW[2], ROW[0], ROW[1], ROW[2], ROW[0], ROW[1], ROW[2],
];
const OPEN: [StackSample; 8] = [
    ROW[0], ROW[1], ROW[2], ROW[0], ROW[2], ROW[0], ROW[1], ROW[2],
];

fn camera(perspective: Fixed64) -> Projection {
    Projection {
        perspective,
        slope: Fixed::ZERO,
        scale: Fixed::from_int(4),
        center: [Fixed::from_int(12); 2],
    }
}

fn pose() -> StackTransform {
    let zero = Fixed::ZERO;
    let one = Fixed::ONE;
    StackTransform::from_basis(
        [zero; 3],
        [[one, zero, zero], [zero, one, zero], [zero, zero, one]],
        |v| v,
    )
}

#[test]
fn side_faces_fill_the_space_between_slices_and_share_depth() {
    let levels = [SparseLevel::new(1, &[0, 3, 6, 9], &BLOCK).unwrap()];
    let source =
        PreparedSpriteStack::new([3, 1, 3], [256, 0, 256], 256, &[[255, 0, 0, 255]], &levels)
            .unwrap();
    assert!(matches!(
        SolidStack::new(&source, 23),
        Err(RenderError::InsufficientWorkspace {
            required_bytes: 24,
            capacity_bytes: 23
        })
    ));
    let stack = SolidStack::new(&source, 24).unwrap();
    let mut frame = Frame::new(24, 24).unwrap();
    let mut workspace = Workspace::new(24, 24).unwrap();

    workspace.with_canvas(&mut frame, |tile| {
        tile.clear_depth();
        for y in 0..24 {
            for x in 0..24 {
                tile.write_visible(x, y, Fixed::ONE, GREEN);
            }
        }
        tile.write_visible(12, 12, -Fixed::ONE, GREEN);
        stack.draw(tile, &camera(Fixed64::ZERO), &pose(), |rgba, _| {
            u32::from_le_bytes(rgba)
        });
        for y in 6..18 {
            for x in 6..18 {
                let pixel = tile.get(tile.color.index(x, y));
                if (x, y) == (12, 12) {
                    assert_eq!(pixel, GREEN);
                } else {
                    assert_eq!(pixel, RED, "uncovered voxel at {x},{y}");
                    assert_eq!(tile.sample_depth(x, y), Some(-Fixed::HALF));
                }
            }
        }
        assert_eq!(tile.get(tile.color.index(5, 12)), GREEN);
    });
}

#[test]
fn perspective_preserves_an_intentional_open_cell() {
    let levels = [SparseLevel::new(1, &[0, 3, 5, 8], &OPEN).unwrap()];
    let source =
        PreparedSpriteStack::new([3, 1, 3], [256, 0, 256], 256, &[[255, 0, 0, 255]], &levels)
            .unwrap();
    let stack = SolidStack::new(&source, 24).unwrap();
    let mut frame = Frame::new(24, 24).unwrap();
    let mut workspace = Workspace::new(24, 24).unwrap();

    workspace.with_canvas(&mut frame, |tile| {
        tile.clear_depth();
        stack.draw(tile, &camera(Fixed64::ONE / 16), &pose(), |rgba, _| {
            u32::from_le_bytes(rgba)
        });
        for (x, y) in [(12, 8), (8, 12), (16, 12), (12, 16)] {
            assert_eq!(tile.get(tile.color.index(x, y)), RED);
            assert!(tile.sample_depth(x, y).unwrap() < Fixed::ZERO);
        }
        assert_eq!(tile.sample_depth(12, 12), Some(Fixed::MAX));
    });
}
