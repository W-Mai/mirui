use super::geometry::board_geometry;
use super::setup_app;
use super::state::PictureSurface;
use crate::core::i18n::{I18n, Locale};
use crate::core::model::ModelHandle;
use crate::ecs::World;
use crate::gallery::play::expeditions::{ExpeditionModal, ExpeditionUiModelHandle, ResultStars};
use crate::gallery::play::picture::{PictureModel, PictureModelHandle};
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::dirty::VisualDirty;
use crate::ui::view::ViewRegistry;
use crate::ui::widgets::Text;
use crate::ui::{Children, ComputedRect, Hidden, HitTarget, InteractionFeedback};

fn fixture() -> World {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    ViewRegistry::reconcile_observations(&mut app.world);
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    app.world
}

fn handles(world: &World) -> (PictureModelHandle, ExpeditionUiModelHandle) {
    let surface = world.find_by_id("picture_surface").unwrap();
    let surface = world.get::<PictureSurface>(surface).unwrap();
    (surface.model.clone(), surface.expedition.clone())
}

fn flush(world: &mut World) {
    crate::core::reactive::flush_signal_dirty(world);
}

fn editable_cell(model: &PictureModelHandle) -> (u8, i32, i32) {
    ModelHandle::read(model, |model| {
        let size = model.level().size();
        let cell = (0..size * size)
            .find(|cell| !model.level().is_given(*cell))
            .expect("the first picture has an editable cell");
        let geometry = board_geometry(model);
        let x = geometry.x + i32::from(cell % size) * geometry.cell + 1;
        let y = geometry.y + i32::from(cell / size) * geometry.cell + 1;
        (cell, x, y)
    })
}

fn trigger(world: &mut World, event: &GestureEvent) -> bool {
    {
        let decision = crate::__macro_support::dispatch_entity_gesture(world, event, 0).unwrap();
        crate::__macro_support::flush_entity_events(world).unwrap();
        decision == crate::input::event::BubbleControl::Prevent
    }
}

fn visible_interactive_descendant(world: &World, entity: Entity) -> bool {
    if crate::ui::branch::is_effectively_hidden(world, entity) {
        return false;
    }
    if world.has::<HitTarget>(entity) || world.has::<InteractionFeedback>(entity) {
        return true;
    }
    world.get::<Children>(entity).is_some_and(|children| {
        children
            .0
            .iter()
            .copied()
            .any(|child| visible_interactive_descendant(world, child))
    })
}

#[test]
fn composition_binds_game_and_expedition_models() {
    let mut world = fixture();
    let (model, expedition) = handles(&world);
    assert!(world.find_by_id("picture_fill").is_some());
    assert!(world.find_by_id("pic_col_9").is_some());
    assert_eq!(world.query::<PictureSurface>().iter().count(), 1);

    let map = world.find_by_id("picture_map").unwrap();
    assert!(world.has::<Hidden>(map));
    let locked = world.find_by_id("picture_map_level_1").unwrap();
    assert!(!visible_interactive_descendant(&world, locked));

    expedition.open(model.level_index());
    flush(&mut world);
    assert!(!world.has::<Hidden>(map));
    assert!(!visible_interactive_descendant(&world, locked));

    expedition.open_rules();
    flush(&mut world);
    assert!(!world.has::<Hidden>(world.find_by_id("picture_rules").unwrap()));
    expedition.close();
    flush(&mut world);
    assert!(world.has::<Hidden>(world.find_by_id("picture_rules").unwrap()));
}

#[test]
fn surface_event_uses_the_bound_picture_model() {
    let mut world = fixture();
    let surface = world.find_by_id("picture_surface").unwrap();
    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    let (model, _) = handles(&world);
    let (cell, x, y) = editable_cell(&model);
    let before = ModelHandle::read(&model, |model| model.cell(cell));

    assert!(trigger(
        &mut world,
        &GestureEvent::Tap {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        }
    ));
    assert_ne!(ModelHandle::read(&model, |model| model.cell(cell)), before);
    assert_eq!(model.history_len(), 1);
}

