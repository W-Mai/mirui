#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::render::font::FontToken;
#[cfg(test)]
use crate::render::path::PathCmd;
use crate::render::path::{Path, PathId, PathStore};
use crate::ui::IgnoreHitTest;
use crate::ui::widgets::{
    ParagraphStyle, ShapingPolicy, Text, TextAlign, TextDirection, TextVerticalAlign, TextWrap,
};

pub const VIEWPORT: (u16, u16) = (128, 128);

const BACKGROUND: ColorToken = ColorToken::Surface;
const PANEL: ColorToken = ColorToken::SurfaceVariant;
const BORDER: ColorToken = ColorToken::Outline;
const TEXT: ColorToken = ColorToken::OnSurface;
const MUTED: ColorToken = ColorToken::OnSurfaceVariant;
const CYAN: ColorToken = ColorToken::Primary;
const MARQUEE_CYCLE: i32 = 192;
const TEXT_WINDOW: i32 = 400;

static WAVE: Path = path!(
    "M -160 38 C -140 10 -100 10 -80 38 C -60 66 -20 66 0 38 C 20 10 60 10 80 38 C 100 66 140 66 160 38 C 180 10 220 10 240 38 C 260 66 300 66 320 38 C 340 10 380 10 400 38 C 420 66 460 66 480 38"
);

#[derive(Clone, Copy)]
struct CompactCurveNodes {
    path: PathId,
    text: Entity,
}

#[derive(Default)]
struct CompactCurveMotion(super::motion::BrakedPhase);

fn bitmap_line(align: TextAlign) -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::NoWrap,
        align,
        vertical_align: TextVerticalAlign::Center,
        max_lines: Some(1),
        direction: TextDirection::LeftToRight,
        shaping: ShapingPolicy::Simple,
        ..ParagraphStyle::default()
    }
}

fn text_path(path: PathId, phase: Fixed) -> crate::text::TextPath {
    let offset = phase * Fixed::from_int(MARQUEE_CYCLE) / Fixed::from_int(360);
    crate::text::TextPath::new(path).with_window(offset, Fixed::from_int(TEXT_WINDOW))
}

#[mirui_macros::system(order = ANIMATION)]
fn compact_curve_animation_system(world: &mut World) {
    let dt = world
        .resource::<DeltaTimeMs>()
        .map_or(16, |delta| delta.0)
        .min(50);
    let Some(motion) = world.resource_mut::<CompactCurveMotion>() else {
        return;
    };
    let phase = motion.0.advance(
        dt,
        520,
        Fixed::from_int(90),
        Fixed::ONE,
        false,
        Fixed::from_int(360),
    );
    let Some(nodes) = world.resource::<CompactCurveNodes>().copied() else {
        return;
    };
    if let Some(mut text) = world.widget_mut(nodes.text) {
        text.text_path(text_path(nodes.path, phase));
    }
}

#[compose]
pub fn build_widgets(path: PathId) {
    ui! {
        Column (
            id: "compact_curve_text_shell",
            grow: 1.0,
            padding: Padding::all(6),
            row_gap: 4,
            bg_color: BACKGROUND
        ) {
            Row (height: 14, align: AlignItems::Center, column_gap: 4) {
                View (width: 4, height: 10, bg_color: CYAN, border_radius: 2)
                Text (
                    "CURVE TEXT",
                    grow: 1.0,
                    height: 14,
                    font: FontToken::Default,
                    font_size: 6,
                    text_color: TEXT,
                    paragraph: bitmap_line(TextAlign::Start)
                )
            }
            View (id: "compact_curve_text_stage", grow: 1.0, clip_children: true) {
                Text (
                    id: "compact_curve_text_primary",
                    "MIRUI RIDES THE WAVE    MIRUI RIDES THE WAVE    ",
                    path: text_path(path, Fixed::ZERO),
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    font: FontToken::Default,
                    font_size: 8,
                    text_color: TEXT,
                    paragraph: bitmap_line(TextAlign::Start)
                ) [
                    IgnoreHitTest,
                ]
            }
            View (
                id: "compact_curve_text_footer",
                width: Dimension::percent(100),
                height: 18,
                padding: Padding {
                    top: Dimension::px(3),
                    right: Dimension::px(10),
                    bottom: Dimension::px(3),
                    left: Dimension::px(10),
                },
                bg_color: PANEL,
                border_color: BORDER,
                border_width: 1,
                border_radius: 8
            ) [
                IgnoreHitTest,
            ] {
                Text (
                    id: "compact_curve_text_footer_label",
                    "AUTO LOOP",
                    grow: 1.0,
                    font: FontToken::Default,
                    font_size: 6,
                    text_color: MUTED,
                    paragraph: bitmap_line(TextAlign::Center)
                ) [
                    IgnoreHitTest,
                ]
            }
        }
    };
}

