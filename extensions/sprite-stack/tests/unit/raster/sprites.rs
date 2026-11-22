use super::*;
const CELLS: &[StackSample] = &[
    StackSample {
        x: 0,
        y: 0,
        color: 0,
    },
    StackSample {
        x: 4,
        y: 4,
        color: 1,
    },
    StackSample {
        x: 1,
        y: 3,
        color: 0,
    },
    StackSample {
        x: 3,
        y: 2,
        color: 1,
    },
];
const MIP: &[StackSample] = &[
    StackSample {
        x: 0,
        y: 0,
        color: 0,
    },
    StackSample {
        x: 2,
        y: 2,
        color: 1,
    },
];
const LEVELS: &[SparseLevel<'static>] = &[
    SparseLevel::new(1, &[0, 2, 4], CELLS).unwrap(),
    SparseLevel::new(2, &[0, 1, 2], MIP).unwrap(),
];
const MODEL: PreparedSpriteStack<'static> = PreparedSpriteStack::new(
    [6, 6, 2],
    [256, 512, 128],
    256,
    &[[0, 0, 0, 255], [255; 4]],
    LEVELS,
)
.unwrap();
fn fixture(scale: Fixed, perspective: bool) -> (Projection, ProjectedStack<'static, 2>) {
    let p = Projection {
        scale,
        center: [Fixed::from_int(64); 2],
        perspective: if perspective {
            mirui::types::Fixed64::from_ratio(1, 64)
        } else {
            mirui::types::Fixed64::ZERO
        },
        slope: Fixed::from_ratio(2, 3),
    };
    let a = Fixed::from_ratio(3, 5);
    let b = Fixed::from_ratio(4, 5);
    let transform = StackTransform {
        origin: [Fixed::ZERO; 3],
        axes: [
            [a, -b, Fixed::ZERO],
            [b, a, Fixed::ZERO],
            [Fixed::ZERO, Fixed::ZERO, Fixed::ONE],
        ],
        scale: Fixed::ONE,
        layer_spacing: Fixed::ONE,
        depth_radius: Fixed::from_int(3),
    };
    let sprite = ProjectedStack::new(&MODEL, transform, &p, Fixed::from_int(3), |_, _| {
        Some(Fixed::ZERO)
    })
    .unwrap();
    (p, sprite)
}
#[test]
fn solid_samples_keep_all_palette_indices_and_respect_scene_depth() {
    use super::super::{Frame, Workspace};
    const CELLS: &[StackSample] = &[
        StackSample {
            x: 0,
            y: 0,
            color: 0,
        },
        StackSample {
            x: 3,
            y: 0,
            color: 255,
        },
    ];
    const INDEX: crate::format::IndexedLevel<'static, 1> =
        SparseLevel::new(1, &[0, 2], CELLS).unwrap().indexed();
    const MODEL: PreparedSpriteStack<'static> =
        PreparedSpriteStack::new([4, 1, 1], [0; 3], 256, &[[255; 4]; 256], &[INDEX.level()])
            .unwrap();
    let projection = Projection {
        scale: Fixed::from_int(3),
        slope: Fixed::ZERO,
        perspective: Fixed64::ZERO,
        center: [Fixed::from_int(4), Fixed::from_int(8)],
    };
    let mut sprite = ProjectedStack::<1>::new(
        &MODEL,
        StackTransform {
            origin: [Fixed::ZERO; 3],
            axes: [
                [Fixed::ONE, Fixed::ZERO, Fixed::ZERO],
                [Fixed::ZERO, Fixed::ZERO, Fixed::ONE],
                [Fixed::ZERO, Fixed::ONE, Fixed::ZERO],
            ],
            scale: Fixed::ONE,
            layer_spacing: Fixed::ONE,
            depth_radius: Fixed::ONE,
        },
        &projection,
        Fixed::ONE,
        |_, _| Some(Fixed::ZERO),
    )
    .unwrap();
    sprite.prepare_regions(&projection);
    let mut frame = Frame::new(20, 16).unwrap();
    let mut work = Workspace::new(20, 16).unwrap();
    work.with_rows(&mut frame, 0, 16, |tile| {
        tile.clear_depth();
        let shade = |s: &StackSample| if s.color == 0 { 0xff0000ff } else { 0xff00ff00 };
        sprite.view().draw(tile, &projection, shade);
        let red = tile.color.index(4, 8);
        let green = tile.color.index(13, 8);
        assert_eq!([tile.get(red), tile.get(green)], [0xff0000ff, 0xff00ff00]);
        assert_eq!([tile.depth[red], tile.depth[green]], [Fixed::ZERO; 2]);
        tile.depth[red] = -Fixed::ONE;
        sprite.view().draw(tile, &projection, |_| u32::MAX);
        assert_eq!([tile.get(red), tile.get(green)], [0xff0000ff, u32::MAX]);
    });
}

