use crate::prelude::*;
use crate::ui::widgets::{Image, ParagraphStyle, Text, TextOverflow, TextWrap};

pub const VIEWPORT: (u16, u16) = (1024, 720);

const BACKGROUND: ColorToken = ColorToken::Surface;
const PANEL: ColorToken = ColorToken::SurfaceVariant;
const PANEL_ALT: ColorToken = ColorToken::Surface;
const BORDER: ColorToken = ColorToken::Outline;
const TEXT: ColorToken = ColorToken::OnSurface;
const MUTED: ColorToken = ColorToken::OnSurfaceVariant;
const CYAN: ColorToken = ColorToken::Primary;
const BLUE: ColorToken = ColorToken::Secondary;
const VIOLET: ColorToken = ColorToken::Tertiary;
const GOLD: ColorToken = ColorToken::Success;

fn bounded_body(lines: u16) -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::Word,
        overflow: TextOverflow::Ellipsis,
        max_lines: Some(lines),
        ..ParagraphStyle::default()
    }
}

#[derive(Clone, Copy)]
struct LayoutChip {
    label: &'static str,
    color: ColorToken,
}

const CHIPS: [LayoutChip; 5] = [
    LayoutChip {
        label: "CONTENT",
        color: CYAN,
    },
    LayoutChip {
        label: "GROW",
        color: BLUE,
    },
    LayoutChip {
        label: "SHRINK",
        color: VIOLET,
    },
    LayoutChip {
        label: "MIN / MAX",
        color: GOLD,
    },
    LayoutChip {
        label: "WRAP",
        color: CYAN,
    },
];

#[compose]
fn compose_header() -> Entity {
    ui! {
        Row (
            id: "layout_lab_header",
            min_height: @id(layout_lab_document).width {
                if layout_lab_document.width < Fixed::from_int(640) { 108 } else { 70 }
            },
            wrap: FlexWrap::Wrap,
            align: AlignItems::Center,
            row_gap: 4,
            column_gap: 14
        ) {
            View (width: 8, height: 42, bg_color: CYAN, border_radius: 4)
            Column (grow: 1.0, min_width: 150, row_gap: 3) {
                Text (
                    "LAYOUT LAB",
                    width: Dimension::percent(100),
                    font_size: 24,
                    text_color: TEXT,
                    paragraph: bounded_body(1)
                )
                Text (
                    "responsive geometry / one widget tree",
                    width: Dimension::percent(100),
                    min_height: 18,
                    font_size: 13,
                    text_color: MUTED,
                    paragraph: bounded_body(2)
                )
            }
            Text (
                "8 CAPABILITIES",
                width: @id(layout_lab_document).width {
                    if layout_lab_document.width < Fixed::from_int(640) {
                        Dimension::percent(100)
                    } else {
                        Dimension::percent(32)
                    }
                },
                min_width: 96,
                max_width: @id(layout_lab_document).width {
                    if layout_lab_document.width < Fixed::from_int(640) {
                        Dimension::percent(100)
                    } else {
                        Dimension::px(154)
                    }
                },
                height: 30,
                bg_color: PANEL_ALT,
                border_color: BORDER,
                border_width: 1,
                border_radius: 15,
                font_size: 12,
                text_color: CYAN,
                paragraph: ParagraphStyle::label()
            )
        }
    }
}

