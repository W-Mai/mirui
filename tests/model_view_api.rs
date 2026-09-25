use mirui::app::App;
use mirui::core::model::SharedValue;
use mirui::ui::view::ViewCtx;
use mirui::{component, model, view};

#[model]
struct Level {
    #[observe]
    value: u16,
}

#[model]
impl Level {
    fn set(&mut self, value: u16) {
        self.value = value;
    }
}

#[component(bind(left, right))]
struct Pair {
    left: Level,
    right: Level,
}

#[view(
    component = Pair,
    read(left, right),
    watch(left.value(), right.value()),
    priority = 64,
)]
fn paint_pair(left: &Level, right: &Level, ctx: &mut ViewCtx<'_>) {
    ctx.bg_handled = left.value < right.value;
}

#[test]
fn external_crate_registers_a_multi_model_view() {
    let mut app = App::headless(32, 32);
    let left = app.add_model(Level { value: 1 });
    let right = app.add_model(Level { value: 2 });
    app.with_widget(paint_pair::view());
    let entity = app.world.spawn_empty();
    app.world.insert(
        entity,
        Pair {
            left: left.share(),
            right: right.share(),
        },
    );
    left.set(3);
    right.set(4);
}
