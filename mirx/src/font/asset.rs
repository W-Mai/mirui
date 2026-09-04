use alloc::vec::Vec;

pub(super) mod source;
use source::Source;

use super::{
    FontCodepointError, FontError, FontRepresentation, FontRepresentations, GLYPH_METRICS_LEN,
    GLYPH_REGION_LEN, GLYPH_SURFACE_RECORD_LEN, GlyphMap, GlyphMetrics, GlyphPacking,
    GlyphSurfaceRecord, LINE_METRICS_LEN, LineMetrics, REPRESENTATION_RECORD_LEN, RawGlyphs,
    RepresentationRecord,
};
use crate::{
    PayloadLimits,
    image::{
        ColorDescription, EncodedImageAsset, EncodedStoragePlan, PLANE_RECORD_LEN,
        PlaneMemoryLayout, SurfaceDescriptor,
    },
    media::{
        DataIntegrity, INTEGRITY_RECORD_LEN, IntegrityRange, MEDIA_CRC_LEN, MEDIA_HEADER_LEN,
        MEDIA_SECTION_LEN, MEDIA_VERSION, MediaFlags, MediaSectionKind, UnitIndex,
        output::PayloadOutput,
    },
    wire::write_u16_le,
};

/// One representation's size semantics, hinted metrics and shared storage references.
#[derive(Clone, Copy, Debug)]
pub struct RepresentationAsset<'a> {
    metadata: FontRepresentation,
    surface: u16,
    line: LineMetrics,
    metrics: &'a [GlyphMetrics],
    map: Option<u32>,
}

impl<'a> RepresentationAsset<'a> {
    pub const fn new(
        metadata: FontRepresentation,
        surface: u16,
        line: LineMetrics,
        metrics: &'a [GlyphMetrics],
    ) -> Self {
        Self {
            metadata,
            surface,
            line,
            metrics,
            map: None,
        }
    }

    /// References a shared Atlas2D map supplied through `FontAsset::with_maps`.
    /// GlyphMajor representations omit this reference.
    pub const fn with_map(mut self, map: u32) -> Self {
        self.map = Some(map);
        self
    }
    pub const fn metadata(self) -> FontRepresentation {
        self.metadata
    }
    pub const fn surface_index(self) -> u16 {
        self.surface
    }
    pub const fn line_metrics(self) -> LineMetrics {
        self.line
    }
    pub const fn metrics(self) -> &'a [GlyphMetrics] {
        self.metrics
    }
    pub const fn map_index(self) -> Option<u32> {
        self.map
    }
}

/// One shared scalar sample allocation, independent of representation metrics.
#[derive(Clone, Copy, Debug)]
pub enum GlyphSurfaceAsset<'a> {
    Raw {
        glyphs: RawGlyphs<'a, 'a>,
        integrity: DataIntegrity<'a>,
    },
    Encoded {
        map: GlyphMap<'a>,
        image: EncodedImageAsset<'a>,
    },
}

impl<'a> GlyphSurfaceAsset<'a> {
    pub const fn raw(glyphs: RawGlyphs<'a, 'a>) -> Self {
        Self::Raw {
            glyphs,
            integrity: DataIntegrity::Whole,
        }
    }

    pub const fn with_integrity(self, integrity: DataIntegrity<'a>) -> Self {
        match self {
            Self::Raw { glyphs, .. } => Self::Raw { glyphs, integrity },
            Self::Encoded { map, image } => Self::Encoded {
                map,
                image: image.with_integrity(integrity),
            },
        }
    }

