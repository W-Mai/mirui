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
