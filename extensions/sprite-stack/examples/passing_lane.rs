use mirui::core::cache::Handle;
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui::ui::widgets::{Button, Text};
use mirui_sprite_stack::{
    Camera, HeightField, HeightFieldHandle, Object, Pose, SpriteStackPlugin, SpriteStackReaderExt,
    StackScene, StackViewport, TerrainCell, TerrainGrid,
};

const ASPHALT: Color = Color::rgb(64, 75, 86);
const VERGE: Color = Color::rgb(59, 105, 77);
const LIFT: Color = Color::rgb(216, 151, 68);

fn road_surface() -> HeightField {
    let grid = TerrainGrid::repeating([4, 4], Fixed::from_int(32)).unwrap();
    HeightField::generate(grid, &[ASPHALT, VERGE, LIFT], |[lane, _]| TerrainCell {
        level: 0,
        material: u8::from(lane >= 2),
    })
    .unwrap()
}

#[model]
struct Race {
    scene: model!(owned StackScene),
    road: Object,
    surface: HeightFieldHandle,
    driver: Object,
    leader: Object,
    #[observe]
    passes: u32,
}

impl Race {
    fn new(surface: HeightFieldHandle) -> Self {
        let reader = mirx::Reader::open(include_bytes!("car.mirx")).expect("embedded model");
        let car = Handle::new(reader.sprite_stack().expect("car layers"));
        let mut scene = StackScene::bounded(3);
        let road = scene.place(surface.clone(), Pose::default()).unwrap();
        let driver = scene.place(car.clone(), Pose::at(16, 0, 16)).unwrap();
        let leader = scene.place(car, Pose::at(48, 0, 56)).unwrap();
        Self {
            scene: scene.into(),
            road,
            surface,
            driver,
            leader,
            passes: 0,
        }
    }

    fn road_height(&self, x: Fixed, z: Fixed) -> Fixed {
        self.scene
            .height_at(self.road, x, z)
            .unwrap()
            .expect("repeating road")
    }

    fn follow_road(&mut self, car: Object, advance: i32) {
        let mut pose = self.scene.pose(car).unwrap();
        pose.position[2] += Fixed::from_int(advance);
        pose.position[1] = self.road_height(pose.position[0], pose.position[2]);
        self.scene.set_pose(car, pose).unwrap();
    }
}

#[model]
impl Race {
    fn drive(&mut self) {
        self.follow_road(self.driver, 8);
        let driver = self.scene.pose(self.driver).unwrap();
        let leader = self.scene.pose(self.leader).unwrap();
        if driver.position[2] >= leader.position[2] {
            self.passes += 1;
            self.follow_road(self.leader, 48);
        }
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
        self.follow_road(self.driver, 0);
        self.follow_road(self.leader, 0);
    }

    fn driver(&self) -> Object {
        self.driver
    }
}

#[compose]
fn course(race: model!(Race)) -> Entity {
    let scene = race.scene();
    let chase = Camera {
        yaw: Fixed::from_int(25),
        scale: Fixed::from_int(2),
        ..Camera::chase(race.driver())
    };
    let overhead = Camera {
        target: Some(race.driver()),
        ..Camera::overhead()
    };
    ui! {
        Column(grow: 1.0) {
            Row(grow: 1.0) {
                StackViewport(grow: 2.0, min_width: 0, scene: scene.clone(), camera: chase)
                StackViewport(grow: 1.0, min_width: 0, scene: scene, camera: overhead)
            }
            Row(gap: 12, padding: Padding::all(8)) {
                Button("Drive") on Tap { race.drive(); }
                Button("Toggle lift") on Tap { race.toggle_lift(); }
                Text(text: ${ format!("Passes: {}", race.passes()) })
            }
        }
    }
}

fn setup<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) -> RaceHandle {
    app.with_default_widgets()
        .with_default_systems()
        .add_plugin(SpriteStackPlugin);
    let surface = app.add_model(road_surface()).finish();
    let race = app.add_model(Race::new(surface)).finish();
    let root = app.spawn_root().id();
    app.compose(root, course(race.clone()));
    race
}

fn main() {
    let mut app = App::new(SdlSurface::new("Passing Lane", 360, 200));
    setup(&mut app);
    app.run();
}

#[cfg(test)]
mod tests {
    use super::*;
    use mirui::core::model::ModelHandle;
    use mirui::surface::FramebufferAccess;

    #[test]
    fn two_views_share_the_drivers_moves_and_one_score() {
        let mut app = App::headless(240, 160);
        let race = setup(&mut app);
        let before = app.snapshot().unwrap().unwrap();
        for _ in 0..4 {
            race.drive();
        }
        let after = app.snapshot().unwrap().unwrap();
        let (a, b) = (before.read_pixels().unwrap(), after.read_pixels().unwrap());
        for (left, right) in [(0, 160), (160, 240)] {
            assert!(
                (0..100).any(|y| (left..right).any(|x| a.get_pixel(x, y) != b.get_pixel(x, y)))
            );
        }
        assert_eq!(race.passes(), 0);
        race.drive();
        assert_eq!(race.passes(), 1);
    }

    #[test]
    fn lifting_the_road_updates_contact_and_both_views_on_the_same_frame() {
        let mut app = App::headless(360, 200);
        let race = setup(&mut app);
        let contact = || {
            race.read(|race| {
                let driver = race.scene.pose(race.driver).unwrap();
                let [x, y, z] = driver.position;
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
        race.drive();
        assert_eq!(contact(), (Fixed::from_int(8), Fixed::from_int(8)));
        race.drive();
        assert_eq!(contact(), (Fixed::ZERO, Fixed::ZERO));
        assert_eq!(race.passes(), 0);
    }
}
