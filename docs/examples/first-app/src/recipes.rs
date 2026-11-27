use super::motion::indicator_transition;
use mirui::core::model::{ModelHandle, ModelItemId, ModelList};
use mirui::core::persistence::PersistencePlugin;
use mirui::core::reactive::Signal;
use mirui::core::storage::StorageHandle;
use mirui::input::event::scroll::{ScrollConfig, ScrollOffset};
use mirui::prelude::draw::*;
use mirui::prelude::*;
use mirui::render::path::{PathCmd, PathStore};
use mirui::render::scene::{LineCap, LineJoin, Paint};
use mirui::text::{PathCaretGeometry, PathDirection, TextGeometry, TextPath};
use mirui::types::Transform;
use mirui::ui::Outputs;
use mirui::ui::widgets::navigation::{
    self, CreatePage as StackCreatePage, NavigationPath, NavigationStack, PageTransition,
};
use mirui::ui::widgets::page_view::{self, CreatePage as ViewCreatePage, PageView};
use mirui::ui::widgets::{
    Button, FollowBounds, ParagraphStyle, Placeholder, Text, TextAlign, TextDocument, TextEditing,
    TextInput, TextOverflow, TextWrap,
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
    TypoEllipsis,
    TypoStaticPath,
    TypoMultiline,
    TypoRange,
    TypoSignalPath,
    TypoElastic,
    TypoBend,
    TypoCarets,
    EditorShared,
    NavStack,
    NavPageView,
    ComposeNiche,
    StateAt,
    PortingInspector,
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
            "typo-ellipsis" => Some(Self::TypoEllipsis),
            "typo-static" => Some(Self::TypoStaticPath),
            "typo-multiline" => Some(Self::TypoMultiline),
            "typo-range" => Some(Self::TypoRange),
            "typo-signal" => Some(Self::TypoSignalPath),
            "typo-elastic" => Some(Self::TypoElastic),
            "typo-bend" => Some(Self::TypoBend),
            "typo-carets" => Some(Self::TypoCarets),
            "editor-shared" => Some(Self::EditorShared),
            "nav-stack" => Some(Self::NavStack),
            "nav-pageview" => Some(Self::NavPageView),
            "compose-niche" => Some(Self::ComposeNiche),
            "state-at" => Some(Self::StateAt),
            "porting-inspector" => Some(Self::PortingInspector),
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
            app.compose(root, scroll_scene(reduced));
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
        Recipe::TypoEllipsis => {
            let mode = app.add_model(OverflowDemo::default()).finish();
            app.compose(root, typo_ellipsis_scene(mode));
        }
        Recipe::TypoStaticPath => {
            let arc = register_arc(app);
            app.compose(root, curved_title(arc));
        }
        Recipe::TypoMultiline => {
            let lines = register_lines(app);
            app.compose(root, multiline_label(lines));
        }
        Recipe::TypoRange => {
            let ring = app
                .paths()
                .insert_static(RING_PATH.commands())
                .expect("ring path");
            let state = app.add_model(RangeControl::new(ring)).finish();
            app.compose(root, typo_range_scene(state));
        }
        Recipe::TypoSignalPath => {
            let first = app
                .paths()
                .insert_static(WAVE_CREST.commands())
                .expect("first path");
            let second = app
                .paths()
                .insert_static(WAVE_VALLEY.commands())
                .expect("second path");
            app.compose(root, selectable_path(first, second));
        }
        Recipe::TypoElastic => {
            let amp = Signal::new(Fixed::from_int(28));
            app.compose(root, elastic_baseline(amp));
        }
        Recipe::TypoBend => {
            let mut geometry = Path::try_with_capacity(2).expect("path storage");
            geometry
                .move_to(Point::new(16, 62))
                .quad_to(Point::new(136, 98), Point::new(256, 62));
            let path = app.paths().insert(geometry).expect("path capacity");
            app.compose(root, bendable_label(path));
        }
        Recipe::TypoCarets => {
            app.with_widget(caret_overlay::view());
            let arc = register_arc(app);
            app.compose(root, typo_carets_scene(arc));
        }
        Recipe::EditorShared => {
            app.with_widget(search_view::view());
            let form = app.add_model(SearchForm::default()).finish();
            let _ = form.query().set_capacity(128);
            let _ = form.query().set_text("Mirui Canvas");
            app.compose(root, editor_shared_scene(form));
        }
        Recipe::NavStack => {
            app.with_widget(navigation::view::<StackRoute>());
            let path = app
                .add_model(NavigationPath::<StackRoute>::with_capacity(6))
                .finish();
            app.compose(root, nav_stack_scene(path, reduced));
        }
        Recipe::NavPageView => {
            app.with_widget(page_view::view::<ViewPage>());
            let session = app.add_model(ViewSession::default()).finish();
            app.compose(root, nav_pageview_scene(session, reduced));
        }
        Recipe::ComposeNiche => {
            app.with_widget(ui!(compose StatusCard));
            let state = app.add_model(NicheState::default()).finish();
            app.compose(root, compose_niche_scene(state));
        }
        Recipe::StateAt => {
            app.compose(root, state_at_scene());
        }
        Recipe::PortingInspector => {
            let probe = app.add_model(SurfaceProbe::default()).finish();
            app.compose(root, porting_inspector_scene(probe));
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
        let _ = items.try_push(NoteItem {
            label: "Wire SPI display".into(),
        });
        let _ = items.try_push(NoteItem {
            label: "Mount root view".into(),
        });
        Self {
            items: items.into(),
            seq: 3,
        }
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
        let path = if self.inverted {
            self.valley
        } else {
            self.crest
        };
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
fn scroll_button(label: &'static str, list: Entity, y: i32, primary: bool) -> Entity {
    ui! {
        Button (
            label,
            grow: 1.0,
            height: 30,
            font_size: 12,
            border_radius: 7,
            normal_color: if primary { ColorToken::Primary } else { ColorToken::SurfaceVariant },
            pressed_color: if primary { ColorToken::Secondary } else { ColorToken::Outline },
            text_color: if primary { ColorToken::OnPrimary } else { ColorToken::OnSurface }
        ) on Tap {
            commands!().scroll_to(
                list,
                ScrollOffset {
                    x: Fixed::ZERO,
                    y: Fixed::from_int(y),
                },
            );
        }
    }
}

#[compose]
fn scroll_scene(reduced: bool) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(16),
            row_gap: 10,
            text_color: ColorToken::OnSurface
        ) {
            Row (height: 32, column_gap: 6, align: AlignItems::Center) {
                scroll_button ("Top (#1)", id("scroll_list"), 0, false)
                scroll_button ("Middle (#4)", id("scroll_list"), 132, false)
                scroll_button ("Bottom (#7)", id("scroll_list"), 264, true)
            }
            Column (
                id: "scroll_list",
                grow: 1.0,
                min_height: 0,
                padding: Padding::all(8),
                row_gap: 6,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 10,
                clip_children: true
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

//~focus-start:typo-ellipsis
#[model]
#[derive(Default)]
pub struct OverflowDemo {
    #[observe]
    pub wrap: bool,
}

#[model]
impl OverflowDemo {
    pub fn toggle(&mut self) {
        self.wrap = !self.wrap;
    }
}

#[compose]
fn typo_ellipsis_scene(mode: model!(OverflowDemo)) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(18),
            justify: JustifyContent::SpaceBetween,
            text_color: ColorToken::OnSurface
        ) {
            Row (height: 32, align: AlignItems::Center) {
                Text (
                    text: ${ if mode.wrap() { "Wrap · max_lines: 2" } else { "NoWrap · Ellipsis (1 line)" } },
                    grow: 1.0,
                    height: 20,
                    font_size: 12,
                    text_color: ColorToken::OnSurfaceVariant
                )
                Button (
                    "Toggle wrap",
                    width: 96,
                    height: 30,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary,
                    text_color: ColorToken::OnPrimary
                ) on Tap { mode.toggle(); }
            }
            View (
                height: 68,
                padding: Padding::all(14),
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 10
            ) {
                Text (
                    "A long status label that clips with an ellipsis or wraps across two lines when toggled.",
                    width: Dimension::percent(100),
                    height: 40,
                    font_size: 14,
                    paragraph: ${
                        ParagraphStyle {
                            wrap: if mode.wrap() { TextWrap::Word } else { TextWrap::NoWrap },
                            max_lines: Some(if mode.wrap() { 2 } else { 1 }),
                            overflow: TextOverflow::Ellipsis,
                            ..ParagraphStyle::default()
                        }
                    }
                )
            }
        }
    };
}
//~focus-end:typo-ellipsis

