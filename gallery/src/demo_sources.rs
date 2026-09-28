macro_rules! split_demo_sources {
    (
        $(
            $name:ident => {
                demo: $demo:literal,
                marker: $marker:literal,
                parts: [$($part:literal),+ $(,)?]
            }
        ),+ $(,)?
    ) => {
        $(
            pub(crate) const $name: &str = concat!(
                include_str!(concat!(
                    "../../src/gallery/demos/",
                    $demo,
                    ".rs"
                )),
                $(
                    "\n\n// ",
                    $demo,
                    "/",
                    $part,
                    "\n",
                    include_str!(concat!(
                        "../../src/gallery/demos/",
                        $demo,
                        "/",
                        $part
                    )),
                )+
            );
        )+

        #[cfg(test)]
        pub(crate) const CASES: &[(&str, &[&str], &str)] = &[
            $(($demo, &[$($part),+], $marker),)+
        ];
    };
}

split_demo_sources! {
    ORBIT_CONSOLE => {
        demo: "orbit_console",
        marker: "fn radial_paint",
        parts: [
            "style.rs",
            "state.rs",
            "visuals.rs",
            "composition.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    LAYOUT_LAB => {
        demo: "layout_lab",
        marker: "fn compose_flex_card",
        parts: ["style.rs", "chips.rs", "cards.rs", "shell.rs", "tests.rs"]
    },
    TYPOGRAPHY_LAB => {
        demo: "typography_lab",
        marker: "fn caret_overlay_render",
        parts: [
            "style.rs",
            "state.rs",
            "caret.rs",
            "contour.rs",
            "runtime.rs",
            "composition.rs",
            "tests.rs",
        ]
    },
    KINETIC_TYPE => {
        demo: "curve_text",
        marker: "fn curve_stage_render",
        parts: [
            "style.rs",
            "state.rs",
            "geometry.rs",
            "stage.rs",
            "runtime.rs",
            "composition.rs",
            "tests.rs",
        ]
    },
    INTERACTION_LAB => {
        demo: "interaction_lab",
        marker: "fn compose_gesture_card",
        parts: [
            "style.rs",
            "state.rs",
            "composition.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    KINETIC_CONSOLE => {
        demo: "kinetic_console",
        marker: "fn orbit_render",
        parts: [
            "style.rs",
            "state.rs",
            "render.rs",
            "composition.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    SIGNAL_SCOPE => {
        demo: "signal_scope",
        marker: "fn scope_render",
        parts: [
            "signal.rs",
            "state.rs",
            "render.rs",
            "controls.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    WIDGETS => {
        demo: "widgets",
        marker: "fn row_binder",
        parts: [
            "state.rs",
            "binding.rs",
            "composition.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    MARBLE_PLAY => {
        demo: "marble_play",
        marker: "fn paint_play_board",
        parts: [
            "style.rs",
            "audio.rs",
            "board.rs",
            "scenes.rs",
            "settings.rs",
            "inspector.rs",
            "shell.rs",
            "tests.rs",
        ]
    },
    LUMEN_LAB => {
        demo: "lumen_lab",
        marker: "fn paint_board",
        parts: ["style.rs", "runtime.rs", "board.rs", "shell.rs", "tests.rs"]
    },
    PIXEL_LOOM => {
        demo: "pixel_loom",
        marker: "fn paint_pixel_surface",
        parts: [
            "style.rs",
            "state.rs",
            "render.rs",
            "input.rs",
            "shell.rs",
            "tests.rs",
        ]
    },
    MOSS_STUDY => {
        demo: "moss_study",
        marker: "fn paint_moss_surface",
        parts: [
            "style.rs",
            "state.rs",
            "render.rs",
            "input.rs",
            "shell.rs",
            "tests.rs",
        ]
    },
    POCKET_POST => {
        demo: "pocket_post",
        marker: "fn paint_post_surface",
        parts: [
            "style.rs",
            "state.rs",
            "render.rs",
            "input.rs",
            "shell.rs",
            "tests.rs",
        ]
    },
    ORBITAL_MISSION => {
        demo: "orbital_mission",
        marker: "fn paint_map",
        parts: ["style.rs", "state.rs", "render.rs", "input.rs", "shell.rs"]
    },
    LOGIC_CIRCUIT => {
        demo: "logic_circuit",
        marker: "fn paint_circuit_surface",
        parts: [
            "style.rs",
            "state.rs",
            "render.rs",
            "input.rs",
            "shell.rs",
            "tests.rs",
        ]
    },
    MODULE_FACTORY => {
        demo: "module_factory",
        marker: "fn paint_factory_surface",
        parts: [
            "style.rs",
            "state.rs",
            "render.rs",
            "input.rs",
            "shell.rs",
            "tests.rs",
        ]
    },
    TIDAL_ATLAS => {
        demo: "tidal_atlas",
        marker: "fn paint_surface",
        parts: [
            "style.rs",
            "state.rs",
            "render.rs",
            "input.rs",
            "persistence.rs",
            "shell.rs",
            "tests.rs",
        ]
    },
    ECHO_WALKER => {
        demo: "echo_walker",
        marker: "fn paint_surface",
        parts: [
            "style.rs",
            "state.rs",
            "render.rs",
            "input.rs",
            "shell.rs",
            "tests.rs",
        ]
    },
    TWIN_BEACONS => {
        demo: "twin_beacons",
        marker: "fn paint_board",
        parts: [
            "style.rs",
            "state.rs",
            "render.rs",
            "input.rs",
            "shell.rs",
            "tests.rs",
        ]
    },
    FOLDING_ARK => {
        demo: "folding_ark",
        marker: "fn paint_map",
        parts: [
            "style.rs",
            "state.rs",
            "render.rs",
            "input.rs",
            "shell.rs",
            "tests.rs",
        ]
    },
    ATLAS_RESTORATION => {
        demo: "atlas_restoration",
        marker: "fn paint_board",
        parts: [
            "style.rs",
            "geometry.rs",
            "state.rs",
            "render.rs",
            "input.rs",
            "persistence.rs",
            "shell.rs",
            "tests.rs",
        ]
    },
}
