use mirui::app::plugins::{FrameRateCapPlugin, StdInstantClockPlugin};
use mirui::core::time::clock_now_ns;
use mirui::core::{cache::Handle, model::ModelHandle};
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui::ui::widgets::{Button, Text};
use mirui_sprite_stack::{
    Camera, HeightField, HeightFieldHandle, Pose, SpriteStackAsset, SpriteStackPlugin,
    SpriteStackReaderExt, StackStage, StackStageCommands, StackViewport, StackVisual, TerrainCell,
    TerrainGrid,
};

const ASPHALT: Color = Color::rgb(64, 75, 86);
const VERGE: Color = Color::rgb(59, 105, 77);
const LIFT: Color = Color::rgb(216, 151, 68);
const MOWN_VERGE: Color = Color::rgb(74, 119, 85);
const ROAD_PERIOD: Fixed = Fixed::from_int(128);
const TRAFFIC_SPACING: Fixed = Fixed::from_int(384);

fn now_ms() -> u64 {
    clock_now_ns() / 1_000_000
}

fn road_surface() -> HeightField {
    let grid = TerrainGrid::repeating([4, 4], ROAD_PERIOD / 4).unwrap();
    HeightField::generate(grid, &[ASPHALT, VERGE, LIFT, MOWN_VERGE], |[lane, row]| {
        TerrainCell {
            level: 0,
            material: if lane < 2 {
                0
            } else if row % 2 == 0 {
                1
            } else {
                3
            },
        }
    })
    .unwrap()
}

#[model]
struct Race {
    stage: model!(owned StackStage),
    surface: HeightFieldHandle,
    #[observe]
    driver: Pose,
    #[observe]
    leader: Pose,
    #[observe]
    passes: u32,
    #[observe]
    paused: bool,
    last_ms: u64,
}

impl Race {
    fn new(surface: HeightFieldHandle) -> Self {
        // The atlas is longitudinal along local X. Both lanes run along world +Z.
        let lane_pose = |x, z| Pose {
            yaw: Fixed::from_int(270),
            ..Pose::at(x, 0, z)
        };
        Self {
            stage: StackStage::bounded(3).into(),
            surface,
            driver: lane_pose(16, 16),
            leader: lane_pose(48, 56),
            passes: 0,
            paused: false,
            last_ms: now_ms(),
        }
    }

    fn road_height(&self, x: Fixed, z: Fixed) -> Fixed {
        self.surface
            .read(|field| field.height_at(x, z))
            .expect("repeating road")
    }

    fn place_on_road(&self, mut pose: Pose) -> Pose {
        pose.position[1] = self.road_height(pose.position[0], pose.position[2]);
        pose
    }
}

#[model]
impl Race {
    fn advance(&mut self, now: u64) {
        // Bound the first step after a stalled or suspended window.
        let elapsed = now.saturating_sub(self.last_ms).min(250) as i32;
        self.last_ms = now;
        let (mut driver, mut leader) = (self.driver, self.leader);
        let was_ahead = leader.position[2] > driver.position[2];
        driver.position[2] += Fixed::from_ratio(elapsed, 16);
        leader.position[2] += Fixed::from_ratio(elapsed, 32);
        if was_ahead && leader.position[2] <= driver.position[2] {
            self.passes = self.passes.saturating_add(1);
        }
        // Reuse the slow car only after it has left both views.
        if driver.position[2] - leader.position[2] >= TRAFFIC_SPACING / 2 {
            leader.position[2] += TRAFFIC_SPACING;
        }
        // Keep coordinates bounded while preserving the view and the gap between the cars.
        if driver.position[2] >= ROAD_PERIOD {
            driver.position[2] -= ROAD_PERIOD;
            leader.position[2] -= ROAD_PERIOD;
        }
        self.driver = self.place_on_road(driver);
        self.leader = self.place_on_road(leader);
    }

    fn toggle_pause(&mut self) {
        self.paused = !self.paused;
        self.last_ms = now_ms();
    }

    fn toggle_lift(&mut self) {
        let cell = if self.road_height(Fixed::ZERO, Fixed::ZERO).is_zero() {
            TerrainCell {
                level: 8,
                material: 2,
            }
        } else {
            TerrainCell {
                level: 0,
                material: 0,
            }
        };
        self.surface.set_cell([0, 0], cell).unwrap();
        self.driver = self.place_on_road(self.driver);
        self.leader = self.place_on_road(self.leader);
    }

    fn surface(&self) -> HeightFieldHandle {
        self.surface.clone()
    }
}

struct CourseAsset(Option<Handle<SpriteStackAsset>>);

#[component]
struct Road;

#[component]
struct Car;

#[system]
fn populate(race: model!(Race), asset: res!(mut CourseAsset), commands: commands!()) {
    let Some(car) = asset.0.take() else {
        return;
    };
    let stage = race.stage();
    let mut members = commands.stage(&stage);
    let driver = car.clone();
    members.spawn(Road, StackVisual::new(race.surface()));
    members.spawn(
        Car,
        StackVisual::from_model(race, move |state| {
            StackVisual::new(driver.clone()).at(state.driver())
        }),
    );
    members.spawn(
        Car,
        StackVisual::from_model(race, move |state| {
            StackVisual::new(car.clone()).at(state.leader())
        }),
    );
}

