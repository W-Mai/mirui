use super::{
    Profile,
    cockpit::Cockpit,
    flame::Flames,
    flight::{Flight, Settings, View as CameraView},
    raster::{Palette, Raster},
    world::World as Terrain,
};
use core::cell::Cell;
use mirui::core::model::{ModelHandle, OwnedModel};
use mirui::render::RenderError;
use mirui_sprite_stack::{Camera, ScenePainter, SpriteStackPlugin, StackScene, StackViewport};
mod objects;
use objects::FlightObjects;

use mirui::ecs::DeltaTimeMs;
use mirui::input::event::{
    PointerCursor,
    input::{Key, PhysicalKey},
    lifecycle::{HostFocusChanged, HostVisibilityChanged},
    press::{PressCancelReason, PressCancelled, Pressed, Released},
};
use mirui::prelude::*;
use mirui::render::command::{CompositeMode, DrawCommand};
use mirui::render::font::{Font, FontManager};
use mirui::ui::widgets::{ParagraphStyle, Text, TextAlign};
use mirui::ui::{ComputedRect, view::PaintCtx};
#[mirui::model]
pub struct FlightModel {
    profile: Profile,
    terrain: Terrain,
    flight: Flight,
    pub scene: model!(owned StackScene),
    objects: FlightObjects,
    day: super::raster::Daylight,
    display_bytes: Cell<usize>,
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
            terrain,
            flight,
            scene: scene.into(),
            objects,
            display_bytes: Cell::new(0),
            day: super::raster::Daylight::default(),
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
    /// Flight dynamics and cloud sampling share a fixed terrain origin. Removing or moving this ground suspends the flight simulation.
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
        if self.autoplay {
            return;
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
            return;
        }
        if !pressed || repeat {
            return;
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
            _ => {}
        }
    }
    pub fn advance(
        &mut self,
        milliseconds: u32,
        width: u16,
        height: u16,
        pointer: Option<[Fixed; 2]>,
    ) {
        if width == 0 || height == 0 || !self.focused || !self.visible {
            return;
        }
        if !self
            .scene
            .read(|scene| self.objects.supports_terrain(scene, &self.terrain))
        {
            return;
        }
        let (scene_width, scene_height, scale) = self.profile.scene_size(width, height);
        let milliseconds = milliseconds.min(50) as u16;
        if self.autoplay {
            self.view_elapsed_ms += milliseconds;
            if self.view_elapsed_ms >= 12_000 {
                self.view_elapsed_ms -= 12_000;
                self.cycle_view();
            }
        } else if let Some(point) = pointer {
            self.flight.controls.point([
                (point[0]
                    - ((Fixed::from_int((width) as i32)
                        - Fixed::from_int((scene_width * scale) as i32))
                        / Fixed::from_int(2)
                        + Fixed::from_ratio(5, 10))
                    .floor())
                    / Fixed::from_int((scale) as i32),
                (point[1]
                    - ((Fixed::from_int((height) as i32)
                        - Fixed::from_int((scene_height * scale) as i32))
                        / Fixed::from_int(2)
                        + Fixed::from_ratio(5, 10))
                    .floor())
                    / Fixed::from_int((scale) as i32),
            ]);
        } else {
            self.flight.controls.pointer = None;
            self.flight.release_pointer(true);
        }
        self.flight
            .step(&self.terrain, milliseconds, scene_width, scene_height);
        self.hint_time += Fixed::from_int(milliseconds.into()) / Fixed::from_int(1000);
        self.objects.sync(&mut self.scene, &mut self.flight);
        self.scene.invalidate();
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn resident_bytes(&self) -> usize {
        self.terrain.bytes() + core::mem::size_of::<Self>() + self.flight.bytes()
            - core::mem::size_of::<Flight>()
            + self.display_bytes.get()
    }
}
struct FlightOutput {
    profile: Profile,
    raster: Option<Raster>,
    cockpit: Option<Cockpit>,
    flames: Option<Flames>,
    viewport: (u16, u16),
    cache_revision: u64,
    last: Option<u64>,
}
impl FlightOutput {
    fn new(profile: Profile) -> Self {
        Self {
            profile,
            raster: None,
            cockpit: None,
            flames: if profile == Profile::Desktop {
                Some(Flames::new().expect("flight flame storage"))
            } else {
                None
            },
            viewport: (0, 0),
            cache_revision: 0,
            last: None,
        }
    }
    fn prepare(&mut self, width: u16, height: u16, day: super::raster::Daylight) {
        let (w, h, _) = self.profile.scene_size(width, height);
        self.viewport = (width, height);
        if self
            .raster
            .as_ref()
            .is_some_and(|r| r.frame.width == w && r.frame.height == h && r.palette.day == day)
        {
            return;
        }
        let seen = self
            .raster
            .as_ref()
            .and_then(|r| ((r.frame.width, r.frame.height) == (w, h)).then_some(r.seen));
        self.rebuild(w, h, day);
        if let Some(seen) = seen {
            self.raster.as_mut().expect("prepared raster").seen = seen;
        }
        self.last = None;
    }
    fn rebuild(&mut self, width: usize, height: usize, day: super::raster::Daylight) {
        self.cockpit = None;
        self.raster = None;
        let palette = Palette::new(day);
        let cockpit = match self.profile {
            Profile::Desktop => Cockpit::new(width, height, &palette),
            Profile::Compact => Cockpit::new_compact(width, height, &palette),
        };
        let mut raster = match self.profile {
            Profile::Desktop => Raster::new(width, height),
            Profile::Compact => Raster::new_compact(width, height),
        }
        .expect("bounded flight frame");
        raster.palette = palette;
        self.cockpit = Some(cockpit);
        self.raster = Some(raster);
    }
    fn render_frame(&mut self, model: &FlightModel, scene: &StackScene, stamp: u64) {
        let Some(raster) = &mut self.raster else {
            return;
        };
        if let Some(cockpit) = &self.cockpit {
            let placed = model.objects.placed(scene);
            raster.render_with_cockpit(&model.terrain, &model.flight, cockpit, &placed);
        }
        let (_, _, scale) = self.profile.scene_size(self.viewport.0, self.viewport.1);
        if let Some(flames) = &mut self.flames {
            flames
                .prepare(
                    &raster.flames[..raster.flame_count],
                    model.flight.time,
                    Fixed::from_int((scale) as i32),
                    [raster.frame.width, raster.frame.height],
                )
                .expect("prepared flame textures");
        }
        self.cache_revision = self.cache_revision.wrapping_add(1);
        raster.frame.texture.cache_revision = self.cache_revision;
        self.last = Some(stamp);
        model.display_bytes.set(self.bytes());
    }
    fn paint(&self, paint: &mut PaintCtx<'_>) {
        let Some(raster) = &self.raster else {
            return;
        };
        let rect = paint.rect();
        let (_, _, scale) = self.profile.scene_size(self.viewport.0, self.viewport.1);
        let scale = Fixed::from_int(scale as i32);
        let width = Fixed::from_int(raster.frame.width as i32) * scale;
        let height = Fixed::from_int(raster.frame.height as i32) * scale;
        let origin = Point::new(
            rect.x + ((rect.w - width) / 2).round(),
            rect.y + ((rect.h - height) / 2).round(),
        );
        paint.draw(&DrawCommand::Blit {
            pos: origin,
            size: Point::new(width, height),
            transform: paint.transform(),
            quad: None,
            texture: &raster.frame.texture,
            opa: 255,
            radius: Fixed::ZERO,
            composite: CompositeMode::SourceOver,
        });
        if let Some(flames) = &self.flames {
            flames.paint(paint, origin, scale);
        }
    }

