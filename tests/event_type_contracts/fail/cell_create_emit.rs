use mirui::prelude::*;
use mirui::ui::widgets::lazy_list::CreateCell;

#[event]
fn forge_binding(event: event!(CreateCell), node: entity!(), commands: commands!()) -> Entity {
    commands.emit(
        node,
        CreateCell {
            item: event.item.clone(),
        },
    );
    node
}

fn main() {}
