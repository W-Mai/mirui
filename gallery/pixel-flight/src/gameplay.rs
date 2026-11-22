use crate::{
    Profile,
    flight::{Flight, Settings, View as CameraView},
    presentation::palette::Daylight,
    world::{World as Terrain, coordinates},
};
use core::cell::Cell;
use mirui::{core::cache::Handle, input::event::input::PhysicalKey, prelude::*};
use mirui_sprite_stack::{Pose, StackStage};

#[mirui::model]
pub struct FlightModel {
    pub(crate) profile: Profile,
    pub(crate) terrain: Handle<Terrain>,
    pub(crate) ground_pose: Option<Pose>,
    pub(crate) flight: Flight,
    pub stage: model!(owned StackStage),
    pub(crate) day: Daylight,
    pub(crate) display_bytes: Cell<usize>,
    pub(crate) cockpit_bytes: Cell<usize>,
    pub(crate) revision: u64,
    pub(crate) hint_time: Fixed,
    pub(crate) focused: bool,
    pub(crate) visible: bool,
    pub(crate) autoplay: bool,
    pub(crate) view_elapsed_ms: u16,
}
impl FlightModel {
    pub(crate) fn new(width: u16, height: u16) -> Self {
        Self::with_profile(width, height, Profile::Desktop)
    }
    pub(crate) fn with_profile(_width: u16, _height: u16, profile: Profile) -> Self {
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
                - crate::flight::smooth((self.hint_time - Fixed::from_int(5)) / Fixed::ONE))
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
        coordinates::position(&self.flight.jets[0])
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
    pub(crate) fn handle_key(
        &mut self,
        physical: PhysicalKey,
        pressed: bool,
        repeat: bool,
    ) -> bool {
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

    pub(crate) fn advance_viewport(
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
