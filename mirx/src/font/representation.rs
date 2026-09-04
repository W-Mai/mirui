type RepresentationIdentity = (u8, u16, u16, u16, u16, u16);
type SelectionRank = (u16, u8, u16, u32, RepresentationIdentity);

/// Raster semantics carried by one representation of a font face.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum FontRepresentationKind {
    /// Fixed-size grayscale coverage samples.
    Coverage { bits: u8 },
    /// Scalable signed-distance samples generated with the declared spread.
    SignedDistance { bits: u8, spread: u16 },
    /// Application-defined glyph sample semantics.
    Application(u16),
}

/// Failure while constructing or validating a font representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FontRepresentationError {
    InvalidBits {
        kind: FontRepresentationKind,
        bits: u8,
    },
    InvalidSpread,
    InvalidDesignPpem,
    InvalidSizeRange {
        min_ppem: u16,
        max_ppem: u16,
    },
    DesignOutsideSizeRange {
        design_ppem: u16,
        min_ppem: u16,
        max_ppem: u16,
    },
    CoverageMustBeFixedSize {
        design_ppem: u16,
        min_ppem: u16,
        max_ppem: u16,
    },
}

/// Size and sample contract for one raster representation of a font face.
///
/// `decoded_bytes` participates only in deterministic selection between
/// overlapping representations. It describes the decoded glyph surface group,
/// not the size of this metadata record.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FontRepresentation {
    kind: FontRepresentationKind,
    design_ppem: u16,
    min_ppem: u16,
    max_ppem: u16,
    decoded_bytes: u32,
}

impl FontRepresentation {
    /// Creates a fixed-size grayscale coverage representation.
    pub const fn coverage(
        bits: u8,
        ppem: u16,
        decoded_bytes: u32,
    ) -> Result<Self, FontRepresentationError> {
        Self::new(
            FontRepresentationKind::Coverage { bits },
            ppem,
            ppem,
            ppem,
            decoded_bytes,
        )
    }

    /// Creates a signed-distance representation with an inclusive size range.
    pub const fn signed_distance(
        bits: u8,
        spread: u16,
        design_ppem: u16,
        min_ppem: u16,
        max_ppem: u16,
        decoded_bytes: u32,
    ) -> Result<Self, FontRepresentationError> {
        Self::new(
            FontRepresentationKind::SignedDistance { bits, spread },
            design_ppem,
            min_ppem,
            max_ppem,
            decoded_bytes,
        )
    }

    /// Creates an application-defined representation.
    pub const fn application(
        kind: u16,
        design_ppem: u16,
        min_ppem: u16,
        max_ppem: u16,
        decoded_bytes: u32,
    ) -> Result<Self, FontRepresentationError> {
        Self::new(
            FontRepresentationKind::Application(kind),
            design_ppem,
            min_ppem,
            max_ppem,
            decoded_bytes,
        )
    }

    /// Creates and validates one representation contract.
    pub const fn new(
        kind: FontRepresentationKind,
        design_ppem: u16,
        min_ppem: u16,
        max_ppem: u16,
        decoded_bytes: u32,
    ) -> Result<Self, FontRepresentationError> {
        let representation = Self {
            kind,
            design_ppem,
            min_ppem,
            max_ppem,
            decoded_bytes,
        };
        match representation.validate() {
            Ok(()) => Ok(representation),
            Err(error) => Err(error),
        }
    }

