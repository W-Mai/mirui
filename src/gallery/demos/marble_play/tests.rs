#[cfg(feature = "audio")]
use super::audio::audio_bank;
use super::board::{MarbleBoard, board_gesture, board_render, event_point};
use super::scenes::scene_board_render;
use super::settings::settings_panel_render;
use super::shell::build_widgets;
use super::style::{BOARD_HEIGHT, BOARD_WIDTH};
use super::{TEXT_LAYOUT_CAPACITY, VIEWPORT, marble_tick_system, setup_app};
#[cfg(feature = "audio")]
use crate::audio::{AudioHandle, AudioOutputState};
use crate::ecs::{DeltaTimeMs, SystemScheduler};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::marble::{MarbleModel, MarbleModelHandle, Page, THEMES};
use crate::gallery::play::paint::PlayPainter;
use crate::input::event::HandlerCtx;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::types::{Fixed64, Transform};
use crate::ui::ComputedRect;
#[cfg(feature = "audio")]
use crate::ui::Hidden;
use crate::ui::branch::is_hidden_in_tree;
use crate::ui::view::{ViewCtx, ViewRegistry};
use crate::ui::widgets::{Button, Slider, Switch, Text};
use crate::ui::{Children, IdMap, UiScope};

#[crate::model]
struct BoardSelection {
    #[observe]
    marker: u8,
}

#[crate::model]
impl BoardSelection {
    fn set_marker(&mut self, marker: u8) {
        self.marker = marker;
    }
}

#[crate::component(bind(game, selection))]
struct MultiSourceMarbleBoard {
    game: MarbleModel,
    selection: BoardSelection,
}

#[crate::view(
    component = MultiSourceMarbleBoard,
    read(game, selection),
    watch(game.visual_revision(), selection.marker()),
    name = "MultiSourceMarbleBoard",
    priority = 60
)]
fn multi_source_board_render(
    renderer: &mut dyn Renderer,
    game: &MarbleModel,
    selection: &BoardSelection,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    board_render(renderer, game, rect, ctx);
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 199);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(
        Rect::new(16, 16, 24, 24),
        if selection.marker == 0 {
            Color::rgb(245, 20, 30)
        } else {
            Color::rgb(20, 230, 80)
        },
        Fixed::ONE,
    );
}

fn fixture() -> World {
    let mut world = World::new();
    let mut registry = ViewRegistry::with_builtins();
    registry.insert(board_render::view());
    registry.insert(scene_board_render::view());
    registry.insert(settings_panel_render::view());
    world.insert_resource(registry);
    world.insert_resource(IdMap::new());
    let (cell, model) = crate::core::model::register(&mut world, MarbleModel::new());
    let registration = world.spawn_empty();
    world.insert(registration, cell);
    #[cfg(feature = "audio")]
    {
        let core = crate::audio::SharedAudioCore::new(crate::audio::AudioBus::<32>::new());
        world.insert_resource(crate::audio::SharedAudioCore::handle(&core));
        world.insert_resource(core.state_signal());
        world.insert_resource(core);
    }
    let root = WidgetBuilder::new(&mut world).id();
    #[cfg(feature = "audio")]
    let audio = world.resource::<AudioHandle>().cloned();
    let mut cx = UiScope::new(&mut world, root);
    #[cfg(feature = "audio")]
    build_widgets(&mut cx, model.clone(), audio);
    #[cfg(not(feature = "audio"))]
    build_widgets(&mut cx, model.clone());
    world.insert_resource(model);
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert!(world.get::<Children>(root).is_some());
    world
}

fn test_board_gesture(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    board_gesture(&HandlerCtx {
        world,
        entity,
        event,
    })
}

