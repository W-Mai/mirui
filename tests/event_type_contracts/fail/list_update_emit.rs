use mirui::prelude::*;
use mirui::ui::widgets::lazy_list::BindItem;

#[event]
fn forge_binding(node: entity!(), commands: commands!()) {
    commands.emit(
        node,
        BindItem {
            item: node,
            index: 0,
        },
    );
}

fn main() {}
