use crate::{Geometry, Pose, stage::Membership};
use alloc::boxed::Box;
use core::{
    any::TypeId,
    cell::{Cell, OnceCell},
};
use mirui::{
    __macro_support::{despawn_subtree, with_world, with_world_scope},
    core::{
        model::{BindingOwner, ModelHandle},
        reactive::Effect,
    },
    ecs::{CommandCommitError, Component, Entity, World},
    types::{Color, Fixed},
};

mod sampled;
pub use sampled::{LayerEntrance, SampledStyle};

/// Geometry and pose attached to a stage member.
pub struct StackVisual {
    geometry: Geometry,
    pose: Pose,
    material: Material,
    role: Option<fn() -> TypeId>,
    sampled_style: Option<Box<SampledStyle>>,
    pub(crate) membership: Option<Membership>,
}

impl Component for StackVisual {}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Material {
    pub palette: Option<&'static [[u8; 4]]>,
    flash: Fixed,
}

impl Material {
    pub fn is_default(self) -> bool {
        self.palette.is_none() && self.flash.is_zero()
    }

    fn fits(self, geometry: &Geometry) -> bool {
        if self.is_default() {
            return true;
        }
        let Some(palette) = self.palette else {
            return !matches!(geometry, Geometry::Field(_) | Geometry::LiveField(_));
        };
        let palette_size = match geometry {
            Geometry::Sprite(sprite) => sprite.get().atlas().indexed_color_table().map(|p| p.len()),
            Geometry::Prepared(sprite) | Geometry::Sampled(sprite) => Some(sprite.palette().len()),
            Geometry::Generated(sprite) => Some(sprite.data().palette().len()),
            Geometry::Field(_) | Geometry::LiveField(_) => None,
        };
        palette_size == Some(palette.len())
    }

    #[inline]
    pub fn shade(self, [r, g, b, alpha]: [u8; 4], index: Option<usize>) -> u32 {
        let [r, g, b, _] = match (self.palette, index) {
            (Some(palette), Some(i)) => palette[i],
            _ => [r, g, b, alpha],
        };
        let color = Color::rgba(r, g, b, alpha);
        let color = if self.flash.is_zero() {
            color
        } else {
            Color::lerp(color, Color::rgba(255, 255, 255, alpha), self.flash)
        };
        u32::from_le_bytes([color.r, color.g, color.b, color.a])
    }
}

impl StackVisual {
    pub fn new(geometry: impl Into<Geometry>) -> Self {
        Self {
            geometry: geometry.into(),
            pose: Pose::default(),
            material: Material::default(),
            role: None,
            sampled_style: None,
            membership: None,
        }
    }

    pub fn at(mut self, pose: Pose) -> Self {
        self.pose = pose;
        self
    }

    /// Replace the RGB channels of an indexed sprite's palette. The table must have the source length. Source alpha continues to determine coverage.
    pub fn with_palette(mut self, palette: &'static [[u8; 4]]) -> Self {
        self.material.palette = Some(palette);
        self
    }

    /// Mix the sprite's visible colors with white by an amount between zero and one.
    pub fn with_flash(mut self, amount: Fixed) -> Self {
        self.material.flash = amount.clamp(Fixed::ZERO, Fixed::ONE);
        self
    }

    pub(crate) const fn material(&self) -> Material {
        self.material
    }

    pub fn with_sampled_style(self, style: SampledStyle) -> SampledVisual {
        SampledVisual {
            visual: self,
            style,
        }
    }

    /// Names the visual for selection by procedural viewport content.
    pub fn with_role<R: 'static>(mut self) -> Self {
        self.role = Some(TypeId::of::<R>);
        self
    }

    pub(crate) fn role(&self) -> Option<TypeId> {
        self.role.map(|role| role())
    }

    pub(crate) fn sampled_style(&self) -> Option<&SampledStyle> {
        self.sampled_style.as_deref()
    }

    pub const fn geometry(&self) -> &Geometry {
        &self.geometry
    }

    pub(crate) fn drawable_geometry(&self) -> Option<&Geometry> {
        match &self.geometry {
            Geometry::LiveField(field) if !field.callback_ready() => None,
            geometry => Some(geometry),
        }
    }

    pub const fn pose(&self) -> Pose {
        self.pose
    }

    /// Bind a visual description to the observed fields read from a registered model. Retirement removes an attached visual or despawns an entity created by the stage.
    pub fn from_model<H, V>(model: &H, visual: impl Fn(&H) -> V + 'static) -> StackVisualInput
    where
        H: ModelHandle + 'static,
        V: Into<(Self, Option<SampledStyle>)> + 'static,
    {
        StackVisualInput::bind(model, visual, Into::into)
    }

    fn check(&self, world: &World) -> Result<(), CommandCommitError> {
        if self.membership.is_some() {
            return Err(CommandCommitError::OperationRejected {
                operation: "mounted stack visual",
            });
        }
        if let Geometry::LiveField(field) = &self.geometry {
            BindingOwner::registered(field)
                .check_world(world)
                .map_err(|_| CommandCommitError::ForeignOwner {
                    service: "stack visual geometry",
                })?;
        }
        if !self.material.fits(&self.geometry) {
            return Err(CommandCommitError::OperationRejected {
                operation: "stack visual material",
            });
        }
        check_sampled_style(&self.geometry, self.sampled_style())
    }
}