//~focus-start:typo-static
static ARC: Path = path!(M 18 76 Q 136 8 254 76);

fn register_arc<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) -> PathId {
    app.paths()
        .insert_static(ARC.commands())
        .expect("path capacity")
}

#[compose]
fn curved_title(arc: PathId) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(18),
            justify: JustifyContent::Center
        ) {
            View (
                height: 104,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 12
            ) {
                Text (
                    "Status · Ready on static arc",
                    path: arc,
                    position: Position::Absolute,
                    left: 8,
                    top: 10,
                    width: Dimension::percent(94),
                    height: 86,
                    font_size: 16,
                    text_color: ColorToken::Primary
                )
            }
        }
    };
}
//~focus-end:typo-static

//~focus-start:typo-multiline
static LINES: Path = path!(
    M 18 32 L 254 32
    M 24 68 Q 138 106 252 68
);

fn register_lines<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) -> PathId {
    app.paths()
        .insert_static(LINES.commands())
        .expect("path capacity")
}

#[compose]
fn multiline_label(lines: PathId) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(18),
            justify: JustifyContent::Center
        ) {
            View (
                height: 112,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 12
            ) {
                Text (
                    "The first line stays straight\nand the next follows the curve.",
                    path: lines,
                    position: Position::Absolute,
                    left: 8,
                    top: 8,
                    width: Dimension::percent(94),
                    height: 96,
                    font_size: 14,
                    text_color: ColorToken::OnSurface,
                    paragraph: ParagraphStyle {
                        wrap: TextWrap::Word,
                        max_lines: Some(2),
                        ..ParagraphStyle::default()
                    }
                )
            }
        }
    };
}
//~focus-end:typo-multiline

