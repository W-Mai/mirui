use super::*;
use mirx::{
    coding::{CodingRecord, Frequency, FrequencyGeometry, Lz4, Pixel, Rle},
    image::{
        ColorDescription, EncodedImageAsset, SampleLayout, SurfaceDescriptor, UnitGroup, UnitIndex,
        UnitSelection,
    },
    types::DataIntegrity,
};

fn encoded_fixture(codec: usize, indexed: bool) -> (Rc<Vec<u8>>, Texture<'static>) {
    let surface =
        SurfaceDescriptor::new(32, 32, SampleLayout::RGBA8888, ColorDescription::SRGB).unwrap();
    let mut reference = Texture::owned(32, 32, ColorFormat::RGBA8888).unwrap();
    for y in 0..32 {
        for x in 0..32 {
            reference.pixels_mut().unwrap().set_pixel(
                x,
                y,
                &Color::rgb((x * 7) as u8, (y * 7) as u8, 255),
            );
        }
    }
    let pixels = reference.read_pixels().unwrap();
    let mut data = Vec::new();
    let mut ends = vec![0u32];
    let mut parameters = [0];
    let coding = match codec {
        0 => CodingRecord::RAW,
        1 => Rle::new().record(),
        2 => Lz4::new().record(),
        3 => Pixel::new(SampleLayout::RGBA8888).unwrap().record(),
        _ => Frequency::reversible().record_into(&mut parameters),
    };
    for y in (0..32).step_by(8) {
        for x in (0..32).step_by(8) {
            let mut tile = [0u8; 8 * 8 * 4];
            for row in 0..8 {
                tile[row * 32..(row + 1) * 32].copy_from_slice(
                    &pixels.bytes()[(y + row) * 128 + x * 4..(y + row) * 128 + (x + 8) * 4],
                );
            }
            let mut output = [0; 1024];
            let size = match codec {
                0 => {
                    output[..tile.len()].copy_from_slice(&tile);
                    tile.len()
                }
                1 => Rle::new().encode_into(&tile, &mut output).unwrap(),
                2 => Lz4::new()
                    .encoder(&mut [0u32; 256])
                    .unwrap()
                    .encode_into(&tile, &mut output)
                    .unwrap(),
                3 => Pixel::new(SampleLayout::RGBA8888)
                    .unwrap()
                    .encode_into(&tile, &mut output)
                    .unwrap(),
                _ => Frequency::reversible()
                    .encode_into(
                        FrequencyGeometry::for_plane(SampleLayout::RGBA8888, 0, 8, 8).unwrap(),
                        &tile,
                        &mut output,
                    )
                    .unwrap(),
            };
            data.extend_from_slice(&output[..size]);
            ends.push(data.len() as u32);
        }
    }
    drop(pixels);
    let indexes: Vec<_> = ends.windows(2).map(|pair| pair[0]..pair[1]).collect();
    let split = ends[8];
    let second: Vec<_> = indexes[8..]
        .iter()
        .map(|range| range.start - split..range.end - split)
        .collect();
    let first_cells: Vec<_> = (0..8u32).collect();
    let second_cells: Vec<_> = (8..16u32).collect();
    let groups = [
        UnitGroup::builder(surface, coding, &data[..split as usize])
            .with_tiles(8, 8)
            .with_selection(UnitSelection::cells(16, &first_cells).unwrap())
            .with_index(UnitIndex::ranges(&indexes[..8], mirx::types::ByteAlignment::ONE).unwrap())
            .build()
            .unwrap(),
        UnitGroup::builder(surface, coding, &data[split as usize..])
            .with_tiles(8, 8)
            .with_selection(UnitSelection::cells(16, &second_cells).unwrap())
            .with_index(UnitIndex::ranges(&second, mirx::types::ByteAlignment::ONE).unwrap())
            .build()
            .unwrap(),
    ];
    let mut asset = EncodedImageAsset::from_groups(surface, &groups);
    if indexed {
        asset = asset.with_integrity(DataIntegrity::Indexed(&ends[1..]));
    }
    let mut document = mirx::Document::new();
    let primary = document.push_encoded_image(&asset).unwrap();
    document
        .push_encoded_image_with_flags(&asset, mirx::ChunkFlags::CRITICAL)
        .unwrap();
    document.set_primary(primary).unwrap();
    (Rc::new(document.finish().unwrap().into_owned()), reference)
}