#[test]
fn bound_tick_uses_delta_time_and_zero_elapsed_is_idle() {
    let mut world = fixture();
    let model = world.resource::<MarbleModelHandle>().unwrap().clone();
    let mut scheduler = SystemScheduler::new();
    scheduler.add(marble_tick_system::system(model.clone()));

    let revision = model.visual_revision();
    world.insert_resource(DeltaTimeMs(60));
    scheduler.run_all(&mut world);
    assert!(model.visual_revision() > revision);

    let revision = model.visual_revision();
    world.insert_resource(DeltaTimeMs(0));
    scheduler.run_all(&mut world);
    assert_eq!(model.visual_revision(), revision);
}

#[test]
fn composition_uses_real_text_and_button_widgets() {
    let world = fixture();
    assert!(world.query::<Text>().collect().len() >= 4);
    assert!(world.query::<Button>().collect().len() >= 6);
    assert_eq!(world.query::<MarbleBoard>().collect().len(), 1);
    for (id, capacity) in [
        ("marble_pause", 4),
        ("marble_audio", 5),
        ("marble_record", 4),
        ("marble_status", 32),
        ("marble_marble_count", 9),
        ("marble_pad_count", 6),
        ("marble_setting_bpm", 16),
        ("marble_readout", 34),
        ("marble_pitch", 4),
        ("marble_timbre", 6),
    ] {
        let text = world.get::<Text>(world.find_by_id(id).unwrap()).unwrap();
        assert_eq!(text.text_capacity(), Some(capacity), "{id}");
        assert!(text.has_valid_content(), "{id}");
        assert_eq!(text.last_content_error(), None, "{id}");
    }
}

#[test]
fn declared_text_layout_capacity_covers_page_switches() {
    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets();
    app.with_text_layout_limits(crate::text::TextLayoutLimits::EMBEDDED);
    #[cfg(feature = "audio")]
    app.add_plugin(crate::app::plugins::AudioPlugin::new(
        crate::audio::SilentAudioSink::default(),
        audio_bank(),
    ));
    let reserved = crate::text::layout::TextLayoutCache::try_new_bounded(
        crate::text::TextLayoutLimits::EMBEDDED,
        TEXT_LAYOUT_CAPACITY,
    )
    .unwrap()
    .resident_bytes();
    assert!(reserved <= crate::text::TextLayoutLimits::EMBEDDED.cache_bytes);
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.prepare_text_layout().unwrap();
    let model = app
        .world
        .query::<MarbleBoard>()
        .iter()
        .next()
        .unwrap()
        .1
        .model
        .clone();

    app.render().unwrap();
    #[cfg(feature = "audio")]
    {
        let audio = app.audio().unwrap();
        assert!(audio.set_muted(false));
        app.render_dirty().unwrap();
        assert!(audio.set_muted(true));
        app.render_dirty().unwrap();
        assert_eq!(app.last_text_layout_failure(), None);
    }
    for page in [
        Page::Edit,
        Page::Scenes,
        Page::Settings,
        Page::Play,
        Page::Settings,
        Page::Edit,
    ] {
        model.set_page(page);
        crate::core::reactive::flush_signal_dirty(&mut app.world);
        app.render_dirty().unwrap();
        assert_eq!(app.last_text_layout_failure(), None);
        if page == Page::Edit {
            model.set_inspector(true);
        } else if page == Page::Settings {
            model.set_bpm(Fixed64::from_int(160));
        } else {
            continue;
        }
        crate::core::reactive::flush_signal_dirty(&mut app.world);
        app.render_dirty().unwrap();
        assert_eq!(app.last_text_layout_failure(), None);
        if page == Page::Edit {
            model.adjust_pitch(1);
            crate::core::reactive::flush_signal_dirty(&mut app.world);
            app.render_dirty().unwrap();
            model.cycle_timbre();
            crate::core::reactive::flush_signal_dirty(&mut app.world);
            app.render_dirty().unwrap();
            assert_eq!(app.last_text_layout_failure(), None);
        }
    }
}

