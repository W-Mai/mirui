/// Allocation and collection bounds used by MIRX payload preflight.
///
/// Limits apply independently to each payload decode. A value of zero disables
/// that resource for callers that need a stricter profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PayloadLimits {
    max_decoded_bytes: usize,
    max_raster_groups: u32,
    max_raster_units: u32,
    max_raster_work: u64,
    max_font_glyphs: u32,
    max_font_codepoints: u32,
    max_font_representations: u32,
    max_font_shaping_bytes: usize,
    max_scene_ops: u32,
    max_path_commands: u32,
    max_gradient_stops: u32,
    max_dash_elements: u32,
    max_string_bytes: usize,
    max_meta_entries: u32,
    max_meta_bytes: usize,
    max_frame_records: u32,
    max_palette_colors: u32,
}

impl PayloadLimits {
    /// Default limits for constrained and embedded targets.
    pub const EMBEDDED: Self = Self {
        max_decoded_bytes: 128 * 1024,
        max_raster_groups: 1_024,
        max_raster_units: 65_535,
        max_raster_work: 16 * 1024 * 1024,
        max_font_glyphs: 4_096,
        max_font_codepoints: 16_384,
        max_font_representations: 64,
        max_font_shaping_bytes: 512 * 1024,
        max_scene_ops: 4_096,
        max_path_commands: 16_384,
        max_gradient_stops: 4_096,
        max_dash_elements: 4_096,
        max_string_bytes: 64 * 1024,
        max_meta_entries: 1_024,
        max_meta_bytes: 64 * 1024,
        max_frame_records: 4_096,
        max_palette_colors: 256,
    };

    /// Larger limits intended for explicit host-side tooling.
    pub const HOST: Self = Self {
        max_decoded_bytes: 64 * 1024 * 1024,
        max_raster_groups: 65_535,
        max_raster_units: 16 * 1024 * 1024,
        max_raster_work: 1024 * 1024 * 1024,
        max_font_glyphs: 65_535,
        max_font_codepoints: 1_000_000,
        max_font_representations: 1_024,
        max_font_shaping_bytes: 64 * 1024 * 1024,
        max_scene_ops: 1_000_000,
        max_path_commands: 4_000_000,
        max_gradient_stops: 1_000_000,
        max_dash_elements: 1_000_000,
        max_string_bytes: 16 * 1024 * 1024,
        max_meta_entries: 65_535,
        max_meta_bytes: 64 * 1024 * 1024,
        max_frame_records: 65_535,
        max_palette_colors: 1_000_000,
    };

    pub const fn new() -> Self {
        Self::EMBEDDED
    }

    /// Decoded allocation bound; encoded rasters apply it to one tight unit.
    pub const fn max_decoded_bytes(self) -> usize {
        self.max_decoded_bytes
    }

    /// Total admitted raster groups, including implicit whole-surface groups.
    pub const fn max_raster_groups(self) -> u32 {
        self.max_raster_groups
    }
    pub const fn with_max_raster_groups(mut self, value: u32) -> Self {
        self.max_raster_groups = value;
        self
    }
    /// Total stored raster units across admitted groups and surfaces.
    pub const fn max_raster_units(self) -> u32 {
        self.max_raster_units
    }
    pub const fn with_max_raster_units(mut self, value: u32) -> Self {
        self.max_raster_units = value;
        self
    }
    /// Conservative raster parsing, coverage, syntax and checksum work.
    /// Common metadata opening precedes this budget.
    pub const fn max_raster_work(self) -> u64 {
        self.max_raster_work
    }
    pub const fn with_max_raster_work(mut self, value: u64) -> Self {
        self.max_raster_work = value;
        self
    }

    pub const fn with_max_decoded_bytes(mut self, value: usize) -> Self {
        self.max_decoded_bytes = value;
        self
    }

    pub const fn max_font_glyphs(self) -> u32 {
        self.max_font_glyphs
    }

    pub const fn with_max_font_glyphs(mut self, value: u32) -> Self {
        self.max_font_glyphs = value;
        self
    }

    pub const fn max_font_codepoints(self) -> u32 {
        self.max_font_codepoints
    }

    pub const fn with_max_font_codepoints(mut self, value: u32) -> Self {
        self.max_font_codepoints = value;
        self
    }

    /// Bounds representation-table parsing and pairwise duplicate checks.
    pub const fn max_font_representations(self) -> u32 {
        self.max_font_representations
    }

    pub const fn with_max_font_representations(mut self, value: u32) -> Self {
        self.max_font_representations = value;
        self
    }

    pub const fn max_font_shaping_bytes(self) -> usize {
        self.max_font_shaping_bytes
    }

    pub const fn with_max_font_shaping_bytes(mut self, value: usize) -> Self {
        self.max_font_shaping_bytes = value;
        self
    }

    pub const fn max_scene_ops(self) -> u32 {
        self.max_scene_ops
    }

    pub const fn with_max_scene_ops(mut self, value: u32) -> Self {
        self.max_scene_ops = value;
        self
    }

    pub const fn max_path_commands(self) -> u32 {
        self.max_path_commands
    }

    pub const fn with_max_path_commands(mut self, value: u32) -> Self {
        self.max_path_commands = value;
        self
    }

    pub const fn max_gradient_stops(self) -> u32 {
        self.max_gradient_stops
    }

    pub const fn with_max_gradient_stops(mut self, value: u32) -> Self {
        self.max_gradient_stops = value;
        self
    }

    pub const fn max_dash_elements(self) -> u32 {
        self.max_dash_elements
    }

    pub const fn with_max_dash_elements(mut self, value: u32) -> Self {
        self.max_dash_elements = value;
        self
    }

