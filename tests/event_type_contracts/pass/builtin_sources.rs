#![deny(warnings)]

use mirui::input::event::contract::*;
use mirui::prelude::{
    TextCommitted, DragCancel, DragEnd, DragMove, DragStart, HostInteraction, Key,
    LongPress, MemoryWarning, Pinch, Quit, Resumed, Rotary, Rotate, Started,
    Suspended, Tap, Wheel,
};
use mirui::ui::widgets::{Checkbox, ProgressBar, Slider, Switch, TabBar, TextInput};

fn widget<W: mirui::ecs::Component, E>(_: EventKey<E>)
where
    E: Event<Route = EntityNotification, Source = WidgetSource<W>, Options = ()>,
{
}

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
    widget::<Slider, _>(Slider::ValueChanged);
    widget::<Slider, _>(Slider::DragStarted);
    widget::<Slider, _>(Slider::DragEnded);
    widget::<Switch, _>(Switch::Toggled);
    widget::<Checkbox, _>(Checkbox::Toggled);
    widget::<ProgressBar, _>(ProgressBar::ValueChanged);
    widget::<TabBar, _>(TabBar::SelectionChanged);
    widget::<TextInput, _>(TextInput::Changed);
    notice(Slider::ValueChanged);
    notice(Slider::DragStarted);
    notice(Slider::DragEnded);
    notice(Switch::Toggled);
    notice(Checkbox::Toggled);
    notice(ProgressBar::ValueChanged);
    notice(TabBar::SelectionChanged);
    notice(TextInput::Changed);
    entity(Tap::EVENT);
    entity(LongPress::EVENT);
    entity(DragStart::EVENT);
    entity(DragMove::EVENT);
    entity(DragEnd::EVENT);
    entity(DragCancel::EVENT);
    entity(Pinch::EVENT);
    entity(Rotate::EVENT);
    entity(Key::EVENT);
    entity(TextCommitted::EVENT);
    app(Key::EVENT);
    app(TextCommitted::EVENT);
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
