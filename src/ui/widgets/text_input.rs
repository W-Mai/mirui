use crate::ecs::{Entity, World};

use crate::render::command::DrawCommand;
use crate::text::TextGeometry;
use crate::types::{Fixed, Point, Rect};
use crate::ui::outputs::Outputs;
use crate::ui::theme::{ColorToken, ThemedColor};

use super::text_document::TextEditError;
use super::text_editing::{CursorBlinkPhase, TextEditing};

/// Single-line editor appearance. TextEditing supplies its document and input behavior.
#[crate::component]
#[derive(Clone)]
pub struct TextInput {
    pub text_color: ThemedColor,
    pub placeholder_color: ThemedColor,
    pub cursor_color: ThemedColor,
    pub focus_border_color: ThemedColor,
}

impl TextInput {
    pub fn new() -> Self {
        Self {
            text_color: ThemedColor::Token(ColorToken::OnSurface),
            placeholder_color: ThemedColor::Token(ColorToken::OnSurfaceVariant),
            cursor_color: ThemedColor::Token(ColorToken::OnSurface),
            focus_border_color: ThemedColor::Token(ColorToken::Primary),
        }
    }

    pub fn with_text_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.text_color = color.into();
        self
    }

    pub fn with_placeholder_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.placeholder_color = color.into();
        self
    }

    pub fn with_cursor_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.cursor_color = color.into();
        self
    }

    pub fn with_focus_border_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.focus_border_color = color.into();
        self
    }

    pub fn build() -> TextInputBuilder {
        TextInputBuilder {
            text_input: TextInput::new(),
            editing: TextEditing::default(),
            style: None,
            placeholder: None,
            properties: (),
            bindings: (),
        }
    }
}

impl Default for TextInput {
    fn default() -> Self {
        Self::new()
    }
}

pub struct TextInputBuilder<P = (), B = ()> {
    text_input: TextInput,
    editing: TextEditing,
    style: Option<crate::ui::Style>,
    placeholder: Option<&'static str>,
    properties: P,
    bindings: B,
}

impl<P, B> TextInputBuilder<P, B> {
    pub fn capacity(mut self, capacity: usize) -> Result<Self, TextEditError> {
        self.editing = self.editing.with_capacity(capacity)?;
        Ok(self)
    }

    pub fn preedit_capacity(mut self, capacity: usize) -> Result<Self, TextEditError> {
        self.editing = self.editing.with_preedit_capacity(capacity)?;
        Ok(self)
    }

    pub fn style(mut self, style: crate::ui::Style) -> Self {
        self.style = Some(style);
        self
    }

    pub fn placeholder(mut self, ph: &'static str) -> Self {
        self.placeholder = Some(ph);
        self
    }

    pub fn font(mut self, token: impl Into<crate::render::font::FontToken>) -> Self {
        self.style.get_or_insert_default().set_font_token(token);
        self
    }

    pub fn font_stack(mut self, stack: impl Into<crate::render::font::FontStack>) -> Self {
        self.style.get_or_insert_default().set_font_stack(stack);
        self
    }

    pub fn font_size(mut self, size: u16) -> Self {
        self.style.get_or_insert_default().set_font_size(size);
        self
    }

    pub fn text_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.text_input.text_color = color.into();
        self
    }

    pub fn placeholder_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.text_input.placeholder_color = color.into();
        self
    }

    pub fn cursor_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.text_input.cursor_color = color.into();
        self
    }

    pub fn focus_border_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.text_input.focus_border_color = color.into();
        self
    }
}

crate::ui::recipe::widget_builder_bindings!(
    TextInputBuilder,
    TextEditing,
    text_input,
    editing,
    style,
    placeholder
);

