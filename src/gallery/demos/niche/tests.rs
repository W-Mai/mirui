use super::*;
use crate::prelude::*;
use crate::surface::FramebufferAccess;
use crate::ui::widgets::Text;
use crate::ui::{NicheMap, Parent};

#[test]
fn dirty_text_inside_slots_matches_full_render() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.render().unwrap();
    crate::ui::render_system::collect_dirty_regions(
        &mut app.world,
        root,
        &crate::types::Viewport::new(480, 320, Fixed::ONE),
    )
    .unwrap();
    let texts: Vec<_> = app
        .world
        .query::<Text>()
        .iter()
        .map(|(entity, _)| entity)
        .collect();
    for entity in texts {
        app.world.invalidate(entity);
        app.render_dirty().unwrap();
        let partial = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        app.render().unwrap();
        let full = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        let mismatches = partial.iter().zip(&full).filter(|(a, b)| a != b).count();
        assert_eq!(
            mismatches, 0,
            "dirty text {entity:?} differs from full render"
        );
    }
}

#[test]
fn build_widgets_smoke() {
    let mut app = App::headless(64, 64);
    app.with_widget(ui!(compose Card));
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets());
    let world = &app.world;

    let cards: Vec<Entity> = world.query::<Card>().collect();
    assert_eq!(cards.len(), 2, "expected two Card widgets");

    let first = cards[0];
    let map = world
        .get::<NicheMap>(first)
        .expect("card_attach registers NicheMap");
    let body = map.get("body").expect("body niche registered");

    let texts: Vec<Entity> = world.query::<crate::ui::widgets::Text>().collect();
    let body_populated = texts.into_iter().any(|t| {
        let mut cur = Some(t);
        while let Some(e) = cur {
            if e == body {
                return true;
            }
            cur = world.get::<Parent>(e).map(|p| p.0);
        }
        false
    });
    assert!(
        body_populated,
        "@body slot subtree should contain at least one Text",
    );
}
