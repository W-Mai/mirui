use super::{
    Profile,
    flight::{Flight, Settings, View as CameraView},
    raster::Daylight,
    world::World as Terrain,
};
use core::cell::Cell;
use mirui_sprite_stack::{SpriteStackPlugin, StackScene, StackViewport};
mod objects;
pub(crate) use objects::FlightObjects;
mod renderer;
use renderer::{FlightRenderer, cockpit_layer};

use mirui::input::event::{
    PointerCursor,
    input::{Key, PhysicalKey},
    lifecycle::{HostFocusChanged, HostVisibilityChanged},
    press::{PressCancelReason, PressCancelled, Pressed, Released},
};
use mirui::prelude::*;
use mirui::render::font::{Font, FontManager, FontToken};
use mirui::ui::ComputedRect;
use mirui::ui::widgets::{ParagraphStyle, Text, TextAlign};
use mirui::{
    core::{cache::Handle, model::ModelHandle},
    ecs::DeltaTimeMs,
};

const FLIGHT_FONT: FontToken = FontToken::Custom("pixel-flight-ui");
#[mirui::model]
pub struct FlightModel {
    profile: Profile,
    terrain: Handle<Terrain>,
    flight: Flight,
    pub scene: model!(owned StackScene),
    objects: FlightObjects,
    day: Daylight,
    display_bytes: Cell<usize>,
    cockpit_bytes: Cell<usize>,
    revision: u64,
    hint_time: Fixed,
    focused: bool,
    visible: bool,
    autoplay: bool,
    view_elapsed_ms: u16,
}
impl FlightModel {
    fn new(width: u16, height: u16) -> Self {
        Self::with_profile(width, height, Profile::Desktop)
    }
    fn with_profile(_width: u16, _height: u16, profile: Profile) -> Self {
        let terrain = match profile {
            Profile::Desktop => Terrain::new(7, Fixed::from_ratio(33, 100)),
            Profile::Compact => Terrain::with_size(7, Fixed::from_ratio(33, 100), 64, 32, 16),
        };
        let flight = Flight::new(&terrain, Settings::default());
        let (scene, objects) = FlightObjects::new(&terrain, &flight);
        Self {
            profile,
            terrain: Handle::new(terrain),
            flight,
            scene: scene.into(),
            objects,
            display_bytes: Cell::new(0),
            cockpit_bytes: Cell::new(0),
            day: Daylight::default(),
            revision: 0,
            hint_time: Fixed::ZERO,
            focused: true,
            visible: true,
            autoplay: false,
            view_elapsed_ms: 0,
        }
    }
}
#[mirui::model]
impl FlightModel {
    /// Enable unattended flight and cycle the camera every twelve simulation seconds.
    ///
    /// Manual input is ignored while enabled. Hidden or unfocused scenes pause.
    ///
    /// Changing the mode releases held controls without resetting flight or camera state.
    pub fn set_autoplay(&mut self, enabled: bool) {
        if self.autoplay == enabled {
            return;
        }
        self.autoplay = enabled;
        self.view_elapsed_ms = 0;
        self.flight.controls.release_all();
    }
    /// The procedural terrain object used by the flight camera and ground-clearance controls.
    pub fn ground(&self) -> mirui_sprite_stack::Object {
        self.objects.ground()
    }

    /// Returns the scene object used by the pilot and chase cameras.
    pub fn player(&self) -> mirui_sprite_stack::Object {
        self.objects.player()
    }

