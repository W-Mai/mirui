use super::binding::{POOL_SIZE, ROW_H};
use super::composition::build_widgets;
use crate::input::event::scroll::ScrollOffset;
use crate::prelude::*;
use crate::ui::Children;
use crate::ui::widgets::Text;

#[test]
fn build_widgets_smoke() {
    let mut app = App::headless(320, 320);
    app.with_default_widgets();
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets());
    let mut world = app.world;
    assert!(
        world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
    let list = world.find_by_id("lazy_list_view").unwrap();
    let row = world.get::<Children>(list).unwrap().0[42 % POOL_SIZE];
    assert!(!world.has::<Text>(row));
    let label = world.get::<Children>(row).unwrap().0[0];
    let paragraph = world.get::<Text>(label).unwrap().paragraph().clone();
    world.get_mut::<ScrollOffset>(list).unwrap().y = Fixed::from_int(42 * ROW_H);
    crate::ui::widgets::lazy_list::lazy_list_system(&mut world).unwrap();
    assert_eq!(world.get::<Text>(label).unwrap().resolve(&world), "Row 42");
    assert_eq!(world.get::<Text>(label).unwrap().paragraph(), &paragraph);
    assert_eq!(
        world.get::<Style>(row).unwrap().bg_color,
        Some(ColorToken::Surface.into())
    );
}