impl<P: crate::__macro_support::PropertyBindings, B: crate::__macro_support::EntityEventBindings>
    crate::ui::recipe::WidgetRecipe for TextInputBuilder<P, B>
{
    type Properties = P;
    type Events = B;

    fn insert(self, world: &mut World, entity: Entity) -> (P, B) {
        world.insert(entity, self.text_input);
        world.insert(entity, self.editing);
        if let Some(style) = self.style {
            world.insert(entity, style);
        }
        if let Some(ph) = self.placeholder {
            world.insert(entity, Placeholder(ph));
        }
        (self.properties, self.bindings)
    }
}

/// Optional placeholder text rendered when the buffer is empty. Stored
/// as a separate component so the common case (no placeholder) doesn't
/// pay 32 extra bytes inside `TextInput`.
#[crate::component]
pub struct Placeholder(pub &'static str);

use super::text_editing::geometry::{
    TextEditingLayoutHandle, caret_and_scroll, input_content_rect,
};

#[crate::view(
    component = TextInput,
    name = "TextInput",
    invalidation = Layout,
    priority = 70,
    attach = text_input_attach::callback()
)]
fn text_input_render(
    // Placeholder remains a dependency of the prepared layout.
    rows: query!(TextInput, TextEditing, TextEditingLayoutHandle, optional Placeholder),
    entity: entity!(),
    outputs: res!(optional Outputs<TextGeometry>),
    manager: res!(optional crate::render::font::FontManager),
    theme: res!(crate::ui::Theme),
    phase: res!(optional CursorBlinkPhase),
    paint: paint!(),
) {
    let Some((ti, editing, handle, _)) = rows.get(entity) else {
        return;
    };
    let Some(outputs) = outputs else {
        return;
    };
    let face_limit = outputs.storage().layouts.limits().fallback_faces;
    let Ok(Some(fonts)) = crate::render::font::ResolvedFontStack::resolve(
        manager.as_deref(),
        &paint.style().font_stack,
        paint.style().font_size,
        face_limit,
    ) else {
        return;
    };
    let storage = outputs.storage();
    let Some(layout) = storage.layouts.get(handle.0) else {
        return;
    };
    let metrics = fonts.primary().metrics(fonts.primary().size);
    let state = paint.state();
    let rect = paint.rect();
    let text_color = ti.text_color.resolve_in(&theme, state);
    let placeholder_color = ti.placeholder_color.resolve_in(&theme, state);
    let cursor_color = ti.cursor_color.resolve_in(&theme, state);
    let focus_border_color = ti.focus_border_color.resolve_in(&theme, state);

    if editing.focused() {
        paint.draw(&DrawCommand::Border {
            area: rect,
            transform: paint.transform(),
            quad: paint.quad(),
            color: focus_border_color,
            width: Fixed::ONE,
            radius: Fixed::ZERO,
            opa: 255,
        });
    }

    let content_rect = input_content_rect(rect);
    let Some(content_clip) = paint.clip().intersect(&content_rect) else {
        return;
    };
    let display_empty = editing.read_display(str::is_empty);
    let color = if display_empty {
        placeholder_color
    } else {
        text_color
    };
    let (caret, scroll) = caret_and_scroll(&layout, editing, content_rect);
    let transform = paint.transform();
    for (selection, underline) in [
        (editing.display_selection(), false),
        (editing.preedit_range().unwrap_or(0..0), true),
    ] {
        if editing.focused() && !selection.is_empty() && !display_empty {
            let selection = selection.start as u32..selection.end as u32;
            for line in layout.lines() {
                let range = line.carets();
                let carets = &layout.carets()[range.start as usize..range.end as usize];
                for run in layout.runs_for(*line).unwrap_or_default() {
                    let span = carets
                        .windows(2)
                        .filter(|pair| {
                            super::text::selected_pair(
                                pair,
                                run.text(),
                                run.bidi_level(),
                                &selection,
                            )
                        })
                        .fold(None, |span: Option<(i32, i32)>, pair| {
                            let left = pair[0].position.x.min(pair[1].position.x);
                            let right = pair[0].position.x.max(pair[1].position.x);
                            Some(span.map_or((left, right), |(start, end)| {
                                (start.min(left), end.max(right))
                            }))
                        });
                    if let Some((left, right)) = span.filter(|(left, right)| left < right) {
                        paint.draw_clipped(
                            &DrawCommand::Fill {
                                area: Rect {
                                    x: content_rect.x - scroll
                                        + crate::types::fixed::from_textflow(left),
                                    y: content_rect.y
                                        + crate::types::fixed::from_textflow(line.origin().y)
                                        - metrics.ascender
                                        + if underline {
                                            metrics.line_height - Fixed::ONE
                                        } else {
                                            Fixed::ZERO
                                        },
                                    w: crate::types::fixed::from_textflow(right - left),
                                    h: if underline {
                                        Fixed::ONE
                                    } else {
                                        metrics.line_height
                                    },
                                },
                                transform,
                                quad: paint.quad(),
                                color: focus_border_color,
                                radius: Fixed::ZERO,
                                opa: if underline { 255 } else { 80 },
                            },
                            content_clip,
                        );
                    }
                }
            }
        }
    }
    super::text::draw_text_layout(
        paint,
        &layout,
        |font_id| fonts.font(font_id),
        super::text::TextPaint::new(
            Point {
                x: content_rect.x - scroll,
                y: content_rect.y,
            },
            transform,
            &content_clip,
            color,
        ),
    );

    if editing.focused() {
        let blink_on = phase.map(|phase| phase.0).unwrap_or(true);
        if blink_on && let Some(caret) = caret {
            paint.draw_clipped(
                &DrawCommand::Fill {
                    area: Rect {
                        x: content_rect.x + caret - scroll,
                        y: content_rect.y,
                        w: Fixed::ONE,
                        h: metrics.line_height.min(content_rect.h),
                    },
                    transform: paint.transform(),
                    quad: paint.quad(),
                    color: cursor_color,
                    radius: Fixed::ZERO,
                    opa: 255,
                },
                content_clip,
            );
        }
    }
}

