mod lab_snapshot_support;

use mirui::gallery::demos::marble_play;
use mirui::input::event::bubble_dispatch;
use mirui::input::event::gesture::GestureEvent;
use mirui::types::Fixed;

fn main() {
    lab_snapshot_support::run("marble-play.png", marble_play::VIEWPORT, |app, root| {
        marble_play::setup_app(app, root);
        let page = std::env::var("MIRUI_MARBLE_SNAPSHOT_PAGE").unwrap_or_default();
        let nav_id = match page.as_str() {
            "" | "play" => return,
            "edit" => "marble_nav_edit",
            "scenes" => "marble_nav_scenes",
            "settings" => "marble_nav_settings",
            _ => panic!("unknown Marble snapshot page: {page}"),
        };
        let target = app
            .world
            .find_by_id(nav_id)
            .expect("Marble page navigation");
        bubble_dispatch(
            &mut app.world,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target,
            },
        );
    });
}