    #[observe]
    pub fn autoplay(&self) -> bool {
        self.autoplay
    }
    #[observe]
    pub fn revision(&self) -> u64 {
        self.revision
    }
    #[observe]
    pub fn hint_color(&self) -> Color {
        Color {
            r: 255,
            g: 255,
            b: 255,
            a: ((Fixed::ONE
                - super::flight::smooth((self.hint_time - Fixed::from_int(5)) / Fixed::ONE))
                * Fixed::from_int(220))
            .trunc_to_int() as u8,
        }
    }
    #[observe]
    pub fn view(&self) -> CameraView {
        self.flight.view
    }
    pub fn select_view(&mut self, view: CameraView) {
        self.flight.view = view;
        self.hint_time = Fixed::ZERO;
    }
    pub fn cycle_view(&mut self) {
        self.flight.cycle_view();
        self.hint_time = Fixed::ZERO;
    }
    pub fn press(&mut self) {
        if !self.autoplay {
            self.flight.controls.press();
        }
    }
    pub fn release(&mut self, cancelled: bool) {
        self.flight.release_pointer(cancelled);
    }
    pub fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        if !focused {
            self.flight.controls.release_all();
        }
    }
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        if !visible {
            self.flight.controls.release_all();
        }
    }
    pub fn key(&mut self, physical: PhysicalKey, pressed: bool, repeat: bool) {
        self.handle_key(physical, pressed, repeat);
    }

    #[model(local)]
    fn handle_key(&mut self, physical: PhysicalKey, pressed: bool, repeat: bool) -> bool {
        if self.autoplay || !self.focused || !self.visible {
            return false;
        }
        let bit = match physical {
            PhysicalKey::ArrowLeft => Some(0),
            PhysicalKey::KeyA => Some(1),
            PhysicalKey::ArrowRight => Some(2),
            PhysicalKey::KeyD => Some(3),
            PhysicalKey::ArrowUp => Some(4),
            PhysicalKey::KeyW => Some(5),
            PhysicalKey::ArrowDown => Some(6),
            PhysicalKey::KeyS => Some(7),
            PhysicalKey::ShiftLeft => Some(8),
            PhysicalKey::ShiftRight => Some(9),
            _ => None,
        };
        if let Some(bit) = bit {
            self.flight.controls.key(bit, pressed);
            return true;
        }
        if !pressed || repeat {
            return false;
        }
        match physical {
            PhysicalKey::Space => self.cycle_view(),
            PhysicalKey::Digit1 => self.select_view(CameraView::Overhead),
            PhysicalKey::Digit2 => self.select_view(CameraView::Chase),
            PhysicalKey::Digit3 => self.select_view(CameraView::Cockpit),
            PhysicalKey::KeyR => {
                self.flight.intro = Fixed::ZERO;
                self.flight.burn = Fixed::ZERO;
            }
            PhysicalKey::KeyE => {
                self.flight.settings.spread = if self.flight.settings.spread > Fixed::ONE {
                    Fixed::ONE
                } else {
                    Fixed::from_int(5)
                };
            }
            PhysicalKey::KeyF => {
                self.flight.set_wingmen(self.flight.count % 3);
            }
            PhysicalKey::KeyT => {
                self.day = self.day.next();
                self.scene.invalidate();
            }
            _ => return false,
        }
        true
    }
    pub fn advance(
        &mut self,
        milliseconds: u32,
        width: u16,
        height: u16,
        pointer: Option<[Fixed; 2]>,
    ) {
        self.advance_viewport(
            milliseconds,
            [Fixed::from(width), Fixed::from(height)],
            pointer,
        );
    }

    fn advance_viewport(
        &mut self,
        milliseconds: u32,
        dimensions: [Fixed; 2],
        pointer: Option<[Fixed; 2]>,
    ) {
        if !self.focused || !self.visible || dimensions.iter().any(|value| !value.is_positive()) {
            return;
        }
        let ([scene_width, scene_height], pointer) = self.profile.frame_input(dimensions, pointer);
        let milliseconds = milliseconds.min(50) as u16;
        if self.autoplay {
            self.view_elapsed_ms += milliseconds;
            if self.view_elapsed_ms >= 12_000 {
                self.view_elapsed_ms -= 12_000;
                self.cycle_view();
            }
        } else if let Some(point) = pointer {
            self.flight.controls.point(point);
        } else {
            self.flight.controls.pointer = None;
            self.flight.release_pointer(true);
        }
        self.flight.step_above(
            self.terrain.at(self.scene.pose(self.objects.ground())),
            milliseconds,
            scene_width,
            scene_height,
        );
        self.hint_time += Fixed::from_int(milliseconds.into()) / Fixed::from_int(1000);
        self.objects.sync(&mut self.scene, &mut self.flight);
        self.scene.invalidate();
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn resident_bytes(&self) -> usize {
        self.terrain.bytes() + core::mem::size_of::<Self>() + self.flight.bytes()
            - core::mem::size_of::<Flight>()
            + self.display_bytes.get()
            + self.cockpit_bytes.get()
            + core::mem::size_of::<mirui::core::cache::handle::CacheEntry<Terrain>>()
    }
}
#[mirui::component]
struct FlightInput {
    pub model: model!(FlightModel),
}
#[mirui::view(component = FlightInput, name = "Flight controls",systems = [animate::system()])]
fn flight_input() {}

