use super::{Canvas, DepthQuery, Frame, RollMapping, SceneCanvas, Workspace};
use mirui::{
    render::{
        RenderError,
        texture::{ColorFormat, TexturePixelsMut},
    },
    types::{Color, Fixed},
};

/// Output tile edge in the bounded RGB565 layout.
const TILE_SIDE: usize = 32;
/// Source tile for a rotated output tile and its two-pixel sampling halo.
const SOURCE_SIDE: usize = 50;

/// Storage to borrow while drawing one frame. The scene supplies pixels in global source coordinates.
enum FrameLayout<'a> {
    /// Draw RGBA rows directly into the output using as many rows as fit in the workspace.
    Rows,
    /// Produce an RGBA image, using the supplied sheet when the camera is rolled.
    Mapped {
        mapping: &'a RollMapping,
        sheet: &'a mut Frame,
    },
    /// Produce RGB565 from bounded RGBA tiles. The source sheet is SOURCE_SIDE square. The RGBA band spans the output width and up to TILE_SIDE rows.
    Tiled {
        mapping: &'a RollMapping,
        sheet: &'a mut Frame,
        band: &'a mut Frame,
    },
}

/// Optional screen-space foreground. Fully covered tiles can omit their scene pass.
pub trait FrameOverlay {
    fn covers(&self, _rect: [i32; 4], _scratch: &mut [u8]) -> bool {
        false
    }
    fn paint(&self, canvas: &mut Canvas<'_>);
}

/// Reusable depth and color storage for a camera that can roll. RGBA8888 uses a full source sheet. RGB565 uses bounded source tiles and one output band.
pub struct CameraWorkspace {
    workspace: Workspace,
    sheet: Frame,
    band: Option<Frame>,
}

impl CameraWorkspace {
    pub fn new(output: &Frame, pivot: Fixed) -> Result<Self, RenderError> {
        if output.width == 0 || output.height == 0 {
            return Err(RenderError::InvalidGeometry);
        }
        let (side, band) = match output.texture.format() {
            ColorFormat::RGBA8888 => (super::sheet_side(output.width, output.height, pivot), None),
            ColorFormat::RGB565 => (
                SOURCE_SIDE,
                Some(Frame::new(output.width, output.height.min(TILE_SIDE))?),
            ),
            _ => return Err(RenderError::InvalidGeometry),
        };
        Ok(Self {
            workspace: Workspace::new(side, side)?,
            sheet: Frame::new(side, side)?,
            band,
        })
    }

    /// Number of bytes in the source color, depth, coverage and output-band buffers.
    pub fn bytes(&self) -> usize {
        self.workspace.bytes() + self.sheet.bytes() + self.band.as_ref().map_or(0, Frame::bytes)
    }

    /// Renders a camera and its optional screen-space foreground into an owned frame.
    pub fn render(
        &mut self,
        output: &mut Frame,
        mapping: &RollMapping,
        overlay: Option<&dyn FrameOverlay>,
        scene: impl FnMut(&mut SceneCanvas<'_, '_>) -> Result<(), RenderError>,
    ) -> Result<(), RenderError> {
        if mapping.output != [output.width, output.height] {
            return Err(RenderError::InvalidGeometry);
        }
        if mapping.rolled {
            let [width, height] = mapping.output.map(|v| v as i32);
            source_bounds(mapping, [0, 0, width, height])?;
        }
        let layout = match self.band.as_mut() {
            Some(band) => FrameLayout::Tiled {
                mapping,
                sheet: &mut self.sheet,
                band,
            },
            None if !mapping.rolled => FrameLayout::Rows,
            None => FrameLayout::Mapped {
                mapping,
                sheet: &mut self.sheet,
            },
        };
        self.workspace.render_frame(output, layout, overlay, scene)
    }

    /// Samples outstanding effect depths when an overlay or roll excluded them from the rendered tiles. Reuses the existing source tile for each group of nearby points.
    pub fn finish_depth_queries(
        &mut self,
        mapping: &RollMapping,
        queries: &mut [impl DepthQuery],
        mut depth: impl FnMut(&mut SceneCanvas<'_, '_>) -> Result<(), RenderError>,
    ) -> Result<(), RenderError> {
        while let Some((index, origin)) = queries
            .iter()
            .enumerate()
            .find_map(|(i, q)| Some((i, q.next_point()?)))
        {
            self.workspace.with_region(
                &mut self.sheet,
                mapping.size,
                origin,
                mapping.bounds,
                |tile| {
                    depth(tile)?;
                    for query in &mut *queries {
                        query.resolve(tile);
                    }
                    Ok::<(), RenderError>(())
                },
            )?;
            if queries[index].next_point() == Some(origin) {
                return Err(RenderError::InvalidGeometry);
            }
        }
        Ok(())
    }
}

impl Workspace {
    /// Produces the entire RGBA frame in bands using the available depth storage.
    pub fn render_rows(
        &mut self,
        output: &mut Frame,
        scene: impl FnMut(&mut SceneCanvas<'_, '_>) -> Result<(), RenderError>,
    ) -> Result<(), RenderError> {
        self.render_frame(output, FrameLayout::Rows, None, scene)
    }