/// Visual input for sampled geometry with its own projection and paint parameters.
pub struct SampledVisual {
    visual: StackVisual,
    style: SampledStyle,
}

impl From<StackVisual> for (StackVisual, Option<SampledStyle>) {
    fn from(visual: StackVisual) -> Self {
        (visual, None)
    }
}

impl From<SampledVisual> for (StackVisual, Option<SampledStyle>) {
    fn from(sampled: SampledVisual) -> Self {
        (sampled.visual, Some(sampled.style))
    }
}

fn check_sampled_style(
    geometry: &Geometry,
    style: Option<&SampledStyle>,
) -> Result<(), CommandCommitError> {
    if let Some(style) = style {
        let palette_len = match geometry {
            Geometry::Sampled(asset) => asset.palette().len(),
            Geometry::Generated(asset) => asset.data().palette().len(),
            _ => {
                return Err(CommandCommitError::OperationRejected {
                    operation: "sampled style geometry",
                });
            }
        };
        if style.entrance.duration <= Fixed::ZERO
            || style.palettes.iter().any(|p| p.len() != palette_len)
        {
            return Err(CommandCommitError::OperationRejected {
                operation: "sampled style colors or timing",
            });
        }
    }
    Ok(())
}

/// A static description or a model binding supplied to a stage command.
pub struct StackVisualInput(VisualInput);

type InstallVisual = Box<
    dyn FnOnce(&mut World, Entity, OnModelRetirement) -> Result<Option<Effect>, CommandCommitError>,
>;

enum VisualInput {
    Static(StackVisual),
    Bound {
        owner: BindingOwner,
        install: InstallVisual,
    },
}

enum VisualRead<V> {
    Ready(V),
    Waiting,
    Retired,
}

#[derive(Clone, Copy)]
pub(crate) enum OnModelRetirement {
    Despawn,
    Detach,
}

impl From<StackVisual> for StackVisualInput {
    fn from(visual: StackVisual) -> Self {
        Self(VisualInput::Static(visual))
    }
}

impl From<SampledVisual> for StackVisualInput {
    fn from(sampled: SampledVisual) -> Self {
        let SampledVisual { mut visual, style } = sampled;
        visual.sampled_style = Some(Box::new(style));
        visual.into()
    }
}

impl StackVisualInput {
    fn bind<H: ModelHandle + 'static, V: 'static>(
        model: &H,
        visual: impl Fn(&H) -> V + 'static,
        parts: fn(V) -> (StackVisual, Option<SampledStyle>),
    ) -> Self {
        let owner = BindingOwner::registered(model);
        let model = model.clone();
        let install = Box::new(move |world: &mut World, entity, retire| {
            let retirement = OnceCell::new();
            install_bound(
                world,
                entity,
                retire,
                move || {
                    let live = retirement.get_or_init(|| model.access().observe_lifetime());
                    if !live.as_ref().is_some_and(|observe| observe()) {
                        VisualRead::Retired
                    } else if model.callback_ready() {
                        VisualRead::Ready(visual(&model))
                    } else {
                        VisualRead::Waiting
                    }
                },
                parts,
            )
        });
        Self(VisualInput::Bound { owner, install })
    }

    pub(crate) fn check(&self, world: &World) -> Result<(), CommandCommitError> {
        match &self.0 {
            VisualInput::Static(visual) => visual.check(world),
            VisualInput::Bound { owner, .. } => {
                owner
                    .check_world(world)
                    .map_err(|_| CommandCommitError::ForeignOwner {
                        service: "stack visual model",
                    })
            }
        }
    }