#[compose]
fn compose_flex_card() -> Entity {
    ui! {
        Column (
            id: "layout_lab_flex",
            grow: 1.0,
            width: @id(layout_lab_grid).width {
                if layout_lab_grid.width < Fixed::from_int(560) {
                    Dimension::percent(100)
                } else {
                    Dimension::percent(48)
                }
            },
            min_width: 250,
            min_height: 300,
            padding: Padding::all(14),
            row_gap: 10,
            clip_children: true,
            bg_color: PANEL,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text (
                "FLEX / CONTENT > GROW > LIMITS",
                width: Dimension::percent(100),
                min_height: 28,
                font_size: 12,
                text_color: BLUE,
                paragraph: bounded_body(2)
            )
            Row (
                min_height: 70,
                wrap: FlexWrap::Wrap,
                align: AlignItems::Center,
                row_gap: 8,
                column_gap: 8
            ) {
                Text (
                    id: "layout_lab_content",
                    "CONTENT",
                    width: Dimension::Content,
                    height: 38,
                    padding: Padding::all(8),
                    bg_color: CYAN,
                    border_radius: 9,
                    font_size: 11,
                    text_color: ColorToken::OnPrimary,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    id: "layout_lab_grow",
                    "GROW 1",
                    grow: 1.0,
                    min_width: 88,
                    max_width: 220,
                    height: 48,
                    bg_color: BLUE,
                    border_radius: 11,
                    font_size: 12,
                    text_color: ColorToken::OnSecondary,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    id: "layout_lab_fixed",
                    "96 PX",
                    width: 96,
                    height: 32,
                    bg_color: VIOLET,
                    border_radius: 8,
                    font_size: 11,
                    text_color: ColorToken::OnTertiary,
                    paragraph: ParagraphStyle::label()
                )
            }
            Row (
                id: "layout_lab_wrap",
                wrap: FlexWrap::Wrap,
                height: 100,
                align: AlignItems::Center,
                row_gap: 7,
                column_gap: 7
            ) {
                walk CHIPS.iter() with chip {
                    Text (
                        chip.label,
                        width: 76,
                        height: 28,
                        bg_color: PANEL_ALT,
                        border_color: chip.color,
                        border_width: 1,
                        border_radius: 14,
                        font_size: 9,
                        text_color: chip.color,
                        paragraph: ParagraphStyle::label()
                    )
                }
            }
            Text (
                "The same row wraps when its minimum widths no longer fit.",
                width: Dimension::percent(100),
                min_height: 32,
                font_size: 11,
                text_color: MUTED,
                paragraph: bounded_body(2)
            )
        }
    }
}

#[compose]
fn compose_surface_card() -> Entity {
    ui! {
        Column (
            id: "layout_lab_surfaces",
            grow: 1.0,
            width: @id(layout_lab_grid).width {
                if layout_lab_grid.width < Fixed::from_int(560) {
                    Dimension::percent(100)
                } else {
                    Dimension::percent(48)
                }
            },
            min_width: 250,
            min_height: 278,
            padding: Padding::all(14),
            row_gap: 12,
            clip_children: true,
            bg_color: PANEL_ALT,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text (
                "SURFACES / BORDER / RADIUS",
                width: Dimension::percent(100),
                min_height: 28,
                font_size: 12,
                text_color: GOLD,
                paragraph: bounded_body(2)
            )
            Row (grow: 1.0, align: AlignItems::Center, justify: JustifyContent::SpaceEvenly) {
                View (
                    id: "layout_lab_surface_round",
                    width: 72,
                    height: 72,
                    bg_color: BLUE,
                    border_color: ColorToken::OnSecondary,
                    border_width: 2,
                    border_radius: 18
                )
                View (
                    width: 72,
                    height: 72,
                    bg_color: CYAN,
                    border_color: ColorToken::OnPrimary,
                    border_width: 3,
                    border_radius: 36
                )
                View (
                    width: 72,
                    height: 72,
                    border_color: VIOLET,
                    border_width: 3,
                    border_radius: 9
                )
            }
            Text (
                id: "layout_lab_surface_note",
                "Fill, stroke and radius stay independent.",
                width: Dimension::percent(100),
                height: 28,
                font_size: 11,
                text_color: MUTED,
                paragraph: bounded_body(2)
            )
        }
    }
}

