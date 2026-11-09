//! Capture the responsive showcase through the existing scene renderer.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::PathBuf;

use mirui::app::App;
use mirui::ecs::DeltaTimeMs;
use mirui::gallery::demos::orbit_console::{self, DemoRunMode};
use mirui::prelude::*;
use mirui::render::PreparedDraw;
use mirui::render::backends::scene::SceneRenderer;
use mirui::render::font::{Font, GlyphId, GlyphKind};
use mirui::render::path::{Path, PathCmd};
use mirui::render::raster::FillRule;
use mirui::render::renderer::{DrawRequest, RenderError, RenderRoute, Renderer};
use mirui::render::scene::record::ResourceResolver;
use mirui::render::scene::{GradientUnits, Paint, ResourceRef, Scene, SceneOp, SpreadMode};
use mirui::render::texture::Texture;
use mirui::surface::Surface;
use mirui::types::Transform;
use serde_json::{Value, json};

#[derive(Default)]
struct Export {
    definitions: Vec<Value>,
    keys: BTreeMap<String, usize>,
    fonts: Vec<Font>,
}

impl ResourceResolver for Export {
    fn resolve_font(&mut self, font: &Font) -> ResourceRef {
        let index = self
            .fonts
            .iter()
            .position(|f| {
                f.face_id() == font.face_id()
                    && f.revision() == font.revision()
                    && f.size == font.size
            })
            .unwrap_or_else(|| {
                self.fonts.push(font.clone());
                self.fonts.len() - 1
            });
        ResourceRef::Index(index as u32)
    }

    fn resolve_texture(&mut self, _: &Texture<'_>) -> ResourceRef {
        panic!("The vector showcase must not contain rasterized widgets");
    }
}

struct Recorder<'a> {
    inner: SceneRenderer<'a>,
    clips: Vec<Rect>,
}

impl Renderer for Recorder<'_> {
    type TargetState = ();

    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        self.inner.route(request)
    }

    fn submit(&mut self, draw: &PreparedDraw<'_>) -> Result<(), RenderError> {
        self.inner.submit(draw)?;
        self.clips.push(draw.request().clip);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), RenderError> {
        self.inner.flush()
    }
}

fn rect(area: Rect) -> Value {
    json!([
        area.x.to_f32(),
        area.y.to_f32(),
        area.w.to_f32(),
        area.h.to_f32()
    ])
}

fn matrix(t: Transform) -> Value {
    json!([
        t.m00.to_f32(),
        t.m10.to_f32(),
        t.m01.to_f32(),
        t.m11.to_f32(),
        t.tx.to_f32(),
        t.ty.to_f32()
    ])
}

fn point(p: Point) -> String {
    format!("{} {}", p.x.to_f32(), p.y.to_f32())
}

fn path_data(path: &Path) -> String {
    let mut data = String::new();
    for command in path.commands() {
        match command {
            PathCmd::MoveTo(p) => write!(data, "M{}", point(*p)),
            PathCmd::LineTo(p) => write!(data, "L{}", point(*p)),
            PathCmd::QuadTo { ctrl, end } => write!(data, "Q{} {}", point(*ctrl), point(*end)),
            PathCmd::CubicTo { ctrl1, ctrl2, end } => {
                write!(data, "C{} {} {}", point(*ctrl1), point(*ctrl2), point(*end))
            }
            PathCmd::Close => write!(data, "Z"),
        }
        .expect("write path");
    }
    data
}

fn hex(r: u8, g: u8, b: u8) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

impl Export {
    fn define(&mut self, value: Value) -> usize {
        let key = value.to_string();
        if let Some(index) = self.keys.get(&key) {
            return *index;
        }
        let index = self.definitions.len();
        self.keys.insert(key, index);
        self.definitions.push(value);
        index
    }

