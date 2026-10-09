use mirui::ecs::{Entity, World};
use mirui::ui::view::{PaintCtx, View};

fn paint(_: &World, _: Entity, _: &mut PaintCtx<'_>) {}

fn main() {
    let _ = View::new("raw", 64, paint);
    let _ = View::systems_only("raw", &[]);
}
