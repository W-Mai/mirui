use crate::gallery::demos::life::{self, LifeBoard};
use crate::prelude::*;
use crate::render::font::FontToken;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign, TextVerticalAlign, TextWrap};

pub const VIEWPORT: (u16, u16) = (128, 128);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompactLayout {
    Portrait,
    Square,
    Landscape,
}

#[derive(Clone, Copy)]
struct CompactMetrics {
    layout: CompactLayout,
    narrow: bool,
    shell_direction: FlexDirection,
    header_direction: FlexDirection,
    header_width: Dimension,
    header_height: Dimension,
    stage_width: Dimension,
    stage_height: Dimension,
    marker_width: Dimension,
    marker_height: Dimension,
    title_width: Dimension,
    title_height: Dimension,
    mode_width: Dimension,
    mode_height: Dimension,
    mode: &'static str,
}

impl CompactMetrics {
    fn for_size(width: u16, height: u16) -> Self {
        let layout = if width > height.saturating_add(16) {
            CompactLayout::Landscape
        } else if height > width.saturating_add(16) {
            CompactLayout::Portrait
        } else {
            CompactLayout::Square
        };
        let landscape = layout == CompactLayout::Landscape;
        Self {
            layout,
            narrow: width.min(height) < 112,
            shell_direction: if landscape {
                FlexDirection::Row
            } else {
                FlexDirection::Column
            },
            header_direction: if landscape {
                FlexDirection::Column
            } else {
                FlexDirection::Row
            },
            header_width: if landscape {
                Dimension::px(36)
            } else {
                Dimension::percent(100)
            },
            header_height: if landscape {
                Dimension::percent(100)
            } else {
                Dimension::px(14)
            },
            stage_width: if landscape {
                Dimension::Auto
            } else {
                Dimension::percent(100)
            },
            stage_height: if landscape {
                Dimension::percent(100)
            } else {
                Dimension::Auto
            },
            marker_width: Dimension::px(if landscape { 10 } else { 4 }),
            marker_height: Dimension::px(if landscape { 4 } else { 10 }),
            title_width: if landscape {
                Dimension::percent(100)
            } else {
                Dimension::Auto
            },
            title_height: if landscape {
                Dimension::Auto
            } else {
                Dimension::px(14)
            },
            mode_width: if landscape {
                Dimension::percent(100)
            } else {
                Dimension::px(match layout {
                    CompactLayout::Portrait => 28,
                    CompactLayout::Square => 28,
                    CompactLayout::Landscape => 28,
                })
            },
            mode_height: if landscape {
                Dimension::px(12)
            } else {
                Dimension::px(14)
            },
            mode: match layout {
                CompactLayout::Portrait => "TALL",
                CompactLayout::Square => "LIVE",
                CompactLayout::Landscape => "WIDE",
            },
        }
    }

    const fn signature(self) -> (CompactLayout, bool) {
        (self.layout, self.narrow)
    }

    fn text_align(self) -> TextAlign {
        if self.layout == CompactLayout::Landscape {
            TextAlign::Center
        } else {
            TextAlign::Start
        }
    }

    fn mode_align(self) -> TextAlign {
        if self.layout == CompactLayout::Landscape {
            TextAlign::Center
        } else {
            TextAlign::End
        }
    }
}

#[derive(Clone, Copy)]
struct CompactLayoutState {
    signature: (CompactLayout, bool),
    shell: Entity,
    header: Entity,
    stage: Entity,
    marker: Entity,
    title: Entity,
    mode: Entity,
}

fn label(align: TextAlign) -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::NoWrap,
        align,
        vertical_align: TextVerticalAlign::Center,
        max_lines: Some(1),
        ..ParagraphStyle::default()
    }
}

