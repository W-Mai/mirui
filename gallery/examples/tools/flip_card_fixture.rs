use mirui::gallery::demos::flip_card;
use mirui::prelude::*;
use mirui::ui::render_system;

pub fn build<B, F>(app: &mut App<B, F>, frames: u16) -> Entity
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    flip_card::setup_app(app, root);
    let viewport = app.backend.display_info().viewport();
    render_system::update_layout(&mut app.world, root, &viewport);
    for _ in 0..frames {
        flip_card::flip_system(&mut app.world);
    }
    root
}
