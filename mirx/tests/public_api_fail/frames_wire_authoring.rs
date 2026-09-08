use mirx::{
    frames::{FrameSequence, FramesAsset},
    image::{SurfaceDescriptor, UnitGroup},
};

fn retained<'a>(
    sequence: FrameSequence,
    surface: SurfaceDescriptor,
    groups: &'a [UnitGroup<'a>],
    counts: &'a [u32],
) {
    let _ = FramesAsset::new(sequence, surface, groups, counts)
        .unwrap()
        .with_unit_index(&[]);
}

fn main() {}