#[test]
fn masked_surfaces_share_object_depth_and_contours_across_tiles() {
    use super::super::{Frame, Workspace};
    const VISIBLE: u32 = 0xff33_44ff;
    const OTHER: u32 = 0xffee_8833;
    const OUTLINE: u32 = 0xff10_1010;
    const SKY: u32 = 0xff11_2266;
    // The last sample shares a pixel with the first and maps to a hole.
    const SAMPLES: &[StackSample] = &[
        StackSample {
            x: 0,
            y: 0,
            color: 0,
        },
        StackSample {
            x: 2,
            y: 0,
            color: 0,
        },
        StackSample {
            x: 0,
            y: 1,
            color: 1,
        },
    ];
    const SHAPE: PreparedSpriteStack<'static> = PreparedSpriteStack::new(
        [3, 2, 1],
        [0; 3],
        256,
        &[[255; 4]; 2],
        &[SparseLevel::new(1, &[0, 3], SAMPLES).unwrap()],
    )
    .unwrap();
    let projection = Projection {
        scale: Fixed::ONE,
        slope: Fixed::ZERO,
        perspective: Fixed64::ZERO,
        center: [Fixed::from_int(5), Fixed::from_int(5)],
    };
    let sprite = |start: i32, slope: i32| {
        ProjectedStack::<1>::new(
            &SHAPE,
            StackTransform {
                origin: [Fixed::ZERO, Fixed::from_int(start), Fixed::ZERO],
                axes: [
                    [Fixed::ONE, Fixed::from_int(slope), Fixed::ZERO],
                    [Fixed::ZERO; 3],
                    [Fixed::ZERO, Fixed::ZERO, Fixed::ONE],
                ],
                scale: Fixed::ONE,
                layer_spacing: Fixed::ONE,
                depth_radius: Fixed::ONE,
            },
            &projection,
            Fixed::ONE,
            |_, _| Some(Fixed::ZERO),
        )
        .unwrap()
    };
    let a = sprite(3, -2);
    let b = sprite(1, 2);
    let render = |rows: usize, reverse: bool| {
        let mut frame = Frame::new(12, 12).unwrap();
        let mut work = Workspace::new(12, rows).unwrap();
        let mut pixels = [(0u32, Fixed::MAX); 144];
        work.render_rows(&mut frame, |tile| {
            tile.clear_depth();
            for at in 0..tile.depth.len() {
                tile.put(at, SKY);
            }
            // At the outline this third object is nearer. A further opaque
            // fragment drawn after the outline will also have to pass depth.
            tile.write_visible(5, 4, Fixed::from_int(-3), SKY);
            let draw = |tile: &mut SceneCanvas<'_, '_>, s: &ProjectedStack<'_, 1>, color: u32| {
                s.view().draw_masked(
                    tile,
                    &projection,
                    SpritePaint {
                        depth_bias: Fixed::ZERO,
                        opacity: Fixed::ONE,
                        outline: Some(OUTLINE),
                    },
                    |v| u8::from(v.color == 0),
                    |_| color,
                );
            };
            if reverse {
                draw(tile, &b, OTHER);
                draw(tile, &a, VISIBLE);
            } else {
                draw(tile, &a, VISIBLE);
                draw(tile, &b, OTHER);
            }
            tile.write_visible(7, 6, Fixed::from_int(20), SKY);
            let [l, t, r, b] = tile.color.clip;
            for y in t..b {
                for x in l..r {
                    let at = tile.color.index(x, y);
                    pixels[y as usize * 12 + x as usize] = (tile.get(at), tile.depth[at]);
                }
            }
            Ok(())
        })
        .unwrap();
        pixels
    };
    let expected = render(12, false);
    assert_eq!(expected, render(12, true));
    assert_eq!(expected, render(1, false));
    assert_eq!(expected, render(3, true));
    let pixel = |x: usize, y: usize| expected[y * 12 + x];
    assert_eq!(pixel(5, 5), (OTHER, Fixed::ONE));
    assert_eq!(pixel(7, 5), (VISIBLE, -Fixed::ONE));
    assert_eq!(pixel(5, 4), (SKY, Fixed::from_int(-3)));
    assert_eq!(pixel(7, 6), (OUTLINE, -Fixed::ONE));
    assert_eq!(pixel(8, 8), (SKY, Fixed::MAX));

    // A visible, fading surface blends once even on a one-row tile.
    let mut frame = Frame::new(12, 12).unwrap();
    let mut work = Workspace::new(12, 1).unwrap();
    work.with_rows(&mut frame, 5, 1, |tile| {
        tile.clear_depth();
        let at = tile.color.index(5, 5);
        tile.put(at, 0xff00_0000);
        for opacity in [Fixed::ZERO, Fixed::HALF] {
            a.view().draw_masked(
                tile,
                &projection,
                SpritePaint {
                    depth_bias: Fixed::ONE,
                    opacity,
                    outline: None,
                },
                |v| u8::from(v.color == 0),
                |_| u32::MAX,
            );
            if opacity == Fixed::ZERO {
                assert_eq!((tile.get(at), tile.depth[at]), (0xff00_0000, Fixed::MAX));
            }
        }
        assert_eq!(tile.depth[at], Fixed::from_int(2));
        assert_eq!(tile.get(at), 0xff7f_7f7f);
        tile.clear_depth();
        tile.put(at, 0);
        a.view().draw_masked(
            tile,
            &projection,
            SpritePaint {
                depth_bias: Fixed::ZERO,
                opacity: Fixed::HALF,
                outline: None,
            },
            |v| u8::from(v.color == 0),
            |_| 0xff33_22ff,
        );
        assert_eq!(tile.get(at), 0x8033_22ff);
    });
}

