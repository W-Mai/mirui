use super::*;
use crate::test_support::tracked_allocations;
use crate::types::Fixed;

#[test]
fn transfer_keeps_pixels_geometry_and_charge_until_the_final_owner_releases() {
    let outputs = Outputs::<WidgetTexture>::with_budget(64);
    let mut key = super::tests::key(1, 1);
    key.layout_rect = Rect::new(13, 17, 4, 4);
    key.local_bounds = Rect::new(-1, -2, 8, 8);
    key.scale = Fixed::HALF;
    assert!(outputs.prepare_test_texture(key).unwrap().publish());
    let address = outputs
        .get(key.entity)
        .unwrap()
        .texture()
        .read_pixels()
        .unwrap()
        .bytes()
        .as_ptr();
    let budget = outputs.storage().budget.clone();
    let version = outputs.state(key.entity).unwrap().current.unwrap().version;
    let mut extracted = None;
    assert_eq!(
        tracked_allocations(|| {
            extracted = outputs.take_texture(key.entity).unwrap();
        }),
        0
    );
    let extracted = extracted.unwrap();
    assert_eq!(extracted.layout_rect(), key.layout_rect);
    assert_eq!(extracted.local_bounds(), key.local_bounds);
    assert_eq!(
        extracted.texture().read_pixels().unwrap().bytes().as_ptr(),
        address
    );
    assert_eq!(extracted.texture().cache_revision, version);
    assert!(outputs.get(key.entity).is_none());
    assert!(outputs.state(key.entity).is_none());
    assert!(outputs.prepare_test_texture(key).is_none());
    assert_eq!(budget.allocated(), 64);
    drop(outputs);
    assert_eq!(budget.allocated(), 64);
    drop(extracted);
    assert_eq!(budget.available(), 64);
}

#[test]
fn transfer_rejects_busy_history_stale_and_incompatible_storage_without_detaching() {
    let outputs = Outputs::<WidgetTexture>::with_budget(128);
    let key = super::tests::key(1, 1);
    assert!(outputs.prepare_test_texture(key).unwrap().publish());
    let read = outputs.get(key.entity).unwrap();
    assert!(outputs.take_texture(key.entity).unwrap().is_none());
    drop(read);
    {
        let _borrow = outputs.storage_mut();
        assert!(outputs.take_texture(key.entity).unwrap().is_none());
    }
    {
        let _batch = outputs.begin_batch();
        assert!(outputs.take_texture(key.entity).unwrap().is_none());
    }
    let plan = outputs.take_texture_plan().unwrap();
    assert!(outputs.take_texture(key.entity).unwrap().is_none());
    outputs.put_texture_plan(plan);
    for blocked in 0..3 {
        {
            let mut storage = outputs.storage_mut();
            let slots = storage.textures.get_mut(&key.entity).unwrap();
            slots.writing = blocked == 0;
            slots.discard = blocked == 1;
            if blocked == 2 {
                slots.shape.0 += 1;
            }
        }
        assert!(outputs.take_texture(key.entity).unwrap().is_none());
        let mut storage = outputs.storage_mut();
        let slots = storage.textures.get_mut(&key.entity).unwrap();
        slots.writing = false;
        slots.discard = false;
        slots.shape = (key.w, key.h, key.format);
    }
    outputs.set_retained(key.entity, true).unwrap();
    assert!(outputs.take_texture(key.entity).unwrap().is_none());
    outputs.set_retained(key.entity, false).unwrap();
    let stale = Entity {
        generation: key.entity.generation + 1,
        ..key.entity
    };
    assert!(outputs.take_texture(stale).unwrap().is_none());
    assert!(outputs.get(key.entity).is_some());
    let output = outputs.take_texture(key.entity).unwrap().unwrap();
    let first_revision = output.texture().cache_revision;
    assert!(outputs.prepare_test_texture(key).unwrap().publish());
    assert!(outputs.get(key.entity).unwrap().texture().cache_revision > first_revision);
}

#[test]
fn app_transfer_reconciles_lifecycle_and_can_render_the_source_again() {
    let mut app = crate::app::App::headless(4, 4);
    app.with_default_widgets().with_offscreen_pool_budget(512);
    let root = app.spawn_root().id();
    app.world.insert(root, crate::ui::OffscreenRender::new());
    app.render().unwrap();
    let output = app.take_widget_output(root).unwrap().unwrap();
    assert_eq!(output.texture().width, 4);
    assert!(app.world.has::<crate::ui::OffscreenRender>(root));
    app.render().unwrap();
    assert!(app.take_widget_output(root).unwrap().is_some());
    app.world.despawn(root);
    assert!(app.take_widget_output(root).unwrap().is_none());
    assert_eq!(
        output.into_texture().read_pixels().unwrap().bytes().len(),
        64
    );
}

#[crate::compose]
fn capture_pair() -> (Entity, Entity) {
    let source = crate::ui! {
        View(width: 4, height: 4) [crate::ui::OffscreenRender::new()] {}
    };
    let consumer = crate::ui! { View(width: 4, height: 4) {} };
    (source, consumer)
}

#[test]
fn app_transfer_observes_new_and_removed_output_consumers() {
    let mut app = crate::app::App::headless(4, 8);
    app.with_default_widgets().with_offscreen_pool_budget(1024);
    let root = app.spawn_root().id();
    let (source, consumer) = app.compose(root, capture_pair());
    app.render().unwrap();
    app.world
        .insert(consumer, crate::ui::widgets::MirrorOf::new(source));
    assert!(app.take_widget_output(source).unwrap().is_none());
    app.world.remove::<crate::ui::widgets::MirrorOf>(consumer);
    app.render().unwrap();
    assert!(app.take_widget_output(source).unwrap().is_some());
}