//~focus-start:typo-range
static RING_PATH: Path = path!(
    M 14 26 L 254 26
    M 22 68 Q 138 22 254 68
);

#[model]
pub struct RangeControl {
    path: PathId,
    #[observe]
    pub reversed: bool,
    #[observe]
    pub offset: i32,
}

#[model]
impl RangeControl {
    pub fn new(path: PathId) -> Self {
        Self {
            path,
            reversed: false,
            offset: 8,
        }
    }

    #[observe]
    pub fn placement(&self) -> TextPath {
        let dir = if self.reversed {
            PathDirection::Reverse
        } else {
            PathDirection::Forward
        };
        TextPath::new(self.path)
            .with_subpath(1)
            .with_range(Fixed::from_int(12)..Fixed::from_int(230))
            .with_offset(Fixed::from_int(self.offset))
            .with_direction(dir)
    }

    pub fn flip_dir(&mut self) {
        self.reversed = !self.reversed;
    }

    pub fn step_offset(&mut self) {
        self.offset = (self.offset + 16) % 64;
    }
}

#[compose]
fn typo_range_scene(state: model!(RangeControl)) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(18),
            justify: JustifyContent::SpaceBetween,
            text_color: ColorToken::OnSurface
        ) {
            Row (height: 32, column_gap: 6, align: AlignItems::Center) {
                Text (
                    text: ${
                        format_args!(
                            "{} · +{} px",
                            if state.reversed() { "Reverse" } else { "Forward" },
                            state.offset()
                        )
                    },
                    capacity: 28,
                    grow: 1.0,
                    height: 20,
                    font_size: 12,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Button (
                    "Offset +16",
                    width: 80,
                    height: 30,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::SurfaceVariant,
                    pressed_color: ColorToken::Outline,
                    text_color: ColorToken::OnSurface
                ) on Tap { state.step_offset(); }
                Button (
                    "Reverse",
                    width: 72,
                    height: 30,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary,
                    text_color: ColorToken::OnPrimary
                ) on Tap { state.flip_dir(); }
            }
            View (
                height: 92,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 12
            ) {
                Text (
                    "Range & direction on subpath 1",
                    path: ${ state.placement() },
                    position: Position::Absolute,
                    left: 6,
                    top: 8,
                    width: Dimension::percent(95),
                    height: 76,
                    font_size: 14,
                    text_color: ColorToken::Primary
                )
            }
        }
    };
}
//~focus-end:typo-range

