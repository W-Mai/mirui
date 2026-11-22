use super::snap;

#[test]
fn sizes_snap_to_the_nearest_step_with_ties_down_and_a_floor() {
    for (asked, drawn) in [
        (1, 10),
        (6, 10),
        (10, 10),
        (14, 10),
        (15, 10),
        (16, 20),
        (24, 20),
        (25, 20),
        (26, 30),
        (35, 30),
    ] {
        assert_eq!(snap(10, asked), drawn, "{asked}");
    }
    assert_eq!(snap(1, 13), 13);
}
