use crate::prelude::*;
use crate::ui::widgets::{Image, ParagraphStyle, Text, TextAlign, TextVerticalAlign, TextWrap};

pub const VIEWPORT: (u16, u16) = (1024, 720);

const BACKGROUND: Color = Color::rgb(9, 16, 28);
const PANEL: Color = Color::rgb(17, 30, 49);
const PANEL_ALT: Color = Color::rgb(21, 38, 60);
const BORDER: Color = Color::rgb(47, 73, 101);
const TEXT: Color = Color::rgb(232, 240, 248);
const MUTED: Color = Color::rgb(139, 163, 188);
const CYAN: Color = Color::rgb(86, 226, 205);
const BLUE: Color = Color::rgb(102, 161, 255);
const VIOLET: Color = Color::rgb(179, 132, 255);
const GOLD: Color = Color::rgb(255, 198, 92);

#[derive(Clone, Copy)]
struct LayoutChip {
    label: &'static str,
    color: Color,
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

fn centered_label() -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::NoWrap,
        align: TextAlign::Center,
        vertical_align: TextVerticalAlign::Center,
        max_lines: Some(1),
        ..ParagraphStyle::default()
    }
}

#[compose]
fn compose_header() -> Entity {
    ui! {
        Row (
            id: "layout_lab_header",
            height: 62,
            align: AlignItems::Center,
            column_gap: 14
        ) {
            View (width: 8, height: 42, bg_color: CYAN, border_radius: 4)
            Column (grow: 1.0, row_gap: 3) {
                Text ("LAYOUT LAB", font_size: 24, text_color: TEXT)
                Text (
                    "responsive composition · explicit geometry · one widget tree",
                    font_size: 13,
                    text_color: MUTED
                )
            }
            Text (
                "8 CAPABILITIES",
                width: 154,
                height: 30,
                bg_color: PANEL_ALT,
                border_color: BORDER,
                border_width: 1,
                border_radius: 15,
                font_size: 12,
                text_color: CYAN,
                paragraph: centered_label()
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
            min_width: 430,
            min_height: 278,
            padding: Padding::all(14),
            row_gap: 10,
            bg_color: PANEL,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text ("FLEX · CONTENT → GROW → LIMITS", font_size: 12, text_color: BLUE)
            Row (height: 70, align: AlignItems::Center, column_gap: 8) {
                Text (
                    id: "layout_lab_content",
                    "CONTENT",
                    width: Dimension::Content,
                    height: 38,
                    padding: Padding::all(8),
                    bg_color: CYAN,
                    border_radius: 9,
                    font_size: 11,
                    text_color: BACKGROUND,
                    paragraph: centered_label()
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
                    text_color: BACKGROUND,
                    paragraph: centered_label()
                )
                Text (
                    id: "layout_lab_fixed",
                    "96 PX",
                    width: 96,
                    height: 32,
                    bg_color: VIOLET,
                    border_radius: 8,
                    font_size: 11,
                    text_color: TEXT,
                    paragraph: centered_label()
                )
            }
            Row (
                id: "layout_lab_wrap",
                wrap: FlexWrap::Wrap,
                min_height: 86,
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
                        paragraph: centered_label()
                    )
                }
            }
            Text (
                "The same row wraps when its minimum widths no longer fit.",
                font_size: 11,
                text_color: MUTED
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
            min_width: 300,
            min_height: 278,
            padding: Padding::all(14),
            row_gap: 12,
            bg_color: PANEL_ALT,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text ("SURFACES · BORDER / RADIUS", font_size: 12, text_color: GOLD)
            Row (grow: 1.0, align: AlignItems::Center, justify: JustifyContent::SpaceEvenly) {
                View (
                    id: "layout_lab_surface_round",
                    width: 72,
                    height: 72,
                    bg_color: BLUE,
                    border_color: TEXT,
                    border_width: 2,
                    border_radius: 18
                )
                View (
                    width: 72,
                    height: 72,
                    bg_color: CYAN,
                    border_color: BACKGROUND,
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
                text_color: MUTED
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
            min_width: 360,
            min_height: 278,
            padding: Padding::all(14),
            row_gap: 8,
            bg_color: PANEL_ALT,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text ("OVERLAY · ABSOLUTE IN A FLEX CARD", font_size: 12, text_color: VIOLET)
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
                    bg_color: Color::rgb(20, 61, 78),
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
                    left: 254,
                    top: 104,
                    width: 88,
                    height: 30,
                    bg_color: VIOLET,
                    border_radius: 15,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: centered_label()
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
            min_width: 370,
            min_height: 278,
            padding: Padding::all(14),
            row_gap: 10,
            bg_color: PANEL,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text ("COMPOSITION · WALK / IF / IMAGE", font_size: 12, text_color: CYAN)
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
                        font_size: 12,
                        text_color: TEXT
                    )
                    Text (
                        "Overlay reuses its resource.",
                        font_size: 11,
                        text_color: MUTED
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
                    bg_color: Color::rgb(21, 67, 68),
                    border_radius: 8,
                    font_size: 11,
                    text_color: CYAN,
                    paragraph: centered_label()
                )
            }
        }
    }
}

#[compose]
pub fn build_widgets() {
    //~focus-start
    ui! {
        Column (
            id: "layout_lab_shell",
            grow: 1.0,
            padding: Padding::all(18),
            row_gap: 12,
            bg_color: BACKGROUND
        ) {
            compose_header ()
            Row (
                id: "layout_lab_grid",
                grow: 1.0,
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
    };
    //~focus-end
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
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
}
