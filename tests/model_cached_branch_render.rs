use mirui::app::App;
use mirui::core::model::SharedValue;
use mirui::core::reactive::flush_signal_dirty;
use mirui::ecs::{Entity, World};
use mirui::input::event::hit_test::hit_test;
use mirui::model;
use mirui::render::{DrawCommand, DrawRequest, RenderError, RenderRoute, Renderer};
use mirui::types::{Color, Dimension, Fixed, Viewport};
use mirui::ui;
use mirui::ui::render_system;
use mirui::ui::{IdMap, branch};

const FIRST_COLOR: Color = Color::rgb(241, 31, 42);
const SECOND_COLOR: Color = Color::rgb(27, 88, 239);

#[model]
struct Mode {
    #[observe]
    selected: u8,
}

#[model]
impl Mode {
    fn select(&mut self, selected: u8) {
        self.selected = selected;
    }
}

#[derive(Default)]
struct FillColors(Vec<Color>);

impl Renderer for FillColors {
    fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        Ok(RenderRoute::Native)
    }

    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        if let DrawCommand::Fill { color, .. } = request.command {
            self.0.push(*color);
        }
        Ok(())
    }

    fn flush(&mut self) {}
}

fn rendered_fills_and_hit(world: &mut World, root: Entity) -> (Vec<Color>, Option<Entity>) {
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    render_system::update_layout(world, root, &viewport);
    let hit = hit_test(world, root, 8.into(), 8.into(), 64, 64);
    let mut fills = FillColors::default();
    render_system::render(world, root, &viewport, &mut fills).unwrap();
    (fills.0, hit)
}

#[test]
fn observed_model_switches_cached_match_paint_and_hit_target_in_both_directions() {
    let mut app = App::headless(64, 64);
    app.with_default_widgets();
    app.world.insert_resource(IdMap::new());
    let root = app.spawn_root().id();
    let mode = app.add_model(Mode { selected: 0 });
    let selection = mode.share();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        Column (width: Dimension::px(64), height: Dimension::px(64)) {
            match ${ selection.selected() } {
                0 => {
                    View (
                        id: "first",
                        width: 32,
                        height: 32,
                        bg_color: FIRST_COLOR
                    ) on Tap {}
                }
                _ => {
                    View (
                        id: "second",
                        width: 32,
                        height: 32,
                        bg_color: SECOND_COLOR
                    ) on Tap {}
                }
            }
        }
    };

    let first = app.world.find_by_id("first").unwrap();
    let second = app.world.find_by_id("second").unwrap();

    let (fills, hit) = rendered_fills_and_hit(&mut app.world, root);
    assert!(fills.contains(&FIRST_COLOR));
    assert!(!fills.contains(&SECOND_COLOR));
    assert_eq!(hit, Some(first));

    mode.select(1);
    flush_signal_dirty(&mut app.world);
    let (fills, hit) = rendered_fills_and_hit(&mut app.world, root);
    assert!(!fills.contains(&FIRST_COLOR));
    assert!(fills.contains(&SECOND_COLOR));
    assert_eq!(hit, Some(second));
    assert!(branch::is_effectively_hidden(&app.world, first));

    mode.select(0);
    flush_signal_dirty(&mut app.world);
    let (fills, hit) = rendered_fills_and_hit(&mut app.world, root);
    assert!(fills.contains(&FIRST_COLOR));
    assert!(!fills.contains(&SECOND_COLOR));
    assert_eq!(hit, Some(first));
    assert!(branch::is_effectively_hidden(&app.world, second));
    assert_eq!(app.world.find_by_id("first"), Some(first));
    assert_eq!(app.world.find_by_id("second"), Some(second));
    assert!(app.world.is_alive(first));
    assert!(app.world.is_alive(second));
}
