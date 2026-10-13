use mirui::ecs::SystemScheduler;
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
    render_system::try_update_layout(&mut app.world, root, &viewport)
        .expect("prepare projective demo layout");
    match demo {
        Demo::FlipCard => {
            let mut scheduler = SystemScheduler::new();
            scheduler.add(flip_card::flip_system::system());
            for _ in 0..frames {
                scheduler.run_all(&mut app.world).unwrap();
            }
        }
        Demo::ImageFlip => {
            let mut scheduler = SystemScheduler::new();
            scheduler.add(image_flip::spin_system::system());
            for _ in 0..frames {
                scheduler.run_all(&mut app.world).unwrap();
            }
        }
        Demo::BookFlip => {
            let mut scheduler = SystemScheduler::new();
            scheduler.add(book_flip::flip_system::system());
            for _ in 0..frames {
                scheduler.run_all(&mut app.world).unwrap();
            }
        }
        Demo::CoverFlow => {
            let mut scheduler = SystemScheduler::new();
            scheduler.add(cover_flow::layout_system::system());
            scheduler.run_all(&mut app.world).unwrap();
        }
    }
    root
}
