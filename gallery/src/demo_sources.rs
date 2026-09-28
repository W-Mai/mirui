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
    CURVE_TEXT_COMPACT => {
        demo: "curve_text_compact",
        marker: "fn compact_curve_animation_system",
        parts: [
            "style.rs",
            "state.rs",
            "geometry.rs",
            "composition.rs",
            "runtime.rs",
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
    THREE_BODY => {
        demo: "three_body",
        marker: "fn three_body_step",
        parts: [
            "state.rs",
            "orbit.rs",
            "simulation.rs",
            "composition.rs",
            "tests.rs",
        ]
    },
    LIFE => {
        demo: "life",
        marker: "fn life_render",
        parts: [
            "state.rs",
            "render.rs",
            "runtime.rs",
            "composition.rs",
            "tests.rs",
        ]
    },
    EFFECT_PANELS => {
        demo: "effect_panels",
        marker: "fn animate_color_flash",
        parts: [
            "animation.rs",
            "style.rs",
            "composition.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    OFFSCREEN_MODAL => {
        demo: "offscreen_modal",
        marker: "fn modal_slide_system",
        parts: [
            "state.rs",
            "style.rs",
            "animation.rs",
            "composition.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    VECTOR_MANDALA => {
        demo: "vector_mandala",
        marker: "fn vector_mandala_render",
        parts: [
            "state.rs",
            "scene.rs",
            "render.rs",
            "composition.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    RENDER_SHOWCASE => {
        demo: "render_showcase",
        marker: "fn showcase_render",
        parts: [
            "scene.rs",
            "render.rs",
            "composition.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    OFFSCREEN => {
        demo: "offscreen",
        marker: "fn mode_toggle_system",
        parts: [
            "state.rs",
            "systems.rs",
            "composition.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    SPATIAL_ANIM => {
        demo: "spatial_anim",
        marker: "pub fn spring_system",
        parts: ["motion.rs", "composition.rs", "runtime.rs", "tests.rs"]
    },
    ANIMATION => {
        demo: "animation",
        marker: "mirui_macros::animate!(AnimateColor",
        parts: ["motion.rs", "composition.rs", "runtime.rs", "tests.rs"]
    },
    EFFECT_GLASS => {
        demo: "effect_glass",
        marker: "mirui_macros::animate!(GaussRadius",
        parts: ["motion.rs", "composition.rs", "runtime.rs", "tests.rs"]
    },
    SUBPIXEL => {
        demo: "subpixel",
        marker: "fn bar_move_system",
        parts: [
            "state.rs",
            "motion.rs",
            "composition.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    IMAGE_FLIP => {
        demo: "image_flip",
        marker: "fn spin_system",
        parts: ["motion.rs", "composition.rs", "runtime.rs", "tests.rs"]
    },
    TRANSFORM => {
        demo: "transform",
        marker: "fn spin_system",
        parts: ["motion.rs", "composition.rs", "runtime.rs", "tests.rs"]
    },
    BOOK_FLIP => {
        demo: "book_flip",
        marker: "fn flip_system",
        parts: [
            "state.rs",
            "animation.rs",
            "composition.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    FLIP_CARD => {
        demo: "flip_card",
        marker: "fn flip_system",
        parts: [
            "state.rs",
            "animation.rs",
            "composition.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    ICON => {
        demo: "icon",
        marker: "pub(super) fn icons",
        parts: [
            "catalog.rs",
            "motion.rs",
            "composition.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    GRADIENT => {
        demo: "gradient",
        marker: "fn gradient_render",
        parts: [
            "scene.rs",
            "render.rs",
            "composition.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    CLIP_PATH => {
        demo: "clip_path",
        marker: "fn clip_path_render",
        parts: [
            "scene.rs",
            "render.rs",
            "composition.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    FILL_RULES => {
        demo: "fill_rules",
        marker: "fn fill_rules_render",
        parts: ["render.rs", "composition.rs", "runtime.rs", "tests.rs"]
    },
    BLUR_FILTER => {
        demo: "blur_filter",
        marker: "fn blur_filter_render",
        parts: [
            "scene.rs",
            "render.rs",
            "composition.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    CUSTOM_VIEW => {
        demo: "custom_view",
        marker: "fn diamond_render",
        parts: ["view.rs", "composition.rs", "runtime.rs", "tests.rs"]
    },
    SHAPES => {
        demo: "shapes",
        marker: "fn shapes_render",
        parts: ["view.rs", "composition.rs", "runtime.rs", "tests.rs"]
    },
    PARTICLES => {
        demo: "particles",
        marker: "fn particle_system",
        parts: [
            "state.rs",
            "motion.rs",
            "composition.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    BUTTERFLY => {
        demo: "butterfly",
        marker: "fn butterfly_render",
        parts: [
            "state.rs",
            "render.rs",
            "animation.rs",
            "composition.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    PINCH_ROTATE => {
        demo: "pinch_rotate",
        marker: "fn refresh",
        parts: [
            "state.rs",
            "gesture.rs",
            "composition.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    THEME_SWAP => {
        demo: "theme_swap",
        marker: "fn dark_with_accent",
        parts: ["state.rs", "composition.rs", "runtime.rs", "tests.rs"]
    },
    COVER_FLOW => {
        demo: "cover_flow",
        marker: "fn layout_system",
        parts: [
            "state.rs",
            "layout.rs",
            "composition.rs",
            "runtime.rs",
            "tests.rs",
        ]
    },
    STATE_SHOW => {
        demo: "state_show",
        marker: "fn setup_app",
        parts: ["composition.rs", "tests.rs"]
    },
    STATE_LIST => {
        demo: "state_list",
        marker: "fn build_widgets",
        parts: ["state.rs", "composition.rs", "runtime.rs", "tests.rs"]
    },
    STATE_COUNTER => {
        demo: "state_counter",
        marker: "pub fn increment",
        parts: ["model.rs", "composition.rs", "runtime.rs", "tests.rs"]
    },
    STATE_TODO => {
        demo: "state_todo",
        marker: "fn todo_row",
        parts: ["model.rs", "composition.rs", "runtime.rs", "tests.rs"]
    },
    NICHE => {
        demo: "niche",
        marker: "ui!(compose Card {",
        parts: ["card.rs", "composition.rs", "runtime.rs", "tests.rs"]
    },
    I18N => {
        demo: "i18n",
        marker: "fn build_widgets",
        parts: ["catalog.rs", "composition.rs", "tests.rs"]
    },
    PERSISTENCE_COUNTER => {
        demo: "persistence_counter",
        marker: "fn pick_storage",
        parts: ["storage.rs", "composition.rs", "runtime.rs", "tests.rs"]
    },
    LAZY_LIST => {
        demo: "lazy_list",
        marker: "fn row_binder",
        parts: ["binding.rs", "composition.rs", "tests.rs"]
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
    WIDGETS_COMPACT => {
        demo: "widgets_compact",
        marker: "fn bind_row",
        parts: [
            "style.rs",
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
