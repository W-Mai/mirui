use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;
use std::time::Instant;

use sdl2::EventPump;
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect as SdlRect;
use sdl2::render::{Canvas, Texture as SdlTexture, TextureCreator};
use sdl2::video::{Window, WindowContext};

use super::{DisplayInfo, FramebufferAccess, InputEvent, Surface, logical_from_physical};
use crate::render::texture::{ColorFormat, Texture};
use crate::types::{Fixed, Rect};

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
    canvas: Canvas<Window>,
    pending_present: bool,
    event_pump: EventPump,
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
        let sdl = sdl2::init().expect("SDL2 init failed");
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

        Self {
            tex,
            canvas,
            pending_present: false,
            event_pump,
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
                id: VIRT_FINGER_A,
                x: Fixed::from(ax as i32),
                y: Fixed::from(ay as i32),
            });
            self.pending.push_back(InputEvent::PointerDown {
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
            id: VIRT_FINGER_A,
            x: Fixed::from(ax as i32),
            y: Fixed::from(ay as i32),
        });
        self.pending.push_back(InputEvent::PointerMove {
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
            id: VIRT_FINGER_A,
            x: Fixed::from(ax as i32),
            y: Fixed::from(ay as i32),
        });
        self.pending.push_back(InputEvent::PointerUp {
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

    fn begin_flush(&mut self) {
        self.pending_present = false;
    }

    fn flush(&mut self, area: &Rect) {
        // SDL `Texture::update` requires the rect to be inside the
        // texture, so clip first.
        let (x0, y0, x1, y1) = area.pixel_bounds();
        let fx0 = x0.max(0);
        let fy0 = y0.max(0);
        let fx1 = x1.min(self.width as i32);
        let fy1 = y1.min(self.height as i32);
        if fx1 <= fx0 || fy1 <= fy0 {
            return;
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
            .expect("texture update failed");
        self.pending_present = true;
    }

    fn end_flush(&mut self) {
        if !self.pending_present {
            return;
        }
        self.canvas
            .copy(&self.tex, None, None)
            .expect("copy failed");
        self.canvas.present();
        self.pending_present = false;
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        if let Some(e) = self.pending.pop_front() {
            return Some(e);
        }
        if self.multi.active {
            if let Some(t) = self.multi.last_event {
                if t.elapsed().as_millis() > MULTI_GESTURE_TIMEOUT_MS {
                    self.end_multi_gesture();
                    return self.pending.pop_front();
                }
            }
        }

        // poll_iter drains SDL's queue; queue all translated events
        // before returning one or the tail of a busy frame is lost.
        let events: Vec<_> = self.event_pump.poll_iter().collect();
        for event in events {
            match event {
                Event::Quit { .. } => self.pending.push_back(InputEvent::Quit),
                Event::KeyDown {
                    keycode: Some(kc), ..
                } => {
                    use crate::event::input::*;
                    let code = match kc {
                        Keycode::Backspace => KEY_BACKSPACE,
                        Keycode::Delete => KEY_DELETE,
                        Keycode::Left => KEY_LEFT,
                        Keycode::Right => KEY_RIGHT,
                        Keycode::Home => KEY_HOME,
                        Keycode::End => KEY_END,
                        Keycode::Return => KEY_RETURN,
                        Keycode::Escape => {
                            self.pending.push_back(InputEvent::Quit);
                            continue;
                        }
                        _ => continue,
                    };
                    self.pending.push_back(InputEvent::Key {
                        code,
                        pressed: true,
                    });
                }
                Event::MouseButtonDown { x, y, .. } => {
                    self.pending.push_back(InputEvent::PointerDown {
                        id: 0,
                        x: x.into(),
                        y: y.into(),
                    });
                }
                Event::MouseButtonUp { x, y, .. } => {
                    self.pending.push_back(InputEvent::PointerUp {
                        id: 0,
                        x: x.into(),
                        y: y.into(),
                    });
                }
                Event::MouseMotion { x, y, .. } => {
                    self.last_mouse_x = x;
                    self.last_mouse_y = y;
                    self.pending.push_back(InputEvent::PointerMove {
                        id: 0,
                        x: x.into(),
                        y: y.into(),
                    });
                }
                Event::MouseWheel { x, y, .. } => {
                    self.pending.push_back(InputEvent::Wheel {
                        dx: Fixed::from(x),
                        dy: Fixed::from(y),
                        x: Fixed::from(self.last_mouse_x),
                        y: Fixed::from(self.last_mouse_y),
                    });
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
                    if let Some(ch) = text.chars().next() {
                        self.pending.push_back(InputEvent::CharInput { ch });
                    }
                }
                Event::Window {
                    win_event: WindowEvent::Leave,
                    ..
                } => {
                    // SDL has no hover-leave InputEvent; synthesize a
                    // miss. Don't touch `last_mouse_x/y` so a wheel
                    // event arriving before the next motion still
                    // anchors at the last in-window position.
                    const OFF: i32 = i16::MIN as i32;
                    self.pending.push_back(InputEvent::PointerMove {
                        id: 0,
                        x: Fixed::from_int(OFF),
                        y: Fixed::from_int(OFF),
                    });
                }
                _ => {}
            }
        }
        self.pending.pop_front()
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
    }
}
