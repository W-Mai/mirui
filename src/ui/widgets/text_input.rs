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

use super::text_editing::geometry::{TextEditingLayoutHandle, input_content_rect};

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
    theme: res!(optional crate::ui::Theme),
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
        manager,
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
    let theme =
        theme.expect("App::new must insert Theme; missing means a test fixture skipped App");
    let state = paint.state();
    let rect = paint.rect();
    let text_color = ti.text_color.resolve_in(theme, state);
    let placeholder_color = ti.placeholder_color.resolve_in(theme, state);
    let cursor_color = ti.cursor_color.resolve_in(theme, state);
    let focus_border_color = ti.focus_border_color.resolve_in(theme, state);

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
    let color = if editing.is_empty() {
        placeholder_color
    } else {
        text_color
    };
    let caret = if editing.is_empty() {
        Fixed::ZERO
    } else {
        layout
            .carets()
            .iter()
            .find(|caret| caret.text_offset as usize == editing.cursor())
            .map(|caret| crate::types::fixed::from_textflow(caret.position.x))
            .unwrap_or(Fixed::ZERO)
    };
    let visible_width = (content_rect.w - Fixed::ONE).max(Fixed::ZERO);
    let scroll = (caret - visible_width).max(Fixed::ZERO);
    let transform = paint.transform();
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
        if blink_on {
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
                    self.glyph_runs.push((*pos, glyphs.len()));
                }
                DrawCommand::Fill { area, .. } => self.fills.push(*area),
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
        let (mut world, entity, style, rect) = input_fixture(text, placeholder, width);
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
        drop(paint);
        renderer
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
        let document = app.add_model(super::super::TextDocument::new());
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
