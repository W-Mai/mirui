use super::*;

#[test]
fn spatial_index_matches_unsorted_samples_and_empty_layers() {
    let samples = [
        StackSample {
            x: 11,
            y: 7,
            color: 0,
        },
        StackSample {
            x: 2,
            y: 4,
            color: 0,
        },
        StackSample {
            x: 9,
            y: 1,
            color: 0,
        },
        StackSample {
            x: 5,
            y: 6,
            color: 0,
        },
    ];
    let level = SparseLevel::new(1, &[0, 0, 1, 4], &samples).unwrap();
    let index = level.indexed::<3>();
    for layer in 0..4 {
        assert_eq!(index.level().layer_bounds(layer), level.layer_bounds(layer));
    }
    assert_eq!(level.layer_bounds(1).unwrap()[0], [[255, 0]; 2]);
    assert_eq!(
        level.layer_bounds(2).unwrap(),
        [[[2, 2], [4, 4]], [[5, 9], [1, 6]]]
    );
    assert_eq!(level.layer_bounds(3), None);
}

#[test]
fn sparse_levels_validate_bounds_before_rendering() {
    let sample = [StackSample {
        x: 1,
        y: 0,
        color: 0,
    }];
    let palette = [[0, 0, 0, 255]];
    assert!(SparseLevel::new(1, &[0, 2, 1], &sample).is_none());
    let base = SparseLevel::new(1, &[0, 0, 1], &sample).unwrap();
    assert_eq!(base.layer(0), Some([].as_slice()));
    assert_eq!(base.layer(usize::MAX), None);
    let coarse = SparseLevel::new(2, &[0, 0, 1], &sample).unwrap();
    assert!(PreparedSpriteStack::new([2, 1, 2], [0; 3], 256, &palette, &[base]).is_some());
    assert!(PreparedSpriteStack::new([2, 1, 2], [0; 3], 256, &palette, &[base, coarse]).is_none());
    assert!(PreparedSpriteStack::new([1, 1, 2], [0; 3], 256, &palette, &[base]).is_none());
    assert!(PreparedSpriteStack::new([2, 1, 2], [0; 3], 256, &[[0; 4]], &[base]).is_none());
}