    fn bytes(&self) -> usize {
        core::mem::size_of::<Self>()
            + self
                .raster
                .as_ref()
                .map_or(0, |raster| raster.bytes() - core::mem::size_of::<Raster>())
            + self.cockpit.as_ref().map_or(0, |cockpit| {
                cockpit.bytes() - core::mem::size_of::<Cockpit>()
            })
            + self
                .flames
                .as_ref()
                .map_or(0, |flames| flames.bytes() - core::mem::size_of::<Flames>())
    }
    fn update(
        &mut self,
        model: &FlightModel,
        scene: &StackScene,
        stamp: u64,
        width: u16,
        height: u16,
    ) {
        self.prepare(width, height, model.day);
        if self.last != Some(stamp) {
            self.render_frame(model, scene, stamp);
        }
    }
}
struct FlightPainter {
    model: FlightModelHandle,
    output: FlightOutput,
}
impl ScenePainter for FlightPainter {
    fn paint(
        &mut self,
        scene: &StackScene,
        _camera: Camera,
        stamp: u64,
        paint: &mut PaintCtx<'_>,
    ) -> Result<(), RenderError> {
        let rect = paint.rect();
        let (w, h) = (
            rect.w.to_int().clamp(0, 65535) as u16,
            rect.h.to_int().clamp(0, 65535) as u16,
        );
        if w == 0 || h == 0 {
            return Ok(());
        }
        self.model.read(|model| {
            if model.objects.supports_terrain(scene, &model.terrain) {
                self.output.update(model, scene, stamp, w, h);
            }
        });
        if self.output.raster.is_some() {
            paint.set_background_handled(true);
            self.output.paint(paint);
        }
        Ok(())
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
        stage.model.advance(
            delta.0.into(),
            rect.w.to_int().clamp(0, 65535) as u16,
            rect.h.to_int().clamp(0, 65535) as u16,
            pointer,
        );
    }
}
#[mirui::event]
fn key(event: event!(Key), model: model!(FlightModel)) -> bool {
    let Some(physical) = event.physical.filter(|key| {
        matches!(
            key,
            PhysicalKey::ArrowLeft
                | PhysicalKey::ArrowRight
                | PhysicalKey::ArrowUp
                | PhysicalKey::ArrowDown
                | PhysicalKey::KeyA
                | PhysicalKey::KeyD
                | PhysicalKey::KeyW
                | PhysicalKey::KeyS
                | PhysicalKey::ShiftLeft
                | PhysicalKey::ShiftRight
                | PhysicalKey::Space
                | PhysicalKey::Digit1
                | PhysicalKey::Digit2
                | PhysicalKey::Digit3
                | PhysicalKey::KeyR
                | PhysicalKey::KeyE
                | PhysicalKey::KeyF
                | PhysicalKey::KeyT
        )
    }) else {
        return false;
    };
    model.key(physical, event.pressed, event.repeat == Some(true));
    true
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
    if !res!(optional MonoClock).with(|clock| clock.is_some()) {
        res!().insert(MonoClock::from_time_source());
    }
    let viewport = StackViewport::with_painter(
        model.scene(),
        Camera::default(),
        FlightPainter {
            model: model.clone(),
            output: FlightOutput::new(profile),
        },
    );

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
            FlightInput { model: model.clone() },
            viewport,
            mirui::input::event::scroll::TouchAction::None,
        ] {
            Text (
                ${ hint(model.view(), profile, model.autoplay()) },
                width: Dimension::percent(100),
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
    if profile == Profile::Desktop {
        let font = Font::from_mirx(
            "Flight UI",
            12,
            include_bytes!("../../../src/gallery/demos/assets/nearby-menu.mirx"),
            &mirx::reader::PayloadLimits::HOST,
        )
        .expect("bundled pixel UI font");
        app.with_fonts(FontManager::new(mirui::core::cache::MaxSize::Unbound, font));
    }
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

    fn assert_hint_fits(world: &mirui::ecs::World) {
        let fits = mirui::gallery::demos::text_fits(world);
        assert!(!fits.is_empty());
        for fit in fits {
            assert!(!fit.overflows(0), "{} exceeds its text bounds", fit.text);
        }
    }

    #[test]
    fn additional_scene_viewports_share_one_simulation_step() {
        #[mirui::compose]
        fn minimap(scene: model!(StackScene), player: mirui_sprite_stack::Object) -> Entity {
            ui! { StackViewport(width:16,height:16,scene:scene,camera:Camera{target:Some(player),range:Fixed::from_int(8),..Camera::overhead()}) }
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
    fn losing_the_flight_ground_holds_the_last_view_without_blocking_rendering() {
        let mut app = App::headless(32, 32);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        let game = setup_compact_app(&mut app, root);
        let original = app.snapshot().unwrap().unwrap();
        let rev = game.revision();
        let ground = game.ground();
        game.scene()
            .set_pose(ground, mirui_sprite_stack::Pose::at(1, 0, 0))
            .unwrap();
        game.advance(16, 32, 32, None);
        assert_eq!(game.revision(), rev);
        app.render_dirty().unwrap();
        let frozen = app.snapshot().unwrap().unwrap();
        assert_eq!(
            original.read_pixels().unwrap().bytes(),
            frozen.read_pixels().unwrap().bytes()
        );
        game.scene()
            .set_pose(ground, mirui_sprite_stack::Pose::default())
            .unwrap();
        game.advance(16, 32, 32, None);
        app.render_dirty().unwrap();
        assert_eq!(game.revision(), rev + 1);
        game.scene().remove(ground).unwrap();
        game.advance(16, 32, 32, None);
        app.render_dirty().unwrap();
        assert_eq!(game.revision(), rev + 1);
    }
    #[test]
    fn output_revisions_survive_storage_rebuilds() {
        let mut scene = FlightModel::with_profile(128, 128, Profile::Compact);
        let mut output = FlightOutput::new(Profile::Compact);
        let mut previous = 0;
        for (width, height) in [(128, 128), (320, 240), (128, 128), (128, 160)] {
            for _ in 0..3 {
                scene.key(PhysicalKey::KeyT, true, false);
                scene.advance(16, width, height, None);
                scene
                    .scene
                    .read(|shared| output.update(&scene, shared, scene.revision, width, height));
                let revision = output.raster.as_ref().unwrap().frame.texture.cache_revision;
                assert!(revision > previous);
                previous = revision;
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
        let mut scene = FlightModel::with_profile(128, 128, Profile::Compact);
        let mut output = FlightOutput::new(Profile::Compact);
        scene
            .scene
            .read(|shared| output.update(&scene, shared, scene.revision, 128, 128));
        eprintln!("compact scene bytes: {}", scene.resident_bytes());
        assert!(scene.resident_bytes() < 128 * 1024);
        assert_eq!(Profile::Compact.scene_size(128, 128), (128, 128, 1));
        assert_eq!(
            (
                output.raster.as_ref().unwrap().frame.width,
                output.raster.as_ref().unwrap().frame.height
            ),
            (128, 128)
        );
        for (width, height) in [
            (128, 128),
            (160, 128),
            (128, 160),
            (320, 240),
            (64, 64),
            (800, 600),
        ] {
            for view in [CameraView::Overhead, CameraView::Chase, CameraView::Cockpit] {
                scene.select_view(view);
                scene.advance(16, width, height, None);
                scene
                    .scene
                    .read(|shared| output.update(&scene, shared, scene.revision, width, height));
                let allocations = crate::test_support::tracked_allocations(|| {
                    for _ in 0..8 {
                        scene.advance(16, width, height, None);
                        scene.scene.read(|shared| {
                            output.update(&scene, shared, scene.revision, width, height)
                        });
                    }
                });
                assert_eq!(allocations, 0);
                let raster = output.raster.as_ref().unwrap();
                assert!(raster.frame.width <= 128 && raster.frame.height <= 128);
                assert!(scene.resident_bytes() < 128 * 1024);
                assert!(
                    raster
                        .frame
                        .texture
                        .read_pixels()
                        .unwrap()
                        .direct_format()
                        .unwrap()
                        == mirui::render::texture::ColorFormat::RGB565
                );
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