#[test]
fn surface_preserves_consumption_when_layout_is_unavailable() {
    let mut world = fixture();
    let surface = world.find_by_id("picture_surface").unwrap();
    world.remove::<ComputedRect>(surface);
    let (model, expedition) = handles(&world);
    let (_, x, y) = editable_cell(&model);
    let tap = GestureEvent::Tap {
        x: Fixed::from_int(x),
        y: Fixed::from_int(y),
        target: surface,
    };
    let drag_start = GestureEvent::DragStart {
        x: Fixed::from_int(x),
        y: Fixed::from_int(y),
        target: surface,
    };
    let drag_move = GestureEvent::DragMove {
        x: Fixed::from_int(x + 10),
        y: Fixed::from_int(y),
        dx: Fixed::from_int(10),
        dy: Fixed::ZERO,
        target: surface,
    };

    assert!(!trigger(&mut world, &tap));
    assert!(!trigger(&mut world, &drag_start));
    assert!(trigger(&mut world, &drag_move));

    expedition.open_rules();
    assert!(trigger(&mut world, &tap));
    assert!(trigger(&mut world, &drag_start));
    assert!(trigger(&mut world, &drag_move));
}

#[test]
fn overlays_block_surface_taps_drags_and_keyboard_commands() {
    let mut world = fixture();
    let surface = world.find_by_id("picture_surface").unwrap();
    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    let (model, expedition) = handles(&world);
    let (cell, x, y) = editable_cell(&model);
    let before = ModelHandle::read(&model, |model| model.cell(cell));
    let cursor = ModelHandle::read(&model, PictureModel::cursor);

    expedition.open_rules();
    assert!(trigger(
        &mut world,
        &GestureEvent::Tap {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        }
    ));
    assert!(trigger(
        &mut world,
        &GestureEvent::DragStart {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        }
    ));
    assert!(trigger(
        &mut world,
        &GestureEvent::DragMove {
            x: Fixed::from_int(x + 10),
            y: Fixed::from_int(y),
            dx: Fixed::from_int(10),
            dy: Fixed::ZERO,
            target: surface,
        }
    ));
    crate::input::event::dispatch_input(
        &mut world,
        None,
        &crate::test_support::physical_key(PhysicalKey::KeyD).into(),
        0,
        480,
        320,
    );
    assert_eq!(ModelHandle::read(&model, |model| model.cell(cell)), before);
    assert_eq!(ModelHandle::read(&model, PictureModel::cursor), cursor);
    assert_eq!(model.history_len(), 0);

    expedition.close();
    while model.modal() == ExpeditionModal::None {
        model.reveal_hint();
    }
    assert_eq!(model.modal(), ExpeditionModal::Result);
    let cursor = ModelHandle::read(&model, PictureModel::cursor);
    let tool = model.tool();
    let history_len = model.history_len();
    for position in [PhysicalKey::KeyD, PhysicalKey::KeyX] {
        crate::input::event::dispatch_input(
            &mut world,
            None,
            &crate::test_support::physical_key(position).into(),
            1,
            480,
            320,
        );
    }
    assert!(trigger(
        &mut world,
        &GestureEvent::Tap {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        }
    ));
    assert!(trigger(
        &mut world,
        &GestureEvent::DragStart {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        }
    ));
    assert_eq!(ModelHandle::read(&model, PictureModel::cursor), cursor);
    assert_eq!(model.tool(), tool);
    assert_eq!(model.history_len(), history_len);
}

#[test]
fn stroke_completion_does_not_depend_on_layout_components() {
    let mut world = fixture();
    let surface = world.find_by_id("picture_surface").unwrap();
    let (model, expedition) = handles(&world);
    let (cell, x, y) = editable_cell(&model);
    let before = ModelHandle::read(&model, |model| model.cell(cell));

    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    assert!(trigger(
        &mut world,
        &GestureEvent::DragStart {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        }
    ));
    expedition.open_rules();
    world.remove::<ComputedRect>(surface);
    assert!(trigger(
        &mut world,
        &GestureEvent::DragCancel {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        }
    ));
    assert_eq!(ModelHandle::read(&model, |model| model.cell(cell)), before);
    assert_eq!(model.history_len(), 0);

    expedition.close();
    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    assert!(trigger(
        &mut world,
        &GestureEvent::DragStart {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        }
    ));
    expedition.open_rules();
    world.remove::<ComputedRect>(surface);
    assert!(trigger(
        &mut world,
        &GestureEvent::DragEnd {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            vx: Fixed::ZERO,
            vy: Fixed::ZERO,
            target: surface,
        }
    ));
    assert_ne!(ModelHandle::read(&model, |model| model.cell(cell)), before);
    assert_eq!(model.history_len(), 1);
}