    /// Validates the kind-specific size and sample invariants.
    pub const fn validate(self) -> Result<(), FontRepresentationError> {
        match self.kind {
            FontRepresentationKind::Coverage { bits } => {
                if !matches!(bits, 1 | 2 | 4 | 8) {
                    return Err(FontRepresentationError::InvalidBits {
                        kind: self.kind,
                        bits,
                    });
                }
            }
            FontRepresentationKind::SignedDistance { bits, spread } => {
                if !matches!(bits, 4 | 8) {
                    return Err(FontRepresentationError::InvalidBits {
                        kind: self.kind,
                        bits,
                    });
                }
                if spread == 0 {
                    return Err(FontRepresentationError::InvalidSpread);
                }
            }
            FontRepresentationKind::Application(_) => {}
        }

        if self.design_ppem == 0 {
            return Err(FontRepresentationError::InvalidDesignPpem);
        }
        if self.min_ppem == 0 || self.min_ppem > self.max_ppem {
            return Err(FontRepresentationError::InvalidSizeRange {
                min_ppem: self.min_ppem,
                max_ppem: self.max_ppem,
            });
        }
        if self.design_ppem < self.min_ppem || self.design_ppem > self.max_ppem {
            return Err(FontRepresentationError::DesignOutsideSizeRange {
                design_ppem: self.design_ppem,
                min_ppem: self.min_ppem,
                max_ppem: self.max_ppem,
            });
        }
        if matches!(self.kind, FontRepresentationKind::Coverage { .. })
            && (self.min_ppem != self.design_ppem || self.max_ppem != self.design_ppem)
        {
            return Err(FontRepresentationError::CoverageMustBeFixedSize {
                design_ppem: self.design_ppem,
                min_ppem: self.min_ppem,
                max_ppem: self.max_ppem,
            });
        }
        Ok(())
    }

    pub const fn kind(self) -> FontRepresentationKind {
        self.kind
    }

    pub const fn design_ppem(self) -> u16 {
        self.design_ppem
    }

    pub const fn min_ppem(self) -> u16 {
        self.min_ppem
    }

    pub const fn max_ppem(self) -> u16 {
        self.max_ppem
    }

    pub const fn decoded_bytes(self) -> u32 {
        self.decoded_bytes
    }

    pub const fn supports(self, ppem: u16) -> bool {
        ppem >= self.min_ppem && ppem <= self.max_ppem
    }

    const fn selection_identity(self) -> RepresentationIdentity {
        let (class, detail, spread) = match self.kind {
            FontRepresentationKind::Coverage { bits } => (0, bits as u16, 0),
            FontRepresentationKind::SignedDistance { bits, spread } => (1, bits as u16, spread),
            FontRepresentationKind::Application(kind) => (2, kind, 0),
        };
        (
            class,
            detail,
            spread,
            self.design_ppem,
            self.min_ppem,
            self.max_ppem,
        )
    }

    fn distance_to_design(self, ppem: u16) -> u16 {
        self.design_ppem.abs_diff(ppem)
    }

    fn distance_to_range(self, ppem: u16) -> u16 {
        if ppem < self.min_ppem {
            self.min_ppem - ppem
        } else {
            ppem.saturating_sub(self.max_ppem)
        }
    }
}

/// Representation kinds considered by a size request.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum FontRepresentationPreference {
    #[default]
    Auto,
    Coverage,
    SignedDistance,
    Application(u16),
}

/// Behavior when no representation covers the requested size.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum FontRepresentationFallback {
    #[default]
    Reject,
    Nearest,
}

/// Selection policy for one requested font size.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FontRepresentationRequest {
    ppem: u16,
    preference: FontRepresentationPreference,
    fallback: FontRepresentationFallback,
}

impl FontRepresentationRequest {
    pub const fn new(ppem: u16) -> Self {
        Self {
            ppem,
            preference: FontRepresentationPreference::Auto,
            fallback: FontRepresentationFallback::Reject,
        }
    }

    pub const fn with_preference(mut self, preference: FontRepresentationPreference) -> Self {
        self.preference = preference;
        self
    }

    pub const fn with_fallback(mut self, fallback: FontRepresentationFallback) -> Self {
        self.fallback = fallback;
        self
    }

    pub const fn ppem(self) -> u16 {
        self.ppem
    }

    pub const fn preference(self) -> FontRepresentationPreference {
        self.preference
    }

    pub const fn fallback(self) -> FontRepresentationFallback {
        self.fallback
    }
}

/// Failure while validating or selecting from a representation collection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FontSelectionError {
    Empty,
    InvalidRequestSize,
    InvalidRepresentation {
        index: usize,
        error: FontRepresentationError,
    },
    DuplicateRepresentation {
        first: usize,
        duplicate: usize,
    },
    NoMatch,
}

/// Result of deterministic selection, independent of record storage lifetimes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FontRepresentationMatch {
    index: usize,
    representation: FontRepresentation,
    used_fallback: bool,
}

impl FontRepresentationMatch {
    pub const fn index(self) -> usize {
        self.index
    }

    pub const fn representation(self) -> FontRepresentation {
        self.representation
    }