#[crate::event]
fn text_input_attach(
    entity: entity!(),
    inputs: query!(TextInput, optional TextEditing),
    commands: commands!(),
) {
    if inputs
        .get(entity)
        .is_some_and(|(_, editing)| editing.is_none())
    {
        commands.insert(entity, TextEditing::default());
    }
}

pub use text_input_render::view;

#[cfg(test)]
mod tests {
    use alloc::{rc::Rc, vec::Vec};

    use super::*;
    use crate::render::font::{
        Font, FontBackend, FontFaceId, FontMetrics, FontProvider, FontToken, GlyphId, RasterGlyph,
    };
    use crate::render::renderer::Renderer;
    use crate::types::Transform;
    use crate::ui::Style;
    use crate::ui::theme::{Theme, WidgetState};
    use crate::ui::view::PaintCtx;
    use crate::ui::widgets::text_editing::geometry::prepare_layout;

    fn focus(world: &mut World, entity: Entity) {
        world.insert_resource(crate::input::event::focus::FocusState {
            focused: Some(entity),
        });
        super::super::text_editing::view().install(|system| system.run_once(world).unwrap());
    }

    struct ProportionalFace;

    impl FontProvider for ProportionalFace {
        fn face_id(&self) -> FontFaceId {
            FontFaceId::new(41)
        }

        fn map_char(&self, ch: char) -> Option<GlyphId> {
            ch.is_ascii().then_some(GlyphId::new(ch as u16))
        }

