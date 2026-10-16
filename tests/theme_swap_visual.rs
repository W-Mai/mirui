extern crate alloc;

#[cfg(test)]
mod tests {
    use mirui::prelude::*;
    use mirui::render::sw::SwRenderer;
    use mirui::render::texture::ColorFormat;
    use mirui::surface::FramebufferAccess;
    use mirui::surface::framebuf::FramebufSurface;
    use mirui::types::Viewport;
    use mirui::ui::layout::FlexDirection;
    use mirui::ui::render_system;
    use mirui::ui::widgets::{
        Button, Checkbox, ProgressBar, Slider, Switch, TabBar, Text, TextInput,
    };

    #[test]
    fn theme_swap_chooser_paints_buttons() {
        let width: u16 = 480;
        let height: u16 = 320;
        let bg = (20, 22, 30);

        let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
        let mut app: App<_, _> = App::new(backend);
        app.with_default_widgets().with_default_systems();

        let root = app
            .spawn_root()
            .bg_color(Color::rgb(bg.0, bg.1, bg.2))
            .layout(LayoutStyle {
                direction: FlexDirection::Column,
                width: Dimension::px(width as i32),
                height: Dimension::px(height as i32),
                padding: Padding::all(20),
                ..Default::default()
            })
            .id();

        app.compose(root, |cx| {
            ui! {
                Row (height: 44) {
                    Button (
                        grow: 1.0,
                        height: 36,
                        border_radius: 6,
                        text_color: ColorToken::OnPrimary,
                        normal_color: Color::rgb(40, 50, 70),
                        pressed_color: Color::rgb(20, 25, 35)
                    ) {
                        Text ("Dark")
                    } on Tap(|_| false)
                    Button (
                        grow: 1.0,
                        height: 36,
                        border_radius: 6,
                        text_color: ColorToken::OnPrimary,
                        normal_color: Color::rgb(0, 100, 200),
                        pressed_color: Color::rgb(0, 70, 150)
                    ) {
                        Text ("Light")
                    } on Tap(|_| false)
                    Button (
                        grow: 1.0,
                        height: 36,
                        border_radius: 6,
                        text_color: ColorToken::OnPrimary,
                        normal_color: Color::rgb(255, 105, 180),
                        pressed_color: Color::rgb(200, 70, 140)
                    ) {
                        Text ("Custom")
                    } on Tap(|_| false)
                }
            }
        });

        app.compose(root, |cx| {
            ui! {
                Column (grow: 1.0) {
                    Row (height: 28, align: AlignItems::Center) {
                        View (width: 90) {
                            Text ("Slider")
                        }
                        Slider (
                            min: Fixed::ZERO,
                            max: Fixed::from_int(100),
                            grow: 1.0,
                            height: 20
                        )
                    }
                    Row (height: 36, align: AlignItems::Center) {
                        View (width: 90) {
                            Text ("Switch")
                        }
                        Switch (width: 56, height: 28)
                    }
                    Row (height: 36, align: AlignItems::Center) {
                        View (width: 90) {
                            Text ("Checkbox")
                        }
                        Checkbox (width: 24, height: 24, border_radius: 4)
                    }
                    Row (height: 28, align: AlignItems::Center) {
                        View (width: 90) {
                            Text ("Progress")
                        }
                        ProgressBar (grow: 1.0, height: 12, border_radius: 6)
                    }
                    Row (height: 36, align: AlignItems::Center) {
                        View (width: 90) {
                            Text ("Input")
                        }
                        TextInput (grow: 1.0, height: 28)
                    }
                    Row (height: 24, align: AlignItems::Center) {
                        View (width: 90) {
                            Text ("Tabs")
                        }
                        TabBar (count: 3, grow: 1.0, height: 24) {
                            View (grow: 1.0)
                            View (grow: 1.0)
                            View (grow: 1.0)
                        }
                    }
                }
            }
        });

        let world = &mut app.world;
        let viewport = Viewport::new(width, height, Fixed::ONE);
        render_system::try_update_layout(world, root, &viewport).unwrap();
        {
            let tex = app.backend.framebuffer();
            let mut renderer = SwRenderer::new(tex);
            renderer.viewport = viewport;
            render_system::render(world, root, &viewport, &mut renderer).unwrap();
        }

        let tex = app.backend.framebuffer();
        let pixel_view = tex.read_pixels().expect("read pixels");
        let pixels = pixel_view.bytes();
        let stride = pixel_view.stride();

        let mut non_bg_count = 0usize;
        let mut found_dark_btn = false;
        let mut found_light_btn = false;
        let mut found_custom_btn = false;
        for y in 0..(height as usize) {
            for x in 0..(width as usize) {
                let i = y * stride + x * 4;
                let (r, g, b) = (pixels[i], pixels[i + 1], pixels[i + 2]);
                if r != bg.0 || g != bg.1 || b != bg.2 {
                    non_bg_count += 1;
                }
                if (35..=45).contains(&r) && (45..=55).contains(&g) && (65..=75).contains(&b) {
                    found_dark_btn = true;
                }
                if r <= 10 && (95..=105).contains(&g) && (195..=205).contains(&b) {
                    found_light_btn = true;
                }
                if r >= 250 && (100..=110).contains(&g) && (175..=185).contains(&b) {
                    found_custom_btn = true;
                }
            }
        }

        assert!(
            non_bg_count > 1000,
            "should paint plenty of non-bg pixels, got {non_bg_count}"
        );
        assert!(
            found_dark_btn,
            "Dark button colour (40,50,70) should appear"
        );
        assert!(
            found_light_btn,
            "Light button colour (0,100,200) should appear"
        );
        assert!(
            found_custom_btn,
            "Custom button colour (255,105,180) should appear"
        );
    }
}