#[compose]
fn compose_overlay_card() -> Entity {
    ui! {
        Column (
            id: "layout_lab_overlay_card",
            grow: 1.0,
            width: @id(layout_lab_grid).width {
                if layout_lab_grid.width < Fixed::from_int(560) {
                    Dimension::percent(100)
                } else {
                    Dimension::percent(48)
                }
            },
            min_width: 250,
            min_height: 278,
            padding: Padding::all(14),
            row_gap: 8,
            clip_children: true,
            bg_color: PANEL_ALT,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text (
                "OVERLAY / ABSOLUTE IN A FLEX CARD",
                width: Dimension::percent(100),
                min_height: 28,
                font_size: 12,
                text_color: VIOLET,
                paragraph: bounded_body(2)
            )
            View (
                id: "layout_lab_overlay_stage",
                grow: 1.0,
                min_height: 162,
                bg_color: BACKGROUND,
                border_radius: 11,
                clip_children: true
            ) {
                View (
                    width: Dimension::percent(72),
                    height: Dimension::percent(62),
                    bg_color: ColorToken::Primary,
                    border_radius: 12
                )
                View (
                    id: "layout_lab_absolute",
                    position: Position::Absolute,
                    left: 24,
                    top: 46,
                    width: 126,
                    height: 58,
                    bg_color: BLUE,
                    border_radius: 13
                )
                Image (
                    id: "layout_lab_image_overlay",
                    position: Position::Absolute,
                    left: 166,
                    top: 28,
                    width: 56,
                    height: 56,
                    src: "thumbs_up"
                )
                Text (
                    "PINNED",
                    position: Position::Absolute,
                    right: 10,
                    top: 104,
                    width: 88,
                    height: 30,
                    bg_color: VIOLET,
                    border_radius: 15,
                    font_size: 10,
                    text_color: ColorToken::OnTertiary,
                    paragraph: ParagraphStyle::label()
                )
            }
        }
    }
}

#[compose]
fn compose_collection_card() -> Entity {
    let show_note = true;

    ui! {
        Column (
            id: "layout_lab_collection",
            grow: 1.0,
            width: @id(layout_lab_grid).width {
                if layout_lab_grid.width < Fixed::from_int(560) {
                    Dimension::percent(100)
                } else {
                    Dimension::percent(48)
                }
            },
            min_width: 250,
            min_height: 278,
            padding: Padding::all(14),
            row_gap: 10,
            clip_children: true,
            bg_color: PANEL,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text (
                "COMPOSITION / WALK / IF / IMAGE",
                width: Dimension::percent(100),
                min_height: 28,
                font_size: 12,
                text_color: CYAN,
                paragraph: bounded_body(2)
            )
            Row (height: 82, align: AlignItems::Center, column_gap: 10) {
                Image (
                    id: "layout_lab_image_flow",
                    width: 58,
                    height: 58,
                    src: "thumbs_up"
                )
                Column (grow: 1.0, row_gap: 6) {
                    Text (
                        "Typed image stays in flex flow.",
                        width: Dimension::percent(100),
                        font_size: 12,
                        text_color: TEXT,
                        paragraph: bounded_body(2)
                    )
                    Text (
                        "Overlay reuses its resource.",
                        width: Dimension::percent(100),
                        font_size: 11,
                        text_color: MUTED,
                        paragraph: bounded_body(2)
                    )
                }
            }
            Row (id: "layout_lab_walk", height: 42, column_gap: 7) {
                walk CHIPS.iter() with chip {
                    View (grow: 1.0, height: 20, bg_color: chip.color, border_radius: 6)
                }
            }
            if show_note {
                Text (
                    id: "layout_lab_conditional",
                    "Conditional branch retained",
                    height: 30,
                    bg_color: ColorToken::Primary,
                    border_radius: 8,
                    font_size: 11,
                    text_color: ColorToken::OnPrimary,
                    paragraph: ParagraphStyle::label()
                )
            }
        }
    }
}

