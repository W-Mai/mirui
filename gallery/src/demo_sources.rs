macro_rules! split_demo_source {
    ($name:ident, $demo:literal, [$($part:literal),+ $(,)?]) => {
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
    };
}

split_demo_source!(
    MARBLE_PLAY,
    "marble_play",
    [
        "style.rs",
        "audio.rs",
        "board.rs",
        "scenes.rs",
        "settings.rs",
        "inspector.rs",
        "shell.rs",
        "tests.rs",
    ]
);

split_demo_source!(
    LUMEN_LAB,
    "lumen_lab",
    ["style.rs", "runtime.rs", "board.rs", "shell.rs", "tests.rs",]
);

split_demo_source!(
    PIXEL_LOOM,
    "pixel_loom",
    [
        "style.rs",
        "state.rs",
        "render.rs",
        "input.rs",
        "shell.rs",
        "tests.rs",
    ]
);
