#![cfg(feature = "widget")]

use mirui::{
    core::model::{ModelHandle, ModelList},
    prelude::*,
    render::RenderError,
};
use mirui_sprite_stack::{
    Camera, ContentFrame, FrameContent, FrameSize, HeightField, Pose, SpriteStackPlugin,
    StackStage, StackStageCommands, StackViewport, StackVisual, TerrainCell, TerrainGrid,
    ViewportContent, ViewportOutput, WorldCamera,
    raster::{
        Canvas, DepthQuery, FrameCamera, FrameEffects, Projection, RollMapping, SceneCanvas,
        SceneLayers,
    },
};
use std::{cell::Cell, rc::Rc};

#[path = "support/allocations.rs"]
mod allocations;

const RED: Color = Color::rgb(255, 0, 0);
const GREEN: Color = Color::rgb(0, 255, 0);
const BLUE: u32 = u32::from_le_bytes([0, 0, 255, 255]);
const SKY: u32 = u32::from_le_bytes([0, 0, 0, 255]);

#[mirui::component]
struct Landscape;

#[mirui::system]
fn mount(stage: model!(StackStage), field: model!(HeightField), commands: commands!()) {
    if stage.is_empty() {
        commands.stage(stage).spawn(
            Landscape,
            StackVisual::new(field.clone()).at(Pose::at(-2, 0, 2)),
        );
    }
}

#[mirui::system]
fn leave(stage: model!(StackStage), land: query!(Entity, Landscape), commands: commands!()) {
    let _ = stage;
    for (entity, _) in land {
        commands.despawn(entity);
    }
}

struct Occluder<'a> {
    scanline: &'a [i32],
}

impl SceneLayers for Occluder<'_> {
    fn terrain(&self, tile: &mut SceneCanvas<'_, '_>, _: Option<Fixed>) {
        let [left, top, right, bottom] = tile.color.clip;
        for y in top..bottom {
            for x in left..right.min(self.scanline[y as usize]) {
                tile.write_visible(x, y, Fixed::ZERO, BLUE);
            }
        }
    }

    fn background(&self, tile: &mut SceneCanvas<'_, '_>) {
        tile.paint_background(|_, _| (), |_, _| (), |_, _, _, _| SKY);
    }
}

struct Probe {
    point: [i32; 2],
    depth: Option<Fixed>,
}

impl DepthQuery for Probe {
    fn next_point(&self) -> Option<[i32; 2]> {
        self.depth.is_none().then_some(self.point)
    }

    fn resolve(&mut self, tile: &SceneCanvas<'_, '_>) {
        self.depth = tile.sample_depth(self.point[0], self.point[1]);
    }
}

struct Probes<'a> {
    points: [Probe; 2],
    readback: &'a Cell<[Fixed; 2]>,
}

impl FrameEffects for Probes<'_> {
    type Query = Probe;

    fn queries(&mut self) -> &mut [Probe] {
        &mut self.points
    }

    fn finish_queries(&mut self, _: &RollMapping) {
        self.readback.set(
            self.points
                .each_ref()
                .map(|probe| probe.depth.unwrap_or(Fixed::MAX)),
        );
    }

    fn composite(&self, _: &mut Canvas<'_>) -> Result<(), RenderError> {
        Ok(())
    }
}

#[derive(Default)]
struct Samples {
    points: Cell<[[i32; 2]; 2]>,
    depths: Cell<[Fixed; 2]>,
}

#[mirui::model]
struct Panel {
    scanline: Box<[i32]>,
    #[observe]
    revision: u64,
}

#[mirui::model]
impl Panel {
    fn move_edge(&mut self, edge: i32) {
        self.scanline.fill(edge);
        self.revision += 1;
    }
}

struct Procedural(Rc<Samples>, ViewportOutput);

impl FrameContent<Panel> for Procedural {
    fn camera(&self, _: &Panel, frame: FrameSize) -> Result<WorldCamera, RenderError> {
        let center = frame.pixels().map(|side| Fixed::from(side) / 2);
        Ok(WorldCamera {
            camera: FrameCamera {
                projection: Projection {
                    perspective: Default::default(),
                    slope: Fixed::ZERO,
                    scale: Fixed::from_int(2),
                    center,
                },
                position: [Fixed::ZERO, Fixed::from_int(16), Fixed::ZERO],
                yaw: Fixed::ZERO,
                pitch: Fixed::PI / 2,
                roll: Fixed::ZERO,
            },
            pivot: center[1],
            roll: false,
        })
    }

    fn output(&self) -> ViewportOutput {
        self.1
    }