    pub const fn map(self) -> GlyphMap<'a> {
        match self {
            Self::Raw { glyphs, .. } => glyphs.map(),
            Self::Encoded { map, .. } => map,
        }
    }
    pub const fn data(self) -> &'a [u8] {
        match self {
            Self::Raw { glyphs, .. } => glyphs.as_bytes(),
            Self::Encoded { image, .. } => image.data(),
        }
    }
    fn descriptor(self) -> SurfaceDescriptor {
        match self {
            Self::Raw { glyphs, .. } => SurfaceDescriptor::new(
                self.map().width(),
                self.map().height(),
                glyphs.sample_layout(),
                ColorDescription::NONE,
            )
            .expect("scalar glyph surface"),
            Self::Encoded { image, .. } => image.surface(),
        }
    }
    pub const fn integrity(self) -> DataIntegrity<'a> {
        match self {
            Self::Raw { integrity, .. } => integrity,
            Self::Encoded { image, .. } => image.integrity(),
        }
    }
    fn plan(self) -> Result<Storage<'a>, FontError> {
        match self {
            Self::Raw { glyphs, .. } => {
                let map = glyphs.map();
                let (width, height) = map.cell_extent().unwrap_or((map.width(), map.height()));
                let plane = glyphs
                    .sample_layout()
                    .plane_geometry(width, height, 0)
                    .expect("scalar geometry");
                let tight = PlaneMemoryLayout::tight(plane).expect("validated tight span");
                Ok(Storage::Raw(
                    (tight != glyphs.memory_layout()).then_some(glyphs.memory_layout()),
                ))
            }
            Self::Encoded { map, image } => {
                let surface = image.surface();
                if !surface.sample_layout().is_alpha()
                    || surface.color() != ColorDescription::NONE
                    || surface.flags().bits() != 0
                    || image.color_table().is_some()
                    || (surface.width(), surface.height()) != (map.width(), map.height())
                {
                    return Err(FontError::SurfaceMismatch);
                }
                EncodedStoragePlan::new(image)
                    .map(Storage::Encoded)
                    .map_err(FontError::Image)
            }
        }
    }
}

/// Borrowed native authoring input for one face with shared codepoints and surfaces.
/// Directory positions and byte offsets are assigned by the writer.
#[derive(Clone, Copy, Debug)]
pub struct FontAsset<'a> {
    codepoints: &'a [char],
    representations: &'a [RepresentationAsset<'a>],
    surfaces: &'a [GlyphSurfaceAsset<'a>],
    maps: &'a [GlyphMap<'a>],
}

impl<'a> FontAsset<'a> {
    pub const fn new(
        codepoints: &'a [char],
        representations: &'a [RepresentationAsset<'a>],
        surfaces: &'a [GlyphSurfaceAsset<'a>],
    ) -> Self {
        Self {
            codepoints,
            representations,
            surfaces,
            maps: &[],
        }
    }
    pub const fn with_maps(mut self, maps: &'a [GlyphMap<'a>]) -> Self {
        self.maps = maps;
        self
    }
    pub const fn codepoints(self) -> &'a [char] {
        self.codepoints
    }
    pub const fn representations(self) -> &'a [RepresentationAsset<'a>] {
        self.representations
    }
    pub const fn surfaces(self) -> &'a [GlyphSurfaceAsset<'a>] {
        self.surfaces
    }
    pub const fn maps(self) -> &'a [GlyphMap<'a>] {
        self.maps
    }

    /// Exact canonical size after structural validation; DATA is not decoded.
    pub fn encoded_len(self) -> Result<usize, FontError> {
        Ok(Plan::new(self)?.len)
    }

    /// Required payload placement for every aligned DATA body, independent of its pointer.
    pub fn input_alignment(self) -> Result<u32, FontError> {
        Plan::new(self)?;
        self.surfaces.iter().try_fold(1, |alignment, surface| {
            Ok(alignment.max(surface.plan()?.alignment()))
        })
    }

    /// Writes only after validation and capacity checks; the output suffix is untouched.
    pub fn encode_into(self, output: &mut [u8]) -> Result<usize, FontError> {
        let plan = Plan::new(self)?;
        if output.len() < plan.len {
            return Err(FontError::BufferTooSmall {
                needed: plan.len,
                available: output.len(),
            });
        }
        plan.emit(PayloadOutput::buffer(&mut output[..plan.len]));
        Ok(plan.len)
    }

    /// Allocates one complete payload; no extra backing-address alignment is promised.
    pub fn encode(self) -> Result<Vec<u8>, FontError> {
        let plan = Plan::new(self)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(plan.len)
            .map_err(|_| FontError::AllocationFailed)?;
        bytes.resize(plan.len, 0);
        plan.emit(PayloadOutput::buffer(&mut bytes));
        Ok(bytes)
    }

    pub fn matches_payload(self, bytes: &[u8]) -> Result<bool, FontError> {
        let plan = Plan::new(self)?;
        Ok(bytes.len() == plan.len && plan.emit(PayloadOutput::comparison(bytes)))
    }

    /// Checks scalar syntax with resource-wide counters before owning output is allocated.
    pub fn preflight(self, limits: &PayloadLimits) -> Result<(), FontError> {
        Source::preflight(self, limits)
    }
}

