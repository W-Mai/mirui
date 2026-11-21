# Pixel Flight

Fly over procedural islands with overhead, chase and cockpit views. The game builds two levels of aircraft detail from [jet.mirx](assets/jet.mirx) and renders the scene with [SpriteStack](../../extensions/sprite-stack).

## Run the demo

Run the desktop version from the Mirui checkout:

```sh
cargo run -p gallery --example pixel_flight_demo --profile release-opt --no-default-features --features sdl-gpu,pixel-flight
```

Point to steer, click to roll, hold the pointer button or Shift to boost, and press Space to switch views. Both desktop and Compact support window resizing and the same controls.

Compact renders at native 128×128 with bounded scene tiles and lossless MIRX cockpit rows. Mirui selects the output format within a two-byte-per-pixel budget:

```sh
cargo run -p gallery --example pixel_flight_compact_demo --profile release-opt --no-default-features --features sdl-gpu,pixel-flight
```

Append `-- --auto` to either command for automatic flight with camera changes every twelve simulation seconds. In a `no_std` application, call `set_autoplay(true)` on the Model returned by `mirui_pixel_flight::setup_compact_app`. Passing `false` restores manual control without restarting the flight.

## How a frame is drawn

The flight model advances controls, aircraft and trail history. The camera tracks the player and supplies one projection per view. The viewport's attached content supplies the landscape, aircraft materials and exhaust. SpriteStack selects the output storage and runs the color and depth passes. The cockpit builds its artwork on resize and reads the current flight state for its live instruments. [Model](src/flight.rs) · [Camera](src/flight/camera.rs) · [Scene content](src/runtime/renderer.rs) · [Layers](src/raster.rs) · [Artwork](src/cockpit/artwork.rs) · [Instruments](src/cockpit/instruments.rs)

![Pixel Flight updates the aircraft poses once per simulation tick. The active camera projects the island and aircraft into a desktop or Compact frame.](docs/architecture.svg)

## Add a tracking minimap

![Rust code for a second StackViewport. The main camera and the overhead map read the same StackScene. The map targets the existing player and lets pointer input reach the flight controls.](docs/authoring.svg)

Run the [minimap example](examples/minimap.rs):

```sh
cargo run -p mirui-pixel-flight --example minimap --features mirui/sdl --profile release-opt
```

Press Space to switch the main camera. The inset keeps tracking the aircraft from above. In `minimap`, set the `camera` property to `Camera::chase(player)` to try an angled view of the same scene. The [flight update](src/runtime.rs) advances the simulation, and [the scene objects](src/runtime/objects.rs) publish aircraft poses for both cameras.
