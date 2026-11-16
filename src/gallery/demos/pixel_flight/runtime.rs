use super::{
    cockpit::Cockpit,
    flame::Flames,
    flight::{Flight, Settings, View as CameraView},
    raster::{Palette, Raster, scene_size},
    world::World as Terrain,
};
use crate::ecs::DeltaTimeMs;
use crate::input::event::{
    PointerCursor,
    input::{Key, PhysicalKey},
    lifecycle::{HostFocusChanged, HostVisibilityChanged},
    press::{PressCancelReason, PressCancelled, Pressed, Released},
};
use crate::prelude::*;
use crate::render::command::{CompositeMode, DrawCommand};
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};
use crate::ui::{ComputedRect, view::PaintCtx};

#[crate::model]
pub struct FlightModel {
    terrain: Terrain,
    flight: Flight,
    raster: Option<Raster>,
    cockpit: Option<Cockpit>,
    flames: Flames,
    viewport: (u16, u16),
    revision: u32,
    hint_time: f32,
    focused: bool,
    visible: bool,
}
impl FlightModel {
    fn new(width: u16, height: u16) -> Self {
        let terrain = Terrain::new(7, 0.33);
        let flight = Flight::new(&terrain, Settings::default());
        let mut state = Self {
            terrain,
            flight,
            raster: None,
            cockpit: None,
            flames: Flames::new().expect("flight flame storage"),
            viewport: (width, height),
            revision: 0,
            hint_time: 0.0,
            focused: true,
            visible: true,
        };
        state.prepare(width, height);
        state.render_frame();
        state
    }
    fn prepare(&mut self, width: u16, height: u16) {
        let (w, h, _) = scene_size(width, height);
        self.viewport = (width, height);
        if self
            .raster
            .as_ref()
            .is_some_and(|r| r.frame.width == w && r.frame.height == h)
        {
            return;
        }
        // Release the old workspaces before allocating the replacement set.
        let day = self
            .raster
            .as_ref()
            .map(|raster| raster.palette.day)
            .unwrap_or_default();
        self.cockpit = None;
        self.raster = None;
        let mut raster = Raster::new(w, h).expect("bounded flight frame");
        raster.palette = Palette::new(day);
        self.cockpit = Some(Cockpit::new(w, h, &raster.palette));
        self.raster = Some(raster);
    }
    fn render_frame(&mut self) {
        let Some(raster) = &mut self.raster else {
            return;
        };
        raster.render(&self.terrain, &self.flight);
        if let Some(cockpit) = &self.cockpit {
            raster.frame.with_canvas(|canvas| {
                cockpit.draw(canvas, &self.terrain, &self.flight, &raster.palette)
            });
        }
        let (_, _, scale) = scene_size(self.viewport.0, self.viewport.1);
        self.flames
            .prepare(
                &raster.flames[..raster.flame_count],
                self.flight.time,
                scale as f32,
                [raster.frame.width, raster.frame.height],
            )
            .expect("prepared flame textures");
        self.revision = self.revision.wrapping_add(1);
    }
}

#[crate::model]
impl FlightModel {
    #[observe]
    pub fn revision(&self) -> u32 {
        self.revision
    }
    #[observe]
    pub fn hint_color(&self) -> Color {
        Color {
            r: 255,
            g: 255,
            b: 255,
            a: ((1.0 - super::flight::smooth((self.hint_time - 5.0) / 1.0)) * 220.0) as u8,
        }
    }
    #[observe]
    pub fn view(&self) -> CameraView {
        self.flight.view
    }
    pub fn select_view(&mut self, view: CameraView) {
        self.flight.view = view;
        self.hint_time = 0.0;
    }
    pub fn cycle_view(&mut self) {
        self.flight.cycle_view();
        self.hint_time = 0.0;
    }
    pub fn press(&mut self) {
        self.flight.controls.press();
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
                self.flight.intro = 0.0;
                self.flight.burn = 0.0;
            }
            PhysicalKey::KeyE => {
                self.flight.settings.spread = if self.flight.settings.spread > 1.0 {
                    1.0
                } else {
                    5.0
                };
            }
            PhysicalKey::KeyF => {
                self.flight.set_wingmen(self.flight.count % 3);
            }
            PhysicalKey::KeyT => {
                if let Some(raster) = &mut self.raster {
                    self.cockpit = None;
                    raster.palette = Palette::new(raster.palette.day.next());
                    self.cockpit = Some(Cockpit::new(
                        raster.frame.width,
                        raster.frame.height,
                        &raster.palette,
                    ));
                }
            }
            _ => {}
        }
    }
    pub fn advance(
        &mut self,
        milliseconds: u32,
        width: u16,
        height: u16,
        pointer: Option<[f32; 2]>,
    ) {
        if width == 0 || height == 0 || !self.focused || !self.visible {
            return;
        }
        self.prepare(width, height);
        let (scene_width, scene_height, scale) = scene_size(width, height);
        if let Some(point) = pointer {
            self.flight.controls.point([
                (point[0]
                    - ((f32::from(width) - (scene_width * scale) as f32) / 2.0 + 0.5).floor())
                    / scale as f32,
                (point[1]
                    - ((f32::from(height) - (scene_height * scale) as f32) / 2.0 + 0.5).floor())
                    / scale as f32,
            ]);
        } else {
            self.flight.controls.pointer = None;
            self.flight.release_pointer(true);
        }
        let raster = self.raster.as_ref().expect("prepared flight raster");
        self.flight.step(
            &self.terrain,
            milliseconds as f32 / 1000.0,
            raster.frame.width,
            raster.frame.height,
        );
        self.hint_time += milliseconds.min(50) as f32 / 1000.0;
        self.render_frame();
    }
    pub fn paint(&self, paint: &mut PaintCtx<'_>) {
        let Some(raster) = &self.raster else {
            return;
        };
        let rect = paint.rect();
        let (_, _, scale) = scene_size(self.viewport.0, self.viewport.1);
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
        self.flames.paint(paint, origin, scale);
    }
    pub fn resident_bytes(&self) -> usize {
        self.terrain.bytes()
            + core::mem::size_of::<Self>()
            + self.raster.as_ref().map_or(0, Raster::bytes)
            + self.cockpit.as_ref().map_or(0, Cockpit::bytes)
            + self.flames.bytes()
    }
}

