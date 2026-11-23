# Flux Run

Pilot a magnetic salvage skiff through an endless series of island harbors. Hold the magnet to catch a core, steer around patrols, then release to send the core through a recovery gate. A thrown core can stun a guard at the cost of stability. Deliver all five cores, then steer through the exit arch to fly into the next harbor. A wreck breaks the combo and sends the skiff back to the safe dock with the recovered cores intact.

## Run

```sh
cargo run -p gallery --example flux_run_demo --features flux-run --release
cargo run -p gallery --example flux_run_demo --features flux-run --release -- --compact --auto
```

Use the arrow keys or WASD to steer relative to the screen, hold Space or J to attach the magnet, and release to throw. Enter starts or resumes a run. Press P to pause or T to switch between manual and automatic play. Open View to choose eight compass directions, Top or Follow. On Compact, tap V to cycle through all ten views. The number keys select the corresponding compass direction in a keypad grid, with 5 for Top and 0 for Follow. The V key cycles overview, follow and overhead. Esc also pauses in the desktop example. The bottom bar provides touch controls. The title screen also starts either mode directly.

## Scene and game rules

[Game rules](src/gameplay.rs) advance the ship, cores and guards in fixed steps. [Harbor generation](src/world.rs) starts with a single quay around an inner basin. Higher difficulty permits additional quays. The seed chooses where to divide the harbor and where to connect each new quay with a raised bridge. Recovery gates and the exit sit in separate berths inside the quays. Every quay has cargo to recover. Each harbor requires all five cores to be delivered. [Autopilot](src/gameplay/autopilot.rs) plans on the same quay and bridge layout used for collisions, then feeds the same movement and magnet inputs. [The app Model](src/app.rs) publishes HUD state and render poses. [The scene](src/presentation.rs) draws bridge decks, quay fixtures, recovery gates and the skiff cabin from shared SpriteStack slices. [Harbor scenery](src/presentation/scenery.rs) generates weathered paving, curbs and roof cladding from the same layout and places equipment, cargo and beacons around the quays. The desktop view fits its perspective camera to the playfield and uses closer-spaced depth slices. Compact uses an oblique orthographic camera at native pixel density. All session actions share the same footer beneath the playfield.

For a 128×128 host, install `mirui_flux_run::setup_compact_app(app, parent)` and call `set_autoplay(true)` on the returned Model. Compact autoplay fills the display, starts with the follow camera and changes views every eight seconds of play. The corner counter shows the harbor and recovered cargo. The world renders at an integer 2× scale.
