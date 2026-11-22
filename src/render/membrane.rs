use crate::render::path::Path;
use crate::types::{Fixed, Point};

#[derive(Clone, Copy, Debug)]
pub enum MagneticMembraneEdge {
    Flat {
        angle: Fixed,
    },
    Arc {
        center: Point,
        radius: Fixed,
        angle: Fixed,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct MagneticMembrane {
    pub edge: MagneticMembraneEdge,
    pub sigma: Fixed,
    pub max_amp: Fixed,
    pub visible_span: Fixed,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MagneticMembraneState {
    pub ball_offset: Fixed,
    pub amp: Fixed,
}

impl Default for MagneticMembrane {
    fn default() -> Self {
        Self {
            edge: MagneticMembraneEdge::Flat { angle: Fixed::ZERO },
            sigma: Fixed::from_int(34),
            max_amp: Fixed::from_int(28),
            visible_span: Fixed::from_int(3),
        }
    }
}

impl MagneticMembrane {
    pub fn max_pull(&self) -> Fixed {
        self.span() * Fixed::from_int(2) / Fixed::from_int(5)
    }

    pub fn span(&self) -> Fixed {
        self.sigma.max(Fixed::ONE) * self.visible_span.max(Fixed::ONE)
    }

    fn basis_at(&self, edge_x: Fixed, mid_y: Fixed, t: Fixed) -> (Point, Point) {
        match self.edge {
            MagneticMembraneEdge::Flat { angle } => {
                let outward = Point {
                    x: Fixed::cos_deg(angle),
                    y: Fixed::sin_deg(angle),
                };
                let normal = Point {
                    x: -outward.x,
                    y: -outward.y,
                };
                let tangent = Point {
                    x: -normal.y,
                    y: normal.x,
                };
                (
                    Point {
                        x: edge_x + tangent.x * t,
                        y: mid_y + tangent.y * t,
                    },
                    normal,
                )
            }
            MagneticMembraneEdge::Arc {
                center,
                radius,
                angle,
            } => {
                let radius = radius.max(Fixed::ONE);
                let theta = angle + t * Fixed::from_int(180) / (radius * Fixed::PI);
                let outward = Point {
                    x: Fixed::cos_deg(theta),
                    y: Fixed::sin_deg(theta),
                };
                let normal = Point {
                    x: -outward.x,
                    y: -outward.y,
                };
                (
                    Point {
                        x: center.x + radius * outward.x,
                        y: center.y + radius * outward.y,
                    },
                    normal,
                )
            }
        }
    }

    pub fn path(&self, edge_x: Fixed, mid_y: Fixed, state: MagneticMembraneState) -> Path {
        let span = self.span();
        let safe = span.max(Fixed::ONE);
        let amp = state.amp.min(self.max_amp);
        let sigma = self.sigma.max(Fixed::ONE);
        let mut path = Path::new();
        let (start, _) = self.basis_at(edge_x, mid_y, -span);
        path.move_to(start);
        for i in 0..=64 {
            let t = Fixed::from_int(-64 + i * 2) * span / Fixed::from_int(64);
            let edge_u = t.abs() / safe;
            let edge_fade = (Fixed::ONE - edge_u * edge_u).max(Fixed::ZERO);
            let d = (t - state.ball_offset).abs() / sigma;
            let ball_fade = (Fixed::ONE - d * d).max(Fixed::ZERO);
            let a = amp * edge_fade * edge_fade * ball_fade * ball_fade;
            let (base, normal) = self.basis_at(edge_x, mid_y, t);
            path.line_to(Point {
                x: base.x + normal.x * a,
                y: base.y + normal.y * a,
            });
        }
        let (end, _) = self.basis_at(edge_x, mid_y, span);
        path.line_to(end);
        for i in (0..=64).rev() {
            let t = Fixed::from_int(-64 + i * 2) * span / Fixed::from_int(64);
            let (base, _) = self.basis_at(edge_x, mid_y, t);
            path.line_to(base);
        }
        path.close();
        path
    }
}

#[cfg(test)]
#[path = "../../tests/unit/render/membrane.rs"]
mod tests;
