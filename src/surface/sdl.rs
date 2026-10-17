use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;
use std::time::Instant;

use sdl2::EventPump;
use sdl2::event::Event;
use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect as SdlRect;
use sdl2::render::{Canvas, Texture as SdlTexture, TextureCreator};
use sdl2::video::{Window, WindowContext};

use super::{DisplayInfo, FramebufferAccess, InputEvent, Surface, logical_from_physical};
use crate::render::texture::{ColorFormat, Texture};
use crate::types::Fixed;

/// macOS trackpad pinch / rotate is delivered by SDL as `MultiGesture`,
/// which has no "end" sentinel; if no `MultiGesture` arrives within
/// this window we synthesize PointerUp for the two virtual fingers.
/// 50ms ≈ 3 frames at 60Hz — long enough to bridge a frame skip,
/// short enough that the gesture clearly ends when the user lifts.
const MULTI_GESTURE_TIMEOUT_MS: u128 = 50;

/// Half-distance between the two virtual fingers when the gesture
/// starts. The recognizer only sees relative scale, so the absolute
/// value doesn't matter for correctness — but a too-small initial
/// puts both virtual fingers on the same pixel and the
/// `initial_dist` clamp kicks in. 5% of min(w, h) is comfortably
/// above any reasonable subpixel rounding.
const INITIAL_DIST_FRAC: f32 = 0.05;

const VIRT_FINGER_A: u8 = 1;
const VIRT_FINGER_B: u8 = 2;

#[derive(Default)]
struct MultiGestureState {
    active: bool,
    f_a: (f32, f32),
    f_b: (f32, f32),
    last_event: Option<Instant>,
}

pub struct SdlSurface {
    #[cfg(target_os = "macos")]
    menu: Option<super::native_menu::SdlMenu>,
    #[cfg(target_os = "linux")]
    clipboard: super::host_clipboard::linux::Clipboard,
    /// Streaming texture sized to the framebuffer; `flush` updates
    /// per-rect, `end_flush` does the single copy + present. `'static`
    /// because the backing `TextureCreator` is `Box::leak`ed in `new`
    /// — one leak per `SdlSurface` (typically one per program), the
    /// OS reclaims it at exit. Avoids a self-referential struct.
    ///
    /// Declared before `canvas` so it's dropped first: SDL textures
    /// must be destroyed before the renderer that created them, and
    /// Rust drops struct fields in declaration order.
    tex: SdlTexture<'static>,
    texture_creator: &'static TextureCreator<WindowContext>,
    texture_lost: bool,
    canvas: Canvas<Window>,
    pending_present: bool,
    needs_redraw: bool,
    _event_pump: EventPump,
    text_input: super::sdl_events::NativeTextInput,
    buf: Vec<u8>,
    width: u16,
    height: u16,
    scale: Fixed,
    /// SDL `MouseWheel` doesn't carry cursor coordinates — cache them
    /// from `MouseMotion` so the forwarded `Wheel` has an anchor.
    last_mouse_x: i32,
    last_mouse_y: i32,
    multi: MultiGestureState,
    /// Each SDL event can expand into multiple mirui events
    /// (MultiGesture → 2 PointerDowns / 2 PointerMoves; timeout → 2
    /// PointerUps). Surface returns one per `poll_event` call, so
    /// queue the rest.
    pending: VecDeque<InputEvent>,
}

impl SdlSurface {
    pub fn new(title: &str, width: u16, height: u16) -> Self {
        Self::new_with_vsync(title, width, height, true)
    }

    pub fn new_with_vsync(title: &str, width: u16, height: u16, vsync: bool) -> Self {
        let sdl = super::sdl_events::init().expect("SDL2 init failed");
        let video = sdl.video().expect("SDL2 video init failed");
        let window = video
            .window(title, width as u32, height as u32)
            .position_centered()
            .allow_highdpi()
            .build()
            .expect("SDL2 window creation failed");
        let mut canvas_builder = window.into_canvas();
        if vsync {
            canvas_builder = canvas_builder.present_vsync();
        }
        let canvas = canvas_builder.build().expect("SDL2 canvas failed");
        let texture_creator: &'static TextureCreator<WindowContext> =
            Box::leak(Box::new(canvas.texture_creator()));
        let event_pump = sdl.event_pump().expect("SDL2 event pump failed");
        let text_input = super::sdl_events::NativeTextInput::new(
            &video,
            sdl.event().expect("SDL2 event subsystem failed"),
        );
        #[cfg(target_os = "macos")]
        let menu = super::native_menu::SdlMenu::new(
            canvas.window(),
            sdl.event().expect("SDL2 event subsystem failed"),
        )
        .expect("SDL2 native menu registration failed");

        let (draw_w, _) = canvas.output_size().unwrap();
        let scale_int = (draw_w as u16) / width;
        let scale_int = if scale_int == 0 { 1 } else { scale_int };
        let scale = Fixed::from(scale_int);

        // Physical pixel framebuffer
        let phys_w = width * scale_int;
        let phys_h = height * scale_int;
        let buf = vec![0u8; phys_w as usize * phys_h as usize * 4];

        sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "0");
        let tex = texture_creator
            .create_texture_streaming(PixelFormatEnum::RGBA32, phys_w as u32, phys_h as u32)
            .expect("texture creation failed");

