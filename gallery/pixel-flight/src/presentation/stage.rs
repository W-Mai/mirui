use crate::{
    app::input::FlightViewState,
    gameplay::FlightModel,
    world::coordinates::{pose, radians},
};
use crate::{
    jet,
    raster::{Palette, PlacedAircraft, Placements, aircraft_lights},
};
use mirui::{core::model::ModelHandle, prelude::*};
use mirui_sprite_stack::{
    Geometry, LayerEntrance, Pose, SampledStyle, SampledVisual, StackStageCommands, StackVisual,
    raster::sprites::SpritePaint,
};

#[mirui::component]
struct FlightMember {
    owner: Entity,
    slot: Option<usize>,
}

#[mirui::component]
pub(crate) struct FlightTerrain;

pub(super) fn aircraft(model: &FlightModel, slot: usize) -> (Pose, PlacedAircraft) {
    let pose = pose(&model.flight.jets[slot]);
    let [x, altitude, z] = pose.position;
    let placed = PlacedAircraft::new(
        [x, -z, altitude],
        [pose.yaw, pose.pitch, -pose.roll].map(radians),
    );
    (pose, placed)
}

pub(super) fn placements(model: &FlightModel) -> Placements {
    core::array::from_fn(|slot| (slot < model.flight.count).then(|| aircraft(model, slot).1))
}

fn visual(model: &FlightModel, slot: usize) -> SampledVisual {
    let (pose, placed) = aircraft(model, slot);
    let flight = &model.flight;
    let palette = Palette::shared(model.day);
    let opacity = if slot == 0 {
        flight.aircraft_opacity()
    } else {
        Fixed::ONE
    };
    let lights = aircraft_lights(placed.axes, palette.sun, palette.day);
    StackVisual::new(Geometry::Sampled(&jet::MODEL))
        .at(pose)
        .with_sampled_style(SampledStyle {
            axes: placed.axes.map(|[x, horizontal, up]| [x, up, -horizontal]),
            scale: flight.settings.jet_scale,
            layer_spacing: flight.spread * Fixed::from_ratio(jet::MODEL.spacing_q8(), 256),
            depth_radius: Fixed::from(jet::MODEL.extent()[1])
                * Fixed::from_ratio(6, 10)
                * flight.settings.jet_scale,
            max_cell_pixels: Fixed::from_int(3),
            cull_margin: Fixed::from_int(40),
            entrance: LayerEntrance {
                elapsed: flight.intro - Fixed::from_int(slot as i32) * Fixed::from_ratio(25, 100),
                delay_per_layer: Fixed::from_ratio(7, 100),
                duration: Fixed::from_ratio(36, 100),
                viewport_height_scale: Fixed::HALF,
                distance: Fixed::from_int(44),
            },
            paint: SpritePaint {
                depth_bias: Fixed::ONE,
                opacity,
                outline: (opacity >= Fixed::HALF).then_some(palette.outline),
            },
            palettes: lights.map(|index| &palette.jets[index][..jet::MODEL.palette().len()]),
        })
}

#[mirui::system(order = Animation)]
pub(crate) fn sync(
    views: query!(Entity, FlightViewState),
    members: query!(Entity, FlightMember, StackVisual),
    commands: commands!(),
) {
    for (entity, member, visual) in members {
        let Some((_, view)) = views.get(member.owner) else {
            continue;
        };
        let bit = match member.slot {
            None => {
                let Some(pose) = view.model.read(|model| model.ground_pose) else {
                    commands.despawn(entity);
                    continue;
                };
                if visual.pose() != pose {
                    let stage = view.model.stage();
                    let ground = view.model.read(|model| model.terrain.field());
                    commands.remove::<StackVisual>(entity);
                    commands.stage(&stage).attach(
                        entity,
                        StackVisual::new(ground)
                            .at(pose)
                            .with_role::<FlightTerrain>(),
                    );
                }
                1
            }
            Some(slot) if view.model.read(|model| slot < model.flight.count) => 2 << slot,
            Some(_) => {
                commands.despawn(entity);
                continue;
            }
        };
        view.observed_members.set(view.observed_members.get() | bit);
    }
    for (owner, view) in views {
        let model = &view.model;
        let stage = model.stage();
        let (ground_pose, count) = model.read(|model| (model.ground_pose, model.flight.count));
        let present = view.observed_members.replace(0);
        if let Some(pose) = ground_pose.filter(|_| present & 1 == 0) {
            let ground = model.read(|model| model.terrain.field());
            commands.stage(&stage).spawn(
                (FlightMember { owner, slot: None }, FlightTerrain),
                StackVisual::new(ground)
                    .at(pose)
                    .with_role::<FlightTerrain>(),
            );
        }
        for slot in 0..count {
            if present & (2 << slot) == 0 {
                commands.stage(&stage).spawn(
                    FlightMember {
                        owner,
                        slot: Some(slot),
                    },
                    StackVisual::from_model(model, move |model| {
                        model.revision();
                        model.read(|data| visual(data, slot))
                    }),
                );
            }
        }
    }
}