#[test]
fn first_page_switch_keeps_dirty_storage_capacity_and_updates_output() {
    use crate::input::event::hit_test::hit_test;
    use crate::surface::FramebufferAccess;
    use crate::ui::dirty::Dirty;

    let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.prepare_text_layout().unwrap();
    let model = app
        .world
        .query::<MarbleBoard>()
        .iter()
        .next()
        .unwrap()
        .1
        .model
        .clone();
    let properties = app.world.find_by_id("marble_properties").unwrap();
    let slots = app.world.allocated_entity_slots() as usize;
    let prepared = app
        .world
        .storage::<Dirty>()
        .unwrap()
        .reserved_entity_capacity();
    assert!(prepared.0 >= slots);
    assert!(prepared.1 >= slots);

    app.render().unwrap();
    let play_pixels = app
        .backend
        .framebuffer()
        .buf
        .as_slice()
        .iter()
        .fold(0u64, |hash, byte| {
            hash.wrapping_mul(16777619) ^ *byte as u64
        });
    let before = app
        .world
        .storage::<Dirty>()
        .unwrap()
        .reserved_entity_capacity();
    assert!(is_hidden_in_tree(&app.world, properties));

    model.set_page(Page::Edit);
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    let after = app
        .world
        .storage::<Dirty>()
        .unwrap()
        .reserved_entity_capacity();
    assert_eq!(after, before);
    assert!(!is_hidden_in_tree(&app.world, properties));

    app.render_dirty().unwrap();
    let edit_pixels = app
        .backend
        .framebuffer()
        .buf
        .as_slice()
        .iter()
        .fold(0u64, |hash, byte| {
            hash.wrapping_mul(16777619) ^ *byte as u64
        });
    assert_ne!(edit_pixels, play_pixels);
    let rect = app
        .world
        .get::<crate::ui::ComputedRect>(properties)
        .unwrap()
        .0;
    let half = Fixed::from_ratio(1, 2);
    assert_eq!(
        hit_test(
            &app.world,
            root,
            rect.x + rect.w * half,
            rect.y + rect.h * half,
            VIEWPORT.0,
            VIEWPORT.1,
        ),
        Some(properties)
    );
}

#[test]
fn marble_boards_share_only_their_bound_model_instance() {
    use crate::ui::dirty::VisualDirty;

    let mut app = App::headless(480, 320);
    app.with_default_widgets();
    app.with_widget(board_render::view());
    let shared = app.add_model(MarbleModel::new());
    let separate = app.add_model(MarbleModel::new());
    let board_layout = LayoutStyle {
        width: Dimension::px(150),
        height: Dimension::px(199),
        ..LayoutStyle::default()
    };
    let boards: [_; 3] =
        core::array::from_fn(|_| WidgetBuilder::new(&mut app.world).layout(board_layout).id());
    let root = WidgetBuilder::new(&mut app.world)
        .child(boards[0])
        .child(boards[1])
        .child(boards[2])
        .id();
    app.set_root(root);
    for (index, board) in boards.into_iter().enumerate() {
        app.world.insert(
            board,
            MarbleBoard {
                model: if index == 2 {
                    separate.clone()
                } else {
                    shared.clone()
                },
            },
        );
    }
    ViewRegistry::reconcile_observations(&mut app.world);
    for board in boards {
        app.world.remove::<VisualDirty>(board);
    }

    shared.set_page(Page::Edit);
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    assert!(app.world.has::<VisualDirty>(boards[0]));
    assert!(app.world.has::<VisualDirty>(boards[1]));
    assert!(!app.world.has::<VisualDirty>(boards[2]));
    for board in boards {
        app.world.remove::<VisualDirty>(board);
    }

    separate.set_page(Page::Scenes);
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    assert!(!app.world.has::<VisualDirty>(boards[0]));
    assert!(!app.world.has::<VisualDirty>(boards[1]));
    assert!(app.world.has::<VisualDirty>(boards[2]));
}

