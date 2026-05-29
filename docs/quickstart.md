# Quickstart

How to go from zero to a running mirui application — desktop, embedded,
or sharing UI code across both.

This guide is the long-form companion to the README and the docs.rs
crate-level introduction. It expects familiarity with `cargo` and
basic Rust, plus an SPI datasheet reading habit if you target an MCU.

## Contents

1. [Toolchain prerequisites](#toolchain-prerequisites)
2. [Desktop SDL — five minutes from zero](#desktop-sdl)
3. [ESP32-C3 embedded — fifteen minutes from zero](#esp32-c3-embedded)
4. [Cargo workspace — share UI code across multiple targets](#cargo-workspace)
5. [Skip the boilerplate with `cargo-generate`](#skip-the-boilerplate)
6. [Where to go next](#where-to-go-next)

## Toolchain prerequisites

Stable Rust (1.85 or newer) is enough for the desktop path:

```bash
rustup default stable
rustup update
```

The ESP32-C3 path also wants a `riscv32imc` target and the `espflash`
flasher. Both work on stable:

```bash
rustup target add riscv32imc-unknown-none-elf
cargo install espflash
```

For the workspace template that mixes desktop and embedded crates, the
above two are enough — no nightly required.

## Desktop SDL

The SDL backend is the fastest way to see mirui render anything. SDL2 is
linked dynamically; install it from your package manager:

```bash
brew install sdl2          # macOS
apt-get install libsdl2-dev   # Debian / Ubuntu
```

Create a fresh project:

```bash
cargo new hello-mirui
cd hello-mirui
```

`Cargo.toml`:

```toml
[package]
name = "hello-mirui"
version = "0.1.0"
edition = "2024"

[dependencies]
mirui = { version = "0.23", features = ["sdl"] }
mirui-macros = "0.23"
```

`src/main.rs`:

```rust
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;

fn main() {
    let backend = SdlSurface::new("hello mirui", 480, 320);
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(ColorToken::Surface)
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

        column (direction: FlexDirection::Column, grow: 1.0) {
            header (
                bg_color: ColorToken::Primary,
                text_color: ColorToken::OnPrimary,
                height: 40,
                text: "Hello mirui!",
                border_radius: 8
            ) {}
            content (bg_color: ColorToken::SurfaceVariant, grow: 1.0) {}
            footer (height: 30, text: "ECS + DSL") {}
        }
    };

    app.set_root(root);
    app.run();
}
```

Run it:

```bash
cargo run
```

A 480x320 window appears with a blue header, a darker content area, and
a footer strip — three widgets stacked inside a column container.

If `cargo run` fails to find `libSDL2.dylib` on macOS (Apple Silicon),
add the Homebrew lib path:

```bash
export LIBRARY_PATH="/opt/homebrew/lib:$LIBRARY_PATH"
```

## ESP32-C3 embedded

The embedded path takes a hardware kit, an SPI display, and a USB cable.
The shape of an mirui ESP project is small — `no_std` `main`, esp-hal
peripherals, and a `FramebufSurface` whose flush closure speaks SPI to
the panel — but the BSP wiring (clock setup, SPI mode + DMA buffers,
ST7735/ST7789/GC9A01 init sequence, panel reset) is dozens of lines and
moves with each esp-hal release.

Rather than reproduce that wiring here and let it bit-rot, the
canonical reference is the
[mirui-examples](https://github.com/W-Mai/mirui-examples) project, which
this guide tracks. Start there:

```bash
git clone https://github.com/W-Mai/mirui-examples
cd mirui-examples/examples/esp32c3-animation
cargo build --release --no-default-features --features=demo-threebody
```

The `esp32c3-animation` crate's `src/board.rs` carries the SPI + ST7735
driver, `src/main.rs` ties it into `App::new(FramebufSurface::…)`, and
`Cargo.toml` pins compatible esp-hal / esp-alloc / esp-bootloader-esp-idf
versions. Copy the project, point `board.rs` at your own panel's
pinout, and replace the demo widgets with your own `ui!` tree.

Once it builds, flash with:

```bash
espflash flash --monitor target/riscv32imc-unknown-none-elf/release/mirui-esp32c3
```

The piece that's universal across boards is the mirui side:

```rust
let backend = FramebufSurface::with_format(
    W, H,
    mirui::draw::texture::ColorFormat::RGB565Swapped,  // or RGB565
    |bytes: &[u8], area: &Rect| {
        // Push `bytes` to your LCD over SPI for the window described by `area`.
    },
);

let mut app = App::new(backend);
app.with_default_widgets().with_default_systems();
// build the ui! tree, set_root, run — same as the desktop hello.
```

`ColorFormat::RGB565Swapped` is the byte order most ST7735/ST7789 panels
expect when the host MCU is little-endian; use `RGB565` if your panel
takes the bytes the other way around. mirui's
[Surface trait docs](../src/surface/mod.rs) cover the rest of the
contract — `display_info`, `flush`, `poll_event`, persistence — and
which to override for a custom backend.

## Cargo workspace

When the same UI code should drive both a desktop window and an MCU
panel, put the UI in a shared library crate and let one binary crate
per target consume it. mirui ships a Cargo workspace template that
sets this up; you can also build it by hand.

Layout:

```
my-app/
├── Cargo.toml             # [workspace] members = ["app", "targets/*"]
├── app/                   # shared UI library, std/no_std dual
│   ├── Cargo.toml
│   └── src/lib.rs
└── targets/               # one crate per target, glob-matched
    ├── desktop/           # SDL bin
    │   ├── Cargo.toml
    │   └── src/main.rs
    └── esp32c3/           # ESP32-C3 bin
        ├── Cargo.toml
        ├── .cargo/config.toml
        ├── rust-toolchain.toml
        └── src/main.rs
```

Root `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = ["app", "targets/*"]
```

The `targets/*` glob means a new target crate dropped into
`targets/<name>/` is picked up without editing the workspace manifest.

`app/Cargo.toml`:

```toml
[package]
name = "app"
version = "0.1.0"
edition = "2024"

[features]
default = []
std = ["mirui/std"]

[dependencies]
mirui = { version = "0.23", default-features = false, features = ["quad-aa"] }
mirui-macros = "0.23"
```

`app/src/lib.rs`:

```rust
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

use mirui::prelude::*;
use mirui::ecs::{Entity, World};

pub fn build_ui(world: &mut World, parent: Entity) -> Entity {
    ui! {
        :(
            parent: parent
            world: world
        :)

        root (bg_color: ColorToken::Surface) {
            hello (
                text: "Hello mirui!",
                text_color: ColorToken::OnSurface
            ) {}
        }
    }
}
```

`targets/desktop/Cargo.toml` enables `sdl` on mirui and `std` on `app`;
`targets/esp32c3/Cargo.toml` keeps both at `default-features = false`
and adds the esp-hal stack. Each target's `main.rs` opens a Surface,
constructs `App`, and calls `app::build_ui` to populate the tree.

### Adding a new target

The workspace is meant to grow. To add ESP32-S3, RP2040, STM32, or any
other MCU:

1. Copy an existing target as a starting point: `cp -r targets/esp32c3 targets/esp32s3`.
2. Update the new crate's `Cargo.toml` with the right `[package].name`
   and BSP dependencies.
3. Update `.cargo/config.toml` and `rust-toolchain.toml` for the new
   target triple and linker script.
4. Adjust `src/main.rs` to talk to the new chip's clocks, SPI, and
   panel.

Build it with `cargo build -p esp32s3`. Workspace membership is
automatic — the glob picks the new directory up on the next `cargo`
invocation.

## Skip the boilerplate

The same templates this guide walks through by hand are published as a
[`cargo-generate`](https://github.com/cargo-generate/cargo-generate)
template repository. After installing the tool:

```bash
cargo install cargo-generate
```

Generate a project from any of the templates:

```bash
# Single-target SDL
cargo generate W-Mai/mirui-templates sdl-only --name hello-mirui

# Single-target ESP32-C3
cargo generate W-Mai/mirui-templates esp32c3 --name hello-mirui-esp32c3

# Multi-target Cargo workspace (app + targets/desktop + targets/esp32c3)
cargo generate W-Mai/mirui-templates workspace --name my-app
```

Each template asks for the project name and the mirui version, fills
the Cargo manifests and source files in, and leaves you with a project
that builds on the first `cargo build`.

A `wasm` template is published alongside the others; it pins a future
`web-canvas` Surface backend that has not landed yet, so it is a
placeholder and will not build until that backend ships.

## Where to go next

You have a running mirui app. The next steps depend on what you want to
build:

- **Add your own widget** — see the widget cookbook (planned for the
  1.0 cycle) for the rendering, theme integration, and animation
  contracts the built-in widgets follow.
- **Drive your own LCD or touch IC** — the surface cookbook (planned)
  walks through ST7789, GC9A01, FT6236, GT911, and the
  `Surface` trait that ties them to mirui.
- **React to state changes declaratively** — the state-management
  story (planned for 1.0) layers `Signal<T>` / `Computed<T>` /
  `Effect` over the existing ECS World.
- **Persist user state across runs** — the lifecycle plugin (planned
  for 1.0) ships `PersistencePlugin` plus pause / resume hooks for
  embedded power management.

Until those land, the working examples in
[`gallery/examples/`](../gallery/examples/) and
[`mirui-examples`](https://github.com/W-Mai/mirui-examples) are the
most complete reference.
