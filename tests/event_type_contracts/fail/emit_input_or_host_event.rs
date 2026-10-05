use mirui::input::event::gesture::Tap;
use mirui::input::event::lifecycle::Started;
use mirui::types::Fixed;

#[mirui::event]
fn forge(node: entity!(), commands: commands!()) {
    commands.emit(
        node,
        Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: node,
        },
    );
    commands.emit(node, Started);
}

fn main() {}