#[test]
fn encoded_sources_reuse_scalar_decoders_for_cropped_and_nested_renderer_consumption() {
    for codec in 0..5 {
        for indexed in [false, true] {
            let (bytes, reference) = encoded_fixture(codec, indexed);
            let control = Rc::new(Control::default());
            control.complete();
            let app = source_app(bytes, control.clone(), 16 * 1024, indexed);
            let texture = resolve(&app, "encoded");
            assert_eq!(control.opens.get(), 1);
            assert!(texture.pixels().unwrap().is_none());
            let area = Region::new(5, 7, 7, 4).unwrap();
            let pixels = region(&texture, area).unwrap();
            let expected = reference.read_pixels().unwrap();
            for y in area.y()..area.bottom() {
                for x in area.x()..area.right() {
                    assert_eq!(
                        pixels.get_pixel(x, y),
                        Some(expected.get_pixel(x as i32, y as i32))
                    );
                }
            }
            super::super::mapped::compare_region_routes(
                &texture,
                &reference,
                mirui::types::Rect::new(5, 7, 7, 4),
            );
            let reads = control.reads.get();
            assert_eq!(
                tracking_allocator::tracked_allocations(|| {
                    assert!(region(&texture, area).is_ok());
                }),
                0
            );
            assert_eq!(control.reads.get(), reads);
            let full = region(&texture, Region::new(0, 0, 32, 32).unwrap()).unwrap();
            assert_eq!(full.pixels().unwrap().bytes(), expected.bytes());
            // The previous crop remains immutable after the shared input workspace is reused.
            assert_eq!(pixels.get_pixel(5, 7), Some(expected.get_pixel(5, 7)));
            drop(app);
            assert_eq!(full.pixels().unwrap().bytes(), expected.bytes());
        }
    }
}

#[test]
fn encoded_pending_regions_do_not_publish_partial_tiles_or_hold_manager_borrows() {
    let (bytes, reference) = encoded_fixture(1, true);
    let control = Rc::new(Control::default());
    control.complete();
    let app = app(bytes, control.clone(), 16 * 1024);
    let texture = resolve(&app, "pending");
    control.ready.set(false);
    let area = Region::new(4, 4, 12, 12).unwrap();
    let mut cx = Context::from_waker(Waker::noop());
    assert!(texture.poll_pixels_in(area, &mut cx).is_pending());
    assert!(texture.pixels_in(area).unwrap().is_none());
    assert!(control.wake.borrow().is_some());
    let changed = Region::new(20, 20, 4, 4).unwrap();
    assert!(texture.poll_pixels_in(changed, &mut cx).is_pending());
    control.complete();
    let result = region(&texture, changed).unwrap();
    assert_eq!(
        result.get_pixel(20, 20),
        Some(reference.read_pixels().unwrap().get_pixel(20, 20))
    );
    assert!(texture.pixels_in(area).unwrap().is_some());
}

#[test]
fn encoded_primary_and_critical_corruption_and_low_budget_never_publish() {
    let (bytes, _) = encoded_fixture(2, true);
    let reader = mirx::Reader::open(&bytes).unwrap();
    let mut cases = vec![(bytes.clone(), 256)];
    for chunk in reader.chunks() {
        let payload = chunk.payload();
        let media = mirx::reader::MediaLayout::parse(&payload[..8], payload.len()).unwrap();
        let mut scan = media.sections();
        while let Some(range) = scan.record_range() {
            let section = scan.accept(&payload[range]).unwrap().unwrap();
            if section.kind() == mirx::reader::MediaSectionKind::DATA {
                let mut corrupt = (*bytes).clone();
                corrupt[chunk.payload_offset() as usize + section.offset() as usize] ^= 1;
                cases.push((Rc::new(corrupt), 16 * 1024));
            }
        }
    }
    for (bytes, budget) in cases {
        let control = Rc::new(Control::default());
        control.complete();
        let app = app(bytes, control, budget);
        let images = app
            .world
            .resource::<ResourceManager<Texture<'static>>>()
            .unwrap();
        let mut cx = Context::from_waker(Waker::noop());
        for _ in 0..256 {
            assert_ne!(images.resolve("bad").width(), 32);
            images.poll_pending(&mut cx);
        }
    }
}

