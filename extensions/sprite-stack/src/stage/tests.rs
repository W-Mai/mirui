use super::*;
use crate::{HeightField, TerrainCell, TerrainGrid};
use mirui::core::model::{ModelItemId, ModelList};
use mirui::{core::cache::Handle, ecs::CommandError, prelude::*};

#[mirui::model]
struct Car {
    #[observe]
    pose: Pose,
}

#[mirui::model]
impl Car {
    fn move_to(&mut self, pose: Pose) {
        self.pose = pose;
    }
}

#[mirui::model]
struct Fleet {
    cars: model!(owned ModelList<Car>),
}

#[mirui::model]
impl Fleet {
    fn remove_rejected(&mut self, item: ModelItemId) -> Result<(), ()> {
        assert!(self.cars.remove(item).is_some());
        Err(())
    }
}

#[mirui::component]
struct Pilot;

struct Operation {
    stage: StackStageHandle,
    edit: Option<Edit>,
}

enum Edit {
    Spawn(StackVisualInput),
    Attach(Entity, StackVisualInput),
    Transfer(Entity),
    Leave(Entity),
    Despawn(Entity),
}

#[mirui::system]
fn edit_stage(edit: res!(mut Operation), commands: commands!()) {
    let Some(operation) = edit.edit.take() else {
        return;
    };
    match operation {
        Edit::Spawn(visual) => commands.stage(&edit.stage).spawn(Pilot, visual),
        Edit::Attach(entity, visual) => commands.stage(&edit.stage).attach(entity, visual),
        Edit::Transfer(entity) => commands.stage(&edit.stage).transfer(entity),
        Edit::Leave(entity) => commands.remove::<StackVisual>(entity),
        Edit::Despawn(entity) => commands.despawn(entity),
    }
}

fn geometry() -> Handle<HeightField> {
    const COLORS: [Color; 1] = [Color::rgb(255, 255, 255)];
    Handle::new(
        HeightField::generate(
            TerrainGrid::finite([1; 2], Fixed::ONE).unwrap(),
            &COLORS,
            |_| TerrainCell {
                level: 0,
                material: 0,
            },
        )
        .unwrap(),
    )
}

fn apply<B: Surface, F: RendererFactory<B>>(
    app: &mut App<B, F>,
    stage: &StackStageHandle,
    edit: Edit,
) -> Result<(), CommandError> {
    app.add_resource(Operation {
        stage: stage.clone(),
        edit: Some(edit),
    });
    let result = app
        .systems
        .run_all(&mut app.world)
        .map_err(|error| error.cause);
    mirui::core::reactive::flush_signal_dirty(&mut app.world);
    result
}

