# mirui

[![Crates.io](https://img.shields.io/crates/v/mirui.svg)](https://crates.io/crates/mirui)
[![docs.rs](https://docs.rs/mirui/badge.svg)](https://docs.rs/mirui)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A lightweight, `no_std` ECS-driven UI framework for embedded, desktop, and WebAssembly. Renders with 24.8 fixed-point subpixel precision on a software rasterizer designed for MCUs without an FPU.

## Features

- **ECS architecture** — entities, components, systems, resources, queries
- **`no_std` + `alloc`** — runs on bare-metal MCUs (ESP32-C3, STM32) with a global allocator
- **Subpixel rasterizer** — 24.8 fixed-point throughout the pipeline (layout, rendering, events). Scanline coverage anti-aliasing on any `Path`
- **Vector drawing API** — `fill_path` / `stroke_path` / `draw_line` / `draw_arc` on `DrawBackend`, cubic Bezier accurate circles
- **Declarative DSL** — `ui!` macro powered by [xrune](https://github.com/W-Mai/xrune)
- **Flexbox + absolute positioning** — familiar layout model with `Dimension::{Px, Percent, Auto, Content}`
- **HiDPI** — automatic scale factor propagation
- **Dirty-flag partial refresh** — only re-renders changed regions
- **ScrollView** — inertia, elastic bounce, scroll chaining, spring resistance
- **Widgets** — Button, Checkbox, ProgressBar, Image, ScrollView
- **Pluggable backends** — SDL2 (desktop), FramebufBackend (embedded RGB565 / ARGB8888 / RGB888 / RGB565Swapped)

## Quick Start

```toml
[dependencies]
mirui = "0.5"
mirui-macros = "0.5"
```

```rust
use mirui::app::App;
use mirui::backend::sdl::SdlBackend;
use mirui::layout::*;
use mirui::types::{Color, Dimension};
use mirui::widget::builder::WidgetBuilder;
use mirui_macros::ui;

fn main() {
    let backend = SdlBackend::new("hello mirui", 480, 320);
    let mut app = App::new(backend);

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            ..Default::default()
        })
        .id();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        content (direction: FlexDirection::Column, grow: 1.0) {
            header (bg_color: Color::rgb(88, 166, 255), height: 40, text: "Hello mirui!", border_radius: 8) {}
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
    :(
        parent: root
        world: &mut world
    :)

    // Widgets with attributes
    widget_name (attr: value, attr: value) {
        child1 (attr: value) {}
        child2 (attr: value) {}
    }

    // Enchants: attach arbitrary ECS components to the spawned entity
    img (width: 16, height: 16, image: Image::new(&IMG_THUMBS_UP)) [
        PhysicsBody { x: Fixed::ZERO, y: Fixed::ZERO },
        Velocity { vx: Fixed::from_int(1), vy: Fixed::ZERO },
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
| `border_radius` | `Fixed` | Corner radius (subpixel) |
| `border_color` | `Color` | Border color |
| `width` / `height` | `Dimension` | Px / Percent / Auto / Content |
| `grow` | `f32` | Flex grow factor |
| `direction` | `FlexDirection` | Row / Column |
| `justify` | `JustifyContent` | Main axis alignment |
| `align` | `AlignItems` | Cross axis alignment |
| `padding` | `Padding` | Inner padding |
| `position` | `Position` | Flex / Absolute |
| `left` / `top` | `Dimension` | Absolute position |
| `image` | `Image` | Image component |

Integer literals passed in the DSL (e.g. `height: 40`, `border_radius: 8`) are coerced to `Fixed` or `Dimension` via `Into`.

## Vector Drawing (0.3+)

`DrawBackend` is a full 2D rendering surface. Every primitive — solid rects, borders, text, blits, arbitrary paths — goes through it.

```rust
use mirui::draw::backend::DrawBackend;
use mirui::draw::path::Path;
use mirui::types::{Color, Fixed, Point, Rect};

// Inside any code holding a `&mut impl DrawBackend`:

// Stroked line
backend.draw_line(
    Point { x: Fixed::from_int(10), y: Fixed::from_int(10) },
    Point { x: Fixed::from_int(100), y: Fixed::from_int(80) },
    &clip,
    Fixed::from_int(2),         // width
    &Color::rgb(255, 180, 80),
    255,
);

// Stroked arc (degrees, counter-clockwise from +X axis)
backend.draw_arc(
    Point { x: Fixed::from_int(64), y: Fixed::from_int(64) },
    Fixed::from_int(40),        // radius
    Fixed::from_int(0),
    Fixed::from_int(360),
    &clip,
    Fixed::from_int(3),
    &Color::rgb(80, 180, 220),
    255,
);

// Filled custom path
let mut path = Path::new();
path.move_to(Point { x: Fixed::from_int(20), y: Fixed::from_int(20) });
path.cubic_to(
    Point { x: Fixed::from_int(60), y: Fixed::ZERO },
    Point { x: Fixed::from_int(100), y: Fixed::from_int(60) },
    Point { x: Fixed::from_int(80), y: Fixed::from_int(100) },
);
path.close();
backend.fill_path(&path, &clip, &Color::rgb(200, 90, 160), 230);
```

Paths are flattened via De Casteljau (8 segments per quadratic, 16 per cubic) then rasterized with a 4-sub-scanline coverage integration into the target texture. No allocation per pixel; per-edge sqrt only for strokes falling inside the AA ramp.

## Hybrid Backends — `compose_backend!` (0.3.1+)

Want the path rasterizer on software but blit and clear on a GPU fast path? Declare a hybrid struct and a route table; the `compose_backend!` proc-macro emits the full `DrawBackend` + `Renderer` impls statically — no runtime dispatch.

```rust
use mirui_macros::compose_backend;

compose_backend! {
    pub struct Hybrid {
        sw: SwDrawBackend,
        gpu: MyGpuBackend,
    }
    route {
        default => sw,       // everything unrouted goes here
        blit => gpu,
        clear => gpu,
    }
}
```

Generated:

```rust
pub struct Hybrid<__B0, __B1> {
    pub sw: __B0,
    pub gpu: __B1,
}
impl<__B0: DrawBackend, __B1: DrawBackend> DrawBackend for Hybrid<__B0, __B1> {
    fn blit(&mut self, ...) { self.gpu.blit(...) }
    fn clear(&mut self, ...) { self.gpu.clear(...) }
    fn fill_path(&mut self, ...) { self.sw.fill_path(...) }
    // ...and the rest, routed to `default` when unspecified
}
```

`Hybrid` is generic over one type parameter per field, so backends carrying lifetimes (`SwDrawBackend<'fb>`) flow through without needing the struct itself to declare any.

### Plugging into `App`

`App` takes a second generic `F: RendererFactory` that defaults to `SwDrawBackendFactory` (so every existing `App::new(backend)` call keeps working). To use a hybrid backend in the normal run loop:

```rust
struct HybridFactory { /* your per-frame setup */ }

impl RendererFactory for HybridFactory {
    type Renderer<'a> = Hybrid<SwDrawBackend<'a>, MyGpuBackend>;
    fn make<'a>(&'a mut self, tex: Texture<'a>, scale: Fixed) -> Self::Renderer<'a> {
        // build the fields each frame
    }
}

let mut app = App::with_factory(backend, HybridFactory { ... });
app.run();
```

Error messages are reasonable — unknown method names and unknown field names in `route { ... }` come with Levenshtein "did you mean" suggestions.

See `examples/compose_backend_demo.rs` (direct API) and `examples/compose_backend_dsl.rs` (ECS + `ui!` + `App::with_factory`).

## Plugins (0.4+)

Bundle cross-cutting behaviour — monotonic clock, FPS summary, logging, hotkeys — into objects `App` drives through five lifecycle hooks:

```rust
use mirui::plugin::Plugin;
use mirui::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};

app.add_plugin(StdInstantClockPlugin::default())
   .add_plugin(FpsSummaryPlugin::default())
   .add_system(my_system);

app.run();
```

`Plugin` trait has one required method (`build`) and four optional hooks:

| Hook | When |
|---|---|
| `build(&mut self, app)` | Once at `add_plugin` — register systems, insert resources, swap `app.clock` |
| `pre_render(world)` | Before each `render` / `render_dirty` |
| `post_render(world, render_nanos)` | After each render, with the measured duration |
| `on_event(world, event) -> bool` | For every input event before widget dispatch; `true` consumes it |
| `on_quit(world)` | Right before `App::run` returns |

Any `FnMut(&mut App<B, F>)` is a plugin via a blanket impl, so simple setup can be a closure:

```rust
app.add_plugin(|app: &mut App<_, _>| {
    app.world.insert_resource(GameSeed(42));
    app.add_system(spawn_entities);
});
```

### Built-in plugins

- **`StdInstantClockPlugin`** (`feature = "std"`) — swaps `app.clock` to a `std::time::Instant`-backed monotonic clock. Without a clock plugin installed, `post_render` sees `0` every frame and timing-oriented plugins no-op.
- **`FpsSummaryPlugin`** — accumulates `render_nanos` over a configurable frame bucket and prints an average. Use `FpsSummaryPlugin::new(count).with_sink(my_sink)` to route the output somewhere other than stderr (an LCD overlay, a UART log).

On bare metal an application normally writes its own clock plugin (e.g. an `esp_hal` systimer reader) and points the existing `FpsSummaryPlugin` at `esp_println` through `with_sink`.

### Event consumption

`on_event` returning `true` stops further widget dispatch for that event — use it for global hotkeys:

```rust
fn on_event(&mut self, _world: &mut World, event: &InputEvent) -> bool {
    matches!(event, InputEvent::Key { code: KEY_ESCAPE, pressed: true })
}
```

## ECS

```rust
// Spawn entities
let e = world.spawn();
world.insert(e, MyComponent { ... });

// Query
let mut buf = Vec::new();
world.query::<PhysicsBody>().and::<Velocity>().without::<Disabled>().collect_into(&mut buf);
for e in &buf {
    world.get_mut::<Velocity>(*e).unwrap().vx += Fixed::from_int(1);
}

// Resources (global singletons)
world.insert_resource(DeltaTime(Fixed::from_f32(0.016)));
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
        ScrollOffset { x: Fixed::ZERO, y: Fixed::ZERO },
        ScrollConfig {
            direction: ScrollAxis::Vertical,
            elastic: true,
            content_height: Fixed::from_int(800),
            content_width: Fixed::ZERO,
        }
    ] {
        walk items.iter() with item {
            row (height: 60, bg_color: item.color, text: item.label) {}
        }
    }
};
```

Features: drag scrolling, inertia, elastic bounce with spring resistance, iOS-style scroll chaining across nested scroll views.

## Performance

ESP32-C3 (RISC-V 160 MHz, no FPU) + ST7735S 128×128 SPI display, RGB565:

| Demo | FPS | Notes |
|------|-----|-------|
| Three-body (widgets + dirty rect) | 160 | `border_radius: 3` anti-aliasing enabled |
| Shapes (clock face, raw `DrawBackend`) | 32-35 | 1 circle + 12 tick lines + sweeping hand per frame |
| Butterfly (vector, raw `DrawBackend`) | 30-32 | 8 `fill_path` + 3 `draw_line` per frame; Lissajous flight + yaw rotation |

Binary size: `mirui` + a typical ESP32 app + esp-hal around 120 KB `.text` for the vector demos.

## Hardware Examples

[mirui-examples](https://github.com/W-Mai/mirui-examples) has the ESP32-C3 demos above:

- `demo-threebody` (default) — three gravitating bodies rendered with widgets + dirty rect refresh
- `demo-particles` — pulse rings, bouncing bars, floating particles
- `demo-subpixel` — two bars moving by 1 px vs 0.1 px, showcasing subpixel AA
- `demo-shapes` — clock face drawn via `draw_line` / `draw_arc`
- `demo-butterfly` — flapping, flying, yaw-rotating vector butterfly

Flash with `cargo run --release --features demo-butterfly --no-default-features` (or any other `demo-*` feature).

## License

MIT
