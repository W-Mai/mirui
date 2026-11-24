use crate::ecs::{Entity, World};
use crate::render::command::DrawCommand;
use crate::types::Dimension;
use crate::ui::layout::{AlignItems, JustifyContent, LayoutStyle, Padding};
use crate::ui::style::{ExplicitButtonLayout, StyleField, StyleInput, mark_button_layout};
use crate::ui::theme::{ColorToken, ThemedColor};
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

    pub(crate) fn apply_layout_defaults(self, layout: &mut LayoutStyle, explicit: u64) {
        let Some(metrics) = self.metrics() else {
            return;
        };
        if explicit & StyleField::padding.bit() == 0 && layout.padding == Padding::default() {
            layout.padding = metrics.padding;
        }
        if explicit & StyleField::min_height.bit() == 0
            && layout.min_height == Dimension::Auto
            && layout.height == Dimension::Auto
        {
            layout.min_height = metrics.min_height;
        }
        if explicit & StyleField::justify.bit() == 0 && layout.justify == JustifyContent::FlexStart
        {
            layout.justify = JustifyContent::Center;
        }
        if explicit & StyleField::align.bit() == 0 && layout.align == AlignItems::FlexStart {
            layout.align = AlignItems::Center;
        }
    }
}

#[crate::component]
pub struct Button {
    pub size: ButtonSize,
    pub normal_color: ThemedColor,
    pub pressed_color: ThemedColor,
    /// Label colour while pressed. `None` keeps the label's own colour, so a
    /// light label on a light `pressed_color` should set one.
    pub pressed_text_color: Option<ThemedColor>,
}

impl Default for Button {
    fn default() -> Self {
        Self {
            size: ButtonSize::Regular,
            normal_color: ThemedColor::Token(ColorToken::SurfaceVariant),
            pressed_color: ThemedColor::Token(ColorToken::Primary),
            pressed_text_color: None,
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

    pub fn with_pressed_text_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.pressed_text_color = Some(color.into());
        self
    }

    pub fn build() -> ButtonBuilder {
        ButtonBuilder {
            button: Button::new(),
            style: None,
            layout_fields: 0,
            explicit: 0,
            style_binding: (),
            classes: (),
            properties: (),
            bindings: (),
        }
    }
}

pub struct ButtonBuilder<P = (), B = (), S = (), C = ()> {
    button: Button,
    style: Option<crate::ui::Style>,
    layout_fields: u64,
    explicit: u64,
    style_binding: S,
    classes: C,
    properties: P,
    bindings: B,
}

impl<P, B, R, C> ButtonBuilder<P, B, R, C> {
    pub fn style<S: StyleInput<Button>>(mut self, mut style: S) -> Self {
        if S::COMPLETE {
            self.layout_fields = 0;
            self.explicit |= StyleField::COMMON;
        }
        self.explicit |= style.apply_widget(&mut self.button, 0);
        let fields = style.apply_style(self.style.get_or_insert_default(), 0);
        self.layout_fields |= fields;
        self.explicit |= fields;
        self
    }

    /// Bind a style computed from reactive values.
    pub fn bind_style<S: StyleInput<Button>>(
        self,
        read: impl Fn() -> S + 'static,
    ) -> ButtonBuilder<P, B, impl crate::__macro_support::PropertyBindings, C> {
        ButtonBuilder {
            style_binding: crate::ui::style::StyleBinding::<Button, _>::new(read),
            button: self.button,
            style: self.style,
            layout_fields: self.layout_fields,
            explicit: self.explicit,
            classes: self.classes,
            properties: self.properties,
            bindings: self.bindings,
        }
    }

    /// Set the label's foreground colour.
    pub fn text_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.style.get_or_insert_default().set_text_color(color);
        self.explicit |= StyleField::text_color.bit();
        self
    }

    pub fn normal_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.button.normal_color = color.into();
        self.explicit |= StyleField::normal_color.bit();
        self
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.button.size = size;
        self
    }

    pub fn pressed_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.button.pressed_color = color.into();
        self.explicit |= StyleField::pressed_color.bit();
        self
    }

    pub fn pressed_text_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.button.pressed_text_color = Some(color.into());
        self.explicit |= StyleField::pressed_text_color.bit();
        self
    }
}

crate::ui::recipe::widget_builder_bindings!(
    ButtonBuilder,
    Button,
    [S, C],
    button,
    style,
    layout_fields,
    explicit,
    style_binding,
    classes
);

crate::ui::recipe::widget_builder_classes!(
    ButtonBuilder,
    button,
    style,
    layout_fields,
    explicit,
    style_binding,
    properties,
    bindings
);

impl<
    P: crate::__macro_support::PropertyBindings,
    B: crate::__macro_support::EntityEventBindings,
    S: crate::__macro_support::PropertyBindings,
    C: crate::__macro_support::PropertyBindings,
> crate::ui::recipe::WidgetRecipe for ButtonBuilder<P, B, S, C>
{
    type Properties = (C, S, P);
    type Events = B;

    fn insert(mut self, world: &mut World, entity: Entity) -> ((C, S, P), B) {
        world.insert(entity, self.button);
        if let Some(style) = self.style {
            world.insert(entity, style);
        }
        mark_button_layout(world, entity, self.layout_fields);
        if S::HAS_DYNAMIC_STYLE || C::HAS_CLASSES {
            let fields = self.explicit | self.properties.style_fields();
            self.style_binding.set_explicit_style(fields);
            self.classes.set_explicit_style(fields);
        }
        (
            (self.classes, self.style_binding, self.properties),
            self.bindings,
        )
    }
}

