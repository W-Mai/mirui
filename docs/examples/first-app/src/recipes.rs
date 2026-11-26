use super::motion::indicator_transition;
use mirui::core::model::{ModelHandle, ModelItemId, ModelList};
use mirui::core::persistence::PersistencePlugin;
use mirui::core::storage::StorageHandle;
use mirui::input::event::scroll::{ScrollConfig, ScrollOffset};
use mirui::prelude::draw::*;
use mirui::prelude::*;
use mirui::render::scene::{LineCap, LineJoin, Paint};
use mirui::text::TextPath;
use mirui::types::Transform;
use mirui::ui::widgets::{
    Button, FollowBounds, ParagraphStyle, Placeholder, Text, TextAlign, TextDocument, TextEditing,
    TextInput,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Recipe {
    Reactive,
    Layout,
    Spring,
    Vector,
    Curve,
    Scroll,
    MiniWindow,
    MiniState,
    MiniList,
    MiniStyle,
    MiniMotion,
    MiniStorage,
}

impl Recipe {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "reactive" => Some(Self::Reactive),
            "layout" => Some(Self::Layout),
            "spring" => Some(Self::Spring),
            "vector" => Some(Self::Vector),
            "curve" => Some(Self::Curve),
            "scroll" => Some(Self::Scroll),
            "mini-window" => Some(Self::MiniWindow),
            "mini-state" => Some(Self::MiniState),
            "mini-list" => Some(Self::MiniList),
            "mini-style" => Some(Self::MiniStyle),
            "mini-motion" => Some(Self::MiniMotion),
            "mini-storage" => Some(Self::MiniStorage),
            _ => None,
        }
    }

    pub fn needs_storage(self) -> bool {
        matches!(self, Self::MiniStorage)
    }
}

pub fn install_recipe<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    recipe: Recipe,
    reduced: bool,
    storage: Option<StorageHandle>,
) {
    if recipe.needs_storage()
        && let Some(storage) = storage
    {
        app.add_plugin(PersistencePlugin::new(storage).autosave(200));
    }
    if matches!(recipe, Recipe::MiniStyle) {
        //~focus-start:mini-style
        app.add_plugin(
            StyleSheetPlugin::new()
                .class("pill", style!(height: 36, border_radius: 8))
                .class("pill.on", style!(text_color: ColorToken::Primary))
                .class("pill.off", style!(text_color: ColorToken::OnSurfaceVariant)),
        );
        //~focus-end:mini-style
    }
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
        Recipe::Curve => {
            let crest = app
                .paths()
                .insert_static(WAVE_CREST.commands())
                .expect("crest path");
            let valley = app
                .paths()
                .insert_static(WAVE_VALLEY.commands())
                .expect("valley path");
            let route = app.add_model(CurveRoute::new(crest, valley)).finish();
            app.compose(root, curve_scene(route));
        }
        Recipe::Scroll => {
            let scroller = app.add_model(ScrollTarget::default()).finish();
            app.compose(root, scroll_scene(scroller, reduced));
        }
        Recipe::MiniWindow => {
            app.compose(root, mini_window_scene());
        }
        Recipe::MiniState => {
            let state = app.add_model(MiniSwitch::default()).finish();
            app.compose(root, mini_state_scene(state));
        }
        Recipe::MiniList => {
            let notes = app.add_model(MiniNotes::default()).finish();
            let draft = app.add_model(TextDocument::new()).finish();
            app.compose(root, mini_list_scene(notes, draft));
        }
        Recipe::MiniStyle => {
            let pill = app.add_model(MiniPill::default()).finish();
            app.compose(root, mini_style_scene(pill));
        }
        Recipe::MiniMotion => {
            let tabs = app.add_model(SpringTabs::default()).finish();
            app.compose(root, spring_scene(tabs, reduced));
        }
        Recipe::MiniStorage => {
            let counter = app.add_model(SavedCount::default()).finish();
            app.compose(root, mini_storage_scene(counter));
        }
    }
}

