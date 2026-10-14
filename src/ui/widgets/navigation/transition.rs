use super::mount;
use crate::anim::{Motion, MotionComponent, Transition};
use crate::ecs::{Entity, World};
use crate::types::Fixed;

/// Horizontal page movement, independent of the motion algorithm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageDirection {
    /// The destination enters from the right; the current page leaves to the left.
    Forward,
    /// The destination enters from the left; the current page leaves to the right.
    Backward,
}

/// Visual replacement of two page subtrees. The first page is shown immediately.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum PageTransition {
    #[default]
    None,
    Slide {
        motion: Transition,
        direction: Option<PageDirection>,
    },
    Fade(Transition),
}

impl PageTransition {
    /// Slide horizontally using the configured transition.
    pub fn slide(transition: impl Into<Transition>) -> Self {
        Self::Slide {
            motion: transition.into(),
            direction: None,
        }
    }

    /// Override the container's slide direction. Fade and None have no spatial direction.
    ///
    /// Without an override, NavigationStack derives direction from its path and PageView enters forward. An active page pair keeps its original direction, including when it reverses.
    pub const fn with_direction(mut self, direction: PageDirection) -> Self {
        if let Self::Slide {
            direction: value, ..
        } = &mut self
        {
            *value = Some(direction);
        }
        self
    }

    /// Cross-fade complete page groups using the configured offscreen pixel budget.
    ///
    /// Unavailable output storage or backend support completes the switch without animation and logs a warning.
    pub fn fade(transition: impl Into<Transition>) -> Self {
        Self::Fade(transition.into())
    }
}

#[derive(Clone, Copy)]
pub(crate) struct PagePair {
    pub from: Entity,
    pub to: Entity,
    motion: Motion,
    transition: Transition,
    direction: Fixed,
    reversed: bool,
    fade: bool,
    complete: bool,
}
impl crate::ecs::Component for PagePair {}

impl MotionComponent for PagePair {
    fn motion(&self) -> &Motion {
        &self.motion
    }

    fn motion_mut(&mut self) -> &mut Motion {
        &mut self.motion
    }
}

impl PagePair {
    fn finished(self) -> bool {
        self.complete || self.motion.is_done()
    }

    fn progress(self) -> Fixed {
        if self.complete {
            if self.reversed {
                Fixed::ZERO
            } else {
                Fixed::ONE
            }
        } else {
            self.motion.value()
        }
    }

    pub(crate) fn destination(self) -> Entity {
        if self.reversed { self.from } else { self.to }
    }
}

pub(crate) fn begin(
    world: &mut World,
    container: Entity,
    from: Entity,
    to: Entity,
    transition: PageTransition,
    forward: bool,
) -> bool {
    let (config, forward) = match transition {
        PageTransition::None => return false,
        PageTransition::Slide { motion, direction } => (
            motion,
            direction.map_or(forward, |direction| direction == PageDirection::Forward),
        ),
        PageTransition::Fade(config) => (config, forward),
    };
    if config.is_immediate()
        || from == to
        || !mount::attached(world, container, from)
        || !mount::attached(world, container, to)
        || world.get::<crate::ui::ComputedRect>(from).is_none()
    {
        return false;
    }
    let motion = PagePair {
        from,
        to,
        motion: config.motion(Fixed::ZERO, Fixed::ONE),
        transition: config,
        direction: if forward { Fixed::ONE } else { -Fixed::ONE },
        reversed: false,
        fade: matches!(transition, PageTransition::Fade(_)),
        complete: false,
    };
    world.insert(container, motion);
    mount::present(world, from);
    mount::present(world, to);
    if world.is_alive(container) {
        if motion.fade {
            prepare_fade(world, motion);
        }
        apply(world, motion);
        world.invalidate(container);
    }
    true
}

fn prepare_fade(world: &mut World, motion: PagePair) {
    for (page, required, opacity) in [(motion.from, motion.to, 0), (motion.to, motion.from, 255)] {
        if !world.is_alive(page) {
            continue;
        }
        world.insert(page, crate::ui::OffscreenRender::new());
        world.insert(
            page,
            crate::ui::offscreen::CompositeFallback { required, opacity },
        );
    }
}

pub(crate) fn running(world: &World, container: Entity) -> Option<PagePair> {
    world.get::<PagePair>(container).copied()
}

