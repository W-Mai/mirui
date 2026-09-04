#![doc = include_str!("../../docs/font-tables.md")]

use super::{
    FontCodepoints, FontRepresentationRequest, FontSelectionError, GLYPH_METRICS_LEN,
    GLYPH_REGION_LEN, GlyphMap, GlyphMapError, GlyphPacking, GlyphSurfaceRecord, LINE_METRICS_LEN,
    MetricsError, MetricsTable, RepresentationRecord, RepresentationTable,
};

/// Joined face metadata with one shared Unicode ordinal space.
///
/// Representation-specific metrics and maps are validated at construction;
/// lookup does not rescan atlas records or allocate decoded metadata.
/// Media-directory admission, storage and DATA integrity remain separate.
#[derive(Clone, Copy, Debug)]
pub struct FaceTables<'a> {
    codepoints: FontCodepoints<'a>,
    representations: RepresentationTable<'a>,
    metrics: &'a [u8],
    maps: &'a [u8],
    metric_table_len: usize,
    map_table_len: usize,
}

impl<'a> FaceTables<'a> {
    /// Joins exact METRICS and GLYPH_MAPS bodies to validated shared tables.
    /// Count limits are inherited from `RepresentationTable::open`.
    pub fn new(
        codepoints: FontCodepoints<'a>,
        representations: RepresentationTable<'a>,
        metrics: &'a [u8],
        maps: &'a [u8],
    ) -> Result<Self, FaceTablesError> {
        let count = codepoints.len();
        if count != representations.glyph_count() {
            return Err(FaceTablesError::Cardinality {
                codepoints: count,
                glyphs: representations.glyph_count(),
            });
        }
        let metric_table_len = count
            .checked_mul(GLYPH_METRICS_LEN)
            .and_then(|size| size.checked_add(LINE_METRICS_LEN))
            .ok_or(FaceTablesError::SizeOverflow)?;
        let expected = metric_table_len
            .checked_mul(representations.len())
            .ok_or(FaceTablesError::SizeOverflow)?;
        u32::try_from(expected).map_err(|_| FaceTablesError::SizeOverflow)?;
        if metrics.len() != expected {
            return Err(FaceTablesError::MetricsLength {
                expected,
                actual: metrics.len(),
            });
        }
        let map_table_len = count
            .checked_mul(GLYPH_REGION_LEN)
            .ok_or(FaceTablesError::SizeOverflow)?;
        u32::try_from(maps.len()).map_err(|_| FaceTablesError::SizeOverflow)?;
        if (map_table_len == 0 && !maps.is_empty())
            || (map_table_len != 0 && maps.len() % map_table_len != 0)
        {
            return Err(FaceTablesError::MapsLength {
                table_len: map_table_len,
                actual: maps.len(),
            });
        }
        let table = Self {
            codepoints,
            representations,
            metrics,
            maps,
            metric_table_len,
            map_table_len,
        };
        for (index, record) in representations.iter().enumerate() {
            table
                .metrics_at(index)
                .map_err(|error| FaceTablesError::Metrics {
                    representation: index,
                    error,
                })?;
            let surface = representations.surface(record);
            let offset = record.glyph_map_offset() as usize;
            match surface.packing() {
                GlyphPacking::GlyphMajor => {
                    if offset != 0 {
                        return Err(FaceTablesError::ImplicitMapOffset {
                            representation: index,
                            offset: record.glyph_map_offset(),
                        });
                    }
                    GlyphMap::glyph_major(surface.width(), surface.height(), count).map_err(
                        |error| FaceTablesError::Map {
                            representation: index,
                            error,
                        },
                    )?;
                }
                GlyphPacking::Atlas2D => {
                    if (map_table_len == 0 && offset != 0)
                        || (map_table_len != 0 && offset % map_table_len != 0)
                    {
                        return Err(FaceTablesError::MapOffset {
                            representation: index,
                            offset: record.glyph_map_offset(),
                        });
                    }
                    let end = offset
                        .checked_add(map_table_len)
                        .ok_or(FaceTablesError::SizeOverflow)?;
                    let bytes = maps
                        .get(offset..end)
                        .ok_or(FaceTablesError::MapOutOfBounds {
                            representation: index,
                        })?;
                    GlyphMap::from_records(surface.width(), surface.height(), bytes).map_err(
                        |error| FaceTablesError::Map {
                            representation: index,
                            error,
                        },
                    )?;
                }
            }
        }
        // At most one distinct table per representation; reject unclaimed bodies
        // before bounded reference scans rather than traversing arbitrary padding.
        let map_count = maps.len().checked_div(map_table_len).unwrap_or(0);
        if map_count > representations.len() {
            return Err(FaceTablesError::UnreferencedMaps);
        }
        for map_index in 0..map_count {
            let offset = map_index * map_table_len;
            if !representations.iter().any(|record| {
                record.glyph_map_offset() as usize == offset
                    && representations.surface(record).packing() == GlyphPacking::Atlas2D
            }) {
                return Err(FaceTablesError::UnreferencedMaps);
            }
        }
        Ok(table)
    }

