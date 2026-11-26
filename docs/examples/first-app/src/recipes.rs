use super::motion::indicator_transition;
use mirui::prelude::draw::*;
use mirui::prelude::*;
use mirui::render::scene::{LineCap, LineJoin, Paint};
use mirui::types::Transform;
use mirui::ui::widgets::{Button, FollowBounds, ParagraphStyle, Text, TextAlign};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Recipe {
    Reactive,
    Layout,
    Spring,
    Vector,
}

impl Recipe {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "reactive" => Some(Self::Reactive),
            "layout" => Some(Self::Layout),
            "spring" => Some(Self::Spring),
            "vector" => Some(Self::Vector),
            _ => None,
        }
    }
}

pub fn install_recipe<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    recipe: Recipe,
    reduced: bool,
) {
    let root = app.spawn_root().id();
    match recipe {
        Recipe::Reactive => {
            let gauge = app.add_model(Gauge::default()).finish();
            app.compose(root, reactive_scene(gauge));
        }
        Recipe::Layout => {
            let deck = app.add_model(FlexDeck::default()).finish();
            app.compose(root, layout_scene(deck));
        }
        Recipe::Spring => {
            let tabs = app.add_model(SpringTabs::default()).finish();
            app.compose(root, spring_scene(tabs, reduced));
        }
        Recipe::Vector => {
            app.with_widget(paint_glyph::view());
            app.compose(root, vector_scene());
        }
    }
}

// #region recipe-reactive
#[model]
pub struct Gauge {
    #[observe]
    pub level: i32,
}

impl Default for Gauge {
    fn default() -> Self {
        Self { level: 42 }
    }
}

#[model]
impl Gauge {
    pub fn step(&mut self, delta: i32) {
        self.level = (self.level + delta).clamp(0, 100);
    }
}

#[compose]
fn reactive_scene(gauge: model!(Gauge)) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(20),
            justify: JustifyContent::SpaceBetween,
            text_color: ColorToken::OnSurface
        ) {
            Row (height: 28, justify: JustifyContent::SpaceBetween, align: AlignItems::Center) {
                Text ("Signal binding", font_size: 14, text_color: ColorToken::OnSurfaceVariant)
                Text (
                    text: ${ format_args!("{} %", gauge.level()) },
                    capacity: 12,
                    font_size: 22,
                    height: 28,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
            }
            View (height: 14, bg_color: ColorToken::SurfaceVariant, border_radius: 7, clip_children: true) {
                View (
                    width: ${ Dimension::percent(gauge.level()) },
                    height: Dimension::percent(100),
                    bg_color: ${ if gauge.level() >= 80 { ColorToken::Primary } else { ColorToken::Secondary } },
                    border_radius: 7
                )
            }
            Row (height: 40, column_gap: 8) {
                Button (
                    "- 10",
                    grow: 1.0,
                    height: 40,
                    border_radius: 9,
                    normal_color: ColorToken::SurfaceVariant,
                    pressed_color: ColorToken::Outline,
                    text_color: ColorToken::OnSurface
                ) on Tap { gauge.step(-10); }
                Button (
                    "42",
                    width: 64,
                    height: 40,
                    border_radius: 9,
                    normal_color: ColorToken::SurfaceVariant,
                    pressed_color: ColorToken::Outline,
                    text_color: ColorToken::OnSurfaceVariant
                ) on Tap { gauge.step(42 - gauge.level()); }
                Button (
                    "+ 10",
                    grow: 1.0,
                    height: 40,
                    border_radius: 9,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary,
                    text_color: ColorToken::OnPrimary
                ) on Tap { gauge.step(10); }
            }
        }
    };
}
// #endregion recipe-reactive

// #region recipe-layout
#[model]
#[derive(Default)]
pub struct FlexDeck {
    #[observe]
    pub compact: bool,
}

#[model]
impl FlexDeck {
    pub fn toggle(&mut self) {
        self.compact = !self.compact;
    }
}

