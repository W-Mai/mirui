#[cfg(test)]
mod tests {
    use mirui::render::{DrawCommand, Renderer};
    use mirui::types::{Color, Dimension, Fixed, Rect, Viewport};
    use mirui::ui::builder::WidgetBuilder;
    use mirui::ui::layout::*;
    use mirui::ui::render_system;

    struct RecordingRenderer {
        commands: Vec<(Rect, Color)>,
    }

    impl RecordingRenderer {
        fn new() -> Self {
            Self {
                commands: Vec::new(),
            }
        }
    }

    impl Renderer for RecordingRenderer {
        fn draw(&mut self, cmd: &DrawCommand, _clip: &Rect) {
            if let DrawCommand::Fill { area, color, .. } = cmd {
                self.commands.push((*area, *color));
            }
        }
        fn flush(&mut self) {}
    }

    #[test]
    fn render_system_produces_draw_commands() {
        let mut app = mirui::app::App::headless(64, 64);
        app.with_default_widgets();
        let mut world = app.world;

        let child = WidgetBuilder::new(&mut world)
            .bg_color(Color::rgb(255, 0, 0))
            .layout(LayoutStyle {
                width: Dimension::px(50),
                height: Dimension::px(50),
                ..Default::default()
            })
            .id();

        let root = WidgetBuilder::new(&mut world)
            .bg_color(Color::rgb(0, 0, 0))
            .layout(LayoutStyle {
                direction: FlexDirection::Row,
                width: Dimension::px(200),
                height: Dimension::px(100),
                ..Default::default()
            })
            .child(child)
            .id();

        let mut recorder = RecordingRenderer::new();
        let transform = Viewport::new(200, 100, Fixed::ONE);
        render_system::render(&world, root, &transform, &mut recorder);

        // Should have 2 draw commands: root bg + child bg
        assert_eq!(recorder.commands.len(), 2);

        // Root fills entire area
        assert_eq!(
            recorder.commands[0].0,
            Rect {
                x: Fixed::from_int(0),
                y: Fixed::from_int(0),
                w: Fixed::from_int(200),
                h: Fixed::from_int(100)
            }
        );
        assert_eq!(recorder.commands[0].1, Color::rgb(0, 0, 0));

        // Child at (0,0) with 50x50
        assert_eq!(
            recorder.commands[1].0,
            Rect {
                x: Fixed::from_int(0),
                y: Fixed::from_int(0),
                w: Fixed::from_int(50),
                h: Fixed::from_int(50)
            }
        );
        assert_eq!(recorder.commands[1].1, Color::rgb(255, 0, 0));
    }

    #[test]
    fn render_system_respects_layout() {
        let mut app = mirui::app::App::headless(64, 64);
        app.with_default_widgets();
        let mut world = app.world;

        let c1 = WidgetBuilder::new(&mut world)
            .bg_color(Color::rgb(255, 0, 0))
            .layout(LayoutStyle {
                grow: Fixed::from_f32(1.0),
                height: Dimension::px(100),
                ..Default::default()
            })
            .id();

        let c2 = WidgetBuilder::new(&mut world)
            .bg_color(Color::rgb(0, 255, 0))
            .layout(LayoutStyle {
                grow: Fixed::from_f32(1.0),
                height: Dimension::px(100),
                ..Default::default()
            })
            .id();

        let root = WidgetBuilder::new(&mut world)
            .layout(LayoutStyle {
                direction: FlexDirection::Row,
                width: Dimension::px(200),
                height: Dimension::px(100),
                ..Default::default()
            })
            .child(c1)
            .child(c2)
            .id();

        let mut recorder = RecordingRenderer::new();
        let transform = Viewport::new(200, 100, Fixed::ONE);
        render_system::render(&world, root, &transform, &mut recorder);

        // Root has no bg_color, so only 2 commands for children
        assert_eq!(recorder.commands.len(), 2);
        // Each child gets half: 100px wide
        assert_eq!(recorder.commands[0].0.w, Fixed::from_int(100));
        assert_eq!(recorder.commands[1].0.w, Fixed::from_int(100));
        assert_eq!(recorder.commands[1].0.x, Fixed::from_int(100));
    }
}