    pub(crate) fn install(
        self,
        world: &mut World,
        entity: Entity,
        retire: OnModelRetirement,
    ) -> Result<Option<Effect>, CommandCommitError> {
        match self.0 {
            VisualInput::Static(visual) => {
                let field = match visual.geometry() {
                    Geometry::LiveField(field) => Some(field.clone()),
                    _ => None,
                };
                world.insert(entity, visual);
                Ok(field.map(|field| {
                    let lifetime = field.access().observe_lifetime();
                    with_world_scope(world, || {
                        Effect::new(move || {
                            if lifetime.as_ref().is_some_and(|live| live())
                                && field.callback_ready()
                            {
                                field.revision();
                            }
                            with_world(|world| {
                                if let Some(member) = world
                                    .get::<StackVisual>(entity)
                                    .and_then(|visual| visual.membership.as_ref())
                                {
                                    member.members.publish();
                                }
                            });
                        })
                    })
                }))
            }
            VisualInput::Bound { install, .. } => install(world, entity, retire),
        }
    }
}

fn install_bound<V: 'static>(
    world: &mut World,
    entity: Entity,
    retire: OnModelRetirement,
    read: impl Fn() -> VisualRead<V> + 'static,
    parts: fn(V) -> (StackVisual, Option<SampledStyle>),
) -> Result<Option<Effect>, CommandCommitError> {
    let field_revision = Cell::new(None);
    let observe_field = field_observer();
    let effect = with_world_scope(world, || {
        Effect::new(move || {
            let next = read();
            with_world(|world| {
                if !world.is_alive(entity) {
                    return;
                }
                let next = match next {
                    VisualRead::Ready(visual) => Some(parts(visual)),
                    VisualRead::Waiting => None,
                    VisualRead::Retired => {
                        if world
                            .get::<StackVisual>(entity)
                            .is_some_and(|visual| visual.membership.is_some())
                        {
                            match retire {
                                OnModelRetirement::Despawn => despawn_subtree(world, entity),
                                OnModelRetirement::Detach => {
                                    world.remove::<StackVisual>(entity);
                                }
                            }
                        }
                        return;
                    }
                };
                let Some((mut next, style)) = next.filter(|(next, style)| {
                    next.check(world).is_ok()
                        && check_sampled_style(&next.geometry, style.as_ref()).is_ok()
                }) else {
                    if let Some(visual) = world.get::<StackVisual>(entity) {
                        let revision = observe_field(&visual.geometry);
                        if field_revision.replace(revision) != revision
                            && let Some(member) = &visual.membership
                        {
                            member.members.publish();
                        }
                    }
                    return;
                };
                field_revision.set(observe_field(&next.geometry));
                let Some(current) = world.get::<StackVisual>(entity) else {
                    next.sampled_style = style.map(Box::new);
                    world.insert(entity, next);
                    return;
                };
                if current.pose == next.pose
                    && same_geometry(&current.geometry, &next.geometry)
                    && current.material == next.material
                    && current.role() == next.role()
                    && current.sampled_style() == style.as_ref()
                {
                    return;
                }
                if current.membership.is_none() {
                    return;
                }
                let visual = world
                    .get_mut::<StackVisual>(entity)
                    .expect("mounted visual");
                visual.pose = next.pose;
                visual.geometry = next.geometry;
                visual.material = next.material;
                visual.role = next.role;
                match (visual.sampled_style.as_deref_mut(), style) {
                    (Some(current), Some(style)) => *current = style,
                    (None, Some(style)) => visual.sampled_style = Some(Box::new(style)),
                    (_, None) => visual.sampled_style = None,
                }
                visual
                    .membership
                    .as_ref()
                    .expect("mounted visual")
                    .members
                    .publish();
            });
        })
    });
    if !world.has::<StackVisual>(entity) {
        return Err(CommandCommitError::OperationRejected {
            operation: "initial stack visual",
        });
    }
    Ok(Some(effect))
}

fn field_observer() -> impl Fn(&Geometry) -> Option<u64> {
    let lifetime = Cell::new(None);
    move |geometry| {
        let Geometry::LiveField(field) = geometry else {
            lifetime.set(None);
            return None;
        };
        // Keep the last source's signal alive until its replacement is observed.
        let current = field.access().observe_lifetime();
        let revision = current
            .as_ref()
            .filter(|live| live() && field.callback_ready())
            .map(|_| field.revision());
        lifetime.set(current);
        revision
    }
}

fn same_geometry(left: &Geometry, right: &Geometry) -> bool {
    match (left, right) {
        (Geometry::Sprite(a), Geometry::Sprite(b)) => a.ptr_eq(b),
        (Geometry::Prepared(a), Geometry::Prepared(b))
        | (Geometry::Sampled(a), Geometry::Sampled(b)) => core::ptr::eq(*a, *b),
        (Geometry::Field(a), Geometry::Field(b)) => a.ptr_eq(b),
        (Geometry::Generated(a), Geometry::Generated(b)) => a == b,
        _ => false,
    }
}