// The App paces frames. Advance once per tick when the simulation clock has moved.
#[timer(every = 1)]
fn drive(race: model!(Race)) {
    if !race.paused() {
        race.advance(now_ms());
    }
}

#[compose]
fn course(race: model!(Race)) -> Entity {
    let stage = race.stage();
    let chase = Camera {
        yaw: Fixed::from_int(25),
        range: Fixed::from_int(112),
        scale: Fixed::from_int(2),
        ..Camera::default()
    };
    let overhead = Camera {
        range: Fixed::from_int(96),
        ..Camera::overhead()
    };
    ui! {
        Column (grow: 1.0) {
            Row (grow: 1.0) {
                StackViewport (
                    grow: 2.0,
                    min_width: 0,
                    stage: stage.clone(),
                    camera: ${
                        Camera {
                            center: race.driver().position,
                            ..chase
                        }
                    }
                )
                StackViewport (
                    grow: 1.0,
                    min_width: 0,
                    stage: stage,
                    camera: ${
                        Camera {
                            center: race.driver().position,
                            ..overhead
                        }
                    }
                )
            }
            Row (height: 44, gap: 12, padding: Padding::all(8), align: AlignItems::Center) {
                Button (text: ${ if race.paused() { "Resume" } else { "Pause" } }) on Tap { race.toggle_pause(); }
                Button ("Toggle lift") on Tap { race.toggle_lift(); }
                Text (text: ${ format!("Passes: {}", race.passes()) })
            }
        }
    }
}

fn setup<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) -> RaceHandle {
    app.with_default_widgets()
        .with_default_systems()
        .add_plugin(StdInstantClockPlugin)
        .add_plugin(SpriteStackPlugin);
    let surface = app.add_model(road_surface()).finish();
    let race = app.add_model(Race::new(surface)).finish();
    let reader = mirx::Reader::open(include_bytes!("car.mirx")).expect("embedded model");
    let car = Handle::new(reader.sprite_stack().expect("car layers"));
    app.add_resource(CourseAsset(Some(car)));
    app.add_system(populate::system(race.clone()));
    app.add_timer(drive::timer(race.clone()));
    let root = app.spawn_root().id();
    app.compose(root, course(race.clone()));
    race
}

fn main() {
    let mut app = App::new(SdlSurface::new("Passing Lane", 360, 200));
    setup(&mut app);
    app.add_plugin(FrameRateCapPlugin::new(120));
    app.run();
}

#[cfg(test)]
mod tests {
    use super::*;
    use mirui::core::time::mock;
    use mirui::input::event::sim::{SimCommand, SimulatedInput, sim_input_system};
    use mirui::surface::framebuf::FramebufSurface;
    use mirui::surface::{FramebufferAccess, InputEvent, PointerKind};
    use mirui::ui::ComputedRect;

    type TestApp = App<FramebufSurface<fn(&[u8], PhysicalRect)>>;

    fn car_positions(race: &RaceHandle) -> [Fixed; 2] {
        [race.driver(), race.leader()].map(|pose| pose.position[2])
    }

    fn click_button(app: &mut TestApp, label: &str) {
        let (button, _) = app
            .world
            .query::<Text>()
            .iter()
            .find(|(entity, text)| app.world.has::<Button>(*entity) && text.resolve(None) == label)
            .expect("button with the requested label");
        let rect = app.world.get::<ComputedRect>(button).unwrap().0;
        let display = app.backend.display_info();
        let window = Rect::new(0, 0, display.width, display.height);
        assert_eq!(
            rect.intersect(&window),
            Some(rect),
            "{label} outside the window"
        );
        let x = rect.x + rect.w / Fixed::from_int(2);
        let y = rect.y + rect.h / Fixed::from_int(2);
        let kind = PointerKind::Mouse;
        app.add_resource(SimulatedInput::new(vec![
            SimCommand::at(0, InputEvent::PointerDown { kind, id: 0, x, y }),
            SimCommand::at(0, InputEvent::PointerUp { kind, id: 0, x, y }),
        ]));
        assert!(!app.tick());
    }

    #[test]
    fn overhead_car_is_longer_along_the_road_than_across_it() {
        let mut app = App::headless(360, 200);
        setup(&mut app);
        assert!(!app.tick());
        let frame = app.snapshot().unwrap().unwrap();
        let pixels = frame.read_pixels().unwrap();
        let (camera, _) = app
            .world
            .query::<StackViewport>()
            .iter()
            .find(|(_, view)| view.camera.pitch == Fixed::from_int(90))
            .unwrap();
        let view = app.world.get::<ComputedRect>(camera).unwrap().0;
        let x = (view.x + view.w / 2).to_int();
        let y = (view.y + view.h / 2).to_int();
        // Isolate the tracked car at the center of the overhead view.
        let paint = Color::rgb(224, 61, 41);
        let body = (y - 20..=y + 20)
            .flat_map(|row| (x - 20..=x + 20).map(move |col| (col, row)))
            .filter(|&(col, row)| pixels.get_pixel(col, row) == paint)
            .map(|(col, row)| Rect::new(col, row, 1, 1))
            .reduce(|bounds, pixel| bounds.union(&pixel))
            .expect("visible car body");
        assert!(body.h > body.w, "overhead body bounds {body:?}");
    }

