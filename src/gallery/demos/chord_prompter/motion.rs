/// Center-relative emphasis in 1/256 units for the browser strip.
pub fn center_emphasis(distance: i32) -> u16 {
    let near = 256_u32.saturating_sub(distance.unsigned_abs().saturating_mul(2));
    (near * near / 256) as u16
}