#[test]
fn marble_board_pixels_follow_only_their_bound_model() {
    use crate::render::texture::ColorFormat;
    use crate::surface::FramebufferAccess;
    use crate::surface::framebuf::FramebufSurface;
    use crate::ui::layout::FlexDirection;
    use alloc::vec::Vec;

    fn board_pixels(buf: &[u8], stride: usize) -> [Vec<u8>; 3] {
        core::array::from_fn(|board| {
            let mut pixels = Vec::with_capacity(480 * 199 * 4);
            for y in 0..199 {
                let start = y * stride + board * 480 * 4;
                pixels.extend_from_slice(&buf[start..start + 480 * 4]);
            }
            pixels
        })
    }

    let backend = FramebufSurface::with_format(1440, 199, ColorFormat::RGBA8888, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets();
    app.with_widget(board_render::view());
    let shared = app.add_model(MarbleModel::new());
    let separate = app.add_model(MarbleModel::new());
    let board_layout = LayoutStyle {
        width: Dimension::px(480),
        height: Dimension::px(199),
        ..LayoutStyle::default()
    };
    let boards: [_; 3] =
        core::array::from_fn(|_| WidgetBuilder::new(&mut app.world).layout(board_layout).id());
    let root = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            direction: FlexDirection::Row,
            width: Dimension::px(1440),
            height: Dimension::px(199),
            ..LayoutStyle::default()
        })
        .child(boards[0])
        .child(boards[1])
        .child(boards[2])
        .id();
    app.set_root(root);
    for (slot, board) in boards.into_iter().enumerate() {
        app.world.insert(
            board,
            MarbleBoard {
                model: if slot == 2 {
                    separate.clone()
                } else {
                    shared.clone()
                },
            },
        );
    }

    app.render().unwrap();
    app.world.clear_subtree_dirty(root);
    let texture = app.backend.framebuffer();
    let initial = board_pixels(texture.buf.as_slice(), texture.stride);
    assert!(
        initial[0] == initial[1],
        "shared boards start with equal pixels"
    );
    assert!(
        initial[0] == initial[2],
        "equal models start with equal pixels"
    );
    assert!(
        initial[0]
            .chunks_exact(4)
            .any(|pixel| pixel[..3] != [0, 0, 0])
    );

    shared.set_page(Page::Scenes);
    app.render_dirty().unwrap();
    let texture = app.backend.framebuffer();
    let after_shared = board_pixels(texture.buf.as_slice(), texture.stride);
    assert!(
        after_shared[0] != initial[0],
        "shared model repaints its boards"
    );
    assert!(
        after_shared[0] == after_shared[1],
        "shared boards stay equal"
    );
    assert!(
        after_shared[2] == initial[2],
        "independent board stays untouched"
    );

    separate.reset_scene(1);
    app.render_dirty().unwrap();
    let texture = app.backend.framebuffer();
    let after_separate = board_pixels(texture.buf.as_slice(), texture.stride);
    assert!(
        after_separate[0] == after_shared[0],
        "shared board stays untouched"
    );
    assert!(
        after_separate[1] == after_shared[1],
        "shared board stays untouched"
    );
    assert!(
        after_separate[2] != after_shared[2],
        "independent model repaints"
    );
}

