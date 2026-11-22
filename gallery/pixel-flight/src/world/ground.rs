use super::World;
use mirui::types::Fixed;
use mirui_sprite_stack::Pose;

#[derive(Clone, Copy)]
enum GroundPose {
    Original,
    Placed {
        origin: [Fixed; 3],
        rotation: [Fixed; 2],
    },
    Absent,
}

#[derive(Clone, Copy)]
pub(crate) struct Ground<'a> {
    pub(crate) world: &'a World,
    placement: GroundPose,
}

impl Ground<'_> {
    pub(crate) fn is_original(&self) -> bool {
        matches!(self.placement, GroundPose::Original)
    }

    pub(crate) fn elevation(&self) -> Fixed {
        match self.placement {
            GroundPose::Placed { origin, .. } => origin[1],
            _ => Fixed::ZERO,
        }
    }
    fn local_address(
        &self,
        x: Fixed,
        y: Fixed,
        origin: [Fixed; 3],
        [sin, cos]: [Fixed; 2],
    ) -> usize {
        let dx = x - origin[0];
        let dz = -y - origin[2];
        self.world
            .ground
            .address_at(dx * cos - dz * sin, dx * sin + dz * cos)
            .expect("flight ground repeats across the map")
    }

    pub(crate) fn placed_sample(&self, [x, y]: [Fixed; 2]) -> Option<(Fixed, usize)> {
        let GroundPose::Placed { origin, rotation } = self.placement else {
            return None;
        };
        let i = self.local_address(x, y, origin, rotation);
        Some((
            Fixed::from_int(i32::from(self.world.ground.levels()[i])) + origin[1],
            i,
        ))
    }
    pub(crate) fn height(&self, x: i32, y: i32) -> Fixed {
        if self.is_original() {
            return Fixed::from_int(i32::from(self.world.height(x, y)));
        }
        self.placed_sample([x, y].map(Fixed::from_int))
            .map_or(Fixed::ZERO, |(h, _)| h)
    }
    pub(crate) fn material(&self, x: i32, y: i32) -> u8 {
        match self.placement {
            GroundPose::Original => self.world.material(x, y),
            GroundPose::Placed { origin, rotation } => self.world.ground.materials()
                [self.local_address(Fixed::from_int(x), Fixed::from_int(y), origin, rotation)],
            GroundPose::Absent => 0,
        }
    }
}

impl World {
    pub(crate) fn at(&self, pose: Option<Pose>) -> Ground<'_> {
        let placement = match pose {
            Some(p) if p == Pose::default() => GroundPose::Original,
            Some(p) if p.pitch.is_zero() && p.roll.is_zero() => GroundPose::Placed {
                origin: p.position,
                rotation: [Fixed::sin_deg(p.yaw), Fixed::cos_deg(p.yaw)],
            },
            _ => GroundPose::Absent,
        };
        Ground {
            world: self,
            placement,
        }
    }
}
