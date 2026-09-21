extern crate alloc;

use crate::core::reactive::{Effect, Signal};
use crate::prelude::*;
use crate::ui::widgets::{Button, ParagraphStyle, Text};

#[compose]
pub fn build_widgets() {
    let value = Signal::new(0i32);
    // an effect with a side effect: count how many times `value` changed
    let changes = Signal::new(0i32);
    {
        let (value, changes) = (value.clone(), changes.clone());
        core::mem::forget(Effect::new(move || {
            let _ = value.get();
            changes.update(|c| *c += 1);
        }));
    }
    let (bump, vlabel, clabel) = (value.clone(), value.clone(), changes.clone());

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(20),
            row_gap: 10
        ) {
            Text (
                text: ${ alloc::format!("VALUE  {}", vlabel.get()) },
                width: Dimension::percent(100),
                max_width: 260,
                height: 52,
                font_size: 24,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label()
            )
            Text (
                text: ${ alloc::format!("effect observed {} updates", clabel.get()) },
                width: Dimension::percent(100),
                max_width: 260,
                height: 30,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label()
            )
            Button (
                width: Dimension::percent(100),
                max_width: 180,
                height: 44,
                border_radius: 12,
                normal_color: ColorToken::Secondary,
                pressed_color: ColorToken::Primary,
                text_color: ColorToken::OnSecondary
            ) [
                Text::label("Bump value"),
            ] on Tap { bump.update(|v| *v += 1); }
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
    use crate::app::plugins::StdInstantClockPlugin;
    app.add_plugin(StdInstantClockPlugin);
    app.compose(parent, build_widgets);
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(360, 260);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::flush_signal_dirty;
    use crate::input::event::GestureHandler;
    use crate::input::event::gesture::GestureEvent;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::UiScope;
    use crate::ui::widgets::text::Text;

    fn label_text(world: &World, label: Entity) -> alloc::string::String {
        let t = world.get::<Text>(label).expect("label has Text");
        t.resolve(world).into_owned()
    }

    #[test]
    fn effect_counts_each_source_change() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);

        let col = world.get::<Children>(parent).unwrap().0[0];
        let runs_label = world.get::<Children>(col).unwrap().0[1];
        let btn = world.get::<Children>(col).unwrap().0[2];

        // effect ran once at creation
        assert_eq!(label_text(&world, runs_label), "effect observed 1 updates");

        let tap = GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: btn,
        };
        GestureHandler::trigger(&mut world, btn, &tap);
        flush_signal_dirty(&mut world);
        assert_eq!(label_text(&world, runs_label), "effect observed 2 updates");
    }
}