#[test]
fn independent_marble_boards_repaint_for_a_shared_selection_source() {
    use crate::render::texture::ColorFormat;
    use crate::surface::FramebufferAccess;
    use crate::surface::framebuf::FramebufSurface;
    use crate::ui::layout::FlexDirection;
    use alloc::vec::Vec;

    fn board_pixels(buf: &[u8], stride: usize) -> [Vec<u8>; 2] {
        core::array::from_fn(|board| {
            let mut pixels = Vec::with_capacity(480 * 199 * 4);
            for y in 0..199 {
                let start = y * stride + board * 480 * 4;
                pixels.extend_from_slice(&buf[start..start + 480 * 4]);
            }
            pixels
        })
    }

    let backend = FramebufSurface::with_format(960, 199, ColorFormat::RGBA8888, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets();
    app.with_widget(multi_source_board_render::view());
    let first = app.add_model(MarbleModel::new());
    let second = app.add_model(MarbleModel::new());
    let selection = app.add_model(BoardSelection { marker: 0 });
    let board_layout = LayoutStyle {
        width: Dimension::px(480),
        height: Dimension::px(199),
        ..LayoutStyle::default()
    };
    let boards: [_; 2] =
        core::array::from_fn(|_| WidgetBuilder::new(&mut app.world).layout(board_layout).id());
    let root = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            direction: FlexDirection::Row,
            width: Dimension::px(960),
            height: Dimension::px(199),
            ..LayoutStyle::default()
        })
        .child(boards[0])
        .child(boards[1])
        .id();
    app.set_root(root);
    for (board, game) in boards.into_iter().zip([first.clone(), second.clone()]) {
        app.world.insert(
            board,
            MultiSourceMarbleBoard {
                game,
                selection: selection.clone(),
            },
        );
    }

    app.render().unwrap();
    app.world.clear_subtree_dirty(root);
    let texture = app.backend.framebuffer();
    let initial = board_pixels(texture.buf.as_slice(), texture.stride);
    assert!(
        initial[0] == initial[1],
        "equal models start with equal pixels"
    );

    selection.set_marker(1);
    app.render_dirty().unwrap();
    let texture = app.backend.framebuffer();
    let selected = board_pixels(texture.buf.as_slice(), texture.stride);
    assert!(
        selected[0] != initial[0],
        "shared selection repaints first board"
    );
    assert!(
        selected[1] != initial[1],
        "shared selection repaints second board"
    );
    assert!(
        selected[0] == selected[1],
        "shared selection keeps boards equal"
    );

    first.set_page(Page::Scenes);
    app.render_dirty().unwrap();
    let texture = app.backend.framebuffer();
    let after_first_game = board_pixels(texture.buf.as_slice(), texture.stride);
    assert!(
        after_first_game[0] != selected[0],
        "first game repaints first board"
    );
    assert!(
        after_first_game[1] == selected[1],
        "second board stays untouched"
    );

    second.set_page(Page::Settings);
    app.render_dirty().unwrap();
    let texture = app.backend.framebuffer();
    let after_second_game = board_pixels(texture.buf.as_slice(), texture.stride);
    assert!(
        after_second_game[0] == after_first_game[0],
        "first board stays untouched"
    );
    assert!(
        after_second_game[1] != after_first_game[1],
        "second game repaints second board"
    );
}

#[cfg(feature = "audio")]
#[test]
fn installed_model_routes_pad_sound_to_the_shared_audio_core() {
    use crate::audio::AudioCommand;

    let mut app = App::headless(480, 320);
    app.with_default_widgets();
    let core = crate::audio::SharedAudioCore::new(crate::audio::AudioBus::<32>::new());
    app.world
        .insert_resource(crate::audio::SharedAudioCore::handle(&core));
    app.world.insert_resource(core.state_signal());
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    assert_eq!(core.pop(), Some(AudioCommand::SetMasterGain(107)));
    assert_eq!(core.pop(), Some(AudioCommand::SetMuted(true)));

    let board = app.world.find_by_id("marble_play_board").unwrap();
    let model = app.world.get::<MarbleBoard>(board).unwrap().model.clone();
    model.adjust_pitch(1);
    assert!(matches!(core.pop(), Some(AudioCommand::Tone(_))));
    assert!(matches!(core.pop(), Some(AudioCommand::Tone(_))));
    assert_eq!(core.pop(), None);
}

