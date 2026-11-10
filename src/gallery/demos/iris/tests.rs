use super::lens::{IrisLens, UNIT, blade_reach, blade_swing, glass_radius};
use super::state::{IrisLayout, IrisRun, IrisState, MAX_STOP, OPEN_EDGE, f_number, opening};
use super::*;
use crate::prelude::*;
use crate::render::texture::ColorFormat;
use crate::surface::FramebufferAccess;
use crate::surface::framebuf::FramebufSurface;

const SIZES: [(u16, u16, IrisLayout); 6] = [
    (128, 160, IrisLayout::Micro),
    (128, 128, IrisLayout::Dial),
    (240, 240, IrisLayout::Dial),
    (390, 844, IrisLayout::Portrait),
    (800, 480, IrisLayout::Landscape),
    (1280, 800, IrisLayout::Landscape),
];

fn fx(value: i32) -> Fixed {
    Fixed::from_int(value)
}

#[test]
fn font_covers_every_readout_character() {
    let font = crate::render::font::Font::from_mirx(
        "Iris",
        14,
        FONT_BYTES,
        &mirx::reader::PayloadLimits::HOST,
    )
    .unwrap();
    let mut buffer = [0u8; 4];
    let missing: alloc::string::String = (' '..='~')
        .filter(|c| !font.covers(c.encode_utf8(&mut buffer)))
        .collect();
    assert!(missing.is_empty(), "readout font lacks {missing:?}");
}

static SCHEDULED_NS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

fn scheduled_clock() -> u64 {
    SCHEDULED_NS.load(core::sync::atomic::Ordering::Relaxed)
}

#[test]
fn scheduled_frames_advance_the_showcase_clock() {
    let mut app = App::headless(390, 844);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup(&mut app, root, IrisRun::Capture);
    // The delta-time system reads the clock, so a stepped clock gives exact frames.
    app.world
        .insert_resource(crate::ecs::MonoClock::new(scheduled_clock));
    for _ in 0..86 {
        SCHEDULED_NS.fetch_add(50_000_000, core::sync::atomic::Ordering::Relaxed);
        app.systems.run_all(&mut app.world).unwrap();
        crate::core::reactive::flush_signal_dirty(&mut app.world);
    }
    let model = app
        .world
        .query::<IrisLens>()
        .iter()
        .next()
        .map(|(_, lens)| lens.model.clone())
        .unwrap();
    let (stop, shots) = crate::core::model::with_model_read_only(|| {
        crate::core::model::ModelHandle::read(&model, |state| (state.stop(), state.shots()))
    });
    assert!(stop > fx(4), "stop {stop}");
    assert_eq!(shots, 42);
}

#[test]
fn viewports_choose_the_expected_layout() {
    for (width, height, layout) in SIZES {
        assert_eq!(
            IrisLayout::classify(fx(i32::from(width)), fx(i32::from(height))),
            layout,
            "{width}x{height}"
        );
    }
    assert_eq!(IrisLayout::classify(fx(160), fx(128)), IrisLayout::Micro);
    assert_eq!(
        IrisLayout::classify(fx(1440), fx(900)),
        IrisLayout::Landscape
    );
}

#[test]
fn stops_follow_the_engraved_scale() {
    assert!(
        (f_number(Fixed::ZERO) - Fixed::from_ratio(1414, 1000)).abs() < Fixed::from_ratio(1, 100)
    );
    assert_eq!(f_number(fx(MAX_STOP)), fx(16));
    assert!((opening(Fixed::ZERO) - OPEN_EDGE).abs() < Fixed::from_ratio(1, 100));
    let mut previous = opening(Fixed::ZERO);
    for third in 1..=MAX_STOP * 3 {
        let current = opening(Fixed::from_ratio(third, 3));
        assert!(current < previous, "opening grows at third stop {third}");
        previous = current;
    }
}