fn apply_metrics(world: &mut World, state: CompactLayoutState, metrics: CompactMetrics) {
    if let Some(style) = world.get_mut::<Style>(state.shell) {
        style.layout.direction = metrics.shell_direction;
        style.layout.padding = Padding::all(if metrics.narrow { 3 } else { 4 });
    }
    if let Some(style) = world.get_mut::<Style>(state.header) {
        style.layout.direction = metrics.header_direction;
        style.layout.width = metrics.header_width;
        style.layout.height = metrics.header_height;
    }
    if let Some(style) = world.get_mut::<Style>(state.stage) {
        style.layout.width = metrics.stage_width;
        style.layout.height = metrics.stage_height;
    }
    if let Some(style) = world.get_mut::<Style>(state.marker) {
        style.layout.width = metrics.marker_width;
        style.layout.height = metrics.marker_height;
    }
    if let Some(style) = world.get_mut::<Style>(state.title) {
        style.layout.width = metrics.title_width;
        style.layout.height = metrics.title_height;
    }
    if let Some(text) = world.get_mut::<Text>(state.title) {
        text.set_paragraph(label(metrics.text_align()));
    }
    if let Some(style) = world.get_mut::<Style>(state.mode) {
        style.layout.width = metrics.mode_width;
        style.layout.height = metrics.mode_height;
    }
    if let Some(text) = world.get_mut::<Text>(state.mode) {
        text.set_content(metrics.mode);
        text.set_paragraph(label(metrics.mode_align()));
    }
    world.invalidate(state.shell);
}

#[mirui_macros::system]
fn sync_compact_layout(world: &mut World) {
    let Some(viewport) = crate::ui::root_viewport(world) else {
        return;
    };
    let Some(state) = world.resource::<CompactLayoutState>().copied() else {
        return;
    };
    let metrics = CompactMetrics::for_size(
        viewport.w.to_int().clamp(0, i32::from(u16::MAX)) as u16,
        viewport.h.to_int().clamp(0, i32::from(u16::MAX)) as u16,
    );
    if metrics.signature() == state.signature {
        return;
    }
    apply_metrics(world, state, metrics);
    if let Some(state) = world.resource_mut::<CompactLayoutState>() {
        state.signature = metrics.signature();
    }
}