#[test]
fn bound_controls_follow_page_selection_and_scene_reset() {
    let mut world = fixture();
    let properties = world.find_by_id("marble_properties").unwrap();
    let scene_label = world.find_by_id("marble_scene_0_name").unwrap();
    let gravity = world.find_by_id("marble_setting_gravity_control").unwrap();
    let bpm = world.find_by_id("marble_setting_bpm_control").unwrap();
    let pitch = world.find_by_id("marble_pitch").unwrap();
    assert!(is_hidden_in_tree(&world, properties));
    assert!(is_hidden_in_tree(&world, scene_label));

    world
        .resource::<MarbleModelHandle>()
        .unwrap()
        .set_page(Page::Edit);
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert!(!is_hidden_in_tree(&world, properties));

    world
        .resource::<MarbleModelHandle>()
        .unwrap()
        .adjust_pitch(1);
    crate::core::reactive::flush_signal_dirty(&mut world);
    let selected_pitch = world
        .resource::<MarbleModelHandle>()
        .unwrap()
        .selected_pitch();
    assert_eq!(selected_pitch, 74);
    assert_eq!(world.get::<Text>(pitch).unwrap().resolve(&world), "D5");

    world
        .resource::<MarbleModelHandle>()
        .unwrap()
        .set_page(Page::Scenes);
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert!(is_hidden_in_tree(&world, properties));
    assert!(!is_hidden_in_tree(&world, scene_label));

    world
        .resource::<MarbleModelHandle>()
        .unwrap()
        .reset_scene(2);
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert!(is_hidden_in_tree(&world, scene_label));
    assert_eq!(
        world.get::<Slider>(gravity).unwrap().value,
        THEMES[2].gravity.to_fixed()
    );
    assert_eq!(
        world.get::<Slider>(bpm).unwrap().value,
        Fixed::from_int(128)
    );
}

#[test]
fn bound_readout_tracks_visual_only_hit_changes() {
    let mut world = fixture();
    let readout = world.find_by_id("marble_readout").unwrap();
    crate::core::model::ModelHandle::update(
        world.resource::<MarbleModelHandle>().unwrap(),
        |model| {
            model.hits += 1;
            ChangeSet::VISUAL
        },
    );
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert!(
        world
            .get::<Text>(readout)
            .unwrap()
            .resolve(&world)
            .contains("1 HITS")
    );
}

#[test]
fn bound_switches_update_without_widget_events() {
    let mut world = fixture();
    let trails = world.find_by_id("marble_setting_trails_control").unwrap();
    assert!(world.get::<Switch>(trails).unwrap().on);
    world
        .resource::<MarbleModelHandle>()
        .unwrap()
        .toggle_trails();
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert!(!world.get::<Switch>(trails).unwrap().on);
    assert!(!world.resource::<MarbleModelHandle>().unwrap().trails());
}

#[test]
fn nav_tap_updates_model_and_bound_page() {
    let mut world = fixture();
    let edit = world.find_by_id("marble_nav_edit").unwrap();
    let properties = world.find_by_id("marble_properties").unwrap();
    crate::input::event::bubble_dispatch(
        &mut world,
        &GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: edit,
        },
    );
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert_eq!(
        world.resource::<MarbleModelHandle>().unwrap().page(),
        Page::Edit
    );
    assert!(!is_hidden_in_tree(&world, properties));
}

#[test]
fn nav_pointer_tap_reaches_edit_after_layout() {
    use crate::input::event::gesture::GestureSystem;
    use crate::input::event::hit_test::hit_test;
    use crate::input::event::input::InputEvent;
    use crate::types::Viewport;
    use crate::ui::render_system;

    let mut app = App::headless(480, 320);
    app.world.insert_resource(IdMap::new());
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    render_system::update_layout(&mut app.world, root, &Viewport::new(480, 320, Fixed::ONE));

    let edit = app.world.find_by_id("marble_nav_edit").unwrap();
    assert_eq!(
        hit_test(&app.world, root, 180.into(), 301.into(), 480, 320),
        Some(edit)
    );

    let x = Fixed::from_int(180);
    let y = Fixed::from_int(301);
    for (event, now) in [
        (InputEvent::PointerDown { id: 0, x, y }, 0),
        (InputEvent::PointerUp { id: 0, x, y }, 50),
    ] {
        crate::input::event::dispatch_input(&mut app.world, root, &event, now, 480, 320);
    }
    let gestures: Vec<_> = app
        .world
        .resource_mut::<GestureSystem>()
        .unwrap()
        .events
        .drain()
        .collect();
    for gesture in gestures {
        crate::input::event::bubble_dispatch_at(&mut app.world, &gesture, 50);
    }
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    let properties = app.world.find_by_id("marble_properties").unwrap();
    assert!(!is_hidden_in_tree(&app.world, properties));
}

