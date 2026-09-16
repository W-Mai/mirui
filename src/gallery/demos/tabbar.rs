extern crate alloc;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
#[cfg(feature = "std")]
use crate::prelude::plugin::FpsSummaryPlugin;
use crate::prelude::*;
use crate::ui::IdMap;
use crate::ui::widgets::{ParagraphStyle, TabBar, TabContent, Text};
use alloc::format;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SelectionState {
    current: u8,
    previous: u8,
    changes: u32,
}

#[compose]
pub fn build_widgets() {
    if cx.world_mut().resource::<IdMap>().is_none() {
        cx.world_mut().insert_resource(IdMap::new());
    }
    let selection = Signal::new(SelectionState::default());
    let selection_text = selection.clone();
    let selection_action = selection.clone();

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(16),
            row_gap: 10,
            bg_color: Color::rgb(20, 20, 30)
        ) {
            Text (
                text: ${
                    let state = selection_text.get();
                    format!(
                        "selected: {} · previous: {} · changes: {}", state.current, state.previous,
                        state.changes
                    )
                },
                id: "tabbar_selection_status",
                height: 30,
                text_color: Color::rgb(255, 255, 255),
                paragraph: ParagraphStyle::label()
            )
            TabBar (
                id: "tabbar_demo_tabs",
                bg_color: Color::rgb(40, 40, 56),
                height: 40,
                count: 3,
                indicator_height: Fixed::from_int(3)
            ) on SelectionChanged {
                selection_action
                    .update(|state| {
                        state.current = *new;
                        state.previous = *old;
                        state.changes = state.changes.saturating_add(1);
                    });
            }
            {
                Text (
                    "Home",
                    text_color: Color::rgb(220, 220, 230),
                    grow: 1.0,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    "Search",
                    text_color: Color::rgb(220, 220, 230),
                    grow: 1.0,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    "Profile",
                    text_color: Color::rgb(220, 220, 230),
                    grow: 1.0,
                    paragraph: ParagraphStyle::label()
                )
            }
            View (grow: 1.0, clip_children: true) {
                Text (
                    "Home page",
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    bg_color: Color::rgb(63, 185, 80),
                    text_color: Color::rgb(255, 255, 255),
                    paragraph: ParagraphStyle::label()
                ) [
                    TabContent {
                        tab_bar: id("tabbar_demo_tabs"),
                        index: 0,
                    },
                ]
                Text (
                    "Search page",
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    bg_color: Color::rgb(255, 165, 80),
                    text_color: Color::rgb(255, 255, 255),
                    paragraph: ParagraphStyle::label()
                ) [
                    TabContent {
                        tab_bar: id("tabbar_demo_tabs"),
                        index: 1,
                    },
                ]
                Text (
                    "Profile page",
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100),
                    bg_color: Color::rgb(210, 168, 255),
                    text_color: Color::rgb(40, 40, 56),
                    paragraph: ParagraphStyle::label()
                ) [
                    TabContent {
                        tab_bar: id("tabbar_demo_tabs"),
                        index: 2,
                    },
                ]
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
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::flush_signal_dirty;
    use crate::input::event::bubble_dispatch_at;
    use crate::input::event::gesture::GestureEvent;
    use crate::input::event::multi_tap::MultiTapTracker;
    use crate::ui::Children;
    use crate::ui::ComputedRect;
    use crate::ui::UiScope;
    use crate::ui::view::ViewRegistry;
    use crate::ui::widgets::tabbar::TabBarHandler;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(MultiTapTracker::new());
        world.insert_resource(ViewRegistry::with_builtins());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
        let tabs = world.find_by_id("tabbar_demo_tabs").unwrap();
        assert!(world.has::<TabBarHandler>(tabs));
        world.insert(
            tabs,
            ComputedRect(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(300),
                h: Fixed::from_int(40),
            }),
        );
        bubble_dispatch_at(
            &mut world,
            &GestureEvent::Tap {
                x: Fixed::from_int(150),
                y: Fixed::ZERO,
                target: tabs,
            },
            100,
        );
        flush_signal_dirty(&mut world);
        let status = world.find_by_id("tabbar_selection_status").unwrap();
        let status_text = world.get::<Text>(status).unwrap().resolve(&world);
        assert!(
            status_text.contains("selected: 1 · previous: 0 · changes: 1"),
            "{status_text}"
        );
    }
}
