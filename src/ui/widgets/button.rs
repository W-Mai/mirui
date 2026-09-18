use crate::ecs::{Entity, World};
use crate::render::command::DrawCommand;
use crate::render::renderer::Renderer;
use crate::types::{Dimension, Rect};
use crate::ui::layout::{AlignItems, JustifyContent, Padding};
use crate::ui::theme::{ColorToken, ThemedColor};
use crate::ui::view::{View, ViewCtx};
use crate::ui::{HitTarget, InteractionFeedback, Style};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ButtonSize {
    /// Tight metrics for constrained and embedded interfaces.
    Compact,
    /// Default control metrics for general interfaces.
    #[default]
    Regular,
    /// Leaves padding, minimum height, and content alignment to the caller.
    Custom,
}

#[derive(Clone, Copy)]
struct ButtonMetrics {
    padding: Padding,
    min_height: Dimension,
}

impl ButtonSize {
    const fn metrics(self) -> Option<ButtonMetrics> {
        match self {
            Self::Compact => Some(ButtonMetrics {
                padding: Padding {
                    top: Dimension::px(2),
                    right: Dimension::px(4),
                    bottom: Dimension::px(2),
                    left: Dimension::px(4),
                },
                min_height: Dimension::px(20),
            }),
            Self::Regular => Some(ButtonMetrics {
                padding: Padding {
                    top: Dimension::px(4),
                    right: Dimension::px(8),
                    bottom: Dimension::px(4),
                    left: Dimension::px(8),
                },
                min_height: Dimension::px(28),
            }),
            Self::Custom => None,
        }
    }
}

#[derive(crate::Component)]
pub struct Button {
    pub size: ButtonSize,
    pub normal_color: ThemedColor,
    pub pressed_color: ThemedColor,
}

impl Default for Button {
    fn default() -> Self {
        Self {
            size: ButtonSize::Regular,
            normal_color: ThemedColor::Token(ColorToken::SurfaceVariant),
            pressed_color: ThemedColor::Token(ColorToken::Primary),
        }
    }
}

impl Button {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_normal_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.normal_color = color.into();
        self
    }

    pub const fn with_size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    pub fn with_pressed_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.pressed_color = color.into();
        self
    }

    pub fn build() -> ButtonBuilder {
        ButtonBuilder {
            button: Button::new(),
            style: None,
        }
    }
}

pub struct ButtonBuilder {
    button: Button,
    style: Option<crate::ui::Style>,
}

impl ButtonBuilder {
    pub fn style(mut self, style: crate::ui::Style) -> Self {
        self.style = Some(style);
        self
    }

    pub fn normal_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.button.normal_color = color.into();
        self
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.button.size = size;
        self
    }

    pub fn pressed_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.button.pressed_color = color.into();
        self
    }

    pub fn spawn(self, world: &mut World) -> Entity {
        world.spawn(self)
    }
}

impl crate::ecs::IntoBundle for ButtonBuilder {
    fn spawn_into(self, world: &mut World, entity: Entity) {
        world.insert(entity, self.button);
        if let Some(style) = self.style {
            world.insert(entity, style);
        }
    }
}

fn button_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(btn) = world.get::<Button>(entity) else {
        return;
    };
    let theme = ctx.theme(world);
    let color = if matches!(ctx.state, crate::ui::theme::WidgetState::Pressed) {
        btn.pressed_color.resolve_in(theme, ctx.state)
    } else {
        btn.normal_color.resolve_in(theme, ctx.state)
    };
    ctx.draw(
        renderer,
        &DrawCommand::Fill {
            area: *rect,
            transform: ctx.transform,
            quad: ctx.quad,
            color,
            radius: ctx.style.border_radius,
            opa: 255,
        },
        ctx.clip,
    );
    ctx.bg_handled = true;
}

fn button_attach(world: &mut World, entity: Entity) {
    let Some(button) = world.get::<Button>(entity) else {
        return;
    };
    let Some(metrics) = button.size.metrics() else {
        world.insert(entity, HitTarget);
        world.insert(entity, InteractionFeedback);
        return;
    };
    if let Some(style) = world.get_mut::<Style>(entity) {
        if style.layout.padding == Padding::default() {
            style.layout.padding = metrics.padding;
        }
        if style.layout.min_height == Dimension::Auto && style.layout.height == Dimension::Auto {
            style.layout.min_height = metrics.min_height;
        }
        if style.layout.justify == JustifyContent::FlexStart {
            style.layout.justify = JustifyContent::Center;
        }
        if style.layout.align == AlignItems::FlexStart {
            style.layout.align = AlignItems::Center;
        }
    }
    world.insert(entity, HitTarget);
    world.insert(entity, InteractionFeedback);
}

