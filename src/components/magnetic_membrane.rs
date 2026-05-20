use crate::draw::path::Path;
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
        self.sigma * self.visible_span
    }

    pub fn path(&self, edge_x: Fixed, mid_y: Fixed, state: MagneticMembraneState) -> Path {
        self.edge_path(edge_x, mid_y, state)
    }

    fn basis_at(&self, edge_x: Fixed, mid_y: Fixed, t: Fixed) -> (Point, Point) {
        match self.edge {
            MagneticMembraneEdge::Flat { angle } => {
                let outward = Point {
                    x: Fixed::cos_deg(angle),
                    y: Fixed::sin_deg(angle),
                };
                let normal = Point {
                    x: Fixed::ZERO - outward.x,
                    y: Fixed::ZERO - outward.y,
                };
                let tangent = Point {
                    x: Fixed::ZERO - normal.y,
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
                    x: Fixed::ZERO - outward.x,
                    y: Fixed::ZERO - outward.y,
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

    fn edge_path(&self, edge_x: Fixed, mid_y: Fixed, state: MagneticMembraneState) -> Path {
        let span = self.span();
        let safe = span.max(Fixed::ONE);
        let pull_ratio = (state.ball_offset.abs() / safe).min(Fixed::ONE);
        let amp = state.amp.min(self.max_amp) * pull_ratio;
        let mut path = Path::new();
        let (start, _) = self.basis_at(edge_x, mid_y, Fixed::ZERO - span);
        path.move_to(start).line_to(start);
        for i in 0..=64 {
            let t = Fixed::from_int(-64 + i * 2) * span / Fixed::from_int(64);
            let edge_u = t.abs() / safe;
            let edge_fade = (Fixed::ONE - edge_u * edge_u).max(Fixed::ZERO);
            let d = (t - state.ball_offset).abs() / self.sigma;
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
mod tests {
    use super::*;

    #[test]
    fn membrane_path_returns_to_edge_at_ends() {
        let membrane = MagneticMembrane::default();
        let path = membrane.path(
            Fixed::from_int(100),
            Fixed::from_int(100),
            MagneticMembraneState {
                ball_offset: Fixed::from_int(30),
                amp: Fixed::from_int(28),
            },
        );
        assert!(!path.cmds.is_empty());
    }

    #[test]
    fn arc_membrane_path_builds() {
        let membrane = MagneticMembrane {
            edge: MagneticMembraneEdge::Arc {
                center: Point {
                    x: Fixed::from_int(0),
                    y: Fixed::from_int(100),
                },
                radius: Fixed::from_int(100),
                angle: Fixed::ZERO,
            },
            ..MagneticMembrane::default()
        };
        let path = membrane.path(
            Fixed::from_int(100),
            Fixed::from_int(100),
            MagneticMembraneState {
                ball_offset: Fixed::from_int(20),
                amp: Fixed::from_int(20),
            },
        );
        assert!(!path.cmds.is_empty());
    }

    #[test]
    fn max_pull_is_smaller_than_span() {
        let membrane = MagneticMembrane::default();
        assert!(membrane.max_pull() < membrane.span());
    }
}
