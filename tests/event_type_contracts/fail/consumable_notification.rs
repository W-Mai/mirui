use mirui::input::event::contract::*;

struct Notice;
impl Event for Notice {
    type Route = AppNotification;
    type Source = AppSource;
    type Options = ();
}

fn notify<E: Event, R: EventResult<E::Route>>(_: EventKey<E>, result: R) {
    let _ = result.into_decision();
}

fn main() {
    notify(Notice::EVENT, true);
}
