#![cfg(all(feature = "widget", not(target_os = "none")))]

use mirui::prelude::*;
use mirui_sprite_stack::{
    Camera, GeneratedError, GeneratedLevel, GeneratedStackAsset, Pose, SpriteStackPlugin,
    StackSample, StackStage, StackStageCommands, StackViewport, StackVisual,
};

#[path = "support/allocations.rs"]
mod allocations;

const RED: Color = Color::rgb(255, 0, 0);
const GREEN: Color = Color::rgb(0, 255, 0);
const BLUE: Color = Color::rgb(0, 0, 255);
const YELLOW: Color = Color::rgb(255, 255, 0);

fn generate(
    x: u8,
    colors: [Color; 2],
    budget: usize,
) -> Result<GeneratedStackAsset, GeneratedError> {
    let fine = StackSample { x, y: 0, color: 0 };
    let coarse = StackSample { x, y: 0, color: 1 };
    GeneratedStackAsset::new(
        [4, 4, 2],
        [256, 256, 0],
        256,
        colors.map(|c| [c.r, c.g, c.b, c.a]).to_vec(),
        vec![
            GeneratedLevel::new(1, vec![0, 1, 1], vec![fine]),
            GeneratedLevel::new(2, vec![0, 0, 1], vec![coarse]),
        ],
        budget,
    )
}

#[mirui::model]
struct Ship {
    #[observe]
    hull: GeneratedStackAsset,
}

#[mirui::model]
impl Ship {
    fn rebuild(&mut self, x: u8, colors: [Color; 2], total: usize) -> Result<(), GeneratedError> {
        let budget = total.saturating_sub(self.hull.storage_bytes());
        self.hull = generate(x, colors, budget)?;
        Ok(())
    }

    fn install_hull(&mut self, hull: GeneratedStackAsset) {
        self.hull = hull;
    }
}

#[mirui::component]
struct ShipActor;

#[derive(Default)]
struct Edit {
    request: Option<(u8, usize)>,
    result: Option<Result<(), GeneratedError>>,
}

#[mirui::system]
fn mount(stage: model!(StackStage), ship: model!(Ship), commands: commands!()) {
    if stage.is_empty() {
        let visual = StackVisual::from_model(ship, |ship| {
            StackVisual::new(ship.hull()).at(Pose::at(0, 0, 0))
        });
        commands.stage(stage).spawn(ShipActor, visual);
    }
}

#[mirui::system]
fn edit(ship: model!(Ship), edit: res!(mut Edit)) {
    if let Some((x, budget)) = edit.request.take() {
        edit.result = Some(ship.rebuild(x, [BLUE, YELLOW], budget));
    }
}

#[mirui::compose]
fn views(stage: model!(StackStage)) -> Entity {
    ui! {
        Row {
            StackViewport(width: 24, height: 24, stage: stage.clone(), camera: Camera {
                scale: 2.into(),
                ..Camera::overhead()
            })
            StackViewport(width: 24, height: 24, stage: stage, camera: Camera {
                scale: Fixed::HALF,
                ..Camera::overhead()
            })
        }
    }
}

#[test]
fn generated_hull_has_one_owned_copy_and_publishes_only_after_validation() {
    let mut original = None;
    let allocations = allocations::tracked_storage(|| {
        original = Some(generate(0, [RED, GREEN], usize::MAX).unwrap());
    });
    let original = original.unwrap();
    let bytes = original.storage_bytes();
    assert_eq!(bytes, allocations.allocated - allocations.freed);
    assert!(bytes > 0);

    let mut app = App::headless(48, 24);
    app.with_default_widgets().add_plugin(SpriteStackPlugin);
    app.add_resource(Edit::default());
    let stage = app.add_model(StackStage::bounded(1)).finish();
    let ship = app
        .add_model(Ship {
            hull: original.clone(),
        })
        .finish();
    let root = app.spawn_root().id();
    app.compose(root, views(stage.clone()));
    app.add_system(mount::system(stage.clone(), ship.clone()));
    app.add_system(edit::system(ship.clone()));
    app.tick();

    let colors = |app: &mut App<_, _>, cases: [Color; 2]| {
        let image = app.snapshot().unwrap().unwrap();
        let pixels = image.read_pixels().unwrap();
        [(cases[0], 0..24), (cases[1], 24..48)].map(|(color, xs)| {
            (0..24)
                .flat_map(|y| xs.clone().map(move |x| [x, y]))
                .any(|[x, y]| pixels.get_pixel(x, y) == color)
        })
    };
    assert_eq!(colors(&mut app, [RED, GREEN]), [true, true]);
    let revision = stage.revision();
    let actor = app.world.query::<ShipActor>().iter().next().unwrap().0;

    for (x, budget, expected) in [
        (3, 2 * bytes, GeneratedError::Invalid),
        (2, 2 * bytes - 1, GeneratedError::Budget),
    ] {
        app.world.resource_mut::<Edit>().unwrap().request = Some((x, budget));
        app.tick();
        assert_eq!(
            app.world.resource_mut::<Edit>().unwrap().result.take(),
            Some(Err(expected))
        );
        assert_eq!(stage.revision(), revision);
        assert!(ship.hull() == original);
        assert!(app.dirty_regions().unwrap().is_empty());
        assert_eq!(colors(&mut app, [RED, GREEN]), [true, true]);
    }

    app.world.resource_mut::<Edit>().unwrap().request = Some((2, 2 * bytes));
    app.tick();
    assert_eq!(
        app.world.resource_mut::<Edit>().unwrap().result.take(),
        Some(Ok(()))
    );
    assert_eq!(stage.revision(), revision + 1);
    assert_eq!(ship.hull().storage_bytes(), bytes);
    assert_eq!(colors(&mut app, [BLUE, YELLOW]), [true, true]);
    assert_eq!(colors(&mut app, [RED, GREEN]), [false, false]);
    assert_eq!(
        app.world.query::<ShipActor>().iter().next().unwrap().0,
        actor
    );
    let retired = allocations::tracked_storage(|| drop(original));
    assert_eq!(retired.freed, bytes);

    let replacement = generate(0, [RED, GREEN], bytes).unwrap();
    let repaint_allocations = allocations::tracked_allocations(|| {
        ship.install_hull(replacement);
        app.render_dirty().unwrap();
    });
    assert_eq!(repaint_allocations, 0);
    assert_eq!(colors(&mut app, [RED, GREEN]), [true, true]);
    eprintln!(
        "generated asset: {bytes} allocator-requested bytes, replacement: {} bytes",
        2 * bytes
    );
}
