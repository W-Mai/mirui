use super::{
    Profile,
    flight::{Flight, Settings, View as CameraView},
    raster::Daylight,
    world::World as Terrain,
};
use core::cell::Cell;
use mirui_sprite_stack::{Pose, SpriteStackPlugin, StackStage, StackViewport};
mod objects;
mod renderer;
mod stage_members;
use renderer::{cockpit_layer, flight_content};

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
    ground_pose: Option<Pose>,
    flight: Flight,
    pub stage: model!(owned StackStage),
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
        Self {
            profile,
            terrain: Handle::new(terrain),
            ground_pose: Some(Pose::default()),
            flight,
            stage: StackStage::bounded(8).into(),
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
    /// Current player position in the stage's Y-up coordinates.
    #[observe]
    pub fn player_position(&self) -> [Fixed; 3] {
        objects::position(&self.flight.jets[0])
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
                self.revision = self.revision.wrapping_add(1);
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
            self.terrain.at(self.ground_pose),
            milliseconds,
            scene_width,
            scene_height,
        );
        self.hint_time += Fixed::from_int(milliseconds.into()) / Fixed::from_int(1000);
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
    observed_members: Cell<u8>,
}
#[mirui::view(component = FlightInput, name = "Flight controls",systems = [animate::system(), stage_members::sync::system()])]
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
    let content = flight_content(model.clone(), profile);
    let cockpit = cockpit_layer(model.clone(), profile);

    ui! {
        StackViewport (
            stage: model.stage(),
            resolution: profile.resolution(),
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
                observed_members: Cell::new(0),
            },
            content,
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
    if profile == Profile::Compact {
        app.require_text_layout_capacity(mirui::text::TextLayoutCapacity {
            layout_slots: 3,
            measurements: 8,
            lines: 6,
            runs: 6,
            glyphs: 32,
            carets: 40,
            workspace: mirui::text::WorkspaceCapacity {
                runs: 4,
                glyphs: 16,
                scratch_glyphs: 16,
                lines: 4,
            },
        })
        .expect("register compact flight hint layout");
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
#[path = "../tests/unit/runtime.rs"]
mod tests;
