use alloc::{boxed::Box, rc::Rc};
use core::cell::RefCell;
use core::task::{Poll, ready};

use super::super::decoded::{DecodePool, EncodedImage, EncodedPreparation};
use super::{
    AssetError, ContainerLayout, LoadContext, LoadError, PlaneLayout, PlaneRequirements,
    SourceImage, map_mirx_format,
};
use crate::render::budget::Allocation;
use mirx::image::{EncodedImageScan, RawImageScan};
use mirx::reader::{
    IntegrityScan, MediaIntegrityScan, MediaLayout, MetadataScan, RangeCheck, WholeDataScan,
};

enum Stage {
    Directory,
    Header,
    Metadata(MetadataScan),
    Typed(RawImageScan),
    Encoded(EncodedImageScan),
    EncodedRead(EncodedPreparation),
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
        pool: &Rc<RefCell<DecodePool>>,
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
                            let format = map_mirx_format(
                                surface
                                    .sample_layout()
                                    .color_format()
                                    .ok_or(AssetError::Unsupported)?,
                            )?;
                            let memory = layout.plane(0).ok_or(AssetError::InvalidSource)?;
                            if memory.flags().bits() != 0 {
                                return Poll::Ready(Err(AssetError::Unsupported.into()));
                            }
                            let layout = PlaneLayout::new(
                                surface.width().try_into().map_err(invalid)?,
                                surface.height().try_into().map_err(invalid)?,
                                format,
                                memory.stride() as usize,
                                PlaneRequirements::CPU,
                            )
                            .map_err(invalid)?;
                            let start = self.offset
                                + scan.data_range().unwrap().start
                                + memory.data_offset() as usize;
                            self.selected = Some(SourceImage {
                                layout,
                                range: start..start + memory.byte_len() as usize,
                                logical_len: self.container.logical_len(),
                                encoded: None,
                            });
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
                        self.encoded = Some(EncodedImage::new(scan, self.offset, pool)?);
                        self.stage = Stage::EncodedRead(EncodedPreparation::default());
                    }
                }
                Stage::EncodedRead(progress) => {
                    ready!(
                        self.encoded
                            .as_mut()
                            .expect("encoded metadata")
                            .poll_prepare(progress, pool, cx)
                    )?;
                    let image = self.encoded.take().unwrap();
                    if self.primary {
                        let surface = image.surface();
                        if surface.plane_count() != 1 {
                            return Poll::Ready(Err(AssetError::Unsupported.into()));
                        }
                        let format = map_mirx_format(
                            surface
                                .sample_layout()
                                .color_format()
                                .ok_or(AssetError::Unsupported)?,
                        )?;
                        let layout = PlaneLayout::packed(
                            surface.width().try_into().map_err(invalid)?,
                            surface.height().try_into().map_err(invalid)?,
                            format,
                        )
                        .map_err(invalid)?;
                        self.selected = Some(SourceImage {
                            layout,
                            range: image.data_range(),
                            logical_len: self.container.logical_len(),
                            encoded: Some(image),
                        });
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
