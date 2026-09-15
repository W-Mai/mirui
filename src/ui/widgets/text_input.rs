use crate::ecs::{Entity, World};
use crate::input::event::focus::{FocusState, Focusable, KeyHandler};
use crate::input::event::gesture::GestureEvent;
use crate::input::event::input::{
    InputEvent, KEY_BACKSPACE, KEY_DELETE, KEY_END, KEY_HOME, KEY_LEFT, KEY_RIGHT,
};
use crate::render::command::DrawCommand;
use crate::render::renderer::Renderer;
use crate::types::{Fixed, Point, Rect};
use crate::ui::dirty::Dirty;
use crate::ui::theme::{ColorToken, ThemedColor};
use crate::ui::view::{View, ViewCtx};

/// Caret on/off, toggled by `cursor_blink_system` every ~500 ms.
#[derive(Default, Clone, Copy)]
pub struct CursorBlinkPhase(pub bool);

pub const TEXT_INPUT_CAP: usize = 32;

/// Single-line ASCII text input with a fixed-capacity buffer.
///
/// `buffer[..len]` are the live characters and `cursor` is the insertion
/// point in `0..=len`.
#[derive(crate::Component)]
pub struct TextInput {
    pub buffer: [u8; TEXT_INPUT_CAP],
    pub len: u8,
    pub cursor: u8,
    pub focused: bool,
    pub text_color: ThemedColor,
    pub placeholder_color: ThemedColor,
    pub cursor_color: ThemedColor,
    pub focus_border_color: ThemedColor,
}