#[test]
fn collection_retirement_follows_the_entity_and_preserves_independent_entities() {
    let mut app = App::headless(32, 32);
    app.add_system(edit_stage::system());
    let road = app.add_model(StackStage::bounded(5)).finish();
    let garage = app.add_model(StackStage::bounded(1)).finish();
    let mut cars = ModelList::bounded(3);
    assert!(
        cars.try_push(Car {
            pose: Pose::at(0, 0, 0)
        })
        .is_ok()
    );
    assert!(
        cars.try_push(Car {
            pose: Pose::at(1, 0, 0)
        })
        .is_ok()
    );
    let fleet = app.add_model(Fleet { cars: cars.into() }).finish();
    let cars = fleet.cars();
    let mut entries = cars.entries();
    let (removed_id, removed) = entries.next().unwrap();
    let (retained_id, retained) = entries.next().unwrap();
    drop(entries);
    let independent = app
        .add_model(Car {
            pose: Pose::at(2, 0, 0),
        })
        .finish();
    let unrelated = app.world.spawn_empty();
    let existing = app.world.spawn_empty();
    let field = geometry();
    let calls = [Rc::new(Cell::new(0)), Rc::new(Cell::new(0))];
    let visual = |model: &CarHandle, calls: Option<Rc<Cell<usize>>>| {
        let field = field.clone();
        StackVisual::from_model(model, move |car| {
            if let Some(calls) = &calls {
                calls.set(calls.get() + 1);
            }
            StackVisual::new(field.clone()).at(car.pose())
        })
    };
    apply(
        &mut app,
        &road,
        Edit::Spawn(visual(&removed, Some(calls[0].clone()))),
    )
    .unwrap();
    let (derived, _) = app.world.query::<Pilot>().iter().next().unwrap();
    apply(
        &mut app,
        &road,
        Edit::Spawn(visual(&retained, Some(calls[1].clone()))),
    )
    .unwrap();
    apply(
        &mut app,
        &road,
        Edit::Attach(existing, visual(&removed, None)),
    )
    .unwrap();
    apply(&mut app, &road, Edit::Spawn(visual(&independent, None))).unwrap();
    apply(
        &mut app,
        &road,
        Edit::Attach(unrelated, StackVisual::new(field.clone()).into()),
    )
    .unwrap();
    assert_eq!(road.len(), 5);

    cars.swap(0, 1).unwrap();
    mirui::core::reactive::flush_signal_dirty(&mut app.world);
    assert_eq!(calls.each_ref().map(|c| c.get()), [1, 1]);
    retained.move_to(Pose::at(4, 0, 0));
    mirui::core::reactive::flush_signal_dirty(&mut app.world);
    assert_eq!(calls.each_ref().map(|c| c.get()), [1, 2]);
    apply(&mut app, &garage, Edit::Transfer(derived)).unwrap();

    assert_eq!(fleet.remove_rejected(removed_id), Err(()));
    assert!(cars.get_by_id(removed_id).is_none());
    assert!(!app.tick());
    assert_eq!([road.len(), garage.len()], [3, 0]);
    assert!(!app.world.is_alive(derived));
    assert!(app.world.is_alive(existing));
    assert!(!app.world.has::<StackVisual>(existing));
    assert!(app.world.has::<StackVisual>(unrelated));
    assert!(ModelHandle::callback_ready(&independent));
    assert_eq!(calls.each_ref().map(|c| c.get()), [1, 2]);

    let replacement_id = cars
        .try_push(Car {
            pose: Pose::at(5, 0, 0),
        })
        .ok()
        .unwrap();
    assert_ne!(replacement_id, removed_id);
    let replacement = cars.get_by_id(replacement_id).unwrap();
    apply(&mut app, &garage, Edit::Spawn(visual(&replacement, None))).unwrap();
    assert!(cars.remove(retained_id).is_ok());
    assert!(!app.tick());
    assert_eq!([road.len(), garage.len()], [2, 1]);
    assert!(ModelHandle::callback_ready(&replacement));
    assert_eq!(calls.each_ref().map(|c| c.get()), [1, 2]);
}

#[test]
fn stage_commands_preserve_identity_and_release_the_model_consumer() {
    let mut app = App::headless(32, 32);
    app.add_system(edit_stage::system());
    let road = app.add_model(StackStage::bounded(2)).finish();
    let garage = app.add_model(StackStage::bounded(1)).finish();
    let car = app
        .add_model(Car {
            pose: Pose::at(2, 0, 0),
        })
        .finish();
    let geometry = geometry();
    let calls = Rc::new(core::cell::Cell::new(0));
    let visual = {
        let calls = calls.clone();
        let geometry = geometry.clone();
        StackVisual::from_model(&car, move |car| {
            calls.set(calls.get() + 1);
            StackVisual::new(geometry.clone()).at(car.pose())
        })
    };
    apply(&mut app, &road, Edit::Spawn(visual)).unwrap();
    assert_eq!(road.len(), 1);
    assert_eq!(calls.get(), 1);
    let (player, _) = app.world.query::<Pilot>().iter().next().unwrap();
    assert!(!app.world.has::<Widget>(player));
    car.move_to(Pose::at(3, 0, 0));
    mirui::core::reactive::flush_signal_dirty(&mut app.world);
    assert_eq!(
        app.world.get::<StackVisual>(player).unwrap().pose(),
        Pose::at(3, 0, 0)
    );

    let blocker = app.world.spawn_empty();
    apply(
        &mut app,
        &garage,
        Edit::Attach(blocker, StackVisual::new(geometry.clone()).into()),
    )
    .unwrap();
    let before = (road.revision(), garage.revision());
    assert_eq!(
        apply(&mut app, &garage, Edit::Transfer(player)),
        Err(CommandError::Commit(
            CommandCommitError::OperationRejected {
                operation: "stack stage capacity"
            }
        ))
    );
    assert_eq!((road.revision(), garage.revision()), before);
    assert_eq!([road.len(), garage.len()], [1, 1]);
    apply(&mut app, &garage, Edit::Leave(blocker)).unwrap();
    assert!(app.world.is_alive(blocker));
    apply(&mut app, &garage, Edit::Transfer(player)).unwrap();
    assert_eq!([road.len(), garage.len()], [0, 1]);
    assert!(app.world.has::<Pilot>(player));
    car.move_to(Pose::at(4, 0, 0));
    mirui::core::reactive::flush_signal_dirty(&mut app.world);
    assert_eq!(
        app.world.get::<StackVisual>(player).unwrap().pose(),
        Pose::at(4, 0, 0)
    );

    let before = calls.get();
    apply(&mut app, &garage, Edit::Despawn(player)).unwrap();
    assert!(garage.is_empty());
    let replacement = app.world.spawn_empty();
    assert_eq!(replacement.id, player.id);
    assert_ne!(replacement.generation, player.generation);
    assert_eq!(
        apply(
            &mut app,
            &road,
            Edit::Attach(player, StackVisual::new(geometry).into())
        ),
        Err(CommandError::Commit(CommandCommitError::MissingTarget {
            entity: player
        }))
    );
    car.move_to(Pose::at(5, 0, 0));
    mirui::core::reactive::flush_signal_dirty(&mut app.world);
    assert_eq!(calls.get(), before);
    assert!(!app.world.has::<StackVisual>(replacement));
}