#[crate::component]
pub(super) struct Stage {
    pub model: model!(FlightModel),
}

#[crate::view(component=Stage,name="Pixel Flight",priority=60)]
fn flight_scene(model: model!(component.model, watch(revision)), paint: paint!()) {
    paint.set_background_handled(true);
    model.paint(paint);
}

#[crate::system(order=Animation)]
fn animate(
    stages: query!(Stage, ComputedRect),
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
            .then(|| [(cursor.x - rect.x).to_f32(), (cursor.y - rect.y).to_f32()]);
        stage.model.advance(
            delta.0.into(),
            rect.w.to_int().clamp(0, 65535) as u16,
            rect.h.to_int().clamp(0, 65535) as u16,
            pointer,
        );
    }
}
#[crate::event]
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
#[crate::event]
fn focus(event: event!(HostFocusChanged), model: model!(FlightModel)) {
    model.set_focus(event.focused);
}
#[crate::event]
fn visibility(event: event!(HostVisibilityChanged), model: model!(FlightModel)) {
    model.set_visible(event.visible);
}
#[crate::event]
fn press(_event: event!(Pressed), model: model!(FlightModel)) {
    model.press();
}
#[crate::event]
fn release(_event: event!(Released), model: model!(FlightModel)) {
    model.release(false);
}
#[crate::event]
fn cancel(event: event!(PressCancelled), model: model!(FlightModel)) {
    if event.reason != PressCancelReason::Gesture {
        model.release(true);
    }
}

#[crate::compose]
fn build_widgets(model: model!(FlightModel)) {
    ui! {
        View (
            grow: 1.0,
            clip_children: true,
            direction: FlexDirection::Column,
            justify: JustifyContent::FlexEnd,
            padding: Padding {
                bottom: Dimension::px(12),
                ..Padding::all(0)
            }
        ) [
            Stage { model: model.clone() },
            crate::input::event::scroll::TouchAction::None,
        ] {
            Column (width: Dimension::percent(100), padding: Padding::all(6)) [
                crate::ui::IgnoreHitTest,
            ] {
                Text (
                    ${ hint(model.view()) },
                    font_size: 8,
                    text_color: ${ model.hint_color() },
                    paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
                )
            }
        } on Pressed(press::callback(model.clone())) on Released(release::callback(model.clone())) on PressCancelled(cancel::callback(model))
    };
}

fn hint(view: CameraView) -> &'static str {
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

/// Install the flight scene and its standard keyboard and pointer controls.
pub fn setup_app<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    parent: Entity,
) -> FlightModelHandle {
    app.compose(parent, crate::gallery::demos::clock::ensure_clock());
    let (width, height) = app.backend.viewport().logical_size();
    let model = app.add_model(FlightModel::new(width, height)).finish();
    app.with_widget(flight_scene::view());
    app.add_system(animate::system());
    app.on_event(Key::EVENT, key::callback(model.clone()));
    app.on_event(HostFocusChanged::EVENT, focus::callback(model.clone()));
    app.on_event(
        HostVisibilityChanged::EVENT,
        visibility::callback(model.clone()),
    );
    app.compose(parent, build_widgets(model.clone()));
    model
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let key = crate::test_support::physical_key(PhysicalKey::Space);
        crate::input::event::deliver_input(&mut app.world, Some(root), &key.into(), 1, 640, 360);
        assert_eq!(model.view(), CameraView::Chase);
        let mut scene = FlightModel::new(640, 360);
        scene.select_view(CameraView::Cockpit);
        scene.key(PhysicalKey::KeyT, true, false);
        let day = scene.raster.as_ref().unwrap().palette.day;
        scene.advance(16, 960, 600, None);
        assert_eq!(scene.view(), CameraView::Cockpit);
        assert_eq!(scene.raster.as_ref().unwrap().palette.day, day);
        assert!(scene.resident_bytes() < 16 * 1024 * 1024);
    }
}
