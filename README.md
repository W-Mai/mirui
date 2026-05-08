# mirui

[![Crates.io](https://img.shields.io/crates/v/mirui.svg)](https://crates.io/crates/mirui)
[![docs.rs](https://docs.rs/mirui/badge.svg)](https://docs.rs/mirui)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A lightweight, `no_std` ECS-driven UI framework for embedded, desktop, and WebAssembly.

## Features

- **ECS architecture** — entities, components, systems, resources, queries
- **`no_std` + `alloc`** — runs on bare-metal MCUs (ESP32-C3, STM32) with a global allocator
- **Declarative DSL** — `ui!` macro powered by [xrune](https://github.com/W-Mai/xrune)
- **Flexbox + absolute positioning** — familiar layout model
- **HiDPI support** — automatic scale factor handling
- **Dirty-flag partial refresh** — only re-renders changed regions (160fps on ESP32-C3)
- **ScrollView** — inertia, elastic bounce, scroll chaining, spring resistance
- **Components** — Button, Checkbox, ProgressBar, Image, ScrollView
- **Pluggable backends** — SDL2 (desktop), FramebufBackend (embedded)

## Quick Start

```toml
[dependencies]
mirui = "0.1"
mirui-macros = "0.1"
```

```rust
use mirui::app::App;
use mirui::backend::sdl::SdlBackend;
use mirui::layout::*;
use mirui::types::Color;
use mirui::widget::builder::WidgetBuilder;
use mirui_macros::ui;

fn main() {
    let backend = SdlBackend::new("hello mirui", 480, 320);
    let mut app = App::new(backend);

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Some(480),
            height: Some(320),
            ..Default::default()
        })
        .id();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        content (direction: FlexDirection::Column, grow: 1.0) {
            header (bg_color: Color::rgb(88, 166, 255), height: 40, text: "Hello mirui!") {}
            body (grow: 1.0, bg_color: Color::rgb(40, 40, 60)) {}
            footer (bg_color: Color::rgb(210, 168, 255), height: 30, text: "ECS + DSL") {}
        }
    };

    app.set_root(root);
    app.run();
}
```

## DSL Syntax

```rust
ui! {
    // Context: parent entity + world reference
    :(
        parent: root
        world: &mut world
    :)

    // Widgets with attributes
    widget_name (attr: value, attr: value) {
        child1 (attr: value) {}
        child2 (attr: value) {}
    }

    // Enchants: attach arbitrary components
    img (width: 16, height: 16, image: Image::new(data, 16, 16)) [
        PhysicsBody { x: 0, y: 0 },
        Velocity { vx: 1, vy: 0 },
    ] {}

    // Iteration
    walk items.iter() with item {
        row (text: item.name, bg_color: item.color) {}
    }

    // Conditional
    if show_footer {
        footer (text: "visible") {}
    }
}
```

## Supported Attributes

| Attribute | Type | Description |
|-----------|------|-------------|
| `bg_color` | `Color` | Background color |
| `text` | `&str` | Text content |
| `text_color` | `Color` | Text color |
| `border_radius` | `u16` | Corner radius |
| `border_color` | `Color` | Border color |
| `width` / `height` | `u16` | Fixed size |
| `grow` | `f32` | Flex grow factor |
| `direction` | `FlexDirection` | Row / Column |
| `justify` | `JustifyContent` | Main axis alignment |
| `align` | `AlignItems` | Cross axis alignment |
| `padding` | `Padding` | Inner padding |
| `position` | `Position` | Flex / Absolute |
| `left` / `top` | `i32` | Absolute position |
| `image` | `Image` | Image component |

## ECS

```rust
// Spawn entities
let e = world.spawn();
world.insert(e, MyComponent { ... });

// Query
let mut buf = Vec::new();
world.query::<PhysicsBody>().and::<Velocity>().without::<Disabled>().collect_into(&mut buf);
for e in &buf {
    world.get_mut::<Velocity>(*e).unwrap().vx += 1;
}

// Resources (global singletons)
world.insert_resource(DeltaTime(0.016));
let dt = world.resource::<DeltaTime>().unwrap().0;

// Systems
app.add_system(physics_system);
app.systems.add_fn(|world| { /* closure system */ });
```

## ScrollView

```rust
ui! {
    :(
        parent: root
        world: &mut world
    :)

    scroll_container (direction: FlexDirection::Column, grow: 1.0) [
        ScrollOffset { x: 0, y: 0 },
        ScrollConfig { direction: ScrollAxis::Vertical, elastic: true, content_height: 800, content_width: 0 }
    ] {
        walk items.iter() with item {
            row (height: 60, bg_color: item.color, text: item.label) {}
        }
    }
};
```

Features: drag scrolling, inertia, elastic bounce with spring resistance, scroll chaining for nested scroll views.

## Performance

Tested on ESP32-C3 (RISC-V 160MHz, no FPU) + ST7735S 128×128 SPI display:

| Mode | FPS | Notes |
|------|-----|-------|
| Full-screen refresh | 60 | SPI 26MHz bottleneck |
| Partial refresh (dirty rect) | 160 | Only changed regions transmitted |
| Binary size (.text) | ~36KB | mirui + app + esp-hal |

## Hardware Examples

See [mirui-examples](https://github.com/W-Mai/mirui-examples) for ESP32-C3 demos including three-body physics simulation with partial refresh.

## License

MIT
