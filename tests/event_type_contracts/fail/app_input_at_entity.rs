use mirui::input::event::contract::*;
use mirui::prelude::{Rotary, Wheel};

#[mirui::component]
struct Node;

fn entity<E: Event>(_: EventKey<E>)
where
    E::Route: EntityRoute,
    E::Source: EntitySourceFor<Node>,
{
}

fn main() {
    entity(Wheel::EVENT);
    entity(Rotary::EVENT);
}
