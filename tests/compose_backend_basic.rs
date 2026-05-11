//! Compose a hybrid backend out of two Dummy backends and verify every
//! method routes to the intended side.
//!
//! Each Dummy counts method invocations. The `Hybrid` instance exposes both
//! dummies as struct fields, so we can read the counters directly.

use std::cell::Cell;

use mirui::draw::backend::DrawBackend;
use mirui::draw::path::Path;
use mirui::draw::texture::{ColorFormat, Texture};
use mirui::types::{Color, Fixed, Point, Rect, Transform};
use mirui_macros::compose_backend;

#[derive(Default)]
struct Counts {
    fill_path: Cell<u32>,
    stroke_path: Cell<u32>,
    blit: Cell<u32>,
    clear: Cell<u32>,
    draw_label: Cell<u32>,
    flush: Cell<u32>,
    fill_rect: Cell<u32>,
    stroke_rect: Cell<u32>,
    draw_line: Cell<u32>,
    draw_arc: Cell<u32>,
}

struct Dummy {
    counts: Counts,
}

impl Dummy {
    fn new() -> Self {
        Self {
            counts: Counts::default(),
        }
    }
}

impl DrawBackend for Dummy {
    fn fill_path(&mut self, _: &Path, _: &Rect, _: &Color, _: u8) {
        self.counts.fill_path.set(self.counts.fill_path.get() + 1);
    }
    fn stroke_path(&mut self, _: &Path, _: &Rect, _: Fixed, _: &Color, _: u8) {
        self.counts
            .stroke_path
            .set(self.counts.stroke_path.get() + 1);
    }
    fn blit(&mut self, _: &Texture, _: &Rect, _: Point, _: Point, _: &Rect) {
        self.counts.blit.set(self.counts.blit.get() + 1);
    }
    fn clear(&mut self, _: &Rect, _: &Color) {
        self.counts.clear.set(self.counts.clear.get() + 1);
    }
    fn draw_label(&mut self, _: &Point, _: &[u8], _: &Rect, _: &Color, _: u8) {
        self.counts.draw_label.set(self.counts.draw_label.get() + 1);
    }
    fn flush(&mut self) {
        self.counts.flush.set(self.counts.flush.get() + 1);
    }
    // Override default impls so the counter actually gets hit without going
    // through fill_path / stroke_path.
    fn fill_rect(&mut self, _: &Rect, _: &Rect, _: &Color, _: Fixed, _: u8) {
        self.counts.fill_rect.set(self.counts.fill_rect.get() + 1);
    }
    fn stroke_rect(&mut self, _: &Rect, _: &Rect, _: Fixed, _: &Color, _: Fixed, _: u8) {
        self.counts
            .stroke_rect
            .set(self.counts.stroke_rect.get() + 1);
    }
    fn draw_line(&mut self, _: Point, _: Point, _: &Rect, _: Fixed, _: &Color, _: u8) {
        self.counts.draw_line.set(self.counts.draw_line.get() + 1);
    }
    fn draw_arc(
        &mut self,
        _: Point,
        _: Fixed,
        _: Fixed,
        _: Fixed,
        _: &Rect,
        _: Fixed,
        _: &Color,
        _: u8,
    ) {
        self.counts.draw_arc.set(self.counts.draw_arc.get() + 1);
    }
}

compose_backend! {
    pub struct Hybrid {
        sw: Dummy,
        gpu: Dummy,
    }
    route {
        default => sw,
        blit => gpu,
        clear => gpu,
        fill_rect => gpu,
    }
}

fn fresh_hybrid() -> Hybrid<Dummy, Dummy> {
    Hybrid {
        sw: Dummy::new(),
        gpu: Dummy::new(),
    }
}

fn zero_rect() -> Rect {
    Rect::new(0, 0, 4, 4)
}

fn dummy_texture_buf() -> Vec<u8> {
    vec![0u8; 4 * 4 * 4]
}

#[test]
fn default_methods_route_to_sw() {
    let mut h = fresh_hybrid();
    let path = Path::new();
    let rect = zero_rect();
    let color = Color::rgb(0, 0, 0);

    h.fill_path(&path, &rect, &color, 255);
    h.stroke_path(&path, &rect, Fixed::ONE, &color, 255);
    h.draw_label(&Point::ZERO, b"x", &rect, &color, 255);
    h.flush();

    assert_eq!(h.sw.counts.fill_path.get(), 1);
    assert_eq!(h.sw.counts.stroke_path.get(), 1);
    assert_eq!(h.sw.counts.draw_label.get(), 1);
    assert_eq!(h.sw.counts.flush.get(), 1);
    assert_eq!(h.gpu.counts.fill_path.get(), 0);
}

#[test]
fn explicit_routes_go_to_gpu() {
    let mut h = fresh_hybrid();
    let mut buf = dummy_texture_buf();
    let tex = Texture::new(&mut buf, 4, 4, ColorFormat::ARGB8888);
    let rect = zero_rect();
    let color = Color::rgb(0, 0, 0);

    h.blit(&tex, &rect, Point::ZERO, Point::ZERO, &rect);
    h.clear(&rect, &color);
    h.fill_rect(&rect, &rect, &color, Fixed::ZERO, 255);

    assert_eq!(h.gpu.counts.blit.get(), 1);
    assert_eq!(h.gpu.counts.clear.get(), 1);
    assert_eq!(h.gpu.counts.fill_rect.get(), 1);
    assert_eq!(h.sw.counts.blit.get(), 0);
    assert_eq!(h.sw.counts.clear.get(), 0);
    assert_eq!(h.sw.counts.fill_rect.get(), 0);
}