#[test]
fn keyboard_commands_wait_for_the_active_stroke_transaction() {
    let mut world = fixture();
    let surface = world.find_by_id("picture_surface").unwrap();
    world.insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
    let (model, _) = handles(&world);
    let (cell, x, y) = editable_cell(&model);
    let before = ModelHandle::read(&model, |model| model.cell(cell));
    let hints = model.hints();
    let history_len = model.history_len();

    assert!(trigger(
        &mut world,
        &GestureEvent::DragStart {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            target: surface,
        }
    ));
    crate::input::event::dispatch_input(
        &mut world,
        None,
        &crate::test_support::physical_key(PhysicalKey::KeyH).into(),
        0,
        480,
        320,
    );
    assert_eq!(model.hints(), hints);
    assert_eq!(model.history_len(), history_len);

    assert!(trigger(
        &mut world,
        &GestureEvent::DragEnd {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            vx: Fixed::ZERO,
            vy: Fixed::ZERO,
            target: surface,
        }
    ));
    assert_eq!(model.history_len(), history_len + 1);
    model.undo();
    assert_eq!(ModelHandle::read(&model, |model| model.cell(cell)), before);
}

#[derive(Default)]
struct UnhandledKey(Option<PhysicalKey>);

#[event]
fn unhandled_key(input: event!(Key), seen: res!(mut UnhandledKey)) -> bool {
    seen.0 = input.physical;
    true
}

#[test]
fn registered_keyboard_uses_both_model_instances_and_leaves_unknown_positions_unhandled() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    let (model, expedition) = handles(&app.world);
    let observer = model.clone();
    let panel = expedition.clone();
    app.world.insert_resource(UnhandledKey::default());
    app.on_event(Key::EVENT, unhandled_key::callback());
    drop(model);
    drop(expedition);

    let before = observer.read(PictureModel::cursor);
    crate::input::event::dispatch_input(
        &mut app.world,
        Some(root),
        &crate::test_support::physical_key(PhysicalKey::KeyD).into(),
        0,
        480,
        320,
    );
    let moved = observer.read(PictureModel::cursor);
    assert_ne!(moved, before);
    assert_eq!(app.world.resource::<UnhandledKey>().unwrap().0, None);

    panel.open_rules();
    crate::input::event::dispatch_input(
        &mut app.world,
        Some(root),
        &crate::test_support::physical_key(PhysicalKey::KeyA).into(),
        1,
        480,
        320,
    );
    assert_eq!(observer.read(PictureModel::cursor), moved);
    assert_eq!(
        app.world.resource::<UnhandledKey>().unwrap().0,
        Some(PhysicalKey::KeyA)
    );

    panel.close();
    crate::input::event::dispatch_input(
        &mut app.world,
        Some(root),
        &crate::test_support::physical_key(PhysicalKey::KeyQ).into(),
        2,
        480,
        320,
    );
    assert_eq!(observer.read(PictureModel::cursor), moved);
    assert_eq!(
        app.world.resource::<UnhandledKey>().unwrap().0,
        Some(PhysicalKey::KeyQ)
    );
}