    fn paint(&mut self, paint: &Paint) -> (String, f32) {
        let definition = match paint {
            Paint::Color(c) => return (hex(c.r, c.g, c.b), f32::from(c.a) / 255.0),
            Paint::LinearGradient(g) => {
                json!({"kind":"linearGradient", "x1":g.start.x.to_f32(), "y1":g.start.y.to_f32(), "x2":g.end.x.to_f32(), "y2":g.end.y.to_f32(), "stops":g.stops.iter().map(|s| json!([s.offset.to_f32(), hex(s.color.r,s.color.g,s.color.b), f32::from(s.color.a)/255.0])).collect::<Vec<_>>(), "units":units(g.units), "spread":spread(g.spread), "transform":[g.transform.m00.to_f32(),g.transform.m10.to_f32(),g.transform.m01.to_f32(),g.transform.m11.to_f32(),g.transform.tx.to_f32(),g.transform.ty.to_f32()]})
            }
            Paint::RadialGradient(g) => {
                json!({"kind":"radialGradient", "cx":g.center.x.to_f32(), "cy":g.center.y.to_f32(), "r":g.radius.to_f32(), "fx":g.focal.x.to_f32(), "fy":g.focal.y.to_f32(), "fr":g.focal_radius.to_f32(), "stops":g.stops.iter().map(|s| json!([s.offset.to_f32(), hex(s.color.r,s.color.g,s.color.b), f32::from(s.color.a)/255.0])).collect::<Vec<_>>(), "units":units(g.units), "spread":spread(g.spread), "transform":[g.transform.m00.to_f32(),g.transform.m10.to_f32(),g.transform.m01.to_f32(),g.transform.m11.to_f32(),g.transform.tx.to_f32(),g.transform.ty.to_f32()]})
            }
        };
        let id = self.define(definition);
        (format!("url(#d{id})"), 1.0)
    }

    fn glyph(&mut self, font: &Font, id: GlyphId, size: u16) -> Value {
        // Preserve shaped positions, using the same face's distance contour
        // when available instead of enlarging small hinted coverage cells.
        let high = font.glyph_by_id(id, 64);
        let (glyph, bearing_scale) = match high {
            Some(glyph) if matches!(&glyph.kind, GlyphKind::Raster { representation, .. } if matches!(representation.kind(), mirx::font::FontRepresentationKind::SignedDistance { .. })) => {
                (glyph, f32::from(size) / 64.0)
            }
            _ => (font.glyph_by_id(id, size).expect("captured glyph"), 1.0),
        };
        let GlyphKind::Raster {
            samples,
            stride,
            region,
            representation,
            bearing_x,
            bearing_y,
        } = glyph.kind
        else {
            panic!("unsupported glyph representation");
        };
        let (bits, distance) = match representation.kind() {
            mirx::font::FontRepresentationKind::Coverage { bits } => (bits, false),
            mirx::font::FontRepresentationKind::SignedDistance { bits, .. } => (bits, true),
            other => panic!("unsupported scalar semantics: {other:?}"),
        };
        assert!([1, 2, 4, 8].contains(&bits));
        let mut pixels = Vec::new();
        let max = (1_u16 << bits) - 1;
        for y in 0..region.height() {
            for x in 0..region.width() {
                let bit = (region.x() + x) as usize * usize::from(bits);
                let value = (samples[(region.y() + y) as usize * stride as usize + bit / 8]
                    >> (8 - usize::from(bits) - bit % 8))
                    & max as u8;
                pixels.push((u16::from(value) * 255 / max) as u8);
            }
        }
        let index = self.define(json!({"kind":"glyph", "width":region.width(), "height":region.height(), "distance":distance, "samples":pixels}));
        json!({"ref":index, "scale":f32::from(size)/f32::from(representation.design_ppem()), "x":bearing_x.to_f32()*bearing_scale, "y":-bearing_y.to_f32()*bearing_scale})
    }