#[test]
fn unrouted_default_impl_methods_fall_through_to_trait_default() {
    // stroke_rect, draw_line, draw_arc were not routed. They should go
    // through the DrawBackend trait default, which ultimately calls
    // stroke_path on the default backend (sw).
    let mut h = fresh_hybrid();
    let rect = zero_rect();
    let color = Color::rgb(0, 0, 0);

    h.stroke_rect(&rect, &rect, Fixed::ONE, &color, Fixed::ZERO, 255);
    h.draw_line(Point::ZERO, Point::ZERO, &rect, Fixed::ONE, &color, 255);
    h.draw_arc(
        Point::ZERO,
        Fixed::from_int(4),
        Fixed::ZERO,
        Fixed::from_int(90),
        &rect,
        Fixed::ONE,
        &color,
        255,
    );

    // sw counters: each default-impl call funnels into stroke_path.
    // Dummy overrides stroke_rect/draw_line/draw_arc so those are NOT called
    // on sw — instead the Hybrid's trait default path took over (since we
    // didn't route them) and invoked sw.stroke_path directly.
    assert_eq!(h.sw.counts.stroke_path.get(), 3);
    assert_eq!(h.sw.counts.stroke_rect.get(), 0);
    assert_eq!(h.sw.counts.draw_line.get(), 0);
    assert_eq!(h.sw.counts.draw_arc.get(), 0);
}

/// A backend that borrows a pixel buffer, giving it a real lifetime parameter
/// so we can prove the macro handles `BorrowedDummy<'fb>` as a generic type
/// argument without needing the Hybrid struct itself to declare `'fb`.
struct BorrowedDummy<'fb> {
    buf: &'fb mut [u8],
    fills: Cell<u32>,
}

impl<'fb> BorrowedDummy<'fb> {
    fn new(buf: &'fb mut [u8]) -> Self {
        Self {
            buf,
            fills: Cell::new(0),
        }
    }
}

impl<'fb> DrawBackend for BorrowedDummy<'fb> {
    fn fill_path(&mut self, _: &Path, _: &Rect, _: &Color, _: u8) {
        // Touch the borrowed buffer so the lifetime actually matters at the
        // call site — otherwise `'fb` could be optimised away and the test
        // would be vacuous.
        if !self.buf.is_empty() {
            self.buf[0] = self.buf[0].wrapping_add(1);
        }
        self.fills.set(self.fills.get() + 1);
    }
    fn stroke_path(&mut self, _: &Path, _: &Rect, _: Fixed, _: &Color, _: u8) {}
    fn blit(&mut self, _: &Texture, _: &Rect, _: Point, _: Point, _: &Rect) {}
    fn clear(&mut self, _: &Rect, _: &Color) {}
    fn draw_label(&mut self, _: &Point, _: &[u8], _: &Rect, _: &Color, _: u8) {}
    fn flush(&mut self) {}
}

struct PlainDummy;
impl DrawBackend for PlainDummy {
    fn fill_path(&mut self, _: &Path, _: &Rect, _: &Color, _: u8) {}
    fn stroke_path(&mut self, _: &Path, _: &Rect, _: Fixed, _: &Color, _: u8) {}
    fn blit(&mut self, _: &Texture, _: &Rect, _: Point, _: Point, _: &Rect) {}
    fn clear(&mut self, _: &Rect, _: &Color) {}
    fn draw_label(&mut self, _: &Point, _: &[u8], _: &Rect, _: &Color, _: u8) {}
    fn flush(&mut self) {}
}

compose_backend! {
    pub struct HybridWithLifetime {
        borrowed: BorrowedDummy,
        plain: PlainDummy,
    }
    route {
        default => borrowed,
        blit => plain,
    }
}

#[test]
fn hybrid_is_a_renderer_and_dispatches_drawcommands() {
    // Verifies the Renderer impl the macro emits alongside DrawBackend.
    // Sending a DrawCommand::Blit should reach the field that owns `blit`
    // in the route table.
    use mirui::draw::DrawCommand;
    use mirui::draw::renderer::Renderer;

    let mut h = fresh_hybrid();
    let mut buf = dummy_texture_buf();
    let tex = Texture::new(&mut buf, 4, 4, ColorFormat::ARGB8888);
    let rect = zero_rect();

    Renderer::draw(
        &mut h,
        &DrawCommand::Blit {
            pos: Point::ZERO,
            size: Point::ZERO,
            transform: Transform::IDENTITY,
            texture: &tex,
        },
        &rect,
    );

    assert_eq!(h.gpu.counts.blit.get(), 1);
    assert_eq!(h.sw.counts.blit.get(), 0);
}

#[test]
fn hybrid_accepts_backend_with_lifetime_parameter() {
    let mut buf = [0u8; 8];
    let borrowed = BorrowedDummy::new(&mut buf);
    let plain = PlainDummy;

    // The hybrid type itself takes two generic parameters — no lifetime on
    // Hybrid, the borrow in BorrowedDummy<'_> gets threaded through the
    // generic slot.
    let mut h: HybridWithLifetime<BorrowedDummy<'_>, PlainDummy> =
        HybridWithLifetime { borrowed, plain };

    let rect = Rect::new(0, 0, 4, 4);
    let path = Path::new();
    h.fill_path(&path, &rect, &Color::rgb(0, 0, 0), 255);

    assert_eq!(h.borrowed.fills.get(), 1);
    // Side effect through the borrowed slice proves the lifetime really is
    // being honoured end-to-end.
    assert_eq!(h.borrowed.buf[0], 1);
}
