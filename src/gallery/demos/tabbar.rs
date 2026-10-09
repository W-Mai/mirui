extern crate alloc;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
#[cfg(feature = "std")]
use crate::prelude::plugin::FpsSummaryPlugin;
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, TabBar, TabContent, Text, TextAlign};
use alloc::format;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SelectionState {
    current: u8,
    previous: u8,
    changes: u32,
}

#[compose]
pub fn build_widgets() {
    let selection = Signal::new(SelectionState::default());
    let selection_text = selection.clone();
    let selection_action = selection.clone();

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
                text: ${
                    let state = selection_text.get();
                    format!(
                        "TAB {} · FROM {} · {} CHANGE{}", state.current + 1, state.previous + 1, state
                        .changes, if state.changes == 1 { "" } else { "S" }
                    )
                },
                id: "tabbar_selection_status",
                width: Dimension::percent(100),
                max_width: 480,
                height: 22,
                font_size: 10,
                text_color: ColorToken::Primary,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            TabBar (
                id: "tabbar_demo_tabs",
                width: Dimension::percent(100),
                max_width: 480,
                bg_color: ColorToken::SurfaceVariant,
                height: 44,
                border_radius: 14,
                clip_children: true,
                count: 3,
                indicator_height: Fixed::from_int(3)
            ) {
                Text (
                    "Home",
                    text_color: ColorToken::OnSurfaceVariant,
                    grow: 1.0,
                    height: Dimension::percent(100),
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    "Search",
                    text_color: ColorToken::OnSurfaceVariant,
                    grow: 1.0,
                    height: Dimension::percent(100),
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    "Profile",
                    text_color: ColorToken::OnSurfaceVariant,
                    grow: 1.0,
                    height: Dimension::percent(100),
                    paragraph: ParagraphStyle::label()
                )
            } on SelectionChanged {
                selection_action
                    .update(|state| {
                        state.current = event.new;
                        state.previous = event.old;
                        state.changes = state.changes.saturating_add(1);
                    });
            }
            View (
                width: Dimension::percent(100),
                max_width: 480,
                grow: 1.0,
                clip_children: true,
                border_radius: 18
            ) {
                Column (
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    justify: JustifyContent::Center,
                    padding: Padding::all(22),
                    row_gap: 8,
                    bg_color: ColorToken::Primary
                ) [
                    TabContent {
                        tab_bar: id("tabbar_demo_tabs"),
                        index: 0,
                    },
                ] {
                    Text (
                        "HOME",
                        height: 30,
                        font_size: 22,
                        text_color: ColorToken::OnPrimary,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "Active workspace overview.",
                        text_color: ColorToken::OnPrimary
                    )
                }
                Column (
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    justify: JustifyContent::Center,
                    padding: Padding::all(22),
                    row_gap: 8,
                    bg_color: ColorToken::Secondary
                ) [
                    TabContent {
                        tab_bar: id("tabbar_demo_tabs"),
                        index: 1,
                    },
                ] {
                    Text (
                        "SEARCH",
                        height: 30,
                        font_size: 22,
                        text_color: ColorToken::OnSecondary,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "Explore the current workspace.",
                        text_color: ColorToken::OnSecondary
                    )
                }
                Column (
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    justify: JustifyContent::Center,
                    padding: Padding::all(22),
                    row_gap: 8,
                    bg_color: ColorToken::Tertiary
                ) [
                    TabContent {
                        tab_bar: id("tabbar_demo_tabs"),
                        index: 2,
                    },
                ] {
                    Text (
                        "PROFILE",
                        height: 30,
                        font_size: 22,
                        text_color: ColorToken::OnTertiary,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "Identity and preferences.",
                        text_color: ColorToken::OnTertiary
                    )
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
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default());
    app.compose(parent, build_widgets());
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 320);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::flush_signal_dirty;
    use crate::input::event::gesture::GestureEvent;
    use crate::ui::ComputedRect;
    use crate::ui::UiScope;
    use crate::ui::{Children, IdMap};

    #[test]
    fn compositions_register_ids_without_replacing_the_application_map() {
        type Composition = fn(&mut UiScope<'_>);
        let demos: [(Composition, &str); 3] = [
            (
                |cx| crate::gallery::demos::widgets::build_widgets()(cx),
                "widgets_list",
            ),
            (
                |cx| crate::gallery::demos::widgets_compact::build_widgets()(cx),
                "compact_widgets_list",
            ),
            (|cx| build_widgets()(cx), "tabbar_demo_tabs"),
        ];
        for (compose, id) in demos {
            let mut app = App::headless(480, 320);
            app.with_default_widgets();
            let root = app.spawn_root().id();
            let sentinel = app.world.spawn_empty();
            app.world
                .resource_mut::<IdMap>()
                .unwrap()
                .insert("existing_composition_id", sentinel);
            app.compose(root, compose);
            assert_eq!(
                app.world.find_by_id("existing_composition_id"),
                Some(sentinel)
            );
            assert!(
                app.world
                    .find_by_id(id)
                    .is_some_and(|entity| app.world.is_alive(entity))
            );
            assert!(
                app.world
                    .get::<Children>(root)
                    .is_some_and(|children| !children.0.is_empty())
            );
        }
    }

    #[test]
    fn build_widgets_smoke() {
        let mut app = App::headless(480, 320);
        app.with_event_capacity(
            EventCapacity::new()
                .notifications::<crate::ui::widgets::tabbar::SelectionChanged>(1)
                .flush_limit(1),
        );
        app.with_default_widgets();
        let parent = app.spawn_root().layout(LayoutStyle::default()).id();
        app.compose(parent, build_widgets());
        let world = &mut app.world;
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
        let tabs = world.find_by_id("tabbar_demo_tabs").unwrap();
        assert!(
            crate::input::event::runtime::has_entity_listener::<
                crate::ui::widgets::tabbar::SelectionChanged,
            >(world, tabs)
            .unwrap()
        );
        world.insert(
            tabs,
            ComputedRect(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(300),
                h: Fixed::from_int(40),
            }),
        );
        {
            crate::__macro_support::dispatch_entity_gesture(
                world,
                &GestureEvent::Tap {
                    x: Fixed::from_int(150),
                    y: Fixed::ZERO,
                    target: tabs,
                },
                100,
            )
            .unwrap();
            crate::__macro_support::flush_entity_events(world).unwrap();
        };
        flush_signal_dirty(world);
        let status = world.find_by_id("tabbar_selection_status").unwrap();
        let status_text = world.get::<Text>(status).unwrap().resolve(world);
        assert!(
            status_text.contains("TAB 2 · FROM 1 · 1 CHANGE"),
            "{status_text}"
        );
    }
}
