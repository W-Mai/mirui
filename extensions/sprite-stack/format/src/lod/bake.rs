//! Offline generation of sparse detail levels from the original atlas.
use super::{HEADER, LodColor, LodError, RECORD, validate_ramps};
use crate::StackDescriptor;
use alloc::vec::Vec;

#[derive(Clone, Copy)]
pub enum Samples<'a> {
    Indexed(&'a [u8]),
    Rgba,
}

impl Samples<'_> {
    fn opaque(self, value: LodColor) -> Result<bool, LodError> {
        let alpha = match (self, value) {
            (Self::Indexed(palette), LodColor::Indexed(index)) => *palette
                .get(usize::from(index) * 4 + 3)
                .ok_or(LodError::InvalidPalette)?,
            (Self::Rgba, LodColor::Rgba(rgba)) => rgba[3],
            _ => return Err(LodError::InvalidSample),
        };
        match alpha {
            0 => Ok(false),
            255 => Ok(true),
            _ => Err(LodError::InvalidSample),
        }
    }
}

struct Votes<'a> {
    format: Samples<'a>,
    groups: [u16; 256],
    grouped: bool,
    values: Vec<(u32, u32)>,
}
impl<'a> Votes<'a> {
    fn new(format: Samples<'a>, ramps: &[[u8; 2]]) -> Result<Self, LodError> {
        validate_ramps(ramps.iter().copied())?;
        if let Samples::Indexed(bytes) = format {
            if bytes.is_empty() || bytes.len() > 1024 || !bytes.len().is_multiple_of(4) {
                return Err(LodError::InvalidPalette);
            }
        } else if !ramps.is_empty() {
            return Err(LodError::InvalidRecipe);
        }
        let mut groups = [u16::MAX; 256];
        for (group, &[lo, hi]) in ramps.iter().enumerate() {
            for index in lo..=hi {
                groups[usize::from(index)] = group as u16
            }
        }
        Ok(Self {
            format,
            groups,
            grouped: !ramps.is_empty(),
            values: Vec::new(),
        })
    }
    fn record(&mut self, color: LodColor) -> Result<(), LodError> {
        if !self.format.opaque(color)? {
            return Ok(());
        }
        let sample = match color {
            LodColor::Indexed(i) => u32::from(i),
            LodColor::Rgba(rgba) => u32::from_le_bytes(rgba),
        };
        let vote = if self.grouped {
            let group = self.groups[sample as usize];
            if group == u16::MAX {
                return Err(LodError::InvalidRecipe);
            }
            (u32::from(group), sample)
        } else {
            (sample, self.values.len() as u32)
        };
        self.values.push(vote);
        Ok(())
    }
    fn finish(&mut self) -> Result<Option<LodColor>, LodError> {
        let mut best = None;
        self.values.sort_unstable();
        for group in self.values.chunk_by(|a, b| a.0 == b.0) {
            let (key, first) = group[0];
            let (priority, value) = if self.grouped {
                let sum = group.iter().map(|i| u64::from(i.1)).sum::<u64>();
                let n = group.len() as u64;
                (key, ((sum + n / 2) / n) as u32)
            } else {
                (u32::MAX - first, key)
            };
            let rank = (group.len(), priority, value);
            if best.is_none_or(|old| rank > old) {
                best = Some(rank)
            }
        }
        self.values.clear();
        let Some((_, _, value)) = best else {
            return Ok(None);
        };
        let color = match self.format {
            Samples::Indexed(_) => LodColor::Indexed(value as u8),
            Samples::Rgba => LodColor::Rgba(value.to_le_bytes()),
        };
        if !self.format.opaque(color)? {
            return Err(LodError::InvalidPalette);
        }
        Ok(Some(color))
    }
}
/// Bakes additional levels directly from base samples. The callback receives primary atlas coordinates.
/// Steps must start at one and increase strictly. An empty ramp list selects dominant-color reduction.
/// The byte limit bounds the encoded result and the size of a single block's voting workspace independently.
pub fn encode(
    descriptor: StackDescriptor<'_>,
    format: Samples<'_>,
    steps: &[u16],
    ramps: &[[u8; 2]],
    limit: usize,
    mut sample: impl FnMut(u32, u32) -> Result<LodColor, LodError>,
) -> Result<Option<Vec<u8>>, LodError> {
    if steps.first() != Some(&1) || steps.windows(2).any(|s| s[1] <= s[0]) {
        return Err(LodError::InvalidRecipe);
    }
    let mut votes = Votes::new(format, ramps)?;
    if steps.len() == 1 {
        return Ok(None);
    }
    let count = u16::try_from(steps.len() - 1).map_err(|_| LodError::InvalidRecipe)?;
    let layers = u16::try_from(descriptor.layers().len()).map_err(|_| LodError::InvalidRecipe)?;
    let table_start = HEADER + ramps.len() * 2;
    let initial = table_start + usize::from(count) * RECORD;
    let limit = limit.min(u32::MAX as usize);
    if initial > limit {
        return Err(LodError::Limit);
    }
    let mut p = Vec::new();
    p.try_reserve_exact(initial).map_err(|_| LodError::Limit)?;
    p.resize(initial, 0);
    let (w, h) = descriptor.canvas();
    p[..4].copy_from_slice(b"SLOD");
    p[4..6].copy_from_slice(&1u16.to_le_bytes());
    p[6] = match format {
        Samples::Indexed(_) => 1,
        Samples::Rgba => 4,
    };
    p[7] = u8::from(!ramps.is_empty());
    for (i, v) in [count, layers, ramps.len() as u16, w, h, 0]
        .into_iter()
        .enumerate()
    {
        p[8 + i * 2..10 + i * 2].copy_from_slice(&v.to_le_bytes())
    }
    for (b, r) in p[HEADER..table_start].chunks_exact_mut(2).zip(ramps) {
        b.copy_from_slice(r)
    }
    let stride = usize::from(p[6]) + 4;
    for (level, &step) in steps[1..].iter().enumerate() {
        let start = p.len();
        let traces = (usize::from(layers) + 1) * 4;
        let data = start
            .checked_add(traces)
            .filter(|&n| n <= limit)
            .ok_or(LodError::Limit)?;
        p.try_reserve(traces).map_err(|_| LodError::Limit)?;
        p.resize(data, 0);
        let mut count = 0u32;
        for (layer, region) in descriptor.layers().into_iter().enumerate() {
            if region.width() != 0 {
                for y in (0..u32::from(h)).step_by(usize::from(step)) {
                    for x in (0..u32::from(w)).step_by(usize::from(step)) {
                        let endx = (x + u32::from(step)).min(u32::from(w));
                        let endy = (y + u32::from(step)).min(u32::from(h));
                        let n = ((endx - x) as usize)
                            .checked_mul((endy - y) as usize)
                            .ok_or(LodError::Limit)?;
                        if n > limit / core::mem::size_of::<(u32, u32)>() {
                            return Err(LodError::Limit);
                        }
                        votes
                            .values
                            .try_reserve_exact(n)
                            .map_err(|_| LodError::Limit)?;
                        for row in y..endy {
                            for col in x..endx {
                                votes.record(sample(region.x() + col, region.y() + row)?)?
                            }
                        }
                        if let Some(color) = votes.finish()? {
                            if limit - p.len() < stride {
                                return Err(LodError::Limit);
                            }
                            p.try_reserve(stride).map_err(|_| LodError::Limit)?;
                            p.extend_from_slice(&(x as u16).to_le_bytes());
                            p.extend_from_slice(&(y as u16).to_le_bytes());
                            match color {
                                LodColor::Indexed(i) => p.push(i),
                                LodColor::Rgba(c) => p.extend_from_slice(&c),
                            }
                            count += 1;
                        }
                    }
                }
            }
            let at = start + (layer + 1) * 4;
            p[at..at + 4].copy_from_slice(&count.to_le_bytes());
        }
        let size = (p.len() - start) as u32;
        let r = &mut p[table_start + level * RECORD..][..RECORD];
        r[..2].copy_from_slice(&step.to_le_bytes());
        r[4..8].copy_from_slice(&(start as u32).to_le_bytes());
        r[8..12].copy_from_slice(&size.to_le_bytes());
    }
    Ok(Some(p))
}