    #[test]
    fn both_views_show_traffic_and_passes_without_input() {
        let _clock = mock::install();
        mock::set_ms(0);
        let mut app = App::headless(240, 160);
        let race = setup(&mut app);
        app.add_system(sim_input_system::system());
        assert!(!app.tick());
        let before = app.snapshot().unwrap().unwrap();
        for time in [8, 17, 25] {
            mock::set_ms(time);
            assert!(!app.tick());
            assert_eq!(
                car_positions(&race)[0],
                Fixed::from_int(16) + Fixed::from_ratio(time as i32, 16)
            );
        }
        for time in (80..=640).step_by(80) {
            mock::set_ms(time);
            assert!(!app.tick());
        }
        assert_eq!(
            car_positions(&race),
            [Fixed::from_int(56), Fixed::from_int(76)]
        );
        let after = app.snapshot().unwrap().unwrap();
        let (a, b) = (before.read_pixels().unwrap(), after.read_pixels().unwrap());
        for (left, right) in [(0, 160), (160, 240)] {
            assert!(
                (0..100).any(|y| (left..right).any(|x| a.get_pixel(x, y) != b.get_pixel(x, y)))
            );
        }
        assert_eq!(race.passes(), 0);
        for time in (720..=1360).step_by(80) {
            mock::set_ms(time);
            assert!(!app.tick());
        }
        assert_eq!(race.passes(), 1);
        assert_eq!(
            car_positions(&race),
            [Fixed::from_int(101), Fixed::from_ratio(197, 2)]
        );
        click_button(&mut app, "Pause");
        let parked = car_positions(&race);
        mock::advance_ms(10_000);
        assert!(!app.tick());
        assert_eq!(car_positions(&race), parked);
        click_button(&mut app, "Toggle lift");
        assert_eq!(
            race.read(|race| race.road_height(Fixed::ZERO, Fixed::ZERO)),
            Fixed::from_int(8)
        );
        click_button(&mut app, "Resume");
        mock::advance_ms(32);
        assert!(!app.tick());
        assert_eq!(
            car_positions(&race),
            [parked[0] + Fixed::from_int(2), parked[1] + Fixed::ONE]
        );
    }

    #[test]
    fn lifting_the_road_updates_contact_and_both_views_on_the_same_frame() {
        let _clock = mock::install();
        mock::set_ms(0);
        let mut app = App::headless(360, 200);
        let race = setup(&mut app);
        assert!(!app.tick());
        let contact = || {
            race.read(|race| {
                let [x, y, z] = race.driver.position;
                (y, race.road_height(x, z))
            })
        };
        app.render_dirty().unwrap();
        for raised in [true, false, true] {
            race.toggle_lift();
            let expected = if raised { 8 } else { 0 };
            assert_eq!(
                contact(),
                (Fixed::from_int(expected), Fixed::from_int(expected))
            );
            assert!(app.dirty_region().unwrap().is_some());
            app.render_dirty().unwrap();
            let frame = app.backend.framebuffer();
            let pixels = frame.read_pixels().unwrap();
            for viewport in [0..240, 240..360] {
                let has_lift_color =
                    (0..140).any(|y| viewport.clone().any(|x| pixels.get_pixel(x, y) == LIFT));
                assert_eq!(has_lift_color, raised);
            }
        }
        race.advance(96);
        assert_eq!(contact(), (Fixed::from_int(8), Fixed::from_int(8)));
        race.advance(192);
        race.advance(256);
        assert_eq!(contact(), (Fixed::ZERO, Fixed::ZERO));
        assert_eq!(race.passes(), 0);
    }

    #[test]
    fn traffic_falls_behind_before_reentering_ahead_on_the_repeating_road() {
        let _clock = mock::install();
        mock::set_ms(0);
        let mut app = App::headless(360, 200);
        let race = setup(&mut app);
        let mut last_gap = Fixed::from_int(40);
        let mut reentries = 0;
        for time in (64..=16_000).step_by(64) {
            race.advance(time);
            let [driver, leader] = car_positions(&race);
            assert!((Fixed::ZERO..ROAD_PERIOD).contains(&driver));
            let gap = leader - driver;
            if gap > last_gap {
                assert!(last_gap < Fixed::from_int(-160) && gap > Fixed::from_int(160));
                reentries += 1;
            } else {
                assert_eq!(gap, last_gap - Fixed::from_int(2));
            }
            last_gap = gap;
        }
        assert_eq!((reentries, race.passes()), (1, 2));
    }
}