pub(crate) fn redirect(world: &mut World, container: Entity, target: Option<Entity>) -> bool {
    let Some(motion) = world.get_mut::<PagePair>(container) else {
        return false;
    };
    let reversed = if target == Some(motion.from) {
        true
    } else if target == Some(motion.to) {
        false
    } else {
        return false;
    };
    if motion.reversed == reversed {
        return false;
    }
    motion.reversed = reversed;
    motion.transition.retarget(
        &mut motion.motion,
        if reversed { Fixed::ZERO } else { Fixed::ONE },
    );
    if motion.fade {
        let motion = *motion;
        redirect_fade(world, motion);
    }
    true
}

fn redirect_fade(world: &mut World, motion: PagePair) {
    for page in [motion.from, motion.to] {
        if let Some(fallback) = world.get_mut::<crate::ui::offscreen::CompositeFallback>(page) {
            fallback.opacity = if page == motion.destination() { 255 } else { 0 };
        }
    }
}

pub(crate) fn finish(world: &mut World, container: Entity) -> Option<(Entity, Entity)> {
    let motion = running(world, container)?;
    if !motion.finished() {
        return None;
    }
    cancel(world, container);
    let winner = motion.destination();
    let loser = if winner == motion.to {
        motion.from
    } else {
        motion.to
    };
    Some((winner, loser))
}

pub(crate) fn cancel(world: &mut World, container: Entity) {
    if let Some(motion) = world.remove::<PagePair>(container) {
        mount::offset(world, motion.from, Fixed::ZERO);
        mount::offset(world, motion.to, Fixed::ZERO);
        if motion.fade {
            for page in [motion.from, motion.to] {
                world.remove::<crate::ui::OffscreenRender>(page);
                world.remove::<crate::ui::offscreen::CompositeFallback>(page);
            }
        }
    }
}

fn apply(world: &mut World, motion: PagePair) {
    let progress = motion.progress();
    if motion.fade {
        let opacity = ((progress * 255).to_int()).clamp(0, 255) as u8;
        for (page, opacity) in [(motion.from, 255 - opacity), (motion.to, opacity)] {
            if let Some(offscreen) = world.get_mut::<crate::ui::OffscreenRender>(page) {
                if offscreen.opacity == opacity {
                    continue;
                }
                offscreen.opacity = opacity;
                world.invalidate_visual(page);
            }
        }
        return;
    }
    mount::offset(world, motion.from, -progress * motion.direction);
    mount::offset(world, motion.to, (Fixed::ONE - progress) * motion.direction);
}

pub(crate) fn advance<T: crate::ecs::Component>(
    world: &mut World,
) -> Result<(), crate::ecs::CommandError> {
    let dt = world
        .resource::<crate::ecs::DeltaTimeMs>()
        .map_or(16, |dt| dt.0);
    world.for_each_stable::<T>(|world, container| {
        if crate::ui::branch::is_hidden_in_tree(world, container)
            || !crate::core::reactive::retained::enabled(world, container)
        {
            return;
        }
        let Some(motion) =
            running(world, container).and_then(|motion| step(world, container, motion, dt))
        else {
            return;
        };
        *world.get_mut::<PagePair>(container).unwrap() = motion;
        apply(world, motion);
        world.invalidate_visual(container);
    });
    Ok(())
}

fn step(world: &World, container: Entity, mut motion: PagePair, dt: u16) -> Option<PagePair> {
    if !mount::attached(world, container, motion.to) {
        motion.reversed = true;
        motion.complete = true;
    } else if !mount::attached(world, container, motion.from) {
        motion.reversed = false;
        motion.complete = true;
    } else if world.get::<crate::ui::ComputedRect>(motion.to).is_none() {
        return None;
    } else if motion.fade && !fade_ready(world, motion) {
        crate::warn!("page fade outputs are unavailable; completing the switch without animation");
        motion.complete = true;
    } else {
        crate::anim::generated::advance_motion(&mut motion, dt);
    }
    Some(motion)
}

fn fade_ready(world: &World, motion: PagePair) -> bool {
    world
        .resource::<crate::ui::Outputs<crate::ui::WidgetTexture>>()
        .is_some_and(|outputs| {
            outputs.get(motion.from).is_some() && outputs.get(motion.to).is_some()
        })
}
