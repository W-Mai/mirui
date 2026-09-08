use alloc::vec::Vec;
use core::mem::size_of;

use super::asset::{Plan, source::Source};
use super::{
    CmapEntry, FontAdvanceSource, FontAsset, FontError, FontFace, GlyphId, GlyphMap, GlyphPacking,
    GlyphSurfaceAsset, RasterMetrics, RawGlyphs, RepresentationAsset,
};
use crate::{
    ByteAlignment, Fixed, PayloadLimits,
    image::{
        AtlasMap, ColorDescription, EncodedImageAsset, ImageEncodeError, PlaneMemoryLayout, Region,
        SampleLayout, SurfaceDescriptor, UnitGroupRecord,
    },
    media::{CodingTable, DataIntegrity, output::PayloadOutput},
};

/// Owned face metadata and stored samples. Encoded surfaces remain encoded.
/// Structural fields are private so borrowed emission cannot observe invalid references.
#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    face: FontFace,
    cmap: Vec<CmapEntry>,
    glyph_ids: Option<Vec<GlyphId>>,
    advance_source: AdvanceSource,
    representations: Vec<Representation>,
    raster_metrics: Vec<RasterMetrics>,
    surfaces: Vec<Surface>,
    maps: Vec<Map>,
}

#[derive(Clone, Debug, PartialEq)]
struct Representation {
    metadata: super::FontRepresentation,
    surface: u16,
    atlas_map: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
enum AdvanceSource {
    Advances(Vec<Fixed>),
    Shaping(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq)]
struct Map {
    width: u32,
    height: u32,
    regions: Vec<Region>,
}

#[derive(Clone, Debug, PartialEq)]
struct Surface {
    width: u32,
    height: u32,
    layout: SampleLayout,
    packing: GlyphPacking,
    storage: Storage,
}

#[derive(Clone, Debug, PartialEq)]
enum Storage {
    Raw {
        memory: PlaneMemoryLayout,
        data: Vec<u8>,
        partitions: Option<Vec<u32>>,
    },
    Encoded {
        codings: Vec<u8>,
        groups: Option<Vec<UnitGroupRecord>>,
        index: Vec<u8>,
        partitions: Option<Vec<u32>>,
        alignment: ByteAlignment,
        data: Vec<u8>,
    },
}

impl Font {
    /// Owns validated metadata and stored DATA after bounding every persistent allocation.
    pub fn from_asset(asset: FontAsset<'_>, limits: &PayloadLimits) -> Result<Self, FontError> {
        asset.preflight(limits)?;
        let mut size = OwnedSize::new(limits.max_decoded_bytes());
        size.items::<CmapEntry>(asset.cmap().len())?;
        size.items::<GlyphId>(asset.glyph_ids().map_or(0, <[GlyphId]>::len))?;
        match asset.advance_source() {
            FontAdvanceSource::Advances(values) => size.items::<Fixed>(values.len())?,
            FontAdvanceSource::Shaping(bytes) => size.items::<u8>(bytes.len())?,
        }
        size.items::<Representation>(asset.representations().len())?;
        size.items::<RasterMetrics>(asset.raster_metrics().len())?;
        size.items::<Surface>(asset.surfaces().len())?;
        size.items::<Map>(asset.atlas_maps().len())?;
        for map in asset.atlas_maps() {
            size.items::<Region>(map.len())?;
        }
        for surface in asset.surfaces() {
            size.items::<u8>(surface.data().len())?;
            size.items::<u32>(surface.integrity().partitions().map_or(0, <[u32]>::len))?;
            if let GlyphSurfaceAsset::Encoded { image, .. } = surface {
                size.items::<u8>(
                    CodingTable::encoded_iter_len(image.codings())
                        .map_err(|e| FontError::Image(ImageEncodeError::Codings(e)))?,
                )?;
                size.items::<UnitGroupRecord>(image.groups().map_or(0, <[UnitGroupRecord]>::len))?;
                size.items::<u8>(image.unit_index().len())?;
            }
        }
        let cmap = Self::copy(asset.cmap())?;
        let glyph_ids = asset.glyph_ids().map(Self::copy).transpose()?;
        let advance_source = match asset.advance_source() {
            FontAdvanceSource::Advances(values) => AdvanceSource::Advances(Self::copy(values)?),
            FontAdvanceSource::Shaping(bytes) => AdvanceSource::Shaping(Self::copy(bytes)?),
        };
        let mut representations = Self::reserve(asset.representations().len())?;
        for r in asset.representations() {
            representations.push(Representation {
                metadata: r.metadata(),
                surface: r.surface_index(),
                atlas_map: r.atlas_map_index(),
            });
        }
        let raster_metrics = Self::copy(asset.raster_metrics())?;
        let mut maps = Self::reserve(asset.atlas_maps().len())?;
        for map in asset.atlas_maps() {
            let mut regions = Self::reserve(map.len())?;
            regions.extend(map.iter());
            maps.push(Map {
                width: map.width(),
                height: map.height(),
                regions,
            });
        }
        let mut surfaces = Self::reserve(asset.surfaces().len())?;
        for &surface in asset.surfaces() {
            let map = surface.map();
            let (width, height) = map.cell_extent().unwrap_or((map.width(), map.height()));
            let layout;
            let storage = match surface {
                GlyphSurfaceAsset::Raw {
                    glyphs: raw,
                    integrity,
                } => {
                    layout = raw.sample_layout();
                    Storage::Raw {
                        memory: raw.memory_layout(),
                        data: Self::copy(raw.as_bytes())?,
                        partitions: integrity.partitions().map(Self::copy).transpose()?,
                    }
                }
                GlyphSurfaceAsset::Encoded { image, .. } => {
                    layout = image.surface().sample_layout();
                    let len = CodingTable::encoded_iter_len(image.codings())
                        .map_err(|e| FontError::Image(ImageEncodeError::Codings(e)))?;
                    let mut codings = Self::reserve(len)?;
                    codings.resize(len, 0);
                    CodingTable::encode_iter_into(image.codings(), &mut codings)
                        .expect("validated coding body");
                    Storage::Encoded {
                        codings,
                        groups: image.groups().map(Self::copy).transpose()?,
                        index: Self::copy(image.unit_index())?,
                        partitions: image.integrity().partitions().map(Self::copy).transpose()?,
                        alignment: if image.groups().is_some() {
                            ByteAlignment::ONE
                        } else {
                            image.input_alignment().map_err(FontError::Image)?
                        },
                        data: Self::copy(image.data())?,
                    }
                }
            };
            surfaces.push(Surface {
                width,
                height,
                layout,
                packing: map.packing(),
                storage,
            });
        }
        Ok(Self {
            face: asset.face(),
            cmap,
            glyph_ids,
            advance_source,
            representations,
            raster_metrics,
            surfaces,
            maps,
        })
    }

