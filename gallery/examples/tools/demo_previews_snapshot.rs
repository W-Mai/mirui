mod snapshot_render;

use std::env;
use std::path::PathBuf;

use mirui::gallery::DemoSize;

fn clamp_axis(preferred: u16, minimum: Option<u16>, maximum: Option<u16>) -> u16 {
    let lower = minimum.unwrap_or(1);
    let upper = maximum.unwrap_or(u16::MAX).max(lower);
    preferred.clamp(lower, upper)
}

fn preview_size(size: DemoSize) -> (u16, u16) {
    if let Some(fixed) = size.fixed_size() {
        return fixed;
    }
    (
        clamp_axis(480, size.min_width, size.max_width),
        clamp_axis(320, size.min_height, size.max_height),
    )
}

fn nearby_scene(app: &mut snapshot_render::SnapshotApp, scene: &str) -> mirui::prelude::Entity {
    use mirui::gallery::demos::nearby::{self, data::Scenario, launcher::Screen};
    let root = app.spawn_root().id();
    let state = nearby::setup_app(app, root);
    app.add_resource(mirui::ecs::MonoClock::new(|| {
        static TIME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        TIME.fetch_add(250_000_000, std::sync::atomic::Ordering::Relaxed)
    }));
    let library = state.launcher();
    if let Ok(palette) = env::var("NEARBY_PALETTE") {
        let palette = palette.parse::<usize>().expect("Nearby palette index");
        assert!(palette < 3, "Nearby palette must be 0, 1 or 2");
        for _ in 0..palette {
            library.adjust(nearby::launcher::MenuAction::CycleTheme);
        }
    }
    match scene {
        "actions" => library.choose_game(0, 0),
        "panel" => state.open_network(),
        "lobby" | "discovery-actions" | "candidates" | "discovery-current" | "discovery-empty" => {
            if scene == "discovery-current" {
                state.switch_entry();
                state.back_page();
            }
            if scene == "discovery-empty" {
                while state.scenario() != Scenario::EmptyRooms {
                    state.next_scenario();
                }
            }
            state.open_network();
            if matches!(scene, "discovery-actions" | "discovery-current") {
                state.expand_discovery(true);
            }
            if scene == "candidates" {
                state.activate(0);
                state.advance();
            }
        }
        "panel-game" | "panel-host" => {
            state.launch_game("双人冒险".into(), true);
            if scene == "panel-host" {
                state.activate(0);
            }
        }
        "favorites" => {}
        "platforms" => library.open(Screen::Platforms),
        "games" => library.choose_platform(0),
        "connection-games" | "empty-games" => {
            library.close();
            if scene == "empty-games" {
                while state.scenario() != Scenario::EmptyGames {
                    state.next_scenario();
                }
            }
            state.activate(1);
        }
        "network-menu" => {
            library.network();
            library.show_overlay(nearby::launcher::Overlay::Network);
        }
        "joined-game" | "approval" | "ready" | "connection-failed" | "restore-failed" => {
            library.close();
            let scenario = match scene {
                "connection-failed" => Scenario::ConnectionFailure,
                "restore-failed" => Scenario::RestoreFailure,
                _ => Scenario::Normal,
            };
            while state.scenario() != scenario {
                state.next_scenario();
            }
            state.switch_entry();
            state.activate(usize::from(scene == "joined-game"));
            if scene == "joined-game" {
                state.advance();
                state.activate(0);
            } else if scene == "restore-failed" {
                state.back_page();
                state.advance();
            } else if scene == "connection-failed" {
                state.advance();
            } else {
                for _ in 0..3 {
                    state.advance();
                }
                if scene == "ready" {
                    state.confirm();
                }
            }
        }
        "search" => library.open(Screen::Search),
        "search-results" | "search-empty" => {
            library.open(Screen::Search);
            library.set_query(if scene == "search-results" {
                "adventure".into()
            } else {
                "not-a-game".into()
            });
        }
        "settings" => library.show_overlay(nearby::launcher::Overlay::System),
        "controls" => library.show_overlay(nearby::launcher::Overlay::Controls),
        "tools" => library.show_overlay(nearby::launcher::Overlay::Tools),
        "power" => library.show_overlay(nearby::launcher::Overlay::Power),
        "exit" => library.show_overlay(nearby::launcher::Overlay::Exit),
        "mode" => library.close(),
        "game-entry" => {
            library.close();
            state.switch_entry();
        }
        "choice" | "host" | "joined" | "rooms" | "leaving" | "network" => {
            library.close();
            state.activate(0);
            if scene != "choice" {
                state.activate(usize::from(matches!(scene, "joined" | "rooms")));
                if matches!(scene, "joined" | "rooms") {
                    state.advance();
                }
                if scene == "joined" {
                    state.activate(0);
                }
                if scene == "leaving" {
                    state.back_page();
                }
                if scene == "network" {
                    for _ in 0..3 {
                        state.advance();
                    }
                    state.confirm();
                    state.advance();
                }
            }
        }
        "unavailable" => {
            library.close();
            while state.scenario() != Scenario::UnsupportedGame {
                state.next_scenario();
            }
            state.switch_entry();
        }
        _ => panic!("unknown Nearby scene: {scene}"),
    }
    app.prepare_text_layout().expect("prepare Nearby scene");
    app.render().expect("layout Nearby scene");
    let settle = env::var("NEARBY_SETTLE_FRAMES").map_or(40, |value| {
        value.parse::<usize>().expect("Nearby settle frame count")
    });
    for _ in 0..settle {
        app.tick();
        app.render_dirty().expect("settle Nearby scene");
    }
    root
}

fn main() {
    let output = env::args()
        .nth(1)
        .map(PathBuf::from)
        .expect("output directory");
    let selected = env::args().nth(2);
    if let Some(slug) = &selected {
        assert!(gallery::lookup_demo(slug).is_some(), "unknown demo: {slug}");
    }

    for demo in gallery::DEMOS {
        if selected.as_ref().is_some_and(|slug| slug != demo.slug) {
            continue;
        }
        snapshot_render::render(
            &output.join(format!("{}.png", demo.slug)),
            if demo.slug == "nearby" && env::var_os("NEARBY_SCENE").is_some() {
                env::var("NEARBY_VIEWPORT").map_or((640, 480), |value| {
                    let (width, height) =
                        value.split_once('x').expect("Nearby viewport WIDTHxHEIGHT");
                    let size = (
                        width.parse().expect("viewport width"),
                        height.parse().expect("viewport height"),
                    );
                    assert!(size.0 > 0 && size.1 > 0, "viewport must be nonzero");
                    size
                })
            } else {
                preview_size(demo.size)
            },
            true,
            |app| {
                if demo.slug == "nearby"
                    && let Ok(scene) = env::var("NEARBY_SCENE")
                {
                    nearby_scene(app, &scene)
                } else {
                    gallery::setup_demo(demo.slug, app).expect("registered demo")
                }
            },
        );
    }
}