    /// Renders the complete output with the existing depth and coverage storage. The callback receives one clipped region at a time. A failed scene pass aborts the frame before later regions run.
    fn render_frame(
        &mut self,
        output: &mut Frame,
        layout: FrameLayout<'_>,
        overlay: Option<&dyn FrameOverlay>,
        mut scene: impl FnMut(&mut SceneCanvas<'_, '_>) -> Result<(), RenderError>,
    ) -> Result<(), RenderError> {
        let size = [output.width, output.height];
        if size.contains(&0) {
            return Err(RenderError::InvalidGeometry);
        }
        match layout {
            FrameLayout::Rows => {
                if output.texture.format() != ColorFormat::RGBA8888 {
                    return Err(RenderError::InvalidGeometry);
                }
                let rows = self.depth.len() / output.width;
                if rows == 0 || self.columns.len() < output.width {
                    return Err(RenderError::MissingWorkspace);
                }
                for y in (0..output.height).step_by(rows) {
                    self.with_rows(
                        output,
                        y,
                        rows.min(output.height - y),
                        |tile| -> Result<(), RenderError> {
                            scene(tile)?;
                            if let Some(overlay) = overlay {
                                overlay.paint(tile.color);
                            }
                            Ok(())
                        },
                    )?;
                }
            }
            FrameLayout::Mapped { mapping, sheet } => {
                if output.texture.format() != ColorFormat::RGBA8888 {
                    return Err(RenderError::InvalidGeometry);
                }
                if sheet.texture.format() != ColorFormat::RGBA8888
                    || sheet.width < mapping.size[0]
                    || sheet.height < mapping.size[1]
                    || self.depth.len() < sheet.width * sheet.height
                    || self.columns.len() < sheet.width
                {
                    return Err(RenderError::MissingWorkspace);
                }
                self.with_region(sheet, mapping.size, [0, 0], mapping.bounds, scene)?;
                let pixels = sheet.texture.read_pixels()?;
                output.with_canvas(|canvas| {
                    sample_sheet(canvas, &pixels, [0, 0], sheet.width, mapping);
                    if let Some(overlay) = overlay {
                        overlay.paint(canvas);
                    }
                });
            }
            FrameLayout::Tiled {
                mapping,
                sheet,
                band,
            } => {
                if output.texture.format() != ColorFormat::RGB565 {
                    return Err(RenderError::InvalidGeometry);
                }
                if sheet.texture.format() != ColorFormat::RGBA8888
                    || band.texture.format() != ColorFormat::RGBA8888
                    || [sheet.width, sheet.height] != [SOURCE_SIDE; 2]
                    || band.width != size[0]
                    || band.height < TILE_SIDE.min(size[1])
                    || self.depth.len() < SOURCE_SIDE * SOURCE_SIDE
                    || self.columns.len() < SOURCE_SIDE
                {
                    return Err(RenderError::MissingWorkspace);
                }
                self.render_tiles(output, sheet, band, mapping, overlay, scene)?;
            }
        }
        Ok(())
    }