//~focus-start:typo-signal
#[compose]
fn selectable_path(first: PathId, second: PathId) {
    let selected = Signal::new(first);
    let selection = selected.clone();

    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(18),
            justify: JustifyContent::SpaceBetween,
            text_color: ColorToken::OnSurface
        ) {
            Row (height: 32, align: AlignItems::Center) {
                Text (
                    "Signal<PathId> binding",
                    grow: 1.0,
                    height: 20,
                    font_size: 12,
                    text_color: ColorToken::OnSurfaceVariant
                )
                Button (
                    "Switch path",
                    width: 92,
                    height: 30,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary,
                    text_color: ColorToken::OnPrimary
                ) on Tap {
                    selected.update(|path| {
                        *path = if *path == first { second } else { first };
                    });
                }
            }
            View (
                height: 92,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 12
            ) {
                Text (
                    "Live route swaps PathId",
                    path: $selection,
                    position: Position::Absolute,
                    left: 6,
                    top: 10,
                    width: Dimension::percent(95),
                    height: 74,
                    font_size: 15,
                    text_color: ColorToken::Primary
                )
            }
        }
    };
}
//~focus-end:typo-signal

//~focus-start:typo-elastic
#[compose]
fn elastic_baseline(amplitude: Signal<Fixed>) {
    let mut geometry = Path::try_with_capacity(2).expect("path storage");
    geometry
        .move_to(Point::new(16, 54))
        .quad_to(Point::new(136, 54), Point::new(256, 54));
    let path = cx.paths().insert(geometry).expect("path capacity");
    let live_amplitude = amplitude.clone();
    let toggle_amp = amplitude.clone();

    cx.bind_path(path, move |geometry| {
        let y = Fixed::from_int(54) - live_amplitude.get();
        geometry
            .set_command(
                1,
                PathCmd::QuadTo {
                    ctrl: Point::new(Fixed::from_int(136), y),
                    end: Point::new(256, 54),
                },
            )
            .expect("stable command shape");
    })
    .expect("mutable path");

    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(18),
            justify: JustifyContent::SpaceBetween,
            text_color: ColorToken::OnSurface
        ) {
            Row (height: 32, align: AlignItems::Center) {
                Text (
                    "cx.bind_path(path, ...)",
                    grow: 1.0,
                    height: 20,
                    font_size: 12,
                    text_color: ColorToken::OnSurfaceVariant
                )
                Button (
                    "Invert amplitude",
                    width: 112,
                    height: 30,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary,
                    text_color: ColorToken::OnPrimary
                ) on Tap {
                    toggle_amp.update(|a| *a = -*a);
                }
            }
            View (
                height: 92,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 12
            ) {
                Text (
                    "Signal-driven curve",
                    path: path,
                    position: Position::Absolute,
                    left: 8,
                    top: 8,
                    width: Dimension::percent(94),
                    height: 76,
                    font_size: 15,
                    text_color: ColorToken::Primary
                )
            }
        }
    };
}
//~focus-end:typo-elastic

//~focus-start:typo-bend
#[compose]
fn bendable_label(path: PathId) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(18),
            justify: JustifyContent::Center
        ) {
            View (
                height: 108,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 12
            ) {
                Text (
                    "Tap this text to bend its curve",
                    path: path,
                    position: Position::Absolute,
                    left: 8,
                    top: 10,
                    width: Dimension::percent(94),
                    height: 88,
                    font_size: 15,
                    text_color: ColorToken::Primary
                ) on Tap(|paths: res!(mut PathStore)| {
                    paths
                        .edit(path, |geometry| {
                            let next_y = match geometry.commands().get(1) {
                                Some(PathCmd::QuadTo { ctrl, .. }) if ctrl.y < Fixed::from_int(40) => 98,
                                _ => 12,
                            };
                            geometry
                                .set_command(
                                    1,
                                    PathCmd::QuadTo {
                                        ctrl: Point::new(136, next_y),
                                        end: Point::new(256, 62),
                                    },
                                )
                                .expect("stable command shape");
                        })
                        .expect("mutable path");
                })
            }
        }
    };
}
//~focus-end:typo-bend

//~focus-start:typo-carets
#[component]
#[depends_on(TextGeometry)]
struct CaretOverlay;

