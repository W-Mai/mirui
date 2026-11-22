#![cfg(all(feature = "widget", not(target_os = "none")))]

use mirui::{
    core::{cache::Handle, model::ModelList},
    prelude::*,
    render::{RenderError, texture::ColorFormat},
    surface::framebuf::FramebufSurface,
};
use mirui_sprite_stack::{
    Camera, ContentFrame, FrameContent, FrameSize, GeneratedLevel, GeneratedStackAsset, Geometry,
    HeightField, HeightFieldHandle, LayerEntrance, Pose, PreparedSpriteStack, SampledStyle,
    SparseLevel, SpriteStackPlugin, StackSample, StackStage, StackStageCommands, StackViewport,
    StackVisual, TerrainCell, TerrainGrid, TerrainTarget, ViewportContent, ViewportOutput,
    WorldCamera,
    raster::{
        FrameCamera, FrameEffects, Projection, SceneCanvas, SceneLayers, sprites::SpritePaint,
    },
};
use std::{cell::Cell, rc::Rc};

#[path = "support/allocations.rs"]
mod allocations;

const RED: Color = Color::rgb(255, 0, 0);
const BLACK: u32 = u32::from_le_bytes([0, 0, 0, 255]);

#[mirui::component]
struct PrimaryGround;

#[mirui::component]
struct ConflictingGround;

#[mirui::component]
struct OrdinaryMember;

#[mirui::model]
struct GroundRole {
    #[observe]
    slot: u8,
}

#[mirui::model]
impl GroundRole {
    fn choose(&mut self, slot: u8) {
        self.slot = slot;
    }
}

#[mirui::system]
fn mount_ground(
    stage: model!(StackStage),
    role: model!(GroundRole),
    field: res!(Handle<HeightField>),
    commands: commands!(),
) {
    if stage.is_empty() {
        let visual = |slot, x| {
            let field = field.clone();
            StackVisual::from_model(role, move |model| {
                let visual = StackVisual::new(field.clone()).at(Pose::at(x, 0, 1));
                if model.slot() == slot {
                    visual.with_role::<PrimaryGround>()
                } else {
                    visual
                }
            })
        };
        let mut stage = commands.stage(stage);
        stage.spawn(PrimaryGround, visual(0, -7));
        stage.spawn(OrdinaryMember, visual(1, 3));
    }
}

#[mirui::system]
fn conflict(
    stage: model!(StackStage),
    field: res!(Handle<HeightField>),
    add: query!(Entity, PrimaryGround),
    existing: query!(Entity, ConflictingGround),
    commands: commands!(),
) {
    if add.into_iter().next().is_some() && existing.into_iter().next().is_none() {
        commands.stage(stage).spawn(
            (PrimaryGround, ConflictingGround),
            StackVisual::new(field.clone())
                .at(Pose::at(0, 0, 4))
                .with_role::<PrimaryGround>(),
        );
    }
}

#[mirui::system]
fn retire_ground(members: query!(Entity, PrimaryGround), commands: commands!()) {
    for (entity, _) in members {
        commands.despawn(entity);
    }
}

struct Sky;

impl SceneLayers for Sky {
    fn background(&self, tile: &mut SceneCanvas<'_, '_>) {
        tile.paint_background(|_, _| (), |_, _| (), |_, _, _, _| BLACK);
    }
}

struct Content {
    output: ViewportOutput,
    terrain: Option<Rc<Cell<Option<Entity>>>>,
}

impl FrameContent for Content {
    fn camera(&self, _: &(), size: FrameSize) -> Result<WorldCamera, RenderError> {
        let center = size.pixels().map(|side| Fixed::from(side) / 2);
        let (position, pitch, scale) = if self.terrain.is_some() {
            (
                [Fixed::ZERO, 16.into(), Fixed::ZERO],
                Fixed::PI / 2,
                2.into(),
            )
        } else {
            (
                [Fixed::ZERO, Fixed::ZERO, (-8).into()],
                Fixed::ZERO,
                4.into(),
            )
        };
        Ok(WorldCamera {
            camera: FrameCamera {
                projection: Projection {
                    perspective: Default::default(),
                    slope: Fixed::ZERO,
                    scale,
                    center,
                },
                position,
                yaw: Fixed::ZERO,
                pitch,
                roll: Fixed::ZERO,
            },
            pivot: center[1],
            roll: false,
        })
    }