#[compose]
pub fn build_widgets() {
    //~focus-start
    ui! {
        Scroll (
            id: "layout_lab_shell",
            grow: 1.0,
            clip_children: true,
            bg_color: BACKGROUND
        ) {
            Column (
                id: "layout_lab_document",
                width: Dimension::percent(100),
                height: Dimension::Content,
                min_height: Dimension::percent(100),
                padding: Padding::all(18),
                row_gap: 12
            ) {
                compose_header ()
                Row (
                    id: "layout_lab_grid",
                    width: Dimension::percent(100),
                    height: Dimension::Content,
                    wrap: FlexWrap::Wrap,
                    align: AlignItems::FlexStart,
                    row_gap: 12,
                    column_gap: 12
                ) {
                    compose_flex_card ()
                    compose_surface_card ()
                    compose_overlay_card ()
                    compose_collection_card ()
                }
            }
        }
    };
    //~focus-end
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    crate::gallery::showcase_theme::install(&mut app.world);
    app.add_plugin(crate::app::plugins::ImageResourcesPlugin::default());
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{Children, IdMap, Style, UiScope};

    fn fixture() -> (World, Entity) {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);
        (world, parent)
    }

    #[test]
    fn preserves_every_migrated_layout_capability() {
        let (world, _) = fixture();
        for id in [
            "layout_lab_shell",
            "layout_lab_header",
            "layout_lab_flex",
            "layout_lab_content",
            "layout_lab_grow",
            "layout_lab_surface_round",
            "layout_lab_overlay_stage",
            "layout_lab_absolute",
            "layout_lab_image_flow",
            "layout_lab_image_overlay",
            "layout_lab_walk",
            "layout_lab_conditional",
        ] {
            assert!(world.find_by_id(id).is_some(), "missing {id}");
        }

        let grow = world
            .get::<Style>(world.find_by_id("layout_lab_grow").unwrap())
            .unwrap();
        assert_eq!(grow.layout.grow, Fixed::ONE);
        assert_eq!(grow.layout.min_width, Dimension::px(88));
        assert_eq!(grow.layout.max_width, Dimension::px(220));

        let absolute = world
            .get::<Style>(world.find_by_id("layout_lab_absolute").unwrap())
            .unwrap();
        assert_eq!(absolute.layout.position, Position::Absolute);

        let walk = world
            .get::<Children>(world.find_by_id("layout_lab_walk").unwrap())
            .unwrap();
        assert_eq!(walk.0.len(), CHIPS.len());
        for id in ["layout_lab_image_flow", "layout_lab_image_overlay"] {
            assert!(world.get::<Image>(world.find_by_id(id).unwrap()).is_some());
        }
    }

    #[test]
    fn default_viewport_keeps_the_two_by_two_grid_inside_the_shell() {
        use crate::types::Viewport;
        use crate::ui::ComputedRect;
        use crate::ui::render_system::update_layout;

        let (mut world, parent) = fixture();
        update_layout(
            &mut world,
            parent,
            &Viewport::new(VIEWPORT.0, VIEWPORT.1, Fixed::ONE),
        );
        let rect = |world: &World, id| {
            world
                .get::<ComputedRect>(world.find_by_id(id).unwrap())
                .unwrap()
                .0
        };
        let shell = rect(&world, "layout_lab_shell");
        let flex = rect(&world, "layout_lab_flex");
        let surfaces = rect(&world, "layout_lab_surfaces");
        let overlay = rect(&world, "layout_lab_overlay_card");
        let collection = rect(&world, "layout_lab_collection");

        assert_eq!(flex.y, surfaces.y);
        assert_eq!(overlay.y, collection.y);
        assert!(overlay.y > flex.y);
        for card in [flex, surfaces, overlay, collection] {
            assert!(card.x >= shell.x && card.x + card.w <= shell.x + shell.w);
            assert!(card.y >= shell.y && card.y + card.h <= shell.y + shell.h);
        }

        let note = rect(&world, "layout_lab_surface_note");
        assert!(note.x + note.w <= surfaces.x + surfaces.w);
        assert!(note.y + note.h <= surfaces.y + surfaces.h);
    }

    #[test]
    fn phone_viewport_stacks_cards_without_horizontal_overflow() {
        use crate::types::Viewport;
        use crate::ui::ComputedRect;
        use crate::ui::render_system::update_layout;

        let (mut world, parent) = fixture();
        update_layout(&mut world, parent, &Viewport::new(320, 568, Fixed::ONE));
        let rect = |world: &World, id| {
            world
                .get::<ComputedRect>(world.find_by_id(id).unwrap())
                .unwrap()
                .0
        };
        let shell = rect(&world, "layout_lab_shell");
        let cards = [
            rect(&world, "layout_lab_flex"),
            rect(&world, "layout_lab_surfaces"),
            rect(&world, "layout_lab_overlay_card"),
            rect(&world, "layout_lab_collection"),
        ];
        for card in cards {
            assert!(card.x >= shell.x);
            assert!(card.x + card.w <= shell.x + shell.w);
        }

        let overlay = rect(&world, "layout_lab_overlay_stage");
        let absolute = rect(&world, "layout_lab_absolute");
        assert!(absolute.x + absolute.w <= overlay.x + overlay.w);
    }

    #[test]
    fn medium_portrait_cards_fill_the_single_column() {
        use crate::types::Viewport;
        use crate::ui::ComputedRect;
        use crate::ui::render_system::update_layout;

        for (width, height) in [(539, 734), (320, 568)] {
            let mut app = App::headless(width, height);
            app.with_default_widgets().with_default_systems();
            let parent = app.spawn_root().id();
            setup_app(&mut app, parent);
            app.set_root(parent);
            update_layout(
                &mut app.world,
                parent,
                &Viewport::new(width, height, Fixed::ONE),
            );
            let rect = |world: &World, id| {
                world
                    .get::<ComputedRect>(world.find_by_id(id).unwrap())
                    .unwrap()
                    .0
            };
            let grid = rect(&app.world, "layout_lab_grid");
            for id in [
                "layout_lab_flex",
                "layout_lab_surfaces",
                "layout_lab_overlay_card",
                "layout_lab_collection",
            ] {
                let card = rect(&app.world, id);
                assert_eq!(card.x, grid.x, "{width}x{height}: {id}");
                assert_eq!(card.w, grid.w, "{width}x{height}: {id}");
            }
            super::super::assert_text_layouts_fit(&app.world);
        }
    }

    #[test]
    fn phone_scroll_extent_ends_at_the_last_visible_descendant() {
        use crate::input::event::scroll::scroll_bounds;
        use crate::types::Viewport;
        use crate::ui::ComputedRect;
        use crate::ui::render_system::update_layout;

        for (width, height) in [(320, 568), (422, 600), (480, 320)] {
            let (mut world, parent) = fixture();
            update_layout(
                &mut world,
                parent,
                &Viewport::new(width, height, Fixed::ONE),
            );
            let shell = world.find_by_id("layout_lab_shell").unwrap();
            let content = world.find_by_id("layout_lab_document").unwrap();
            let last = world.find_by_id("layout_lab_collection").unwrap();
            let shell_rect = world.get::<ComputedRect>(shell).unwrap().0;
            let content_rect = world.get::<ComputedRect>(content).unwrap().0;
            let last_rect = world.get::<ComputedRect>(last).unwrap().0;
            let bounds = scroll_bounds(&world, shell).unwrap();
            let extent = bounds.content_height;
            let max_offset = bounds.max_y;

            assert!(max_offset > Fixed::ZERO, "{width}x{height}");
            assert!(
                last_rect.y + last_rect.h - max_offset <= shell_rect.y + shell_rect.h,
                "{width}x{height}: {last_rect:?} {shell_rect:?} {extent:?}",
            );
            assert!(
                last_rect.y + last_rect.h - max_offset > shell_rect.y,
                "{width}x{height}: {last_rect:?} {shell_rect:?} {extent:?}",
            );
            assert!(extent >= last_rect.y + last_rect.h - content_rect.y);
        }
    }
}
