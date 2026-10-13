use super::mount;
use crate::anim::{Motion, MotionComponent, Spring, SpringConfig};
use crate::ecs::{Entity, World};
use crate::types::Fixed;

/// Visual replacement of two page subtrees. The first page is shown immediately.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum PageTransition {
    #[default]
    None,
    Slide(SpringConfig),
    Fade(SpringConfig),
}

impl PageTransition {
    /// Slide horizontally using a spring. A zero duration hint switches immediately.
    pub const fn slide(spring: SpringConfig) -> Self {
        Self::Slide(spring)
    }

    /// Cross-fade complete page groups using the configured offscreen pixel budget.
    ///
    /// Unavailable output storage or backend support completes the switch without animation and logs a warning.
    pub const fn fade(spring: SpringConfig) -> Self {
        Self::Fade(spring)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct PagePair {
    pub from: Entity,
    pub to: Entity,
    motion: Motion,
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
    let spring = match transition {
        PageTransition::None => return false,
        PageTransition::Slide(spring) | PageTransition::Fade(spring) => spring,
    };
    if spring.duration_ms == 0
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
        motion: Spring::preset(Fixed::ZERO, Fixed::ONE, spring).into(),
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
    if let Motion::Spring(spring) = motion.motion_mut() {
        spring.retarget(if reversed { Fixed::ZERO } else { Fixed::ONE }, None);
    }
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
                offscreen.opacity = opacity;
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