# [mirui :: system (order = Animation)]
fn animate(
    stages: query!(FlightInput, ComputedRect),
    cursor: res!(PointerCursor),
    delta: res!(DeltaTimeMs),
) {
    for (stage, rect) in stages {
        let rect = rect.0;
        let pointer = (cursor.present
            && cursor.x >= rect.x
            && cursor.y >= rect.y
            && cursor.x < rect.x + rect.w
            && cursor.y < rect.y + rect.h)
            .then(|| [(cursor.x - rect.x), (cursor.y - rect.y)]);
        stage
            .model
            .advance_viewport(delta.0.into(), [rect.w, rect.h], pointer);
    }
}
#[mirui::event]
fn key(event: event!(Key), model: model!(FlightModel)) -> bool {
    let Some(physical) = event.physical else {
        return false;
    };
    let mut handled = false;
    model.update(|flight| {
        handled = flight.handle_key(physical, event.pressed, event.repeat == Some(true));
    });
    handled
}
#[mirui::event]
fn focus(event: event!(HostFocusChanged), model: model!(FlightModel)) {
    model.set_focus(event.focused);
}
#[mirui::event]
fn visibility(event: event!(HostVisibilityChanged), model: model!(FlightModel)) {
    model.set_visible(event.visible);
}
#[mirui::event]
fn press(_event: event!(Pressed), model: model!(FlightModel)) {
    model.press();
}
#[mirui::event]
fn release(_event: event!(Released), model: model!(FlightModel)) {
    model.release(false);
}
#[mirui::event]
fn cancel(event: event!(PressCancelled), model: model!(FlightModel)) {
    if event.reason != PressCancelReason::Gesture {
        model.release(true);
    }
}
#[mirui::compose]
fn build_widgets(model: model!(FlightModel), profile: Profile) {
    if profile == Profile::Desktop {
        let font = Font::from_mirx(
            "Flight UI",
            12,
            include_bytes!("../../../src/gallery/demos/assets/nearby-menu.mirx"),
            &mirx::reader::PayloadLimits::HOST,
        )
        .expect("bundled pixel UI font");
        res!(FontManager).with(|fonts| fonts.add_static(FLIGHT_FONT.cache_key(), font));
    }
    if !res!(optional MonoClock).with(|clock| clock.is_some()) {
        res!().insert(MonoClock::from_time_source());
    }
    let mut viewport =
        StackViewport::with_renderer(model.scene(), FlightRenderer::new(model.clone(), profile));
    viewport.resolution = profile.resolution();
    let cockpit = cockpit_layer(model.clone(), profile);

    ui! {
        View (
            grow: 1.0,
            clip_children: true,
            direction: FlexDirection::Column,
            justify: JustifyContent::FlexEnd,
            padding: Padding {
                bottom: Dimension::px(if profile == Profile::Compact { 2 } else { 18 }),
                left: Dimension::px(if profile == Profile::Compact { 2 } else { 18 }),
                right: Dimension::px(if profile == Profile::Compact { 2 } else { 18 }),
                ..Padding::all(0)
            }
        ) [
            FlightInput {
                model: model.clone(),
            },
            viewport,
            cockpit,
            mirui::input::event::scroll::TouchAction::None,
        ] {
            Text (
                ${ hint(model.view(), profile, model.autoplay()) },
                width: Dimension::percent(100),
                font: if profile == Profile::Compact { FontToken::Default } else { FLIGHT_FONT },
                font_size: if profile == Profile::Compact { 8 } else { 12 },
                text_color: ${ model.hint_color() },
                paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
            ) [
                mirui::ui::IgnoreHitTest,
            ]
        } on Pressed(press::callback(model.clone())) on Released(release::callback(model.clone())) on PressCancelled(cancel::callback(model))
    };
}
fn hint(view: CameraView, profile: Profile, autoplay: bool) -> &'static str {
    if autoplay {
        return match (view, profile) {
            (CameraView::Overhead, Profile::Compact) => "AUTO / TOP",
            (CameraView::Chase, Profile::Compact) => "AUTO / CHASE",
            (CameraView::Cockpit, Profile::Compact) => "AUTO / COCKPIT",
            (CameraView::Overhead, Profile::Desktop) => "AUTO FLIGHT / OVERHEAD / CAMERA EVERY 12S",
            (CameraView::Chase, Profile::Desktop) => "AUTO FLIGHT / CHASE / CAMERA EVERY 12S",
            (CameraView::Cockpit, Profile::Desktop) => "AUTO FLIGHT / COCKPIT / CAMERA EVERY 12S",
        };
    }
    if profile == Profile::Compact {
        return match view {
            CameraView::Overhead => "TOP / SPACE",
            CameraView::Chase => "CHASE / SPACE",
            CameraView::Cockpit => "COCKPIT / SPACE",
        };
    }
    match view {
        CameraView::Overhead => {
            "MOVE TO STEER · CLICK TO ROLL · HOLD / SHIFT TO BOOST · SPACE FOR CHASE VIEW"
        }
        CameraView::Chase => {
            "POINT TO TURN, CLIMB AND DIVE · CLICK TO ROLL · HOLD / SHIFT TO BOOST · SPACE FOR COCKPIT"
        }
        CameraView::Cockpit => {
            "ARROWS / WASD TO FLY · CLICK TO ROLL · HOLD / SHIFT TO BOOST · SPACE FOR OVERHEAD"
        }
    }
}
#[doc = " Install the flight scene and its standard keyboard and pointer controls."]
pub fn setup_app<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    parent: Entity,
) -> FlightModelHandle {
    setup_profile(app, parent, Profile::Desktop)
}