#[test]
fn blade_swing_lands_the_edge_where_requested() {
    for per_mille in [-40, 0, 60, 200, 400, 700] {
        let edge = Fixed::from_ratio(per_mille * UNIT, 1000);
        let swing = blade_swing(edge);
        let reached = blade_reach(swing);
        assert!(
            (reached - edge).abs() < Fixed::from_ratio(1, 4),
            "edge {edge}: swing {swing} reaches {reached}"
        );
    }
    assert!(blade_reach(Fixed::ZERO) > glass_radius());
}

#[test]
fn capture_run_loops_exactly() {
    let mut state = IrisState::new(IrisRun::Capture, 390, 844);
    let mut samples = alloc::vec::Vec::new();
    for loop_index in 0..3 {
        for _ in 0..(LOOP_MS / 50) {
            state.advance(50);
            if loop_index > 0 {
                samples.push((state.stop(), state.closure(), state.shots(), state.toast()));
            }
        }
    }
    let half = samples.len() / 2;
    assert_eq!(samples[..half], samples[half..]);
    assert!(samples[..half].iter().any(|sample| sample.1 == Fixed::ONE));
    assert!(samples[..half].iter().any(|sample| sample.2 == 42));
    assert_eq!(samples[0].2, 41);
    assert!(samples[..half].iter().any(|sample| sample.3 > 200));
}

#[test]
fn manual_aperture_stops_the_script() {
    let mut state = IrisState::new(IrisRun::Live, 1280, 800);
    state.set_stop(fx(6));
    for _ in 0..80 {
        state.advance(50);
    }
    assert!(!state.scripted());
    assert_eq!(super::state::third_label(state.aperture_third()), "11");
    state.toggle_auto();
    for _ in 0..80 {
        state.advance(50);
    }
    assert!(state.scripted());
    assert!(state.stop() < fx(6), "the showcase path takes over again");
}

/// Render a few capture frames and return the lens rectangle and the pixels.
fn render(width: u16, height: u16, theme: &'static str) -> (Rect, alloc::vec::Vec<u8>) {
    let surface = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
    let mut app = App::new(surface);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup(&mut app, root, IrisRun::Capture);
    app.set_root(root);
    app.set_theme(theme).unwrap();
    for _ in 0..4 {
        app.systems.run_all(&mut app.world).unwrap();
        crate::core::reactive::flush_signal_dirty(&mut app.world);
        app.render().unwrap();
    }
    // Every layout branch is built once; only the selected one is laid out.
    let shown: alloc::vec::Vec<Rect> = app
        .world
        .query::<IrisLens>()
        .iter()
        .filter_map(|(entity, _)| app.world.get::<crate::ui::ComputedRect>(entity))
        .map(|rect| rect.0)
        .filter(|rect| rect.w.is_positive() && rect.h.is_positive())
        .collect();
    assert_eq!(shown.len(), 1, "{width}x{height} visible lenses {shown:?}");
    let rect = shown[0];
    let pixels = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    (rect, pixels)
}

#[test]
fn every_layout_renders_a_lit_iris() {
    for (width, height, layout) in SIZES {
        for theme in [DARK_ID, LIGHT_ID] {
            let (rect, pixels) = render(width, height, theme);
            assert!(rect.w >= fx(60), "{width}x{height} lens {rect:?}");
            assert!(rect.x >= Fixed::ZERO && rect.x + rect.w <= fx(i32::from(width)));
            assert!(rect.y >= Fixed::ZERO && rect.y + rect.h <= fx(i32::from(height)));
            let cx = (rect.x + rect.w / 2).to_int() as usize;
            let cy = (rect.y + rect.h / 2).to_int() as usize;
            let offset = (cy * usize::from(width) + cx) * 4;
            let [r, g, b] = [pixels[offset], pixels[offset + 1], pixels[offset + 2]];
            assert!(
                r > 150 && g < 140 && b < 120,
                "{layout:?} {theme} centre is {r},{g},{b}"
            );
        }
    }
}
