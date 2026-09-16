extern crate alloc;

use crate::prelude::*;
use crate::ui::widgets::{Button, ParagraphStyle, Text};

#[compose]
pub fn build_widgets() {
    let count = Signal::new(0i32);
    let (dec, inc, label) = (count.clone(), count.clone(), count.clone());

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(20),
            row_gap: 14
        ) {
            Text (
                text: ${ alloc::format!("COUNT  {}", label.get()) },
                width: Dimension::percent(100),
                max_width: 260,
                height: 56,
                font_size: 26,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label()
            )
            Row (
                width: Dimension::percent(100),
                max_width: 180,
                height: 44,
                column_gap: 12
            ) {
                Button (
                    grow: 1.0,
                    height: 44,
                    border_radius: 12,
                    normal_color: ColorToken::Error,
                    pressed_color: ColorToken::SurfaceVariant,
                    text_color: ColorToken::OnPrimary
                ) [
                    Text::label("−"),
                ] on Tap { dec.update(|n| *n -= 1); }
                Button (
                    grow: 1.0,
                    height: 44,
                    border_radius: 12,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary,
                    text_color: ColorToken::OnPrimary
                ) [
                    Text::label("+"),
                ] on Tap { inc.update(|n| *n += 1); }
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
    use crate::app::plugins::StdInstantClockPlugin;
    app.add_plugin(StdInstantClockPlugin);
    app.compose(parent, build_widgets);
}

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
    fn tap_increments_and_reactive_text_updates() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);

        let col = world.get::<Children>(parent).unwrap().0[0];
        let label = world.get::<Children>(col).unwrap().0[0];
        let row = world.get::<Children>(col).unwrap().0[1];
        let inc = world.get::<Children>(row).unwrap().0[1];

        assert_eq!(label_text(&world, label), "COUNT  0");

        let tap = GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: inc,
        };
        GestureHandler::trigger(&mut world, inc, &tap);
        flush_signal_dirty(&mut world);
        assert_eq!(label_text(&world, label), "COUNT  1");

        GestureHandler::trigger(&mut world, inc, &tap);
        flush_signal_dirty(&mut world);
        assert_eq!(label_text(&world, label), "COUNT  2");
    }
}
