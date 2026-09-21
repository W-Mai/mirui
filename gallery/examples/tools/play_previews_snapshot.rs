mod snapshot_render;

use std::env;
use std::path::PathBuf;

use mirui::gallery::demos::{
    atlas_restoration, echo_walker, folding_ark, logic_circuit, lumen_lab, marble_play,
    module_factory, moss_study, orbital_mission, pixel_loom, pocket_post, tidal_atlas,
    twin_beacons,
};

macro_rules! render_preview {
    ($output:expr, $slug:literal, $demo:ident) => {
        snapshot_render::render(
            &$output.join(concat!($slug, ".png")),
            $demo::VIEWPORT,
            |app, root| $demo::setup_app(app, root),
        );
    };
}

fn main() {
    let output = env::args()
        .nth(1)
        .map(PathBuf::from)
        .expect("output directory");

    render_preview!(output, "marble_play", marble_play);
    render_preview!(output, "twin_beacons", twin_beacons);
    render_preview!(output, "tidal_atlas", tidal_atlas);
    render_preview!(output, "echo_walker", echo_walker);
    render_preview!(output, "pixel_loom", pixel_loom);
    render_preview!(output, "lumen_lab", lumen_lab);
    render_preview!(output, "folding_ark", folding_ark);
    render_preview!(output, "atlas_restoration", atlas_restoration);
    render_preview!(output, "module_factory", module_factory);
    render_preview!(output, "orbital_mission", orbital_mission);
    render_preview!(output, "logic_circuit", logic_circuit);
    render_preview!(output, "moss_study", moss_study);
    render_preview!(output, "pocket_post", pocket_post);
}