#[compose]
pub fn build_widgets(view_w: u16, view_h: u16) {
    let metrics = CompactMetrics::for_size(view_w, view_h);
    let (cols, rows) = life::dims_from_px(i32::from(view_w), i32::from(view_h));
    let board: LifeBoard = life::seeded_board(cols, rows);

    ui! {
        View (
            id: "compact_life_shell",
            grow: 1.0,
            direction: metrics.shell_direction,
            padding: Padding::all(if metrics.narrow { 3 } else { 4 }),
            row_gap: 4,
            column_gap: 4,
            bg_color: ColorToken::Surface
        ) {
            View (
                id: "compact_life_header",
                width: metrics.header_width,
                height: metrics.header_height,
                direction: metrics.header_direction,
                align: AlignItems::Center,
                justify: JustifyContent::Center,
                row_gap: 3,
                column_gap: 4
            ) {
                View (
                    id: "compact_life_marker",
                    width: metrics.marker_width,
                    height: metrics.marker_height,
                    bg_color: ColorToken::Success,
                    border_radius: 2
                )
                Text (
                    id: "compact_life_title",
                    "LIFE",
                    grow: 1.0,
                    width: metrics.title_width,
                    height: metrics.title_height,
                    font: FontToken::Default,
                    font_size: 6,
                    text_color: ColorToken::OnSurface,
                    paragraph: label(metrics.text_align())
                )
                Text (
                    id: "compact_life_mode",
                    metrics.mode,
                    width: metrics.mode_width,
                    height: metrics.mode_height,
                    font: FontToken::Default,
                    font_size: 6,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: label(metrics.mode_align())
                )
            }
            View (
                id: "compact_life_stage",
                grow: 1.0,
                min_width: 0,
                min_height: 0,
                width: metrics.stage_width,
                height: metrics.stage_height,
                padding: Padding::all(2),
                bg_color: ColorToken::SurfaceVariant,
                border_color: ColorToken::Outline,
                border_width: 1,
                border_radius: 8
            ) {
                View (
                    id: "compact_life_board",
                    grow: 1.0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    bg_color: ColorToken::Surface,
                    border_radius: 5,
                    clip_children: true
                ) [
                    board,
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
    let info = app.backend.display_info();
    app.add_system(sync_compact_layout::system());
    app.compose(parent, |cx| build_widgets(cx, info.width, info.height));

    let metrics = CompactMetrics::for_size(info.width, info.height);
    let state = CompactLayoutState {
        signature: metrics.signature(),
        shell: app.world.find_by_id("compact_life_shell").unwrap(),
        header: app.world.find_by_id("compact_life_header").unwrap(),
        stage: app.world.find_by_id("compact_life_stage").unwrap(),
        marker: app.world.find_by_id("compact_life_marker").unwrap(),
        title: app.world.find_by_id("compact_life_title").unwrap(),
        mode: app.world.find_by_id("compact_life_mode").unwrap(),
    };
    apply_metrics(&mut app.world, state, metrics);
    app.world.insert_resource(state);
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    life::install_runtime(app);
    install(app, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::FramebufferAccess;
    use crate::ui::ComputedRect;

    fn compact_layout(width: u16, height: u16) -> (Rect, Rect) {
        let mut app = App::headless(width, height);
        app.with_default_widgets()
            .with_default_systems()
            .with_widget(life::life_view());
        let root = app.spawn_root().id();
        install(&mut app, root);
        app.set_root(root);
        app.render().unwrap();

        let header = app.world.find_by_id("compact_life_header").unwrap();
        let stage = app.world.find_by_id("compact_life_stage").unwrap();
        (
            app.world.get::<ComputedRect>(header).unwrap().0,
            app.world.get::<ComputedRect>(stage).unwrap().0,
        )
    }

    #[test]
    fn compact_life_reflows_between_portrait_square_and_landscape() {
        let (portrait_header, portrait_stage) = compact_layout(96, 160);
        let (square_header, square_stage) = compact_layout(128, 128);
        let (landscape_header, landscape_stage) = compact_layout(160, 96);

        assert!(portrait_stage.y >= portrait_header.y + portrait_header.h);
        assert!(square_stage.y >= square_header.y + square_header.h);
        assert!(landscape_stage.x >= landscape_header.x + landscape_header.w);
        assert!(landscape_stage.y <= landscape_header.y + Fixed::from_int(1));
        assert!(portrait_stage.h > landscape_stage.h);
        assert!(landscape_stage.w > portrait_stage.w);
    }

    #[test]
    fn compact_life_reflows_after_live_viewport_changes() {
        let mut app = App::headless(128, 128);
        app.with_default_widgets()
            .with_default_systems()
            .with_widget(life::life_view());
        let root = app.spawn_root().id();
        install(&mut app, root);
        app.set_root(root);
        app.render().unwrap();

        app.world
            .insert(root, ComputedRect(Rect::new(0, 0, 160, 96)));
        sync_compact_layout(&mut app.world);
        let state = app.world.resource::<CompactLayoutState>().unwrap();
        assert_eq!(state.signature.0, CompactLayout::Landscape);
        assert_eq!(
            app.world
                .get::<Style>(state.shell)
                .unwrap()
                .layout
                .direction,
            FlexDirection::Row,
        );
        assert_eq!(
            app.world
                .get::<Text>(state.mode)
                .unwrap()
                .resolve(&app.world),
            "WIDE",
        );

        app.world
            .insert(root, ComputedRect(Rect::new(0, 0, 96, 160)));
        sync_compact_layout(&mut app.world);
        let state = app.world.resource::<CompactLayoutState>().unwrap();
        assert_eq!(state.signature.0, CompactLayout::Portrait);
        assert_eq!(
            app.world
                .get::<Style>(state.shell)
                .unwrap()
                .layout
                .direction,
            FlexDirection::Column,
        );
        assert_eq!(
            app.world
                .get::<Text>(state.mode)
                .unwrap()
                .resolve(&app.world),
            "TALL",
        );
    }

    #[test]
    fn compact_life_renders_at_native_size() {
        let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
        app.with_default_widgets()
            .with_default_systems()
            .with_widget(life::life_view());
        let root = app.spawn_root().id();
        install(&mut app, root);
        app.set_root(root);
        app.render().unwrap();

        let board = app.world.find_by_id("compact_life_board").unwrap();
        let rect = app.world.get::<ComputedRect>(board).unwrap().0;
        assert!(rect.w > Fixed::from_int(80));
        assert!(rect.h > Fixed::from_int(80));
        assert!(
            app.backend
                .framebuffer()
                .buf
                .as_slice()
                .iter()
                .any(|byte| *byte != 0),
        );
    }
}