#[test]
fn foreign_stages_and_model_sources_are_rejected_before_mounting() {
    let mut app = App::headless(32, 32);
    let mut foreign = App::headless(32, 32);
    app.add_system(edit_stage::system());
    let stage = app.add_model(StackStage::bounded(2)).finish();
    let other = foreign.add_model(StackStage::bounded(2)).finish();
    let foreign_car = foreign
        .add_model(Car {
            pose: Pose::default(),
        })
        .finish();
    assert_eq!(
        apply(
            &mut app,
            &other,
            Edit::Spawn(StackVisual::new(geometry()).into())
        ),
        Err(CommandError::Commit(CommandCommitError::ForeignOwner {
            service: "stack stage"
        }))
    );
    let visual =
        StackVisual::from_model(&foreign_car, |_| panic!("foreign getter must stay unread"));
    assert_eq!(
        apply(&mut app, &stage, Edit::Spawn(visual)),
        Err(CommandError::Commit(CommandCommitError::ForeignOwner {
            service: "stack visual model"
        }))
    );
    assert!(stage.is_empty());
    assert!(other.is_empty());
    assert!(app.world.query::<Pilot>().iter().next().is_none());
}

#[mirui::compose]
fn viewports(
    first: Signal<StackStageHandle>,
    stage: model!(StackStage),
    other: model!(StackStage),
) -> Entity {
    use crate::{Camera, StackViewport};
    ui! {
        Row {
            StackViewport(stage: ${first.get()}, camera: Camera::overhead(), width: 16, height: 16)
            StackViewport(stage: stage, camera: Camera::overhead(), width: 16, height: 16)
            StackViewport(stage: other, camera: Camera::overhead(), width: 16, height: 16)
        }
    }
}

