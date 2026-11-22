use super::super::*;
use crate::{flight::cockpit_center, jet, world::World};
use mirui_sprite_stack::{
    format::PreparedSpriteStack,
    raster::{
        CameraWorkspace, FrameOverlay, FramePass, draw_layers,
        sprites::{ProjectedStack, SpritePaint, StackTransform},
    },
};

struct ReferenceView {
    projection: Projection,
    source_size: [usize; 2],
    height: usize,
    compact: bool,
}

pub(crate) struct Raster {
    compact: bool,
    workspace: CameraWorkspace,
    pub(crate) scene: FlightScene,
}

impl PlacedAircraft {
    pub(crate) fn from_flight(flight: &Flight) -> [Option<Self>; 3] {
        core::array::from_fn(|i| {
            (i < flight.count).then(|| {
                let jet = &flight.jets[i];
                Self::new(jet.position, [jet.yaw, jet.pitch, jet.roll + jet.spin()])
            })
        })
    }
}

impl<'a> Scene<'a> {
    pub(super) fn new(
        placed: &Placements,
        ground: Ground<'a>,
        flight: &'a Flight,
        palette: &'a Palette,
        frame_height: usize,
        projection: Projection,
    ) -> Self {
        Self::with_heading(
            placed,
            ground,
            flight,
            palette,
            frame_height,
            projection,
            projection.yaw.sin_cos(),
        )
    }

    pub(super) fn draw(&self, canvas: &mut SceneCanvas<'_, '_>) -> [usize; 3] {
        let order = self.order();
        let prepared = self.prepare([canvas.color.width, canvas.color.height], [&jet::MODEL; 3]);
        self.draw_ordered::<false>(canvas, &order, &prepared, Fixed::MAX)
            .unwrap();
        order
    }

    pub(super) fn prepare(
        &self,
        size: [usize; 2],
        models: [&'static PreparedSpriteStack<'static>; 3],
    ) -> [Option<PreparedJet>; 3] {
        core::array::from_fn(|index| {
            PreparedJet::new(
                self.flight,
                index,
                (self.placed[index]?, models[index]),
                &self.projection,
                self.heading,
                self.palette,
                size,
            )
        })
    }

    pub(super) fn draw_ordered<const DEPTH_ONLY: bool>(
        &self,
        canvas: &mut SceneCanvas<'_, '_>,
        order: &[usize; 3],
        prepared: &[Option<PreparedJet>; 3],
        limit: Fixed,
    ) -> Result<(), RenderError> {
        let layers = FlightLayers {
            scene: self,
            order,
            prepared,
        };
        let pass = if DEPTH_ONLY {
            FramePass::Depth { limit }
        } else {
            FramePass::Color
        };
        draw_layers(&layers, canvas, pass)
    }
}

struct FlightLayers<'a> {
    scene: &'a Scene<'a>,
    order: &'a [usize; 3],
    prepared: &'a [Option<PreparedJet>; 3],
}

impl SceneLayers for FlightLayers<'_> {
    fn terrain(&self, canvas: &mut SceneCanvas<'_, '_>, limit: Option<Fixed>) {
        self.scene.terrain(canvas, limit);
    }

    fn background(&self, canvas: &mut SceneCanvas<'_, '_>) {
        self.scene.background(canvas);
    }

    fn effects_behind_sprites(&self, canvas: &mut SceneCanvas<'_, '_>) {
        self.scene.effects_behind_sprites(canvas);
    }

    fn sprites(&self, canvas: &mut SceneCanvas<'_, '_>) {
        let scene = self.scene;
        for i in &self.order[..scene.flight.count] {
            if let Some(jet) = &self.prepared[*i] {
                jet.draw(canvas, &scene.projection, scene.palette);
            }
        }
    }
}

struct CockpitOverlay<'a> {
    cockpit: &'a crate::cockpit::Cockpit,
    prepared: crate::cockpit::PreparedCockpit,
    world: Ground<'a>,
}

impl FrameOverlay for CockpitOverlay<'_> {
    fn covers(&self, rect: [i32; 4], scratch: &mut [u8]) -> bool {
        self.cockpit.covers(rect, &self.prepared, scratch)
    }

    fn paint(&self, canvas: &mut Canvas<'_>) {
        self.cockpit
            .draw_prepared(canvas, self.world, &self.prepared);
    }
}

