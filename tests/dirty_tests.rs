#[cfg(test)]
mod tests {
    use mirui::ecs::World;
    use mirui::layout::*;
    use mirui::types::{Dimension, Fixed, Rect, Viewport};
    use mirui::widget::builder::WidgetBuilder;
    use mirui::widget::dirty::{Dirty, PrevRect};
    use mirui::widget::{Children, Style};

    fn setup_world() -> (World, mirui::ecs::Entity, mirui::ecs::Entity) {
        let mut world = World::new();
        let root = WidgetBuilder::new(&mut world)
            .layout(LayoutStyle {
                width: Dimension::px(128),
                height: Dimension::px(128),
                ..Default::default()
            })
            .id();
        let child = WidgetBuilder::new(&mut world)
            .layout(LayoutStyle {
                position: Position::Absolute,
                left: Dimension::px(10),
                top: Dimension::px(20),
                width: Dimension::px(16),
                height: Dimension::px(16),
                ..Default::default()
            })
            .id();
        world.insert(child, mirui::widget::Parent(root));
        if let Some(children) = world.get_mut::<Children>(root) {
            children.0.push(child);
        }
        (world, root, child)
    }

    #[test]
    fn set_position_stores_prev_rect_on_pixel_change() {
        let (mut world, _root, child) = setup_world();

        // Move from (10, 20) to (15, 25) — pixels change
        mirui::widget::set_position(&mut world, child, 15, 25);

        let prev = world.get::<PrevRect>(child);
        assert!(
            prev.is_some(),
            "PrevRect should be stored when pixels change"
        );
        let pr = prev.unwrap().0;
        assert_eq!(pr.to_px(), (10, 20, 16, 16));
    }

    #[test]
    fn set_position_no_prev_rect_when_pixels_unchanged() {
        let (mut world, _root, child) = setup_world();

        // Move from (10, 20) to (10, 20) — exact same position
        mirui::widget::set_position(&mut world, child, 10, 20);

        let prev = world.get::<PrevRect>(child);
        assert!(
            prev.is_none(),
            "PrevRect should NOT be stored when pixel footprint unchanged"
        );
    }

    #[test]
    fn set_position_subpixel_boundary_crossing() {
        let (mut world, _root, child) = setup_world();

        // First move to subpixel position
        mirui::widget::set_position(
            &mut world,
            child,
            Fixed::from_raw(10 * 256 + 200), // 10.78
            Fixed::from_int(20),
        );

        // PrevRect should cover old pixel footprint
        let prev = world.get::<PrevRect>(child).unwrap().0;
        // Old was (10, 20, 16, 16), new is (10.78, 20) -> pixel (10, 20, 17, 16)
        // They differ in width (16 vs 17), so PrevRect stored
        assert_eq!(prev.to_px(), (10, 20, 16, 16));

        // Clear PrevRect, update style manually for next test
        world.remove::<PrevRect>(child);
        if let Some(style) = world.get_mut::<Style>(child) {
            style.layout.left = Dimension::Px(Fixed::from_raw(10 * 256 + 200));
        }

        // Move from 10.78 to 11.2 — pixel x changes from 10 to 11
        mirui::widget::set_position(
            &mut world,
            child,
            Fixed::from_raw(11 * 256 + 50), // 11.19
            Fixed::from_int(20),
        );

        let prev = world.get::<PrevRect>(child).unwrap().0;
        // Old was (10.78, 20, 16, 16) -> pixel (10, 20, 17, 16)
        assert_eq!(prev.to_px(), (10, 20, 17, 16));
    }

    #[test]
    fn dirty_region_covers_both_old_and_new() {
        let (mut world, root, child) = setup_world();

        // Move widget
        mirui::widget::set_position(&mut world, child, 50, 60);

        // Verify Dirty is set
        assert!(world.get::<Dirty>(child).is_some());

        // Collect dirty region
        let transform = Viewport::new(128, 128, Fixed::ONE);
        let dirty =
            mirui::widget::render_system::collect_dirty_region(&mut world, root, &transform);

        let area = dirty.expect("should have dirty region");
        let (dx, dy, dw, dh) = area.to_px();

        // Must cover old position (10, 20) and new position (50, 60)
        assert!(dx <= 10, "dirty x={dx} should be <= 10 (old pos)");
        assert!(dy <= 20, "dirty y={dy} should be <= 20 (old pos)");
        assert!(
            dx + dw as i32 >= 50 + 16,
            "dirty right should cover new pos right edge"
        );
        assert!(
            dy + dh as i32 >= 60 + 16,
            "dirty bottom should cover new pos bottom edge"
        );
    }