    fn output(&self) -> ViewportOutput {
        self.output
    }

    fn terrain(&self) -> Option<TerrainTarget> {
        self.terrain
            .as_ref()
            .map(|_| TerrainTarget::role::<PrimaryGround>())
    }

    fn prepare(
        &mut self,
        _: &(),
        frame: ContentFrame,
    ) -> Result<(impl SceneLayers, impl FrameEffects), RenderError> {
        if let Some(selected) = &self.terrain {
            selected.set(frame.terrain.map(|member| member.entity));
        }
        Ok((Sky, ()))
    }
}

#[mirui::compose]
fn ground_screens(stage: model!(StackStage), content: ViewportContent) -> Entity {
    ui! {
        Row {
            StackViewport(width: 32, height: 24, stage: stage.clone()) [content]
            StackViewport(width: 32, height: 24, stage: stage, camera: Camera {
                center: [Fixed::ZERO, Fixed::from_int(16), Fixed::ZERO],
                scale: Fixed::from_int(2),
                ..Camera::overhead()
            })
        }
    }
}

struct GroundSource(Signal<HeightFieldHandle>);

#[mirui::system]
fn mount_live_ground(
    stage: model!(StackStage),
    source: res!(GroundSource),
    owner: model!(GroundRole),
    commands: commands!(),
) {
    if stage.is_empty() {
        let mut stage = commands.stage(stage);
        let signal = source.0.clone();
        stage.spawn(
            OrdinaryMember,
            StackVisual::new(signal.get()).at(Pose::at(3, 0, 1)),
        );
        stage.spawn(
            PrimaryGround,
            StackVisual::from_model(owner, move |_| {
                StackVisual::new(signal.get())
                    .at(Pose::at(-7, 0, 1))
                    .with_role::<PrimaryGround>()
            }),
        );
    }
}

#[test]
fn retired_height_field_clears_role_and_ordinary_view_in_both_output_modes() {
    const COLORS: [Color; 1] = [RED];
    for output in [ViewportOutput::Full, ViewportOutput::Bounded] {
        let make_field = || {
            HeightField::generate(
                TerrainGrid::finite([1; 2], 4.into()).unwrap(),
                &COLORS,
                |_| TerrainCell {
                    level: 0,
                    material: 0,
                },
            )
            .unwrap()
        };
        let mut app = App::headless(64, 24);
        app.with_default_widgets().add_plugin(SpriteStackPlugin);
        let stage = app.add_model(StackStage::bounded(2)).finish();
        let mut fields = ModelList::bounded(2);
        assert!(fields.try_push(make_field()).is_ok());
        assert!(fields.try_push(make_field()).is_ok());
        let fields = app.add_model(fields).finish();
        let mut entries = fields.entries();
        let (old, current) = entries.next().unwrap();
        let (_, replacement) = entries.next().unwrap();
        drop(entries);
        let source = Signal::new(current);
        app.add_resource(GroundSource(source.clone()));
        let owner = app.add_model(GroundRole { slot: 0 }).finish();
        app.add_system(mount_live_ground::system(stage.clone(), owner));
        let chosen = Rc::new(Cell::new(None));
        let root = app.spawn_root().id();
        app.compose(
            root,
            ground_screens(
                stage.clone(),
                ViewportContent::new(Content {
                    output,
                    terrain: Some(chosen.clone()),
                }),
            ),
        );
        app.tick();
        let count = |app: &mut App<_, _>| {
            let image = app.snapshot().unwrap().unwrap();
            let pixels = image.read_pixels().unwrap();
            [0..16, 16..32, 32..48, 48..64].map(|xs| {
                (0..24)
                    .flat_map(|y| xs.clone().map(move |x| [x, y]))
                    .filter(|&[x, y]| pixels.get_pixel(x, y) == RED)
                    .count()
            })
        };
        assert_eq!(count(&mut app), [0, 64, 64, 64]);
        let terrain = chosen.get().unwrap();
        assert!(fields.remove(old).unwrap().is_some());
        assert_eq!(count(&mut app), [0; 4]);
        assert_eq!(chosen.get(), None);
        source.set(replacement);
        assert_eq!(count(&mut app), [0, 0, 64, 0]);
        assert_eq!(chosen.get(), Some(terrain));
        assert_eq!(stage.len(), 2);
    }
}

