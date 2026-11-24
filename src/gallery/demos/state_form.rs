extern crate alloc;

use crate::core::reactive::{Computed, Signal};
use crate::gallery::sample_style::{centered_sample, primary_action};
use crate::prelude::*;
use crate::ui::widgets::{Button, Checkbox, Placeholder, Text, TextEditing, TextInput};

fn form_width() -> StylePreset {
    style!(width: Dimension::percent(100), max_width: 280)
}

fn submit_style() -> StylePreset {
    style!(width: Dimension::percent(100), max_width: 280, font_size: 16, height: 44, border_radius: 22)
}

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
            style: centered_sample(),
            row_gap: 12
        ) {
            Text (
                "Signal form",
                style: form_width(),
                height: 34,
                font_size: 20,
                text_color: ColorToken::OnSurface
            )
            TextInput (
                id: "state_form_name",
                style: form_width(),
                height: 42,
                font_size: 15,
                bg_color: ColorToken::SurfaceVariant,
                border_color: ColorToken::Outline,
                border_width: 1,
                border_radius: 22
            ) [
                Placeholder("Your name"),
            ] on TextEditing::Changed { name_changed.set(event.len > 0); }
            Row (
                style: form_width(),
                height: 38,
                align: AlignItems::Center,
                column_gap: 10
            ) {
                Checkbox (
                    width: 24,
                    height: 24,
                    border_radius: 8,
                    unchecked_color: ColorToken::Outline
                ) on Toggled { agreement_changed.set(event.now); }
                Text (
                    "I agree to continue",
                    grow: 1.0,
                    font_size: 14,
                    text_color: ColorToken::OnSurface
                )
            }
            if $submit_bg {
                Button (
                    "Ready to submit",
                    id: "state_form_submit_ready",
                    style: primary_action(),
                    width: Dimension::percent(100),
                    max_width: 280,
                    font_size: 16
                )
            } else {
                Button (
                    "Complete the form",
                    id: "state_form_submit_pending",
                    style: submit_style(),
                    normal_color: ColorToken::Outline,
                    pressed_color: ColorToken::Outline,
                    text_color: ColorToken::OnSurfaceVariant
                )
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
    crate::gallery::showcase_theme::install_sample(app);
    app.compose(parent, build_widgets());
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(360, 280);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;
    use crate::input::event::input::{InputEvent, PointerKind, TextCommitted};
    use crate::input::event::sim::{SimCommand, SimulatedInput, sim_input_system};
    use crate::ui::{Children, ComputedRect};

    type TestApp = App<crate::surface::framebuf::FramebufSurface<fn(&[u8], PhysicalRect)>>;

    fn input(app: &mut TestApp, events: impl IntoIterator<Item = InputEvent>) {
        app.add_resource(SimulatedInput::new(
            events
                .into_iter()
                .map(|event| SimCommand::at(0, event))
                .collect(),
        ));
        assert!(!app.tick());
    }

    fn click(app: &mut TestApp, entity: Entity) {
        let rect = app.world.get::<ComputedRect>(entity).unwrap().0;
        let x = rect.x + rect.w / Fixed::from_int(2);
        let y = rect.y + rect.h / Fixed::from_int(2);
        input(
            app,
            [
                InputEvent::PointerDown {
                    kind: PointerKind::Mouse,
                    id: 0,
                    x,
                    y,
                },
                InputEvent::PointerUp {
                    kind: PointerKind::Mouse,
                    id: 0,
                    x,
                    y,
                },
            ],
        );
    }

    #[test]
    fn submit_lights_up_only_when_both_set() {
        let mut app = App::headless(360, 280);
        app.with_default_widgets();
        app.add_system(sim_input_system::system());
        let parent = app.spawn_root().id();
        app.compose(parent, build_widgets());
        app.render().unwrap();
        let world = &app.world;

        let col = world.get::<Children>(parent).unwrap().0[0];
        let name = world.find_by_id("state_form_name").unwrap();
        let agreement_row = world.get::<Children>(col).unwrap().0[2];
        let agree = world.get::<Children>(agreement_row).unwrap().0[0];

        let ready = world.find_by_id("state_form_submit_ready").unwrap();
        let pending = world.find_by_id("state_form_submit_pending").unwrap();
        let normal = |world: &World, expected_ready: bool| {
            let ready_hidden = crate::ui::branch::is_effectively_hidden(world, ready);
            let pending_hidden = crate::ui::branch::is_effectively_hidden(world, pending);
            assert_eq!(ready_hidden, !expected_ready);
            assert_eq!(pending_hidden, expected_ready);
            let submit = if expected_ready { ready } else { pending };
            world.get::<Button>(submit).unwrap().normal_color
        };
        let off = crate::ui::theme::ThemedColor::Token(ColorToken::Outline);
        let on = crate::ui::theme::ThemedColor::Token(ColorToken::Primary);

        assert_eq!(normal(world, false), off, "starts disabled");
        click(&mut app, name);
        input(&mut app, [TextCommitted::new("Ada").into()]);
        app.world
            .get::<TextEditing>(name)
            .unwrap()
            .read(|text| assert_eq!(text, "Ada"));
        assert_eq!(normal(&app.world, false), off, "name alone is not enough");
        click(&mut app, agree);
        assert!(app.world.get::<Checkbox>(agree).unwrap().checked);
        assert_eq!(normal(&app.world, true), on, "both set -> submit enabled");

        click(&mut app, agree);
        assert!(!app.world.get::<Checkbox>(agree).unwrap().checked);
        assert_eq!(normal(&app.world, false), off, "unchecking disables submit");
    }
}