#[test]
fn encoded_workspaces_and_pinned_pixels_share_capacity_across_sources() {
    let (bytes, _) = encoded_fixture(1, false);
    let control = Rc::new(Control::default());
    control.complete();
    let app = app(bytes, control, 8 * 1024);
    let a = resolve(&app, "a");
    let b = resolve(&app, "b");
    let area = Region::new(0, 0, 32, 32).unwrap();
    let first = region(&a, area).unwrap();
    let expected = first.get_pixel(23, 23);
    assert!(matches!(
        region(&b, area),
        Err(RenderError::ResourceLimit(RenderResource::Texture))
    ));
    assert_eq!(first.get_pixel(23, 23), expected);
    drop(first);
    assert_eq!(region(&b, area).unwrap().get_pixel(23, 23), expected);
}

#[test]
fn encoded_region_cancellation_retires_io_and_releases_private_workspaces() {
    let (bytes, _) = encoded_fixture(1, true);
    let control = Rc::new(Control::default());
    control.complete();
    let mut app = app(bytes, control.clone(), 8 * 1024);
    let texture = resolve(&app, "retired");
    control.ready.set(false);
    let mut cx = Context::from_waker(Waker::noop());
    assert!(
        texture
            .poll_pixels_in(Region::new(4, 12, 8, 8).unwrap(), &mut cx)
            .is_pending()
    );
    let reads = control.reads.get();
    let completions = control.completions.get();
    app.world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap()
        .remove_token("retired");
    drop(texture);
    control.complete();
    for _ in 0..4 {
        app.tick();
    }
    assert_eq!(control.reads.get(), reads);
    assert_eq!(control.completions.get(), completions + 1);
    let replacement = resolve(&app, "replacement");
    assert!(region(&replacement, Region::new(0, 0, 24, 24).unwrap()).is_ok());
}

#[test]
fn image_fallback_commits_without_losing_shared_source_readiness() {
    let (bytes, _) = encoded_fixture(1, true);
    let control = Rc::new(Control::default());
    control.complete();
    let mut app = app(bytes, control.clone(), 16 * 1024);
    app.with_widget(mirui::ui::widgets::image::view());
    let texture = resolve(&app, "image");
    control.ready.set(false);
    let root = app.spawn_root().id();
    app.compose(root, |cx| {
        ui! {
            Column(width: 32, height: 32) {
                Image(src: "image", width: 16, height: 16)
                Image(src: "image", width: 16, height: 16)
            }
        }
    });
    app.tick();
    assert!(control.wake.borrow().is_some());
    assert!(texture.pixels().unwrap().is_none());
    let calls = control.completions.get();
    for _ in 0..4 {
        app.tick();
    }
    assert_eq!(control.completions.get(), calls);
    control.complete();
    for _ in 0..64 {
        app.tick();
    }
    assert_eq!(
        tracking_allocator::tracked_allocations(|| {
            for _ in 0..4 {
                app.world.invalidate_visual(root);
                app.tick();
            }
        }),
        0,
        "resident image repaint must reuse prepared storage"
    );
    let framebuffer = app.backend.framebuffer();
    let pixels = framebuffer.read_pixels().unwrap();
    assert_eq!(pixels.get_pixel(0, 0), Color::rgb(0, 0, 255));
    assert_eq!(pixels.get_pixel(0, 16), Color::rgb(0, 0, 255));
    assert_eq!(control.opens.get(), 1);
}

#[mirui::component]
struct StrictImage {
    texture: mirui::core::cache::Handle<Texture<'static>>,
    calls: Rc<Cell<usize>>,
}