pub fn setup_compact_app<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    parent: Entity,
) -> FlightModelHandle {
    setup_profile(app, parent, Profile::Compact)
}

fn setup_profile<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    parent: Entity,
    profile: Profile,
) -> FlightModelHandle {
    let (width, height) = app.viewport().logical_size();
    let model = app
        .add_model(if profile == Profile::Desktop {
            FlightModel::new(width, height)
        } else {
            FlightModel::with_profile(width, height, profile)
        })
        .finish();
    app.add_plugin(SpriteStackPlugin);
    app.with_widget(flight_input::view());
    app.on_event(Key::EVENT, key::callback(model.clone()));
    app.on_event(HostFocusChanged::EVENT, focus::callback(model.clone()));
    app.on_event(
        HostVisibilityChanged::EVENT,
        visibility::callback(model.clone()),
    );
    app.compose(parent, build_widgets(model.clone(), profile));
    model
}
#[cfg(test)]
mod tests {
    use super::*;
    use mirui::core::model::OwnedModel;
    use mirui_sprite_stack::Camera;
    use renderer::FlightOutput;

    fn assert_hint_fits(world: &mirui::ecs::World) {
        let fits = mirui::gallery::demos::text_fits(world);
        assert!(!fits.is_empty());
        for fit in fits {
            assert!(!fit.overflows(0), "{} exceeds its text bounds", fit.text);
        }
    }

