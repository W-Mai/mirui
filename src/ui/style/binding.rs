use core::{cell::Cell, marker::PhantomData};

use crate::core::reactive::{BindingRole, widget_binding::prepare_widget_effect};
use crate::ecs::world::ComponentChangeKind;
use crate::ecs::{Entity, World};
use crate::ui::property::{PropertyChange, invalidate_for_change};
use crate::ui::recipe::PropertyBindings;

use super::{Button, Dimension, FlexDirection, Style, StyleField, StyleInput, metrics_changed};

#[doc(hidden)]
#[derive(Clone, Copy, Default)]
pub struct StyleDefaults {
    pub direction: FlexDirection,
    pub clip_children: bool,
}

pub(crate) struct StyleBinding<W, F> {
    read: F,
    explicit: u64,
    widget: PhantomData<fn() -> W>,
}

impl<W, F> StyleBinding<W, F> {
    pub(crate) fn new(read: F) -> Self {
        Self {
            read,
            explicit: 0,
            widget: PhantomData,
        }
    }
}

impl<W: 'static, S: StyleInput<W>, F: Fn() -> S + 'static> PropertyBindings for StyleBinding<W, F> {
    const HAS_DYNAMIC_STYLE: bool = true;

    fn install(self, world: &mut World, entity: Entity) {
        prepare_style_binding::<W, S>(entity, self.explicit, StyleDefaults::default(), self.read)(
            world, false,
        );
    }

    fn set_explicit_style(&mut self, fields: u64) {
        self.explicit = fields;
    }
}

#[doc(hidden)]
pub fn prepare_style_binding<W: 'static, S: StyleInput<W>>(
    entity: Entity,
    explicit: u64,
    defaults: StyleDefaults,
    read: impl Fn() -> S + 'static,
) -> impl FnOnce(&mut World, bool) {
    let applied = Cell::new(0);
    let effect = prepare_widget_effect(entity, BindingRole::Display, move || {
        let style = read();
        crate::core::reactive::with_world(|world| {
            if world.is_alive(entity) && world.has::<crate::ui::Widget>(entity) {
                let (previous, explicit) = super::classes::release_inline_fields::<W, _>(
                    world,
                    entity,
                    &style,
                    explicit,
                    applied.get(),
                );
                applied.set(reconcile_style::<W, S>(
                    world, entity, style, explicit, defaults, previous,
                ));
            }
        });
    });
    move |world, once| {
        if once {
            effect.install_once(world);
        } else {
            effect.install(world);
        }
    }
}

pub(super) fn reconcile_style<W: 'static, S: StyleInput<W>>(
    world: &mut World,
    entity: Entity,
    mut style: S,
    explicit: u64,
    base: StyleDefaults,
    previous: u64,
) -> u64 {
    let declared = style.declared() & StyleField::COMMON & !explicit;
    let widget_fields = style.update_widget(world, entity, explicit, previous);
    let mut fields = style.fields();
    let removed = previous & StyleField::COMMON & !declared & !explicit;
    let affected = declared | removed;
    if affected == 0 {
        return widget_fields;
    }

    let Some(current) = world.get::<Style>(entity) else {
        return widget_fields;
    };
    let mut defaults = Style::default();
    defaults.layout.direction = base.direction;
    defaults.clip_children = base.clip_children;

    let height = StyleField::height.bit();
    let minimum = StyleField::min_height.bit();
    const BUTTON_LAYOUT: u64 = StyleField::height.bit()
        | StyleField::min_height.bit()
        | StyleField::padding.bit()
        | StyleField::justify.bit()
        | StyleField::align.bit();
    if affected & BUTTON_LAYOUT != 0
        && let Some(button) = world.get::<Button>(entity)
    {
        let authored = explicit | declared;
        let next_height = if removed & height != 0 {
            Dimension::Auto
        } else {
            fields.height.unwrap_or(current.layout.height)
        };
        defaults.layout.height = next_height;
        button
            .size
            .apply_layout_defaults(&mut defaults.layout, authored);
        defaults.layout.height = Dimension::Auto;
        if affected & height != 0 && authored & minimum == 0 {
            fields.min_height = Some(defaults.layout.min_height);
        }
    }
    fields.restore(removed, defaults);
    if let Some(change) = fields.change(current, explicit) {
        let layout = match change {
            ComponentChangeKind::InheritedStyle(fields) => {
                metrics_changed(fields) || fields & ComponentChangeKind::LOCAL_LAYOUT != 0
            }
            _ => matches!(change, ComponentChangeKind::Layout),
        };
        if let Some(current) = world.get_mut_with_change::<Style>(entity, change) {
            fields.apply(current, explicit);
            invalidate_for_change(
                world,
                entity,
                if layout {
                    PropertyChange::Layout
                } else {
                    PropertyChange::Visual
                },
            );
        }
    }
    declared | widget_fields
}