#[test]
fn changed_images_do_not_resume_cached_pixels_until_requested_again() {
    for change in 0..4 {
        let (bytes, _) = encoded_fixture(1, true);
        let control = Rc::new(Control::default());
        control.complete();
        let mut app = app(bytes, control.clone(), 16 * 1024);
        app.with_widget(mirui::ui::widgets::image::view());
        let texture = resolve(&app, "old");
        let root = app.spawn_root().id();
        let image = app.compose(root, |cx| {
            ui! { Image(src: "old", width: 32, height: 32) }
        });
        control.ready.set(false);
        app.tick();
        assert!(control.wake.borrow().is_some());
        assert!(texture.pixels().unwrap().is_none());
        match change {
            0 => {
                app.world.remove::<Image>(image);
            }
            1 => {
                app.world.despawn(image);
            }
            2 => {
                app.world.get_mut::<Image>(image).unwrap().src =
                    mirui::render::path::Path::new().into();
            }
            _ => {
                app.world
                    .resource::<ResourceManager<Texture<'static>>>()
                    .unwrap()
                    .add_static(
                        "replacement",
                        Texture::from_static(&RED[8..], 1, 1, ColorFormat::RGBA8888).unwrap(),
                    );
                app.world.get_mut::<Image>(image).unwrap().src = "replacement".into();
            }
        }
        app.dirty_regions().unwrap();
        app.tick();
        let submitted = control.reads.get();
        let completed = control.completions.get();
        control.complete();
        for _ in 0..4 {
            app.tick();
        }
        assert_eq!(
            control.reads.get(),
            submitted,
            "retired UI must not drive old pixels"
        );
        assert_eq!(control.completions.get(), completed);
        if change == 3 {
            assert_eq!(
                app.backend
                    .framebuffer()
                    .read_pixels()
                    .unwrap()
                    .get_pixel(0, 0),
                Color::rgb(255, 0, 0)
            );
        }
        // Successful values remain usable through their cache and independent owners.
        assert!(texture.ptr_eq(&resolve(&app, "old")));
        assert_eq!(
            region(&texture, Region::new(0, 0, 32, 32).unwrap())
                .unwrap()
                .get_pixel(0, 0),
            Some(Color::rgb(0, 0, 255))
        );
        assert_eq!(control.opens.get(), 1);
    }
}

#[mirui::view(component = StrictImage)]
fn strict_image(image: component!(), paint: paint!()) {
    image.calls.set(image.calls.get() + 1);
    let rect = paint.rect();
    paint.draw(&mirui::render::DrawCommand::Blit {
        pos: mirui::types::Point::new(rect.x, rect.y),
        size: mirui::types::Point::new(rect.w, rect.h),
        transform: paint.transform(),
        quad: paint.quad(),
        texture: &image.texture,
        opa: 255,
        radius: mirui::types::Fixed::ZERO,
        composite: Default::default(),
    });
}

#[test]
fn strict_paint_waits_for_readiness_but_new_damage_can_retry() {
    let (bytes, _) = encoded_fixture(2, true);
    let control = Rc::new(Control::default());
    control.complete();
    let mut app = app(bytes, control.clone(), 16 * 1024);
    let texture = resolve(&app, "strict");
    let calls = Rc::new(Cell::new(0));
    app.with_widget(strict_image::view());
    let root = app.spawn_root().id();
    app.world.insert(
        root,
        StrictImage {
            texture,
            calls: calls.clone(),
        },
    );
    control.ready.set(false);
    assert_eq!(app.render_dirty(), Err(RenderError::ResourcePending));
    let previous = calls.get();
    for _ in 0..4 {
        assert_eq!(app.render_dirty(), Err(RenderError::ResourcePending));
    }
    assert_eq!(
        calls.get(),
        previous,
        "waiting must not replay paint every tick"
    );
    app.world
        .invalidate_rect(mirui::types::Rect::new(0, 0, 4, 4));
    assert_eq!(app.render_dirty(), Err(RenderError::ResourcePending));
    assert!(calls.get() > previous);
    control.complete();
    for _ in 0..64 {
        if app.render_dirty().is_ok() {
            assert!(app.dirty_regions().unwrap().is_empty());
            return;
        }
    }
    panic!("strict source never became drawable");
}

