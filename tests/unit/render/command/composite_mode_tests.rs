use super::CompositeMode::*;

#[test]
fn source_over_returns_src() {
    assert_eq!(SourceOver.blend_channel(0, 0), 0);
    assert_eq!(SourceOver.blend_channel(128, 200), 128);
    assert_eq!(SourceOver.blend_channel(255, 0), 255);
}

#[test]
fn add_saturates_at_255() {
    assert_eq!(Add.blend_channel(128, 128), 255);
    assert_eq!(Add.blend_channel(255, 255), 255);
    assert_eq!(Add.blend_channel(0, 0), 0);
    assert_eq!(Add.blend_channel(64, 64), 128);
}

#[test]
fn screen_is_inverse_multiply_of_inverses() {
    assert!((190..=192).contains(&Screen.blend_channel(128, 128)));
    assert_eq!(Screen.blend_channel(255, 0), 255);
    assert_eq!(Screen.blend_channel(0, 0), 0);
    assert_eq!(Screen.blend_channel(0, 200), 200);
}

#[test]
fn multiply_halves_at_50_percent() {
    assert!((63..=65).contains(&Multiply.blend_channel(128, 128)));
    assert_eq!(Multiply.blend_channel(0, 200), 0);
    assert_eq!(Multiply.blend_channel(255, 200), 200);
}

#[test]
fn darken_keeps_smaller() {
    assert_eq!(Darken.blend_channel(64, 192), 64);
    assert_eq!(Darken.blend_channel(200, 100), 100);
    assert_eq!(Darken.blend_channel(128, 128), 128);
}

#[test]
fn lighten_keeps_larger() {
    assert_eq!(Lighten.blend_channel(64, 192), 192);
    assert_eq!(Lighten.blend_channel(200, 100), 200);
}

#[test]
fn difference_is_absolute_diff() {
    assert_eq!(Difference.blend_channel(192, 64), 128);
    assert_eq!(Difference.blend_channel(64, 192), 128);
    assert_eq!(Difference.blend_channel(100, 100), 0);
}
