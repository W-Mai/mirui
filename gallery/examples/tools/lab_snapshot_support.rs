use std::env;
use std::path::{Path, PathBuf};

use mirui::prelude::Entity;

#[path = "snapshot_render.rs"]
mod snapshot_render;

use snapshot_render::SnapshotApp;

fn default_output(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../.local/screenshots")
        .join(name)
}

pub fn run(output_name: &str, viewport: (u16, u16), setup: impl FnOnce(&mut SnapshotApp, Entity)) {
    let mut args = env::args().skip(1);
    let output = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| default_output(output_name));
    let width = args
        .next()
        .map(|value| value.parse().expect("viewport width"))
        .unwrap_or(viewport.0);
    let height = args
        .next()
        .map(|value| value.parse().expect("viewport height"))
        .unwrap_or(viewport.1);
    snapshot_render::render(&output, (width, height), false, |app| {
        let root = app.spawn_root().id();
        setup(app, root);
        root
    });
}
