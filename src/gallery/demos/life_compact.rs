use crate::gallery::demos::life::{self, LifeBoard};
use crate::prelude::*;
use crate::render::font::FontToken;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign, TextVerticalAlign, TextWrap};

pub const VIEWPORT: (u16, u16) = (128, 128);

fn label(align: TextAlign) -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::NoWrap,
        align,
        vertical_align: TextVerticalAlign::Center,
        max_lines: Some(1),
        ..ParagraphStyle::default()
    }
}

#[compose]
pub fn build_widgets(view_w: u16, view_h: u16) {
    let (cols, rows) = life::dims_from_px(i32::from(view_w), i32::from(view_h));
    let board: LifeBoard = life::seeded_board(cols, rows);

    ui! {
        View (
            id: "compact_life_shell",
            grow: 1.0,
            direction: @(id(compact_life_shell).width, id(compact_life_shell).height) {
                super::compact_layout::shell_direction(compact_life_shell.width, compact_life_shell.height)
            },
            padding: @(id(compact_life_shell).width, id(compact_life_shell).height) {
                super::compact_layout::padding(compact_life_shell.width, compact_life_shell.height, 3, 4)
            },
            row_gap: 4,
            column_gap: 4,
            bg_color: ColorToken::Surface
        ) {
            View (
                id: "compact_life_header",
                width: @(id(compact_life_shell).width, id(compact_life_shell).height) {
                    super::compact_layout::header_width(compact_life_shell.width, compact_life_shell.height, 36)
                },
                height: @(id(compact_life_shell).width, id(compact_life_shell).height) {
                    super::compact_layout::header_height(compact_life_shell.width, compact_life_shell.height)
                },
                direction: @(id(compact_life_shell).width, id(compact_life_shell).height) {
                    super::compact_layout::header_direction(compact_life_shell.width, compact_life_shell.height)
                },
                align: AlignItems::Center,
                justify: JustifyContent::Center,
                row_gap: 3,
                column_gap: 4
            ) {
                View (
                    id: "compact_life_marker",
                    width: @(id(compact_life_shell).width, id(compact_life_shell).height) {
                        super::compact_layout::marker_width(compact_life_shell.width, compact_life_shell.height)
                    },
                    height: @(id(compact_life_shell).width, id(compact_life_shell).height) {
                        super::compact_layout::marker_height(compact_life_shell.width, compact_life_shell.height)
                    },
                    bg_color: ColorToken::Success,
                    border_radius: 2
                )
                Text (
                    id: "compact_life_title",
                    "LIFE",
                    grow: 1.0,
                    width: @(id(compact_life_shell).width, id(compact_life_shell).height) {
                        super::compact_layout::title_width(compact_life_shell.width, compact_life_shell.height)
                    },
                    height: @(id(compact_life_shell).width, id(compact_life_shell).height) {
                        super::compact_layout::title_height(compact_life_shell.width, compact_life_shell.height)
                    },
                    font: FontToken::Default,
                    font_size: 6,
                    text_color: ColorToken::OnSurface,
                    paragraph: @(id(compact_life_shell).width, id(compact_life_shell).height) {
                        label(super::compact_layout::text_align(compact_life_shell.width, compact_life_shell.height))
                    }
                )
                Text (
                    id: "compact_life_mode",
                    "LOOP",
                    width: @(id(compact_life_shell).width, id(compact_life_shell).height) {
                        super::compact_layout::mode_width(compact_life_shell.width, compact_life_shell.height, 28)
                    },
                    height: @(id(compact_life_shell).width, id(compact_life_shell).height) {
                        super::compact_layout::mode_height(compact_life_shell.width, compact_life_shell.height)
                    },
                    font: FontToken::Default,
                    font_size: 6,
                    text_color: ColorToken::OnSurfaceVariant,
                    paragraph: @(id(compact_life_shell).width, id(compact_life_shell).height) {
                        label(super::compact_layout::mode_align(compact_life_shell.width, compact_life_shell.height))
                    }
                )
            }
            View (
                id: "compact_life_stage",
                grow: 1.0,
                min_width: 0,
                min_height: 0,
                width: @(id(compact_life_shell).width, id(compact_life_shell).height) {
                    super::compact_layout::content_width(compact_life_shell.width, compact_life_shell.height)
                },
                height: @(id(compact_life_shell).width, id(compact_life_shell).height) {
                    super::compact_layout::content_height(compact_life_shell.width, compact_life_shell.height)
                },
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
    app.compose(parent, |cx| build_widgets(cx, info.width, info.height));
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
    use crate::types::Viewport;
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

        crate::ui::render_system::update_layout(
            &mut app.world,
            root,
            &Viewport::new(160, 96, Fixed::ONE),
        );
        let shell = app.world.find_by_id("compact_life_shell").unwrap();
        assert_eq!(
            app.world.get::<Style>(shell).unwrap().layout.direction,
            FlexDirection::Row,
        );

        crate::ui::render_system::update_layout(
            &mut app.world,
            root,
            &Viewport::new(96, 160, Fixed::ONE),
        );
        assert_eq!(
            app.world.get::<Style>(shell).unwrap().layout.direction,
            FlexDirection::Column,
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