// #region mini-window
//~focus-start:mini-window
#[compose]
fn mini_window_scene() {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(20),
            row_gap: 12,
            text_color: ColorToken::OnSurface
        ) {
            Column (
                height: 64,
                padding: Padding::all(14),
                justify: JustifyContent::Center,
                bg_color: ColorToken::Secondary,
                border_radius: 10
            ) {
                Text ("Hello Mirui", font_size: 24, height: 32, text_color: ColorToken::OnSecondary)
            }
            Row (height: 44, column_gap: 10) {
                Column (
                    grow: 1.0,
                    padding: Padding::all(12),
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 8
                ) {
                    Text ("First row", font_size: 13, height: 20)
                }
                Column (
                    grow: 1.0,
                    padding: Padding::all(12),
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 8
                ) {
                    Text ("Second row", font_size: 13, height: 20)
                }
            }
        }
    };
}
//~focus-end:mini-window
// #endregion mini-window

// #region mini-state
//~focus-start:mini-state
#[model]
#[derive(Default)]
pub struct MiniSwitch {
    #[observe]
    pub checked: bool,
    #[observe]
    pub taps: u32,
}

#[model]
impl MiniSwitch {
    pub fn flip(&mut self) {
        self.checked = !self.checked;
        self.taps = self.taps.saturating_add(1);
    }
}

#[compose]
fn mini_state_scene(state: model!(MiniSwitch)) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(20),
            justify: JustifyContent::SpaceBetween,
            text_color: ColorToken::OnSurface
        ) {
            Text (
                text: ${ format_args!("Status: {} ({} taps)", if state.checked() { "Done" } else { "Open" }, state.taps()) },
                capacity: 36,
                height: 26,
                font_size: 16
            )
            Button (
                text: ${ if state.checked() { "✓ Completed — tap to undo" } else { "Tap to mark complete" } },
                height: 44,
                border_radius: 10,
                normal_color: ${ if state.checked() { ColorToken::Primary } else { ColorToken::SurfaceVariant } },
                pressed_color: ColorToken::Outline,
                text_color: ${ if state.checked() { ColorToken::OnPrimary } else { ColorToken::OnSurface } }
            ) on Tap { state.flip(); }
        }
    };
}
//~focus-end:mini-state
// #endregion mini-state

// #region mini-list
//~focus-start:mini-list
#[model]
#[derive(Default)]
pub struct NoteItem {
    pub label: String,
}

#[model]
impl NoteItem {}

#[model]
pub struct MiniNotes {
    pub items: model!(owned ModelList<NoteItem>),
    seq: u32,
}

impl Default for MiniNotes {
    fn default() -> Self {
        let mut items = ModelList::bounded(6);
        let _ = items.try_push(NoteItem { label: "Wire SPI display".into() });
        let _ = items.try_push(NoteItem { label: "Mount root view".into() });
        Self { items: items.into(), seq: 3 }
    }
}

#[model]
impl MiniNotes {
    pub fn add(&mut self, text: Option<String>) {
        let label = text.unwrap_or_else(|| format!("Item #{}", self.seq));
        if self.items.try_push(NoteItem { label }).is_ok() {
            self.seq = self.seq.saturating_add(1);
        }
    }

    pub fn remove(&mut self, id: ModelItemId) {
        let _ = self.items.remove(id);
    }
}

#[event]
fn add_note(notes: model!(MiniNotes), draft: model!(TextDocument)) {
    let text = draft.read(|s| {
        let t = s.trim();
        (!t.is_empty()).then(|| t.chars().take(28).collect::<String>())
    });
    notes.add(text);
    let _ = draft.set_text("");
}

#[event]
fn remove_note(id: ModelItemId, notes: model!(MiniNotes)) {
    notes.remove(id);
}

