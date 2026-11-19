use mirui::core::cache::Handle;
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui::ui::widgets::{Button, Text};
use mirui_sprite_stack::{
    Camera, Object, Pose, SpriteStackPlugin, SpriteStackReaderExt, StackScene, StackViewport,
};

#[model]
struct Race {
    scene: model!(owned StackScene),
    driver: Object,
    leader: Object,
    #[observe]
    passes: u32,
}

impl Race {
    fn new() -> Self {
        let reader = mirx::Reader::open(include_bytes!("car.mirx")).expect("embedded model");
        let car = Handle::new(reader.sprite_stack().expect("car layers"));
        let mut scene = StackScene::bounded(2);
        let driver = scene.place(car.clone(), Pose::at(-16, 0, -20)).unwrap();
        let leader = scene.place(car, Pose::at(16, 0, 20)).unwrap();
        Self {
            scene: scene.into(),
            driver,
            leader,
            passes: 0,
        }
    }
}

#[model]
impl Race {
    fn drive(&mut self) {
        self.scene
            .move_forward(self.driver, Fixed::from_int(8))
            .unwrap();
        let driver = self.scene.pose(self.driver).unwrap();
        let mut leader = self.scene.pose(self.leader).unwrap();
        if driver.position[2] >= leader.position[2] {
            self.passes += 1;
            leader.position[2] += Fixed::from_int(48);
            self.scene.set_pose(self.leader, leader).unwrap();
        }
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
                Text(text: ${ format!("Passes: {}", race.passes()) })
            }
        }
    }
}

fn main() {
    let mut app = App::new(SdlSurface::new("Passing Lane", 360, 200));
    app.with_default_widgets()
        .with_default_systems()
        .add_plugin(SpriteStackPlugin);
    let race = app.add_model(Race::new()).finish();
    let root = app.spawn_root().id();
    app.compose(root, course(race));
    app.run();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_views_share_the_drivers_moves_and_one_score() {
        let mut app = App::headless(240, 160);
        app.with_default_widgets()
            .with_default_systems()
            .add_plugin(SpriteStackPlugin);
        let race = app.add_model(Race::new()).finish();
        let root = app.spawn_root().id();
        app.compose(root, course(race.clone()));
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
}
