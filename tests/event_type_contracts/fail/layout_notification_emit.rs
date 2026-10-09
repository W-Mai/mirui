use mirui::prelude::*;

#[event]
fn forge_size(node: entity!(), commands: commands!()) {
    commands.emit(
        node,
        SizeChanged {
            old: None,
            new: LayoutSize {
                width: Fixed::ONE,
                height: Fixed::ONE,
            },
        },
    );
}

fn main() {}
