use crate::path;
use crate::prelude::draw::*;

pub(super) const LOGICAL_SIZE: i32 = 320;

pub(super) static CLIP_CIRCLE: Path = path!(
    M 208 104
    C 208 161.438 161.438 208 104 208
    C 46.562 208 0 161.438 0 104
    C 0 46.562 46.562 0 104 0
    C 161.438 0 208 46.562 208 104
    Z
);