fn map_projection(
    mut projection: Projection,
    width: usize,
    height: usize,
) -> (Projection, RollMapping) {
    let rotation = (projection.cockpit > Fixed::ZERO || projection.roll != Fixed::ZERO)
        .then(|| projection.roll.sin_cos());
    let roll = RollMapping::new([width, height], cockpit_center(height), rotation);
    if roll.rolled {
        let axis = Fixed::from_int(width as i32) / 2;
        projection.camera.center = roll.projection_center([axis, projection.camera.center[1]]);
    }
    (projection, roll)
}

impl Raster {
    pub fn new(size: [usize; 2], compact: bool) -> Result<Self, RenderError> {
        Ok(Self {
            compact,
            workspace: CameraWorkspace::new(size, cockpit_center(size[1]), compact)?,
            scene: FlightScene::new(Palette::shared(Daylight::Day)),
        })
    }
    pub fn bytes(&self) -> usize {
        self.workspace.bytes() + core::mem::size_of::<Self>()
    }
    pub(crate) fn render(
        &mut self,
        frame: &mut Frame,
        world: &World,
        flight: &Flight,
        placed: &Placements,
    ) {
        let projection = flight.projection(frame.width, frame.height);
        self.render_scene(
            frame,
            flight,
            (projection, None),
            world.at(Some(Default::default())),
            placed,
        )
        .unwrap();
    }

    pub(crate) fn render_with_cockpit(
        &mut self,
        frame: &mut Frame,
        world: &World,
        flight: &Flight,
        cockpit: &crate::cockpit::Cockpit,
        placed: &Placements,
    ) {
        let projection = flight.projection(frame.width, frame.height);
        let ground = world.at(Some(Default::default()));
        let overlay = cockpit
            .prepare(flight, projection, self.scene.palette)
            .map(|prepared| CockpitOverlay {
                cockpit,
                prepared,
                world: ground,
            });
        self.render_scene(
            frame,
            flight,
            (
                projection,
                overlay.as_ref().map(|overlay| overlay as &dyn FrameOverlay),
            ),
            ground,
            placed,
        )
        .unwrap();
    }

    pub(super) fn render_scene(
        &mut self,
        frame: &mut Frame,
        flight: &Flight,
        (screen, overlay): (Projection, Option<&dyn FrameOverlay>),
        ground: Ground<'_>,
        placed: &Placements,
    ) -> Result<(), RenderError> {
        let (projection, mapping) = map_projection(screen, frame.width, frame.height);
        self.scene.prepare_reference(
            flight,
            ReferenceView {
                projection,
                source_size: mapping.size,
                height: frame.height,
                compact: self.compact,
            },
            ground,
            placed,
            |layers, effects| {
                self.workspace
                    .render_layers(frame, &mapping, overlay, layers, effects)
            },
        )
    }
}

impl FlightScene {
    fn prepare_reference(
        &mut self,
        flight: &Flight,
        view: ReferenceView,
        ground: Ground<'_>,
        placed: &Placements,
        draw: impl FnOnce(&FlightLayers<'_>, &mut PreparedExhaust<'_>) -> Result<(), RenderError>,
    ) -> Result<(), RenderError> {
        let Self {
            palette,
            flames,
            flame_count,
            seen,
            ..
        } = self;
        let projection = view.projection;
        let heading = projection.yaw.sin_cos();
        let scene = Scene::with_heading(
            placed,
            ground,
            flight,
            palette,
            view.height,
            projection,
            heading,
        );
        let order = scene.order();
        let mut models = scene.prepare(view.source_size, [&jet::MODEL; 3]);
        if view.compact {
            for model in models.iter_mut().flatten() {
                model.prepare_regions(&projection);
            }
        }
        let mut pending = [PendingFlame::default(); 6];
        let mut count = 0;
        for &index in &order[..flight.count] {
            if let Some(jet) = placed[index] {
                project_flames(
                    flight,
                    (index, jet),
                    &projection,
                    scene.heading,
                    &mut pending,
                    &mut count,
                );
            }
        }
        let mut next_seen = *seen;
        draw(
            &FlightLayers {
                scene: &scene,
                order: &order,
                prepared: &models,
            },
            &mut PreparedExhaust {
                queries: pending,
                visibility: &mut next_seen,
                flames: &mut flames[..count],
                time: flight.time,
            },
        )?;

        *seen = next_seen;
        *flame_count = count;
        Ok(())
    }
}