pub fn view() -> View {
    View::new("Button", 40, button_render)
        .with_filter::<Button>()
        .with_attach(button_attach)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styled_button(world: &mut World, button: Button, style: Style) -> Entity {
        let entity = world.spawn_empty();
        world.insert(entity, button);
        world.insert(entity, style);
        button_attach(world, entity);
        entity
    }

    #[test]
    fn build_spawns_button_with_style() {
        let mut world = World::new();
        let e = Button::build()
            .style(crate::ui::Style::default())
            .spawn(&mut world);
        assert!(world.has::<Button>(e));
        assert!(world.has::<crate::ui::Style>(e));
        assert!(world.has::<crate::ui::Widget>(e));
    }

    #[test]
    fn build_without_style_omits_it() {
        let mut world = World::new();
        let e = Button::build().spawn(&mut world);
        assert!(world.has::<Button>(e));
        assert!(!world.has::<crate::ui::Style>(e));
    }

    #[test]
    fn attach_ignores_non_button_entities() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, Style::default());

        button_attach(&mut world, entity);

        assert!(!world.has::<HitTarget>(entity));
        assert!(!world.has::<InteractionFeedback>(entity));
    }

    #[test]
    fn attach_applies_regular_control_metrics_and_hit_target() {
        let mut world = World::new();
        let entity = styled_button(&mut world, Button::new(), Style::default());
        let style = world.get::<Style>(entity).unwrap();
        assert_eq!(style.layout.min_height, Dimension::px(28));
        assert_eq!(
            style.layout.padding,
            Padding {
                top: Dimension::px(4),
                right: Dimension::px(8),
                bottom: Dimension::px(4),
                left: Dimension::px(8),
            }
        );
        assert_eq!(style.layout.justify, JustifyContent::Center);
        assert_eq!(style.layout.align, AlignItems::Center);
        assert!(world.has::<HitTarget>(entity));
        assert!(world.has::<InteractionFeedback>(entity));
    }

    #[test]
    fn compact_metrics_preserve_explicit_layout_values() {
        let mut world = World::new();
        let explicit_padding = Padding::all(2);
        let entity = styled_button(
            &mut world,
            Button::new().with_size(ButtonSize::Compact),
            Style {
                layout: crate::ui::layout::LayoutStyle {
                    min_height: Dimension::px(44),
                    padding: explicit_padding,
                    ..crate::ui::layout::LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        let style = world.get::<Style>(entity).unwrap();
        assert_eq!(style.layout.min_height, Dimension::px(44));
        assert_eq!(style.layout.padding, explicit_padding);
    }

    #[test]
    fn compact_metrics_fill_unspecified_layout_values() {
        let mut world = World::new();
        let entity = styled_button(
            &mut world,
            Button::new().with_size(ButtonSize::Compact),
            Style::default(),
        );
        let style = world.get::<Style>(entity).unwrap();
        assert_eq!(style.layout.min_height, Dimension::px(20));
        assert_eq!(style.layout.padding.top, Dimension::px(2));
        assert_eq!(style.layout.padding.right, Dimension::px(4));
    }

    #[test]
    fn regular_metrics_center_text_content() {
        let mut app = crate::app::App::headless(100, 40);
        app.with_default_widgets();
        let button = app.world.spawn_empty();
        app.world.insert(button, crate::ui::Widget);
        app.world.insert(button, Button::new());
        app.world.insert(
            button,
            Style {
                layout: crate::ui::layout::LayoutStyle {
                    width: Dimension::px(100),
                    height: Dimension::px(40),
                    ..crate::ui::layout::LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        button_attach(&mut app.world, button);

        let label = app.world.spawn_empty();
        app.world.insert(label, crate::ui::Widget);
        app.world.insert(label, crate::ui::Parent(button));
        app.world
            .insert(label, crate::ui::widgets::Text::label("OK"));
        app.world.insert(
            label,
            Style {
                layout: crate::ui::layout::LayoutStyle {
                    width: Dimension::px(10),
                    height: Dimension::px(10),
                    ..crate::ui::layout::LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        app.world
            .insert(button, crate::ui::Children(alloc::vec![label]));

        crate::ui::render_system::update_layout(
            &mut app.world,
            button,
            &crate::types::Viewport::new(100, 40, crate::types::Fixed::ONE),
        );
        let rect = app.world.get::<crate::ui::ComputedRect>(label).unwrap().0;
        assert_eq!(rect.x, crate::types::Fixed::from_int(45));
        assert_eq!(rect.y, crate::types::Fixed::from_int(15));
    }

    #[test]
    fn explicit_twenty_pixel_height_keeps_label_inside_button() {
        let mut app = crate::app::App::headless(80, 20);
        app.with_default_widgets();
        let button = app.world.spawn_empty();
        app.world.insert(button, crate::ui::Widget);
        app.world.insert(button, Button::new());
        app.world.insert(
            button,
            Style {
                layout: crate::ui::layout::LayoutStyle {
                    width: Dimension::px(80),
                    height: Dimension::px(20),
                    ..crate::ui::layout::LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        button_attach(&mut app.world, button);

        let label = app.world.spawn_empty();
        app.world.insert(label, crate::ui::Widget);
        app.world.insert(label, crate::ui::Parent(button));
        app.world
            .insert(label, crate::ui::widgets::Text::label("OK"));
        app.world.insert(
            label,
            Style {
                layout: crate::ui::layout::LayoutStyle {
                    width: Dimension::px(10),
                    height: Dimension::px(10),
                    ..crate::ui::layout::LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        app.world
            .insert(button, crate::ui::Children(alloc::vec![label]));

        crate::ui::render_system::update_layout(
            &mut app.world,
            button,
            &crate::types::Viewport::new(80, 20, crate::types::Fixed::ONE),
        );
        let rect = app.world.get::<crate::ui::ComputedRect>(label).unwrap().0;
        assert_eq!(rect.y, crate::types::Fixed::from_int(5));
        assert!(rect.y >= crate::types::Fixed::ZERO);
        assert!(rect.y + rect.h <= crate::types::Fixed::from_int(20));
    }

    #[test]
    fn custom_size_leaves_default_layout_untouched() {
        let mut world = World::new();
        let entity = styled_button(
            &mut world,
            Button::new().with_size(ButtonSize::Custom),
            Style::default(),
        );
        let layout = world.get::<Style>(entity).unwrap().layout;
        assert_eq!(layout.padding, Padding::default());
        assert_eq!(layout.min_height, Dimension::Auto);
        assert_eq!(layout.justify, JustifyContent::FlexStart);
        assert_eq!(layout.align, AlignItems::FlexStart);
        assert!(world.has::<HitTarget>(entity));
        assert!(world.has::<InteractionFeedback>(entity));
    }
}
