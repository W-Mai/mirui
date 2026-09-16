extern crate alloc;

use crate::core::reactive::{Computed, Signal};
use crate::prelude::*;
use crate::ui::widgets::{Button, ParagraphStyle, Text};

#[compose]
pub fn build_widgets() {
    let n = Signal::new(2i32);
    let squared = {
        let n = n.clone();
        Computed::new(move || n.get() * n.get())
    };
    let (inc, label) = (n.clone(), squared.clone());

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
                text: ${ alloc::format!("n² = {}", label.get()) },
                width: Dimension::percent(100),
                max_width: 260,
                height: 58,
                font_size: 28,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label()
            )
            Button (
                width: Dimension::percent(100),
                max_width: 180,
                height: 44,
                border_radius: 12,
                normal_color: ColorToken::Primary,
                pressed_color: ColorToken::Secondary,
                text_color: ColorToken::OnPrimary
            ) [
                Text::label("Increment n"),
            ] on Tap { inc.update(|v| *v += 1); }
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
    fn computed_label_tracks_source() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);

        let col = world.get::<Children>(parent).unwrap().0[0];
        let label = world.get::<Children>(col).unwrap().0[0];
        let btn = world.get::<Children>(col).unwrap().0[1];

        assert_eq!(label_text(&world, label), "n² = 4");

        let tap = GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: btn,
        };
        GestureHandler::trigger(&mut world, btn, &tap);
        flush_signal_dirty(&mut world);
        assert_eq!(label_text(&world, label), "n² = 9");
    }
}