    fn node(&mut self, op: &SceneOp) -> Value {
        let (mut attrs, transform, fill, opacity) = match op {
            SceneOp::FillRect {
                area,
                transform,
                color,
                radius,
                opa,
                quad,
            } => {
                assert!(quad.is_none());
                (
                    json!({"tag":"rect", "x":area.x.to_f32(), "y":area.y.to_f32(), "width":area.w.to_f32(), "height":area.h.to_f32(), "rx":radius.to_f32()}),
                    *transform,
                    hex(color.r, color.g, color.b),
                    f32::from(*opa) / 255.0 * f32::from(color.a) / 255.0,
                )
            }
            SceneOp::Border {
                area,
                transform,
                color,
                width,
                radius,
                opa,
                quad,
            } => {
                assert!(quad.is_none());
                let half = *width / Fixed::from_int(2);
                (
                    json!({"tag":"rect", "x":(area.x+half).to_f32(), "y":(area.y+half).to_f32(), "width":(area.w-*width).to_f32(), "height":(area.h-*width).to_f32(), "rx":(*radius-half).max(Fixed::ZERO).to_f32(), "stroke-width":width.to_f32(), "fill":"none"}),
                    *transform,
                    hex(color.r, color.g, color.b),
                    f32::from(*opa) / 255.0 * f32::from(color.a) / 255.0,
                )
            }
            SceneOp::FillPath {
                path,
                transform,
                paint,
                opa,
                fill_rule,
            } => {
                let (ink, alpha) = self.paint(paint);
                (
                    json!({"tag":"path", "d":path_data(path), "fill-rule":if *fill_rule==FillRule::EvenOdd {"evenodd"}else{"nonzero"}}),
                    *transform,
                    ink,
                    f32::from(*opa) / 255.0 * alpha,
                )
            }
            SceneOp::StrokePath {
                path,
                transform,
                paint,
                width,
                opa,
                line_cap,
                line_join,
                miter_limit,
                dash,
            } => {
                let (ink, alpha) = self.paint(paint);
                (
                    json!({"tag":"path", "d":path_data(path), "fill":"none", "stroke-width":width.to_f32(), "stroke-linecap":format!("{line_cap:?}").to_lowercase(), "stroke-linejoin":format!("{line_join:?}").to_lowercase(), "stroke-miterlimit":miter_limit.to_f32(), "stroke-dasharray":if dash.is_empty(){"none".into()}else{dash.iter().map(|x| x.to_f32().to_string()).collect::<Vec<_>>().join(" ")}}),
                    *transform,
                    ink,
                    f32::from(*opa) / 255.0 * alpha,
                )
            }
            SceneOp::Line {
                p1,
                p2,
                transform,
                color,
                width,
                opa,
            } => (
                json!({"tag":"line", "x1":p1.x.to_f32(), "y1":p1.y.to_f32(), "x2":p2.x.to_f32(), "y2":p2.y.to_f32(), "stroke-width":width.to_f32()}),
                *transform,
                hex(color.r, color.g, color.b),
                f32::from(*opa) / 255.0 * f32::from(color.a) / 255.0,
            ),
            SceneOp::Arc {
                center,
                transform,
                radius,
                start_angle,
                end_angle,
                color,
                width,
                opa,
            } => {
                let start = start_angle.to_f32().to_radians();
                let span = (end_angle.to_f32() - start_angle.to_f32())
                    .clamp(-360.0, 360.0)
                    .to_radians();
                let segments = (span.abs() / std::f32::consts::PI).ceil().max(1.0) as u32;
                let x = center.x.to_f32();
                let y = center.y.to_f32();
                let r = radius.to_f32();
                let mut d = format!("M{} {}", x + r * start.cos(), y + r * start.sin());
                for segment in 1..=segments {
                    let a = start + span * segment as f32 / segments as f32;
                    write!(
                        d,
                        "A{r} {r} 0 0 {} {} {}",
                        u8::from(span >= 0.0),
                        x + r * a.cos(),
                        y + r * a.sin()
                    )
                    .expect("write arc");
                }
                (
                    json!({"tag":"path", "d":d, "fill":"none", "stroke-width":width.to_f32()}),
                    *transform,
                    hex(color.r, color.g, color.b),
                    f32::from(*opa) / 255.0 * f32::from(color.a) / 255.0,
                )
            }
            SceneOp::GlyphRun {
                font: ResourceRef::Index(index),
                ppem,
                pos,
                transform,
                color,
                opa,
                glyphs,
            } => {
                let font = self.fonts[*index as usize].clone();
                let mut uses = Vec::new();
                if let Some(first) = glyphs.first() {
                    for glyph in glyphs.iter() {
                        let mut shape = self.glyph(&font, glyph.glyph_id(), *ppem);
                        shape["x"] = json!(
                            shape["x"].as_f64().unwrap()
                                + f64::from(glyph.origin.x - first.origin.x + glyph.offset.x)
                                    / 256.0
                        );
                        shape["y"] = json!(
                            shape["y"].as_f64().unwrap()
                                + f64::from(glyph.origin.y - first.origin.y + glyph.offset.y)
                                    / 256.0
                                + f64::from(font.metrics(*ppem).ascender.to_f32())
                        );
                        uses.push(shape);
                    }
                }
                let index = self.define(json!({"kind":"text", "glyphs":uses}));
                (
                    json!({"tag":"use", "ref":index, "x":pos.x.to_f32(), "y":pos.y.to_f32()}),
                    *transform,
                    hex(color.r, color.g, color.b),
                    f32::from(*opa) / 255.0 * f32::from(color.a) / 255.0,
                )
            }
            other => panic!("Unsupported showcase operation: {other:?}"),
        };
        attrs["transform"] = matrix(transform);
        attrs["opacity"] = json!(opacity);
        let channel = if attrs.get("stroke-width").is_some() {
            "stroke"
        } else {
            "fill"
        };
        attrs[channel] = json!(fill);
        attrs
    }