#[cfg(feature = "audio")]
#[test]
fn audio_display_tracks_shared_controls() {
    let mut world = fixture();
    let audio = world.find_by_id("marble_audio").unwrap();
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert!(world.get::<Hidden>(audio).is_none());
    assert_eq!(world.get::<Text>(audio).unwrap().resolve(&world), "WAIT");

    world
        .resource::<alloc::rc::Rc<crate::audio::SharedAudioCore<32>>>()
        .unwrap()
        .set_output_state(AudioOutputState::Ready);
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert_eq!(world.get::<Text>(audio).unwrap().resolve(&world), "ON");

    world.resource::<AudioHandle>().unwrap().set_muted(true);
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert_eq!(world.get::<Text>(audio).unwrap().resolve(&world), "SOUND");
}

#[cfg(feature = "audio")]
#[test]
fn external_mute_updates_shared_state_without_playing_a_pad() {
    use crate::audio::AudioCommand;

    let mut world = fixture();
    let core = world
        .resource::<alloc::rc::Rc<crate::audio::SharedAudioCore<32>>>()
        .unwrap()
        .clone();
    world.resource::<AudioHandle>().unwrap().set_muted(true);
    assert!(
        world
            .resource::<AudioHandle>()
            .unwrap()
            .state()
            .unwrap()
            .muted
    );
    assert_eq!(core.pop(), Some(AudioCommand::SetMuted(true)));
    world.resource::<AudioHandle>().unwrap().set_muted(false);
    assert!(
        !world
            .resource::<AudioHandle>()
            .unwrap()
            .state()
            .unwrap()
            .muted
    );
    assert_eq!(core.pop(), Some(AudioCommand::SetMuted(false)));
    assert_eq!(core.pop(), None);

    let audio_button = world.find_by_id("marble_audio").unwrap();
    let tap = GestureEvent::Tap {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        target: audio_button,
    };
    crate::input::event::bubble_dispatch(&mut world, &tap);
    assert_eq!(core.pop(), Some(AudioCommand::SetMuted(true)));
    crate::input::event::bubble_dispatch(&mut world, &tap);
    assert_eq!(core.pop(), Some(AudioCommand::SetMuted(false)));
    assert!(matches!(core.pop(), Some(AudioCommand::Tone(_))));
}

#[test]
fn drag_cancel_reaches_transactional_model_path() {
    let mut world = fixture();
    let board = world.find_by_id("marble_play_board").unwrap();
    world.insert(board, ComputedRect(Rect::new(0, 52, 480, 199)));
    let original = crate::core::model::ModelHandle::read(
        world.resource::<MarbleModelHandle>().unwrap(),
        |model| model.selected_pad().pos,
    );
    world
        .resource::<MarbleModelHandle>()
        .unwrap()
        .set_page(Page::Edit);
    assert!(test_board_gesture(
        &mut world,
        board,
        &GestureEvent::DragStart {
            x: original.x.to_fixed(),
            y: original.y.to_fixed(),
            target: board,
        }
    ));
    assert!(test_board_gesture(
        &mut world,
        board,
        &GestureEvent::DragMove {
            x: original.x.to_fixed() + Fixed::from_int(30),
            y: original.y.to_fixed(),
            dx: Fixed::from_int(30),
            dy: Fixed::ZERO,
            target: board,
        }
    ));
    world.remove::<ComputedRect>(board);
    assert!(test_board_gesture(
        &mut world,
        board,
        &GestureEvent::DragCancel {
            x: original.x.to_fixed() + Fixed::from_int(30),
            y: original.y.to_fixed(),
            target: board,
        }
    ));
    assert_eq!(
        crate::core::model::ModelHandle::read(
            world.resource::<MarbleModelHandle>().unwrap(),
            |model| model.selected_pad().pos,
        ),
        original
    );
}

