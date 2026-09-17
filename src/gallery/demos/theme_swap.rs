use crate::prelude::*;
use crate::ui::widgets::{
    Button, Checkbox, ParagraphStyle, Placeholder, ProgressBar, Slider, Switch, TabBar, Text,
    TextAlign, TextInput,
};
use crate::ui::{Theme, theme};

pub struct ThemeChoice(pub Theme);

pub const ACCENT: ColorToken = ColorToken::Tertiary;

pub fn dark_with_accent() -> Theme {
    Theme::dark().with(ACCENT, Color::rgb(255, 200, 60))
}

pub fn light_with_accent() -> Theme {
    Theme::light().with(ACCENT, Color::rgb(220, 60, 90))
}

pub fn custom_theme() -> Theme {
    Theme::dark().with_many([
        (ColorToken::Primary, Color::rgb(255, 105, 180)),
        (ColorToken::OnPrimary, Color::rgb(20, 20, 30)),
        (ColorToken::Success, Color::rgb(255, 200, 60)),
        (ColorToken::Surface, Color::rgb(38, 28, 50)),
        (ColorToken::SurfaceVariant, Color::rgb(70, 50, 90)),
        (ColorToken::OnSurface, Color::rgb(245, 235, 255)),
        (ColorToken::OnSurfaceVariant, Color::rgb(180, 150, 200)),
        (ACCENT, Color::rgb(140, 200, 220)),
    ])
}

