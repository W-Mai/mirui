use super::*;

impl<'a> ObjectProjection<'a> {
    pub(in crate::viewport::draw) fn new(camera: &'a ViewProjection, pose: Pose) -> Self {
        Self {
            camera,
            instance: InstancePose::new(pose),
        }
    }
}
use crate::raster::{Frame, Workspace};

#[test]
fn crossing_the_eye_plane_clips_faces_and_preserves_projective_depth() {
    let camera = ViewProjection::horizontal(
        Projection {
            perspective: Fixed64::from_ratio(1, 8),
            slope: Fixed::ZERO,
            scale: Fixed::from_int(6),
            center: [Fixed::from_int(16); 2],
        },
        [Fixed::ZERO; 3],
        [Fixed::ZERO, Fixed::ONE],
        Fixed::from_int(32),
    );
    let object = ObjectProjection::new(&camera, Pose::default());
    let mut frame = Frame::new(32, 32).unwrap();
    let mut work = Workspace::new(32, 32).unwrap();
    work.with_canvas(&mut frame, |tile| {
        tile.clear_depth();
        let q = |v: [i32; 3]| v.map(Fixed::from_int);
        object.quad(
            tile,
            [[-1, -4, -10], [2, -4, 8], [2, 4, 8], [-1, 4, -10]].map(q),
            u32::MAX,
        );
        // At pixel center x=16.5 the ray has x=(1+z/8)/12 and the face has z=6*x-4.
        let depth = tile.sample_depth(16, 16).unwrap();
        assert!(
            (depth - Fixed::from_ratio(-56, 15)).abs() < Fixed::from_ratio(1, 10),
            "{depth:?}"
        );
        let i = tile.color.index(16, 16);
        object.quad(
            tile,
            [[-8, -8, -9], [8, -8, -9], [8, 8, -9], [-8, 8, -9]].map(q),
            0xff0000ff,
        );
        assert_eq!(tile.get(i), u32::MAX);
    });
}