#[test]
fn drag_end_commits_after_layout_component_is_removed() {
    let mut world = fixture();
    let board = world.find_by_id("marble_play_board").unwrap();
    world.insert(board, ComputedRect(Rect::new(0, 52, 480, 199)));
    let original = crate::core::model::ModelHandle::read(
        world.resource::<MarbleModelHandle>().unwrap(),
        |model| model.selected_pad().pos,
    );
    world
        .resource::<MarbleModelHandle>()
        .unwrap()
        .set_page(Page::Edit);
    assert!(test_board_gesture(
        &mut world,
        board,
        &GestureEvent::DragStart {
            x: original.x.to_fixed(),
            y: original.y.to_fixed(),
            target: board,
        }
    ));
    assert!(test_board_gesture(
        &mut world,
        board,
        &GestureEvent::DragMove {
            x: original.x.to_fixed() + Fixed::from_int(20),
            y: original.y.to_fixed(),
            dx: Fixed::from_int(20),
            dy: Fixed::ZERO,
            target: board,
        }
    ));
    world.remove::<ComputedRect>(board);
    assert!(test_board_gesture(
        &mut world,
        board,
        &GestureEvent::DragEnd {
            x: original.x.to_fixed() + Fixed::from_int(20),
            y: original.y.to_fixed(),
            vx: Fixed::ZERO,
            vy: Fixed::ZERO,
            target: board,
        }
    ));
    assert_eq!(
        crate::core::model::ModelHandle::read(
            world.resource::<MarbleModelHandle>().unwrap(),
            |model| model.selected_pad().pos.x,
        ),
        original.x + Fixed64::from_int(20)
    );
}

#[test]
fn pointer_mapping_reverses_centered_uniform_canvas_fit() {
    let rect = Rect::new(10, 20, 960, 600);
    let fit = fit_logical_canvas(rect, Transform::IDENTITY, BOARD_WIDTH, BOARD_HEIGHT);
    let painted = fit.apply_point(Point::new(120, 80));
    let mapped = event_point(rect, painted.x, painted.y).unwrap();

    assert_eq!(mapped.x, Fixed64::from_int(120));
    assert_eq!(mapped.y, Fixed64::from_int(132));
}

#[test]
fn tap_selects_a_pad_without_crossing_drag_threshold() {
    let mut world = fixture();
    let board = world.find_by_id("marble_play_board").unwrap();
    world.insert(board, ComputedRect(Rect::new(0, 52, 480, 199)));
    let target = crate::core::model::ModelHandle::read(
        world.resource::<MarbleModelHandle>().unwrap(),
        |model| model.pads[1].expect("second pad").pos,
    );
    assert!(test_board_gesture(
        &mut world,
        board,
        &GestureEvent::Tap {
            x: target.x.to_fixed(),
            y: target.y.to_fixed(),
            target: board,
        }
    ));
    let model = world.resource::<MarbleModelHandle>().unwrap();
    assert_eq!(model.selected(), 1);
    assert!(crate::core::model::ModelHandle::read(model, |model| model
        .selected_pad()
        .pulse
        .is_positive()));
}

#[test]
fn scene_card_tap_loads_selected_preset() {
    let mut world = fixture();
    let card = world.find_by_id("marble_scene_card_2").unwrap();
    world
        .resource::<MarbleModelHandle>()
        .unwrap()
        .set_page(Page::Scenes);
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert!(!is_hidden_in_tree(&world, card));
    crate::input::event::bubble_dispatch(
        &mut world,
        &GestureEvent::Tap {
            x: Fixed::from_int(340),
            y: Fixed::from_int(120),
            target: card,
        },
    );
    let model = world.resource::<MarbleModelHandle>().unwrap();
    assert_eq!(model.scene(), 2);
    assert_eq!(model.page(), Page::Play);
}