        #[cfg(target_os = "linux")]
        let clipboard = {
            let mut clipboard = super::host_clipboard::linux::Clipboard::default();
            // The clipboard field is dropped before the canvas and its native window.
            unsafe {
                clipboard.attach(canvas.window());
            }
            clipboard
        };

        Self {
            #[cfg(target_os = "macos")]
            menu,
            tex,
            texture_creator,
            texture_lost: false,
            canvas,
            pending_present: false,
            needs_redraw: true,
            _event_pump: event_pump,
            text_input,
            #[cfg(target_os = "linux")]
            clipboard,
            buf,
            width: phys_w,
            height: phys_h,
            scale,
            last_mouse_x: 0,
            last_mouse_y: 0,
            multi: MultiGestureState::default(),
            pending: VecDeque::new(),
        }
    }

    pub fn scale_factor(&self) -> Fixed {
        self.scale
    }

    fn handle_multi_gesture(
        &mut self,
        cx: f32,
        cy: f32,
        d_theta: f32,
        d_dist: f32,
        win_w: f32,
        win_h: f32,
    ) {
        if !self.multi.active {
            // Place the virtual fingers symmetrically on the horizontal
            // axis through the gesture center. Absolute orientation is
            // arbitrary — the recognizer only sees relative motion.
            let half = INITIAL_DIST_FRAC * win_w.min(win_h) / 2.0;
            self.multi.f_a = (cx - half, cy);
            self.multi.f_b = (cx + half, cy);
            self.multi.active = true;
            self.multi.last_event = Some(Instant::now());

            let (ax, ay) = self.multi.f_a;
            let (bx, by) = self.multi.f_b;
            self.pending.push_back(InputEvent::PointerDown {
                kind: crate::input::event::input::PointerKind::Touch,
                id: VIRT_FINGER_A,
                x: Fixed::from(ax as i32),
                y: Fixed::from(ay as i32),
            });
            self.pending.push_back(InputEvent::PointerDown {
                kind: crate::input::event::input::PointerKind::Touch,
                id: VIRT_FINGER_B,
                x: Fixed::from(bx as i32),
                y: Fixed::from(by as i32),
            });
            return;
        }

        rotate_scale_around(&mut self.multi.f_a, cx, cy, d_theta, 1.0 + d_dist);
        rotate_scale_around(&mut self.multi.f_b, cx, cy, d_theta, 1.0 + d_dist);
        // The recenter step keeps the midpoint consistent with the SDL
        // event's (x, y), preventing slow drift when finger pairs walk
        // across the trackpad.
        let mid = (
            (self.multi.f_a.0 + self.multi.f_b.0) / 2.0,
            (self.multi.f_a.1 + self.multi.f_b.1) / 2.0,
        );
        let dx = cx - mid.0;
        let dy = cy - mid.1;
        self.multi.f_a.0 += dx;
        self.multi.f_a.1 += dy;
        self.multi.f_b.0 += dx;
        self.multi.f_b.1 += dy;

        self.multi.last_event = Some(Instant::now());

        let (ax, ay) = self.multi.f_a;
        let (bx, by) = self.multi.f_b;
        self.pending.push_back(InputEvent::PointerMove {
            kind: crate::input::event::input::PointerKind::Touch,
            id: VIRT_FINGER_A,
            x: Fixed::from(ax as i32),
            y: Fixed::from(ay as i32),
        });
        self.pending.push_back(InputEvent::PointerMove {
            kind: crate::input::event::input::PointerKind::Touch,
            id: VIRT_FINGER_B,
            x: Fixed::from(bx as i32),
            y: Fixed::from(by as i32),
        });
    }

    fn end_multi_gesture(&mut self) {
        if !self.multi.active {
            return;
        }
        let (ax, ay) = self.multi.f_a;
        let (bx, by) = self.multi.f_b;
        self.pending.push_back(InputEvent::PointerUp {
            kind: crate::input::event::input::PointerKind::Touch,
            id: VIRT_FINGER_A,
            x: Fixed::from(ax as i32),
            y: Fixed::from(ay as i32),
        });
        self.pending.push_back(InputEvent::PointerUp {
            kind: crate::input::event::input::PointerKind::Touch,
            id: VIRT_FINGER_B,
            x: Fixed::from(bx as i32),
            y: Fixed::from(by as i32),
        });
        self.multi.active = false;
        self.multi.last_event = None;
    }
}

