# Pixel Flight

Pixel Flight renders a wrapping island world with layered aircraft, clouds, shadows, smoke, wingtip trails and smooth afterburners. One camera moves continuously between overhead, chase and cockpit views. The cockpit includes a terrain radar, attitude indicator, speed and altitude readouts, engine meters, throttle and control stick.

```sh
cargo run -p gallery --example pixel_flight_demo --profile release-opt --no-default-features --features sdl
cargo run -p gallery --example pixel_flight_demo --profile release-opt --no-default-features --features sdl-gpu
cargo run -p gallery --example pixel_flight_demo --profile release-opt --no-default-features --features wgpu
```

The Gallery entry is `pixel_flight`. The initial desktop window is 1280×720 and can be resized.

## Controls

| Input | Action |
| --- | --- |
| Pointer | Steer; point above or below the aircraft in chase/cockpit view to climb or dive |
| Click | Roll |
| Hold pointer or Shift | Boost |
| Arrow keys or WASD | Turn, climb and dive |
| Space | Cycle overhead → chase → cockpit → overhead |
| 1 / 2 / 3 | Select a view with a continuous camera transition |
| T | Cycle daylight, sunset and night |
| F | Cycle zero, one and two wingmen |
| E | Expand or collapse the aircraft's layer spacing |
| R | Replay the aircraft stack assembly |

Pointer movement resumes pointer steering after keyboard control. Idle flight follows an automatic course with occasional rolls. Losing host focus or visibility releases held controls and pauses simulation.

## Rendering and storage

The scene generates a 1024×1024 terrain height/material field and a 512×512 cloud field. Aircraft retain thirteen indexed layers. An integer pixel scale of at least three bounds the scene image at 480×300 pixels; larger windows increase the scale. Scene pixels remain sharply scaled while afterburners use independently sampled continuous flame tiles.

Scene storage is reused between frames. Resizing releases the old image, roll and cockpit workspaces before preparing replacements, preserving the flight and camera state. CPU scene storage is bounded below 16 MiB at the largest internal image, including maps, roll/depth workspaces, cockpit layers and six pairs of prepared flame tiles. The host framebuffer and the backend's existing texture upload cache are separate. This configuration targets desktop systems.

The implementation uses standard Model, View, event and Animation-system registration. Generated pixels are submitted through ordinary Texture and Blit operations, with existing SourceOver and Add composition for afterburners. No additional engine or runtime dependency is required.