#[derive(Clone, Copy)]
enum Storage<'a> {
    Raw(Option<PlaneMemoryLayout>),
    Encoded(EncodedStoragePlan<'a>),
}
impl Storage<'_> {
    fn sections(self) -> [Option<(MediaSectionKind, usize)>; 3] {
        match self {
            Self::Raw(plane) => [
                plane.map(|_| (MediaSectionKind::PLANES, PLANE_RECORD_LEN)),
                None,
                None,
            ],
            Self::Encoded(plan) => plan.sections(),
        }
    }
    fn alignment(self) -> u32 {
        match self {
            Self::Raw(memory) => memory.map_or(1, |p| p.required_alignment()),
            Self::Encoded(plan) => plan.alignment(),
        }
    }
    fn emit(self, output: &mut PayloadOutput<'_>) {
        match self {
            Self::Raw(Some(memory)) => {
                let mut bytes = [0; PLANE_RECORD_LEN];
                memory
                    .encode_record_into(&mut bytes)
                    .expect("validated plane");
                output.write(&bytes);
            }
            Self::Raw(None) => {}
            Self::Encoded(plan) => plan.emit_metadata(output),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Plan<S: Source> {
    asset: S,
    sections: u16,
    metadata_end: usize,
    pub(super) len: usize,
    integrity_len: usize,
    indexed: bool,
}

impl<S: Source> Plan<S> {
    fn table_size(asset: S) -> Result<u64, FontError> {
        let lengths = [
            asset.codepoints().len(),
            asset.representation_count(),
            asset.surface_count(),
            asset.map_count(),
        ];
        for length in lengths {
            u32::try_from(length).map_err(|_| FontError::SizeOverflow)?;
        }
        let [glyphs, representations, surfaces, maps] = lengths.map(|n| n as u64);
        let metrics = representations
            .checked_mul(LINE_METRICS_LEN as u64 + glyphs * GLYPH_METRICS_LEN as u64)
            .ok_or(FontError::SizeOverflow)?;
        let map_bytes = maps
            .checked_mul(glyphs * GLYPH_REGION_LEN as u64)
            .ok_or(FontError::SizeOverflow)?;
        let size = (glyphs * 4
            + representations * REPRESENTATION_RECORD_LEN as u64
            + surfaces * GLYPH_SURFACE_RECORD_LEN as u64)
            .checked_add(metrics)
            .and_then(|n| n.checked_add(map_bytes))
            .ok_or(FontError::SizeOverflow)?;
        u32::try_from(size).map_err(|_| FontError::SizeOverflow)?;
        Ok(size)
    }

    pub(super) fn new(asset: S) -> Result<Self, FontError> {
        let plan = Self::metadata(asset)?;
        for surface in asset.surfaces() {
            if let Storage::Encoded(storage) = surface.plan()? {
                storage.validate().map_err(FontError::Image)?;
            }
        }
        Ok(plan)
    }

    fn metadata(asset: S) -> Result<Self, FontError> {
        let mut size = Self::table_size(asset)?;
        if asset.codepoints().is_empty() {
            return Err(FontError::EmptyGlyphTable);
        }
        if asset.surface_count() > asset.representation_count()
            || asset.map_count() > asset.representation_count()
        {
            return Err(FontError::UnreferencedStorage);
        }
        for (index, pair) in asset.codepoints().windows(2).enumerate() {
            if pair[0] >= pair[1] {
                return Err(FontError::Codepoints(FontCodepointError::NotSorted {
                    index: index + 1,
                    previous: pair[0] as u32,
                    current: pair[1] as u32,
                }));
            }
        }
        FontRepresentations::validate_by(asset.representation_count(), |i| {
            asset
                .representation(i)
                .expect("representation ordinal")
                .metadata
        })
        .map_err(FontError::Selection)?;
        for (index, representation) in asset.representations().enumerate() {
            let surface = asset.surface(usize::from(representation.surface)).ok_or(
                FontError::SurfaceOutOfBounds {
                    representation: index,
                },
            )?;
            if representation.metrics.len() != asset.codepoints().len()
                || surface.map().len() != asset.codepoints().len()
            {
                return Err(FontError::Cardinality);
            }
            RepresentationRecord::new(representation.metadata, representation.surface)
                .validate_for(surface.descriptor())
                .map_err(FontError::Record)?;
            match (surface.map().packing(), representation.map) {
                (GlyphPacking::GlyphMajor, None) => {}
                (GlyphPacking::Atlas2D, Some(id)) => {
                    let map = asset.map(id as usize).ok_or(FontError::MapOutOfBounds {
                        representation: index,
                    })?;
                    if map.packing() != GlyphPacking::Atlas2D
                        || map.len() != asset.codepoints().len()
                        || map.iter().any(|region| {
                            region.right() > surface.map().width()
                                || region.bottom() > surface.map().height()
                        })
                    {
                        return Err(FontError::MapMismatch {
                            representation: index,
                        });
                    }
                }
                _ => {
                    return Err(FontError::MapMismatch {
                        representation: index,
                    });
                }
            }
        }
        for index in 0..asset.map_count() {
            if !asset
                .representations()
                .any(|r| r.map.map(|id| id as usize) == Some(index))
            {
                return Err(FontError::UnreferencedStorage);
            }
        }
        let mut sections = 4 + usize::from(asset.map_count() != 0) + asset.surface_count();
        let indexed = asset
            .surfaces()
            .any(|s| s.integrity().partitions().is_some());
        let mut integrity_len = 0u64;
        for (index, surface) in asset.surfaces().enumerate() {
            if !asset
                .representations()
                .any(|r| usize::from(r.surface) == index)
            {
                return Err(FontError::UnreferencedStorage);
            }
            let storage = surface.plan()?;
            surface
                .integrity()
                .section_len(surface.data().len())
                .map_err(FontError::Integrity)?;
            for (_, len) in storage.sections().into_iter().flatten() {
                sections += 1;
                size += len as u64;
            }
            if indexed {
                let count = surface
                    .integrity()
                    .partitions()
                    .map_or(usize::from(!surface.data().is_empty()), <[u32]>::len);
                integrity_len += count as u64 * INTEGRITY_RECORD_LEN as u64;
            }
        }
        sections += usize::from(indexed);
        let sections = u16::try_from(sections).map_err(|_| FontError::SizeOverflow)?;
        size += integrity_len
            + MEDIA_HEADER_LEN as u64
            + u64::from(sections) * MEDIA_SECTION_LEN as u64;
        let metadata_end = u32::try_from(size).map_err(|_| FontError::SizeOverflow)? as usize;
        let mut end = metadata_end;
        for surface in asset.surfaces() {
            end = Self::aligned(end, surface.plan()?.alignment())?;
            end = end
                .checked_add(surface.data().len())
                .ok_or(FontError::SizeOverflow)?;
        }
        let len = end
            .checked_add(if indexed { 0 } else { MEDIA_CRC_LEN })
            .ok_or(FontError::SizeOverflow)?;
        u32::try_from(len).map_err(|_| FontError::SizeOverflow)?;
        Ok(Self {
            asset,
            sections,
            metadata_end,
            len,
            integrity_len: integrity_len as usize,
            indexed,
        })
    }

    fn aligned(offset: usize, alignment: u32) -> Result<usize, FontError> {
        UnitIndex::aligned(
            u32::try_from(offset).map_err(|_| FontError::SizeOverflow)?,
            alignment,
        )
        .map(|n| n as usize)
        .map_err(|_| FontError::SizeOverflow)
    }

    fn metadata_sections(self) -> [(MediaSectionKind, usize); 5] {
        let n = self.asset.codepoints().len();
        let r = self.asset.representation_count();
        [
            (MediaSectionKind::CODEPOINTS, n * 4),
            (
                MediaSectionKind::REPRESENTATIONS,
                r * REPRESENTATION_RECORD_LEN,
            ),
            (
                MediaSectionKind::METRICS,
                r * (LINE_METRICS_LEN + n * GLYPH_METRICS_LEN),
            ),
            (
                MediaSectionKind::SURFACE_GROUPS,
                self.asset.surface_count() * GLYPH_SURFACE_RECORD_LEN,
            ),
            (
                MediaSectionKind::GLYPH_MAPS,
                self.asset.map_count() * n * GLYPH_REGION_LEN,
            ),
        ]
    }

    fn visit_data(&self, mut visitor: impl FnMut(GlyphSurfaceAsset<'_>, usize)) {
        let mut offset = self.metadata_end;
        for surface in self.asset.surfaces() {
            offset = Self::aligned(
                offset,
                surface.plan().expect("validated storage").alignment(),
            )
            .expect("validated span");
            visitor(surface, offset);
            offset += surface.data().len();
        }
    }

    pub(super) fn emit(self, mut output: PayloadOutput<'_>) -> bool {
        let mut header = [0; MEDIA_HEADER_LEN];
        header[0] = MEDIA_VERSION;
        header[1] = if self.indexed {
            MediaFlags::INDEXED_INTEGRITY.bits()
        } else {
            0
        };
        write_u16_le(&mut header, 2, self.sections);
        output.header(&header);
        let mut offset = MEDIA_HEADER_LEN + usize::from(self.sections) * MEDIA_SECTION_LEN;
        for (kind, len) in self.metadata_sections() {
            if kind == MediaSectionKind::GLYPH_MAPS && len == 0 {
                continue;
            }
            output.section(kind, offset, len);
            offset += len;
        }
        for surface in self.asset.surfaces() {
            for (kind, len) in surface
                .plan()
                .expect("validated storage")
                .sections()
                .into_iter()
                .flatten()
            {
                output.section(kind, offset, len);
                offset += len;
            }
        }
        if self.indexed {
            output.section(MediaSectionKind::INTEGRITY, offset, self.integrity_len);
        }
        self.visit_data(|surface, offset| {
            output.section(MediaSectionKind::DATA, offset, surface.data().len())
        });
        for &codepoint in self.asset.codepoints() {
            output.write(&(codepoint as u32).to_le_bytes());
        }
        for r in self.asset.representations() {
            let mut bytes = [0; REPRESENTATION_RECORD_LEN];
            RepresentationRecord::new(r.metadata, r.surface)
                .with_glyph_map_offset(r.map.map_or(0, |id| {
                    id * self.asset.codepoints().len() as u32 * GLYPH_REGION_LEN as u32
                }))
                .encode_record_into(&mut bytes)
                .expect("validated representation");
            output.write(&bytes);
        }
        for r in self.asset.representations() {
            output.write(&r.line.encode_record());
            for metric in r.metrics {
                output.write(&metric.encode_record());
            }
        }
        let data_base = self.sections - self.asset.surface_count() as u16;
        let mut section = 4 + u16::from(self.asset.map_count() != 0);
        for (index, surface) in self.asset.surfaces().enumerate() {
            let map = surface.map();
            let (width, height) = map.cell_extent().unwrap_or((map.width(), map.height()));
            let mut record = GlyphSurfaceRecord::new(
                surface.descriptor().sample_layout(),
                map.packing(),
                width,
                height,
                data_base + index as u16,
            )
            .expect("validated surface");
            let mut index_section = None;
            let mut group_section = None;
            for (kind, _) in surface
                .plan()
                .expect("validated storage")
                .sections()
                .into_iter()
                .flatten()
            {
                match kind {
                    MediaSectionKind::PLANES => {
                        record = record.with_planes(section).expect("RAW storage")
                    }
                    MediaSectionKind::CODINGS => {
                        record = record.with_codings(section).expect("encoded storage")
                    }
                    MediaSectionKind::UNIT_GROUPS => group_section = Some(section),
                    MediaSectionKind::UNIT_INDEX => index_section = Some(section),
                    _ => unreachable!(),
                }
                section += 1;
            }
            if let Some(group) = group_section {
                record = record
                    .with_groups(group, index_section)
                    .expect("encoded groups");
            }
            let mut bytes = [0; GLYPH_SURFACE_RECORD_LEN];
            record
                .encode_record_into(&mut bytes)
                .expect("validated surface record");
            output.write(&bytes);
        }
        for map in self.asset.maps() {
            for region in map {
                output.write(&region.x().to_le_bytes());
                output.write(&region.y().to_le_bytes());
                output.write(&region.width().to_le_bytes());
                output.write(&region.height().to_le_bytes());
            }
        }
        for surface in self.asset.surfaces() {
            surface.plan().expect("validated storage").emit(&mut output);
        }
        if self.indexed {
            self.visit_data(|surface, offset| {
                let mut start = 0;
                let end = surface.data().len() as u32;
                let whole = [end];
                let ends =
                    surface
                        .integrity()
                        .partitions()
                        .unwrap_or(if end == 0 { &[] } else { &whole });
                for &end in ends {
                    let crc = crate::crc32(&surface.data()[start as usize..end as usize]);
                    let range =
                        IntegrityRange::new(offset as u32 + start..offset as u32 + end, crc)
                            .expect("validated integrity range");
                    output.write(&range.encode_record());
                    start = end;
                }
            });
        }
        debug_assert_eq!(output.position(), self.metadata_end);
        self.visit_data(|surface, offset| {
            output.begin_metadata();
            output.pad_to(offset);
            output.begin_data(if self.indexed {
                DataIntegrity::Indexed(&[])
            } else {
                DataIntegrity::Whole
            });
            output.write(surface.data());
        });
        debug_assert_eq!(
            output.position() + if self.indexed { 0 } else { MEDIA_CRC_LEN },
            self.len
        );
        output.finish()
    }
}

#[cfg(test)]
mod tests;