#[view(component = CaretOverlay)]
fn caret_overlay(entity: entity!(), geometries: res!(Outputs<TextGeometry>), paint: paint!()) {
    let Some(geometry) = geometries.get(entity) else {
        return;
    };
    let mut storage = [PathCaretGeometry::default(); 64];
    let Ok(carets) = geometry.carets_into(&mut storage) else {
        return;
    };
    let rect = paint.rect();
    for caret in carets {
        let [start, end] = caret.segment();
        paint.draw(&DrawCommand::Line {
            p1: Point::new(rect.x + start.x, rect.y + start.y),
            p2: Point::new(rect.x + end.x, rect.y + end.y),
            transform: paint.transform(),
            color: Color::rgb(70, 160, 255),
            width: Fixed::ONE,
            opa: 255,
        });
    }
}

#[compose]
fn typo_carets_scene(arc: PathId) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(18),
            justify: JustifyContent::Center
        ) {
            View (
                height: 108,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 12
            ) {
                Text (
                    "Oriented carets along arc",
                    path: arc,
                    position: Position::Absolute,
                    left: 8,
                    top: 14,
                    width: Dimension::percent(94),
                    height: 84,
                    font_size: 16,
                    text_color: ColorToken::OnSurface
                ) [CaretOverlay]
            }
        }
    };
}
//~focus-end:typo-carets

//~focus-start:editor-shared
#[model]
pub struct SearchForm {
    query: model!(owned TextDocument),
    #[observe]
    pub submissions: u32,
}

impl Default for SearchForm {
    fn default() -> Self {
        Self {
            query: TextDocument::new().into(),
            submissions: 0,
        }
    }
}

#[model]
impl SearchForm {
    pub fn submit(&mut self) {
        self.submissions += 1;
    }

    pub fn preset(&mut self, value: &str) {
        let _ = self.query.set_text(value);
    }
}

#[component]
#[derive(Default)]
struct SearchEditor {
    document: model!(optional TextDocument),
}

#[event]
fn attach_search(
    node: entity!(),
    search: query!(SearchEditor),
    editing: query!(TextEditing),
    appearance: query!(TextInput),
    commands: commands!(),
) {
    let Some(search) = search.get(node) else {
        return;
    };
    if editing.get(node).is_none() {
        let editor = search
            .document
            .clone()
            .map(TextEditing::new)
            .unwrap_or_default();
        commands.insert(node, editor);
    }
    if appearance.get(node).is_none() {
        commands.insert(node, TextInput::new());
    }
}

#[view(component = SearchEditor, attach = attach_search::callback())]
fn search_view() {}

#[compose]
fn editor_shared_scene(form: model!(SearchForm)) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(18),
            justify: JustifyContent::SpaceBetween,
            text_color: ColorToken::OnSurface
        ) {
            Row (height: 32, column_gap: 6, align: AlignItems::Center) {
                Text (
                    text: ${ format_args!("Submitted: {}", form.submissions()) },
                    capacity: 24,
                    grow: 1.0,
                    height: 20,
                    font_size: 12,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Button (
                    "Set 'SPI'",
                    width: 72,
                    height: 28,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::SurfaceVariant,
                    pressed_color: ColorToken::Outline,
                    text_color: ColorToken::OnSurface
                ) on Tap { form.preset("SPI 128x160"); }
                Button (
                    "Submit",
                    width: 68,
                    height: 28,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary,
                    text_color: ColorToken::OnPrimary
                ) on Tap { form.submit(); }
            }
            Column (row_gap: 10) {
                SearchEditor (
                    document: Some(form.query()),
                    width: Dimension::percent(100),
                    height: 38,
                    padding: Padding::all(9),
                    font_size: 14,
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 8,
                    border_width: 1,
                    border_color: ColorToken::Outline
                ) [Placeholder("Primary SearchEditor")] on TextEditing::Submitted {
                    form.submit();
                }
                TextInput (
                    width: Dimension::percent(100),
                    height: 38,
                    padding: Padding::all(9),
                    font_size: 14,
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 8,
                    border_width: 1,
                    border_color: ColorToken::Outline
                ) [
                    TextEditing::new(form.query()),
                    Placeholder("Mirror TextInput (independent caret)"),
                ]
            }
        }
    };
}
//~focus-end:editor-shared

