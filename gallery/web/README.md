# gallery-web — `web-canvas` backend smoke test

A `<canvas>` plus a single hello widget driven through
`requestAnimationFrame`.

## Build & run locally

```bash
# 1. wasm32 target + wasm-bindgen-cli (one-time)
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.122

# 2. compile
cargo build -p gallery-web --release --target wasm32-unknown-unknown

# 3. wasm-bindgen post-process; output ends up in gallery/web/pkg/
wasm-bindgen \
  --target web \
  --out-dir gallery/web/pkg \
  target/wasm32-unknown-unknown/release/gallery_web.wasm

# 4. serve
python3 -m http.server --directory gallery/web 8080

# Browse to http://localhost:8080/
```

`cargo xtask wasm-build` runs steps 2 and 3 automatically once
`wasm-bindgen-cli` is on `PATH`.