impl TextInput {
    pub fn new() -> Self {
        Self {
            buffer: [0u8; TEXT_INPUT_CAP],
            len: 0,
            cursor: 0,
            focused: false,
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

    pub fn as_str(&self) -> &str {
        // Buffer only ever holds ASCII (filtered by the key handler) so
        // utf8 validation is a runtime no-op; we still go through the
        // checked API to stay sound.
        core::str::from_utf8(&self.buffer[..self.len as usize]).unwrap_or("")
    }

    pub fn insert(&mut self, ch: u8) -> bool {
        if !(32..=126).contains(&ch) {
            return false;
        }
        if self.len as usize >= TEXT_INPUT_CAP {
            return false;
        }
        let pos = self.cursor as usize;
        let end = self.len as usize;
        if pos > end {
            return false;
        }
        // Shift right.
        let mut i = end;
        while i > pos {
            self.buffer[i] = self.buffer[i - 1];
            i -= 1;
        }
        self.buffer[pos] = ch;
        self.len += 1;
        self.cursor += 1;
        true
    }

    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let pos = self.cursor as usize - 1;
        let end = self.len as usize;
        let mut i = pos;
        while i + 1 < end {
            self.buffer[i] = self.buffer[i + 1];
            i += 1;
        }
        self.len -= 1;
        self.cursor -= 1;
        true
    }

    pub fn delete_forward(&mut self) -> bool {
        if (self.cursor as usize) >= (self.len as usize) {
            return false;
        }
        let pos = self.cursor as usize;
        let end = self.len as usize;
        let mut i = pos;
        while i + 1 < end {
            self.buffer[i] = self.buffer[i + 1];
            i += 1;
        }
        self.len -= 1;
        true
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_right(&mut self) {
        if (self.cursor as usize) < (self.len as usize) {
            self.cursor += 1;
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.len;
    }

    pub fn build() -> TextInputBuilder {
        TextInputBuilder {
            text_input: TextInput::new(),
            style: None,
            placeholder: None,
        }
    }
}

impl Default for TextInput {
    fn default() -> Self {
        Self::new()
    }
}

pub struct TextInputBuilder {
    text_input: TextInput,
    style: Option<crate::ui::Style>,
    placeholder: Option<&'static str>,
}

impl TextInputBuilder {
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

    pub fn spawn(self, world: &mut World) -> Entity {
        world.spawn(self)
    }
}

impl crate::ecs::IntoBundle for TextInputBuilder {
    fn spawn_into(self, world: &mut World, entity: Entity) {
        world.insert(entity, self.text_input);
        if let Some(style) = self.style {
            world.insert(entity, style);
        }
        if let Some(ph) = self.placeholder {
            world.insert(entity, Placeholder(ph));
        }
    }
}

/// Optional placeholder text rendered when the buffer is empty. Stored
/// as a separate component so the common case (no placeholder) doesn't
/// pay 32 extra bytes inside `TextInput`.
pub struct Placeholder(pub &'static str);

fn text_input_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(ti) = world.get::<TextInput>(entity) else {
        return;
    };
    let Some(resource) = world.resource::<crate::text::layout::TextLayoutResource>() else {
        return;
    };
    let face_limit = resource.borrow().limits().fallback_faces;
    let Ok(Some(fonts)) = crate::render::font::ResolvedFontStack::resolve(
        world,
        &ctx.style.font_stack,
        ctx.style.font_size,
        face_limit,
    ) else {
        return;
    };
    let metrics = fonts.primary().metrics(fonts.primary().size);
    let theme = ctx.theme(world);
    let text_color = ti.text_color.resolve_in(theme, ctx.state);
    let placeholder_color = ti.placeholder_color.resolve_in(theme, ctx.state);
    let cursor_color = ti.cursor_color.resolve_in(theme, ctx.state);
    let focus_border_color = ti.focus_border_color.resolve_in(theme, ctx.state);

    if ti.focused {
        ctx.draw(
            renderer,
            &DrawCommand::Border {
                area: *rect,
                transform: ctx.transform,
                quad: ctx.quad,
                color: focus_border_color,
                width: Fixed::ONE,
                radius: Fixed::ZERO,
                opa: 255,
            },
            ctx.clip,
        );
    }

    let inset = Fixed::from_int(2);
    let content_rect = Rect {
        x: rect.x + inset,
        y: rect.y + inset,
        w: (rect.w - inset * Fixed::from_int(2)).max(Fixed::ZERO),
        h: (rect.h - inset * Fixed::from_int(2)).max(Fixed::ZERO),
    };
    let Some(content_clip) = ctx.clip.intersect(&content_rect) else {
        return;
    };
    let (content, color) = if ti.len == 0 {
        (
            world
                .get::<Placeholder>(entity)
                .map(|placeholder| placeholder.0)
                .unwrap_or(""),
            placeholder_color,
        )
    } else {
        (ti.as_str(), text_color)
    };
    let paragraph = super::ParagraphStyle {
        wrap: super::TextWrap::NoWrap,
        max_lines: Some(1),
        ..super::ParagraphStyle::default()
    };
    let request = paragraph.layout_request(content, metrics, Some(content_rect.w));
    let font_fingerprint = fonts.layout_fingerprint(None, paragraph.shaping);
    let Ok(handle) = fonts.with_typefaces(None, paragraph.shaping, |typefaces| {
        resource.borrow_mut().layout_cached(
            u64::from(entity.id) | (u64::from(entity.generation) << 32),
            font_fingerprint,
            request,
            typefaces,
        )
    }) else {
        return;
    };
    let cache = resource.borrow();
    let Some(layout) = cache.get(handle) else {
        return;
    };
    let caret = if ti.len == 0 {
        Fixed::ZERO
    } else {
        layout
            .carets()
            .iter()
            .find(|caret| caret.text_offset == u32::from(ti.cursor))
            .map(|caret| crate::types::fixed::from_textflow(caret.position.x))
            .unwrap_or(Fixed::ZERO)
    };
    let visible_width = (content_rect.w - Fixed::ONE).max(Fixed::ZERO);
    let scroll = (caret - visible_width).max(Fixed::ZERO);
    ctx.record(super::text::draw_text_layout(
        renderer,
        &layout,
        |font_id| fonts.font(font_id),
        super::text::TextPaint::new(
            Point {
                x: content_rect.x - scroll,
                y: content_rect.y,
            },
            ctx.transform,
            &content_clip,
            color,
        ),
    ));

    if ti.focused {
        let blink_on = world
            .resource::<CursorBlinkPhase>()
            .map(|p| p.0)
            .unwrap_or(true);
        if blink_on {
            ctx.draw(
                renderer,
                &DrawCommand::Fill {
                    area: Rect {
                        x: content_rect.x + caret - scroll,
                        y: content_rect.y,
                        w: Fixed::ONE,
                        h: metrics.line_height.min(content_rect.h),
                    },
                    transform: ctx.transform,
                    quad: ctx.quad,
                    color: cursor_color,
                    radius: Fixed::ZERO,
                    opa: 255,
                },
                &content_clip,
            );
        }
    }
}

/// Flip `CursorBlinkPhase` every 500 ms and Dirty every focused TextInput.
#[crate::system(order = ANIMATION, expect = TextInput)]
pub fn cursor_blink_system(world: &mut World) {
    let now_ms = match world.resource::<crate::ecs::MonoClock>() {
        Some(c) => (c.clock)() / 1_000_000,
        None => return,
    };
    let new_phase = (now_ms / 500) % 2 == 0;
    let prev_phase = world
        .resource::<CursorBlinkPhase>()
        .map(|p| p.0)
        .unwrap_or(!new_phase);
    if new_phase == prev_phase {
        return;
    }
    world.insert_resource(CursorBlinkPhase(new_phase));
    let entities: alloc::vec::Vec<_> = world.query::<TextInput>().collect();
    for e in entities {
        if world
            .get::<TextInput>(e)
            .map(|t| t.focused)
            .unwrap_or(false)
        {
            world.insert(e, Dirty);
        }
    }
}

/// Focus is actually set by `focus_on_tap` in `App::run`; this
/// just mirrors `FocusState` onto `TextInput.focused` for the
/// renderer's fast read.
fn textinput_gesture_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    if let GestureEvent::Tap { .. } = event {
        sync_textinput_focus(world);
        world.insert(entity, Dirty);
        return true;
    }
    false
}

/// Copy `FocusState.focused` onto each TextInput's `focused` field.
fn sync_textinput_focus(world: &mut World) {
    let focused = world.resource::<FocusState>().and_then(|fs| fs.focused);
    let entities: alloc::vec::Vec<_> = world.query::<TextInput>().collect();
    for e in entities {
        let want = Some(e) == focused;
        let current = world
            .get::<TextInput>(e)
            .map(|t| t.focused)
            .unwrap_or(false);
        if want != current {
            if let Some(ti) = world.get_mut::<TextInput>(e) {
                ti.focused = want;
            }
            world.insert(e, Dirty);
        }
    }
}

fn textinput_key_handler(world: &mut World, entity: Entity, event: &InputEvent) -> bool {
    let Some(ti) = world.get_mut::<TextInput>(entity) else {
        return false;
    };
    let mut changed = false;
    match event {
        InputEvent::CharInput { ch } => {
            if (*ch as u32) < 128 {
                changed |= ti.insert(*ch as u8);
            }
        }
        InputEvent::Key { code, pressed } if *pressed => match *code {
            KEY_BACKSPACE => changed |= ti.backspace(),
            KEY_DELETE => changed |= ti.delete_forward(),
            KEY_LEFT => {
                ti.move_left();
                changed = true;
            }
            KEY_RIGHT => {
                ti.move_right();
                changed = true;
            }
            KEY_HOME => {
                ti.move_home();
                changed = true;
            }
            KEY_END => {
                ti.move_end();
                changed = true;
            }
            _ => return false,
        },
        _ => return false,
    }
    if changed {
        world.insert(entity, Dirty);
    }
    true
}

fn text_input_attach(world: &mut World, entity: Entity) {
    if world.get::<TextInput>(entity).is_none() {
        return;
    }
    if world.get::<Focusable>(entity).is_none() {
        world.insert(entity, Focusable);
    }
    if world.get::<KeyHandler>(entity).is_none() {
        world.insert(
            entity,
            KeyHandler {
                on_key: textinput_key_handler,
            },
        );
    }
}

pub fn view() -> View {
    View::new("TextInput", 70, text_input_render)
        .with_filter::<TextInput>()
        .with_attach(text_input_attach)
        .with_internal_gesture(textinput_gesture_handler)
        .with_systems(const { &[cursor_blink_system::system()] })
}

#[cfg(test)]
mod tests {
    use alloc::{rc::Rc, vec::Vec};

    use super::*;
    use crate::render::font::{
        Font, FontBackend, FontFaceId, FontMetrics, FontProvider, FontToken, GlyphId, RasterGlyph,
    };
    use crate::types::Transform;
    use crate::ui::Style;
    use crate::ui::theme::{Theme, WidgetState};

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

    fn render_input(
        text: &[u8],
        placeholder: Option<&'static str>,
        width: i32,
    ) -> RecordingRenderer {
        let mut world = World::new();
        world.insert_resource(Theme::default());
        world.insert_resource(crate::text::layout::TextLayoutResource::new(
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
        let mut input = TextInput::new();
        input.focused = true;
        for byte in text {
            assert!(input.insert(*byte));
        }
        world.insert(entity, input);
        if let Some(placeholder) = placeholder {
            world.insert(entity, Placeholder(placeholder));
        }
        let style = Style::default();
        let clip = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(100),
            h: Fixed::from_int(30),
        };
        let rect = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(width),
            h: Fixed::from_int(20),
        };
        let mut ctx = ViewCtx {
            style: &style,
            transform: Transform::IDENTITY,
            quad: None,
            clip: &clip,
            bg_handled: false,
            state: WidgetState::Enabled,
            error: None,
        };
        let mut renderer = RecordingRenderer::default();

        text_input_render(&mut renderer, &world, entity, &rect, &mut ctx);
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
    fn insert_then_backspace() {
        let mut ti = TextInput::new();
        for ch in b"hello".iter() {
            assert!(ti.insert(*ch));
        }
        assert_eq!(ti.as_str(), "hello");
        assert_eq!(ti.cursor, 5);
        assert!(ti.backspace());
        assert!(ti.backspace());
        assert_eq!(ti.as_str(), "hel");
        assert_eq!(ti.cursor, 3);
    }

    #[test]
    fn arrow_keys_navigate() {
        let mut ti = TextInput::new();
        for ch in b"hello".iter() {
            ti.insert(*ch);
        }
        ti.move_left();
        ti.move_left();
        assert_eq!(ti.cursor, 3);
        ti.insert(b'X');
        assert_eq!(ti.as_str(), "helXlo");
        assert_eq!(ti.cursor, 4);
    }

    #[test]
    fn home_end() {
        let mut ti = TextInput::new();
        for ch in b"hi".iter() {
            ti.insert(*ch);
        }
        ti.move_home();
        assert_eq!(ti.cursor, 0);
        ti.move_end();
        assert_eq!(ti.cursor, 2);
    }

    #[test]
    fn delete_forward_removes_at_cursor() {
        let mut ti = TextInput::new();
        for ch in b"abc".iter() {
            ti.insert(*ch);
        }
        ti.move_home();
        assert!(ti.delete_forward());
        assert_eq!(ti.as_str(), "bc");
        assert_eq!(ti.cursor, 0);
    }

    #[test]
    fn rejects_non_printable_ascii() {
        let mut ti = TextInput::new();
        assert!(!ti.insert(0x07)); // bell
        assert!(!ti.insert(0xFF));
        assert_eq!(ti.len, 0);
    }

    #[test]
    fn full_buffer_rejects() {
        let mut ti = TextInput::new();
        for _ in 0..TEXT_INPUT_CAP {
            assert!(ti.insert(b'a'));
        }
        assert!(!ti.insert(b'b'));
        assert_eq!(ti.len as usize, TEXT_INPUT_CAP);
    }

    #[test]
    fn builder_inserts_all() {
        let mut world = World::new();
        let e = TextInput::build()
            .style(crate::ui::Style::default())
            .placeholder("hi")
            .spawn(&mut world);
        assert!(world.get::<TextInput>(e).is_some());
        assert!(world.get::<crate::ui::Style>(e).is_some());
        assert!(world.get::<Placeholder>(e).is_some());
        assert!(world.get::<crate::ui::Widget>(e).is_some());
    }

    #[test]
    fn bare_builder_omits_optionals() {
        let mut world = World::new();
        let e = TextInput::build().spawn(&mut world);
        assert!(world.get::<TextInput>(e).is_some());
        assert!(world.get::<crate::ui::Style>(e).is_none());
        assert!(world.get::<Placeholder>(e).is_none());
    }
}
