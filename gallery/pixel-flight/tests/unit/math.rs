use super::*;
#[test]
fn scalar_operations_preserve_domain_and_precision() {
    for n in -8..=8 {
        let x = Fixed::from_ratio(n, 4);
        assert!((x.exp().ln() - x).abs() <= Fixed::from_ratio(1, 16));
    }
    assert!(
        (Fixed::from_int(4).pow_fraction(Fixed::HALF) - Fixed::from_int(2)).abs()
            <= Fixed::from_ratio(1, 64)
    );
    assert_eq!((-Fixed::HALF).rem_euclid(Fixed::ONE), Fixed::HALF);
}
