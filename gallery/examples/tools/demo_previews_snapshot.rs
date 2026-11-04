mod snapshot_render;

use std::env;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use mirui::gallery::DemoSize;

static REFERENCE_TIME_NS: AtomicU64 = AtomicU64::new(0);

fn reference_clock() -> u64 {
    REFERENCE_TIME_NS.load(Ordering::Relaxed)
}

fn advance_reference(app: &mut snapshot_render::SnapshotApp, current: &mut u64, target: u64) {
    while *current < target {
        *current = (*current + 16).min(target);
        REFERENCE_TIME_NS.store(*current * 1_000_000, Ordering::Relaxed);
        app.tick();
        app.render_dirty().expect("configuration reference stage");
    }
}

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
    use mirui::surface::{Surface, SurfaceSizeHost};
    let inspector = (scene == "simulation").then(|| {
        let (width, height) = app.backend.viewport().logical_size();
        let host = SurfaceSizeHost::new(
            width.checked_sub(300).expect("inspector content width"),
            height,
        );
        app.add_resource(host.control());
        host
    });
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
    if scene == "details-game" {
        library
            .games()
            .load(core::iter::once(nearby::launcher::Game {
                directory: false,
                played: 0,
                key: 0,
                name: "魂斗罗",
                search: "contra",
                platform: Some(0),
                multiplayer: true,
                favorite: true,
            }))
            .expect("detail reference inventory");
    }
    match scene {
        "saves-actions" => {
            library
                .games()
                .load(core::iter::once(nearby::launcher::Game {
                    directory: false,
                    played: 0,
                    key: 0,
                    name: "超级炸弹人5",
                    search: "bomberman",
                    platform: Some(0),
                    multiplayer: true,
                    favorite: false,
                }))
                .expect("save action reference inventory");
            REFERENCE_TIME_NS.store(0, Ordering::Relaxed);
            app.add_resource(mirui::ecs::MonoClock::new(reference_clock));
            library.choose_game(0, 0);
            library.select_action(5);
            app.prepare_text_layout().expect("prepare save actions");
            app.render().expect("initial save actions");
            let mut time = 0;
            advance_reference(app, &mut time, 800);
        }
        scene if scene.starts_with("saves-") => {
            use nearby::wifi::Action;
            let step = scene[6..].parse::<usize>().expect("save reference step");
            assert!(step < 7);
            REFERENCE_TIME_NS.store(0, Ordering::Relaxed);
            app.add_resource(mirui::ecs::MonoClock::new(reference_clock));
            library.saves_fixture();
            app.prepare_text_layout().expect("prepare save reference");
            app.render().expect("initial save reference");
            let mut time = 0;
            advance_reference(app, &mut time, 800);
            for current in 0..=step {
                let actions = match current {
                    0 => Vec::new(),
                    1 => vec![Action::Down; 9],
                    2 => vec![Action::Confirm],
                    3 => vec![Action::Back],
                    4 => vec![Action::Refresh],
                    5 => vec![Action::Back, Action::Right],
                    _ => vec![Action::Right],
                };
                for action in actions {
                    library.saves_input(action);
                }
                advance_reference(app, &mut time, 1200 + current as u64 * 1000);
            }
        }
        scene if scene.starts_with("configuration-") => {
            use nearby::wifi::Action;
            let step = scene[14..].parse::<usize>().expect("configuration step");
            assert!(step < 7);
            let settled = env::var("NEARBY_CONFIG_SETTLE_MS").map_or(0, |value| {
                value
                    .parse::<u64>()
                    .expect("configuration settle milliseconds")
            });
            REFERENCE_TIME_NS.store(0, Ordering::Relaxed);
            app.add_resource(mirui::ecs::MonoClock::new(reference_clock));
            library.configuration_fixture();
            app.prepare_text_layout()
                .expect("prepare configuration reference");
            app.render().expect("initial configuration reference");
            let mut time = 0;
            advance_reference(app, &mut time, 800);
            for current in 0..=step {
                let actions = match current {
                    0 => Vec::new(),
                    1 => vec![Action::Down, Action::Confirm],
                    2 => vec![Action::Down; 20],
                    3 => vec![Action::Confirm],
                    4 => vec![Action::Down; 3],
                    _ => vec![Action::Back],
                };
                for action in actions {
                    library.configuration_input(action);
                }
                advance_reference(
                    app,
                    &mut time,
                    1000 + current as u64 * (200 + settled) + settled,
                );
            }
        }
        scene if scene.starts_with("wifi-") => {
            use nearby::wifi::Page;
            let page = match &scene[5..] {
                "list" => Page::List,
                "password" => Page::Password,
                "hidden" => Page::Hidden,
                "security" => Page::Security,
                "connecting" => Page::Connecting,
                "cancelling" => Page::Cancelling,
                "restoring" => Page::Restoring,
                "success" => Page::Success,
                "info" => Page::Info,
                "details" => Page::Details,
                "forget" => Page::Forget,
                "off" => Page::Off,
                "empty" => Page::Empty,
                "busy" => Page::Busy,
                "failed" => Page::Failed,
                _ => panic!("unknown Wi-Fi snapshot scene: {scene}"),
            };
            library.wifi_fixture(page);
        }
        "actions" | "details-game" => library.choose_game(0, 0),
        "details" | "details-core" | "details-firmware" | "details-saves" | "details-expanded"
        | "details-loading" | "details-failed" => {
            use nearby::details::{Document, Load, Section};
            library.choose_game(0, 0);
            library.show_game_details();
            library.set_detail_load(
                0,
                Load::Ready {
                    document: Document::example(0, "魂斗罗".into(), "NES / FC"),
                },
            );
            let section = match scene {
                "details-core" => Section::Core,
                "details-firmware" => Section::Firmware,
                "details-saves" | "details-expanded" => Section::Saves,
                _ => Section::Overview,
            };
            library.detail_section(section);
            if scene == "details-expanded" {
                library.select_action(1);
                library.toggle_detail();
            }
            if scene == "details-loading" {
                library.set_detail_load(0, Load::Loading);
            }
            if scene == "details-failed" {
                library.set_detail_load(
                    0,
                    Load::Failed {
                        message: "读取失败",
                    },
                );
            }
        }
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
        "favorites-full" => {
            const TOTAL: u32 = 24;
            const NAMES: [&str; 6] = [
                "双人冒险",
                "街头赛车",
                "方块世界",
                "旅途中的朋友",
                "周末一起玩",
                "经典游戏",
            ];
            library
                .games()
                .load((0..TOTAL).map(|key| {
                    let directory = key == TOTAL - 1;
                    nearby::launcher::Game {
                        directory,
                        played: 0,
                        key,
                        name: if directory {
                            "游戏库"
                        } else {
                            NAMES[key as usize % NAMES.len()]
                        },
                        search: "game",
                        platform: (!directory).then_some(key as usize % 6),
                        multiplayer: !directory,
                        favorite: true,
                    }
                }))
                .expect("bounded snapshot inventory");
        }
        "simulation" => state.toggle_controls(),
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
        "controls"
        | "controls-muted"
        | "controls-loading"
        | "controls-pending"
        | "controls-unavailable" => {
            use nearby::launcher::{LevelStatus, SystemLevel};
            library.show_overlay(nearby::launcher::Overlay::Controls);
            library.set_level(SystemLevel::Volume, 55);
            library.set_level(SystemLevel::Brightness, 70);
            match scene {
                "controls-muted" => library.toggle_sound(),
                "controls-loading" => {
                    library.set_level_status(SystemLevel::Volume, LevelStatus::Reading)
                }
                "controls-pending" => {
                    library.set_level_status(SystemLevel::Volume, LevelStatus::Pending)
                }
                "controls-unavailable" => library.set_level_status(
                    SystemLevel::Volume,
                    LevelStatus::Unavailable("设置读取失败"),
                ),
                _ => {}
            }
        }
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
    if let Some(host) = inspector {
        let request = host.take_request().expect("inspector expansion request");
        host.complete(request, Ok(request.size()));
        app.add_resource(host);
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
                let mut size: (u16, u16) =
                    env::var("NEARBY_VIEWPORT").map_or((640, 480), |value| {
                        let (width, height) =
                            value.split_once('x').expect("Nearby viewport WIDTHxHEIGHT");
                        let size = (
                            width.parse().expect("viewport width"),
                            height.parse().expect("viewport height"),
                        );
                        assert!(size.0 > 0 && size.1 > 0, "viewport must be nonzero");
                        size
                    });
                if env::var("NEARBY_SCENE").as_deref() == Ok("simulation") {
                    size.0 = size.0.checked_add(300).expect("expanded inspector width");
                }
                size
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