#[test]
fn two_viewports_draw_stage_updates_without_dirtying_another_stage() {
    let mut app = App::headless(48, 16);
    app.with_default_widgets()
        .add_plugin(crate::SpriteStackPlugin);
    app.add_system(edit_stage::system());
    let stage = app.add_model(StackStage::bounded(1)).finish();
    let other = app.add_model(StackStage::bounded(1)).finish();
    let car = app
        .add_model(Car {
            pose: Pose::at(0, 0, 0),
        })
        .finish();
    let field = geometry();
    let visual = StackVisual::from_model(&car, move |car| {
        StackVisual::new(field.clone()).at(car.pose())
    });
    apply(&mut app, &stage, Edit::Spawn(visual)).unwrap();
    let root = app.spawn_root().id();
    let first_stage = Signal::new(stage.clone());
    let row = app.compose(
        root,
        viewports(first_stage.clone(), stage.clone(), other.clone()),
    );
    app.render().unwrap();
    let before = |app: &mut App<_>, offset| {
        let image = app.snapshot().unwrap().unwrap();
        let pixels = image.read_pixels().unwrap();
        (0..16)
            .flat_map(|y| (offset..offset + 16).map(move |x| (x, y)))
            .filter(|&(x, y)| pixels.get_pixel(x, y) == Color::rgb(255, 255, 255))
            .map(|(x, y)| (x - offset, y))
            .collect::<Vec<_>>()
    };
    let left = before(&mut app, 0);
    let right = before(&mut app, 16);
    assert!(!left.is_empty() && !right.is_empty());
    car.move_to(Pose::at(5, 0, 0));
    assert_eq!(
        app.dirty_regions().unwrap().rects,
        [Rect::new(0, 0, 32, 16)]
    );
    app.render_dirty().unwrap();
    assert_ne!(before(&mut app, 0), left);
    assert_ne!(before(&mut app, 16), right);

    first_stage.set(other.clone());
    app.render_dirty().unwrap();
    assert!(before(&mut app, 0).is_empty());
    car.move_to(Pose::at(1, 0, 0));
    assert_eq!(
        app.dirty_regions().unwrap().rects,
        [Rect::new(16, 0, 16, 16)]
    );
    app.render_dirty().unwrap();
    assert!(before(&mut app, 0).is_empty());
    assert!(!before(&mut app, 16).is_empty());

    apply(
        &mut app,
        &other,
        Edit::Spawn(StackVisual::new(geometry()).into()),
    )
    .unwrap();
    assert_eq!(
        app.dirty_regions().unwrap().rects,
        [Rect::new(0, 0, 16, 16), Rect::new(32, 0, 16, 16)]
    );
    app.render_dirty().unwrap();
    assert_eq!(before(&mut app, 0), left);
    assert_eq!(before(&mut app, 32), right);

    first_stage.set(stage.clone());
    app.render_dirty().unwrap();
    assert!(!before(&mut app, 0).is_empty());

    let first = app.world.get::<mirui::ui::Children>(row).unwrap().0[0];
    apply(&mut app, &stage, Edit::Despawn(first)).unwrap();
    assert_eq!(stage.len(), 1);
    car.move_to(Pose::at(3, 0, 0));
    app.render_dirty().unwrap();
    assert!(!before(&mut app, 0).is_empty());
    assert_eq!(before(&mut app, 16), right);
}

#[test]
fn a_bound_visual_tracks_the_current_live_geometry_until_detached() {
    const COLORS: [Color; 2] = [Color::rgb(180, 30, 40), Color::rgb(15, 180, 30)];
    let mut app = App::headless(48, 16);
    app.with_default_widgets()
        .add_plugin(crate::SpriteStackPlugin);
    app.add_system(edit_stage::system());
    let stage = app.add_model(StackStage::bounded(1)).finish();
    let other = app.add_model(StackStage::bounded(0)).finish();
    let field = |material| {
        HeightField::generate(
            TerrainGrid::finite([1, 1], Fixed::from_int(4)).unwrap(),
            &COLORS,
            |_| TerrainCell { level: 0, material },
        )
        .unwrap()
    };
    let red = app.add_model(field(0)).finish();
    let green = app.add_model(field(1)).finish();
    let car = app
        .add_model(Car {
            pose: Pose::default(),
        })
        .finish();
    let visual = StackVisual::from_model(&car, {
        let (red, green) = (red.clone(), green.clone());
        move |car| {
            let selection = car.pose().position[0];
            let field = if selection.is_zero() { &red } else { &green };
            let visual = StackVisual::new(field.clone());
            if selection.is_negative() {
                visual.with_palette(&[[0; 4]])
            } else {
                visual
            }
        }
    });
    apply(&mut app, &stage, Edit::Spawn(visual)).unwrap();
    let root = app.spawn_root().id();
    app.compose(
        root,
        viewports(Signal::new(stage.clone()), stage.clone(), other),
    );
    let sample = |app: &mut App<_>| {
        let frame = app.snapshot().unwrap().unwrap();
        let pixels = frame.read_pixels().unwrap();
        [0, 16].map(|left| {
            (0..16)
                .flat_map(|y| (left..left + 16).map(move |x| (x, y)))
                .map(|(x, y)| pixels.get_pixel(x, y))
                .find(|color| COLORS.contains(color))
        })
    };
    let set = |field: &crate::HeightFieldHandle, material| {
        field
            .set_cell([0, 0], TerrainCell { level: 0, material })
            .unwrap();
    };
    assert_eq!(sample(&mut app), [Some(COLORS[0]); 2]);
    set(&red, 1);
    assert_eq!(sample(&mut app), [Some(COLORS[1]); 2]);
    set(&red, 0);
    car.move_to(Pose::at(1, 0, 0));
    assert_eq!(sample(&mut app), [Some(COLORS[1]); 2]);
    let revision = stage.revision();
    set(&red, 1);
    assert_eq!(sample(&mut app), [Some(COLORS[1]); 2]);
    assert_eq!(stage.revision(), revision);
    set(&green, 0);
    assert_eq!(sample(&mut app), [Some(COLORS[0]); 2]);

    let revision = stage.revision();
    car.move_to(Pose::at(-1, 0, 0));
    assert_eq!(sample(&mut app), [Some(COLORS[0]); 2]);
    assert_eq!(stage.revision(), revision);
    set(&green, 1);
    assert_eq!(sample(&mut app), [Some(COLORS[1]); 2]);

    let entity = stage.read(|stage| stage.with_entities(|members| members[0]));
    apply(&mut app, &stage, Edit::Leave(entity)).unwrap();
    let revision = stage.revision();
    set(&green, 1);
    assert_eq!(sample(&mut app), [None; 2]);
    assert_eq!(stage.revision(), revision);
}

