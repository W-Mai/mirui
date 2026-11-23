# Flux Run

Pilot a magnetic salvage skiff through an endless series of two-level harbors. Hold the magnet to catch a core, steer around patrols, then release to send the core through a recovery gate. A thrown core can stun a guard at the cost of stability. Meet the quota and steer through the exit arch to fly into the next harbor. A wreck breaks the combo and sends the skiff back to the safe dock with the recovered cores intact.

## Run

```sh
cargo run -p gallery --example flux_run_demo --features flux-run --release
cargo run -p gallery --example flux_run_demo --features flux-run --release -- --compact --auto
```

Use the arrow keys or WASD to steer relative to the screen, hold Space or J to attach the magnet, and release to throw. Enter starts or resumes a run. Press P to pause or T to switch between manual and automatic play. Esc also pauses in the desktop example. The bottom bar provides touch controls. The title screen also starts either mode directly.

## Scene and game rules

[Game rules](src/gameplay.rs) advance the ship, cores and guards in fixed steps. [Harbor generation](src/world.rs) chooses warehouse footprints, bridge placement, open lanes, objectives and hazard timing from the run seed and level. [Autopilot](src/gameplay/autopilot.rs) plans around the same warehouse layout used for collisions, then feeds the same movement and magnet inputs. [The app Model](src/app.rs) publishes HUD state and render poses. [The scene](src/presentation.rs) draws a raised bridge, crane portals, recovery gates and the skiff cabin from shared SpriteStack slices. The desktop view fits its perspective camera to the playfield and uses closer-spaced depth slices. Compact uses an oblique orthographic camera at native pixel density. All session actions share the same footer beneath the playfield.

For a 128×128 host, install `mirui_flux_run::setup_compact_app(app, parent)` and call `set_autoplay(true)` on its returned Model handle to run without a controller.
