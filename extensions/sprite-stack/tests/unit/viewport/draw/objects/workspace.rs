use super::{
    Geometry, Material, ObjectSource, ObjectWork, Pose, Projection, SampledStyle, ViewProjection,
};
use crate::{
    PreparedSpriteStack, SparseLevel, StackSample, allocations,
    raster::{Frame, Workspace},
};
use mirui::types::Fixed;

const RED: [u8; 4] = [173, 39, 41, 255];
const BLUE: [u8; 4] = [35, 95, 181, 255];

struct Fixtures([Option<(Geometry, Pose)>; 2]);

impl ObjectSource<usize> for Fixtures {
    fn visit(&self, mut read: impl FnMut(usize, &Geometry, Pose, Option<&SampledStyle>)) {
        for (index, entry) in self.0.iter().enumerate() {
            if let Some((geometry, pose)) = entry {
                read(index, geometry, *pose, None);
            }
        }
    }

    fn with_geometry<R>(
        &self,
        id: usize,
        read: impl FnOnce(&Geometry, Material, Option<&SampledStyle>) -> R,
    ) -> Option<R> {
        self.0[id]
            .as_ref()
            .map(|(geometry, _)| read(geometry, Material::default(), None))
    }
}

#[test]
fn sampled_layer_workspace_reuses_capacity_across_membership_changes() {
    const CELL: StackSample = StackSample {
        x: 0,
        y: 0,
        color: 0,
    };
    const SHORT: PreparedSpriteStack<'static> = PreparedSpriteStack::new(
        [1, 1, 1],
        [0; 3],
        256,
        &[RED],
        &[SparseLevel::new(1, &[0, 1], &[CELL]).unwrap()],
    )
    .unwrap();
    const TALL: PreparedSpriteStack<'static> = PreparedSpriteStack::new(
        [1, 1, 3],
        [0; 3],
        256,
        &[BLUE],
        &[SparseLevel::new(1, &[0, 1, 2, 3], &[CELL; 3]).unwrap()],
    )
    .unwrap();
    let tall = || (Geometry::Sampled(&TALL), Pose::at(3, 0, 0));
    let mut scene = Fixtures([
        Some((Geometry::Sampled(&SHORT), Pose::at(-3, 0, 0))),
        Some(tall()),
    ]);
    let mut pass = ObjectWork::default();
    let mut frame = Frame::new(16, 16).unwrap();
    let mut work = Workspace::new(16, 4).unwrap();
    let mut draw = |pass: &mut ObjectWork<usize>, scene: &Fixtures| {
        let camera = ViewProjection::horizontal(
            Projection {
                perspective: Default::default(),
                slope: Fixed::ZERO,
                scale: Fixed::ONE,
                center: [Fixed::from_int(8); 2],
            },
            [Fixed::ZERO; 3],
            [Fixed::ZERO, Fixed::ONE],
            Fixed::from_int(32),
        );
        let objects = pass.prepare(scene, camera, None).unwrap();
        work.render_rows(&mut frame, |tile| {
            tile.color.bytes.fill(0);
            tile.clear_depth();
            objects.draw(tile)
        })
        .unwrap();
        let pixels = frame.texture.read_pixels().unwrap();
        let mut counts = [0, 0];
        for color in pixels.bytes().chunks_exact(4) {
            counts[0] += usize::from(color == RED);
            counts[1] += usize::from(color == BLUE);
        }
        counts
    };
    let expected = draw(&mut pass, &scene);
    assert!(expected.into_iter().all(|count| count > 0));
    let capacity = pass.storage_bytes();
    let two_largest_footprints = size_of::<[[Fixed; 2]; 64]>() * 2;
    assert!(capacity < two_largest_footprints, "{capacity} bytes");
    for _ in 0..3 {
        scene.0[1] = None;
        assert_eq!(draw(&mut pass, &scene), [expected[0], 0]);
        scene.0[1] = Some(tall());
        let allocations = allocations::tracked_allocations(|| {
            assert_eq!(draw(&mut pass, &scene), expected);
        });
        assert_eq!(allocations, 0);
        assert_eq!(pass.storage_bytes(), capacity);
    }
    let released = allocations::tracked_storage(|| drop(pass));
    assert_eq!(released.freed, capacity);
}