    fn reserve<T>(len: usize) -> Result<Vec<T>, FontError> {
        let mut values = Vec::new();
        values
            .try_reserve_exact(len)
            .map_err(|_| FontError::AllocationFailed)?;
        Ok(values)
    }
    fn copy<T: Copy>(values: &[T]) -> Result<Vec<T>, FontError> {
        let mut owned = Self::reserve(values.len())?;
        owned.extend_from_slice(values);
        Ok(owned)
    }

    pub const fn face(&self) -> FontFace {
        self.face
    }
    pub fn cmap(&self) -> &[CmapEntry] {
        &self.cmap
    }
    pub fn glyph_ids(&self) -> Option<&[GlyphId]> {
        self.glyph_ids.as_deref()
    }
    pub fn advance_source(&self) -> FontAdvanceSource<'_> {
        match &self.advance_source {
            AdvanceSource::Advances(values) => FontAdvanceSource::Advances(values),
            AdvanceSource::Shaping(bytes) => FontAdvanceSource::Shaping(bytes),
        }
    }
    pub fn raster_metrics(&self) -> &[RasterMetrics] {
        &self.raster_metrics
    }
    pub fn representation_count(&self) -> usize {
        self.representations.len()
    }
    pub fn surface_count(&self) -> usize {
        self.surfaces.len()
    }
    pub fn representation(&self, index: usize) -> Option<RepresentationAsset> {
        self.representations.get(index).map(|r| {
            let value = RepresentationAsset::new(r.metadata, r.surface);
            r.atlas_map.map_or(value, |map| value.with_atlas_map(map))
        })
    }
    pub fn surface(&self, index: usize) -> Option<GlyphSurfaceAsset<'_>> {
        let surface = self.surfaces.get(index)?;
        let map = match surface.packing {
            GlyphPacking::GlyphMajor => GlyphMap::cells(
                surface.width,
                surface.height,
                usize::from(self.face.raster_count()),
            )
            .expect("validated cells"),
            GlyphPacking::Atlas2D => {
                let representation = self
                    .representations
                    .iter()
                    .find(|r| usize::from(r.surface) == index)
                    .expect("referenced surface");
                let regions =
                    &self.maps[representation.atlas_map.expect("atlas map") as usize].regions;
                GlyphMap::atlas(
                    AtlasMap::new(surface.width, surface.height, regions)
                        .expect("validated atlas bounds"),
                )
            }
        };
        Some(match &surface.storage {
            Storage::Raw {
                memory,
                data,
                partitions,
            } => GlyphSurfaceAsset::raw(
                RawGlyphs::builder(map, surface.layout)
                    .with_memory_layout(*memory)
                    .build(data)
                    .expect("validated RAW storage"),
            )
            .with_integrity(
                partitions
                    .as_deref()
                    .map_or(DataIntegrity::Whole, DataIntegrity::Indexed),
            ),
            Storage::Encoded {
                codings,
                groups,
                index,
                partitions,
                alignment,
                data,
            } => {
                let descriptor = SurfaceDescriptor::new(
                    map.width(),
                    map.height(),
                    surface.layout,
                    ColorDescription::NONE,
                )
                .expect("scalar surface");
                let mut image = EncodedImageAsset::from_codings(
                    descriptor,
                    CodingTable::open(codings).expect("validated coding table"),
                    data,
                )
                .with_unit_index(index)
                .with_input_alignment(*alignment)
                .with_integrity(
                    partitions
                        .as_deref()
                        .map_or(DataIntegrity::Whole, DataIntegrity::Indexed),
                );
                if let Some(groups) = groups {
                    image = image.with_groups(groups);
                }
                GlyphSurfaceAsset::Encoded { map, image }
            }
        })
    }

    pub fn set_cmap_entry(&mut self, index: usize, entry: CmapEntry) -> Result<(), FontError> {
        if index >= self.cmap.len() {
            return Err(FontError::GlyphOutOfBounds { index });
        }
        let previous = index.checked_sub(1).and_then(|i| self.cmap.get(i)).copied();
        let next = self.cmap.get(index + 1).copied();
        let violation = previous
            .filter(|value| value.scalar() >= entry.scalar())
            .map(|value| (index, value.scalar(), entry.scalar()))
            .or_else(|| {
                next.filter(|value| value.scalar() <= entry.scalar())
                    .map(|value| (index + 1, entry.scalar(), value.scalar()))
            });
        if let Some((index, previous, current)) = violation {
            return Err(FontError::Cmap(super::CmapIndexError::NotSorted {
                index,
                previous,
                current,
            }));
        }
        if Self::raster_ordinal(self.glyph_ids(), self.face.raster_count(), entry.glyph_id())
            .is_none()
        {
            return Err(FontError::MissingRasterGlyph(entry.glyph_id()));
        }
        self.cmap[index] = entry;
        Ok(())
    }

    fn raster_ordinal(ids: Option<&[GlyphId]>, count: u16, glyph_id: GlyphId) -> Option<usize> {
        match ids {
            Some(ids) => ids.binary_search(&glyph_id).ok(),
            None => {
                let ordinal = usize::from(glyph_id.get());
                (ordinal < usize::from(count)).then_some(ordinal)
            }
        }
    }

    pub fn advances_mut(&mut self) -> Option<&mut [Fixed]> {
        match &mut self.advance_source {
            AdvanceSource::Advances(values) => Some(values),
            AdvanceSource::Shaping(_) => None,
        }
    }

    pub fn raster_metrics_mut(&mut self) -> &mut [RasterMetrics] {
        &mut self.raster_metrics
    }

    pub fn preflight(&self, limits: &PayloadLimits) -> Result<(), FontError> {
        Source::preflight(self, limits)
    }
    pub fn encoded_len(&self) -> Result<usize, FontError> {
        Ok(Plan::new(self)?.len)
    }
    pub fn encode_into(&self, output: &mut [u8]) -> Result<usize, FontError> {
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
    pub fn encode(&self) -> Result<Vec<u8>, FontError> {
        let plan = Plan::new(self)?;
        let mut bytes = Self::reserve(plan.len)?;
        bytes.resize(plan.len, 0);
        plan.emit(PayloadOutput::buffer(&mut bytes));
        Ok(bytes)
    }
    pub fn matches_payload(&self, bytes: &[u8]) -> Result<bool, FontError> {
        let plan = Plan::new(self)?;
        Ok(bytes.len() == plan.len && plan.emit(PayloadOutput::comparison(bytes)))
    }
}

