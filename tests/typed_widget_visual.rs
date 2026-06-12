extern crate alloc;

#[cfg(test)]
mod tests {
    use mirui::components::{Button, Text};
    use mirui::draw::sw::SwRenderer;
    use mirui::draw::texture::ColorFormat;
    use mirui::ecs::World;
    use mirui::event::GestureHandler;
    use mirui::event::gesture::GestureEvent;
    use mirui::layout::FlexDirection;
    use mirui::prelude::*;
    use mirui::surface::FramebufferAccess;
    use mirui::surface::framebuf::FramebufSurface;
    use mirui::types::Viewport;
    use mirui::widget::builder::WidgetBuilder;
    use mirui::widget::render_system;

    fn dummy_handler(_: &mut World, _: mirui::ecs::Entity, _: &GestureEvent) -> bool {
        false
    }

    fn count_non_bg_pixels(
        buf: &[u8],
        stride: usize,
        w: usize,
        h: usize,
        bg: (u8, u8, u8),
    ) -> usize {
        let mut n = 0;
        for y in 0..h {
            for x in 0..w {
                let i = y * stride + x * 4;
                if buf[i] != bg.0 || buf[i + 1] != bg.1 || buf[i + 2] != bg.2 {
                    n += 1;
                }
            }
        }
        n
    }

    #[test]
    fn button_typed_widget_renders_visible_pixels() {
        let width: u16 = 320;
        let height: u16 = 100;
        let bg = (20, 20, 30);

        let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
        let mut app: App<_, _> = App::new(backend);
        app.with_default_widgets().with_default_systems();

        let root = WidgetBuilder::new(&mut app.world)
            .bg_color(Color::rgb(bg.0, bg.1, bg.2))
            .layout(LayoutStyle {
                direction: FlexDirection::Column,
                width: Dimension::px(width as i32),
                height: Dimension::px(height as i32),
                ..Default::default()
            })
            .id();

        ui! {
            :(
                parent: root
                world: &mut app.world
            :)

            Row (height: 44) {
                Button (
                    grow: 1.0,
                    height: 36,
                    border_radius: 6,
                    text_color: ColorToken::OnPrimary,
                    normal_color: Color::rgb(40, 50, 70),
                    pressed_color: Color::rgb(20, 25, 35)
                ) [
                    GestureHandler::from_fn(dummy_handler),
                ] {
                    Text("Dark") {}
                }
            }
        };

        app.set_root(root);

        let world = &mut app.world;
        let viewport = Viewport::new(width, height, Fixed::ONE);
        render_system::update_layout(world, root, &viewport);
        {
            let tex = app.backend.framebuffer();
            let mut renderer = SwRenderer::new(tex);
            renderer.viewport = viewport;
            render_system::render(world, root, &viewport, &mut renderer);
        }

        let tex = app.backend.framebuffer();
        let pixels = tex.buf.as_slice();
        let stride = tex.stride;
        let non_bg = count_non_bg_pixels(pixels, stride, width as usize, height as usize, bg);

        assert!(
            non_bg > 100,
            "Button should paint visible pixels, got {non_bg} non-bg pixels"
        );

        let mut found_button_color = false;
        for y in 0..(height as usize) {
            for x in 0..(width as usize) {
                let i = y * stride + x * 4;
                let r = pixels[i];
                let g = pixels[i + 1];
                let b = pixels[i + 2];
                if r >= 35 && r <= 45 && g >= 45 && g <= 55 && b >= 65 && b <= 75 {
                    found_button_color = true;
                    break;
                }
            }
            if found_button_color {
                break;
            }
        }
        assert!(
            found_button_color,
            "expected to see button normal_color (40,50,70) somewhere"
        );
    }

    #[test]
    fn row_widget_lays_children_horizontally() {
        let width: u16 = 200;
        let height: u16 = 40;
        let bg = (0, 0, 0);

        let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
        let mut app: App<_, _> = App::new(backend);
        app.with_default_widgets().with_default_systems();

        let root = WidgetBuilder::new(&mut app.world)
            .bg_color(Color::rgb(bg.0, bg.1, bg.2))
            .layout(LayoutStyle {
                width: Dimension::px(width as i32),
                height: Dimension::px(height as i32),
                ..Default::default()
            })
            .id();

        ui! {
            :(
                parent: root
                world: &mut app.world
            :)

            Row (width: 200, height: 40) {
                View (grow: 1.0, bg_color: Color::rgb(255, 0, 0)) {}
                View (grow: 1.0, bg_color: Color::rgb(0, 255, 0)) {}
            }
        };

        app.set_root(root);

        let world = &mut app.world;
        let viewport = Viewport::new(width, height, Fixed::ONE);
        render_system::update_layout(world, root, &viewport);
        {
            let tex = app.backend.framebuffer();
            let mut renderer = SwRenderer::new(tex);
            renderer.viewport = viewport;
            render_system::render(world, root, &viewport, &mut renderer);
        }

        let tex = app.backend.framebuffer();
        let pixels = tex.buf.as_slice();
        let stride = tex.stride;

        let mid_y = (height / 2) as usize;
        let left_q = (width / 4) as usize;
        let right_q = ((width as usize) * 3) / 4;
        let li = mid_y * stride + left_q * 4;
        let ri = mid_y * stride + right_q * 4;
        let left = (pixels[li], pixels[li + 1], pixels[li + 2]);
        let right = (pixels[ri], pixels[ri + 1], pixels[ri + 2]);

        assert_eq!(
            left,
            (255, 0, 0),
            "Row left half should be red, got {left:?}"
        );
        assert_eq!(
            right,
            (0, 255, 0),
            "Row right half should be green, got {right:?}"
        );
    }

    #[test]
    fn column_widget_lays_children_vertically() {
        let width: u16 = 40;
        let height: u16 = 200;
        let bg = (0, 0, 0);

        let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
        let mut app: App<_, _> = App::new(backend);
        app.with_default_widgets().with_default_systems();

        let root = WidgetBuilder::new(&mut app.world)
            .bg_color(Color::rgb(bg.0, bg.1, bg.2))
            .layout(LayoutStyle {
                width: Dimension::px(width as i32),
                height: Dimension::px(height as i32),
                ..Default::default()
            })
            .id();

        ui! {
            :(
                parent: root
                world: &mut app.world
            :)

            Column (width: 40, height: 200) {
                View (grow: 1.0, bg_color: Color::rgb(255, 0, 0)) {}
                View (grow: 1.0, bg_color: Color::rgb(0, 255, 0)) {}
            }
        };

        app.set_root(root);

        let world = &mut app.world;
        let viewport = Viewport::new(width, height, Fixed::ONE);
        render_system::update_layout(world, root, &viewport);
        {
            let tex = app.backend.framebuffer();
            let mut renderer = SwRenderer::new(tex);
            renderer.viewport = viewport;
            render_system::render(world, root, &viewport, &mut renderer);
        }

        let tex = app.backend.framebuffer();
        let pixels = tex.buf.as_slice();
        let stride = tex.stride;

        let mid_x = (width / 2) as usize;
        let top_q = (height / 4) as usize;
        let bot_q = ((height as usize) * 3) / 4;
        let ti = top_q * stride + mid_x * 4;
        let bi = bot_q * stride + mid_x * 4;
        let top = (pixels[ti], pixels[ti + 1], pixels[ti + 2]);
        let bot = (pixels[bi], pixels[bi + 1], pixels[bi + 2]);

        assert_eq!(top, (255, 0, 0), "Column top should be red, got {top:?}");
        assert_eq!(
            bot,
            (0, 255, 0),
            "Column bottom should be green, got {bot:?}"
        );
    }
}