#[compose]
fn note_row(notes: model!(MiniNotes), id: ModelItemId, item: model!(NoteItem)) -> Entity {
    let title = item.read(|n| n.label.clone());
    ui! {
        Row (
            height: 34,
            padding: Padding::all(6),
            align: AlignItems::Center,
            justify: JustifyContent::SpaceBetween,
            bg_color: ColorToken::SurfaceVariant,
            border_radius: 7
        ) {
            Text (text: title, grow: 1.0, height: 20, font_size: 13)
            Button (
                "×",
                width: 28,
                height: 26,
                border_radius: 6,
                normal_color: ColorToken::Surface,
                pressed_color: ColorToken::Outline,
                text_color: ColorToken::OnSurfaceVariant
            ) on Tap(remove_note::callback(id, notes))
        }
    }
}

#[compose]
fn mini_list_scene(notes: model!(MiniNotes), draft: model!(TextDocument)) {
    ui! {
        Column (grow: 1.0, padding: Padding::all(16), row_gap: 8, text_color: ColorToken::OnSurface) {
            Row (height: 36, column_gap: 8) {
                TextInput (
                    grow: 1.0,
                    height: 36,
                    padding: Padding::all(8),
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 8
                ) [
                    TextEditing::new(draft.clone()),
                    Placeholder("Type an item..."),
                ] on TextEditing::Submitted(add_note::callback(notes, draft))
                Button (
                    "+ Add",
                    width: 64,
                    height: 36,
                    border_radius: 8,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary,
                    text_color: ColorToken::OnPrimary
                ) on Tap(add_note::callback(notes, draft))
            }
            Column (grow: 1.0, row_gap: 6, clip_children: true) {
                walk ${ notes.items().entries() } with entry by entry.0 {
                    note_row (notes, entry.0, entry.1)
                }
            }
        }
    };
}
//~focus-end:mini-list
// #endregion mini-list

// #region mini-style
//~focus-start:mini-style
#[model]
#[derive(Default)]
pub struct MiniPill {
    #[observe]
    pub active: bool,
}

#[model]
impl MiniPill {
    pub fn toggle(&mut self) {
        self.active = !self.active;
    }
}

#[compose]
fn status_badge(title: prop!(String), active: prop!(bool), pill: model!(MiniPill)) -> Entity {
    ui! {
        Button (
            text: ${ title },
            grow: 1.0,
            normal_color: ColorToken::SurfaceVariant,
            pressed_color: ColorToken::Outline,
            class: ${ ("pill", if active.get() { "pill.on" } else { "pill.off" }) }
        ) on Tap { pill.toggle(); }
    }
}

#[compose]
fn mini_style_scene(pill: model!(MiniPill)) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(20),
            justify: JustifyContent::SpaceBetween,
            text_color: ColorToken::OnSurface
        ) {
            Text (
                "Both buttons share #[compose] & the 'pill' stylesheet class",
                height: 20,
                font_size: 13,
                text_color: ColorToken::OnSurfaceVariant
            )
            Row (height: 44, column_gap: 10, align: AlignItems::Center) {
                status_badge (
                    ${ if pill.active() { "● Active class (.pill.on)" } else { "○ Muted class (.pill.off)" } },
                    ${ pill.active() },
                    pill
                )
                status_badge ("Mirror badge", ${ !pill.active() }, pill)
            }
        }
    };
}
//~focus-end:mini-style
// #endregion mini-style

// #region mini-storage
//~focus-start:mini-storage
#[model(persistence(key = "mirui_mini_saved_count"))]
pub struct SavedCount {
    #[observe]
    #[persist]
    pub value: i32,
}

impl Default for SavedCount {
    fn default() -> Self {
        Self { value: 1 }
    }
}

#[model]
impl SavedCount {
    pub fn add(&mut self, delta: i32) {
        self.value += delta;
    }

    pub fn reset(&mut self) {
        self.value = 1;
    }
}