    #[mirui::event]
    fn host_key(_event: event!(Key), counter: res!(mut u32)) -> bool {
        *counter += 1;
        true
    }

    #[test]
    fn flight_shares_the_hosts_font_manager_and_only_consumes_applied_keys() {
        let mut app = App::headless(128, 128);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        let fonts = app.world.resource::<FontManager>().unwrap();
        fonts.add_static("host-font", Font::default());
        let original = fonts.resolve(FontToken::Default.cache_key());
        let host_font = fonts.resolve("host-font");
        let model = setup_app(&mut app, root);
        let fonts = app.world.resource::<FontManager>().unwrap();
        assert!(
            fonts
                .resolve(FontToken::Default.cache_key())
                .ptr_eq(&original)
        );
        assert!(!fonts.resolve(FLIGHT_FONT.cache_key()).ptr_eq(&original));
        assert!(fonts.resolve("host-font").ptr_eq(&host_font));

        app.add_resource(0u32);
        app.on_event(Key::EVENT, host_key::callback());
        let mut send = |physical, pressed, repeat| {
            let key = Key {
                physical: Some(physical),
                code: 0,
                pressed,
                modifiers: None,
                repeat: Some(repeat),
            };
            mirui::input::event::dispatch_input(
                &mut app.world,
                Some(root),
                &key.into(),
                1,
                128,
                128,
            );
            *app.world.resource::<u32>().unwrap()
        };
        model.set_autoplay(true);
        assert_eq!(send(PhysicalKey::Space, true, false), 1);
        assert_eq!(model.view(), CameraView::Overhead);
        model.set_autoplay(false);
        assert_eq!(send(PhysicalKey::Space, true, false), 1);
        assert_eq!(model.view(), CameraView::Chase);
        assert_eq!(send(PhysicalKey::Space, true, true), 2);
        assert_eq!(send(PhysicalKey::Space, false, false), 3);
        model.set_visible(false);
        assert_eq!(send(PhysicalKey::Space, true, false), 4);
    }

    #[test]
    fn pointer_uses_the_presented_frame_at_a_fractional_scale_boundary() {
        let dimensions = [Fixed::from_ratio(257, 2), Fixed::from_int(128)];
        let point = [Fixed::from_int(39), Fixed::from_int(64)];
        let (size, pointer) = Profile::Compact.frame_input(dimensions, Some(point));
        assert_eq!(size, [65, 64]);
        assert_eq!(pointer, Some([Fixed::from_int(20), Fixed::from_int(32)]));
    }

    #[test]
    fn additional_scene_viewports_share_one_simulation_step() {
        #[mirui::compose]
        fn minimap(scene: model!(StackScene), player: mirui_sprite_stack::Object) -> Entity {
            ui! {
                StackViewport (
                    width: 16,
                    height: 16,
                    scene: scene,
                    camera: Camera {
                        target: Some(player),
                        range: Fixed::from_int(8),
                        ..Camera::overhead()
                    }
                )
            }
        }
        let mut app = App::headless(40, 64);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        let game = setup_compact_app(&mut app, root);
        app.compose(root, minimap(game.scene(), game.player()));
        app.prepare_text_layout().unwrap();
        app.snapshot().unwrap();
        let rev = game.revision();
        app.tick();
        assert_eq!(game.revision(), rev + 1);
        app.snapshot().unwrap();
        assert_eq!(game.revision(), rev + 1);
    }

