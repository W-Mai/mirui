#![deny(warnings)]

use mirui::input::event::contract::*;
use mirui::prelude::{
    CharInput, DragCancel, DragEnd, DragMove, DragStart, HostInteraction, Key,
    LongPress, MemoryWarning, Pinch, Quit, Resumed, Rotary, Rotate, Started,
    Suspended, Tap, Wheel,
};

#[mirui::component]
struct Node;

fn entity<E: Event>(_: EventKey<E>)
where
    E::Route: EntityRoute,
    E::Source: EntitySourceFor<Node>,
{
}

fn app<E: Event>(_: EventKey<E>)
where
    E::Route: AppRoute,
    E::Source: AppEventSource,
{
}

fn notice<E: Event>(_: EventKey<E>)
where
    (): EventResult<E::Route>,
{
}

fn main() {
    entity(Tap::EVENT);
    entity(LongPress::EVENT);
    entity(DragStart::EVENT);
    entity(DragMove::EVENT);
    entity(DragEnd::EVENT);
    entity(DragCancel::EVENT);
    entity(Pinch::EVENT);
    entity(Rotate::EVENT);
    entity(Key::EVENT);
    entity(CharInput::EVENT);
    app(Key::EVENT);
    app(CharInput::EVENT);
    app(Wheel::EVENT);
    app(Rotary::EVENT);
    app(Started::EVENT);
    app(Suspended::EVENT);
    app(Resumed::EVENT);
    app(MemoryWarning::EVENT);
    app(HostInteraction::EVENT);
    app(Quit::EVENT);
    notice(Started::EVENT);
    notice(Suspended::EVENT);
    notice(Resumed::EVENT);
    notice(MemoryWarning::EVENT);
    notice(HostInteraction::EVENT);
    notice(Quit::EVENT);
}
