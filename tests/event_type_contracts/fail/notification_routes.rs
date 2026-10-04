use mirui::__macro_support::NotificationQueue;
use mirui::ecs::Entity;
use mirui::input::event::gesture::Tap;
use mirui::input::event::lifecycle::Started;
use mirui::types::Fixed;
use mirui::ui::widgets::slider;

fn main() {
    let queue = NotificationQueue::default();
    let entity = Entity {
        id: 1,
        generation: 0,
    };
    let _ = queue.push_app(slider::ValueChanged {
        new: Fixed::ONE,
        old: Fixed::ZERO,
    });
    let _ = queue.push_entity(entity, Started);
    let _ = queue.push_entity(
        entity,
        Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: entity,
        },
    );
}