    pub const fn used_fallback(self) -> bool {
        self.used_fallback
    }
}

/// Validated, allocation-free view over a font face's representations.
#[derive(Clone, Copy, Debug)]
pub struct FontRepresentations<'a> {
    records: &'a [FontRepresentation],
}

impl<'a> FontRepresentations<'a> {
    pub fn new(records: &'a [FontRepresentation]) -> Result<Self, FontSelectionError> {
        if records.is_empty() {
            return Err(FontSelectionError::Empty);
        }
        for (index, representation) in records.iter().copied().enumerate() {
            representation
                .validate()
                .map_err(|error| FontSelectionError::InvalidRepresentation { index, error })?;
            for (first, earlier) in records[..index].iter().copied().enumerate() {
                if earlier.selection_identity() == representation.selection_identity() {
                    return Err(FontSelectionError::DuplicateRepresentation {
                        first,
                        duplicate: index,
                    });
                }
            }
        }
        Ok(Self { records })
    }

    pub const fn as_slice(self) -> &'a [FontRepresentation] {
        self.records
    }

    pub const fn len(self) -> usize {
        self.records.len()
    }

    pub const fn is_empty(self) -> bool {
        self.records.is_empty()
    }

    pub fn select(
        self,
        request: FontRepresentationRequest,
    ) -> Result<FontRepresentationMatch, FontSelectionError> {
        request.select_by(self.len(), |index| self.records[index])
    }
}

impl FontRepresentationRequest {
    pub(crate) fn select_by(
        self,
        count: usize,
        record: impl Fn(usize) -> FontRepresentation,
    ) -> Result<FontRepresentationMatch, FontSelectionError> {
        if self.ppem == 0 {
            return Err(FontSelectionError::InvalidRequestSize);
        }

        let covered = self.best_match_by(count, &record, false);
        if let Some((index, representation)) = covered {
            return Ok(FontRepresentationMatch {
                index,
                representation,
                used_fallback: false,
            });
        }
        if self.fallback == FontRepresentationFallback::Nearest {
            if let Some((index, representation)) = self.best_match_by(count, &record, true) {
                return Ok(FontRepresentationMatch {
                    index,
                    representation,
                    used_fallback: true,
                });
            }
        }
        Err(FontSelectionError::NoMatch)
    }

    fn best_match_by(
        self,
        count: usize,
        record: &impl Fn(usize) -> FontRepresentation,
        allow_outside_range: bool,
    ) -> Option<(usize, FontRepresentation)> {
        (0..count)
            .map(|index| (index, record(index)))
            .filter(|(_, representation)| self.accepts(*representation, allow_outside_range))
            .filter(|(_, representation)| allow_outside_range || representation.supports(self.ppem))
            .min_by_key(|(_, representation)| self.rank(*representation, allow_outside_range))
    }
}

impl FontRepresentationRequest {
    fn accepts(self, representation: FontRepresentation, allow_outside_range: bool) -> bool {
        match (self.preference, representation.kind) {
            (FontRepresentationPreference::Auto, FontRepresentationKind::Coverage { .. }) => {
                allow_outside_range || representation.design_ppem == self.ppem
            }
            (FontRepresentationPreference::Auto, FontRepresentationKind::SignedDistance { .. }) => {
                true
            }
            (FontRepresentationPreference::Auto, FontRepresentationKind::Application(_)) => false,
            (FontRepresentationPreference::Coverage, FontRepresentationKind::Coverage { .. }) => {
                true
            }
            (
                FontRepresentationPreference::SignedDistance,
                FontRepresentationKind::SignedDistance { .. },
            ) => true,
            (
                FontRepresentationPreference::Application(expected),
                FontRepresentationKind::Application(actual),
            ) => expected == actual,
            _ => false,
        }
    }

