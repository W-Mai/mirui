extern crate alloc;

use crate::prelude::*;
use crate::ui::widgets::{Button, ParagraphStyle, Text};

#[derive(Default)]
#[crate::model]
pub struct TodoModel {
    #[observe]
    milk: bool,
    #[observe]
    docs: bool,
    #[observe]
    release: bool,
}

#[derive(Clone, Copy)]
enum TodoItem {
    Milk,
    Docs,
    Release,
}

#[crate::model]
impl TodoModel {
    #[observe]
    fn remaining(&self) -> i32 {
        3 - i32::from(self.milk) - i32::from(self.docs) - i32::from(self.release)
    }

    fn toggle(&mut self, item: TodoItem) {
        match item {
            TodoItem::Milk => self.milk = !self.milk,
            TodoItem::Docs => self.docs = !self.docs,
            TodoItem::Release => self.release = !self.release,
        }
    }
}

#[compose(bind(todo))]
fn todo_row(
    label: &'static str,
    completed_label: &'static str,
    item: TodoItem,
    todo: TodoModel,
) -> Entity {
    ui! {
        Button (
            width: Dimension::percent(100),
            max_width: 280,
            height: 40,
            border_radius: 10,
            normal_color: ColorToken::SurfaceVariant,
            pressed_color: ColorToken::Primary,
            text_color: ColorToken::OnSurface
        ) on Tap { todo.toggle(item); }
        {
            Text (
                text: ${
                    let done = match item {
                        TodoItem::Milk => todo.milk(),
                        TodoItem::Docs => todo.docs(),
                        TodoItem::Release => todo.release(),
                    };
                    alloc::string::String::from(if done { completed_label } else { label })
                },
                grow: 1.0,
                paragraph: ParagraphStyle::label()
            )
        }
    }
}

#[compose(bind(todo))]
pub fn build_widgets(todo: TodoModel) {
    let milk = TodoItem::Milk;
    let docs = TodoItem::Docs;
    let release = TodoItem::Release;
    let _ = ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(20),
            row_gap: 8
        ) {
            Text (
                text: ${ alloc::format!("{} REMAINING", todo.remaining()) },
                width: Dimension::percent(100),
                max_width: 280,
                height: 38,
                font_size: 18,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label()
            )
            todo_row ("Buy milk", "✓  Buy milk", milk, todo)
            todo_row ("Write docs", "✓  Write docs", docs, todo)
            todo_row ("Ship release", "✓  Ship release", release, todo)
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
    let todo = app.add_model(TodoModel::default());
    app.compose(parent, |cx| build_widgets(cx, todo));
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

    fn todo_world() -> (World, Entity, [Entity; 3]) {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let (cell, todo) = crate::core::model::register(&mut world, TodoModel::default());
        let registration = world.spawn_empty();
        world.insert(registration, cell);
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx, todo);
        drop(cx);

        let root = world.get::<Children>(parent).unwrap().0[0];
        let (summary, rows) = {
            let children = &world.get::<Children>(root).unwrap().0;
            (children[0], [children[1], children[2], children[3]])
        };
        (world, summary, rows)
    }

    fn tap_row(world: &mut World, row: Entity) {
        GestureHandler::trigger(
            world,
            row,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: row,
            },
        );
        flush_signal_dirty(world);
    }

    fn row_label(world: &World, row: Entity) -> alloc::string::String {
        let label = world.get::<Children>(row).unwrap().0[0];
        label_text(world, label)
    }

    #[test]
    fn toggling_a_row_updates_remaining_count() {
        let (mut world, summary, rows) = todo_world();
        assert_eq!(label_text(&world, summary), "3 REMAINING");

        tap_row(&mut world, rows[0]);
        assert_eq!(label_text(&world, summary), "2 REMAINING");

        tap_row(&mut world, rows[0]);
        assert_eq!(label_text(&world, summary), "3 REMAINING");
    }

    #[test]
    fn row_states_update_independently() {
        let (mut world, summary, rows) = todo_world();
        assert_eq!(row_label(&world, rows[0]), "Buy milk");
        assert_eq!(row_label(&world, rows[1]), "Write docs");
        assert_eq!(row_label(&world, rows[2]), "Ship release");

        tap_row(&mut world, rows[1]);
        assert_eq!(row_label(&world, rows[0]), "Buy milk");
        assert_eq!(row_label(&world, rows[1]), "✓  Write docs");
        assert_eq!(row_label(&world, rows[2]), "Ship release");
        assert_eq!(label_text(&world, summary), "2 REMAINING");

        tap_row(&mut world, rows[2]);
        assert_eq!(row_label(&world, rows[0]), "Buy milk");
        assert_eq!(row_label(&world, rows[1]), "✓  Write docs");
        assert_eq!(row_label(&world, rows[2]), "✓  Ship release");
        assert_eq!(label_text(&world, summary), "1 REMAINING");
    }
}
