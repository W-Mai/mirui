use alloc::rc::Rc;

use super::*;
use crate::render::font::{
    Font, FontFaceId, FontManager, FontMetrics, FontProvider, FontStack, FontToken, GlyphId,
    RasterGlyph,
};
use crate::types::{Dimension, Viewport};
use crate::ui::geometry::effective_transform_3d;
use crate::ui::layout::{FlexDirection, LayoutStyle, Padding};
use crate::ui::widgets::Text;
use crate::ui::widgets::transform::WidgetTransform;
use crate::ui::widgets::transform_3d::WidgetTransform3D;

fn spawn(world: &mut World, parent: Option<Entity>, style: Style) -> Entity {
    let entity = world.spawn_empty();
    world.insert(entity, Widget);
    world.insert(entity, style);
    if let Some(parent) = parent {
        world.insert(entity, Parent(parent));
        if let Some(children) = world.get_mut::<Children>(parent) {
            children.0.push(entity);
        } else {
            world.insert(parent, Children(vec![entity]));
        }
    }
    entity
}

fn world() -> World {
    crate::app::App::headless(64, 64).world
}

#[test]
fn content_size_uses_shaped_font_metrics() {
    let mut world = world();
    let root = spawn(
        &mut world,
        None,
        Style {
            layout: LayoutStyle {
                direction: FlexDirection::Column,
                width: Dimension::px(64),
                height: Dimension::px(64),
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    let label = spawn(
        &mut world,
        Some(root),
        Style {
            layout: LayoutStyle {
                width: Dimension::Content,
                height: Dimension::Content,
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    world.insert(label, Text::from("abc"));

    try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();

    let rect = world.get::<super::super::ComputedRect>(label).unwrap().0;
    assert_eq!(rect.w, Fixed::from_int(24));
    assert_eq!(rect.h, Fixed::from_int(8));
}

#[test]
fn text_intrinsic_size_includes_padding() {
    let mut world = world();
    let root = spawn(
        &mut world,
        None,
        Style {
            layout: LayoutStyle {
                direction: FlexDirection::Column,
                width: Dimension::px(64),
                height: Dimension::px(64),
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    let label = spawn(
        &mut world,
        Some(root),
        Style {
            layout: LayoutStyle {
                width: Dimension::Content,
                height: Dimension::Content,
                padding: Padding {
                    top: Dimension::px(3),
                    right: Dimension::px(4),
                    bottom: Dimension::px(3),
                    left: Dimension::px(4),
                },
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    world.insert(label, Text::from("abc"));

    try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();

    let rect = world.get::<super::super::ComputedRect>(label).unwrap().0;
    assert_eq!(rect.w, Fixed::from_int(32));
    assert_eq!(rect.h, Fixed::from_int(14));
}

#[test]
fn constrained_text_height_tracks_wrapped_lines() {
    let mut world = world();
    let root = spawn(
        &mut world,
        None,
        Style {
            layout: LayoutStyle {
                direction: FlexDirection::Column,
                width: Dimension::px(64),
                height: Dimension::px(64),
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    let label = spawn(
        &mut world,
        Some(root),
        Style {
            layout: LayoutStyle {
                width: Dimension::px(24),
                height: Dimension::Content,
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    world.insert(label, Text::from("ab cd"));

    try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();

    let rect = world.get::<super::super::ComputedRect>(label).unwrap().0;
    assert_eq!(rect.w, Fixed::from_int(24));
    assert_eq!(rect.h, Fixed::from_int(16));
    let handle = world.get::<crate::text::TextLayoutHandle>(label).unwrap();
    let storage = world.resource::<Outputs<TextGeometry>>().unwrap().storage();
    let cache = &storage.layouts;
    assert_eq!(cache.get(*handle).unwrap().lines().len(), 2);
}

#[test]
fn text_padding_reduces_wrap_width_and_expands_content_height() {
    let mut world = world();
    let root = spawn(
        &mut world,
        None,
        Style {
            layout: LayoutStyle {
                direction: FlexDirection::Column,
                width: Dimension::px(64),
                height: Dimension::px(64),
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    let label = spawn(
        &mut world,
        Some(root),
        Style {
            layout: LayoutStyle {
                width: Dimension::px(32),
                height: Dimension::Content,
                padding: Padding {
                    top: Dimension::px(2),
                    right: Dimension::px(4),
                    bottom: Dimension::px(2),
                    left: Dimension::px(4),
                },
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    world.insert(label, Text::from("ab cd"));

    try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();

    let rect = world.get::<super::super::ComputedRect>(label).unwrap().0;
    assert_eq!(rect.w, Fixed::from_int(32));
    assert_eq!(rect.h, Fixed::from_int(20));
    let handle = world.get::<crate::text::TextLayoutHandle>(label).unwrap();
    let storage = world.resource::<Outputs<TextGeometry>>().unwrap().storage();
    let cache = &storage.layouts;
    assert_eq!(cache.get(*handle).unwrap().lines().len(), 2);
}

#[test]
fn text_paints_from_the_padded_content_origin() {
    #[derive(Default)]
    struct Recorder {
        pos: Option<Point>,
        text_run: Option<crate::render::TextRunIdentity>,
    }

    impl Renderer for Recorder {
        type TargetState = ();

        fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            let request = &draw.request();
            self.route(request)?;
            if let DrawCommand::GlyphRun { pos, .. } = request.command {
                self.pos = Some(*pos);
            }
            Ok(())
        }

        fn submit_text_run(
            &mut self,
            draw: &::mirui::render::PreparedDraw<'_>,
            identity: crate::render::TextRunIdentity,
        ) -> Result<(), RenderError> {
            self.text_run = Some(identity);
            self.submit(draw)
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    let mut app = crate::app::App::headless(64, 64);
    app.with_default_widgets();
    let mut world = app.world;
    let root = spawn(
        &mut world,
        None,
        Style {
            layout: LayoutStyle {
                width: Dimension::px(64),
                height: Dimension::px(64),
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    let label = spawn(
        &mut world,
        Some(root),
        Style {
            layout: LayoutStyle {
                width: Dimension::px(32),
                height: Dimension::px(20),
                padding: Padding {
                    top: Dimension::px(3),
                    right: Dimension::px(4),
                    bottom: Dimension::px(3),
                    left: Dimension::px(4),
                },
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    world.insert(label, Text::new("abc").with_capacity(8).unwrap());
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    try_update_layout(&mut world, root, &viewport).unwrap();

    let mut recorder = Recorder::default();
    render(&world, root, &viewport, &mut recorder).unwrap();

    assert_eq!(recorder.pos, Some(Point::new(4, 3)));
    assert_eq!(
        recorder.text_run,
        Some(crate::render::TextRunIdentity {
            entity: label,
            run_index: 0,
        })
    );

    world.get_mut::<Text>(label).unwrap().set_content("def");
    try_update_layout(&mut world, root, &viewport).unwrap();
    let mut recorder = Recorder::default();
    render(&world, root, &viewport, &mut recorder).unwrap();
    assert_eq!(
        recorder.text_run,
        Some(crate::render::TextRunIdentity {
            entity: label,
            run_index: 0,
        })
    );

    world.insert(label, Text::from("abc"));
    try_update_layout(&mut world, root, &viewport).unwrap();
    let mut recorder = Recorder::default();
    render(&world, root, &viewport, &mut recorder).unwrap();
    assert_eq!(recorder.pos, Some(Point::new(4, 3)));
    assert_eq!(recorder.text_run, None);
}

#[test]
fn clipped_text_uses_its_own_rect_as_draw_clip() {
    #[derive(Default)]
    struct Recorder {
        clip: Option<Rect>,
    }

    impl Renderer for Recorder {
        type TargetState = ();

        fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            let request = &draw.request();
            self.route(request)?;
            if matches!(request.command, DrawCommand::GlyphRun { .. }) {
                self.clip = Some(request.clip);
            }
            Ok(())
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    let mut app = crate::app::App::headless(64, 64);
    app.with_default_widgets();
    let mut world = app.world;
    let root = spawn(
        &mut world,
        None,
        Style {
            layout: LayoutStyle {
                direction: FlexDirection::Column,
                width: Dimension::px(64),
                height: Dimension::px(64),
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    let label = spawn(
        &mut world,
        Some(root),
        Style {
            layout: LayoutStyle {
                width: Dimension::px(16),
                height: Dimension::px(8),
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    world.insert(
        label,
        Text::from("ABCD").with_paragraph(crate::ui::widgets::ParagraphStyle {
            wrap: crate::ui::widgets::TextWrap::NoWrap,
            ..Default::default()
        }),
    );
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    try_update_layout(&mut world, root, &viewport).unwrap();

    let mut recorder = Recorder::default();
    render(&world, root, &viewport, &mut recorder).unwrap();
    assert_eq!(
        recorder.clip,
        Some(world.get::<super::super::ComputedRect>(label).unwrap().0)
    );
}

#[test]
fn rendering_reuses_the_constrained_text_layout_snapshot() {
    #[derive(Default)]
    struct Recorder {
        fills: Vec<Rect>,
    }

    impl Renderer for Recorder {
        type TargetState = ();

        fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            let request = &draw.request();
            self.route(request)?;
            if let DrawCommand::Fill { area, .. } = request.command {
                self.fills.push(*area);
            }
            Ok(())
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    let mut app = crate::app::App::headless(64, 64);
    app.with_default_widgets();
    let mut world = app.world;
    let root = spawn(
        &mut world,
        None,
        Style {
            layout: LayoutStyle {
                direction: FlexDirection::Column,
                width: Dimension::px(64),
                height: Dimension::px(64),
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    let label = spawn(
        &mut world,
        Some(root),
        Style {
            layout: LayoutStyle {
                width: Dimension::px(24),
                height: Dimension::Content,
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    world.insert(label, Text::from("ab cd"));
    spawn(
        &mut world,
        Some(root),
        Style {
            bg_color: Some(crate::types::Color::rgb(1, 2, 3).into()),
            layout: LayoutStyle {
                width: Dimension::px(8),
                height: Dimension::px(8),
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    let viewport = Viewport::new(64, 64, Fixed::ONE);

    try_update_layout(&mut world, root, &viewport).unwrap();
    let mut recorder = Recorder::default();
    render(&world, root, &viewport, &mut recorder).unwrap();

    assert!(recorder.fills.iter().any(|area| {
        area.x == Fixed::ZERO
            && area.y == Fixed::from_int(16)
            && area.w == Fixed::from_int(8)
            && area.h == Fixed::from_int(8)
    }));
}

#[test]
fn intrinsic_text_size_does_not_replace_flex_growth() {
    let mut world = world();
    let root = spawn(
        &mut world,
        None,
        Style {
            layout: LayoutStyle {
                direction: FlexDirection::Row,
                width: Dimension::px(64),
                height: Dimension::px(16),
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    let label = spawn(
        &mut world,
        Some(root),
        Style {
            layout: LayoutStyle {
                width: Dimension::Auto,
                height: Dimension::Content,
                grow: Fixed::ONE,
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    world.insert(label, Text::from("abc"));
    let trailing = spawn(
        &mut world,
        Some(root),
        Style {
            layout: LayoutStyle {
                width: Dimension::px(8),
                height: Dimension::px(8),
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );

    try_update_layout(&mut world, root, &Viewport::new(64, 16, Fixed::ONE)).unwrap();

    assert_eq!(
        world.get::<super::super::ComputedRect>(label).unwrap().0,
        Rect::new(0, 0, 56, 8)
    );
    assert_eq!(
        world.get::<super::super::ComputedRect>(trailing).unwrap().0,
        Rect::new(56, 0, 8, 8)
    );
}

#[test]
fn max_lines_constrains_content_height() {
    let mut world = world();
    let root = spawn(
        &mut world,
        None,
        Style {
            layout: LayoutStyle {
                direction: FlexDirection::Column,
                width: Dimension::px(64),
                height: Dimension::px(64),
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    let label = spawn(
        &mut world,
        Some(root),
        Style {
            layout: LayoutStyle {
                width: Dimension::px(16),
                height: Dimension::Content,
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    world.insert(
        label,
        Text::from("ab cd ef").with_paragraph(crate::ui::widgets::ParagraphStyle {
            max_lines: Some(2),
            ..Default::default()
        }),
    );

    try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();

    let rect = world.get::<super::super::ComputedRect>(label).unwrap().0;
    assert_eq!(rect.h, Fixed::from_int(16));
    let handle = world.get::<crate::text::TextLayoutHandle>(label).unwrap();
    let storage = world.resource::<Outputs<TextGeometry>>().unwrap().storage();
    let cache = &storage.layouts;
    assert_eq!(cache.get(*handle).unwrap().lines().len(), 2);
}

struct CjkFace;

impl FontProvider for CjkFace {
    fn face_id(&self) -> FontFaceId {
        FontFaceId::new(9)
    }

    fn map_char(&self, ch: char) -> Option<GlyphId> {
        (ch == '日').then_some(GlyphId::new(1))
    }

    fn glyph_advance(&self, _glyph: GlyphId, _ppem: u16) -> Option<Fixed> {
        Some(Fixed::from_int(10))
    }

    fn raster(
        &self,
        _glyph: GlyphId,
        _layout_ppem: u16,
        _output_ppem: u16,
    ) -> Option<RasterGlyph<'_>> {
        None
    }

    fn metrics(&self, _ppem: u16) -> FontMetrics {
        FontMetrics {
            ascender: Fixed::from_int(8),
            descender: Fixed::from_int(-2),
            line_height: Fixed::from_int(10),
        }
    }
}

#[test]
fn font_stack_falls_back_at_cluster_boundaries() {
    static FALLBACKS: [FontToken; 1] = [FontToken::Custom("cjk")];

    let mut world = world();
    world.resource::<FontManager>().unwrap().add_static(
        "cjk",
        Font {
            family: "cjk",
            size: 10,
            provider: Rc::new(CjkFace),
        },
    );
    let root = spawn(
        &mut world,
        None,
        Style {
            layout: LayoutStyle {
                direction: FlexDirection::Column,
                width: Dimension::px(64),
                height: Dimension::px(64),
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    let label = spawn(
        &mut world,
        Some(root),
        Style {
            font_stack: Some(FontStack::new(FontToken::Default).with_fallbacks(&FALLBACKS[..])),
            layout: LayoutStyle {
                width: Dimension::Content,
                height: Dimension::Content,
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    world.insert(label, Text::from("A日"));

    try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();

    let rect = world.get::<super::super::ComputedRect>(label).unwrap().0;
    assert_eq!(rect.w, Fixed::from_int(18));
    let handle = world.get::<crate::text::TextLayoutHandle>(label).unwrap();
    let storage = world.resource::<Outputs<TextGeometry>>().unwrap().storage();
    let cache = &storage.layouts;
    let layout = cache.get(*handle).unwrap();
    assert_eq!(layout.runs().len(), 2);
    assert_eq!(
        layout.runs()[0].font_id(),
        world
            .resource::<FontManager>()
            .unwrap()
            .resolve(FontToken::Default.cache_key())
            .face_id()
    );
    assert_eq!(layout.runs()[1].font_id(), FontFaceId::new(9));
}

struct PpemFace {
    id: u64,
    character: char,
}

impl FontProvider for PpemFace {
    fn face_id(&self) -> FontFaceId {
        FontFaceId::new(self.id)
    }

    fn map_char(&self, ch: char) -> Option<GlyphId> {
        (ch == self.character).then_some(GlyphId::new(1))
    }

    fn glyph_advance(&self, _glyph: GlyphId, ppem: u16) -> Option<Fixed> {
        Some(Fixed::from_int(i32::from(ppem) / 2))
    }

    fn raster(
        &self,
        _glyph: GlyphId,
        _layout_ppem: u16,
        _output_ppem: u16,
    ) -> Option<RasterGlyph<'_>> {
        None
    }

    fn metrics(&self, ppem: u16) -> FontMetrics {
        FontMetrics {
            ascender: Fixed::from_int(i32::from(ppem) * 3 / 4),
            descender: Fixed::from_int(-(i32::from(ppem) / 4)),
            line_height: Fixed::from_int(i32::from(ppem)),
        }
    }
}

#[test]
fn inherited_font_size_drives_intrinsic_measurement() {
    let mut world = world();
    world.resource::<FontManager>().unwrap().add_static(
        "scalable",
        Font {
            family: "scalable",
            size: 8,
            provider: Rc::new(PpemFace {
                id: 10,
                character: 'A',
            }),
        },
    );
    let root = spawn(
        &mut world,
        None,
        Style {
            font_stack: Some(FontStack::new(FontToken::Custom("scalable"))),
            font_size: 20.into(),
            layout: LayoutStyle {
                direction: FlexDirection::Column,
                width: Dimension::px(64),
                height: Dimension::px(64),
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    let label = spawn(
        &mut world,
        Some(root),
        Style {
            layout: LayoutStyle {
                width: Dimension::Content,
                height: Dimension::Content,
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    world.insert(label, Text::from("AAA"));

    try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();

    let rect = world.get::<super::super::ComputedRect>(label).unwrap().0;
    assert_eq!(rect.w, Fixed::from_int(30));
    assert_eq!(rect.h, Fixed::from_int(20));
}

#[test]
fn fallback_faces_share_the_paragraph_ppem() {
    static FALLBACKS: [FontToken; 1] = [FontToken::Custom("fallback-scalable")];

    let mut world = world();
    for (token, family, size, id, character) in [
        ("primary-scalable", "primary-scalable", 8, 11, 'A'),
        ("fallback-scalable", "fallback-scalable", 30, 12, '日'),
    ] {
        world.resource::<FontManager>().unwrap().add_static(
            token,
            Font {
                family,
                size,
                provider: Rc::new(PpemFace { id, character }),
            },
        );
    }
    let root = spawn(
        &mut world,
        None,
        Style {
            layout: LayoutStyle {
                direction: FlexDirection::Column,
                width: Dimension::px(64),
                height: Dimension::px(64),
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    let label = spawn(
        &mut world,
        Some(root),
        Style {
            font_stack: Some(
                FontStack::new(FontToken::Custom("primary-scalable"))
                    .with_fallbacks(&FALLBACKS[..]),
            ),
            font_size: 20.into(),
            layout: LayoutStyle {
                width: Dimension::Content,
                height: Dimension::Content,
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    world.insert(label, Text::from("A日"));

    try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();

    let rect = world.get::<super::super::ComputedRect>(label).unwrap().0;
    assert_eq!(rect.w, Fixed::from_int(20));
    assert_eq!(rect.h, Fixed::from_int(20));
}

#[test]
fn path_text_intrinsic_size_ignores_widget_padding() {
    let mut app = crate::app::App::headless(96, 96);
    app.with_default_widgets();
    let mut world = app.world;
    let mut baseline = crate::render::path::Path::new();
    baseline
        .move_to(Point::new(0, 24))
        .line_to(Point::new(64, 24));
    let path = world
        .resource_mut::<crate::render::path::PathStore>()
        .unwrap()
        .insert(baseline)
        .unwrap();
    let plain = spawn(&mut world, None, Style::default());
    world.insert(plain, Text::from("ABC"));
    world.widget_mut(plain).unwrap().text_path(path);
    let padded_style = Style {
        layout: LayoutStyle {
            padding: Padding::all(Dimension::px(12)),
            ..LayoutStyle::default()
        },
        ..Style::default()
    };
    let padded_layout = padded_style.layout;
    let padded = spawn(&mut world, None, padded_style);
    world.insert(padded, Text::from("ABC"));
    world.widget_mut(padded).unwrap().text_path(path);

    let mut plain_node = LayoutNode::new(LayoutStyle::default());
    apply_text_intrinsic(&world, plain, &mut plain_node).unwrap();
    let mut padded_node = LayoutNode::new(padded_layout);
    apply_text_intrinsic(&world, padded, &mut padded_node).unwrap();

    assert_eq!(padded_node.intrinsic_width, plain_node.intrinsic_width);
    assert_eq!(padded_node.intrinsic_height, plain_node.intrinsic_height);
}

#[test]
fn input_layout_is_prepared_before_paint_and_tracks_its_display_content() {
    use crate::prelude::*;
    use crate::ui::ComputedRect;
    use crate::ui::widgets::Placeholder;
    use crate::ui::widgets::TextInput;
    use crate::ui::widgets::text_editing::geometry::TextEditingLayoutHandle;

    #[crate::compose]
    fn input() -> Entity {
        crate::ui!(TextInput(width: 64, height: 20) [Placeholder("Type")] {})
    }

    fn prepared(world: &World, input: Entity) -> (crate::text::TextLayoutHandle, usize) {
        let handle = world.get::<TextEditingLayoutHandle>(input).unwrap().0;
        let storage = world.resource::<Outputs<TextGeometry>>().unwrap().storage();
        (handle, storage.layouts.get(handle).unwrap().glyphs().len())
    }

    let mut app = crate::app::App::headless(96, 48);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    assert!(!prepare_text_geometry(&mut app.world, &[root]).unwrap());
    let input = app.compose(root, input());
    let viewport = Viewport::new(96, 48, Fixed::ONE);
    try_update_layout(&mut app.world, root, &viewport).unwrap();
    let (placeholder, glyphs) = prepared(&app.world, input);
    assert_eq!(glyphs, 4);
    assert!(!prepare_text_geometry(&mut app.world, &[root, input]).unwrap());
    app.world.remove::<TextEditingLayoutHandle>(input);
    assert!(prepare_text_geometry(&mut app.world, &[root, input]).unwrap());
    assert_eq!(prepared(&app.world, input), (placeholder, 4));
    let editing = app
        .world
        .remove::<super::super::widgets::TextEditing>(input)
        .unwrap();
    let rect = app.world.get::<ComputedRect>(input).unwrap().0;
    assert!(prepare_input_layout(&mut app.world, input, rect).unwrap());
    assert!(!prepare_input_layout(&mut app.world, input, rect).unwrap());
    assert!(!prepare_text_geometry(&mut app.world, &[root, input]).unwrap());
    app.world.insert(input, editing);
    assert!(prepare_text_geometry(&mut app.world, &[root, input]).unwrap());
    assert_eq!(prepared(&app.world, input).1, 4);
    for _ in 0..3 {
        try_update_layout(&mut app.world, root, &viewport).unwrap();
        assert_eq!(prepared(&app.world, input), (placeholder, 4));
    }
    app.render_dirty().unwrap();

    app.world.get_mut::<Placeholder>(input).unwrap().0 = "Name";
    app.render_dirty().unwrap();
    assert_ne!(prepared(&app.world, input).0, placeholder);
    assert_eq!(prepared(&app.world, input).1, 4);
    let document = app
        .add_model(crate::ui::widgets::TextDocument::new())
        .finish();
    app.world.insert(
        input,
        crate::ui::widgets::TextEditing::new(document.clone()),
    );
    document.set_text("AB").unwrap();
    app.render_dirty().unwrap();
    let (content, glyphs) = prepared(&app.world, input);
    assert_eq!(glyphs, 2);
    assert_ne!(content, placeholder);
    app.world
        .get_mut::<crate::ui::widgets::TextEditing>(input)
        .unwrap()
        .set_selection(crate::ui::widgets::TextSelection::default())
        .unwrap();
    app.render_dirty().unwrap();
    assert_eq!(prepared(&app.world, input), (content, 2));

    app.world.get_mut::<Style>(input).unwrap().layout.width = Dimension::px(32);
    app.render_dirty().unwrap();
    let (resized, _) = prepared(&app.world, input);
    assert_ne!(resized, content);
    assert_eq!(
        app.world.get::<ComputedRect>(input).unwrap().0.w,
        Fixed::from_int(32)
    );
    document.set_text("").unwrap();
    app.render_dirty().unwrap();
    assert_eq!(prepared(&app.world, input).1, 4);
    app.world.remove::<Placeholder>(input);
    app.render_dirty().unwrap();
    assert_eq!(prepared(&app.world, input).1, 0);

    app.world
        .remove_resource::<crate::render::font::FontManager>();
    app.handle_memory_warning();
    app.render().unwrap();
    assert!(app.world.get::<TextEditingLayoutHandle>(input).is_none());
}

#[test]
fn input_layout_recycles_bounded_storage_across_content_updates() {
    use crate::text::{TextLayoutCapacity, WorkspaceCapacity};
    use crate::ui::widgets::TextInput;
    use crate::ui::widgets::text_editing::geometry::TextEditingLayoutHandle;

    let mut app = crate::app::App::headless(64, 32);
    app.with_default_widgets();
    app.require_text_layout_capacity(TextLayoutCapacity {
        layout_slots: 2,
        measurements: 0,
        lines: 3,
        runs: 3,
        glyphs: 6,
        carets: 12,
        workspace: WorkspaceCapacity {
            runs: 2,
            glyphs: 2,
            scratch_glyphs: 2,
            lines: 1,
        },
    })
    .unwrap();
    let root = app.spawn_root().id();
    let input = app.compose(root, |cx| cx.spawn(TextInput::build()));
    let document = app
        .add_model(crate::ui::widgets::TextDocument::new())
        .finish();
    app.world.insert(
        input,
        crate::ui::widgets::TextEditing::new(document.clone()),
    );
    for byte in b'A'..=b'Z' {
        document
            .set_text(core::str::from_utf8(&[byte]).unwrap())
            .unwrap();
        app.render_dirty().unwrap();
        let handle = app.world.get::<TextEditingLayoutHandle>(input).unwrap().0;
        let cache = app
            .world
            .resource::<Outputs<TextGeometry>>()
            .unwrap()
            .storage();
        assert_eq!(cache.layouts.get(handle).unwrap().glyphs().len(), 1);
    }
}

#[test]
fn input_layout_failure_is_reported_before_paint() {
    use crate::prelude::*;
    use crate::ui::widgets::Placeholder;
    use crate::ui::widgets::TextInput;
    use crate::ui::widgets::text_editing::geometry::TextEditingLayoutHandle;

    #[crate::compose]
    fn input() -> Entity {
        crate::ui!(TextInput(width: 64, height: 20) [Placeholder("Type")] {})
    }
    let mut app = crate::app::App::headless(96, 48);
    app.with_default_widgets();
    app.with_text_layout_limits(crate::text::TextLayoutLimits {
        text_bytes: 2,
        ..crate::text::TextLayoutLimits::default()
    });
    let root = app.spawn_root().id();
    let input = app.compose(root, input());
    assert!(matches!(app.render(), Err(RenderError::TextLayout)));
    assert_eq!(app.last_text_layout_failure().unwrap().entity, input);
    assert!(app.world.get::<TextEditingLayoutHandle>(input).is_none());
    assert!(!app.world.resource::<LayoutSnapshot>().unwrap().valid);
    app.world.remove::<Placeholder>(input);
    app.render_dirty().unwrap();
    assert!(app.last_text_layout_failure().is_none());
    assert!(app.world.get::<TextEditingLayoutHandle>(input).is_some());
    assert!(app.world.resource::<LayoutSnapshot>().unwrap().valid);
    assert!(crate::input::event::hit_test::geometry_matches(
        &app.world, root, 96, 48
    ));
    app.world.insert(input, Placeholder("Type"));
    for _ in 0..2 {
        assert!(matches!(app.dirty_regions(), Err(RenderError::TextLayout)));
        assert_eq!(app.last_text_layout_failure().unwrap().entity, input);
        assert!(!app.world.resource::<LayoutSnapshot>().unwrap().valid);
        assert!(!crate::input::event::hit_test::geometry_matches(
            &app.world, root, 96, 48
        ));
    }
    app.world.remove::<Placeholder>(input);
    app.render_dirty().unwrap();
    assert!(app.last_text_layout_failure().is_none());
    assert!(app.world.get::<TextEditingLayoutHandle>(input).is_some());
    assert!(app.world.resource::<LayoutSnapshot>().unwrap().valid);
}

#[test]
fn path_placements_are_ready_before_paint_and_survive_idle_and_visual_updates() {
    use crate::render::path::{Path, PathId, PathStore};
    use crate::text::baseline::{DEFAULT_TOLERANCE, PlacementKey};
    use crate::text::{PathCaretGeometry, TextLayoutHandle, TextPath};

    mod demand {
        use super::*;

        #[crate::component]
        #[depends_on(TextGeometry)]
        pub(super) struct GeometryDemand;

        #[crate::view(component = GeometryDemand, name = "GeometryDemand")]
        pub(super) fn geometry_demand() {}
    }

    #[crate::compose]
    fn label(path: PathId) -> Entity {
        crate::ui!(Text("AB", width: 64, height: 16) [
                TextPath::new(path).with_range(Fixed::ZERO..Fixed::from_int(64)),
                demand::GeometryDemand,
            ] {} )
    }

    fn key(world: &World, label: Entity, path: PathId) -> PlacementKey {
        PlacementKey::new(
            label,
            *world.get::<TextLayoutHandle>(label).unwrap(),
            *world.get::<TextPath>(label).unwrap(),
            world
                .resource::<PathStore>()
                .unwrap()
                .revision(path)
                .unwrap(),
            DEFAULT_TOLERANCE,
        )
    }

    let mut paths = PathStore::new(1).unwrap();
    let mut line = Path::new();
    line.move_to(Point::new(0, 20)).line_to(Point::new(64, 20));
    let path = paths.insert(line).unwrap();
    let mut app = crate::app::App::headless(96, 96);
    app.with_default_widgets()
        .with_widget(demand::geometry_demand::view())
        .add_resource(paths);
    let root = app.spawn_root().id();
    let label = app.compose(root, label(path));
    let viewport = Viewport::new(96, 96, Fixed::ONE);
    try_update_layout(&mut app.world, root, &viewport).unwrap();
    let initial_key = key(&app.world, label, path);
    let initial_origin = app
        .world
        .resource::<Outputs<TextGeometry>>()
        .unwrap()
        .storage()
        .paths
        .glyph_frames(initial_key, |frames| frames[0].local_origin)
        .unwrap();
    let initial_handle = *app.world.get::<TextLayoutHandle>(label).unwrap();
    let mut carets = [PathCaretGeometry::default(); 8];
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            let geometry = app
                .world
                .resource::<Outputs<TextGeometry>>()
                .unwrap()
                .get(label)
                .unwrap();
            assert!(!geometry.carets_into(&mut carets).unwrap().is_empty());
            assert!(
                crate::ui::widgets::text::path_text_ink_bounds(
                    &app.world,
                    label,
                    app.world
                        .get::<super::super::ComputedRect>(label)
                        .unwrap()
                        .0,
                    Transform::IDENTITY,
                    Fixed::ONE,
                )
                .is_some()
            );
        }),
        0
    );

    let mut damage = DirtyRegions::default();
    try_collect_dirty_regions_into(&mut app.world, root, &viewport, &mut damage).unwrap();
    app.render().unwrap();
    for _ in 0..3 {
        try_collect_dirty_regions_into(&mut app.world, root, &viewport, &mut damage).unwrap();
        assert!(damage.rects.is_empty());
        assert!(
            app.world
                .resource::<Outputs<TextGeometry>>()
                .unwrap()
                .storage()
                .paths
                .glyph_frames(initial_key, |frames| !frames.is_empty())
                .unwrap()
        );
    }

    app.world
        .resource_mut::<PathStore>()
        .unwrap()
        .edit(path, |line| {
            line.set_command(0, crate::render::path::PathCmd::MoveTo(Point::new(0, 28)))
                .unwrap();
            line.set_command(1, crate::render::path::PathCmd::LineTo(Point::new(64, 28)))
                .unwrap();
        })
        .unwrap();
    app.world.invalidate_visual(label);
    assert!(!super::super::dirty::has_visible_dirty::<
        super::super::dirty::Dirty,
    >(&app.world, root));
    try_collect_dirty_regions_into(&mut app.world, root, &viewport, &mut damage).unwrap();
    assert_eq!(
        *app.world.get::<TextLayoutHandle>(label).unwrap(),
        initial_handle
    );
    let current_key = key(&app.world, label, path);
    {
        let outputs = app.world.resource::<Outputs<TextGeometry>>().unwrap();
        let storage = outputs.storage();
        assert!(storage.paths.glyph_frames(initial_key, |_| ()).is_none());
        assert_eq!(
            storage
                .paths
                .glyph_frames(current_key, |frames| frames[0].local_origin.y)
                .unwrap(),
            initial_origin.y + (8 << 8)
        );
        assert!(
            !outputs
                .get(label)
                .unwrap()
                .carets_into(&mut carets)
                .unwrap()
                .is_empty()
        );
    }

    app.world
        .remove_resource::<crate::render::font::FontManager>();
    app.handle_memory_warning();
    app.render().unwrap();
    assert!(
        app.world
            .resource::<Outputs<TextGeometry>>()
            .unwrap()
            .storage()
            .paths
            .glyph_frames(current_key, |_| ())
            .is_none()
    );
    assert!(
        app.world
            .resource::<Outputs<TextGeometry>>()
            .unwrap()
            .get(label)
            .is_none()
    );
}

#[test]
fn path_text_ink_drives_culling_and_damage_outside_layout_bounds() {
    use crate::text::{PathSelectionRibbon, PathTextGeometryError};

    mod demand {
        use super::*;

        #[crate::component]
        #[depends_on(TextGeometry)]
        pub(super) struct GeometryDemand;

        #[crate::view(component = GeometryDemand, name = "GeometryDemand")]
        pub(super) fn geometry_demand() {}
    }

    #[derive(Default)]
    struct Recorder {
        posed_runs: usize,
    }

    impl Renderer for Recorder {
        type TargetState = ();

        fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            let request = &draw.request();
            self.route(request)?;
            if matches!(request.command, DrawCommand::PosedGlyphRun { .. }) {
                self.posed_runs += 1;
            }
            Ok(())
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    let mut app = crate::app::App::headless(96, 96);
    app.with_default_widgets()
        .with_widget(demand::geometry_demand::view());
    let mut world = app.world;
    let root = spawn(
        &mut world,
        None,
        Style {
            layout: LayoutStyle {
                direction: FlexDirection::Column,
                width: Dimension::px(96),
                height: Dimension::px(96),
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    let label = spawn(
        &mut world,
        Some(root),
        Style {
            layout: LayoutStyle {
                width: Dimension::Content,
                height: Dimension::Content,
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    world.insert(label, Text::from("ABCDEFG"));
    world.insert(label, demand::GeometryDemand);
    let mut baseline = crate::render::path::Path::new();
    baseline
        .move_to(Point::new(0, 48))
        .line_to(Point::new(64, 48));
    let path = world
        .resource_mut::<crate::render::path::PathStore>()
        .unwrap()
        .insert(baseline)
        .unwrap();
    world.widget_mut(label).unwrap().text_path(path);
    let viewport = Viewport::new(96, 96, Fixed::ONE);

    try_update_layout(&mut world, root, &viewport).unwrap();

    let layout = world.get::<super::super::ComputedRect>(label).unwrap().0;
    let ink = crate::ui::widgets::text::path_text_ink_bounds(
        &world,
        label,
        layout,
        Transform::IDENTITY,
        Fixed::ONE,
    )
    .unwrap();
    assert!(ink.y >= layout.y + layout.h);

    let mut ribbons = [PathSelectionRibbon::default(); 3];
    let local_selection = {
        let geometry = world
            .resource::<Outputs<TextGeometry>>()
            .unwrap()
            .get(label)
            .unwrap();
        assert_eq!(geometry.transform(), Transform3D::IDENTITY);
        let hit = geometry
            .hit_test(Point::new(16, 48), Fixed::from_int(2))
            .unwrap()
            .unwrap();
        assert_eq!(hit.text_offset(), 2);
        assert_eq!(hit.bidi_level(), 0);
        let selection = geometry.selection_into(1..3, &mut ribbons).unwrap();
        assert_eq!(selection.len(), 2);
        assert_eq!(selection[0].text_range(), 1..2);
        assert_eq!(selection[1].text_range(), 2..3);
        assert!(
            selection
                .iter()
                .all(|ribbon| ribbon.quad()[0] != ribbon.quad()[1])
        );
        let sentinel = PathSelectionRibbon::default();
        let mut insufficient = [sentinel; 1];
        assert_eq!(
            geometry.selection_into(1..3, &mut insufficient),
            Err(PathTextGeometryError::InsufficientCapacity {
                required: 2,
                provided: 1,
            })
        );
        assert_eq!(insufficient, [sentinel; 1]);
        [selection[0], selection[1]]
    };

    let translated = Transform::translate(Fixed::from_int(10), Fixed::from_int(5));
    world.insert(label, WidgetTransform(translated));
    try_update_layout(&mut world, root, &viewport).unwrap();
    {
        let geometry = world
            .resource::<Outputs<TextGeometry>>()
            .unwrap()
            .get(label)
            .unwrap();
        let probe = geometry
            .transform()
            .apply_point(Point::new(16, 48))
            .unwrap();
        assert_eq!(probe, Point::new(26, 53));
        let hit = geometry
            .hit_test(probe, Fixed::from_int(2))
            .unwrap()
            .unwrap();
        assert_eq!(hit.text_offset(), 2);
        assert_eq!(hit.bidi_level(), 0);
        let miss = geometry
            .transform()
            .apply_point(Point::new(16, 65))
            .unwrap();
        assert!(
            geometry
                .hit_test(miss, Fixed::from_int(2))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            geometry.selection_into(1..3, &mut ribbons).unwrap(),
            &local_selection
        );
    }
    let projective = Transform3D::rotate_y_perspective(Fixed::from_int(12), Fixed::from_int(320));
    world.insert(label, WidgetTransform3D(projective));
    try_update_layout(&mut world, root, &viewport).unwrap();
    {
        let geometry = world
            .resource::<Outputs<TextGeometry>>()
            .unwrap()
            .get(label)
            .unwrap();
        let probe = geometry
            .transform()
            .apply_point(Point::new(16, 48))
            .unwrap();
        let hit = geometry
            .hit_test(probe, Fixed::from_int(2))
            .unwrap()
            .unwrap();
        assert_eq!(hit.text_offset(), 2);
        assert_eq!(
            geometry.selection_into(1..3, &mut ribbons).unwrap(),
            &local_selection
        );
    }
    let mut behind = Transform3D::IDENTITY;
    behind.m22 = -crate::types::Fixed64::ONE;
    world.insert(label, WidgetTransform3D(behind));
    try_update_layout(&mut world, root, &viewport).unwrap();
    {
        let geometry = world
            .resource::<Outputs<TextGeometry>>()
            .unwrap()
            .get(label)
            .unwrap();
        assert_eq!(
            geometry.hit_test(Point::ZERO, Fixed::ONE),
            Err(PathTextGeometryError::InvalidProjection)
        );
        assert_eq!(
            geometry.selection_into(1..3, &mut ribbons).unwrap(),
            &local_selection
        );
    }
    world.remove::<WidgetTransform>(label);
    world.insert(label, WidgetTransform3D(projective));
    world.insert(
        root,
        crate::input::event::scroll::ScrollOffset {
            x: Fixed::from_int(3),
            y: Fixed::from_int(2),
        },
    );
    try_update_layout(&mut world, root, &viewport).unwrap();
    let shifted_layout = Rect {
        x: layout.x - Fixed::from_int(3),
        y: layout.y - Fixed::from_int(2),
        ..layout
    };
    let widget_projection =
        effective_transform_3d(&Transform3D::IDENTITY, &world, label, shifted_layout);
    let widget_probe = widget_projection.apply_point(Point::new(13, 46)).unwrap();
    {
        let geometry = world
            .resource::<Outputs<TextGeometry>>()
            .unwrap()
            .get(label)
            .unwrap();
        assert_eq!(
            geometry.transform().apply_point(Point::new(16, 48)),
            Some(widget_probe)
        );
        assert_eq!(
            geometry
                .hit_test(widget_probe, Fixed::from_int(2))
                .unwrap()
                .unwrap()
                .text_offset(),
            2
        );
        assert_eq!(
            geometry.selection_into(1..3, &mut ribbons).unwrap(),
            &local_selection
        );
    }
    world.remove::<WidgetTransform3D>(label);
    world.remove::<crate::input::event::scroll::ScrollOffset>(root);
    try_update_layout(&mut world, root, &viewport).unwrap();

    let mut recorder = Recorder::default();
    render_region(&world, root, &viewport, &ink, &mut recorder).unwrap();
    assert_eq!(recorder.posed_runs, 1);

    let damage = collect_dirty_region(&mut world, root, &viewport)
        .unwrap()
        .unwrap();
    assert!(damage.y + damage.h >= ink.y + ink.h);
}

#[test]
fn path_text_uses_the_ancestor_clip_without_changing_placed_geometry() {
    #[derive(Default)]
    struct Recorder {
        clip: Option<Rect>,
        origin: Option<Point>,
    }

    impl Renderer for Recorder {
        type TargetState = ();

        fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            let request = &draw.request();
            self.route(request)?;
            if let DrawCommand::PosedGlyphRun { pos, .. } = request.command {
                self.clip = Some(request.clip);
                self.origin = Some(*pos);
            }
            Ok(())
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    let mut app = crate::app::App::headless(96, 96);
    app.with_default_widgets();
    let mut world = app.world;
    let root = spawn(
        &mut world,
        None,
        Style {
            layout: LayoutStyle {
                width: Dimension::px(96),
                height: Dimension::px(96),
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    let clipper = spawn(
        &mut world,
        Some(root),
        Style {
            clip_children: true,
            layout: LayoutStyle {
                width: Dimension::px(10),
                height: Dimension::px(30),
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    let label = spawn(
        &mut world,
        Some(clipper),
        Style {
            layout: LayoutStyle {
                padding: Padding::all(Dimension::px(5)),
                ..LayoutStyle::default()
            },
            ..Style::default()
        },
    );
    world.insert(label, Text::from("ABC"));
    let mut baseline = crate::render::path::Path::new();
    baseline
        .move_to(Point::new(0, 20))
        .quad_to(Point::new(32, 4), Point::new(64, 20));
    let path = world
        .resource_mut::<crate::render::path::PathStore>()
        .unwrap()
        .insert(baseline)
        .unwrap();
    world.widget_mut(label).unwrap().text_path(path);
    let viewport = Viewport::new(96, 96, Fixed::ONE);
    try_update_layout(&mut world, root, &viewport).unwrap();

    let label_rect = world.get::<super::super::ComputedRect>(label).unwrap().0;
    let uncut_ink = crate::ui::widgets::text::path_text_ink_bounds(
        &world,
        label,
        label_rect,
        Transform::IDENTITY,
        Fixed::ONE,
    )
    .unwrap();
    let clip = world.get::<super::super::ComputedRect>(clipper).unwrap().0;
    assert!(
        uncut_ink.x + uncut_ink.w > clip.x + clip.w,
        "ink {uncut_ink:?} must extend beyond clip {clip:?}"
    );

    let mut recorder = Recorder::default();
    render_region(
        &world,
        root,
        &viewport,
        &Rect::new(0, 0, 96, 96),
        &mut recorder,
    )
    .unwrap();
    assert_eq!(recorder.clip, Some(clip));
    assert_eq!(
        recorder.origin,
        Some(Point {
            x: label_rect.x,
            y: label_rect.y,
        })
    );
}