    #[test]
    fn moving_and_removing_the_ground_keeps_the_scene_and_cameras_live() {
        use mirui_sprite_stack::Pose;
        let mut app = App::headless(128, 128);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        let game = setup_compact_app(&mut app, root);
        game.key(PhysicalKey::KeyE, true, false);
        for _ in 0..45 {
            game.advance(32, 128, 128, None);
        }
        let scene = game.scene();
        let ground = game.ground();
        let player = game.player();
        let frame = |app: &mut App<_>| app.snapshot().unwrap().unwrap();
        let differs = |a: &mirui::render::texture::Texture, b: &mirui::render::texture::Texture| {
            a.read_pixels()
                .unwrap()
                .bytes()
                .iter()
                .zip(b.read_pixels().unwrap().bytes())
                .any(|(a, b)| a != b)
        };
        let original = frame(&mut app);
        let mut moved = Pose::at(123, 26, 17);
        moved.yaw = Fixed::from_int(90);
        scene.set_pose(ground, moved).unwrap();
        let raised = frame(&mut app);
        assert!(differs(&original, &raised));
        let p = scene.pose(player).unwrap();
        let pos = p.position;
        let expected = scene
            .height_at(
                ground,
                Fixed::from_int(pos[0].trunc_to_int()),
                Fixed::from_int(pos[2].trunc_to_int()),
            )
            .unwrap()
            .unwrap();
        let sampled = game.read(|game| {
            game.terrain
                .at(Some(moved))
                .height(pos[0].trunc_to_int(), -pos[2].trunc_to_int())
        });
        assert_eq!(sampled, expected);
        scene.set_pose(ground, Pose::default()).unwrap();
        assert!(!differs(&original, &frame(&mut app)));
        scene.set_pose(ground, moved).unwrap();
        let rev = game.revision();
        game.advance(32, 128, 128, None);
        assert_eq!(game.revision(), rev + 1);
        assert_ne!(scene.pose(player), Some(p));
        let with_ground = frame(&mut app);
        scene.remove(ground).unwrap();
        let no_ground = frame(&mut app);
        assert!(differs(&with_ground, &no_ground));
        let p = scene.pose(player).unwrap();
        game.select_view(CameraView::Chase);
        for _ in 0..32 {
            game.advance(32, 128, 128, None);
        }
        assert_ne!(scene.pose(player), Some(p));
        assert!(differs(&no_ground, &frame(&mut app)));
        let same_view = frame(&mut app);
        scene.remove(player).unwrap();
        assert!(
            differs(&same_view, &frame(&mut app)),
            "the aircraft was visible with no ground"
        );
    }