    fn capture<B: Surface, F: mirui::app::RendererFactory<B>>(&mut self, app: &App<B, F>) -> Value {
        let mut scene = Scene::new();
        let clips = {
            let mut recorder = Recorder {
                inner: SceneRenderer::new(&mut scene, self),
                clips: Vec::new(),
            };
            mirui::ui::render_system::render(
                &app.world,
                app.root.expect("root"),
                &app.backend.viewport(),
                &mut recorder,
            )
            .expect("capture scene");
            assert!(recorder.inner.errors.is_empty());
            recorder.clips
        };
        assert_eq!(scene.ops.len(), clips.len());
        json!(
            scene
                .ops
                .iter()
                .zip(clips)
                .map(|(op, clip)| {
                    let mut node = self.node(op);
                    node["clip"] = rect(clip);
                    node
                })
                .collect::<Vec<_>>()
        )
    }
}

fn units(units: GradientUnits) -> &'static str {
    match units {
        GradientUnits::ObjectBoundingBox => "objectBoundingBox",
        GradientUnits::UserSpaceOnUse => "userSpaceOnUse",
    }
}

fn spread(spread: SpreadMode) -> &'static str {
    match spread {
        SpreadMode::Pad => "pad",
        SpreadMode::Repeat => "repeat",
        SpreadMode::Reflect => "reflect",
    }
}

fn main() {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("output JSON path");
    let mut export = Export::default();
    let mut screens = Vec::new();
    for theme in ["light", "dark"] {
        for (orientation, width, height) in [("wide", 1024, 640), ("tall", 360, 640)] {
            let mut app = App::headless(width, height);
            app.with_default_widgets().with_default_systems();
            let root = app.spawn_root().id();
            orbit_console::setup(&mut app, root, DemoRunMode::Capture);
            app.set_theme(if theme == "light" {
                "showcase-light"
            } else {
                "showcase-dark"
            })
            .expect("theme");
            app.world.insert_resource(DeltaTimeMs(100));
            let mut frames = Vec::new();
            for frame in 0..60 {
                if frame != 0 {
                    app.systems.run_all(&mut app.world).expect("animation");
                    mirui::core::reactive::flush_signal_dirty(&mut app.world);
                }
                app.render().expect("render showcase");
                frames.push(export.capture(&app));
            }
            screens.push(json!({"theme":theme,"orientation":orientation,"width":width,"height":height,"frames":frames}));
        }
    }
    std::fs::create_dir_all(output.parent().expect("output directory"))
        .expect("create output directory");
    let value = json!({"fps":10,"definitions":export.definitions,"screens":screens});
    std::fs::write(output, serde_json::to_vec(&value).expect("encode capture"))
        .expect("save capture");
}