#[crate::view(
    component = Button,
    name = "Button",
    priority = 40,
    attach = button_attach::callback()
)]
fn button_render(component: component!(), paint: paint!(), theme: res!(crate::ui::Theme)) {
    let state = paint.state();
    let color = if matches!(state, crate::ui::theme::WidgetState::Pressed) {
        component.pressed_color.resolve_in(&theme, state)
    } else {
        component.normal_color.resolve_in(&theme, state)
    };
    paint.draw(&DrawCommand::Fill {
        area: paint.rect(),
        transform: paint.transform(),
        quad: paint.quad(),
        color,
        radius: paint.style().border_radius,
        opa: 255,
    });
    paint.set_background_handled(true);
}

#[crate::event]
fn button_attach(
    entity: entity!(),
    buttons: query!(Button),
    styles: query!(mut Style),
    explicit_layouts: query!(ExplicitButtonLayout),
    commands: commands!(),
) {
    let Some(button) = buttons.get(entity) else {
        return;
    };
    let explicit = explicit_layouts.get(entity).map_or(0, |layout| layout.0);
    if explicit != 0 {
        commands.remove::<ExplicitButtonLayout>(entity);
    }
    if button.size != ButtonSize::Custom
        && let Some(style) = styles.get(entity)
    {
        button
            .size
            .apply_layout_defaults(&mut style.layout, explicit);
    }
    commands.insert(entity, HitTarget);
    commands.insert(entity, InteractionFeedback);
}

pub use button_render::view;

#[cfg(test)]
mod tests {
    use super::*;

    fn styled_button(world: &mut World, button: Button, style: Style) -> Entity {
        let entity = world.spawn_empty();
        world.insert(entity, button);
        world.insert(entity, style);
        view().auto_attach().unwrap().invoke(world, entity);
        entity
    }

    #[test]
    fn build_spawns_button_with_style() {
        let mut world = World::new();
        let builder_parent = world.spawn_empty();
        let e = crate::ui::UiScope::new(&mut world, builder_parent)
            .spawn(Button::build().style(crate::ui::Style::default()));
        assert!(world.has::<Button>(e));
        assert!(world.has::<crate::ui::Style>(e));
        assert!(world.has::<crate::ui::Widget>(e));
    }

    #[test]
    fn build_without_style_initializes_default_style() {
        let mut world = World::new();
        let builder_parent = world.spawn_empty();
        let e = crate::ui::UiScope::new(&mut world, builder_parent).spawn(Button::build());
        assert!(world.has::<Button>(e));
        assert!(world.has::<crate::ui::Style>(e));
    }

    #[test]
    fn typed_view_keeps_button_registration_metadata() {
        let view = view();
        assert_eq!(view.name(), "Button");
        assert_eq!(view.priority(), Some(40));
        assert!(view.auto_attach().is_some());
    }

    #[test]
    fn attach_ignores_non_button_entities() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, Style::default());

        view().auto_attach().unwrap().invoke(&mut world, entity);

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
        view().auto_attach().unwrap().invoke(&mut app.world, button);

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

        crate::ui::render_system::try_update_layout(
            &mut app.world,
            button,
            &crate::types::Viewport::new(100, 40, crate::types::Fixed::ONE),
        )
        .unwrap();
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
        view().auto_attach().unwrap().invoke(&mut app.world, button);

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

        crate::ui::render_system::try_update_layout(
            &mut app.world,
            button,
            &crate::types::Viewport::new(80, 20, crate::types::Fixed::ONE),
        )
        .unwrap();
        let rect = app.world.get::<crate::ui::ComputedRect>(label).unwrap().0;
        assert_eq!(rect.y, crate::types::Fixed::from_int(5));
        assert!(rect.y >= crate::types::Fixed::ZERO);
        assert!(rect.y + rect.h <= crate::types::Fixed::from_int(20));
    }

    #[test]
    fn pressed_label_takes_the_pressed_text_color() {
        use crate::prelude::*;
        use crate::render::texture::ColorFormat;
        use crate::surface::FramebufferAccess;
        use crate::surface::framebuf::FramebufSurface;

        #[crate::compose]
        fn fixture() -> Entity {
            ui! {
                Button (
                    "OK",
                    id: "pressed_label",
                    width: 60,
                    height: 30,
                    font_size: 16,
                    normal_color: Color::rgb(0, 0, 0),
                    pressed_color: Color::rgb(250, 250, 250),
                    text_color: Color::rgb(250, 250, 250),
                    pressed_text_color: Color::rgb(200, 0, 0)
                )
            }
        }

        let ink = |pressed: bool| {
            let flush: fn(&[u8], crate::types::PhysicalRect) = |_, _| {};
            let backend = FramebufSurface::with_format(60, 30, ColorFormat::RGBA8888, flush);
            let mut app = App::new(backend);
            app.with_default_widgets().with_default_systems();
            let root = app.spawn_root().id();
            app.compose(root, fixture());
            app.set_root(root);
            if pressed {
                let button = app.world.find_by_id("pressed_label").unwrap();
                app.world
                    .insert(button, crate::ui::state::InteractionState::Pressed);
            }
            app.render().unwrap();
            app.backend
                .framebuffer()
                .read_pixels()
                .unwrap()
                .bytes()
                .chunks_exact(4)
                .filter(|p| p[0] > 120 && p[1] < 80 && p[2] < 80)
                .count()
        };
        assert_eq!(ink(false), 0, "the label keeps its own colour at rest");
        assert!(
            ink(true) > 10,
            "the pressed label is drawn in the pressed colour"
        );
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