#[test]
fn retired_height_field_disappears_from_both_viewports_and_bound_member_can_recover() {
    const COLORS: [Color; 1] = [Color::rgb(255, 255, 255)];
    let mut app = App::headless(48, 16);
    app.with_default_widgets()
        .add_plugin(crate::SpriteStackPlugin);
    app.add_system(edit_stage::system());
    let stage = app.add_model(StackStage::bounded(2)).finish();
    let other = app.add_model(StackStage::bounded(0)).finish();
    let mut fields = ModelList::bounded(2);
    let make_field = || {
        HeightField::generate(
            TerrainGrid::finite([1, 1], Fixed::ONE).unwrap(),
            &COLORS,
            |_| TerrainCell {
                level: 0,
                material: 0,
            },
        )
        .unwrap()
    };
    assert!(fields.try_push(make_field()).is_ok());
    assert!(fields.try_push(make_field()).is_ok());
    let fields = app.add_model(fields).finish();
    let mut entries = fields.entries();
    let (old_id, old) = entries.next().unwrap();
    let (_, replacement) = entries.next().unwrap();
    drop(entries);
    let actor = app
        .add_model(Car {
            pose: Pose::at(4, 0, 0),
        })
        .finish();
    let selected = Signal::new(old.clone());
    apply(
        &mut app,
        &stage,
        Edit::Spawn(StackVisual::new(old).at(Pose::at(-4, 0, 0)).into()),
    )
    .unwrap();
    apply(
        &mut app,
        &stage,
        Edit::Spawn(StackVisual::from_model(&actor, {
            let selected = selected.clone();
            move |actor| StackVisual::new(selected.get()).at(actor.pose())
        })),
    )
    .unwrap();
    let root = app.spawn_root().id();
    app.compose(
        root,
        viewports(Signal::new(stage.clone()), stage.clone(), other),
    );
    let visible = |app: &mut App<_>| {
        let frame = app.snapshot().unwrap().unwrap();
        let pixels = frame.read_pixels().unwrap();
        [0, 16].map(|offset| [4, 12].map(|x| pixels.get_pixel(offset + x, 7) == COLORS[0]))
    };
    assert_eq!(visible(&mut app), [[true, true]; 2]);
    app.render_dirty().unwrap();
    assert!(fields.remove(old_id).unwrap().is_some());
    assert_eq!(
        app.dirty_regions().unwrap().rects,
        [Rect::new(0, 0, 32, 16)]
    );
    app.render_dirty().unwrap();
    assert_eq!(visible(&mut app), [[false, false]; 2]);
    assert_eq!(stage.len(), 2);
    selected.set(replacement);
    assert_eq!(visible(&mut app), [[false, true]; 2]);
}
