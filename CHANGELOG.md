# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.3] - 2026-05-07

### Added

- `mirui-macros` crate: declarative UI DSL powered by xrune
- `ui!` macro with `parent` + `world` context — zero-cost abstraction
- Post-order codegen: children created before parent, solves `&mut World` borrow
- Component-style architecture: each function = a UI component
- dsl_demo example showcasing composable UI functions

## [0.1.2] - 2026-05-07

### Added

- Rounded rectangle rendering (border_radius support)
- Border rendering with rounded corners
- 8x8 bitmap font (ASCII 32-126)
- Text rendering: DrawCommand::Label, .text(), .text_color()
- WidgetBuilder: .border(), .border_radius(), .text(), .text_color()
- Event system: hit test, dispatch, EventHandler callback
- click_demo and rounded_demo examples

## [0.1.1] - 2026-05-06

### Added

- Backend trait: pluggable platform abstraction (display + input)
- SdlBackend: SDL2 window + texture + event pump (feature `sdl`)
- FramebufBackend: memory buffer + flush callback (no_std friendly)
- App struct: unified entry point with render + run loop
- `cargo xtask release` now creates GitHub Release with changelog notes
- InputEvent enum: Touch / Release / Key / Quit

## [0.1.0] - 2026-05-06

### Added

- ECS core: Entity (generational index), SparseSet, World, System trait
- Draw module: Rect, Point, Color, DrawCommand enum, Renderer trait, SwRenderer (fill + alpha blending)
- Layout engine: Flexbox (direction, justify, align, grow, padding)
- Widget system: Style component, WidgetBuilder, RenderSystem (ECS → Layout → Draw pipeline)
- Release profile: opt-level z, LTO, strip, panic=abort (~5KB on RISC-V)
- xtask: ci/build/test/lint/size/bump/publish/release
- SDL2 examples: hello_sdl, layout_demo, widget_demo