    fn prepare<'a>(
        &'a mut self,
        panel: &'a Panel,
        _: ContentFrame,
    ) -> Result<(impl SceneLayers + 'a, impl FrameEffects + 'a), RenderError> {
        Ok((
            Occluder {
                scanline: &panel.scanline,
            },
            Probes {
                points: self
                    .0
                    .points
                    .get()
                    .map(|point| Probe { point, depth: None }),
                readback: &self.0.depths,
            },
        ))
    }
}

#[mirui::compose]
fn screens(
    stage: model!(StackStage),
    panel: model!(Panel),
    samples: Rc<Samples>,
    output: ViewportOutput,
) -> Entity {
    ui! {
        Row {
            StackViewport(width: 16, height: 16, stage: stage.clone()) [
                ViewportContent::from_model(&panel, PanelHandle::revision, Procedural(samples, output))
            ]
            StackViewport(width: 16, height: 16, stage: stage,
                camera: Camera { center: [Fixed::ZERO, Fixed::from_int(16), Fixed::ZERO],
                    scale: Fixed::from_int(2), ..Camera::overhead() })
        }
    }
}

#[test]
fn attached_content_automatically_draws_stage_objects_in_color_and_effect_depth() {
    check_pass(ViewportOutput::Full);
    check_pass(ViewportOutput::Bounded);
    #[cfg(not(target_os = "none"))]
    check_foreign_model();
}

#[cfg(not(target_os = "none"))]
#[mirui::compose]
fn one_screen(stage: model!(StackStage), content: ViewportContent) -> Entity {
    ui! {
        StackViewport(stage: stage, grow: 1.0) [content]
    }
}

#[cfg(not(target_os = "none"))]
fn check_foreign_model() {
    let mut foreign = App::headless(16, 16);
    let panel = foreign
        .add_model(Panel {
            scanline: vec![8; 16].into_boxed_slice(),
            revision: 0,
        })
        .finish();
    let content = ViewportContent::from_model(
        &panel,
        |_| panic!("foreign Model observer must remain unread"),
        Procedural(Rc::default(), ViewportOutput::Bounded),
    );
    let mut app = App::headless(16, 16);
    app.with_default_widgets().add_plugin(SpriteStackPlugin);
    let stage = app.add_model(StackStage::bounded(0)).finish();
    let root = app.spawn_root().id();
    app.compose(root, one_screen(stage, content));
    app.tick();
    app.snapshot().unwrap().unwrap();
}

fn check_pass(output: ViewportOutput) {
    let mut app = App::headless(32, 16);
    app.with_default_widgets().add_plugin(SpriteStackPlugin);
    let stage = app.add_model(StackStage::bounded(1)).finish();
    let field = app
        .add_model(
            HeightField::generate(
                TerrainGrid::finite([1; 2], Fixed::from_int(4)).unwrap(),
                &[RED, GREEN],
                |_| TerrainCell {
                    level: 0,
                    material: 0,
                },
            )
            .unwrap(),
        )
        .finish();
    let samples = Rc::new(Samples::default());
    let mut panels = ModelList::bounded(1);
    assert!(
        panels
            .try_push(Panel {
                scanline: vec![8; 4096].into_boxed_slice(),
                revision: 0
            })
            .is_ok()
    );
    let panels = app.add_model(panels).finish();
    let (panel_id, panel) = panels.entries().next().unwrap();
    let data_address = panel.read(|panel| panel.scanline.as_ptr());
    let root = app.spawn_root().id();
    app.compose(
        root,
        screens(stage.clone(), panel.clone(), samples.clone(), output),
    );
    app.add_system(mount::system(stage.clone(), field.clone()));
    app.tick();

    let image = app.snapshot().unwrap().unwrap();
    let pixels = image.read_pixels().unwrap();
    let points = [0..8, 8..16].map(|xs| {
        (0..16)
            .flat_map(|y| xs.clone().map(move |x| [x, y]))
            .find(|&[x, y]| pixels.get_pixel(x + 16, y) == RED)
            .expect("the ordinary viewport renders one half of the shared field")
    });
    let [[lx, ly], [rx, ry]] = points;
    assert_eq!(pixels.get_pixel(lx, ly), Color::rgb(0, 0, 255));
    assert_eq!(pixels.get_pixel(rx, ry), RED);
    samples.points.set(points);
    field
        .set_cell(
            [0, 0],
            TerrainCell {
                level: 0,
                material: 1,
            },
        )
        .unwrap();
    let image = app.snapshot().unwrap().unwrap();
    let pixels = image.read_pixels().unwrap();
    assert_eq!(pixels.get_pixel(lx, ly), Color::rgb(0, 0, 255));
    assert_eq!(pixels.get_pixel(rx, ry), GREEN);
    assert_eq!(pixels.get_pixel(lx + 16, ly), GREEN);
    assert_eq!(pixels.get_pixel(rx + 16, ry), GREEN);
    assert_eq!(samples.depths.get(), [Fixed::ZERO, Fixed::from_int(16)]);
    app.render_dirty().unwrap();
    let warm_allocations = allocations::tracked_allocations(|| {
        field
            .set_cell(
                [0, 0],
                TerrainCell {
                    level: 0,
                    material: 0,
                },
            )
            .unwrap();
        app.render_dirty().unwrap();
    });
    assert_eq!(warm_allocations, 0);

    let warm_allocations = allocations::tracked_allocations(|| {
        panel.move_edge(0);
        app.render_dirty().unwrap();
    });
    assert_eq!(warm_allocations, 0);
    assert_eq!(panel.read(|panel| panel.scanline.as_ptr()), data_address);
    let image = app.snapshot().unwrap().unwrap();
    let pixels = image.read_pixels().unwrap();
    assert_eq!(pixels.get_pixel(lx, ly), RED);
    assert_eq!(pixels.get_pixel(lx + 16, ly), RED);
    assert_eq!(samples.depths.get(), [Fixed::from_int(16); 2]);
    panel.move_edge(8);

    app.add_system(leave::system(stage));
    app.tick();
    let image = app.snapshot().unwrap().unwrap();
    let pixels = image.read_pixels().unwrap();
    assert_eq!(pixels.get_pixel(lx, ly), Color::rgb(0, 0, 255));
    assert_eq!(pixels.get_pixel(rx, ry), Color::rgb(0, 0, 0));
    assert_eq!(samples.depths.get(), [Fixed::ZERO, Fixed::MAX]);
    drop(pixels);
    drop(image);

    assert!(panels.remove(panel_id).is_ok());
    let image = app.snapshot().unwrap().unwrap();
    let pixels = image.read_pixels().unwrap();
    assert_eq!(pixels.get_pixel(lx, ly), pixels.get_pixel(lx + 16, ly));
    assert_eq!(pixels.get_pixel(rx, ry), pixels.get_pixel(rx + 16, ry));
}

