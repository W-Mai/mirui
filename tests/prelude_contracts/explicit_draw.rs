#![deny(warnings)]

use mirui::prelude::draw::*;
use mirui::prelude::*;
use mirui::render::{Canvas, Renderer, SwRenderer};
use mirui::ui::view::{View, ViewRegistry};

#[component]
struct Marker;

#[view(component = Marker, priority = 60)]
fn marker_paint(paint: paint!()) {
    let _ = paint.rect();
    let _: FillRule = FillRule::NonZero;
    let _: CompositeMode = CompositeMode::SourceOver;
    let _: Path = Path::new();
    let _: Option<DrawCommand<'_>> = None;
}

fn backend_traits<R: Canvas + Renderer>() {}

fn main() {
    backend_traits::<SwRenderer<'static>>();
    let descriptor: View = marker_paint::view();
    let registry = ViewRegistry::default();
    assert_eq!(registry.iter().count(), 0);
    let mut app = App::headless(32, 32);
    app.with_widget(descriptor);
}