#[test]
fn prepared_regions_cover_all_samples() {
    assert!(core::mem::size_of::<PreparedLayer>() <= 36);
    let mut details = [false; 2];
    for scale in [Fixed::HALF, Fixed::from_int(3), Fixed::from_int(8)] {
        for perspective in [false, true] {
            let (projection, mut prepared) = fixture(scale, perspective);
            details[usize::from(prepared.level().step() > 1)] = true;
            assert!(prepared.layers.iter().flatten().all(|l| l.bounds.is_none()));
            let original = prepared.layers;
            prepared.prepare_regions(&projection);
            for (index, layer) in original.iter().enumerate() {
                let Some(layer) = layer else { continue };
                let voxels = prepared.level().layer(index).unwrap();
                for (i, voxel) in voxels.iter().enumerate() {
                    let Some([x, y, scale, _]) = prepared.core.project(layer, voxel, &projection)
                    else {
                        continue;
                    };
                    let bounds = prepared.layers[index]
                        .expect("visible layer")
                        .bounds
                        .expect("prepared layer")[usize::from(i >= voxels.len() / 2)]
                    .map(i32::from);
                    for offset in &prepared.offsets[..prepared.core.samples * prepared.core.samples]
                    {
                        for (axis, position) in [x, y].into_iter().enumerate() {
                            let at = (position + offset[axis] * scale / prepared.core.center[2])
                                .round()
                                .trunc_to_int();
                            assert!(at >= bounds[axis] && at < bounds[axis + 2]);
                        }
                    }
                }
            }
        }
    }
    assert!(details.into_iter().all(|seen| seen));
}
#[test]
fn near_plane_rejects_only_wholly_hidden_model_bands() {
    let (mut p, mut jet) = fixture(Fixed::ONE, true);
    p.perspective = mirui::types::Fixed64::from_ratio(1, 64);
    jet.core.grid = GridBasis::new([Fixed::ZERO, Fixed::ONE, Fixed::ZERO], [Fixed::ZERO; 3]);
    let coords = [[0, 16], [0; 2]];
    let bounds = |depth| {
        let layer = PreparedLayer {
            camera: [Fixed::ZERO, Fixed::from_int(depth - 8), Fixed::ZERO],
            drop: Fixed::ZERO,
            bounds: None,
        };
        jet.core.project_band(&layer, &p, coords)
    };
    assert_eq!(bounds(-80), ProjectionWork::EMPTY_BAND);
    assert_eq!(bounds(-51), [i16::MIN, i16::MIN, i16::MAX, i16::MAX]);
    let [left, top, right, bottom] = bounds(0);
    assert!(left < right && top < bottom);
    assert!(left > i16::MIN && right < i16::MAX);
}
#[test]
fn visible_cells_survive_a_hidden_pivot_and_a_wide_virtual_viewport() {
    use super::super::{Frame, Workspace};
    const SAMPLE: &[StackSample] = &[StackSample {
        x: 24,
        y: 0,
        color: 0,
    }];
    const MODEL: PreparedSpriteStack<'static> = PreparedSpriteStack::new(
        [25, 1, 1],
        [0; 3],
        256,
        &[[255; 4]],
        &[SparseLevel::new(1, &[0, 1], SAMPLE).unwrap()],
    )
    .unwrap();
    let p = Projection {
        center: [Fixed::from_int(40_000), Fixed::from_int(8)],
        scale: Fixed::ONE,
        perspective: Fixed64::from_ratio(1, 64),
        slope: Fixed::ZERO,
    };
    let t = StackTransform {
        origin: [Fixed::ZERO, -Fixed::from_int(65), Fixed::ZERO],
        axes: [
            [Fixed::ZERO, Fixed::ONE, Fixed::ZERO],
            [Fixed::ONE, Fixed::ZERO, Fixed::ZERO],
            [Fixed::ZERO, Fixed::ZERO, Fixed::ONE],
        ],
        scale: Fixed::ONE,
        layer_spacing: Fixed::ONE,
        depth_radius: Fixed::from_int(25),
    };
    let mut sprite =
        ProjectedStack::<1>::new(&MODEL, t, &p, Fixed::ONE, |_, _| Some(Fixed::ZERO)).unwrap();
    sprite.prepare_regions(&p);
    let mut frame = Frame::new(16, 16).unwrap();
    let mut work = Workspace::new(16, 16).unwrap();
    work.with_region(
        &mut frame,
        [50_000, 16],
        [39_992, 0],
        [39_992, 0, 40_008, 16],
        |tile| {
            tile.clear_depth();
            sprite.view().draw(tile, &p, |_| u32::MAX);
            assert_eq!(tile.get(tile.color.index(40_000, 8)), u32::MAX);
        },
    );
}
#[test]
fn sample_bounds_never_reject_a_sample_inside_the_region() {
    for perspective in [false, true] {
        let (_, prepared) = fixture(Fixed::from_int(3), perspective);
        for scale in [1, 63, 256, 512, 1024] {
            let scale = Fixed::from_ratio(scale, 256);
            for axis in 0..2 {
                for raw in (-32 * 256..160 * 256).step_by(67) {
                    let position = Fixed::from_ratio(raw, 256);
                    for clip in [[0, 0, 32, 32], [31, 47, 63, 79], [127, 127, 128, 128]] {
                        let ends = prepared.core.sample_ends(position, axis, scale);
                        if ends[0] == ends[1] {
                            for offset in
                                &prepared.offsets[..prepared.core.samples * prepared.core.samples]
                            {
                                assert_eq!(
                                    (position + offset[axis] * scale / prepared.core.center[2])
                                        .round()
                                        .trunc_to_int(),
                                    ends[0]
                                );
                            }
                        }
                        if ends[0].max(ends[1]) < clip[axis]
                            || ends[0].min(ends[1]) >= clip[axis + 2]
                        {
                            assert!(
                                prepared.offsets[..prepared.core.samples * prepared.core.samples]
                                    .iter()
                                    .all(|offset| {
                                        let at = (position
                                            + offset[axis] * scale / prepared.core.center[2])
                                            .round()
                                            .trunc_to_int();
                                        at < clip[axis] || at >= clip[axis + 2]
                                    })
                            );
                        }
                    }
                }
            }
        }
    }
}
