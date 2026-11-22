use super::*;

#[test]
fn bounded_coverage_never_fills_holes() {
    let mut coverage = UploadCoverage::default();
    let mut uploaded = [[false; 8]; 8];
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for (x, y, w, h) in [
                (0, 0, 2, 2),
                (4, 0, 2, 2),
                (2, 0, 2, 2),
                (2, 2, 4, 2),
                (0, 4, 8, 2),
                (0, 6, 8, 2),
            ] {
                let area = Region::new(x, y, w, h).unwrap();
                assert!(!coverage.contains(area));
                coverage.uploaded(area);
                for row in &mut uploaded[y as usize..(y + h) as usize] {
                    row[x as usize..(x + w) as usize].fill(true);
                }
                assert!(coverage.contains(area));
                for (y, row) in uploaded.iter().enumerate() {
                    for (x, valid) in row.iter().enumerate() {
                        if coverage.contains(Region::new(x as u32, y as u32, 1, 1).unwrap()) {
                            assert!(*valid);
                        }
                    }
                }
            }
        }),
        0
    );
    let full = Region::new(0, 0, 8, 8).unwrap();
    assert!(!coverage.contains(full));
    coverage.uploaded(full);
    assert!(coverage.contains(full));
}
