use mirui::input::event::contract::*;
use mirui::ui::widgets::{Checkbox, Switch};

fn checkbox<E: Event>(_: EventKey<E>)
where
    E::Route: EntityRoute,
    E::Source: EntitySourceFor<Checkbox>,
{
}

fn main() {
    checkbox(Switch::Toggled);
}
