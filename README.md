# mirui

[![Crates.io](https://img.shields.io/crates/v/mirui.svg)](https://crates.io/crates/mirui)
[![docs.rs](https://docs.rs/mirui/badge.svg)](https://docs.rs/mirui)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A lightweight, `no_std` ECS-driven UI framework for embedded, desktop, and WebAssembly.

## Features

- **ECS architecture** — every widget is an entity, styles and state are components, layout and rendering are systems
- **`no_std` + `alloc`** — runs on bare-metal MCUs (STM32, ESP32) with a global allocator
- **Pluggable render backends** — SDL2, framebuffer, WebAssembly Canvas, or bring your own
- **Flexbox + absolute positioning** — familiar layout model
- **Dirty-flag driven** — only re-renders what changed

## Quick Start

```toml
[dependencies]
mirui = "0.1"
```

## Roadmap

- [ ] Minimal ECS core
- [ ] Layout engine (Flexbox)
- [ ] Render trait + SDL2 backend
- [ ] Basic widgets (Label, Button, Container)
- [ ] Event system
- [ ] Declarative macro DSL

## License

MIT
