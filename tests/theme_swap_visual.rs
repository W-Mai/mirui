extern crate alloc;

#[cfg(test)]
mod tests {
    use mirui::components::{
        Button, Checkbox, ProgressBar, Slider, Switch, TabBar, Text, TextInput,
    };
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

    #[test]
    fn theme_swap_chooser_paints_buttons() {
        let width: u16 = 480;
        let height: u16 = 320;
        let bg = (20, 22, 30);

        let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
        let mut app: App<_, _> = App::new(backend);
        app.with_default_widgets().with_default_systems();

        let root = WidgetBuilder::new(&mut app.world)
            .bg_color(Color::rgb(bg.0, bg.1, bg.2))
            .layout(LayoutStyle {
                direction: FlexDirection::Column,
                width: Dimension::px(width as i32),
                height: Dimension::px(height as i32),
                padding: Padding::all(20),
                ..Default::default()
            })
            .id();

        let _chooser = ui! {
            :(
                parent: root
                world: &mut app.world
            :)

            Row (height: 44) {
                Button (
                    grow: 1.0, height: 36, border_radius: 6,
                    text_color: ColorToken::OnPrimary,
                    normal_color: Color::rgb(40, 50, 70),
                    pressed_color: Color::rgb(20, 25, 35)
                ) [
                    GestureHandler { on_gesture: dummy_handler },
                ] {
                    Text("Dark") {}
                }
                Button (
                    grow: 1.0, height: 36, border_radius: 6,
                    text_color: ColorToken::OnPrimary,
                    normal_color: Color::rgb(0, 100, 200),
                    pressed_color: Color::rgb(0, 70, 150)
                ) [
                    GestureHandler { on_gesture: dummy_handler },
                ] {
                    Text("Light") {}
                }
                Button (
                    grow: 1.0, height: 36, border_radius: 6,
                    text_color: ColorToken::OnPrimary,
                    normal_color: Color::rgb(255, 105, 180),
                    pressed_color: Color::rgb(200, 70, 140)
                ) [
                    GestureHandler { on_gesture: dummy_handler },
                ] {
                    Text("Custom") {}
                }
            }
        };

        ui! {
            :(
                parent: root
                world: &mut app.world
            :)

            Column (grow: 1.0) {
                Row (height: 28, align: AlignItems::Center) {
                    View (width: 90) { Text("Slider") {} }
                    Slider (
                        min: Fixed::ZERO,
                        max: Fixed::from_int(100),
                        grow: 1.0,
                        height: 20
                    ) {}
                }
                Row (height: 36, align: AlignItems::Center) {
                    View (width: 90) { Text("Switch") {} }
                    Switch (width: 56, height: 28) {}
                }
                Row (height: 36, align: AlignItems::Center) {
                    View (width: 90) { Text("Checkbox") {} }
                    Checkbox (width: 24, height: 24, border_radius: 4) {}
                }
                Row (height: 28, align: AlignItems::Center) {
                    View (width: 90) { Text("Progress") {} }
                    ProgressBar (grow: 1.0, height: 12, border_radius: 6) {}
                }
                Row (height: 36, align: AlignItems::Center) {
                    View (width: 90) { Text("Input") {} }
                    TextInput (grow: 1.0, height: 28) {}
                }
                Row (height: 24, align: AlignItems::Center) {
                    View (width: 90) { Text("Tabs") {} }
                    TabBar (count: 3, grow: 1.0, height: 24) {
                        View (grow: 1.0) {}
                        View (grow: 1.0) {}
                        View (grow: 1.0) {}
                    }
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
                if r >= 35 && r <= 45 && g >= 45 && g <= 55 && b >= 65 && b <= 75 {
                    found_dark_btn = true;
                }
                if r <= 10 && g >= 95 && g <= 105 && b >= 195 && b <= 205 {
                    found_light_btn = true;
                }
                if r >= 250 && g >= 100 && g <= 110 && b >= 175 && b <= 185 {
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