    fn render_tiles(
        &mut self,
        output: &mut Frame,
        sheet: &mut Frame,
        band: &mut Frame,
        mapping: &RollMapping,
        overlay: Option<&dyn FrameOverlay>,
        mut scene: impl FnMut(&mut SceneCanvas<'_, '_>) -> Result<(), RenderError>,
    ) -> Result<(), RenderError> {
        let size = [output.width, output.height];
        let [width, height] = size;
        let mut target = output.texture.pixels_mut()?;
        // Unrotated, unobstructed columns can span the entire height in the same tile memory.
        let pitch = (SOURCE_SIDE * SOURCE_SIDE / height).min(SOURCE_SIDE);
        let strips = !mapping.rolled && overlay.is_none() && pitch > 4;
        let (tile_width, tile_height) = if strips {
            (pitch - 4, height)
        } else {
            (TILE_SIDE, TILE_SIDE)
        };
        for y in (0..height).step_by(tile_height) {
            let bottom = (y + tile_height).min(height) as i32;
            for x in (0..width).step_by(tile_width) {
                let rect = [
                    x as i32,
                    y as i32,
                    (x + tile_width).min(width) as i32,
                    bottom,
                ];
                if strips {
                    self.with_extent(
                        sheet,
                        [pitch, height],
                        mapping.size,
                        [rect[0] - 2, 0],
                        mapping.bounds,
                        |tile| -> Result<(), RenderError> {
                            scene(tile)?;
                            write_rgb565(tile.color, &mut target, rect);
                            Ok(())
                        },
                    )?;
                    continue;
                }
                if overlay.is_some_and(|o| o.covers(rect, self.coverage_scratch())) {
                    band.with_region(size, [0, rect[1]], rect, |canvas| {
                        for row in rect[1]..rect[3] {
                            for col in rect[0]..rect[2] {
                                canvas.put(canvas.index(col, row), 0xff00_0000);
                            }
                        }
                    });
                    continue;
                }
                let [sx, sy, right, bottom] = source_bounds(mapping, rect)?;
                let origin = [sx - 2, sy - 2];
                let end = [right + 2, bottom + 2];
                if end[0] - origin[0] > SOURCE_SIDE as i32
                    || end[1] - origin[1] > SOURCE_SIDE as i32
                {
                    return Err(RenderError::MissingWorkspace);
                }
                self.with_region(sheet, mapping.size, origin, mapping.bounds, |tile| {
                    tile.color.clip[2] = tile.color.clip[2].min(end[0]);
                    tile.color.clip[3] = tile.color.clip[3].min(end[1]);
                    scene(tile)
                })?;
                let pixels = sheet.texture.read_pixels()?;
                band.with_region(size, [0, rect[1]], rect, |canvas| {
                    sample_sheet(canvas, &pixels, origin, SOURCE_SIDE, mapping);
                });
            }
            if !strips {
                let rect = [0, y as i32, width as i32, bottom];
                band.with_region(size, [0, y as i32], rect, |canvas| {
                    if let Some(overlay) = overlay {
                        overlay.paint(canvas);
                    }
                    write_rgb565(canvas, &mut target, rect);
                });
            }
        }
        Ok(())
    }
}

fn source_bounds(
    mapping: &RollMapping,
    [left, top, right, bottom]: [i32; 4],
) -> Result<[i32; 4], RenderError> {
    let corners = [
        [left, top],
        [right - 1, top],
        [left, bottom - 1],
        [right - 1, bottom - 1],
    ]
    .map(|[x, y]| mapping.source_at(x, y));
    let [x0, y0] = core::array::from_fn(|axis| corners.iter().map(|p| p[axis]).min().unwrap());
    let [x1, y1] = core::array::from_fn(|axis| corners.iter().map(|p| p[axis]).max().unwrap() + 1);
    if x0 < mapping.bounds[0].max(0)
        || y0 < mapping.bounds[1].max(0)
        || x1 > mapping.bounds[2].min(mapping.size[0] as i32)
        || y1 > mapping.bounds[3].min(mapping.size[1] as i32)
    {
        return Err(RenderError::InvalidGeometry);
    }
    Ok([x0, y0, x1, y1])
}

fn sample_sheet(
    canvas: &mut Canvas<'_>,
    pixels: &mirui::render::texture::TexturePixels<'_>,
    origin: [i32; 2],
    pitch: usize,
    mapping: &RollMapping,
) {
    let [left, top, right, bottom] = canvas.clip;
    let pixels = pixels.bytes();
    for y in top..bottom {
        for x in left..right {
            let [sx, sy] = mapping.source_at(x, y);
            let at = ((sy - origin[1]) as usize * pitch + (sx - origin[0]) as usize) * 4;
            let dst = canvas.index(x, y) * 4;
            canvas.bytes[dst..dst + 4].copy_from_slice(&pixels[at..at + 4]);
        }
    }
}

fn write_rgb565(canvas: &Canvas<'_>, target: &mut TexturePixelsMut<'_>, rect: [i32; 4]) {
    let [left, top, right, bottom] = rect;
    let width = (right - left) as usize;
    let stride = target.stride();
    let bytes = target.bytes_mut();
    for y in top..bottom {
        let src = canvas.index(left, y) * 4;
        let dst = y as usize * stride + left as usize * 2;
        let row = &canvas.bytes[src..src + width * 4];
        let output = &mut bytes[dst..dst + width * 2];
        for (rgba, out) in row.chunks_exact(4).zip(output.chunks_exact_mut(2)) {
            let color = Color::rgb(rgba[0], rgba[1], rgba[2]);
            out.copy_from_slice(&(ColorFormat::RGB565.pack(&color) as u16).to_le_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_viewports_reserve_the_full_rotated_diagonal() {
        assert_eq!(
            super::super::sheet_side(6000, 8000, Fixed::from_int(4000)),
            10004
        );
        assert_eq!(super::super::sheet_side(60000, 65000, Fixed::ZERO), 143184);
    }

    #[test]
    fn roll_bounds_cover_every_output_corner_in_all_quadrants() {
        for (width, height) in [
            (8, 8),
            (32, 32),
            (128, 128),
            (128, 64),
            (64, 128),
            (127, 113),
        ] {
            for degrees in (-360..360).step_by(5) {
                let angle = Fixed::from_int(degrees) * Fixed::PI / 180;
                let pivot = (Fixed::from_int(height as i32) * Fixed::from_ratio(34, 100)).round();
                let mapping = RollMapping::new(
                    [width, height],
                    pivot,
                    Some((Fixed::sin_rad(angle), Fixed::cos_rad(angle))),
                );
                for y in (0..height).step_by(TILE_SIDE) {
                    for x in (0..width).step_by(TILE_SIDE) {
                        let right = (x + TILE_SIDE).min(width) - 1;
                        let bottom = (y + TILE_SIDE).min(height) - 1;
                        let corners = [[x, y], [right, y], [x, bottom], [right, bottom]]
                            .map(|[x, y]| mapping.source_at(x as i32, y as i32));
                        let low = [
                            corners.iter().map(|p| p[0]).min().unwrap() - 2,
                            corners.iter().map(|p| p[1]).min().unwrap() - 2,
                        ];
                        for point in corners {
                            assert!(
                                point[0] >= mapping.bounds[0]
                                    && point[1] >= mapping.bounds[1]
                                    && point[0] < mapping.bounds[2]
                                    && point[1] < mapping.bounds[3],
                                "{width}x{height} at {degrees}: {point:?} outside {:?}",
                                mapping.bounds
                            );
                            assert!(
                                point[0] > low[0]
                                    && point[1] > low[1]
                                    && point[0] < low[0] + SOURCE_SIDE as i32 - 1
                                    && point[1] < low[1] + SOURCE_SIDE as i32 - 1
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn rows_stop_at_the_first_failed_scene_tile() {
        let mut output = Frame::new(9, 11).unwrap();
        let mut workspace = Workspace::new(9, 4).unwrap();
        let mut calls = 0;
        let result = workspace.render_rows(&mut output, |tile| {
            calls += 1;
            if calls == 2 {
                return Err(RenderError::InvalidGeometry);
            }
            tile.color.bytes.fill(42);
            Ok(())
        });
        assert!(matches!(result, Err(RenderError::InvalidGeometry)));
        assert_eq!(calls, 2);
        let pixels = output.texture.read_pixels().unwrap();
        assert!(pixels.bytes()[..4 * 9 * 4].iter().all(|v| *v == 42));
        assert!(pixels.bytes()[4 * 9 * 4..].iter().all(|v| *v == 0));
    }

    #[test]
    fn opaque_overlay_skips_scene_tiles_and_effect_queries_share_a_depth_tile() {
        struct Solid;
        impl FrameOverlay for Solid {
            fn covers(&self, _: [i32; 4], _: &mut [u8]) -> bool {
                true
            }
            fn paint(&self, canvas: &mut Canvas<'_>) {
                let [left, top, right, bottom] = canvas.clip;
                for y in top..bottom {
                    for x in left..right {
                        canvas.point(x, y, 0xff00_00ff);
                    }
                }
            }
        }
        struct Sample(Option<[i32; 2]>);
        impl DepthQuery for Sample {
            fn next_point(&self) -> Option<[i32; 2]> {
                self.0
            }
            fn resolve(&mut self, tile: &SceneCanvas<'_, '_>) {
                if let Some([x, y]) = self.0
                    && tile.sample_depth(x, y).is_some()
                {
                    self.0 = None;
                }
            }
        }
        let mut output = Frame::with_format(65, 42, ColorFormat::RGB565).unwrap();
        let mapping = RollMapping::new([65, 42], Fixed::ZERO, None);
        let mut workspace = CameraWorkspace::new(&output, Fixed::ZERO).unwrap();
        workspace
            .render(&mut output, &mapping, Some(&Solid), |_| {
                panic!("covered scene drawn")
            })
            .unwrap();
        assert!(
            output
                .texture
                .read_pixels()
                .unwrap()
                .bytes()
                .chunks_exact(2)
                .all(|p| p == [0, 0xf8])
        );
        let mut queries = [Sample(Some([4, 5])), Sample(Some([40, 39]))];
        let mut fills = 0;
        workspace
            .finish_depth_queries(&mapping, &mut queries, |tile| {
                tile.clear_depth();
                fills += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(fills, 1);
        assert!(queries.iter().all(|sample| sample.next_point().is_none()));
    }
}