//~focus-start:nav-stack
#[derive(Clone, Copy)]
enum StackRoute {
    Devices,
    Detail(u32),
}

#[compose]
fn stack_devices_page(path: model!(NavigationPath<StackRoute>)) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(14),
            row_gap: 8,
            bg_color: ColorToken::SurfaceVariant,
            border_radius: 10
        ) {
            Text ("Pushed · Devices", height: 22, font_size: 15, text_color: ColorToken::Primary)
            Row (height: 34, column_gap: 8) {
                Button (
                    "Inspect #7 →",
                    grow: 1.0,
                    height: 32,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary,
                    text_color: ColorToken::OnPrimary
                ) on Tap { let _ = path.push(StackRoute::Detail(7)); }
                Button (
                    "← Back",
                    width: 76,
                    height: 32,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::Surface,
                    pressed_color: ColorToken::Outline,
                    text_color: ColorToken::OnSurface
                ) on Tap { let _ = path.pop(); }
            }
        }
    }
}

#[compose]
fn stack_detail_page(path: model!(NavigationPath<StackRoute>), id: u32) -> Entity {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(14),
            row_gap: 8,
            bg_color: ColorToken::Secondary,
            border_radius: 10
        ) {
            Text (
                text: ${ format_args!("Device #{id} · ESP32-C3") },
                capacity: 28,
                height: 22,
                font_size: 15,
                text_color: ColorToken::OnSecondary
            )
            Row (height: 34, column_gap: 8) {
                Button (
                    "← Pop one",
                    grow: 1.0,
                    height: 32,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::Surface,
                    pressed_color: ColorToken::Outline,
                    text_color: ColorToken::OnSurface
                ) on Tap { let _ = path.pop(); }
                Button (
                    "Clear root",
                    width: 88,
                    height: 32,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Outline,
                    text_color: ColorToken::OnPrimary
                ) on Tap { let _ = path.clear(); }
            }
        }
    }
}

#[event]
fn build_stack_page(
    event: event!(StackCreatePage<StackRoute>),
    path: model!(NavigationPath<StackRoute>),
) -> Entity {
    match *event.route() {
        StackRoute::Devices => ui! { stack_devices_page(path) },
        StackRoute::Detail(id) => ui! { stack_detail_page(path, id) },
    }
}

#[compose]
fn nav_stack_scene(path: model!(NavigationPath<StackRoute>), reduced: bool) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(18),
            row_gap: 10,
            text_color: ColorToken::OnSurface
        ) {
            Text (
                text: ${ format_args!("Stack depth: {}", path.len()) },
                capacity: 24,
                height: 20,
                font_size: 12,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            NavigationStack (
                path: path.clone(),
                grow: 1.0,
                transition: if reduced {
                    PageTransition::None
                } else {
                    PageTransition::slide(mirui::anim::SMOOTH)
                }
            ) {
                Column (
                    grow: 1.0,
                    padding: Padding::all(14),
                    justify: JustifyContent::SpaceBetween,
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 10
                ) {
                    Text ("Root · Home page", height: 22, font_size: 15)
                    Button (
                        "Push Devices →",
                        width: Dimension::percent(100),
                        height: 34,
                        font_size: 13,
                        border_radius: 8,
                        normal_color: ColorToken::Primary,
                        pressed_color: ColorToken::Secondary,
                        text_color: ColorToken::OnPrimary
                    ) on Tap { let _ = path.push(StackRoute::Devices); }
                }
            } on CreatePage(build_stack_page::callback(path))
        }
    };
}
//~focus-end:nav-stack

//~focus-start:nav-pageview
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewPage {
    #[default]
    Overview,
    Metrics,
    Logs,
}

#[model]
#[derive(Default)]
pub struct ViewSession {
    #[observe]
    pub page: ViewPage,
}

#[model]
impl ViewSession {
    pub fn select(&mut self, page: ViewPage) {
        self.page = page;
    }
}

