use super::*;
use crate::{HeightField, TerrainCell, TerrainGrid};
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
    match edit.edit.take().unwrap() {
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