    pub const fn max_string_bytes(self) -> usize {
        self.max_string_bytes
    }

    pub const fn with_max_string_bytes(mut self, value: usize) -> Self {
        self.max_string_bytes = value;
        self
    }

    pub const fn max_meta_entries(self) -> u32 {
        self.max_meta_entries
    }

    pub const fn with_max_meta_entries(mut self, value: u32) -> Self {
        self.max_meta_entries = value;
        self
    }

    pub const fn max_meta_bytes(self) -> usize {
        self.max_meta_bytes
    }

    pub const fn with_max_meta_bytes(mut self, value: usize) -> Self {
        self.max_meta_bytes = value;
        self
    }

    pub const fn max_frame_records(self) -> u32 {
        self.max_frame_records
    }

    pub const fn with_max_frame_records(mut self, value: u32) -> Self {
        self.max_frame_records = value;
        self
    }

    pub const fn max_palette_colors(self) -> u32 {
        self.max_palette_colors
    }

    pub const fn with_max_palette_colors(mut self, value: u32) -> Self {
        self.max_palette_colors = value;
        self
    }
}

impl Default for PayloadLimits {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_match_the_published_resource_bounds() {
        let embedded = PayloadLimits::EMBEDDED;
        assert_eq!(embedded.max_decoded_bytes(), 131_072);
        assert_eq!(embedded.max_raster_groups(), 1_024);
        assert_eq!(embedded.max_raster_units(), 65_535);
        assert_eq!(embedded.max_raster_work(), 16_777_216);
        assert_eq!(embedded.max_font_glyphs(), 4_096);
        assert_eq!(embedded.max_font_codepoints(), 16_384);
        assert_eq!(embedded.max_font_representations(), 64);
        assert_eq!(embedded.max_font_shaping_bytes(), 524_288);
        assert_eq!(embedded.max_scene_ops(), 4_096);
        assert_eq!(embedded.max_path_commands(), 16_384);
        assert_eq!(embedded.max_gradient_stops(), 4_096);
        assert_eq!(embedded.max_dash_elements(), 4_096);
        assert_eq!(embedded.max_string_bytes(), 65_536);
        assert_eq!(embedded.max_meta_entries(), 1_024);
        assert_eq!(embedded.max_meta_bytes(), 65_536);
        assert_eq!(embedded.max_frame_records(), 4_096);
        assert_eq!(embedded.max_palette_colors(), 256);

        let host = PayloadLimits::HOST;
        assert_eq!(host.max_decoded_bytes(), 67_108_864);
        assert_eq!(host.max_raster_groups(), 65_535);
        assert_eq!(host.max_raster_units(), 16_777_216);
        assert_eq!(host.max_raster_work(), 1_073_741_824);
        assert_eq!(host.max_font_glyphs(), 65_535);
        assert_eq!(host.max_font_codepoints(), 1_000_000);
        assert_eq!(host.max_font_representations(), 1_024);
        assert_eq!(host.max_font_shaping_bytes(), 67_108_864);
        assert_eq!(host.max_scene_ops(), 1_000_000);
        assert_eq!(host.max_path_commands(), 4_000_000);
        assert_eq!(host.max_gradient_stops(), 1_000_000);
        assert_eq!(host.max_dash_elements(), 1_000_000);
        assert_eq!(host.max_string_bytes(), 16_777_216);
        assert_eq!(host.max_meta_entries(), 65_535);
        assert_eq!(host.max_meta_bytes(), 67_108_864);
        assert_eq!(host.max_frame_records(), 65_535);
        assert_eq!(host.max_palette_colors(), 1_000_000);
    }

    #[test]
    fn default_is_the_embedded_profile() {
        assert_eq!(PayloadLimits::new(), PayloadLimits::EMBEDDED);
        assert_eq!(PayloadLimits::default(), PayloadLimits::EMBEDDED);
    }

    #[test]
    fn custom_profile_builders_cover_every_bound_and_accept_zero() {
        let limits = PayloadLimits::HOST
            .with_max_decoded_bytes(0)
            .with_max_raster_groups(0)
            .with_max_raster_units(0)
            .with_max_raster_work(0)
            .with_max_font_glyphs(1)
            .with_max_font_codepoints(11)
            .with_max_font_representations(0)
            .with_max_font_shaping_bytes(12)
            .with_max_scene_ops(2)
            .with_max_path_commands(3)
            .with_max_gradient_stops(4)
            .with_max_dash_elements(5)
            .with_max_string_bytes(6)
            .with_max_meta_entries(7)
            .with_max_meta_bytes(8)
            .with_max_frame_records(9)
            .with_max_palette_colors(10);

        assert_eq!(limits.max_decoded_bytes(), 0);
        assert_eq!(limits.max_raster_groups(), 0);
        assert_eq!(limits.max_raster_units(), 0);
        assert_eq!(limits.max_raster_work(), 0);
        assert_eq!(limits.max_font_glyphs(), 1);
        assert_eq!(limits.max_font_codepoints(), 11);
        assert_eq!(limits.max_font_representations(), 0);
        assert_eq!(limits.max_font_shaping_bytes(), 12);
        assert_eq!(limits.max_scene_ops(), 2);
        assert_eq!(limits.max_path_commands(), 3);
        assert_eq!(limits.max_gradient_stops(), 4);
        assert_eq!(limits.max_dash_elements(), 5);
        assert_eq!(limits.max_string_bytes(), 6);
        assert_eq!(limits.max_meta_entries(), 7);
        assert_eq!(limits.max_meta_bytes(), 8);
        assert_eq!(limits.max_frame_records(), 9);
        assert_eq!(limits.max_palette_colors(), 10);
    }
}
