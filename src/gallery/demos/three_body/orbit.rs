use crate::prelude::{Dimension, Fixed, Style, World};

#[crate::component]
pub struct OrbitRing {
    pub diameter_percent: i32,
}

impl OrbitRing {
    pub(super) fn sync_all(world: &mut World, width: i32, height: i32) {
        world.for_each_stable::<Self>(|world, entity| {
            let Some(percent) = world.get::<Self>(entity).map(|ring| ring.diameter_percent) else {
                return;
            };
            let size = width.min(height) * percent / 100;
            if let Some(style) = world.get_mut::<Style>(entity) {
                style.layout.left = Dimension::px((width - size) / 2);
                style.layout.top = Dimension::px((height - size) / 2);
                style.layout.width = Dimension::px(size);
                style.layout.height = Dimension::px(size);
                style.border_radius = Fixed::from_int(size / 2);
            }
        });
    }
}
