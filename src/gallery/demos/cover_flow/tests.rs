use super::composition::build_widgets;
#[cfg(feature = "std")]
use super::layout::layout_system;
use super::state::CoverFlowBounds;
#[cfg(feature = "std")]
use super::state::{Carousel, CarouselCard};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::types::Dimension;
use crate::ui::{Children, IdMap, UiScope};

#[test]
fn build_widgets_smoke() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx, 640, 360);
    drop(cx);
    assert!(
        world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
}

#[cfg(feature = "std")]
#[test]
fn layout_system_tracks_live_viewport() {
    use crate::types::Viewport;
    use crate::ui::render_system::update_layout;

    let mut app = crate::app::App::headless(640, 360);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    app.compose(root, |cx| build_widgets(cx, 640, 360));
    app.set_root(root);

    let carousel = app.world.query::<Carousel>().collect()[0];
    let card = app.world.query::<CarouselCard>().collect()[0];

    let widths_at = |app: &mut crate::app::App<_, _>, w: u16, h: u16| -> (i32, i32) {
        let vp = Viewport::new(w, h, Fixed::ONE);
        update_layout(&mut app.world, root, &vp);
        layout_system(&mut app.world);
        let carousel_width = match app.world.get::<Style>(carousel).map(|s| s.layout.width) {
            Some(Dimension::Px(px)) => px.to_int(),
            _ => -1,
        };
        let card_width = match app.world.get::<Style>(card).map(|s| s.layout.width) {
            Some(Dimension::Px(px)) => px.to_int(),
            _ => -1,
        };
        (carousel_width, card_width)
    };

    let narrow = widths_at(&mut app, 480, 320);
    let wide = widths_at(&mut app, 960, 540);
    assert!(
        wide.0 > narrow.0,
        "carousel width must grow with the live viewport: {} -> {}",
        narrow.0,
        wide.0,
    );
    assert!(
        wide.1 > narrow.1,
        "card width must grow with the live viewport: {} -> {}",
        narrow.1,
        wide.1,
    );
}

#[test]
fn portrait_perspective_keeps_the_complete_card_in_front() {
    let bounds = CoverFlowBounds::for_px(360, 640);
    assert!(bounds.perspective > bounds.card_h / 2);
}