impl Source for &Font {
    fn face(&self) -> FontFace {
        self.face
    }
    fn cmap(&self) -> &[CmapEntry] {
        &self.cmap
    }
    fn glyph_ids(&self) -> Option<&[GlyphId]> {
        self.glyph_ids.as_deref()
    }
    fn advance_source(&self) -> FontAdvanceSource<'_> {
        Font::advance_source(self)
    }
    fn raster_metrics(&self) -> &[RasterMetrics] {
        &self.raster_metrics
    }
    fn representation_count(&self) -> usize {
        self.representations.len()
    }
    fn surface_count(&self) -> usize {
        self.surfaces.len()
    }
    fn atlas_map_count(&self) -> usize {
        self.maps.len()
    }
    fn representation(&self, index: usize) -> Option<RepresentationAsset> {
        Font::representation(self, index)
    }
    fn atlas_map(&self, index: usize) -> Option<AtlasMap<'_>> {
        self.maps.get(index).map(|map| {
            AtlasMap::new(map.width, map.height, &map.regions).expect("validated owned map")
        })
    }
    fn surface(&self, index: usize) -> Option<GlyphSurfaceAsset<'_>> {
        Font::surface(self, index)
    }
}

struct OwnedSize {
    used: usize,
    limit: usize,
}
impl OwnedSize {
    fn new(limit: usize) -> Self {
        Self { used: 0, limit }
    }
    fn items<T>(&mut self, len: usize) -> Result<(), FontError> {
        self.used = len
            .checked_mul(size_of::<T>())
            .and_then(|n| self.used.checked_add(n))
            .ok_or(FontError::SizeOverflow)?;
        if self.used > self.limit {
            return Err(FontError::OwnedBytesLimitExceeded {
                needed: self.used,
                limit: self.limit,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

mod decode;
