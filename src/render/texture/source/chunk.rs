use alloc::boxed::Box;
use core::task::{Poll, ready};

use super::super::decoded::{EncodedImage, EncodedPreparation, Pool};
use super::{AssetError, ContainerLayout, LoadContext, LoadError, SourceImage};
use crate::render::budget::Allocation;
use mirx::image::{EncodedImageScan, RawImageScan};
use mirx::reader::{
    IntegrityScan, MediaIntegrityScan, MediaLayout, MediaSectionKind, MetadataScan, RangeCheck,
    WholeDataScan,
};

enum Stage {
    Directory,
    Header,
    Metadata(MetadataScan),
    Typed(RawImageScan),
    Encoded(EncodedImageScan),
    EncodedRead {
        preparation: EncodedPreparation,
        palette: Option<core::ops::Range<usize>>,
    },
    Coverage(MediaIntegrityScan),
    Whole(WholeDataScan),
    Indexed {
        records: IntegrityScan,
        base: usize,
        check: Option<RangeCheck>,
    },
}

pub(super) struct ChunkRead {
    container: ContainerLayout,
    index: usize,
    offset: usize,
    length: usize,
    primary: bool,
    selected: Option<SourceImage>,
    media: Option<MediaLayout>,
    window: [u8; 256],
    stage: Stage,
    encoded: Option<Box<EncodedImage>>,
    _allocation: Allocation,
}

fn invalid(_: impl core::fmt::Debug) -> LoadError {
    LoadError::Failed("invalid MIRX image payload")
}

impl ChunkRead {
    pub(super) fn new(
        container: ContainerLayout,
        allocation: Allocation,
    ) -> Result<Self, LoadError> {
        if container.primary_type() != Some(mirx::ChunkType::IMAGE) {
            return Err(AssetError::Unsupported.into());
        }
        Ok(Self {
            container,
            index: 0,
            offset: 0,
            length: 0,
            primary: false,
            selected: None,
            media: None,
            window: [0; 256],
            stage: Stage::Directory,
            encoded: None,
            _allocation: allocation,
        })
    }

