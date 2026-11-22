use super::Daylight;
#[cfg(test)]
use super::{Palette, PlacedAircraft};
#[cfg(test)]
use crate::{
    flight::{Flight, Projection},
    jet,
};
use mirui::types::Fixed;
#[cfg(test)]
use mirui_sprite_stack::raster::{
    SceneCanvas,
    sprites::{ProjectedStack, SpritePaint, StackTransform},
};

pub(crate) fn aircraft_lights(axes: [[Fixed; 3]; 3], sun: [Fixed; 2], day: Daylight) -> [usize; 2] {
    [-Fixed::ONE, Fixed::ONE].map(|side| {
        let n = core::array::from_fn::<_, 3, _>(|i| {
            axes[2][i] + side * Fixed::from_ratio(4, 10) * axes[0][i]
        });
        let dot = -n[0] * sun[0] - n[1] * sun[1] + n[2] * Fixed::from_ratio(11, 10);
        let norm = ((sun[0] * sun[0] + sun[1] * sun[1] + Fixed::from_ratio(121, 100))
            * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]))
            .sqrt();
        let value = dot / norm;
        let (low, high) = if day == Daylight::Night {
            (Fixed::from_ratio(4, 10), Fixed::from_ratio(74, 100))
        } else {
            (Fixed::from_ratio(66, 100), Fixed::from_ratio(9, 10))
        };
        if value > high {
            1
        } else if value < low {
            2
        } else {
            0
        }
    })
}

#[cfg(test)]
pub(super) struct PreparedJet {
    pub(super) sprite: ProjectedStack<'static, { jet::LAYERS }>,
    fade: Fixed,
    lights: [usize; 2],
    right_side_x: i32,
}
#[cfg(test)]
impl PreparedJet {
    pub(super) fn new(
        flight: &Flight,
        index: usize,
        jet: PlacedAircraft,
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
            Fixed::from_int((jet.model.extent()[1]) as i32) * flight.settings.jet_scale * center[2]
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
        let light = |side: Fixed| {
            let n = core::array::from_fn::<_, 3, _>(|i| {
                axes[2][i] + side * Fixed::from_ratio(4, 10) * axes[0][i]
            });
            let dot = -n[0] * pal.sun[0] - n[1] * pal.sun[1] + n[2] * Fixed::from_ratio(11, 10);
            let norm =
                ((pal.sun[0] * pal.sun[0] + pal.sun[1] * pal.sun[1] + Fixed::from_ratio(121, 100))
                    * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]))
                    .sqrt();
            let v = dot / norm;
            if v > if pal.day == Daylight::Night {
                Fixed::from_ratio(74, 100)
            } else {
                Fixed::from_ratio(9, 10)
            } {
                1
            } else if v < if pal.day == Daylight::Night {
                Fixed::from_ratio(4, 10)
            } else {
                Fixed::from_ratio(66, 100)
            } {
                2
            } else {
                0
            }
        };
        let lights = {
            mirui::trace_span!("flight.aircraft.light");
            [light(-Fixed::ONE), light(Fixed::ONE)]
        };
        let sprite = ProjectedStack::new(
            jet.model,
            StackTransform {
                scale: sc,
                layer_spacing: flight.spread * Fixed::from_ratio(jet.model.spacing_q8(), 256),
                depth_radius: Fixed::from(jet.model.extent()[1]) * Fixed::from_ratio(6, 10) * sc,
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
            Fixed::from_ratio(jet.model.pivot_q8()[0], 256),
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
