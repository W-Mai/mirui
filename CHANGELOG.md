# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-05-06

### Added

- ECS core: Entity (generational index), SparseSet, World, System trait
- Draw module: Rect, Point, Color, DrawCommand enum, Renderer trait, SwRenderer (fill + alpha blending)
- Layout engine: Flexbox (direction, justify, align, grow, padding)
- Widget system: Style component, WidgetBuilder, RenderSystem (ECS → Layout → Draw pipeline)
- Release profile: opt-level z, LTO, strip, panic=abort (~5KB on RISC-V)
- xtask: ci/build/test/lint/size/bump/publish/release
- SDL2 examples: hello_sdl, layout_demo, widget_demo