#[test]
fn one_view_replaces_only_the_selected_entity_of_a_shared_field() {
    let mut app = App::headless(64, 24);
    app.with_default_widgets().add_plugin(SpriteStackPlugin);
    let stage = app.add_model(StackStage::bounded(3)).finish();
    app.add_resource(Handle::new(
        HeightField::generate(
            TerrainGrid::finite([1; 2], 4.into()).unwrap(),
            &[RED],
            |_| TerrainCell {
                level: 0,
                material: 0,
            },
        )
        .unwrap(),
    ));
    let role = app.add_model(GroundRole { slot: 0 }).finish();
    let selected = Rc::new(Cell::new(None));
    let root = app.spawn_root().id();
    app.compose(
        root,
        ground_screens(
            stage.clone(),
            ViewportContent::new(Content {
                output: ViewportOutput::Bounded,
                terrain: Some(selected.clone()),
            }),
        ),
    );
    app.add_system(mount_ground::system(stage.clone(), role.clone()));
    app.tick();
    let counts = |app: &mut App<_, _>| {
        let image = app.snapshot().unwrap().unwrap();
        let pixels = image.read_pixels().unwrap();
        [0..16, 16..32, 32..48, 48..64].map(|xs| {
            (0..24)
                .flat_map(|y| xs.clone().map(move |x| [x, y]))
                .filter(|&[x, y]| pixels.get_pixel(x, y) == RED)
                .count()
        })
    };
    assert_eq!(counts(&mut app), [0, 64, 64, 64]);
    let first = selected.get().unwrap();

    // A role-only publication updates the viewport without running any systems.
    role.choose(1);
    assert_eq!(counts(&mut app), [64, 0, 64, 64]);
    let second = selected.get().unwrap();
    assert_ne!(second, first);
    assert_eq!(stage.len(), 2);
    role.choose(2);
    assert_eq!(counts(&mut app), [64, 64, 64, 64]);
    assert!(selected.get().is_none());
    role.choose(0);
    assert_eq!(counts(&mut app), [0, 64, 64, 64]);
    assert_eq!(selected.get(), Some(first));

    app.add_system(conflict::system(stage.clone()));
    app.tick();
    assert!(matches!(app.snapshot(), Err(RenderError::InvalidGeometry)));
    app.add_system(retire_ground::system());
    app.tick();
    let image = app.snapshot().unwrap().unwrap();
    let pixels = image.read_pixels().unwrap();
    assert!(selected.get().is_none());
    assert!((16..32).any(|x| (0..24).any(|y| pixels.get_pixel(x, y) == RED)));
}

