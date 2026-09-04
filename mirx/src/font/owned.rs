use alloc::vec::Vec;
use core::mem::size_of;

use super::asset::{Plan, source::Source};
use super::{
    FontAsset, FontCodepointError, FontError, GlyphMap, GlyphMetrics, GlyphPacking,
    GlyphSurfaceAsset, LineMetrics, RawGlyphs, RepresentationAsset,
};
use crate::{
    PayloadLimits,
    image::{
        ColorDescription, EncodedImageAsset, ImageEncodeError, PlaneMemoryLayout, Region,
        SampleLayout, SurfaceDescriptor, UnitGroupRecord,
    },
    media::{CodingTable, DataIntegrity, output::PayloadOutput},
};

/// Owned face metadata and stored samples. Encoded surfaces remain encoded.
/// Structural fields are private so borrowed emission cannot observe invalid references.
#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    codepoints: Vec<char>,
    representations: Vec<Representation>,
    surfaces: Vec<Surface>,
    maps: Vec<Map>,
}

#[derive(Clone, Debug, PartialEq)]
struct Representation {
    metadata: super::FontRepresentation,
    surface: u16,
    line: LineMetrics,
    metrics: Vec<GlyphMetrics>,
    map: Option<u32>,
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
        alignment: u32,
        data: Vec<u8>,
    },
}

impl Font {
    /// Owns validated metadata and stored DATA after bounding every persistent allocation.
    pub fn from_asset(asset: FontAsset<'_>, limits: &PayloadLimits) -> Result<Self, FontError> {
        asset.preflight(limits)?;
        let mut size = OwnedSize::new(limits.max_decoded_bytes());
        size.items::<char>(asset.codepoints().len())?;
        size.items::<Representation>(asset.representations().len())?;
        size.items::<Surface>(asset.surfaces().len())?;
        size.items::<Map>(asset.maps().len())?;
        for r in asset.representations() {
            size.items::<GlyphMetrics>(r.metrics().len())?;
        }
        for map in asset.maps() {
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
                size.items::<u8>(image.index().len())?;
            }
        }
        let codepoints = Self::copy(asset.codepoints())?;
        let mut representations = Self::reserve(asset.representations().len())?;
        for r in asset.representations() {
            representations.push(Representation {
                metadata: r.metadata(),
                surface: r.surface_index(),
                line: r.line_metrics(),
                metrics: Self::copy(r.metrics())?,
                map: r.map_index(),
            });
        }
        let mut maps = Self::reserve(asset.maps().len())?;
        for map in asset.maps() {
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
                        index: Self::copy(image.index())?,
                        partitions: image.integrity().partitions().map(Self::copy).transpose()?,
                        alignment: if image.groups().is_some() {
                            1
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
            codepoints,
            representations,
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

    pub fn codepoints(&self) -> &[char] {
        &self.codepoints
    }
    pub fn representation_count(&self) -> usize {
        self.representations.len()
    }
    pub fn surface_count(&self) -> usize {
        self.surfaces.len()
    }
    pub fn representation(&self, index: usize) -> Option<RepresentationAsset<'_>> {
        self.representations.get(index).map(|r| {
            let value = RepresentationAsset::new(r.metadata, r.surface, r.line, &r.metrics);
            r.map.map_or(value, |map| value.with_map(map))
        })
    }
    pub fn surface(&self, index: usize) -> Option<GlyphSurfaceAsset<'_>> {
        let surface = self.surfaces.get(index)?;
        let map = match surface.packing {
            GlyphPacking::GlyphMajor => {
                GlyphMap::glyph_major(surface.width, surface.height, self.codepoints.len())
                    .expect("validated cells")
            }
            GlyphPacking::Atlas2D => {
                let representation = self
                    .representations
                    .iter()
                    .find(|r| usize::from(r.surface) == index)
                    .expect("referenced surface");
                let regions = &self.maps[representation.map.expect("atlas map") as usize].regions;
                GlyphMap::atlas(surface.width, surface.height, regions)
                    .expect("validated atlas bounds")
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
                .with_index(index)
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

    /// Updates the shared Unicode ordinal without sorting or moving any glyph record.
    pub fn set_codepoint(&mut self, index: usize, codepoint: char) -> Result<(), FontError> {
        if index >= self.codepoints.len() {
            return Err(FontError::GlyphOutOfBounds { index });
        }
        let previous = index
            .checked_sub(1)
            .and_then(|i| self.codepoints.get(i))
            .copied();
        let next = self.codepoints.get(index + 1).copied();
        let violation = previous
            .filter(|&c| c >= codepoint)
            .map(|c| (index, c, codepoint))
            .or_else(|| {
                next.filter(|&c| c <= codepoint)
                    .map(|c| (index + 1, codepoint, c))
            });
        if let Some((index, previous, current)) = violation {
            return Err(FontError::Codepoints(FontCodepointError::NotSorted {
                index,
                previous: previous as u32,
                current: current as u32,
            }));
        }
        self.codepoints[index] = codepoint;
        Ok(())
    }

    pub fn set_line_metrics(
        &mut self,
        index: usize,
        metrics: LineMetrics,
    ) -> Result<(), FontError> {
        self.representations
            .get_mut(index)
            .ok_or(FontError::RepresentationOutOfBounds { index })?
            .line = metrics;
        Ok(())
    }

    /// Mutable signed metrics with fixed cardinality; sample storage is unchanged.
    pub fn glyph_metrics_mut(&mut self, representation: usize) -> Option<&mut [GlyphMetrics]> {
        self.representations
            .get_mut(representation)
            .map(|r| r.metrics.as_mut_slice())
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
    fn codepoints(&self) -> &[char] {
        &self.codepoints
    }
    fn representation_count(&self) -> usize {
        self.representations.len()
    }
    fn surface_count(&self) -> usize {
        self.surfaces.len()
    }
    fn map_count(&self) -> usize {
        self.maps.len()
    }
    fn representation(&self, index: usize) -> Option<RepresentationAsset<'_>> {
        Font::representation(self, index)
    }
    fn map(&self, index: usize) -> Option<GlyphMap<'_>> {
        self.maps.get(index).map(|map| {
            GlyphMap::atlas(map.width, map.height, &map.regions).expect("validated owned map")
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