#[test]
fn model_changes_drive_board_result_and_bounded_text() {
    let mut world = fixture();
    let (model, expedition) = handles(&world);
    let surface = world.find_by_id("picture_surface").unwrap();
    world.remove::<VisualDirty>(surface);
    let revision = model.visual_revision();

    model.reveal_hint();
    flush(&mut world);
    assert!(model.visual_revision() > revision);
    assert!(world.has::<VisualDirty>(surface));

    for _ in 1..25 {
        model.reveal_hint();
    }
    flush(&mut world);
    assert!(model.progress().completed(0));
    assert!(!world.has::<Hidden>(world.find_by_id("picture_result").unwrap()));
    expedition.open(model.level_index());
    flush(&mut world);
    assert!(visible_interactive_descendant(
        &world,
        world.find_by_id("picture_map_level_1").unwrap()
    ));

    model.restart();
    expedition.open_briefing();
    flush(&mut world);
    assert!(!world.has::<Hidden>(world.find_by_id("picture_briefing").unwrap()));
    expedition.open_summary();
    flush(&mut world);
    assert!(!world.has::<Hidden>(world.find_by_id("picture_summary").unwrap()));
    assert!(
        world
            .get::<Text>(world.find_by_id("picture_summary_line_0").unwrap())
            .unwrap()
            .resolve(world.resource())
            .contains("1/6")
    );

    for id in [
        "picture_level",
        "picture_progress",
        "picture_status",
        "picture_map_summary",
        "picture_result_stats",
        "picture_briefing_title",
        "picture_summary_line_0",
        "pic_row_0",
        "pic_col_0",
    ] {
        let text = world.get::<Text>(world.find_by_id(id).unwrap()).unwrap();
        assert!(text.capacity().is_some(), "{id}");
        assert!(text.has_valid_content(), "{id}");
        assert_eq!(text.last_content_error(), None, "{id}");
    }
}

#[test]
fn first_picture_matches_reference_geometry() {
    let model = PictureModel::default();
    let geometry = board_geometry(&model);
    assert_eq!(
        (geometry.x, geometry.y, geometry.cell, geometry.size),
        (100, 66, 44, 220)
    );
}

fn text_of(world: &World, id: &'static str) -> alloc::string::String {
    let text = world.get::<Text>(world.find_by_id(id).unwrap()).unwrap();
    assert!(text.has_valid_content(), "{id}");
    assert_eq!(text.last_content_error(), None, "{id}");
    text.resolve(world.resource()).into_owned()
}

fn assert_texts_fit(world: &World, context: &str) {
    for fit in crate::gallery::demos::text_fits(world) {
        assert!(
            !fit.overflows(4),
            "{context}: {:?} needs {:?} in {:?}",
            fit.text,
            fit.lines,
            fit.rect
        );
    }
}

fn target_cells(model: &PictureModelHandle) -> alloc::vec::Vec<u8> {
    ModelHandle::read(model, |model| {
        let level = model.level();
        let size = level.size();
        (0..size * size)
            .filter(|cell| level.target(*cell) && !level.is_given(*cell))
            .collect()
    })
}

#[test]
fn hints_name_the_cell_and_a_clean_solve_earns_three_stars() {
    use crate::gallery::play::picture::PictureTool;

    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.set_root(root);
    app.render().unwrap();
    let (model, _expedition) = handles(&app.world);
    assert_eq!(
        text_of(&app.world, "picture_status"),
        "Fill cells to match the numbers on the edges"
    );
    assert_eq!(text_of(&app.world, "picture_progress"), "5×5");

    model.reveal_hint();
    flush(&mut app.world);
    let status = text_of(&app.world, "picture_status");
    assert!(
        status.starts_with("Revealed row ") && status.contains(", column "),
        "{status}"
    );
    assert_eq!(text_of(&app.world, "picture_progress"), "5×5 · HINT 1");
    model.undo();
    model.restart();

    model.set_tool(PictureTool::Fill);
    let size = ModelHandle::read(&model, |model| model.level().size());
    for cell in target_cells(&model) {
        model.apply_cell(cell);
    }
    flush(&mut app.world);
    app.render().unwrap();
    assert_eq!(text_of(&app.world, "picture_status"), "Atlas restored");
    assert_eq!(
        text_of(&app.world, "picture_result_stats"),
        alloc::format!("Hints 0 · {size}×{size}")
    );
    assert_eq!(text_of(&app.world, "picture_next"), "Next →");
    assert_texts_fit(&app.world, "A01 result");
}