pub(super) struct PreparedJet {
    pub(super) sprite: ProjectedStack<'static, { jet::LAYERS }>,
    fade: Fixed,
    lights: [usize; 2],
    right_side_x: i32,
}
impl PreparedJet {
    pub(super) fn new(
        flight: &Flight,
        index: usize,
        (jet, model): (PlacedAircraft, &'static PreparedSpriteStack<'static>),
        p: &Projection,
        heading: (Fixed, Fixed),
        pal: &Palette,
        size: [usize; 2],
    ) -> Option<Self> {
        mirui::trace_span!("flight.aircraft.prepare");
        let fade = if index == 0 { p.jet_alpha } else { Fixed::ONE };
        if fade < Fixed::from_ratio(4, 1000) {
            return None;
        }
        let center = p.project_with_yaw(jet.position, heading)?;
        if center[2] <= Fixed::ZERO {
            return None;
        }
        let reach =
            Fixed::from_int((model.extent()[1]) as i32) * flight.settings.jet_scale * center[2]
                + Fixed::from_int(40);
        if center[0] < -reach
            || center[1] < -reach - Fixed::from_int((size[1]) as i32)
            || center[0] > Fixed::from_int((size[0]) as i32) + reach
            || center[1] > Fixed::from_int((size[1]) as i32) + reach
        {
            return None;
        }
        let axes = jet.axes;
        let (sn, cs) = heading;
        let sc = flight.settings.jet_scale;
        let project_axis = |a: [Fixed; 3]| [a[0] * cs - a[1] * sn, -(a[0] * sn + a[1] * cs), a[2]];
        let offset = core::array::from_fn(|i| jet.position[i] - p.position[i]);
        let lights = aircraft_lights(axes, pal.sun, pal.day);
        let sprite = ProjectedStack::new(
            model,
            StackTransform {
                scale: sc,
                layer_spacing: flight.spread * Fixed::from_ratio(model.spacing_q8(), 256),
                depth_radius: Fixed::from(model.extent()[1]) * Fixed::from_ratio(6, 10) * sc,
                ..StackTransform::from_basis(offset, axes, project_axis)
            },
            &p.camera,
            Fixed::from_int(3),
            |layer, first_slice| {
                let intro =
                    flight.intro - Fixed::from_int((index) as i32) * Fixed::from_ratio(25, 100);
                let t = (intro
                    - Fixed::from_int((layer.saturating_sub(first_slice)) as i32)
                        * Fixed::from_ratio(7, 100))
                    / Fixed::from_ratio(36, 100);
                if t <= Fixed::ZERO {
                    return None;
                }
                let drop = if t < Fixed::ONE {
                    -(Fixed::ONE - t * t)
                        * (Fixed::from_int((size[1]) as i32) / Fixed::from_int(2)
                            + Fixed::from_int(44))
                } else {
                    Fixed::ZERO
                };
                Some(drop)
            },
        )?;
        let (step, pivot) = (
            sprite.level().step(),
            Fixed::from_ratio(model.pivot_q8()[0], 256),
        );
        let right_side_x = (pivot - (Fixed::from(step) - Fixed::ONE) / 2)
            .ceil()
            .to_int();
        Some(Self {
            sprite,
            fade,
            lights,
            right_side_x,
        })
    }

    pub(super) fn prepare_regions(&mut self, p: &Projection) {
        self.sprite.prepare_regions(&p.camera);
    }
    pub(super) fn draw(&self, canvas: &mut SceneCanvas<'_, '_>, p: &Projection, pal: &Palette) {
        mirui::trace_span!("flight.aircraft");
        self.sprite.view().draw_masked(
            canvas,
            &p.camera,
            SpritePaint {
                depth_bias: Fixed::ONE,
                opacity: self.fade,
                outline: (self.fade >= Fixed::HALF).then_some(pal.outline),
            },
            |voxel| voxel.color | (u8::from(i32::from(voxel.x) >= self.right_side_x) << 7),
            |material| {
                pal.jets[self.lights[usize::from(material >> 7)]][usize::from(material & 0x7f)]
            },
        );
    }
}