#[compose]
fn mini_storage_scene(counter: model!(SavedCount)) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(20),
            justify: JustifyContent::SpaceBetween,
            text_color: ColorToken::OnSurface
        ) {
            Row (height: 28, justify: JustifyContent::SpaceBetween, align: AlignItems::Center) {
                Text (
                    "#[persist] snapshot",
                    grow: 1.0,
                    height: 20,
                    font_size: 13,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${ format_args!("Saved: {}", counter.value()) },
                    capacity: 20,
                    grow: 1.0,
                    font_size: 20,
                    height: 28,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
            }
            Row (height: 40, column_gap: 8) {
                Button (
                    "+ 1 (Autosaves)",
                    grow: 1.0,
                    height: 40,
                    border_radius: 9,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary,
                    text_color: ColorToken::OnPrimary
                ) on Tap { counter.add(1); }
                Button (
                    "Reset",
                    width: 80,
                    height: 40,
                    border_radius: 9,
                    normal_color: ColorToken::SurfaceVariant,
                    pressed_color: ColorToken::Outline,
                    text_color: ColorToken::OnSurface
                ) on Tap { counter.reset(); }
            }
        }
    };
}
//~focus-end:mini-storage
// #endregion mini-storage

// #region recipe-reactive
//~focus-start:reactive
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
                Text (
                    "Signal binding",
                    grow: 1.0,
                    height: 20,
                    font_size: 14,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${ format_args!("{} %", gauge.level()) },
                    capacity: 12,
                    grow: 1.0,
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
//~focus-end:reactive
// #endregion recipe-reactive

// #region recipe-layout
//~focus-start:layout
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
            Row (height: 34, column_gap: 8, align: AlignItems::Center) {
                Text (
                    text: ${ if deck.compact() { "Compact · 96 px card" } else { "Expanded · 156 px card" } },
                    capacity: 28,
                    grow: 1.0,
                    min_width: 0,
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
//~focus-end:layout
// #endregion recipe-layout

// #region recipe-spring
//~focus-start:spring,mini-motion
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
                "Tap any pill — FollowBounds retargets mid-flight",
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
                //~add-start:mini-motion
                View (
                    id: "spring_ring",
                    border_width: 2,
                    border_color: ColorToken::Primary,
                    border_radius: 9
                ) [
                    FollowBounds::new(id("spring_first"))
                        .with_transition(indicator_transition(reduced)),
                ]
                //~add-end:mini-motion
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
//~focus-end:spring,mini-motion
// #endregion recipe-spring

// #region recipe-vector
//~focus-start:vector
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
//~focus-end:vector
// #endregion recipe-vector

// #region recipe-curve
//~focus-start:curve
static WAVE_CREST: Path = path!(M 12 68 Q 70 20 128 68 Q 186 116 244 68);
static WAVE_VALLEY: Path = path!(M 12 52 Q 70 100 128 52 Q 186 4 244 52);

#[model]
pub struct CurveRoute {
    crest: PathId,
    valley: PathId,
    #[observe]
    pub inverted: bool,
    #[observe]
    pub offset: i32,
}

impl CurveRoute {
    pub fn new(crest: PathId, valley: PathId) -> Self {
        Self {
            crest,
            valley,
            inverted: false,
            offset: 8,
        }
    }
}

#[model]
impl CurveRoute {
    #[observe]
    pub fn placement(&self) -> TextPath {
        let path = if self.inverted { self.valley } else { self.crest };
        TextPath::new(path).with_offset(Fixed::from_int(self.offset))
    }

    pub fn flip(&mut self) {
        self.inverted = !self.inverted;
    }

    pub fn advance(&mut self) {
        self.offset = (self.offset + 18) % 72;
    }
}

#[compose]
fn curve_scene(route: model!(CurveRoute)) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(18),
            justify: JustifyContent::SpaceBetween,
            text_color: ColorToken::OnSurface
        ) {
            Row (height: 32, column_gap: 6, align: AlignItems::Center) {
                Text (
                    text: ${ format_args!("+{} px", route.offset()) },
                    capacity: 16,
                    grow: 1.0,
                    min_width: 0,
                    height: 22,
                    font_size: 13,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Button (
                    "Slide +18",
                    width: 72,
                    height: 30,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::SurfaceVariant,
                    pressed_color: ColorToken::Outline,
                    text_color: ColorToken::OnSurface
                ) on Tap { route.advance(); }
                Button (
                    "Flip wave",
                    width: 74,
                    height: 30,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary,
                    text_color: ColorToken::OnPrimary
                ) on Tap { route.flip(); }
            }
            View (
                grow: 1.0,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 12,
                clip_children: true
            ) {
                Text (
                    "mirui rides the wave",
                    path: ${ route.placement() },
                    position: Position::Absolute,
                    left: 6,
                    top: 10,
                    width: Dimension::percent(96),
                    height: Dimension::percent(85),
                    font_size: 15,
                    text_color: ColorToken::Primary
                )
            }
        }
    };
}
//~focus-end:curve
// #endregion recipe-curve

// #region recipe-scroll
//~focus-start:scroll
#[model]
#[derive(Default)]
pub struct ScrollTarget {
    #[observe]
    pub offset_y: i32,
}

#[model]
impl ScrollTarget {
    pub fn jump(&mut self, y: i32) {
        self.offset_y = y;
    }
}

#[compose]
fn scroll_item(title: &'static str) -> Entity {
    ui! {
        Row (
            height: 38,
            shrink: 0.0,
            padding: Padding::all(10),
            align: AlignItems::Center,
            bg_color: ColorToken::Surface,
            border_radius: 8
        ) {
            Text (title, height: 18, font_size: 13)
        }
    }
}

#[compose]
fn scroll_scene(scroller: model!(ScrollTarget), reduced: bool) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(16),
            row_gap: 10,
            text_color: ColorToken::OnSurface
        ) {
            Row (height: 32, column_gap: 6, align: AlignItems::Center) {
                Button (
                    "Top (#1)",
                    grow: 1.0,
                    height: 30,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::SurfaceVariant,
                    pressed_color: ColorToken::Outline,
                    text_color: ColorToken::OnSurface
                ) on Tap { scroller.jump(0); }
                Button (
                    "Middle (#4)",
                    grow: 1.0,
                    height: 30,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::SurfaceVariant,
                    pressed_color: ColorToken::Outline,
                    text_color: ColorToken::OnSurface
                ) on Tap { scroller.jump(96); }
                Button (
                    "Bottom (#7)",
                    grow: 1.0,
                    height: 30,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary,
                    text_color: ColorToken::OnPrimary
                ) on Tap { scroller.jump(180); }
            }
            Column (
                grow: 1.0,
                min_height: 0,
                padding: Padding::all(8),
                row_gap: 6,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 10,
                clip_children: true,
                scroll_offset: ${
                    ScrollOffset {
                        x: Fixed::ZERO,
                        y: Fixed::from_int(scroller.offset_y()),
                    }
                }
            ) [
                ScrollOffset::default(),
                ScrollConfig::default().with_transition(indicator_transition(reduced)),
            ] {
                scroll_item ("01 · FramebufSurface (MCU SPI)")
                scroll_item ("02 · SdlSurface (Desktop CPU)")
                scroll_item ("03 · SdlGpuSurface (OpenGL / Metal)")
                scroll_item ("04 · WgpuSurface (Vulkan / DX12)")
                scroll_item ("05 · WebCanvasSurface (Browser)")
                scroll_item ("06 · LinuxDrmSurface (KMS / GBM)")
                scroll_item ("07 · NuttxSurface (Embedded RTOS)")
            }
        }
    };
}
//~focus-end:scroll
// #endregion recipe-scroll
