use super::composition::build_widgets;
#[cfg(feature = "std")]
use super::layout::layout_system;
use super::state::CoverFlowBounds;
#[cfg(feature = "std")]
use super::state::{Carousel, CarouselCard};
#[cfg(feature = "std")]
use crate::ecs::SystemScheduler;
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::types::Dimension;
use crate::ui::Children;

#[test]
fn build_widgets_smoke() {
    let mut app = App::headless(640, 360);
    app.add_resource(CoverFlowBounds::for_view(320, 180));
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets(640, 360));
    let bounds = app.world.resource::<CoverFlowBounds>().unwrap();
    assert_eq!((bounds.view_w, bounds.view_h), (640, 360));
    assert!(
        app.world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
}

#[cfg(feature = "std")]
#[test]
fn layout_system_tracks_live_viewport() {
    use crate::types::Viewport;
    use crate::ui::render_system::try_update_layout;

    let mut app = crate::app::App::headless(640, 360);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    app.compose(root, build_widgets(640, 360));
    app.set_root(root);

    let carousel = app.world.query::<Carousel>().collect()[0];
    let card = app.world.query::<CarouselCard>().collect()[0];

    let widths_at = |app: &mut crate::app::App<_, _>, w: u16, h: u16| -> (i32, i32) {
        let vp = Viewport::new(w, h, Fixed::ONE);
        try_update_layout(&mut app.world, root, &vp).unwrap();
        let mut scheduler = SystemScheduler::new();
        scheduler.add(layout_system::system());
        scheduler.run_all(&mut app.world).unwrap();
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