#[event]
fn create_view_page(event: event!(ViewCreatePage<ViewPage>)) -> Entity {
    match event.route() {
        ViewPage::Overview => ui! {
            Column (
                grow: 1.0,
                padding: Padding::all(16),
                justify: JustifyContent::Center,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 10
            ) {
                Text ("Overview · 160 MHz RISC-V", height: 24, font_size: 15, text_color: ColorToken::Primary)
            }
        },
        ViewPage::Metrics => ui! {
            Column (
                grow: 1.0,
                padding: Padding::all(16),
                justify: JustifyContent::Center,
                bg_color: ColorToken::Secondary,
                border_radius: 10
            ) {
                Text ("Metrics · Dirty flush 0.4 ms", height: 24, font_size: 15, text_color: ColorToken::OnSecondary)
            }
        },
        ViewPage::Logs => ui! {
            Column (
                grow: 1.0,
                padding: Padding::all(16),
                justify: JustifyContent::Center,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 10
            ) {
                Text ("Logs · 0 allocations in frame", height: 24, font_size: 15, text_color: ColorToken::OnSurface)
            }
        },
    }
}

#[compose]
fn nav_pageview_scene(session: model!(ViewSession), reduced: bool) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(18),
            row_gap: 10,
            text_color: ColorToken::OnSurface
        ) {
            Row (height: 32, column_gap: 6) {
                Button (
                    "Overview",
                    grow: 1.0,
                    height: 30,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::SurfaceVariant,
                    pressed_color: ColorToken::Outline,
                    text_color: ColorToken::OnSurface
                ) on Tap { session.select(ViewPage::Overview); }
                Button (
                    "Metrics",
                    grow: 1.0,
                    height: 30,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::SurfaceVariant,
                    pressed_color: ColorToken::Outline,
                    text_color: ColorToken::OnSurface
                ) on Tap { session.select(ViewPage::Metrics); }
                Button (
                    "Logs",
                    grow: 1.0,
                    height: 30,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary,
                    text_color: ColorToken::OnPrimary
                ) on Tap { session.select(ViewPage::Logs); }
            }
            PageView (
                page: ${ session.page() },
                grow: 1.0,
                transition: if reduced {
                    PageTransition::None
                } else {
                    PageTransition::slide(mirui::anim::SNAPPY)
                }
            ) on CreatePage(create_view_page::callback())
        }
    };
}
//~focus-end:nav-pageview

//~focus-start:compose-niche
ui!(compose StatusCard(title: prop!(String)) {
    Column (
        padding: Padding::all(14),
        row_gap: 8,
        bg_color: ColorToken::SurfaceVariant,
        border_radius: 10
    ) {
        Text (text: ${ title }, height: 22, font_size: 15, text_color: ColorToken::Primary)
        @@body
    }
});

#[model]
#[derive(Default)]
pub struct NicheState {
    #[observe]
    pub online: bool,
}

#[model]
impl NicheState {
    pub fn toggle(&mut self) {
        self.online = !self.online;
    }
}

#[compose]
fn compose_niche_scene(state: model!(NicheState)) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(18),
            justify: JustifyContent::SpaceBetween,
            text_color: ColorToken::OnSurface
        ) {
            StatusCard (
                title: ${ if state.online() { "StatusCard · Online" } else { "StatusCard · Standby" } }
            ) {
                @body {
                    Row (height: 32, align: AlignItems::Center) {
                        Text (
                            "Slot @body injected by caller",
                            grow: 1.0,
                            height: 20,
                            font_size: 12,
                            text_color: ColorToken::OnSurfaceVariant
                        )
                        Button (
                            "Toggle",
                            width: 72,
                            height: 28,
                            font_size: 12,
                            border_radius: 7,
                            normal_color: ColorToken::Primary,
                            pressed_color: ColorToken::Secondary,
                            text_color: ColorToken::OnPrimary
                        ) on Tap { state.toggle(); }
                    }
                }
            }
        }
    };
}
//~focus-end:compose-niche

//~focus-start:state-at
#[compose]
fn state_at_scene() {
    ui! {
        Column (
            container: true,
            grow: 1.0,
            padding: Padding::all(18),
            justify: JustifyContent::SpaceBetween,
            text_color: ColorToken::OnSurface
        ) {
            Text (
                "Drag the bottom-right grip to cross 320 px",
                height: 28,
                font_size: @width {
                    if width < Fixed::from_int(320) { 13_u16 } else { 18_u16 }
                },
                text_color: ColorToken::Primary
            )
            View (
                id: "stage",
                width: Dimension::percent(100),
                height: 44,
                padding: Padding::all(12),
                bg_color: ColorToken::Secondary,
                border_radius: 8
            ) {
                Text ("#stage (100% width)", height: 20, font_size: 13, text_color: ColorToken::OnSecondary)
            }
            View (
                width: @id(stage).width { stage.width / 2 },
                height: 36,
                padding: Padding::all(8),
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 8
            ) {
                Text ("@id(stage).width / 2", height: 20, font_size: 12)
            }
        }
    };
}
//~focus-end:state-at

