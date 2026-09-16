extern crate alloc;

use crate::core::reactive::{Computed, Signal};
use crate::prelude::*;
use crate::ui::widgets::{Button, Checkbox, Placeholder, Text, TextInput};

#[compose]
pub fn build_widgets() {
    let name_filled = Signal::new(false);
    let agreed = Signal::new(false);
    let can_submit = {
        let (name_filled, agreed) = (name_filled.clone(), agreed.clone());
        Computed::new(move || name_filled.get() && agreed.get())
    };

    let name_changed = name_filled.clone();
    let agreement_changed = agreed.clone();
    let submit_bg = can_submit.clone();

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(20),
            row_gap: 12
        ) {
            Text (
                "SIGNAL FORM",
                width: Dimension::percent(100),
                max_width: 280,
                height: 34,
                font_size: 20,
                text_color: ColorToken::OnSurface
            )
            TextInput (
                id: "state_form_name",
                width: Dimension::percent(100),
                max_width: 280,
                height: 42,
                bg_color: ColorToken::SurfaceVariant,
                border_color: ColorToken::Outline,
                border_width: 1,
                border_radius: 10
            ) [
                Placeholder("Your name"),
            ] on Changed { name_changed.set(*len > 0); }
            Row (
                width: Dimension::percent(100),
                max_width: 280,
                height: 38,
                align: AlignItems::Center,
                column_gap: 10
            ) {
                Checkbox (width: 24, height: 24) on Toggled { agreement_changed.set(*now); }
                Text ("I agree to continue", grow: 1.0, text_color: ColorToken::OnSurface)
            }
            if $submit_bg {
                Button (
                    id: "state_form_submit",
                    width: Dimension::percent(100),
                    max_width: 280,
                    height: 44,
                    border_radius: 12,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary,
                    text_color: ColorToken::OnPrimary
                ) [
                    Text::label("Ready to submit"),
                ]
            } else {
                Button (
                    id: "state_form_submit",
                    width: Dimension::percent(100),
                    max_width: 280,
                    height: 44,
                    border_radius: 12,
                    normal_color: ColorToken::SurfaceVariant,
                    pressed_color: ColorToken::SurfaceVariant,
                    text_color: ColorToken::OnSurfaceVariant
                ) [
                    Text::label("Complete the form"),
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
    use crate::app::plugins::StdInstantClockPlugin;
    app.add_plugin(StdInstantClockPlugin);
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::flush_signal_dirty;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::UiScope;

    fn emit_input_changed(world: &mut World, entity: Entity, len: u8) {
        let callback = world
            .get::<crate::ui::widgets::TextInputHandler>(entity)
            .expect("input changed handler")
            .on_event
            .clone_out();
        callback.call(
            world,
            entity,
            &crate::ui::widgets::TextInputEvent::Changed { len },
        );
        flush_signal_dirty(world);
    }

    #[test]
    fn submit_lights_up_only_when_both_set() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);

        let col = world.get::<Children>(parent).unwrap().0[0];
        let name = world.find_by_id("state_form_name").unwrap();
        let agreement_row = world.get::<Children>(col).unwrap().0[2];
        let agree = world.get::<Children>(agreement_row).unwrap().0[0];

        let normal = |world: &World| {
            let submit = world.find_by_id("state_form_submit").unwrap();
            world.get::<Button>(submit).unwrap().normal_color
        };
        let off = crate::ui::theme::ThemedColor::Token(ColorToken::SurfaceVariant);
        let on = crate::ui::theme::ThemedColor::Token(ColorToken::Primary);

        assert_eq!(normal(&world), off, "starts disabled");
        emit_input_changed(&mut world, name, 3);
        assert_eq!(normal(&world), off, "name alone is not enough");

        let callback = world
            .get::<crate::ui::widgets::checkbox::CheckboxHandler>(agree)
            .expect("checkbox handler")
            .on_event
            .clone_out();
        callback.call(
            &mut world,
            agree,
            &crate::ui::widgets::checkbox::CheckboxEvent::Toggled { now: true },
        );
        flush_signal_dirty(&mut world);
        assert_eq!(normal(&world), on, "both set -> submit enabled");
    }
}