    fn rank(self, representation: FontRepresentation, outside_range: bool) -> SelectionRank {
        (
            if outside_range {
                representation.distance_to_range(self.ppem)
            } else {
                0
            },
            if !outside_range
                && self.preference == FontRepresentationPreference::Auto
                && matches!(
                    representation.kind,
                    FontRepresentationKind::SignedDistance { .. }
                )
            {
                1
            } else {
                0
            },
            representation.distance_to_design(self.ppem),
            representation.decoded_bytes,
            representation.selection_identity(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_metadata_outlives_native_storage() {
        let selected = {
            let records = [FontRepresentation::coverage(4, 16, 512).unwrap()];
            FontRepresentations::new(&records)
                .unwrap()
                .select(FontRepresentationRequest::new(16))
                .unwrap()
        };
        assert_eq!(selected.index(), 0);
        assert_eq!(
            selected.representation(),
            FontRepresentation::coverage(4, 16, 512).unwrap()
        );
        assert!(!selected.used_fallback());
    }

    #[test]
    fn record_access_selection_has_two_pass_bound_and_identical_policy() {
        use core::cell::Cell;
        let records = [
            FontRepresentation::coverage(4, 16, 512).unwrap(),
            FontRepresentation::signed_distance(8, 4, 24, 17, 48, 1024).unwrap(),
            FontRepresentation::signed_distance(8, 8, 64, 49, 128, 2048).unwrap(),
            FontRepresentation::application(42, 32, 1, 128, 512).unwrap(),
        ];
        let native = FontRepresentations::new(&records).unwrap();
        for size in [0, 1, 16, 17, 24, 48, 49, 64, 128, 129, u16::MAX] {
            for preference in [
                FontRepresentationPreference::Auto,
                FontRepresentationPreference::Coverage,
                FontRepresentationPreference::SignedDistance,
                FontRepresentationPreference::Application(42),
            ] {
                for fallback in [
                    FontRepresentationFallback::Reject,
                    FontRepresentationFallback::Nearest,
                ] {
                    let request = FontRepresentationRequest::new(size)
                        .with_preference(preference)
                        .with_fallback(fallback);
                    let calls = Cell::new(0);
                    let selected = request.select_by(records.len(), |index| {
                        calls.set(calls.get() + 1);
                        let value = records[index];
                        FontRepresentation::new(
                            value.kind(),
                            value.design_ppem(),
                            value.min_ppem(),
                            value.max_ppem(),
                            value.decoded_bytes(),
                        )
                        .unwrap()
                    });
                    assert_eq!(selected, native.select(request));
                    assert!(calls.get() <= records.len() * 2);
                    if size == 0 {
                        assert_eq!(calls.get(), 0);
                    }
                }
            }
        }
    }

    fn coverage(bits: u8, ppem: u16, bytes: u32) -> FontRepresentation {
        FontRepresentation::coverage(bits, ppem, bytes).unwrap()
    }

    fn sdf(
        bits: u8,
        spread: u16,
        design: u16,
        min: u16,
        max: u16,
        bytes: u32,
    ) -> FontRepresentation {
        FontRepresentation::signed_distance(bits, spread, design, min, max, bytes).unwrap()
    }

    #[test]
    fn constructors_enforce_kind_specific_contracts() {
        assert_eq!(
            FontRepresentation::coverage(3, 12, 64),
            Err(FontRepresentationError::InvalidBits {
                kind: FontRepresentationKind::Coverage { bits: 3 },
                bits: 3,
            })
        );
        assert_eq!(
            FontRepresentation::signed_distance(8, 0, 24, 17, 48, 256),
            Err(FontRepresentationError::InvalidSpread)
        );
        assert_eq!(
            FontRepresentation::signed_distance(8, 4, 16, 17, 48, 256),
            Err(FontRepresentationError::DesignOutsideSizeRange {
                design_ppem: 16,
                min_ppem: 17,
                max_ppem: 48,
            })
        );
        assert_eq!(
            FontRepresentation::new(FontRepresentationKind::Coverage { bits: 4 }, 16, 12, 16, 64,),
            Err(FontRepresentationError::CoverageMustBeFixedSize {
                design_ppem: 16,
                min_ppem: 12,
                max_ppem: 16,
            })
        );
    }

    #[test]
    fn auto_uses_exact_coverage_then_sdf_ranges() {
        let records = [
            coverage(4, 12, 144),
            coverage(4, 16, 256),
            sdf(8, 4, 24, 17, 48, 576),
            sdf(8, 8, 64, 49, 128, 4096),
        ];
        let representations = FontRepresentations::new(&records).unwrap();

        assert_eq!(
            representations
                .select(FontRepresentationRequest::new(16))
                .unwrap()
                .index(),
            1
        );
        assert_eq!(
            representations
                .select(FontRepresentationRequest::new(40))
                .unwrap()
                .index(),
            2
        );
        assert_eq!(
            representations
                .select(FontRepresentationRequest::new(72))
                .unwrap()
                .index(),
            3
        );
    }

    #[test]
    fn auto_exact_coverage_precedes_an_exact_sdf_design_size() {
        let exact_sdf = sdf(4, 4, 16, 12, 32, 64);
        let exact_coverage = coverage(8, 16, 256);
        let records = [exact_sdf, exact_coverage];
        let selected = FontRepresentations::new(&records)
            .unwrap()
            .select(FontRepresentationRequest::new(16))
            .unwrap();
        assert_eq!(selected.representation(), exact_coverage);
    }

    #[test]
    fn selection_is_independent_of_record_order() {
        let a = sdf(8, 4, 24, 17, 64, 900);
        let b = sdf(8, 4, 48, 32, 96, 500);
        let first = [a, b];
        let second = [b, a];
        let request = FontRepresentationRequest::new(40);

        let selected_a = FontRepresentations::new(&first)
            .unwrap()
            .select(request)
            .unwrap()
            .representation();
        let selected_b = FontRepresentations::new(&second)
            .unwrap()
            .select(request)
            .unwrap()
            .representation();
        assert_eq!(selected_a, selected_b);
        assert_eq!(selected_a, b);
    }

    #[test]
    fn overlapping_ranges_use_cost_after_design_distance() {
        let expensive = sdf(8, 4, 24, 17, 48, 900);
        let compact = sdf(4, 4, 24, 17, 48, 400);
        let records = [expensive, compact];
        let selected = FontRepresentations::new(&records)
            .unwrap()
            .select(FontRepresentationRequest::new(30))
            .unwrap();
        assert_eq!(selected.representation(), compact);
    }

    #[test]
    fn explicit_kind_never_crosses_to_another_kind() {
        let records = [coverage(4, 16, 256), sdf(8, 4, 24, 17, 48, 576)];
        let representations = FontRepresentations::new(&records).unwrap();
        let coverage_request = FontRepresentationRequest::new(24)
            .with_preference(FontRepresentationPreference::Coverage);
        assert_eq!(
            representations.select(coverage_request),
            Err(FontSelectionError::NoMatch)
        );
        let nearest = coverage_request.with_fallback(FontRepresentationFallback::Nearest);
        let selected = representations.select(nearest).unwrap();
        assert_eq!(selected.index(), 0);
        assert!(selected.used_fallback());
    }

    #[test]
    fn auto_rejects_or_marks_out_of_contract_fallback() {
        let records = [sdf(8, 4, 24, 17, 48, 576)];
        let representations = FontRepresentations::new(&records).unwrap();
        assert_eq!(
            representations.select(FontRepresentationRequest::new(96)),
            Err(FontSelectionError::NoMatch)
        );

        let selected = representations
            .select(
                FontRepresentationRequest::new(96)
                    .with_fallback(FontRepresentationFallback::Nearest),
            )
            .unwrap();
        assert_eq!(selected.index(), 0);
        assert!(selected.used_fallback());
    }

    #[test]
    fn auto_nearest_can_fall_back_to_a_fixed_coverage_size() {
        let records = [coverage(4, 12, 144), coverage(4, 16, 256)];
        let selected = FontRepresentations::new(&records)
            .unwrap()
            .select(
                FontRepresentationRequest::new(15)
                    .with_fallback(FontRepresentationFallback::Nearest),
            )
            .unwrap();
        assert_eq!(selected.index(), 1);
        assert!(selected.used_fallback());
    }

    #[test]
    fn collection_rejects_duplicate_selection_contracts() {
        let records = [sdf(8, 4, 24, 17, 48, 576), sdf(8, 4, 24, 17, 48, 512)];
        assert!(matches!(
            FontRepresentations::new(&records),
            Err(FontSelectionError::DuplicateRepresentation {
                first: 0,
                duplicate: 1,
            })
        ));
    }

    #[test]
    fn zero_request_is_rejected() {
        let records = [coverage(4, 16, 256)];
        assert_eq!(
            FontRepresentations::new(&records)
                .unwrap()
                .select(FontRepresentationRequest::new(0)),
            Err(FontSelectionError::InvalidRequestSize)
        );
    }
}