//~focus-start:porting-inspector
#[model]
pub struct SurfaceProbe {
    #[observe]
    pub rgb565: bool,
    #[observe]
    pub zone: u8,
    #[observe]
    pub rotary: i16,
}

impl Default for SurfaceProbe {
    fn default() -> Self {
        Self {
            rgb565: true,
            zone: 0,
            rotary: 0,
        }
    }
}

#[model]
impl SurfaceProbe {
    pub fn tap_touch(&mut self) {
        self.zone = (self.zone + 1) % 3;
    }

    pub fn turn_rotary(&mut self) {
        self.rotary = self.rotary.wrapping_add(1);
        self.zone = (self.zone + 1) % 3;
    }

    pub fn toggle_format(&mut self) {
        self.rgb565 = !self.rgb565;
    }

    #[observe]
    pub fn rect(&self) -> (u16, u16, u16, u16) {
        match self.zone {
            0 => (16, 14, 108, 34),
            1 => (88, 30, 124, 38),
            _ => (12, 10, 216, 58),
        }
    }

    #[observe]
    pub fn transfer_bytes(&self) -> u32 {
        let (_, _, w, h) = self.rect();
        let bpp = if self.rgb565 { 2_u32 } else { 4_u32 };
        u32::from(w) * u32::from(h) * bpp
    }
}

#[compose]
fn porting_inspector_scene(probe: model!(SurfaceProbe)) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(16),
            justify: JustifyContent::SpaceBetween,
            text_color: ColorToken::OnSurface
        ) {
            Row (height: 30, column_gap: 6, align: AlignItems::Center) {
                Button (
                    "Touch Tap",
                    grow: 1.0,
                    height: 28,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary,
                    text_color: ColorToken::OnPrimary
                ) on Tap { probe.tap_touch(); }
                Button (
                    "Rotary +1",
                    grow: 1.0,
                    height: 28,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::SurfaceVariant,
                    pressed_color: ColorToken::Outline,
                    text_color: ColorToken::OnSurface
                ) on Tap { probe.turn_rotary(); }
                Button (
                    text: ${ if probe.rgb565() { "RGB565 (2B)" } else { "RGBA8 (4B)" } },
                    grow: 1.0,
                    height: 28,
                    font_size: 12,
                    border_radius: 7,
                    normal_color: ColorToken::SurfaceVariant,
                    pressed_color: ColorToken::Outline,
                    text_color: ColorToken::OnSurface
                ) on Tap { probe.toggle_format(); }
            }
            View (
                height: 82,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 10,
                clip_children: true
            ) {
                View (
                    position: Position::Absolute,
                    left: ${ i32::from(probe.rect().0) },
                    top: ${ i32::from(probe.rect().1) },
                    width: ${ Dimension::px(i32::from(probe.rect().2)) },
                    height: ${ Dimension::px(i32::from(probe.rect().3)) },
                    padding: Padding::all(6),
                    bg_color: ColorToken::Secondary,
                    border_width: 2,
                    border_color: ColorToken::Primary,
                    border_radius: 6
                ) {
                    Text (
                        text: ${
                            let (x, y, w, h) = probe.rect();
                            format_args!("flush({x},{y} {w}x{h})")
                        },
                        capacity: 32,
                        height: 18,
                        font_size: 12,
                        text_color: ColorToken::OnSecondary
                    )
                }
            }
            Text (
                text: ${
                    let (x, y, w, h) = probe.rect();
                    format_args!(
                        "CASET {x}..{} · RASET {y}..{} · {} B · rotary #{}",
                        x + w - 1,
                        y + h - 1,
                        probe.transfer_bytes(),
                        probe.rotary()
                    )
                },
                capacity: 64,
                height: 18,
                font_size: 12,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
        }
    };
}
//~focus-end:porting-inspector