        fn glyph_advance(&self, glyph: GlyphId, _ppem: u16) -> Option<Fixed> {
            Some(Fixed::from_int(if glyph.value() == u16::from(b'W') {
                10
            } else {
                3
            }))
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
                ascender: Fixed::from_int(9),
                descender: Fixed::from_int(-3),
                line_height: Fixed::from_int(12),
            }
        }
    }

    #[derive(Default)]
    struct RecordingRenderer {
        glyph_runs: Vec<(Point, usize)>,
        fills: Vec<Rect>,
        fill_clips: Vec<Rect>,
        fills_before_glyphs: usize,
    }

    impl Renderer for RecordingRenderer {
        fn route(
            &self,
            _: &crate::render::DrawRequest<'_, '_>,
        ) -> Result<crate::render::RenderRoute, crate::render::RenderError> {
            Ok(crate::render::RenderRoute::Native)
        }

        fn submit(
            &mut self,
            request: &crate::render::DrawRequest<'_, '_>,
        ) -> Result<(), crate::render::RenderError> {
            self.route(request)?;
            match request.command {
                DrawCommand::GlyphRun { pos, glyphs, .. } => {
                    self.fills_before_glyphs = self.fills.len();
                    self.glyph_runs.push((*pos, glyphs.len()));
                }
                DrawCommand::Fill { area, .. } => {
                    self.fills.push(*area);
                    self.fill_clips.push(request.clip);
                }
                _ => {}
            }
            Ok(())
        }

        fn flush(&mut self) {}
    }

    fn input_fixture(
        text: &[u8],
        placeholder: Option<&'static str>,
        width: i32,
    ) -> (World, Entity, Style, Rect) {
        let mut world = World::new();
        world.insert_resource(Theme::default());
        world.insert_resource(Outputs::<TextGeometry>::new_text(
            crate::text::TextLayoutLimits::EMBEDDED,
        ));
        world.insert_resource(CursorBlinkPhase(true));
        let fonts = crate::render::font::default_font_manager();
        fonts.add_static(
            FontToken::Default.cache_key(),
            Font {
                family: "proportional",
                size: 12,
                backend: FontBackend::Custom(Rc::new(ProportionalFace)),
            },
        );
        world.insert_resource(fonts);
        let entity = world.spawn_empty();
        world.insert(entity, TextInput::new());
        world.insert(
            entity,
            TextEditing::default()
                .with_text(core::str::from_utf8(text).unwrap())
                .unwrap(),
        );
        focus(&mut world, entity);
        if let Some(placeholder) = placeholder {
            world.insert(entity, Placeholder(placeholder));
        }
        let style = Style::default();
        world.insert(entity, style.clone());
        let rect = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(width),
            h: Fixed::from_int(20),
        };
        (world, entity, style, rect)
    }

    fn render_input(
        text: &[u8],
        placeholder: Option<&'static str>,
        width: i32,
    ) -> RecordingRenderer {
        render_selected_input(text, placeholder, width, None)
    }

    fn render_selected_input(
        text: &[u8],
        placeholder: Option<&'static str>,
        width: i32,
        selection: Option<super::super::TextSelection>,
    ) -> RecordingRenderer {
        let (mut world, entity, style, rect) = input_fixture(text, placeholder, width);
        if let Some(selection) = selection {
            world
                .get_mut::<TextEditing>(entity)
                .unwrap()
                .set_selection(selection)
                .unwrap();
        }
        let handle = prepare_layout(&world, entity, rect).unwrap().unwrap();
        world.insert(entity, TextEditingLayoutHandle(handle));
        let clip = Rect::new(0, 0, 100, 30);
        let mut renderer = RecordingRenderer::default();
        let mut paint = PaintCtx::new(
            &mut renderer,
            &style,
            &rect,
            &clip,
            Transform::IDENTITY,
            None,
            WidgetState::Enabled,
        );

        view().render().unwrap()(&world, entity, &mut paint);
        renderer
    }

    #[test]
    fn tapped_document_edits_keep_partial_and_full_frames_identical() {
        use crate::input::event::input::{InputEvent, PointerKind};
        use crate::input::event::sim::{SimCommand, SimulatedInput, sim_input_system};
        use crate::prelude::*;
        use crate::surface::FramebufferAccess;
        use crate::ui::widgets::{Text, TextDocument, TextSelection};

        #[crate::compose]
        fn panel(document: model!(TextDocument)) -> (Entity, Entity) {
            ui! {
                Text ("CARET", height: 24, font_size: 9)
            };
            let first = ui! {
                TextInput (
                    width: 120,
                    height: 30,
                    font_size: 10,
                    bg_color: ColorToken::SurfaceVariant
                ) [
                    TextEditing::new(document.clone()),
                ]
            };
            let second = ui! {
                TextInput (
                    width: 120,
                    height: 30,
                    font_size: 10,
                    bg_color: ColorToken::SurfaceVariant
                ) [
                    TextEditing::new(document),
                ]
            };
            (first, second)
        }
        let mut app = crate::app::App::new(crate::surface::framebuf::FramebufSurface::with_format(
            128,
            128,
            crate::render::texture::ColorFormat::RGB565Swapped,
            |_, _| {},
        ));
        app.with_default_widgets().with_default_systems();
        app.add_resource(crate::ecs::MonoClock::new(|| 600_000_000));
        app.add_system(sim_input_system::system());
        let root = app.spawn_root().id();
        let document = app.add_model(TextDocument::new()).finish();
        let (first, _) = app.compose(root, panel(document.clone()));
        app.add_resource(crate::input::event::focus::FocusState {
            focused: Some(first),
        });
        document.set_text("EDIT").unwrap();
        app.world
            .get_mut::<TextEditing>(first)
            .unwrap()
            .set_selection(TextSelection {
                anchor: 4,
                cursor: 4,
            })
            .unwrap();
        app.tick();
        app.render().unwrap();
        let rect = app.world.get::<crate::ui::ComputedRect>(first).unwrap().0;
        for (x, cursor, text) in [
            (rect.x + Fixed::from_int(2), 0, "A"),
            (rect.x + rect.w - Fixed::from_int(2), 5, "Z"),
        ] {
            let y = rect.y + Fixed::from_int(4);
            app.add_resource(SimulatedInput::new(vec![
                SimCommand::at(
                    0,
                    InputEvent::PointerDown {
                        kind: PointerKind::Mouse,
                        id: 0,
                        x,
                        y,
                    },
                ),
                SimCommand::at(
                    0,
                    InputEvent::PointerUp {
                        kind: PointerKind::Mouse,
                        id: 0,
                        x,
                        y,
                    },
                ),
            ]));
            app.tick();
            assert_eq!(
                app.world.get::<TextEditing>(first).unwrap().cursor(),
                cursor
            );
            app.add_resource(SimulatedInput::new(vec![SimCommand::at(
                0,
                TextCommitted::new(text).into(),
            )]));
            app.tick();
            app.render_dirty().unwrap();
            let partial: Vec<_> = (0..128)
                .flat_map(|y| (0..128).map(move |x| (x, y)))
                .map(|(x, y)| app.backend.framebuffer().get_pixel(x, y))
                .collect();
            app.render().unwrap();
            for y in 0..128 {
                for x in 0..128 {
                    assert_eq!(
                        partial[(y * 128 + x) as usize],
                        app.backend.framebuffer().get_pixel(x, y),
                        "pixel ({x}, {y})"
                    );
                }
            }
        }
    }

    #[test]
    fn caret_hit_reuses_painted_positions_and_rejects_stale_layouts() {
        use crate::ui::ComputedRect;

        fn hit(world: &mut World, entity: Entity, point: Point) -> usize {
            let editor = world.get_mut::<TextEditing>(entity).unwrap();
            let end = editor.len();
            editor
                .set_selection(super::super::TextSelection {
                    anchor: end,
                    cursor: end,
                })
                .unwrap();
            crate::input::event::view_input::invoke_payload(
                super::super::text_editing::view(),
                world,
                entity,
                &crate::input::event::gesture::Tap {
                    x: point.x,
                    y: point.y,
                    target: entity,
                },
                Some(1),
            )
            .unwrap();
            world.get::<TextEditing>(entity).unwrap().cursor()
        }

        let (mut world, entity, _, rect) = input_fixture(b"Wi", None, 12);
        world.insert(entity, ComputedRect(rect));
        let handle = prepare_layout(&world, entity, rect).unwrap().unwrap();
        world.insert(entity, TextEditingLayoutHandle(handle));
        // The trailing caret is at 13, and the visible width is 7.
        assert_eq!(hit(&mut world, entity, Point::new(5, 5)), 1);
        assert_eq!(hit(&mut world, entity, Point::new(9, 5)), 2);
        assert_eq!(hit(&mut world, entity, Point::new(-20, 5)), 0);

        let parent = world.spawn(crate::ui::Widget);
        world.insert(parent, ComputedRect(Rect::new(0, 0, 100, 100)));
        world.insert(
            parent,
            crate::input::event::scroll::ScrollOffset {
                x: 3.into(),
                y: 4.into(),
            },
        );
        world.insert(entity, crate::ui::Parent(parent));
        world.insert(
            parent,
            crate::ui::widgets::WidgetTransform(Transform::translate(20.into(), 10.into())),
        );
        assert_eq!(hit(&mut world, entity, Point::new(22, 11)), 1);
        world.insert(
            parent,
            crate::ui::widgets::WidgetTransform3D(crate::types::Transform3D::rotate_y_perspective(
                20.into(),
                400.into(),
            )),
        );
        let (_, _, projection) =
            crate::ui::render_system::widget_render_geometry(&world, entity).unwrap();
        let projected = projection.apply_point(Point::new(2, 1)).unwrap();
        assert_eq!(hit(&mut world, entity, projected), 1);

        world.insert(entity, TextEditing::default().with_text("ii").unwrap());
        assert_eq!(hit(&mut world, entity, projected), 2);
        world.insert(entity, TextEditing::default().with_text("Wi").unwrap());
        world.get_mut::<ComputedRect>(entity).unwrap().0.w = 30.into();
        assert_eq!(hit(&mut world, entity, projected), 2);
        world.get_mut::<ComputedRect>(entity).unwrap().0 = rect;
        world.get_mut::<Style>(entity).unwrap().font_size = Some(15);
        assert_eq!(hit(&mut world, entity, projected), 2);
    }

    #[test]
    fn render_uses_shaped_advances_for_glyphs_and_caret() {
        let renderer = render_input(b"Wi", None, 100);

        assert_eq!(renderer.glyph_runs, vec![(Point::new(2, 2), 2)]);
        assert_eq!(renderer.fills.len(), 1);
        assert_eq!(renderer.fills[0].x, Fixed::from_int(15));
        assert_eq!(renderer.fills[0].h, Fixed::from_int(12));
    }

    #[test]
    fn selection_uses_visual_advances_before_glyphs_in_either_direction() {
        for (anchor, cursor) in [(0, 2), (2, 0)] {
            let renderer = render_selected_input(
                b"Wi",
                None,
                100,
                Some(super::super::TextSelection { anchor, cursor }),
            );
            assert_eq!(renderer.fills_before_glyphs, 1);
            assert_eq!(renderer.fills[0], Rect::new(2, 2, 13, 12));
            assert_eq!(renderer.fills.len(), 2);
        }
        let renderer = render_selected_input(
            b"WWW",
            None,
            20,
            Some(super::super::TextSelection {
                anchor: 1,
                cursor: 3,
            }),
        );
        assert_eq!(renderer.fills_before_glyphs, 1);
        assert_eq!(renderer.fills[0], Rect::new(-3, 2, 20, 12));
        assert_eq!(renderer.fill_clips[0], Rect::new(2, 2, 16, 16));
        assert_eq!(renderer.glyph_runs[0].0.x, Fixed::from_int(-13));
    }

    #[test]
    fn typed_paint_prepares_optional_dependencies_and_preserves_early_returns_without_allocating() {
        struct Trace(usize);
        impl Renderer for Trace {
            fn submit(
                &mut self,
                _: &crate::render::DrawRequest<'_, '_>,
            ) -> Result<(), crate::render::RenderError> {
                self.0 += 1;
                Ok(())
            }
            fn flush(&mut self) {}
        }
        let mut world = World::new();
        world.insert_resource(Theme::default());
        let entity = world.spawn_empty();
        let descriptor = view();
        assert_eq!(descriptor.name(), "TextInput");
        assert_eq!(descriptor.priority(), Some(70));
        assert!(descriptor.auto_attach().is_some());
        assert!(descriptor.inputs().is_none());
        let mut systems = 0;
        let descriptor = descriptor.install(|_| systems += 1);
        assert_eq!(systems, 0);
        let render = descriptor.render().unwrap();
        let style = Style::default();
        world.insert(entity, style.clone());
        let rect = Rect::new(0, 0, 100, 20);
        for stage in 0..3 {
            match stage {
                1 => {
                    world.insert(entity, TextInput::new());
                    world.insert(entity, TextEditing::default());
                }
                2 => world.insert_resource(Outputs::<TextGeometry>::new_text(
                    crate::text::TextLayoutLimits::EMBEDDED,
                )),
                _ => {}
            }
            assert_eq!(prepare_layout(&world, entity, rect), Ok(None));
            let mut trace = Trace(0);
            {
                let mut paint = PaintCtx::new(
                    &mut trace,
                    &style,
                    &rect,
                    &rect,
                    Transform::IDENTITY,
                    None,
                    WidgetState::Enabled,
                );
                assert_eq!(
                    crate::test_support::tracked_allocations(|| {
                        render(&world, entity, &mut paint);
                    }),
                    0
                );
                assert_eq!(paint.error(), None);
            }
            assert_eq!(trace.0, 0);
        }
        world.insert_resource(crate::render::font::default_font_manager());
        world.insert(entity, Placeholder("A"));
        let handle = prepare_layout(&world, entity, rect).unwrap().unwrap();
        world.insert(entity, TextEditingLayoutHandle(handle));
        world.remove_resource::<Theme>();
        let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut trace = Trace(0);
            let mut paint = PaintCtx::new(
                &mut trace,
                &style,
                &rect,
                &rect,
                Transform::IDENTITY,
                None,
                WidgetState::Enabled,
            );
            render(&world, entity, &mut paint);
        }));
        assert!(rejected.is_err(), "a drawable input requires Theme");
        world.insert_resource(Theme::default());
        focus(&mut world, entity);
        let mut trace = Trace(0);
        {
            let mut paint = PaintCtx::new(
                &mut trace,
                &style,
                &rect,
                &rect,
                Transform::IDENTITY,
                None,
                WidgetState::Enabled,
            );
            render(&world, entity, &mut paint);
            assert_eq!(paint.error(), None);
        }
        assert!(trace.0 > 0);
        for _ in 0..2 {
            let mut trace = Trace(0);
            let mut paint = PaintCtx::new(
                &mut trace,
                &style,
                &rect,
                &rect,
                Transform::IDENTITY,
                None,
                WidgetState::Enabled,
            );
            assert_eq!(
                crate::test_support::tracked_allocations(|| render(&world, entity, &mut paint)),
                0
            );
            assert_eq!(paint.error(), None);
        }
    }

    #[test]
    fn first_paint_borrows_prepared_input_layout_without_allocation_or_miss_production() {
        #[derive(Default)]
        struct Trace {
            commands: usize,
            glyphs: usize,
            caret: Option<Rect>,
        }

        impl Renderer for Trace {
            fn submit(
                &mut self,
                request: &crate::render::DrawRequest<'_, '_>,
            ) -> Result<(), crate::render::RenderError> {
                self.commands += 1;
                match request.command {
                    DrawCommand::GlyphRun { glyphs, .. } => self.glyphs += glyphs.len(),
                    DrawCommand::Fill { area, .. } => self.caret = Some(*area),
                    _ => {}
                }
                Ok(())
            }

            fn flush(&mut self) {}
        }

        for (text, placeholder, caret_x) in [
            (&b"Wi"[..], None, Fixed::from_int(15)),
            (&b""[..], Some("Wi"), Fixed::from_int(2)),
        ] {
            let (mut world, entity, style, rect) = input_fixture(text, placeholder, 100);
            if !text.is_empty() {
                world
                    .get_mut::<TextEditing>(entity)
                    .unwrap()
                    .set_selection(super::super::TextSelection {
                        anchor: 0,
                        cursor: text.len(),
                    })
                    .unwrap();
            }
            let render = view().render().unwrap();
            for stage in 0..3 {
                if stage == 1 {
                    let handle = prepare_layout(&world, entity, rect).unwrap().unwrap();
                    world.insert(entity, TextEditingLayoutHandle(handle));
                } else if stage == 2 {
                    world
                        .resource::<Outputs<TextGeometry>>()
                        .unwrap()
                        .trim_text_memory();
                }
                let storage = world.resource::<Outputs<TextGeometry>>().unwrap().storage();
                let retained = world
                    .get::<TextEditingLayoutHandle>(entity)
                    .and_then(|handle| storage.layouts.get(handle.0));
                let mut trace = Trace::default();
                {
                    let mut paint = PaintCtx::new(
                        &mut trace,
                        &style,
                        &rect,
                        &rect,
                        Transform::IDENTITY,
                        None,
                        WidgetState::Enabled,
                    );
                    assert_eq!(
                        crate::test_support::tracked_allocations(|| {
                            render(&world, entity, &mut paint);
                        }),
                        0
                    );
                    assert_eq!(paint.error(), None);
                }
                if let Some(layout) = retained {
                    assert_eq!(layout.glyphs().len(), 2);
                    assert_eq!(trace.glyphs, 2);
                    assert_eq!(trace.caret.unwrap().x, caret_x);
                } else {
                    assert_eq!(trace.commands, 0);
                    assert_eq!(world.has::<TextEditingLayoutHandle>(entity), stage == 2);
                }
            }
        }
    }

    #[test]
    fn render_keeps_the_shaped_caret_inside_a_narrow_input() {
        let renderer = render_input(b"WWW", None, 20);

        assert_eq!(renderer.glyph_runs, vec![(Point::new(-13, 2), 3)]);
        assert_eq!(renderer.fills[0].x, Fixed::from_int(17));
    }

    #[test]
    fn placeholder_uses_the_glyph_layout_without_moving_the_empty_caret() {
        let renderer = render_input(b"", Some("Wi"), 100);

        assert_eq!(renderer.glyph_runs, vec![(Point::new(2, 2), 2)]);
        assert_eq!(renderer.fills[0].x, Fixed::from_int(2));
    }

    #[test]
    fn builder_installs_appearance_and_document_and_preserves_explicit_capability() {
        let mut app = crate::app::App::headless(64, 32);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        let document = app.add_model(super::super::TextDocument::new()).finish();
        document.set_text("shared").unwrap();
        let entity = crate::ui::UiScope::new(&mut app.world, root).spawn((
            TextInput::build()
                .capacity(512)
                .unwrap()
                .placeholder("hint"),
            TextEditing::new(document),
        ));
        assert!(app.world.has::<TextInput>(entity));
        assert!(app.world.has::<crate::ui::Style>(entity));
        assert_eq!(app.world.get::<Placeholder>(entity).unwrap().0, "hint");
        let editor = app.world.get::<TextEditing>(entity).unwrap();
        assert_eq!(editor.capacity(), 32);
        editor.read(|text| assert_eq!(text, "shared"));
    }

    #[test]
    fn bare_appearance_attachment_installs_the_editing_capability() {
        let mut app = crate::app::App::headless(64, 32);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        let entity = crate::ui::UiScope::new(&mut app.world, root).spawn(TextInput::new());
        assert!(app.world.has::<TextEditing>(entity));
        assert!(
            app.world
                .has::<crate::input::event::focus::Focusable>(entity)
        );
        assert!(app.world.get::<Placeholder>(entity).is_none());
    }
}
