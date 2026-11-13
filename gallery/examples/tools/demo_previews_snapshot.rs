mod snapshot_render;

use std::cell::Cell;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
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

/// Lossless, so pixel fonts and one-pixel rules stay exact; the set comes out
/// at about half the size of the PNG one.
fn write_webp(path: &Path, (width, height): (u16, u16), rgba: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create preview directory");
    }
    let encoded = webp::Encoder::from_rgba(rgba, width.into(), height.into()).encode_lossless();
    fs::write(path, &*encoded).expect("write WebP preview");
    println!("wrote {}", path.display());
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
        "configuration-slot-default"
        | "configuration-slot-entry"
        | "configuration-slot-auto"
        | "configuration-slot-default-draft"
        | "configuration-slot-entry-draft"
        | "configuration-slot-auto-draft" => {
            use nearby::wifi::Action;
            let entry = scene.contains("-entry").then_some(7);
            let auto_index = scene.contains("-auto");
            REFERENCE_TIME_NS.store(0, Ordering::Relaxed);
            app.add_resource(mirui::ecs::MonoClock::new(reference_clock));
            library.configuration_slot_fixture(entry, auto_index);
            for _ in 0..3 {
                library.configuration_input(Action::Down);
            }
            library.configuration_input(Action::Confirm);
            let mut time = 0;
            advance_reference(app, &mut time, 800);
            if scene.ends_with("-draft") {
                library.configuration_input(Action::Down);
                library.configuration_input(Action::Right);
                library.configuration_input(Action::Right);
                advance_reference(app, &mut time, 1600);
            }
            let editor = library.configuration();
            editor.select(editor.count() - 1);
            library.configuration_input(Action::Confirm);
            advance_reference(app, &mut time, 2400);
            editor.select(if scene.ends_with("-draft") {
                editor.count() - 1
            } else {
                3
            });
            advance_reference(app, &mut time, 3200);
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
    let mut args: Vec<String> = env::args().skip(1).collect();
    // `--site` writes the homepage set: dark/<slug>.webp and light/<slug>.webp.
    let site = args.first().is_some_and(|arg| arg == "--site");
    if site {
        args.remove(0);
    }
    let output = args.first().map(PathBuf::from).expect("output directory");
    let selected = args.get(1).cloned();
    if let Some(slug) = &selected {
        assert!(gallery::lookup_demo(slug).is_some(), "unknown demo: {slug}");
    }

    for demo in gallery::DEMOS {
        if selected.as_ref().is_some_and(|slug| slug != demo.slug) {
            continue;
        }
        let viewport = if demo.slug == "chord_prompter" && env::var_os("CHORD_VIEWPORT").is_some() {
            let value = env::var("CHORD_VIEWPORT").unwrap();
            let (width, height) = value.split_once('x').expect("Chord viewport WIDTHxHEIGHT");
            (
                width.parse().expect("viewport width"),
                height.parse().expect("viewport height"),
            )
        } else if demo.slug == "nearby" && env::var_os("NEARBY_SCENE").is_some() {
            let mut size: (u16, u16) = env::var("NEARBY_VIEWPORT").map_or((640, 480), |value| {
                let (width, height) = value.split_once('x').expect("Nearby viewport WIDTHxHEIGHT");
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
        };
        // Demos in a host appearance family follow the page theme, as on the
        // Web canvas; custom palettes keep one look for both.
        let follows_host = Cell::new(false);
        let setup = |app: &mut snapshot_render::SnapshotApp, dark: Option<bool>| {
            let root = if demo.slug == "chord_prompter" && env::var_os("CHORD_TIME_MS").is_some() {
                use mirui::prelude::*;
                let millis: u64 = env::var("CHORD_TIME_MS")
                    .unwrap()
                    .parse()
                    .expect("Chord time in milliseconds");
                REFERENCE_TIME_NS.store(millis * 1_000_000, Ordering::Relaxed);
                app.add_resource(MonoClock::new(reference_clock));
                let root = app.spawn_root().id();
                let session = mirui::gallery::demos::chord_prompter::install(app, root);
                if let Ok(path) = env::var("CHORD_SOURCE") {
                    session
                        .load(&std::fs::read_to_string(path).expect("Chord source file"))
                        .expect("Chord score");
                }
                session.set_connected(env::var("CHORD_STATE").as_deref() != Ok("offline"));
                let state = env::var("CHORD_STATE").unwrap_or_default();
                if state == "upload" {
                    session.set_upload(Some(40));
                }
                if state == "armed" {
                    // A start with a count-in delay, as the browser sends it.
                    let mut packet = [0u8; 20];
                    packet[0] = 1;
                    packet[3..7].copy_from_slice(&1u32.to_le_bytes());
                    packet[8..12].copy_from_slice(&2500u32.to_le_bytes());
                    let mut link = mirui::gallery::demos::chord_prompter::DeviceLink::new(1);
                    assert_eq!(link.write(&session, packet, 0)[0], 0, "start rejected");
                } else if state != "stopped" {
                    session.toggle(0);
                }
                // CHORD_JUMP=BAR@MS queues a jump to a one-based bar at MS,
                // through the same Bluetooth control the display receives.
                if let Ok(jump) = env::var("CHORD_JUMP") {
                    let (bar, at) = jump.split_once('@').expect("Chord jump BAR@MS");
                    let bar = bar.parse::<u32>().expect("jump bar") - 1;
                    let at = at.parse::<u64>().expect("jump time") * 1000;
                    session.advance(at);
                    let mut packet = [0u8; 20];
                    packet[0] = 5;
                    packet[3..7].copy_from_slice(&1u32.to_le_bytes());
                    packet[7] = 2;
                    packet[8..12].copy_from_slice(&bar.to_le_bytes());
                    let mut link = mirui::gallery::demos::chord_prompter::DeviceLink::new(1);
                    assert_eq!(link.write(&session, packet, at)[0], 0, "jump rejected");
                }
                session.advance(millis * 1000);
                if state == "paused" {
                    session.pause(millis * 1000);
                }
                root
            } else if demo.slug == "nearby"
                && let Ok(scene) = env::var("NEARBY_SCENE")
            {
                nearby_scene(app, &scene)
            } else {
                gallery::setup_demo(demo.slug, app).expect("registered demo")
            };
            if let Some(dark) = dark {
                follows_host
                    .set(gallery::appearance_theme_id(app.theme().id().as_str(), false).is_some());
                if let Some(id) = gallery::appearance_theme_id(app.theme().id().as_str(), dark) {
                    app.set_theme(id).expect("appearance family is registered");
                }
            }
            root
        };
        if site {
            let dark =
                snapshot_render::capture(viewport, true, |app| setup(app, Some(true)), demo.slug);
            let light = if follows_host.get() {
                snapshot_render::capture(viewport, true, |app| setup(app, Some(false)), demo.slug)
            } else {
                dark.clone()
            };
            let name = format!("{}.webp", demo.slug);
            write_webp(&output.join("dark").join(&name), viewport, &dark);
            write_webp(&output.join("light").join(&name), viewport, &light);
        } else {
            snapshot_render::render(
                &output.join(format!("{}.png", demo.slug)),
                viewport,
                true,
                |app| setup(app, None),
            );
        }
    }
}
