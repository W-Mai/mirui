use crate::prelude::*;
use crate::types::Transform;
use crate::ui::dirty::Dirty;
use crate::ui::widgets::WidgetTransform;

#[crate::component]
pub struct Spinner {
    pub angle: Fixed,
    pub speed: Fixed,
}

//~focus-start
#[mirui_macros::system(order = Animation)]
pub fn spin_system(spinners: query!(Entity, mut Spinner), commands: commands!()) {
    for (entity, spinner) in spinners {
        spinner.angle += spinner.speed;
        commands.insert(
            entity,
            WidgetTransform(Transform::rotate_deg(spinner.angle)),
        );
        commands.insert(entity, Dirty);
    }
}
//~focus-end
