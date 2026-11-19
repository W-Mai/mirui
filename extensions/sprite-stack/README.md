# SpriteStack

`mirui-sprite-stack` draws layered pixel art from a MIRX image atlas. Its optional `SSTK` chunk stores the order and spacing of the layers and their common pivot. An ordinary MIRX image viewer can open the same file.

## Try it

Run the SDL turntable from the Mirui checkout:

```sh
cargo run -p mirui-sprite-stack --example turntable --features mirui/sdl
```

The [turntable](examples/turntable.rs) includes a [car model](examples/car.obj) and its [prebuilt atlas](examples/car.mirx). To make another asset with ICU:

```sh
icu convert car.obj -F mirx --resolution 32 -O models
icu convert base.png body.png roof.png -F mirx --layers -O models
icu show models/car.mirx
```

PNG layers are ordered from bottom to top. Each layer uses the same canvas size and fully opaque or fully transparent pixels.

## Use in an application

For an application next to a Mirui checkout named `mirui`, add the dependencies explicitly:

```toml
[dependencies]
mirui = { path = "../mirui", default-features = false }
mirui-sprite-stack = { path = "../mirui/extensions/sprite-stack" }
```

Load the embedded asset once, register the extension's view, and pass the shared handle to a composition:

```rust
use mirui::core::cache::Handle;
use mirui::prelude::*;
use mirui_sprite_stack::{SpriteStack, SpriteStackAsset};

#[compose]
fn vehicle(car: Handle<SpriteStackAsset>, heading: Signal<Fixed>) -> Entity {
    ui! {
        SpriteStack(asset: car, yaw: ${ heading.get() }, pitch: 30, width: 96, height: 80)
    }
}

// During App setup:
let car = Handle::new(SpriteStackAsset::from_mirx(include_bytes!("vehicle.mirx"))?);
app.with_offscreen_pool_budget(64 * 1024);
app.with_widget(mirui_sprite_stack::view());
app.compose(root, vehicle(car, heading));
```

`yaw` and `pitch` use degrees. Positive pitch looks down on the layers; zero shows their edges. Omit `layer_spacing` to use the distance saved with the asset. The sprite uses the App's shared offscreen budget. A 96×80 RGBA output at 1× scale uses 30,720 bytes for its pixels.

Tools and build scripts can depend directly on [mirui-sprite-stack-format](format) and use `StackDescriptor` to read or write layer placement. The format crate depends only on `mirx`. The widget exposes the same types in its `format` module.

## Shared scenes

Register `SpriteStackPlugin`, place shared assets in a `StackScene`, and pass its Model handle to each viewport. The camera uses degrees and world units. A moving object can drive a chase camera while an overhead viewport shows the same game state.

```rust
use mirui::prelude::*;
use mirui_sprite_stack::{Camera, Object, StackScene, StackViewport};

#[mirui::compose]
fn screens(scene: model!(StackScene), player: Object) -> Entity {
    ui! {
        Row(grow: 1.0) {
            StackViewport(scene: scene.clone(), camera: Camera::chase(player), grow: 1.0)
            StackViewport(scene: scene, camera: Camera::overhead(), width: 80, height: 80)
        }
    }
}
```

`StackScene::bounded(n)` prepares instance slots. `place` accepts a shared sprite, a static prepared model, a height field, or a registered editable height field. A failed placement preserves existing objects. `set_pose`, `remove` and `height_at` use the returned `Object`. Each viewport retains its output texture and reuses up to sixteen rows of depth and coverage. A game can install a per-viewport `ScenePainter` for its own materials and effects.

`StackViewport` selects detail separately for each camera. It uses the coarsest stored grid step that projects to at most one screen pixel, and draws the base atlas at closer scales. Optional `SLOD` records borrow sparse cells and their original RGBA colors or palette indices from the MIRX file. A coarse cell on the edge stops at the original layer bounds.

[Passing Lane](examples/passing_lane.rs) places two cars from one asset on an editable road. Press **Toggle lift** to raise the amber section under the starting car, then press **Drive** to leave the platform and pass the other car. The game uses `scene.height_at` to keep each car on the road. The chase and overhead cameras show the same edit, and the game counts each pass once.

```sh
cargo run -p mirui-sprite-stack --example passing_lane --features mirui/sdl
```

Procedural games can pass their existing height and material planes to `HeightField::from_planes`. Use `with_coordinate_map` to convert local X/Z coordinates to the source map's axes. `address_at` returns the index for reading both planes.