    pub(super) fn poll(
        &mut self,
        cx: &mut LoadContext<'_>,
        pool: &Pool,
    ) -> Poll<Result<SourceImage, LoadError>> {
        loop {
            match &mut self.stage {
                Stage::Directory => {
                    let Some(range) = self.container.chunk_record_range(self.index) else {
                        return Poll::Ready(
                            self.selected.take().ok_or(AssetError::InvalidSource.into()),
                        );
                    };
                    ready!(
                        cx.poll_read_exact(range.start as u64, &mut self.window[..range.len()])
                    )?;
                    let chunk = self
                        .container
                        .chunk(self.index, &self.window[..range.len()])
                        .map_err(invalid)?
                        .expect("requested chunk");
                    self.index += 1;
                    self.primary =
                        self.selected.is_none() && chunk.chunk_type() == mirx::ChunkType::IMAGE;
                    if !self.primary && !chunk.flags().is_critical() {
                        continue;
                    }
                    if chunk.chunk_type() != mirx::ChunkType::IMAGE {
                        return Poll::Ready(Err(AssetError::Unsupported.into()));
                    }
                    let range = chunk.payload_range();
                    self.offset = range.start;
                    self.length = range.len();
                    self.stage = Stage::Header;
                }
                Stage::Header => {
                    if self.length < 8 {
                        return Poll::Ready(Err(AssetError::Truncated.into()));
                    }
                    ready!(cx.poll_read_exact(self.offset as u64, &mut self.window[..8]))?;
                    let layout =
                        MediaLayout::parse(&self.window[..8], self.length).map_err(invalid)?;
                    self.media = Some(layout);
                    self.stage = Stage::Metadata(layout.metadata());
                }
                Stage::Metadata(scan) => {
                    if let Some(range) = scan.read_range() {
                        let n = ready!(read_window(cx, self.offset, range, &mut self.window))?;
                        scan.accept(&self.window[..n]).map_err(invalid)?;
                    } else {
                        let Stage::Metadata(scan) =
                            core::mem::replace(&mut self.stage, Stage::Directory)
                        else {
                            unreachable!()
                        };
                        scan.finish().map_err(invalid)?;
                        self.stage = Stage::Typed(RawImageScan::new(
                            self.media.unwrap(),
                            self.offset as u32,
                        ));
                    }
                }
                Stage::Typed(scan) => {
                    if let Some(range) = scan.read_range() {
                        let n = ready!(read_window(cx, self.offset, range, &mut self.window))?;
                        match scan.accept(&self.window[..n]) {
                            Err(mirx::image::RawImageViewError::UnexpectedSection(_)) => {
                                self.stage = Stage::Encoded(EncodedImageScan::new(
                                    self.media.unwrap(),
                                    self.offset as u32,
                                ));
                            }
                            result => result.map_err(invalid)?,
                        }
                    } else {
                        let layout = scan.finish().map_err(invalid)?;
                        if self.primary {
                            let surface = layout.surface();
                            if surface.plane_count() != 1 {
                                return Poll::Ready(Err(AssetError::Unsupported.into()));
                            }
                            let format = surface
                                .sample_layout()
                                .color_format()
                                .ok_or(AssetError::Unsupported)?;
                            let memory = layout.plane(0).ok_or(AssetError::InvalidSource)?;
                            if memory.flags().bits() != 0 {
                                return Poll::Ready(Err(AssetError::Unsupported.into()));
                            }
                            let start = self.offset
                                + scan.data_range().unwrap().start
                                + memory.data_offset() as usize;
                            let palette = scan
                                .color_table_range()
                                .map(|p| self.offset + p.start..self.offset + p.end);
                            self.selected = Some(SourceImage::new(
                                [surface.width(), surface.height()],
                                format,
                                memory.stride() as usize,
                                start..start + memory.byte_len() as usize,
                                palette,
                                self.container.logical_len(),
                            )?);
                        }
                        self.stage =
                            Stage::Coverage(self.media.unwrap().integrity().map_err(invalid)?);
                    }
                }
                Stage::Encoded(scan) => {
                    if let Some(range) = scan.read_range() {
                        let n = ready!(read_window(cx, self.offset, range, &mut self.window))?;
                        scan.accept(&self.window[..n]).map_err(invalid)?;
                    } else {
                        let mut storage = ready!(
                            pool.poll_take(&mut core::task::Context::from_waker(cx.waker()))
                        )
                        .map_err(|_| AssetError::Capacity)?;
                        self.encoded = Some(EncodedImage::new(scan, self.offset, &mut storage)?);
                        let palette = scan.section(MediaSectionKind::COLOR_TABLE).map(|section| {
                            let start = self.offset + section.offset() as usize;
                            start..start + section.size() as usize
                        });
                        self.stage = Stage::EncodedRead {
                            preparation: EncodedPreparation::default(),
                            palette,
                        };
                    }
                }
                Stage::EncodedRead {
                    preparation,
                    palette,
                } => {
                    ready!(
                        self.encoded
                            .as_mut()
                            .expect("encoded metadata")
                            .poll_prepare(preparation, pool, cx)
                    )?;
                    let image = self.encoded.take().unwrap();
                    if self.primary {
                        let surface = image.surface();
                        if surface.plane_count() != 1 {
                            return Poll::Ready(Err(AssetError::Unsupported.into()));
                        }
                        let format = surface
                            .sample_layout()
                            .color_format()
                            .ok_or(AssetError::Unsupported)?;
                        let stride = format
                            .minimum_stride(surface.width())
                            .ok_or(AssetError::OutOfBounds)?;
                        let mut selected = SourceImage::new(
                            [surface.width(), surface.height()],
                            format,
                            stride as usize,
                            image.data_range(),
                            palette.take(),
                            self.container.logical_len(),
                        )?;
                        selected.encoded = Some(image);
                        self.selected = Some(selected);
                    }
                    self.stage = Stage::Coverage(self.media.unwrap().integrity().map_err(invalid)?);
                }
                Stage::Coverage(scan) => {
                    if let Some(range) = scan.read_range() {
                        let n = ready!(read_window(cx, self.offset, range, &mut self.window))?;
                        scan.accept(&self.window[..n]).map_err(invalid)?;
                    } else {
                        self.stage = match (*scan).finish().map_err(invalid)? {
                            Some(table) => Stage::Indexed {
                                records: IntegrityScan::new(table.size() as usize)
                                    .map_err(invalid)?,
                                base: table.offset() as usize,
                                check: None,
                            },
                            None => Stage::Whole(
                                self.media
                                    .unwrap()
                                    .whole_data()
                                    .expect("whole DATA integrity"),
                            ),
                        };
                    }
                }
                Stage::Whole(scan) => {
                    if let Some(range) = scan.read_range() {
                        let n = ready!(read_window(cx, self.offset, range, &mut self.window))?;
                        scan.accept(&self.window[..n]).map_err(invalid)?;
                    } else {
                        let Stage::Whole(scan) =
                            core::mem::replace(&mut self.stage, Stage::Directory)
                        else {
                            unreachable!()
                        };
                        scan.finish().map_err(invalid)?;
                        self.stage = Stage::Directory;
                    }
                }
                Stage::Indexed {
                    records,
                    base,
                    check,
                } => {
                    if let Some(current) = check {
                        if let Some(range) = current.read_range() {
                            let n = ready!(read_window(
                                cx,
                                self.offset,
                                range.start as usize..range.end as usize,
                                &mut self.window
                            ))?;
                            current.accept(&self.window[..n]).map_err(invalid)?;
                        } else {
                            check.take().unwrap().finish().map_err(invalid)?;
                        }
                    } else if let Some(range) = records.record_range() {
                        ready!(cx.poll_read_exact(
                            (self.offset + *base + range.start) as u64,
                            &mut self.window[..range.len()]
                        ))?;
                        *check = Some(
                            records
                                .accept(&self.window[..range.len()])
                                .map_err(invalid)?
                                .expect("requested integrity record")
                                .check(),
                        );
                    } else {
                        self.stage = Stage::Directory;
                    }
                }
            }
        }
    }
}

fn read_window(
    cx: &mut LoadContext<'_>,
    base: usize,
    range: core::ops::Range<usize>,
    window: &mut [u8],
) -> Poll<Result<usize, LoadError>> {
    let count = range.len().min(window.len());
    let n = ready!(cx.poll_read((base + range.start) as u64, &mut window[..count]))?;
    if n == 0 {
        Poll::Ready(Err(AssetError::Truncated.into()))
    } else {
        Poll::Ready(Ok(n))
    }
}
