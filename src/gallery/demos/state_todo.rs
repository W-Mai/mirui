extern crate alloc;

use crate::core::reactive::{Computed, Signal};
use crate::prelude::*;
use crate::ui::widgets::{Button, ParagraphStyle, Text};

#[compose]
fn todo_row(label: &'static str, completed_label: &'static str, done: Signal<bool>) -> Entity {
    let state = done.clone();
    let toggle = done;

    ui! {
        Button (
            width: Dimension::percent(100),
            max_width: 280,
            height: 40,
            border_radius: 10,
            normal_color: ColorToken::SurfaceVariant,
            pressed_color: ColorToken::Primary,
            text_color: ColorToken::OnSurface
        ) on Tap { toggle.update(|done| *done = !*done); }
        {
            Text (
                text: ${ alloc::string::String::from(if state.get() { completed_label } else { label }) },
                grow: 1.0,
                paragraph: ParagraphStyle::label()
            )
        }
    }
}

#[compose]
pub fn build_widgets() {
    let milk = Signal::new(false);
    let docs = Signal::new(false);
    let release = Signal::new(false);
    let remaining = {
        let states = [milk.clone(), docs.clone(), release.clone()];
        Computed::new(move || states.iter().filter(|state| !state.get()).count() as i32)
    };
    let summary = remaining.clone();

    let _ = ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(20),
            row_gap: 8
        ) {
            Text (
                text: ${ alloc::format!("{} REMAINING", summary.get()) },
                width: Dimension::percent(100),
                max_width: 280,
                height: 38,
                font_size: 18,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label()
            )
            todo_row ("Buy milk", "✓  Buy milk", milk)
            todo_row ("Write docs", "✓  Write docs", docs)
            todo_row ("Ship release", "✓  Ship release", release)
        }
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::app::plugins::StdInstantClockPlugin;
    app.add_plugin(StdInstantClockPlugin);
    app.compose(parent, build_widgets);
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(320, 280);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::flush_signal_dirty;
    use crate::input::event::GestureHandler;
    use crate::input::event::gesture::GestureEvent;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::UiScope;

    fn label_text(world: &World, label: Entity) -> alloc::string::String {
        let t = world.get::<Text>(label).expect("label has Text");
        t.resolve(world).into_owned()
    }

    #[test]
    fn toggling_a_row_updates_remaining_count() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);

        let root = world.get::<Children>(parent).unwrap().0[0];
        let kids = world.get::<Children>(root).unwrap().0.clone();
        let summary = kids[0];
        let first_row = kids[1];

        assert_eq!(label_text(&world, summary), "3 REMAINING");

        GestureHandler::trigger(
            &mut world,
            first_row,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: first_row,
            },
        );
        flush_signal_dirty(&mut world);
        assert_eq!(label_text(&world, summary), "2 REMAINING");
    }
}