#[test]
fn every_picture_size_fits_its_clues_and_readouts() {
    use crate::gallery::play::picture::PictureTool;

    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.set_root(root);
    let (model, expedition) = handles(&app.world);
    for locale in [Locale::EnUs, Locale::ZhCn] {
        app.world.resource::<I18n>().unwrap().set_locale(locale);
        model.select_level(0);
        for level in 0..36u8 {
            assert_eq!(model.level_index(), level);
            flush(&mut app.world);
            app.render().unwrap();
            assert_texts_fit(&app.world, &alloc::format!("A{:02}", level + 1));
            model.set_tool(PictureTool::Fill);
            for cell in target_cells(&model) {
                model.apply_cell(cell);
            }
            let final_level = level == 35;
            assert_eq!(
                model.modal(),
                if final_level {
                    ExpeditionModal::Final
                } else {
                    ExpeditionModal::Result
                }
            );
            flush(&mut app.world);
            app.render().unwrap();
            assert_texts_fit(&app.world, &alloc::format!("A{:02} result", level + 1));
            model.continue_campaign();
            expedition.close();
        }
    }
}

#[test]
fn chinese_mode_relabels_the_tools_and_panels() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.set_root(root);
    app.render().unwrap();
    let (model, expedition) = handles(&app.world);
    assert_eq!(text_of(&app.world, "picture_title"), "ATLAS RESTORATION");
    assert_eq!(text_of(&app.world, "picture_fill"), "■ Fill");

    app.world
        .resource::<I18n>()
        .unwrap()
        .set_locale(Locale::ZhCn);
    flush(&mut app.world);
    app.render().unwrap();
    assert_texts_fit(&app.world, "zh play");
    assert_eq!(text_of(&app.world, "picture_title"), "星图修复局");
    assert_eq!(text_of(&app.world, "picture_fill"), "■ 填色");
    assert_eq!(text_of(&app.world, "picture_check"), "检查");
    assert_eq!(text_of(&app.world, "picture_status"), "按边上的数字填格子");
    assert_eq!(text_of(&app.world, "picture_chapter"), "初次落笔");
    for open in [0, 1, 2, 3] {
        match open {
            0 => expedition.open(model.level_index()),
            1 => expedition.open_rules(),
            2 => expedition.open_briefing(),
            _ => expedition.open_summary(),
        }
        flush(&mut app.world);
        app.render().unwrap();
        assert_texts_fit(&app.world, "zh panel");
    }
    assert_eq!(
        text_of(&app.world, "picture_briefing_title"),
        "第 1 章 · 初次落笔"
    );
}

#[test]
fn matched_lines_dim_and_a_check_frames_the_wrong_cells() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.set_root(root);
    app.tick();
    let (model, expedition) = handles(&app.world);
    assert!(expedition.banner(), "a picture starts with its name");

    let row = ModelHandle::read(&model, |model| {
        let level = model.level();
        let size = level.size();
        (0..size)
            .find(|row| (0..size).any(|offset| level.target(row * size + offset)))
            .unwrap()
    });
    let color = |app: &App<_, _>, id| {
        app.world
            .get::<Style>(app.world.find_by_id(id).unwrap())
            .map(|style| style.text_color)
    };
    let id = [
        "pic_row_0",
        "pic_row_1",
        "pic_row_2",
        "pic_row_3",
        "pic_row_4",
    ][usize::from(row)];
    let before = color(&app, id);
    let cells: alloc::vec::Vec<u8> = ModelHandle::read(&model, |model| {
        let size = model.level().size();
        (0..size)
            .map(|offset| row * size + offset)
            .filter(|cell| model.level().target(*cell) && !model.level().is_given(*cell))
            .collect()
    });
    for cell in cells {
        model.apply_cell(cell);
    }
    app.tick();
    assert!(!expedition.banner(), "a stroke clears the banner");
    assert_ne!(color(&app, id), before, "a matched row dims its clues");

    let wrong = ModelHandle::read(&model, |model| {
        let level = model.level();
        (0..level.size() * level.size())
            .find(|cell| !level.target(*cell) && !level.is_given(*cell))
            .unwrap()
    });
    model.apply_cell(wrong);
    model.check();
    app.tick();
    assert_eq!(
        text_of(&app.world, "picture_status"),
        "1 cell is wrong, now framed"
    );
    model.undo();
    model.restart();

    for cell in target_cells(&model) {
        model.apply_cell(cell);
    }
    for _ in 0..60 {
        app.tick();
    }
    let row = app.world.find_by_id("picture_result_stars").unwrap();
    let stars = *app.world.get::<ResultStars>(row).unwrap();
    assert_eq!(stars.earned(), 3);
    assert_eq!(stars.scales(), [Fixed::ONE; 3]);
}