    pub const fn len(self) -> usize {
        self.representations.len()
    }
    pub const fn is_empty(self) -> bool {
        self.representations.is_empty()
    }
    pub const fn glyph_count(self) -> usize {
        self.codepoints.len()
    }
    pub const fn codepoints(self) -> FontCodepoints<'a> {
        self.codepoints
    }
    pub const fn representations(self) -> RepresentationTable<'a> {
        self.representations
    }

    /// Resolves one representation in constant time, including its borrowed map.
    pub fn get(self, index: usize) -> Option<FaceRepresentation<'a>> {
        let record = self.representations.get(index)?;
        let surface = self.representations.surface(record);
        let map = match surface.packing() {
            GlyphPacking::GlyphMajor => {
                GlyphMap::glyph_major(surface.width(), surface.height(), self.glyph_count())
                    .expect("validated glyph geometry")
            }
            GlyphPacking::Atlas2D => {
                let offset = record.glyph_map_offset() as usize;
                GlyphMap::from_validated_records(
                    surface.width(),
                    surface.height(),
                    &self.maps[offset..offset + self.map_table_len],
                )
            }
        };
        Some(FaceRepresentation {
            index,
            used_fallback: false,
            record,
            surface,
            map,
            metrics: self.metrics_at(index).expect("validated metric prefix"),
        })
    }

    /// Selects size metadata, metrics and geometry through one representation index.
    pub fn select(
        self,
        request: FontRepresentationRequest,
    ) -> Result<FaceRepresentation<'a>, FontSelectionError> {
        let selected = self.representations.select(request)?;
        let mut representation = self.get(selected.index()).expect("selected ordinal");
        representation.used_fallback = selected.used_fallback();
        Ok(representation)
    }

    fn metrics_at(self, index: usize) -> Result<MetricsTable<'a>, MetricsError> {
        let offset = index * self.metric_table_len;
        MetricsTable::open(&self.metrics[offset..offset + self.metric_table_len])
    }
}

/// One resolved representation's size record, metrics and glyph storage metadata.
#[derive(Clone, Copy, Debug)]
pub struct FaceRepresentation<'a> {
    index: usize,
    used_fallback: bool,
    record: RepresentationRecord,
    surface: GlyphSurfaceRecord,
    metrics: MetricsTable<'a>,
    map: GlyphMap<'a>,
}

impl<'a> FaceRepresentation<'a> {
    pub const fn index(self) -> usize {
        self.index
    }
    /// True only when a size request selected an out-of-range nearest representation.
    pub const fn used_fallback(self) -> bool {
        self.used_fallback
    }
    pub const fn record(self) -> RepresentationRecord {
        self.record
    }
    pub const fn surface(self) -> GlyphSurfaceRecord {
        self.surface
    }
    pub const fn metrics(self) -> MetricsTable<'a> {
        self.metrics
    }
    pub const fn map(self) -> GlyphMap<'a> {
        self.map
    }
}

/// Inconsistent shared ordinals, metric tables or atlas map ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FaceTablesError {
    SizeOverflow,
    Cardinality {
        codepoints: usize,
        glyphs: usize,
    },
    MetricsLength {
        expected: usize,
        actual: usize,
    },
    MapsLength {
        table_len: usize,
        actual: usize,
    },
    Metrics {
        representation: usize,
        error: MetricsError,
    },
    Map {
        representation: usize,
        error: GlyphMapError,
    },
    ImplicitMapOffset {
        representation: usize,
        offset: u32,
    },
    MapOffset {
        representation: usize,
        offset: u32,
    },
    MapOutOfBounds {
        representation: usize,
    },
    UnreferencedMaps,
}

#[cfg(test)]
mod tests;