#[compose]
fn layout_scene(deck: model!(FlexDeck)) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(18),
            row_gap: 12,
            text_color: ColorToken::OnSurface
        ) {
            Row (height: 34, justify: JustifyContent::SpaceBetween, align: AlignItems::Center) {
                Text (
                    text: ${ if deck.compact() { "Compact · 128 px card" } else { "Expanded · full row" } },
                    capacity: 28,
                    height: 22,
                    font_size: 13,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Button (
                    "Toggle width",
                    width: 112,
                    height: 32,
                    font_size: 12,
                    border_radius: 8,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary,
                    text_color: ColorToken::OnPrimary
                ) on Tap { deck.toggle(); }
            }
            Row (
                grow: 1.0,
                padding: Padding::all(14),
                column_gap: 10,
                align: AlignItems::Center,
                justify: JustifyContent::SpaceBetween,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 12
            ) {
                Column (
                    width: ${ Dimension::px(if deck.compact() { 96 } else { 156 }) },
                    height: 76,
                    padding: Padding::all(10),
                    justify: JustifyContent::SpaceBetween,
                    bg_color: ColorToken::Surface,
                    border_radius: 9
                ) {
                    View (width: 24, height: 6, bg_color: ColorToken::Primary, border_radius: 3)
                    Text ("Primary", font_size: 12, height: 18)
                }
                Column (
                    grow: 1.0,
                    height: 76,
                    padding: Padding::all(10),
                    justify: JustifyContent::SpaceBetween,
                    bg_color: ColorToken::Surface,
                    border_radius: 9
                ) {
                    View (width: 36, height: 6, bg_color: ColorToken::Secondary, border_radius: 3)
                    Text ("Flex grow: 1.0", font_size: 12, height: 18, text_color: ColorToken::OnSurfaceVariant)
                }
            }
        }
    };
}
// #endregion recipe-layout

// #region recipe-spring
#[model]
#[derive(Default)]
pub struct SpringTabs {
    #[observe]
    pub active: u8,
}

#[event]
fn pick_tab(
    index: u8,
    tabs: model!(SpringTabs),
    target: entity!(),
    followers: query!(mut FollowBounds),
) {
    tabs.set_active(index);
    for follower in followers {
        follower.target = target;
    }
}

#[model]
impl SpringTabs {
    pub fn set_active(&mut self, index: u8) {
        self.active = index;
    }
}

#[compose]
fn spring_scene(tabs: model!(SpringTabs), reduced: bool) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(20),
            justify: JustifyContent::SpaceBetween,
            text_color: ColorToken::OnSurface
        ) {
            Text (
                "Tap any pill — the frame retargets mid-flight",
                height: 22,
                font_size: 13,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Row (
                height: 52,
                padding: Padding::all(5),
                column_gap: 6,
                align: AlignItems::Center,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 12
            ) {
                Button (
                    "MCU",
                    id: "spring_first",
                    grow: 1.0,
                    height: 38,
                    border_radius: 8,
                    normal_color: ColorToken::Surface,
                    pressed_color: ColorToken::SurfaceVariant,
                    text_color: ColorToken::OnSurface
                ) on Tap(pick_tab::callback(0, tabs))
                Button (
                    "Desktop",
                    grow: 1.4,
                    height: 38,
                    border_radius: 8,
                    normal_color: ColorToken::Surface,
                    pressed_color: ColorToken::SurfaceVariant,
                    text_color: ColorToken::OnSurface
                ) on Tap(pick_tab::callback(1, tabs))
                Button (
                    "Web",
                    grow: 1.0,
                    height: 38,
                    border_radius: 8,
                    normal_color: ColorToken::Surface,
                    pressed_color: ColorToken::SurfaceVariant,
                    text_color: ColorToken::OnSurface
                ) on Tap(pick_tab::callback(2, tabs))
                View (
                    id: "spring_ring",
                    border_width: 2,
                    border_color: ColorToken::Primary,
                    border_radius: 9
                ) [
                    FollowBounds::new(id("spring_first"))
                        .with_transition(indicator_transition(reduced)),
                ]
            }
            Text (
                text: ${
                    match tabs.active() {
                        0 => "Target 01 · SPI framebuffer with dirty-rect clips",
                        1 => "Target 02 · Batched GPU draws with retained textures",
                        _ => "Target 03 · Browser canvas driven by requestAnimationFrame",
                    }
                },
                height: 20,
                font_size: 12,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
        }
    };
}
// #endregion recipe-spring