#[compose]
pub fn build_widgets() {
    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            padding: Padding::all(16),
            row_gap: 10,
            bg_color: ColorToken::Surface
        ) {
            Text (
                "LIVE THEME TOKENS",
                width: Dimension::percent(100),
                max_width: 480,
                height: 24,
                font_size: 16,
                text_color: ColorToken::OnSurface
            )
            Row (
                width: Dimension::percent(100),
                max_width: 480,
                height: 36,
                column_gap: 8
            ) {
                Button (
                    grow: 1.0,
                    height: 36,
                    border_radius: 10,
                    text_color: ColorToken::OnPrimary,
                    normal_color: Color::rgb(40, 50, 70),
                    pressed_color: Color::rgb(20, 25, 35)
                ) [
                    ThemeChoice(dark_with_accent()),
                    Text::label("Dark"),
                ] on Tap {
                    if let Some(theme) = ctx.world.get::<ThemeChoice>(ctx.entity).map(|c| c.0.clone()) {
                        theme::set_theme(ctx.world, theme);
                    }
                }
                Button (
                    grow: 1.0,
                    height: 36,
                    border_radius: 10,
                    text_color: ColorToken::OnPrimary,
                    normal_color: Color::rgb(0, 100, 200),
                    pressed_color: Color::rgb(0, 70, 150)
                ) [
                    ThemeChoice(light_with_accent()),
                    Text::label("Light"),
                ] on Tap {
                    if let Some(theme) = ctx.world.get::<ThemeChoice>(ctx.entity).map(|c| c.0.clone()) {
                        theme::set_theme(ctx.world, theme);
                    }
                }
                Button (
                    id: "theme_custom",
                    grow: 1.0,
                    height: 36,
                    border_radius: 10,
                    text_color: ColorToken::OnPrimary,
                    normal_color: Color::rgb(255, 105, 180),
                    pressed_color: Color::rgb(200, 70, 140)
                ) [
                    ThemeChoice(custom_theme()),
                    Text::label("Custom"),
                ] on Tap {
                    if let Some(theme) = ctx.world.get::<ThemeChoice>(ctx.entity).map(|c| c.0.clone()) {
                        theme::set_theme(ctx.world, theme);
                    }
                }
            }
            Row (
                id: "theme_token_grid",
                width: Dimension::percent(100),
                max_width: 480,
                grow: 1.0,
                max_height: 340,
                padding: Padding::all(10),
                wrap: FlexWrap::Wrap,
                align: AlignItems::FlexStart,
                row_gap: 6,
                column_gap: 8,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 14
            ) {
                Row (
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 6
                ) {
                    Text (
                        "Slider",
                        width: 48,
                        height: 38,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Slider (
                        min: Fixed::ZERO,
                        max: Fixed::from_int(100),
                        grow: 1.0,
                        height: 20
                    )
                }
                Row (
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 6
                ) {
                    Text (
                        "Switch",
                        width: 48,
                        height: 38,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Switch (
                        width: 44,
                        height: 22,
                        off_color: ColorToken::Outline
                    )
                }
                Row (
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 6
                ) {
                    Text (
                        "Checkbox",
                        width: 68,
                        height: 38,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Checkbox (
                        width: 20,
                        height: 20,
                        border_radius: 4,
                        border_color: ColorToken::Outline,
                        border_width: 1
                    )
                }
                Row (
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 6
                ) {
                    Text (
                        "Progress",
                        width: 68,
                        height: 38,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    ProgressBar (grow: 1.0, height: 10, border_radius: 5, value: 0.6)
                }
                Row (
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 6
                ) {
                    Text (
                        "Input",
                        width: 44,
                        height: 38,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    TextInput (
                        grow: 1.0,
                        height: 28,
                        border_radius: 8,
                        bg_color: ColorToken::Surface,
                        border_color: ColorToken::Outline,
                        border_width: 1
                    ) [
                        Placeholder("Theme-aware input"),
                    ]
                }
                Row (
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 6
                ) {
                    Text (
                        "Tabs",
                        width: 32,
                        height: 38,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    TabBar (count: 3, grow: 1.0, height: 24) {
                        Text ("A", grow: 1.0, paragraph: ParagraphStyle::label())
                        Text ("B", grow: 1.0, paragraph: ParagraphStyle::label())
                        Text ("C", grow: 1.0, paragraph: ParagraphStyle::label())
                    }
                }
                Row (
                    id: "theme_accent_row",
                    width: 200,
                    grow: 1.0,
                    height: 38,
                    align: AlignItems::Center,
                    column_gap: 6
                ) {
                    Text (
                        "Accent",
                        width: 48,
                        height: 38,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    View (grow: 1.0, height: 22, border_radius: 6, bg_color: ACCENT)
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
    app.with_theme(dark_with_accent());
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::IdMap;
    use crate::ui::UiScope;

    use crate::input::event::GestureHandler;
    use crate::input::event::gesture::GestureEvent;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(dark_with_accent());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);
        assert!(
            world
                .get::<crate::ui::Children>(parent)
                .is_some_and(|c| !c.0.is_empty())
        );
    }

    #[test]
    fn tap_button_swaps_global_theme() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(dark_with_accent());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);
        let custom_btn = world.find_by_id("theme_custom").unwrap();

        GestureHandler::trigger(
            &mut world,
            custom_btn,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: custom_btn,
            },
        );
        let theme = world.resource::<Theme>().unwrap();
        assert_eq!(theme.resolve(ACCENT), Color::rgb(140, 200, 220));
    }

    #[test]
    fn token_grid_stays_inside_landscape_and_portrait_viewports() {
        use crate::types::Viewport;
        use crate::ui::ComputedRect;
        use crate::ui::render_system::update_layout;

        for (width, height) in [(480, 320), (320, 480)] {
            let mut app = App::headless(width, height);
            app.with_default_widgets()
                .with_default_systems()
                .with_theme(dark_with_accent());
            let root = app.spawn_root().id();
            app.compose(root, build_widgets);
            app.set_root(root);
            update_layout(
                &mut app.world,
                root,
                &Viewport::new(width, height, Fixed::ONE),
            );

            let grid = app.world.find_by_id("theme_token_grid").unwrap();
            let accent = app.world.find_by_id("theme_accent_row").unwrap();
            let grid = app.world.get::<ComputedRect>(grid).unwrap().0;
            let accent = app.world.get::<ComputedRect>(accent).unwrap().0;

            assert!(grid.x >= Fixed::ZERO);
            assert!(grid.y >= Fixed::ZERO);
            assert!(grid.x + grid.w <= Fixed::from_int(width as i32));
            assert!(grid.y + grid.h <= Fixed::from_int(height as i32));
            assert!(accent.x >= grid.x);
            assert!(accent.y >= grid.y);
            assert!(accent.x + accent.w <= grid.x + grid.w);
            assert!(accent.y + accent.h <= grid.y + grid.h);
        }
    }
}