pub fn install<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    let path = app
        .world
        .resource_mut::<PathStore>()
        .expect("path store")
        .insert_static(WAVE.commands())
        .expect("compact curve path");
    app.world.insert_resource(CompactCurveMotion::default());
    app.add_system(compact_curve_animation_system::system());
    app.compose(parent, |cx| build_widgets(cx, path));
    let text = app
        .world
        .find_by_id("compact_curve_text_primary")
        .expect("compact curve text");
    app.world.insert_resource(CompactCurveNodes { path, text });
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    if app.world.resource::<MonoClock>().is_none() {
        app.add_plugin(StdInstantClockPlugin);
    }
    install(app, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::flush_signal_dirty;
    use crate::surface::FramebufferAccess;

    #[test]
    fn wave_has_fixed_topology_and_alternating_extrema() {
        let path = &WAVE;
        assert_eq!(path.commands().len(), 9);
        assert!(path.is_borrowed());
        let PathCmd::MoveTo(start) = path.commands()[0] else {
            panic!("compact path start");
        };
        let PathCmd::CubicTo { ctrl1: crest, .. } = path.commands()[1] else {
            panic!("compact path crest");
        };
        let PathCmd::CubicTo { ctrl1: trough, .. } = path.commands()[2] else {
            panic!("compact path trough");
        };
        assert_eq!(start.x, Fixed::from_int(-160));
        assert_eq!(crest.y, Fixed::from_int(10));
        assert_eq!(trough.y, Fixed::from_int(66));
    }

    #[test]
    fn embedded_layout_uses_bitmap_text_and_constant_path_window() {
        let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        install(&mut app, root);
        app.set_root(root);
        flush_signal_dirty(&mut app.world);
        app.render().unwrap();

        let text = app.world.find_by_id("compact_curve_text_primary").unwrap();
        let initial = *app.world.get::<crate::text::TextPath>(text).unwrap();
        assert_eq!(
            app.world
                .get::<crate::ui::Style>(text)
                .unwrap()
                .font_stack
                .primary(),
            &FontToken::Default
        );

        app.world.insert_resource(DeltaTimeMs(16));
        compact_curve_animation_system(&mut app.world);
        let current = *app.world.get::<crate::text::TextPath>(text).unwrap();
        assert!(current.offset() > initial.offset());
        assert_eq!(
            current.end().unwrap() - current.offset(),
            Fixed::from_int(TEXT_WINDOW)
        );
        assert!(!app.backend.framebuffer().buf.as_slice().is_empty());
    }

    #[test]
    fn compact_shell_resolves_the_active_theme() {
        let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
        app.with_default_widgets().with_default_systems();
        app.world.insert_resource(crate::ui::Theme::light());
        let root = app.spawn_root().id();
        install(&mut app, root);
        app.set_root(root);
        flush_signal_dirty(&mut app.world);
        app.render().unwrap();

        let surface = crate::ui::Theme::light().resolve(BACKGROUND);
        assert_eq!(
            &app.backend.framebuffer().buf.as_slice()[..3],
            &[surface.r, surface.g, surface.b]
        );
    }
}