fn rotate_scale_around(p: &mut (f32, f32), cx: f32, cy: f32, theta: f32, scale: f32) {
    let dx = p.0 - cx;
    let dy = p.1 - cy;
    let s = theta.sin();
    let c = theta.cos();
    let rx = (dx * c - dy * s) * scale;
    let ry = (dx * s + dy * c) * scale;
    p.0 = cx + rx;
    p.1 = cy + ry;
}

impl crate::core::cache::InspectCaches for SdlSurface {}

impl Surface for SdlSurface {
    fn display_info(&self) -> DisplayInfo {
        let (lw, lh) = logical_from_physical(self.width, self.height, self.scale);
        DisplayInfo {
            width: lw,
            height: lh,
            scale: self.scale,
            format: ColorFormat::RGBA8888,
        }
    }

    fn physical_size(&self) -> (u32, u32) {
        (self.width as u32, self.height as u32)
    }

    fn begin_flush(&mut self) -> Result<(), crate::render::RenderError> {
        if self.texture_lost {
            self.tex = self
                .texture_creator
                .create_texture_streaming(
                    PixelFormatEnum::RGBA32,
                    u32::from(self.width),
                    u32::from(self.height),
                )
                .map_err(|_| crate::render::RenderError::BackendFailure)?;
            self.texture_lost = false;
        }
        self.pending_present = false;
        Ok(())
    }

    fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    fn flush(
        &mut self,
        area: crate::types::PhysicalRect,
    ) -> Result<(), crate::render::RenderError> {
        // SDL `Texture::update` requires the rect to be inside the
        // texture, so clip first.
        let fx0 = i32::from(area.x());
        let fy0 = i32::from(area.y());
        let fx1 = i32::from(area.right().min(self.width));
        let fy1 = i32::from(area.bottom().min(self.height));
        if fx1 <= fx0 || fy1 <= fy0 {
            return Ok(());
        }
        let stride = self.width as usize * 4;
        let row_off = fy0 as usize * stride + fx0 as usize * 4;
        let row_w = (fx1 - fx0) as usize * 4;
        // `update` walks `pitch` per row and reads `row_w` bytes
        // each; the slice ends at the band's last pixel, not its
        // last full stride row.
        let band_rows = (fy1 - fy0) as usize;
        let band_len = (band_rows - 1) * stride + row_w;
        let sdl_rect = SdlRect::new(fx0, fy0, (fx1 - fx0) as u32, (fy1 - fy0) as u32);
        self.tex
            .update(sdl_rect, &self.buf[row_off..row_off + band_len], stride)
            .map_err(|_| crate::render::RenderError::BackendFailure)?;
        self.pending_present = true;
        Ok(())
    }

    fn end_flush(&mut self) -> Result<(), crate::render::RenderError> {
        if !self.pending_present {
            return Ok(());
        }
        self.canvas
            .copy(&self.tex, None, None)
            .map_err(|_| crate::render::RenderError::BackendFailure)?;
        self.canvas.present();
        self.pending_present = false;
        self.needs_redraw = false;
        Ok(())
    }

    fn frame_end(&mut self) -> Result<(), crate::render::RenderError> {
        self.end_flush()
    }

    fn abort_flush(&mut self) {
        self.pending_present = false;
    }

    fn set_text_input(&mut self, state: Option<crate::ui::text_input::TextInputState>) {
        self.text_input.set(state);
    }

