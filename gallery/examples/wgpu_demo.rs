//! Demo with the same widget tree as `dsl_demo` on the wgpu backend.

use mirui::draw::wgpu_render::WgpuRendererFactory;
use mirui::prelude::*;
use mirui::surface::wgpu_surface::WgpuSurface;

fn header(world: &mut World, parent: Entity) -> Entity {
    ui! {
        :(
            parent: parent
            world: world
        :)

        header (
            bg_color: Color::rgb(88, 166, 255),
            height: 40,
            text: "Hello xrune!",
            border_radius: 8
        ) {}
    }
}

fn button_row(world: &mut World, parent: Entity) -> Entity {
    ui! {
        :(
            parent: parent
            world: world
        :)

        row (direction: FlexDirection::Row, grow: 1.0) {
            btn1 (bg_color: Color::rgb(63, 185, 80), grow: 1.0, text: "OK", border_radius: 6) {}
            btn2 (bg_color: Color::rgb(248, 81, 73), grow: 1.0, text: "Cancel", border_radius: 6) {}
            btn3 (bg_color: Color::rgb(210, 168, 255), grow: 1.0, text: "Maybe", border_radius: 6) {}
        }
    }
}

fn footer(world: &mut World, parent: Entity) -> Entity {
    ui! {
        :(
            parent: parent
            world: world
        :)

        footer (
            bg_color: Color::rgb(50, 50, 70),
            height: 30,
            text: "Built with mirui + xrune"
        ) {}
    }
}

fn main() {
    let backend = WgpuSurface::new("mirui - wgpu DSL demo", 480, 320);
    let factory = WgpuRendererFactory::new();
    let mut app = App::with_factory(backend, factory);
    app.with_default_widgets().with_default_systems();

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            ..Default::default()
        })
        .id();

    header(&mut app.world, root);
    button_row(&mut app.world, root);
    footer(&mut app.world, root);

    app.set_root(root);
    app.run();
}