#[cfg(not(target_os = "none"))]
#[derive(Default)]
struct ContentChange(Option<(Entity, Option<ViewportContent>)>);

#[cfg(not(target_os = "none"))]
#[mirui::system]
fn replace_content(change: res!(mut ContentChange), commands: commands!()) {
    if let Some((entity, content)) = change.0.take() {
        match content {
            Some(content) => commands.insert(entity, content),
            None => commands.remove::<ViewportContent>(entity),
        }
    }
}

#[cfg(not(target_os = "none"))]
#[test]
fn content_replacement_rebinds_only_the_live_model() {
    let mut app = App::headless(16, 16);
    app.with_default_widgets().add_plugin(SpriteStackPlugin);
    let stage = app.add_model(StackStage::bounded(0)).finish();
    let mut add_panel = |edge| {
        app.add_model(Panel {
            scanline: vec![edge; 16].into_boxed_slice(),
            revision: 0,
        })
        .finish()
    };
    let first = add_panel(8);
    let next = add_panel(16);
    let make_content = |model, output| {
        ViewportContent::from_model(
            model,
            PanelHandle::revision,
            Procedural(Rc::default(), output),
        )
    };
    let root = app.spawn_root().id();
    let view = app.compose(
        root,
        one_screen(stage, make_content(&first, ViewportOutput::Bounded)),
    );
    app.add_resource(ContentChange::default());
    app.add_system(replace_content::system());
    app.tick();
    let blue = Color::rgb(0, 0, 255);
    let sample = |app: &mut App<_, _>| {
        let frame = app.snapshot().unwrap().unwrap();
        let pixels = frame.read_pixels().unwrap();
        [pixels.get_pixel(2, 4), pixels.get_pixel(12, 4)]
    };
    assert_eq!(sample(&mut app), [blue, Color::rgb(0, 0, 0)]);

    app.world.resource_mut::<ContentChange>().unwrap().0 =
        Some((view, Some(make_content(&next, ViewportOutput::Bounded))));
    app.tick();
    assert_eq!(sample(&mut app), [blue, blue]);
    first.move_edge(0);
    assert!(app.dirty_regions().unwrap().is_empty());
    next.move_edge(4);
    app.render_dirty().unwrap();
    assert_eq!(sample(&mut app), [blue, Color::rgb(0, 0, 0)]);

    app.world.resource_mut::<ContentChange>().unwrap().0 = Some((view, None));
    app.tick();
    assert_ne!(sample(&mut app)[0], blue);
    next.move_edge(16);
    assert!(app.dirty_regions().unwrap().is_empty());
}