#[mirui::component]
#[retain_previous(mirui::ui::WidgetTexture)]
struct ImageHistory;

#[mirui::view(component = ImageHistory)]
fn image_history(_history: component!(), _paint: paint!()) {}

#[test]
fn pending_offscreen_production_keeps_history_and_does_not_publish_partial_pixels() {
    use mirui::ui::{OffscreenRender, Outputs, WidgetTexture};
    let (bytes, _) = encoded_fixture(1, true);
    let control = Rc::new(Control::default());
    control.complete();
    let mut app = app(bytes, control.clone(), 16 * 1024);
    let texture = resolve(&app, "offscreen");
    app.with_widget(strict_image::view());
    app.with_widget(image_history::view());
    app.with_offscreen_pool_budget(64 * 1024);
    let root = app.spawn_root().id();
    app.world.insert(root, OffscreenRender::default());
    app.world.insert(root, ImageHistory);
    let old = Texture::from_vec(vec![255, 0, 0, 255], 1, 1, ColorFormat::RGBA8888).unwrap();
    let calls = Rc::new(Cell::new(0));
    app.world.insert(
        root,
        StrictImage {
            texture: mirui::core::cache::Handle::new(old),
            calls: calls.clone(),
        },
    );
    app.render_dirty().unwrap();
    {
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        let current = outputs.get(root).expect("completed initial output");
        assert_eq!(
            current.texture().read_pixels().unwrap().get_pixel(0, 0),
            Color::rgb(255, 0, 0)
        );
    }
    app.world.get_mut::<StrictImage>(root).unwrap().texture = texture;
    app.world.mark_subtree_dirty(root);
    control.ready.set(false);
    assert_eq!(app.render_dirty(), Err(RenderError::ResourcePending));
    {
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        assert!(
            outputs.get(root).is_none(),
            "failed production cannot be current"
        );
        let previous = outputs.previous(root).expect("frozen successful history");
        assert_eq!(
            previous.texture().read_pixels().unwrap().get_pixel(0, 0),
            Color::rgb(255, 0, 0)
        );
    }
    let before = calls.get();
    assert_eq!(app.render_dirty(), Err(RenderError::ResourcePending));
    assert_eq!(calls.get(), before);
    control.complete();
    for _ in 0..64 {
        if app.render_dirty().is_ok() {
            let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
            let current = outputs.get(root).expect("completed decoded output");
            assert_eq!(
                current.texture().read_pixels().unwrap().get_pixel(0, 0),
                Color::rgb(0, 0, 255)
            );
            return;
        }
    }
    panic!("offscreen source never became drawable");
}

#[test]
fn shared_pending_texture_wakes_both_app_consumers() {
    shared_pending_apps(false);
    shared_pending_apps(true);
}

fn shared_pending_apps(drop_last_poller: bool) {
    let (bytes, _) = encoded_fixture(1, true);
    let control = Rc::new(Control::default());
    control.complete();
    let mut first = app(bytes, control.clone(), 16 * 1024);
    let texture = resolve(&first, "shared");
    let mut second = App::headless(32, 32);
    for app in [&mut first, &mut second] {
        app.with_widget(strict_image::view());
        let root = app.spawn_root().id();
        app.world.insert(
            root,
            StrictImage {
                texture: texture.clone(),
                calls: Rc::new(Cell::new(0)),
            },
        );
    }
    control.ready.set(false);
    assert_eq!(first.render_dirty(), Err(RenderError::ResourcePending));
    assert_eq!(second.render_dirty(), Err(RenderError::ResourcePending));
    let mut second = if drop_last_poller { None } else { Some(second) };
    control.complete();
    for _ in 0..64 {
        let _ = first.render_dirty();
        if let Some(second) = &mut second {
            let _ = second.render_dirty();
        }
    }
    assert_eq!(first.render_dirty(), Ok(()));
    if let Some(second) = &mut second {
        assert_eq!(second.render_dirty(), Ok(()));
    }
}