// #region recipe-vector
static APERTURE_BLADE: Path = path!(M 50 6 L 94 50 L 50 94 L 6 50 Z);
static INNER_RING: Path = path!(M 50 24 L 76 50 L 50 76 L 24 50 Z);

const ACCENTS: [Color; 3] = [
    Color::rgb(52, 82, 255),
    Color::rgb(207, 67, 43),
    Color::rgb(42, 157, 143),
];

#[component]
#[derive(Clone, Copy)]
pub struct VectorGlyph {
    pub accent: i32,
    pub stroke: i32,
}

impl Default for VectorGlyph {
    fn default() -> Self {
        Self {
            accent: 0,
            stroke: 2,
        }
    }
}

impl VectorGlyph {
    fn cycle(self) -> Self {
        Self {
            accent: (self.accent + 1) % 3,
            stroke: if self.stroke >= 4 { 2 } else { self.stroke + 1 },
        }
    }
}

#[view(component = VectorGlyph, name = "VectorGlyph", priority = 60)]
fn paint_glyph(glyph: component!(), paint: paint!()) {
    let rect = paint.rect();
    let transform = paint
        .transform()
        .compose(&Transform::translate(rect.x, rect.y))
        .compose(&Transform::scale(
            rect.w / Fixed::from_int(100),
            rect.h / Fixed::from_int(100),
        ));
    let color = ACCENTS[(glyph.accent.rem_euclid(3)) as usize];
    let stroke_paint = Paint::Color(color.into());
    let dash: [Fixed; 0] = [];
    paint.draw(&DrawCommand::StrokePath {
        path: &APERTURE_BLADE,
        transform,
        paint: &stroke_paint,
        width: Fixed::from_int(glyph.stroke),
        opa: 255,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        miter_limit: Fixed::from_int(4),
        dash: &dash,
    });
    paint.draw(&DrawCommand::StrokePath {
        path: &INNER_RING,
        transform,
        paint: &stroke_paint,
        width: Fixed::from_int(1),
        opa: 180,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        miter_limit: Fixed::from_int(4),
        dash: &dash,
    });
}

#[compose]
fn vector_scene() {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(18),
            justify: JustifyContent::SpaceBetween,
            text_color: ColorToken::OnSurface
        ) {
            Text (
                "Custom #[view] — tap any tile to cycle stroke & pigment",
                height: 20,
                font_size: 13,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Row (
                grow: 1.0,
                justify: JustifyContent::SpaceEvenly,
                align: AlignItems::Center,
                column_gap: 12
            ) {
                VectorGlyph (
                    accent: 0,
                    stroke: 2,
                    width: 72,
                    height: 72,
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 16
                ) on Tap(|ev: event!(Tap), q: query!(VectorGlyph), cmd: commands!()| {
                    if let Some(next) = q.get(ev.target).copied().map(VectorGlyph::cycle) {
                        cmd.insert(ev.target, next);
                    }
                })
                VectorGlyph (
                    accent: 1,
                    stroke: 3,
                    width: 72,
                    height: 72,
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 16
                ) on Tap(|ev: event!(Tap), q: query!(VectorGlyph), cmd: commands!()| {
                    if let Some(next) = q.get(ev.target).copied().map(VectorGlyph::cycle) {
                        cmd.insert(ev.target, next);
                    }
                })
                VectorGlyph (
                    accent: 2,
                    stroke: 4,
                    width: 72,
                    height: 72,
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 16
                ) on Tap(|ev: event!(Tap), q: query!(VectorGlyph), cmd: commands!()| {
                    if let Some(next) = q.get(ev.target).copied().map(VectorGlyph::cycle) {
                        cmd.insert(ev.target, next);
                    }
                })
            }
        }
    };
}
// #endregion recipe-vector