const CELLS: &[StackSample] = &[
    StackSample {
        x: 0,
        y: 0,
        color: 0,
    },
    StackSample {
        x: 1,
        y: 0,
        color: 255,
    },
    StackSample {
        x: 2,
        y: 0,
        color: 0,
    },
    StackSample {
        x: 3,
        y: 0,
        color: 255,
    },
];
const LEVELS: &[SparseLevel<'static>] = &[SparseLevel::new(1, &[0, 4], CELLS).unwrap()];
const WIDE: PreparedSpriteStack<'static> =
    PreparedSpriteStack::new([4, 1, 1], [512, 0, 0], 256, &[[255; 4]; 256], LEVELS).unwrap();
const LEFT: [u32; 256] = {
    let mut palette = [BLACK; 256];
    palette[0] = u32::from_le_bytes([255, 0, 0, 255]);
    palette[255] = u32::from_le_bytes([0, 255, 0, 255]);
    palette
};
const RIGHT: [u32; 256] = {
    let mut palette = [BLACK; 256];
    palette[0] = u32::from_le_bytes([0, 0, 255, 255]);
    palette[255] = u32::from_le_bytes([255, 255, 0, 255]);
    palette
};

#[mirui::model]
struct WidePaint {
    #[observe]
    opacity: Fixed,
    #[observe]
    generated: Option<GeneratedStackAsset>,
    #[observe]
    flash: Fixed,
    #[observe]
    recolor: bool,
}

#[mirui::model]
impl WidePaint {
    fn set_opacity(&mut self, opacity: Fixed) {
        self.opacity = opacity;
    }

    fn set_generated(&mut self, generated: Option<GeneratedStackAsset>) {
        self.generated = generated;
    }

    fn set_material(&mut self, flash: Fixed, recolor: bool) {
        self.flash = flash;
        self.recolor = recolor;
    }
}

const RECOLOR: [[u8; 4]; 256] = [[0, 255, 255, 0]; 256];

#[mirui::system]
fn mount_wide(stage: model!(StackStage), paint: model!(WidePaint), commands: commands!()) {
    if stage.is_empty() {
        let [o, u] = [Fixed::ZERO, Fixed::ONE];
        commands.stage(stage).spawn(
            OrdinaryMember,
            StackVisual::from_model(paint, move |paint| {
                let geometry = paint
                    .generated()
                    .map_or(Geometry::Sampled(&WIDE), Geometry::from);
                let visual = StackVisual::new(geometry).with_flash(paint.flash());
                let visual = if paint.recolor() {
                    visual.with_palette(&RECOLOR)
                } else {
                    visual
                };
                visual.with_sampled_style(SampledStyle {
                    axes: [[u, o, o], [o, o, u], [o, u, o]],
                    scale: u,
                    layer_spacing: u,
                    depth_radius: u,
                    max_cell_pixels: 4.into(),
                    cull_margin: o,
                    entrance: LayerEntrance {
                        elapsed: u,
                        delay_per_layer: o,
                        duration: u,
                        viewport_height_scale: o,
                        distance: o,
                    },
                    paint: SpritePaint {
                        depth_bias: o,
                        opacity: paint.opacity(),
                        outline: Some(u32::MAX),
                    },
                    palettes: [&LEFT, &RIGHT],
                })
            }),
        );
    }
}

#[mirui::compose]
fn sampled_screen(stage: model!(StackStage), content: ViewportContent) -> Entity {
    ui! {
        StackViewport(width: 64, height: 48, stage: stage) [content]
    }
}

fn sampled_pixels(output: ViewportOutput, format: ColorFormat) -> Vec<Color> {
    let surface = FramebufSurface::with_format(64, 48, format, |_, _| {});
    let mut app = App::new(surface);
    let quantized = match (output, format) {
        (ViewportOutput::Bounded, ColorFormat::RGBA8888) => ColorFormat::RGB565,
        _ => format,
    };
    app.with_default_widgets().add_plugin(SpriteStackPlugin);
    let stage = app.add_model(StackStage::bounded(1)).finish();
    let paint = app
        .add_model(WidePaint {
            opacity: Fixed::ONE,
            generated: None,
            flash: Fixed::ZERO,
            recolor: false,
        })
        .finish();
    let root = app.spawn_root().id();
    app.compose(
        root,
        sampled_screen(
            stage.clone(),
            ViewportContent::new(Content {
                output,
                terrain: None,
            }),
        ),
    );
    app.add_system(mount_wide::system(stage.clone(), paint.clone()));
    app.tick();
    app.render_dirty().unwrap();
    paint.set_opacity(Fixed::HALF);
    app.render_dirty().unwrap();
    let faded = app.snapshot().unwrap().unwrap();
    let update_allocations = allocations::tracked_allocations(|| {
        paint.set_opacity(Fixed::ONE);
        app.render_dirty().unwrap();
    });
    assert_eq!(update_allocations, 0);
    let image = app.snapshot().unwrap().unwrap();
    let pixels = image.read_pixels().unwrap();
    assert_ne!(faded.read_pixels().unwrap().bytes(), pixels.bytes());
    let revision = stage.revision();
    let incompatible = GeneratedStackAsset::new(
        WIDE.extent(),
        WIDE.pivot_q8(),
        WIDE.spacing_q8(),
        vec![[255; 4]],
        vec![GeneratedLevel::new(
            1,
            vec![0, 4],
            CELLS
                .iter()
                .map(|cell| StackSample { color: 0, ..*cell })
                .collect(),
        )],
        usize::MAX,
    )
    .unwrap();
    paint.set_generated(Some(incompatible));
    app.render_dirty().unwrap();
    assert_eq!(stage.revision(), revision);

    let generated = GeneratedStackAsset::new(
        WIDE.extent(),
        WIDE.pivot_q8(),
        WIDE.spacing_q8(),
        WIDE.palette().to_vec(),
        vec![GeneratedLevel::new(1, vec![0, 4], CELLS.to_vec())],
        usize::MAX,
    )
    .unwrap();
    let replace_allocations = allocations::tracked_allocations(|| {
        paint.set_generated(Some(generated));
        app.render_dirty().unwrap();
    });
    assert_eq!(replace_allocations, 0);
    assert_eq!(stage.revision(), revision + 1);
    let generated = app.snapshot().unwrap().unwrap();
    assert_eq!(generated.read_pixels().unwrap().bytes(), pixels.bytes());

    let shade_allocations = allocations::tracked_allocations(|| {
        paint.set_material(Fixed::HALF, false);
        app.render_dirty().unwrap();
    });
    assert_eq!(shade_allocations, 0);
    let flashed = app.snapshot().unwrap().unwrap();
    let flashed = flashed.read_pixels().unwrap();
    let mut counts = [0; 2];
    for y in 0..48 {
        for x in 0..64 {
            let old = pixels.get_pixel(x, y);
            if old == RED || old == Color::rgb(0, 0, 255) {
                let want = Color::lerp(old, Color::rgb(255, 255, 255), Fixed::HALF);
                assert_eq!(
                    quantized.pack(&flashed.get_pixel(x, y)),
                    quantized.pack(&want)
                );
                counts[0] += 1;
            } else if old == Color::rgb(255, 255, 255) {
                assert_eq!(flashed.get_pixel(x, y), old);
                counts[1] += 1;
            }
        }
    }
    assert!(counts.into_iter().all(|count| count > 0));
    paint.set_material(Fixed::ZERO, true);
    app.render_dirty().unwrap();
    let tinted = app.snapshot().unwrap().unwrap();
    let tinted = tinted.read_pixels().unwrap();
    for y in 0..48 {
        for x in 0..64 {
            let old = pixels.get_pixel(x, y);
            let covered = [
                RED,
                Color::rgb(0, 0, 255),
                Color::rgb(0, 255, 0),
                Color::rgb(255, 255, 0),
            ]
            .contains(&old);
            let want = if covered {
                Color::rgb(0, 255, 255)
            } else {
                old
            };
            assert_eq!(tinted.get_pixel(x, y), want);
        }
    }
    let restore_allocations = allocations::tracked_allocations(|| {
        paint.set_generated(None);
        paint.set_material(Fixed::ZERO, false);
        app.render_dirty().unwrap();
    });
    assert_eq!(restore_allocations, 0);
    let restored = app.snapshot().unwrap().unwrap();
    assert_eq!(restored.read_pixels().unwrap().bytes(), pixels.bytes());
    let pixels = &pixels;
    (0..48)
        .flat_map(|y| (0..64).map(move |x| pixels.get_pixel(x, y)))
        .collect()
}

#[test]
fn sampled_raster_preserves_both_banks_and_reuses_its_bound_appearance() {
    eprintln!("StackVisual: {} bytes", core::mem::size_of::<StackVisual>());
    check_sampled_raster(ColorFormat::RGBA8888);
    check_sampled_raster(ColorFormat::RGB565Swapped);
}

fn check_sampled_raster(format: ColorFormat) {
    let full = sampled_pixels(ViewportOutput::Full, format);
    let bounded = sampled_pixels(ViewportOutput::Bounded, format);
    assert_eq!(full, bounded);
    for color in [
        RED,
        Color::rgb(0, 255, 0),
        Color::rgb(0, 0, 255),
        Color::rgb(255, 255, 0),
    ] {
        assert!(full.contains(&color), "{color:?}");
    }
}