    #[test]
    fn dirty_region_covers_subpixel_edges() {
        let (mut world, root, child) = setup_world();

        // Move to subpixel position
        mirui::widget::set_position(
            &mut world,
            child,
            Fixed::from_raw(30 * 256 + 200), // 30.78
            Fixed::from_raw(40 * 256 + 100), // 40.39
        );

        let transform = Viewport::new(128, 128, Fixed::ONE);
        let dirty =
            mirui::widget::render_system::collect_dirty_region(&mut world, root, &transform);

        let area = dirty.expect("should have dirty region");
        let (dx, dy, dw, dh) = area.to_px();

        // Old: (10, 20, 16, 16) -> pixels 10..26, 20..36
        // New: (30.78, 40.39, 16, 16) -> pixels 30..47, 40..57
        assert!(dx <= 10, "dirty must cover old left edge");
        assert!(dy <= 20, "dirty must cover old top edge");
        assert!(
            dx + dw as i32 >= 47,
            "dirty must cover new right edge (ceil of 30.78+16)"
        );
        assert!(
            dy + dh as i32 >= 57,
            "dirty must cover new bottom edge (ceil of 40.39+16)"
        );
    }

    #[test]
    fn fuzz_dirty_region_always_covers_old_and_new() {
        // Property: for ANY old position and ANY new position,
        // the dirty region must fully contain both pixel footprints.
        let mut rng_state: u32 = 0xDEAD_BEEF;
        let mut rng = || -> i32 {
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 17;
            rng_state ^= rng_state << 5;
            (rng_state % 256) as i32
        };

        for _ in 0..1000 {
            let mut world = World::new();
            let root = WidgetBuilder::new(&mut world)
                .layout(LayoutStyle {
                    width: Dimension::px(128),
                    height: Dimension::px(128),
                    ..Default::default()
                })
                .id();

            let old_x = Fixed::from_raw(rng() + rng() * 256);
            let old_y = Fixed::from_raw(rng() + rng() * 256);
            let new_x = Fixed::from_raw(rng() + rng() * 256);
            let new_y = Fixed::from_raw(rng() + rng() * 256);
            let w = Fixed::from_int(8 + (rng() % 20));
            let h = Fixed::from_int(8 + (rng() % 20));

            let child = WidgetBuilder::new(&mut world)
                .layout(LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::Px(old_x),
                    top: Dimension::Px(old_y),
                    width: Dimension::Px(w),
                    height: Dimension::Px(h),
                    ..Default::default()
                })
                .id();
            world.insert(child, mirui::widget::Parent(root));
            if let Some(children) = world.get_mut::<Children>(root) {
                children.0.push(child);
            }

            mirui::widget::set_position(&mut world, child, new_x, new_y);

            if world.get::<Dirty>(child).is_none() {
                continue;
            }

            let transform = Viewport::new(128, 128, Fixed::ONE);
            let dirty =
                mirui::widget::render_system::collect_dirty_region(&mut world, root, &transform);

            let Some(area) = dirty else { continue };
            let (dx, dy, dw, dh) = area.to_px();
            let dr = dx + dw as i32;
            let db = dy + dh as i32;

            let old_rect = Rect {
                x: old_x,
                y: old_y,
                w,
                h,
            };
            let (ox, oy, ow, oh) = old_rect.to_px();

            let new_rect = Rect {
                x: new_x,
                y: new_y,
                w,
                h,
            };
            let (nx, ny, nw, nh) = new_rect.to_px();

            // Dirty must contain old footprint
            assert!(dx <= ox, "dirty x={dx} > old x={ox}");
            assert!(dy <= oy, "dirty y={dy} > old y={oy}");
            assert!(
                dr >= ox + ow as i32,
                "dirty right={dr} < old right={}",
                ox + ow as i32
            );
            assert!(
                db >= oy + oh as i32,
                "dirty bottom={db} < old bottom={}",
                oy + oh as i32
            );

            // Dirty must contain new footprint
            assert!(dx <= nx, "dirty x={dx} > new x={nx}");
            assert!(dy <= ny, "dirty y={dy} > new y={ny}");
            assert!(
                dr >= nx + nw as i32,
                "dirty right={dr} < new right={}",
                nx + nw as i32
            );
            assert!(
                db >= ny + nh as i32,
                "dirty bottom={db} < new bottom={}",
                ny + nh as i32
            );
        }
    }

    #[test]
    fn fuzz_multi_frame_movement_no_residue() {
        use mirui::draw::SwRenderer;
        use mirui::draw::texture::{ColorFormat, Texture};
        use mirui::widget::render_system;

        // Simulate multiple frames of movement, then compare with full render.
        // This catches cases where physics jumps multiple pixels per frame.
        let mut rng_state: u32 = 0xBAAD_F00D;
        let mut rng = || -> i32 {
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 17;
            rng_state ^= rng_state << 5;
            (rng_state % 256) as i32
        };

        const W: u16 = 64;
        const H: u16 = 64;
        const BUF_SIZE: usize = W as usize * H as usize * 4;

        for _ in 0..50 {
            let mut world = World::new();
            let root = WidgetBuilder::new(&mut world)
                .bg_color(mirui::types::Color::rgb(30, 30, 30))
                .layout(LayoutStyle {
                    width: Dimension::px(W as i32),
                    height: Dimension::px(H as i32),
                    ..Default::default()
                })
                .id();

            let start_x = Fixed::from_raw(rng() + (rng() % 40) * 256);
            let start_y = Fixed::from_raw(rng() + (rng() % 40) * 256);
            let widget_w = Fixed::from_int(10 + (rng() % 8));
            let widget_h = Fixed::from_int(10 + (rng() % 8));

            let child = WidgetBuilder::new(&mut world)
                .bg_color(mirui::types::Color::rgb(255, 100, 50))
                .layout(LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::Px(start_x),
                    top: Dimension::Px(start_y),
                    width: Dimension::Px(widget_w),
                    height: Dimension::Px(widget_h),
                    ..Default::default()
                })
                .id();
            world.insert(child, mirui::widget::Parent(root));
            if let Some(children) = world.get_mut::<Children>(root) {
                children.0.push(child);
            }

            // Initial full render
            let mut buf = vec![0u8; BUF_SIZE];
            let transform = Viewport::new(W, H, Fixed::ONE);
            {
                let mut renderer = SwRenderer::new(Texture::new(
                    &mut buf,
                    W as u16,
                    H as u16,
                    ColorFormat::ARGB8888,
                ));
                render_system::render(&world, root, &transform, &mut renderer);
            }

            // Simulate 10-30 frames of movement
            let num_frames = 10 + (rng() % 20) as usize;
            let vel_x = Fixed::from_raw(rng() % 512 - 256); // -1..+1 px/frame
            let vel_y = Fixed::from_raw(rng() % 512 - 256);
            let mut pos_x = start_x;
            let mut pos_y = start_y;

            for _ in 0..num_frames {
                pos_x += vel_x;
                pos_y += vel_y;
                // Clamp to screen
                pos_x = pos_x
                    .max(Fixed::ZERO)
                    .min(Fixed::from_int((W as i32) - widget_w.to_int()));
                pos_y = pos_y
                    .max(Fixed::ZERO)
                    .min(Fixed::from_int((H as i32) - widget_h.to_int()));

                mirui::widget::set_position(&mut world, child, pos_x, pos_y);

                let dirty = render_system::collect_dirty_region(&mut world, root, &transform);
                if let Some(area) = dirty {
                    let mut renderer = SwRenderer::new(Texture::new(
                        &mut buf,
                        W as u16,
                        H as u16,
                        ColorFormat::ARGB8888,
                    ));
                    render_system::render_region(&world, root, &transform, &area, &mut renderer);
                }
            }

            // Full re-render (reference)
            let mut buf_ref = vec![0u8; BUF_SIZE];
            {
                let mut renderer = SwRenderer::new(Texture::new(
                    &mut buf_ref,
                    W as u16,
                    H as u16,
                    ColorFormat::ARGB8888,
                ));
                render_system::render(&world, root, &transform, &mut renderer);
            }

            // Compare entire framebuffer — no residue anywhere
            for i in 0..BUF_SIZE {
                if buf[i] != buf_ref[i] {
                    let px = (i / 4) % W as usize;
                    let py = (i / 4) / W as usize;
                    let channel = i % 4;
                    panic!(
                        "Residue at pixel ({px},{py}) channel={channel}: dirty_render={} full_render={} after {num_frames} frames, vel=({vel_x:?},{vel_y:?})",
                        buf[i], buf_ref[i]
                    );
                }
            }
        }
    }

    #[test]
    fn fuzz_multi_widget_multi_frame_no_residue() {
        use mirui::draw::SwRenderer;
        use mirui::draw::texture::{ColorFormat, Texture};
        use mirui::widget::render_system;

        let mut rng_state: u32 = 0xF00D_CAFE;
        let mut rng = || -> i32 {
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 17;
            rng_state ^= rng_state << 5;
            (rng_state % 256) as i32
        };

        const W: u16 = 64;
        const H: u16 = 64;
        const BUF_SIZE: usize = W as usize * H as usize * 4;
        const NUM_WIDGETS: usize = 3;

        for _ in 0..30 {
            let mut world = World::new();
            let root = WidgetBuilder::new(&mut world)
                .bg_color(mirui::types::Color::rgb(30, 30, 30))
                .layout(LayoutStyle {
                    width: Dimension::px(W as i32),
                    height: Dimension::px(H as i32),
                    ..Default::default()
                })
                .id();

            let mut children_vec = Vec::new();
            let mut positions: Vec<(Fixed, Fixed)> = Vec::new();
            let mut velocities: Vec<(Fixed, Fixed)> = Vec::new();

            for _ in 0..NUM_WIDGETS {
                let x = Fixed::from_raw(rng() + (rng() % 40) * 256);
                let y = Fixed::from_raw(rng() + (rng() % 40) * 256);
                let ww = Fixed::from_int(8 + (rng() % 6));
                let wh = Fixed::from_int(8 + (rng() % 6));

                let child = WidgetBuilder::new(&mut world)
                    .bg_color(mirui::types::Color::rgb(
                        (100 + rng() % 155) as u8,
                        (100 + rng() % 155) as u8,
                        (100 + rng() % 155) as u8,
                    ))
                    .layout(LayoutStyle {
                        position: Position::Absolute,
                        left: Dimension::Px(x),
                        top: Dimension::Px(y),
                        width: Dimension::Px(ww),
                        height: Dimension::Px(wh),
                        ..Default::default()
                    })
                    .id();
                world.insert(child, mirui::widget::Parent(root));
                if let Some(ch) = world.get_mut::<Children>(root) {
                    ch.0.push(child);
                }
                children_vec.push(child);
                positions.push((x, y));
                velocities.push((
                    Fixed::from_raw(rng() % 768 - 384),
                    Fixed::from_raw(rng() % 768 - 384),
                ));
            }

            let transform = Viewport::new(W, H, Fixed::ONE);
            let mut buf = vec![0u8; BUF_SIZE];
            {
                let mut renderer = SwRenderer::new(Texture::new(
                    &mut buf,
                    W as u16,
                    H as u16,
                    ColorFormat::ARGB8888,
                ));
                render_system::render(&world, root, &transform, &mut renderer);
            }

            let num_frames = 10 + (rng() % 20) as usize;
            for _ in 0..num_frames {
                for i in 0..NUM_WIDGETS {
                    positions[i].0 += velocities[i].0;
                    positions[i].1 += velocities[i].1;
                    positions[i].0 = positions[i].0.max(Fixed::ZERO).min(Fixed::from_int(50));
                    positions[i].1 = positions[i].1.max(Fixed::ZERO).min(Fixed::from_int(50));
                    mirui::widget::set_position(
                        &mut world,
                        children_vec[i],
                        positions[i].0,
                        positions[i].1,
                    );
                }

                let dirty = render_system::collect_dirty_region(&mut world, root, &transform);
                if let Some(area) = dirty {
                    let mut renderer = SwRenderer::new(Texture::new(
                        &mut buf,
                        W as u16,
                        H as u16,
                        ColorFormat::ARGB8888,
                    ));
                    render_system::render_region(&world, root, &transform, &area, &mut renderer);
                }
            }

            let mut buf_ref = vec![0u8; BUF_SIZE];
            {
                let mut renderer = SwRenderer::new(Texture::new(
                    &mut buf_ref,
                    W as u16,
                    H as u16,
                    ColorFormat::ARGB8888,
                ));
                render_system::render(&world, root, &transform, &mut renderer);
            }

            for i in 0..BUF_SIZE {
                if buf[i] != buf_ref[i] {
                    let px = (i / 4) % W as usize;
                    let py = (i / 4) / W as usize;
                    panic!(
                        "Residue at ({px},{py}): dirty={} full={}",
                        buf[i], buf_ref[i]
                    );
                }
            }
        }
    }
}