    fn write_clipboard(
        &mut self,
        request: crate::ui::clipboard::ClipboardWrite,
    ) -> crate::ui::clipboard::ClipboardDispatch {
        #[cfg(target_os = "linux")]
        if super::host_clipboard::linux::is_native(self.canvas.window()) {
            return self.clipboard.write(self.canvas.window(), request);
        }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            super::host_clipboard::write(request)
        }
        #[cfg(target_os = "windows")]
        {
            super::host_clipboard::write(self.canvas.window(), request)
        }
        #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "windows")))]
        {
            super::sdl_events::write_clipboard(self.canvas.window().subsystem(), request)
        }
    }

    fn read_clipboard(
        &mut self,
        request: crate::ui::clipboard::ClipboardRead,
    ) -> crate::ui::clipboard::ClipboardDispatch {
        #[cfg(target_os = "linux")]
        if super::host_clipboard::linux::is_native(self.canvas.window()) {
            return self.clipboard.read(self.canvas.window(), request);
        }
        #[cfg(any(target_os = "macos", target_os = "ios", target_os = "windows"))]
        {
            super::host_clipboard::read(request)
        }
        #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "windows")))]
        {
            super::sdl_events::read_clipboard(self.canvas.window().subsystem(), request)
        }
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        if let Some(e) = self.pending.pop_front() {
            return Some(e);
        }
        #[cfg(target_os = "linux")]
        if let Some(response) = self.clipboard.poll() {
            return Some(InputEvent::Clipboard(response));
        }
        if self.multi.active {
            if let Some(t) = self.multi.last_event {
                if t.elapsed().as_millis() > MULTI_GESTURE_TIMEOUT_MS {
                    self.end_multi_gesture();
                    return self.pending.pop_front();
                }
            }
        }

        loop {
            let event = super::sdl_events::poll();
            #[cfg(target_os = "linux")]
            if let Some(response) = self.clipboard.poll() {
                self.pending.push_back(InputEvent::Clipboard(response));
            }
            let Some(event) = event else {
                break;
            };
            #[cfg(target_os = "macos")]
            if let Some(input) = self.menu.as_ref().and_then(|menu| menu.input(&event)) {
                self.pending.push_back(input);
                continue;
            }
            match event {
                Event::Quit { .. } => self.pending.push_back(InputEvent::Quit),
                Event::RenderDeviceReset { .. } => {
                    self.texture_lost = true;
                    self.needs_redraw = true;
                    self.pending_present = false;
                }
                event @ (Event::KeyDown { .. } | Event::KeyUp { .. }) => {
                    if let Some(input) = super::sdl_events::keyboard_input(&event) {
                        self.pending.push_back(input);
                    }
                }
                Event::MouseButtonDown { x, y, .. } => {
                    self.pending.push_back(InputEvent::PointerDown {
                        kind: crate::input::event::input::PointerKind::Mouse,
                        id: 0,
                        x: x.into(),
                        y: y.into(),
                    });
                }
                Event::MouseButtonUp { x, y, .. } => {
                    self.pending.push_back(InputEvent::PointerUp {
                        kind: crate::input::event::input::PointerKind::Mouse,
                        id: 0,
                        x: x.into(),
                        y: y.into(),
                    });
                }
                Event::MouseMotion { x, y, .. } => {
                    self.last_mouse_x = x;
                    self.last_mouse_y = y;
                    self.pending.push_back(InputEvent::PointerMove {
                        kind: crate::input::event::input::PointerKind::Mouse,
                        id: 0,
                        x: x.into(),
                        y: y.into(),
                    });
                }
                Event::MouseWheel {
                    precise_x,
                    precise_y,
                    ..
                } => {
                    self.pending.push_back(super::sdl_events::wheel_input(
                        precise_x,
                        precise_y,
                        self.last_mouse_x,
                        self.last_mouse_y,
                    ));
                }
                Event::MultiGesture {
                    d_theta,
                    d_dist,
                    x,
                    y,
                    ..
                } => {
                    let win_w = self.width as f32 / self.scale.to_f32();
                    let win_h = self.height as f32 / self.scale.to_f32();
                    let cx = x * win_w;
                    let cy = y * win_h;
                    self.handle_multi_gesture(cx, cy, d_theta, d_dist, win_w, win_h);
                }
                Event::TextInput { text, .. } => {
                    if let Some(input) = self.text_input.commit(text) {
                        self.pending.push_back(input);
                    }
                }
                Event::TextEditing {
                    text,
                    start,
                    length,
                    ..
                } => {
                    if let Some(input) = self.text_input.preedit(text, start, length) {
                        self.pending.push_back(input);
                    }
                }
                Event::Window { win_event, .. } => {
                    if matches!(
                        win_event,
                        sdl2::event::WindowEvent::Exposed
                            | sdl2::event::WindowEvent::Restored
                            | sdl2::event::WindowEvent::Shown
                    ) && !self.needs_redraw
                    {
                        self.pending_present = true;
                    }
                    if let Some(input) = super::sdl_events::pointer_window_input(win_event)
                        .or_else(|| super::sdl_events::host_window_input(win_event))
                    {
                        self.pending.push_back(input);
                    }
                }
                _ => {}
            }
            if let Some(input) = self.pending.pop_front() {
                return Some(input);
            }
        }
        self.pending.pop_front().or_else(|| self.text_input.idle())
    }
}

impl FramebufferAccess for SdlSurface {
    fn framebuffer(&mut self) -> Texture<'_> {
        Texture::new(
            &mut self.buf,
            self.width,
            self.height,
            ColorFormat::RGBA8888,
        )
        .expect("valid pixel storage")
    }
}