    #[test]
    fn lighting_changes_reuse_view_storage() {
        use crate::raster::Daylight;
        for profile in [Profile::Desktop, Profile::Compact] {
            let mut output = FlightOutput::new(profile);
            output.prepare([128, 128], Daylight::Day).unwrap();
            let bytes = output.bytes();
            for day in [Daylight::Sunset, Daylight::Night, Daylight::Day] {
                assert_eq!(
                    crate::test_support::tracked_allocations(|| {
                        let raster = output.prepare([128, 128], day).unwrap();
                        assert_eq!(raster.palette.day, day);
                    }),
                    0
                );
                assert_eq!(output.bytes(), bytes);
            }
        }
    }
    #[test]
    fn autoplay_reuses_flight_and_camera_without_allocating() {
        let mut scene = FlightModel::with_profile(32, 32, Profile::Compact);
        let position = scene.flight.jets[0].position;
        scene.press();
        scene.key(PhysicalKey::ShiftLeft, true, false);
        scene.set_autoplay(true);
        assert!(!scene.flight.controls.holding);
        assert_eq!(scene.flight.controls.keys, 0);
        let allocations = crate::test_support::tracked_allocations(|| {
            for expected in [CameraView::Chase, CameraView::Cockpit, CameraView::Overhead] {
                let before = scene.view();
                for _ in 0..239 {
                    scene.advance(50, 32, 32, Some([Fixed::ONE; 2]));
                }
                assert_eq!(scene.view(), before);
                scene.set_autoplay(true);
                scene.key(PhysicalKey::Space, true, false);
                scene.key(PhysicalKey::ShiftLeft, true, false);
                scene.press();
                scene.advance(50, 32, 32, Some([Fixed::ONE; 2]));
                assert_eq!(scene.view(), expected);
                assert_eq!(scene.view_elapsed_ms, 0);
                assert_eq!(scene.flight.controls.keys, 0);
                assert!(!scene.flight.controls.holding);
                assert!(scene.flight.controls.pointer.is_none());
            }
        });
        assert_eq!(allocations, 0);
        assert_ne!(scene.flight.jets[0].position, position);
        scene.set_autoplay(false);
        scene.key(PhysicalKey::Space, true, false);
        assert_eq!(scene.view(), CameraView::Chase);
        scene.key(PhysicalKey::ShiftLeft, true, false);
        assert_ne!(scene.flight.controls.keys, 0);
    }
    #[test]
    fn autoplay_pauses_and_uses_the_same_clamped_simulation_time() {
        let mut scene = FlightModel::with_profile(32, 32, Profile::Compact);
        scene.set_autoplay(true);
        for _ in 0..239 {
            scene.advance(50, 32, 32, None);
        }
        let time = scene.flight.time;
        let revision = scene.revision;
        scene.set_focus(false);
        scene.advance(u32::MAX, 32, 32, None);
        scene.set_focus(true);
        scene.set_visible(false);
        scene.advance(u32::MAX, 32, 32, None);
        scene.set_visible(true);
        scene.advance(u32::MAX, 0, 128, None);
        assert_eq!(scene.flight.time, time);
        assert_eq!(scene.revision, revision);
        assert_eq!(scene.view(), CameraView::Overhead);
        scene.advance(u32::MAX, 32, 32, None);
        assert_eq!(scene.view(), CameraView::Chase);
        assert_eq!(scene.view_elapsed_ms, 0);
        assert_eq!(scene.flight.time, Fixed::from_int(12));
        let position = scene.flight.jets[0].position;
        scene.set_autoplay(false);
        assert_eq!(scene.view(), CameraView::Chase);
        assert_eq!(scene.flight.jets[0].position, position);
        scene.set_autoplay(true);
        scene.advance(0, 32, 32, None);
        assert_eq!(scene.view_elapsed_ms, 0);
    }
    #[test]
    fn compact_keeps_all_views_and_bounds_workspaces_across_resizes() {
        use super::super::raster::Frame;
        use mirui::render::texture::ColorFormat;

        let mut scene = FlightModel::with_profile(128, 128, Profile::Compact);
        let mut output = FlightOutput::new(Profile::Compact);
        let mut frame = Frame::with_format(128, 128, ColorFormat::RGB565).unwrap();
        scene.scene.read(|shared| {
            output
                .render(&scene, shared, &mut frame, Fixed::ONE)
                .unwrap()
        });
        eprintln!("compact scene bytes: {}", scene.resident_bytes());
        assert!(scene.resident_bytes() < 128 * 1024);
        assert_eq!(Profile::Compact.scene_size(128, 128), (128, 128, 1));
        for (width, height) in [
            (128, 128),
            (160, 128),
            (128, 160),
            (320, 240),
            (64, 64),
            (800, 600),
        ] {
            let (w, h, scale) = Profile::Compact.scene_size(width, height);
            let scale = Fixed::from_int(scale as i32);
            let mut frame = Frame::with_format(w, h, ColorFormat::RGB565).unwrap();
            for view in [CameraView::Overhead, CameraView::Chase, CameraView::Cockpit] {
                scene.select_view(view);
                scene.advance(16, width, height, None);
                scene
                    .scene
                    .read(|shared| output.render(&scene, shared, &mut frame, scale).unwrap());
                let allocations = crate::test_support::tracked_allocations(|| {
                    for _ in 0..8 {
                        scene.advance(16, width, height, None);
                        scene.scene.read(|shared| {
                            output.render(&scene, shared, &mut frame, scale).unwrap()
                        });
                    }
                });
                assert_eq!(allocations, 0);
                assert!(frame.width <= 128 && frame.height <= 128);
                assert!(scene.resident_bytes() < 128 * 1024);
            }
        }
        let mut app = App::headless(128, 128);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        let model = setup_compact_app(&mut app, root);
        for view in [CameraView::Overhead, CameraView::Chase, CameraView::Cockpit] {
            model.select_view(view);
            app.render().unwrap();
            assert_hint_fits(&app.world);
            for (entity, _) in app.world.query::<Text>().iter() {
                let rect = app.world.get::<ComputedRect>(entity).unwrap().0;
                assert!(rect.y >= Fixed::ZERO && rect.y + rect.h <= Fixed::from_int(126));
            }
        }
    }
    #[test]
    fn host_focus_and_key_repeats_do_not_leave_stuck_controls() {
        let mut scene = FlightModel::new(640, 360);
        scene.key(PhysicalKey::Space, true, false);
        assert_eq!(scene.view(), CameraView::Chase);
        scene.key(PhysicalKey::Space, true, true);
        assert_eq!(scene.view(), CameraView::Chase);
        scene.key(PhysicalKey::ShiftLeft, true, false);
        scene.press();
        scene.set_focus(false);
        assert_eq!(scene.flight.controls.keys, 0);
        assert!(!scene.flight.controls.holding);
        let before = scene.flight.time;
        scene.advance(16, 640, 360, None);
        assert_eq!(scene.flight.time, before);
        scene.set_focus(true);
        scene.advance(16, 640, 360, None);
        assert!(scene.flight.time > before);
        assert!(scene.resident_bytes() < 16 * 1024 * 1024);
    }
    #[test]
    fn app_delivers_flight_keys_and_resize_preserves_state() {
        let mut app = App::headless(640, 360);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        let model = setup_app(&mut app, root);
        app.prepare_text_layout().unwrap();
        app.render().unwrap();
        assert_hint_fits(&app.world);
        for (width, height) in [(480, 320), (640, 360), (1280, 720), (720, 960)] {
            let mut preview = App::headless(width, height);
            preview.with_default_widgets().with_default_systems();
            let root = preview.spawn_root().id();
            let state = setup_app(&mut preview, root);
            for view in [CameraView::Overhead, CameraView::Chase, CameraView::Cockpit] {
                state.select_view(view);
                preview.prepare_text_layout().unwrap();
                preview.render().unwrap();
                for (entity, _) in preview.world.query::<Text>().iter() {
                    let rect = preview.world.get::<ComputedRect>(entity).unwrap().0;
                    assert!(rect.x >= Fixed::ZERO && rect.y >= Fixed::ZERO);
                    assert!(rect.x + rect.w <= Fixed::from_int(i32::from(width)));
                    assert!(rect.y + rect.h <= Fixed::from_int(i32::from(height) - 18));
                }
                assert_hint_fits(&preview.world);
            }
        }
        let key = Key {
            physical: Some(PhysicalKey::Space),
            code: mirui::input::event::input::KEY_UNKNOWN,
            pressed: true,
            modifiers: Some(Default::default()),
            repeat: Some(false),
        };
        mirui::input::event::dispatch_input(&mut app.world, Some(root), &key.into(), 1, 640, 360);
        assert_eq!(model.view(), CameraView::Chase);
        let mut scene = FlightModel::new(640, 360);
        scene.select_view(CameraView::Cockpit);
        scene.key(PhysicalKey::KeyT, true, false);
        let day = scene.day;
        scene.advance(16, 960, 600, None);
        assert_eq!(scene.view(), CameraView::Cockpit);
        assert_eq!(scene.day, day);
        assert!(scene.resident_bytes() < 16 * 1024 * 1024);
    }
}
