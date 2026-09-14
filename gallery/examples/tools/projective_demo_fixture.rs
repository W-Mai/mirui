use mirui::gallery::demos::{book_flip, cover_flow, flip_card, image_flip};
use mirui::prelude::*;
use mirui::ui::render_system;

#[derive(Clone, Copy)]
pub enum Demo {
    FlipCard,
    ImageFlip,
    BookFlip,
    CoverFlow,
}

impl Demo {
    pub fn parse(name: &str) -> Self {
        match name {
            "flip_card" => Self::FlipCard,
            "image_flip" => Self::ImageFlip,
            "book_flip" => Self::BookFlip,
            "cover_flow" => Self::CoverFlow,
            _ => panic!("unknown projective demo: {name}"),
        }
    }
}

pub fn build<B, F>(app: &mut App<B, F>, demo: Demo, frames: u16) -> Entity
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    match demo {
        Demo::FlipCard => flip_card::setup_app(app, root),
        Demo::ImageFlip => image_flip::setup_app(app, root),
        Demo::BookFlip => book_flip::setup_app(app, root),
        Demo::CoverFlow => cover_flow::setup_app(app, root),
    }
    let viewport = app.backend.display_info().viewport();
    render_system::update_layout(&mut app.world, root, &viewport);
    match demo {
        Demo::FlipCard => {
            for _ in 0..frames {
                flip_card::flip_system(&mut app.world);
            }
        }
        Demo::ImageFlip => {
            for _ in 0..frames {
                image_flip::spin_system(&mut app.world);
            }
        }
        Demo::BookFlip => {
            for _ in 0..frames {
                book_flip::flip_system(&mut app.world);
            }
        }
        Demo::CoverFlow => cover_flow::layout_system(&mut app.world),
    }
    root
}
