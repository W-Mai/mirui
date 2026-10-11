use crate::ecs::{Entity, World};
use crate::input::event::focus::{FocusState, Focusable, TextEditable};
use crate::input::event::gesture::Tap;
use crate::input::event::input::{
    KEY_BACKSPACE, KEY_DELETE, KEY_END, KEY_HOME, KEY_LEFT, KEY_RETURN, KEY_RIGHT, Key,
    TextCommitted,
};
use crate::render::command::DrawCommand;
use crate::text::TextGeometry;
use crate::types::{Fixed, Point, Rect};
use crate::ui::dirty::Dirty;
use crate::ui::outputs::Outputs;
use crate::ui::theme::{ColorToken, ThemedColor};

use super::text_document::{INLINE_CAPACITY, TextEditError, storage::Storage};
use super::text_editing::{Navigation, TextSelection};

/// Caret on/off, toggled by `cursor_blink_system` every ~500 ms.
#[derive(Default, Clone, Copy)]
pub struct CursorBlinkPhase(pub bool);

pub const TEXT_INPUT_CAP: usize = INLINE_CAPACITY;

crate::input::event::contract::widget_events! {
    TextInput;
    /// The input accepted an insertion or deletion and now contains `len` bytes.
    Changed { len: u32 },
    /// The input handler changed the directed selection's UTF-8 byte offsets.
    SelectionChanged { anchor: u32, cursor: u32 },
    /// Return was accepted without inserting text or changing focus.
    Submitted,
}

/// Single-line UTF-8 text input with explicitly prepared capacity.
#[crate::component]
#[derive(Clone)]
pub struct TextInput {
    buffer: Storage,
    capacity: u32,
    len: u32,
    cursor: u32,
    anchor: u32,
    pub focused: bool,
    pub text_color: ThemedColor,
    pub placeholder_color: ThemedColor,
    pub cursor_color: ThemedColor,
    pub focus_border_color: ThemedColor,
}

impl TextInput {
    pub fn new() -> Self {
        Self {
            buffer: Storage::Inline([0u8; TEXT_INPUT_CAP]),
            capacity: TEXT_INPUT_CAP as u32,
            len: 0,
            cursor: 0,
            anchor: 0,
            focused: false,
            text_color: ThemedColor::Token(ColorToken::OnSurface),
            placeholder_color: ThemedColor::Token(ColorToken::OnSurfaceVariant),
            cursor_color: ThemedColor::Token(ColorToken::OnSurface),
            focus_border_color: ThemedColor::Token(ColorToken::Primary),
        }
    }

    /// Set the byte limit, preparing storage before editing. Existing content and selection are preserved on failure.
    pub fn set_capacity(&mut self, capacity: usize) -> Result<(), TextEditError> {
        if capacity < self.len() {
            return Err(TextEditError::Capacity {
                required: self.len(),
                capacity,
            });
        }
        self.buffer.prepare(capacity)?;
        self.capacity = capacity as u32;
        Ok(())
    }

    pub fn with_capacity(mut self, capacity: usize) -> Result<Self, TextEditError> {
        self.set_capacity(capacity)?;
        Ok(self)
    }

    /// The accepted byte limit, independent of retained allocation size.
    pub fn capacity(&self) -> usize {
        self.capacity as usize
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
        self.buffer.text(self.len())
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn cursor(&self) -> usize {
        self.cursor as usize
    }

    pub fn selection(&self) -> TextSelection {
        TextSelection {
            anchor: self.anchor as usize,
            cursor: self.cursor(),
        }
    }

    /// Set both endpoints atomically. Offsets must be current UTF-8 boundaries.
    pub fn set_selection(&mut self, selection: TextSelection) -> Result<bool, TextEditError> {
        for offset in [selection.anchor, selection.cursor] {
            if !self.as_str().is_char_boundary(offset) {
                return Err(TextEditError::InvalidBoundary { offset });
            }
        }
        let changed = self.selection() != selection;
        self.anchor = selection.anchor as u32;
        self.cursor = selection.cursor as u32;
        Ok(changed)
    }

    /// Replace the selection without allocating. Failure preserves content and selection; the result indicates whether content changed.
    pub fn replace_selection(&mut self, text: &str) -> Result<bool, TextEditError> {
        self.replacement_len(text)?;
        let range = self.selection().range();
        let cursor = range.start + text.len();
        let changed = self
            .buffer
            .replace(&mut self.len, self.capacity as usize, range, text)?;
        self.collapse(cursor as u32);
        Ok(changed)
    }

    fn replacement_len(&self, text: &str) -> Result<usize, TextEditError> {
        if text.chars().any(char::is_control) {
            return Err(TextEditError::ControlCharacter);
        }
        self.buffer
            .replacement_len(self.len(), self.capacity(), &self.selection().range(), text)
    }

    fn collapse(&mut self, cursor: u32) {
        self.cursor = cursor;
        self.anchor = cursor;
    }

    pub fn insert(&mut self, ch: u8) -> bool {
        if !(32..=126).contains(&ch) {
            return false;
        }
        self.insert_char(char::from(ch))
    }

    pub fn insert_char(&mut self, ch: char) -> bool {
        self.replace_selection(ch.encode_utf8(&mut [0; 4]))
            .unwrap_or(false)
    }

    pub fn backspace(&mut self) -> bool {
        if self.anchor != self.cursor {
            return self
                .replace_selection("")
                .expect("deletion fits existing storage");
        }
        if self.cursor == 0 {
            return false;
        }
        let mut pos = self.cursor as usize - 1;
        while pos > 0 && is_utf8_continuation(self.buffer[pos]) {
            pos -= 1;
        }
        self.anchor = pos as u32;
        self.replace_selection("")
            .expect("deletion fits existing storage")
    }

    pub fn delete_forward(&mut self) -> bool {
        if self.anchor != self.cursor {
            return self
                .replace_selection("")
                .expect("deletion fits existing storage");
        }
        if (self.cursor as usize) >= (self.len as usize) {
            return false;
        }
        let pos = self.cursor as usize;
        let end = self.len as usize;
        let mut next = pos + 1;
        while next < end && is_utf8_continuation(self.buffer[next]) {
            next += 1;
        }
        self.anchor = next as u32;
        self.replace_selection("")
            .expect("deletion fits existing storage")
    }

    pub fn move_left(&mut self) {
        self.navigate(Navigation::Left, false);
    }

    pub fn move_right(&mut self) {
        self.navigate(Navigation::Right, false);
    }

    pub fn move_home(&mut self) {
        self.navigate(Navigation::Home, false);
    }

    pub fn move_end(&mut self) {
        self.navigate(Navigation::End, false);
    }

    fn navigate(&mut self, direction: Navigation, extend: bool) {
        let selection = direction.selection(self.as_str(), self.selection(), extend);
        self.anchor = selection.anchor as u32;
        self.cursor = selection.cursor as u32;
    }

    pub fn build() -> TextInputBuilder {
        TextInputBuilder {
            text_input: TextInput::new(),
            style: None,
            placeholder: None,
            properties: (),
            bindings: (),
        }
    }
}

const fn is_utf8_continuation(byte: u8) -> bool {
    byte & 0b1100_0000 == 0b1000_0000
}

impl Default for TextInput {
    fn default() -> Self {
        Self::new()
    }
}

pub struct TextInputBuilder<P = (), B = ()> {
    text_input: TextInput,
    style: Option<crate::ui::Style>,
    placeholder: Option<&'static str>,
    properties: P,
    bindings: B,
}

impl<P, B> TextInputBuilder<P, B> {
    pub fn capacity(mut self, capacity: usize) -> Result<Self, TextEditError> {
        self.text_input.set_capacity(capacity)?;
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
    TextInput,
    text_input,
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

#[crate::component]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TextInputLayoutHandle(pub crate::text::TextLayoutHandle);

pub(crate) fn input_content_rect(rect: Rect) -> Rect {
    let inset = Fixed::from_int(2);
    Rect {
        x: rect.x + inset,
        y: rect.y + inset,
        w: (rect.w - inset * Fixed::from_int(2)).max(Fixed::ZERO),
        h: (rect.h - inset * Fixed::from_int(2)).max(Fixed::ZERO),
    }
}

pub(crate) fn prepare_layout(
    world: &World,
    entity: Entity,
    rect: Rect,
) -> Result<Option<crate::text::TextLayoutHandle>, crate::ui::render_system::TextLayoutFailure> {
    use crate::ui::render_system::{TextLayoutFailure, TextLayoutStage};

    let Some(input) = world.get::<TextInput>(entity) else {
        return Ok(None);
    };
    let Some(style) = world.get::<crate::ui::Style>(entity) else {
        return Ok(None);
    };
    let Some(outputs) = world.resource::<Outputs<TextGeometry>>() else {
        return Ok(None);
    };
    let face_limit = outputs.storage().layouts.limits().fallback_faces;
    let Some(fonts) = crate::render::font::ResolvedFontStack::resolve(
        world.resource::<crate::render::font::FontManager>(),
        &style.font_stack,
        style.font_size,
        face_limit,
    )
    .map_err(|error| {
        TextLayoutFailure::font_stack(entity, TextLayoutStage::Layout, error.required, error.limit)
    })?
    else {
        return Ok(None);
    };
    let metrics = fonts.primary().metrics(fonts.primary().size);
    let content = if input.len == 0 {
        world
            .get::<Placeholder>(entity)
            .map_or("", |placeholder| placeholder.0)
    } else {
        input.as_str()
    };
    let paragraph = super::ParagraphStyle {
        wrap: super::TextWrap::NoWrap,
        max_lines: Some(1),
        ..super::ParagraphStyle::default()
    };
    let request = paragraph.layout_request(content, metrics, Some(input_content_rect(rect).w));
    let font_fingerprint = fonts.layout_fingerprint(None, paragraph.shaping);
    fonts
        .with_typefaces(None, paragraph.shaping, |typefaces| {
            outputs.storage_mut().layouts.layout_cached(
                u64::from(entity.id) | (u64::from(entity.generation) << 32),
                font_fingerprint,
                request,
                typefaces,
            )
        })
        .map(Some)
        .map_err(|error| TextLayoutFailure::text(entity, TextLayoutStage::Layout, error))
}

#[crate::view(
    component = TextInput,
    name = "TextInput",
    invalidation = Layout,
    priority = 70,
    attach = text_input_attach::callback(),
    input = (textinput_tap::callback(), textinput_key::callback(), textinput_committed::callback()),
    systems = [cursor_blink_system::system()]
)]
fn text_input_render(
    // Placeholder remains a dependency of the prepared layout.
    rows: query!(TextInput, TextInputLayoutHandle, optional Placeholder),
    entity: entity!(),
    outputs: res!(optional Outputs<TextGeometry>),
    manager: res!(optional crate::render::font::FontManager),
    theme: res!(optional crate::ui::Theme),
    phase: res!(optional CursorBlinkPhase),
    paint: paint!(),
) {
    let Some((ti, handle, _)) = rows.get(entity) else {
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

    if ti.focused {
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
    let color = if ti.len == 0 {
        placeholder_color
    } else {
        text_color
    };
    let caret = if ti.len == 0 {
        Fixed::ZERO
    } else {
        layout
            .carets()
            .iter()
            .find(|caret| caret.text_offset == ti.cursor)
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

    if ti.focused {
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

/// Flip `CursorBlinkPhase` every 500 ms and Dirty every focused TextInput.
#[crate::system(order = Animation, expect = TextInput)]
pub fn cursor_blink_system(
    clock: res!(optional crate::ecs::MonoClock),
    phase: res!(optional CursorBlinkPhase),
    inputs: query!(Entity, TextInput),
    commands: commands!(),
) {
    let now_ms = match clock {
        Some(clock) => (clock.clock)() / 1_000_000,
        None => return,
    };
    let new_phase = (now_ms / 500) % 2 == 0;
    let prev_phase = phase.map(|phase| phase.0).unwrap_or(!new_phase);
    if new_phase == prev_phase {
        return;
    }
    commands.insert_resource(CursorBlinkPhase(new_phase));
    for (entity, input) in inputs {
        if input.focused {
            commands.insert(entity, Dirty);
        }
    }
}

#[crate::event]
fn textinput_tap(
    _tap: event!(Tap),
    focus: res!(optional FocusState),
    inputs: query!(Entity, TextInput),
    commands: commands!(),
) {
    let focused = focus.and_then(|state| state.focused);
    for (entity, input) in inputs {
        let now = Some(entity) == focused;
        if input.focused != now {
            commands.set::<crate::ui::property::prop::TextInputFocused>(entity, now);
        }
    }
}

#[crate::event]
fn textinput_committed(
    node: entity!(),
    event: event!(TextCommitted),
    inputs: query!(TextInput),
    commands: commands!(),
) -> bool {
    let Some(input) = inputs.get(node) else {
        return false;
    };
    let edit = InputEdit::Replace(event.text());
    if edit.changes(input) {
        commands.apply_event(node, apply_committed_text);
    }
    true
}

fn apply_committed_text(
    world: &mut World,
    node: Entity,
    event: &TextCommitted,
) -> Result<(), crate::ecs::CommandError> {
    apply_input_edit(world, node, InputEdit::Replace(event.text()))
}

#[crate::event]
fn textinput_key(
    node: entity!(),
    event: event!(Key),
    inputs: query!(TextInput),
    commands: commands!(),
) -> bool {
    if !event.pressed {
        return false;
    }
    let Some(input) = inputs.get(node) else {
        return false;
    };
    let extend = event.modifiers.is_some_and(|modifiers| modifiers.shift);
    let edit = match event.code {
        KEY_RETURN => {
            commands.emit(node, Submitted);
            return true;
        }
        KEY_BACKSPACE => InputEdit::Backspace,
        KEY_DELETE => InputEdit::Delete,
        KEY_LEFT => InputEdit::Navigate(Navigation::Left, extend),
        KEY_RIGHT => InputEdit::Navigate(Navigation::Right, extend),
        KEY_HOME => InputEdit::Navigate(Navigation::Home, extend),
        KEY_END => InputEdit::Navigate(Navigation::End, extend),
        _ => return false,
    };
    if edit.changes(input) {
        commands.apply(node, edit, apply_input_edit);
    }
    true
}

#[derive(Clone, Copy)]
enum InputEdit<'text> {
    Replace(&'text str),
    Backspace,
    Delete,
    Navigate(Navigation, bool),
}

impl InputEdit<'_> {
    fn changes(self, input: &TextInput) -> bool {
        let selection = input.selection();
        let selected = selection.anchor != selection.cursor;
        match self {
            Self::Replace(text) => match input.replacement_len(text) {
                Ok(_) => selected || !text.is_empty(),
                Err(error) => {
                    crate::warn!(target: "mirui::text_input", "text edit rejected: {:?}", error);
                    false
                }
            },
            Self::Backspace => selected || selection.cursor > 0,
            Self::Delete => selected || selection.cursor < input.len(),
            Self::Navigate(direction, extend) => {
                direction.selection(input.as_str(), selection, extend) != selection
            }
        }
    }

    fn apply(self, input: &mut TextInput) -> bool {
        match self {
            Self::Replace(text) => input.replace_selection(text).unwrap_or(false),
            Self::Backspace => input.backspace(),
            Self::Delete => input.delete_forward(),
            Self::Navigate(direction, extend) => {
                input.navigate(direction, extend);
                false
            }
        }
    }
}

fn apply_input_edit(
    world: &mut World,
    node: Entity,
    edit: InputEdit<'_>,
) -> Result<(), crate::ecs::CommandError> {
    if !world
        .get::<TextInput>(node)
        .is_some_and(|input| edit.changes(input))
    {
        return Ok(());
    }
    let input = world.get_mut::<TextInput>(node).expect("validated input");
    let previous = (input.anchor, input.cursor);
    let content_changed = edit.apply(input);
    let len = input.len() as u32;
    let selection = SelectionChanged {
        anchor: input.anchor,
        cursor: input.cursor,
    };
    let selection_changed = previous != (selection.anchor, selection.cursor);
    let content_result = if content_changed {
        crate::input::event::emission::submit_if_listened(world, node, Changed { len })
    } else {
        Ok(())
    };
    let selection_result = if selection_changed {
        crate::input::event::emission::submit_if_listened(world, node, selection)
    } else {
        Ok(())
    };
    content_result.and(selection_result)
}

#[crate::event]
fn text_input_attach(
    entity: entity!(),
    inputs: query!(TextInput, optional Focusable, optional TextEditable),
    commands: commands!(),
) {
    let Some((_, focusable, editable)) = inputs.get(entity) else {
        return;
    };
    if focusable.is_none() {
        commands.insert(entity, Focusable);
    }
    if editable.is_none() {
        commands.insert(entity, TextEditable);
    }
}

pub use text_input_render::view;

#[cfg(test)]
#[path = "text_input_edit_tests.rs"]
mod edit_tests;

#[cfg(test)]
#[path = "text_input_event_tests.rs"]
mod event_tests;

#[cfg(test)]
mod tests {
    use crate::input::event::input::InputEvent;
    use alloc::{rc::Rc, vec::Vec};

    use super::*;
    use crate::input::event::{EventCapacity, runtime};
    use crate::render::font::{
        Font, FontBackend, FontFaceId, FontMetrics, FontProvider, FontToken, GlyphId, RasterGlyph,
    };
    use crate::render::renderer::Renderer;
    use crate::types::Transform;
    use crate::ui::Style;
    use crate::ui::theme::{Theme, WidgetState};
    use crate::ui::view::PaintCtx;

    #[test]
    fn focus_sync_reuses_prepared_callback_storage() {
        let mut world = World::new();
        let first = world.spawn(TextInput::new());
        let second = world.spawn(TextInput::new());
        world.insert(first, Dirty);
        world.insert(second, Dirty);
        world.insert(first, crate::ui::dirty::VisualDirty);
        world.insert(second, crate::ui::dirty::VisualDirty);
        world.insert_resource(FocusState::default());
        let target = world.spawn_empty();
        let callback = textinput_tap::callback::<crate::__macro_support::NodeContext>()
            .prepare(
                &mut world,
                crate::input::event::view_input::fixture_capacity(),
            )
            .unwrap();
        for focused in [None, Some(first), Some(second), None] {
            world.insert_resource(FocusState { focused });
            for _ in 0..2 {
                world.remove::<Dirty>(first);
                world.remove::<Dirty>(second);
                assert_eq!(
                    crate::test_support::tracked_allocations(|| {
                        callback
                            .invoke(
                                &mut world,
                                &Tap {
                                    x: Fixed::ZERO,
                                    y: Fixed::ZERO,
                                    target,
                                },
                                target,
                            )
                            .unwrap();
                    }),
                    0
                );
                for entity in [first, second] {
                    assert_eq!(
                        world.get::<TextInput>(entity).unwrap().focused,
                        Some(entity) == focused
                    );
                }
            }
        }
    }

    fn invoke_key(
        world: &mut World,
        node: Entity,
        event: &InputEvent,
    ) -> Result<bool, crate::ecs::CommandError> {
        use crate::input::event::view_input::invoke_payload;
        match event {
            InputEvent::Key {
                code,
                pressed,
                modifiers,
                repeat,
            } => invoke_payload(
                view(),
                world,
                node,
                &Key {
                    code: *code,
                    pressed: *pressed,
                    modifiers: *modifiers,
                    repeat: *repeat,
                },
                None,
            ),
            InputEvent::TextCommitted(text) => invoke_payload(view(), world, node, text, None),
            _ => Ok(false),
        }
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
        world.insert(entity, TextInputLayoutHandle(handle));
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
        assert!(descriptor.inputs().is_some());
        let mut systems = 0;
        let descriptor = descriptor.install(|_| systems += 1);
        assert_eq!(systems, 1);
        let render = descriptor.render().unwrap();
        let style = Style::default();
        world.insert(entity, style.clone());
        let rect = Rect::new(0, 0, 100, 20);
        for stage in 0..3 {
            match stage {
                1 => world.insert(entity, TextInput::new()),
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
        world.insert(entity, TextInputLayoutHandle(handle));
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
        world.get_mut::<TextInput>(entity).unwrap().focused = true;
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
                    world.insert(entity, TextInputLayoutHandle(handle));
                } else if stage == 2 {
                    world
                        .resource::<Outputs<TextGeometry>>()
                        .unwrap()
                        .trim_text_memory();
                }
                let storage = world.resource::<Outputs<TextGeometry>>().unwrap().storage();
                let retained = world
                    .get::<TextInputLayoutHandle>(entity)
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
                    assert_eq!(world.has::<TextInputLayoutHandle>(entity), stage == 2);
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
    fn utf8_editing_keeps_the_cursor_on_scalar_boundaries() {
        let mut ti = TextInput::new();
        assert!(ti.insert_char('A'));
        assert!(ti.insert_char('中'));
        assert!(ti.insert_char('🙂'));
        assert_eq!(ti.as_str(), "A中🙂");
        assert_eq!(ti.cursor, 8);

        ti.move_left();
        assert_eq!(ti.cursor, 4);
        assert!(ti.backspace());
        assert_eq!(ti.as_str(), "A🙂");
        assert_eq!(ti.cursor, 1);

        assert!(ti.delete_forward());
        assert_eq!(ti.as_str(), "A");
        assert_eq!(ti.cursor, 1);
    }

    #[test]
    fn utf8_insert_rejects_a_scalar_that_exceeds_the_byte_capacity() {
        let mut ti = TextInput::new();
        for _ in 0..(TEXT_INPUT_CAP - 1) {
            assert!(ti.insert(b'a'));
        }
        assert!(!ti.insert_char('中'));
        assert_eq!(ti.len as usize, TEXT_INPUT_CAP - 1);
        assert_eq!(ti.as_str().len(), TEXT_INPUT_CAP - 1);
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
    fn builder_inserts_all_and_installs_typed_events_once() {
        let mut world = World::new();
        world.insert_resource(ChangeTrace::default());
        runtime::configure(
            &mut world,
            EventCapacity::new()
                .commands(crate::input::event::view_input::fixture_capacity())
                .notifications::<Changed>(1)
                .flush_limit(1),
        )
        .unwrap();
        let builder_parent = world.spawn_empty();
        let entity = crate::ui::UiScope::new(&mut world, builder_parent).spawn(
            TextInput::build()
                .on_event(TextInput::Changed, record_changed::callback())
                .style(crate::ui::Style::default())
                .placeholder("hi"),
        );
        assert!(world.has::<TextInput>(entity));
        assert!(world.has::<crate::ui::Style>(entity));
        assert!(world.has::<Placeholder>(entity));
        assert!(world.has::<crate::ui::Widget>(entity));
        assert!(
            invoke_key(
                &mut world,
                entity,
                &InputEvent::TextCommitted(crate::input::event::input::TextCommitted::new("a"))
            )
            .unwrap()
        );
        assert_eq!(runtime::flush(&mut world).unwrap(), 1);
        assert_eq!(world.resource::<ChangeTrace>().unwrap().calls, 1);
    }

    #[test]
    fn prepared_large_input_uses_default_notifications_without_hot_allocation() {
        let mut world = World::new();
        world.insert_resource(ChangeTrace::default());
        runtime::configure(&mut world, EventCapacity::default()).unwrap();
        let parent = world.spawn_empty();
        let entity = crate::ui::UiScope::new(&mut world, parent).spawn(
            TextInput::build()
                .capacity(512)
                .unwrap()
                .on_event(TextInput::Changed, record_changed::callback()),
        );
        let callback = textinput_committed::callback::<crate::__macro_support::NodeContext>()
            .prepare(
                &mut world,
                crate::input::event::view_input::fixture_capacity(),
            )
            .unwrap();
        callback
            .invoke(&mut world, &TextCommitted::new("a"), entity)
            .unwrap();
        runtime::flush(&mut world).unwrap();
        *world.resource_mut::<ChangeTrace>().unwrap() = ChangeTrace::default();
        world.get_mut::<TextInput>(entity).unwrap().backspace();
        world
            .get_mut::<TextInput>(entity)
            .unwrap()
            .replace_selection(&"a".repeat(300))
            .unwrap();
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                callback
                    .invoke(&mut world, &TextCommitted::new("中"), entity)
                    .unwrap();
                assert_eq!(runtime::flush(&mut world).unwrap(), 1);
            }),
            0
        );
        let trace = world.resource::<ChangeTrace>().unwrap();
        assert_eq!(trace.calls, 1);
        assert_eq!(trace.lengths[0], 303);
        assert_eq!(trace.current_lengths[0], 303);
        assert_eq!(world.get::<TextInput>(entity).unwrap().cursor(), 303);
    }

    #[test]
    fn bare_builder_initializes_style_without_placeholder() {
        let mut world = World::new();
        let builder_parent = world.spawn_empty();
        let e = crate::ui::UiScope::new(&mut world, builder_parent).spawn(TextInput::build());
        assert!(world.get::<TextInput>(e).is_some());
        assert!(world.get::<crate::ui::Style>(e).is_some());
        assert!(world.get::<Placeholder>(e).is_none());
    }

    #[derive(Default)]
    struct ChangeTrace {
        lengths: [u32; 2],
        current_lengths: [usize; 2],
        calls: usize,
    }

    #[crate::event]
    fn record_changed(
        input: event!(Changed),
        node: entity!(),
        values: query!(TextInput),
        trace: res!(mut ChangeTrace),
    ) {
        trace.lengths[trace.calls] = input.len;
        trace.current_lengths[trace.calls] = values.get(node).unwrap().len();
        trace.calls += 1;
    }

    #[crate::event]
    fn remove_input(node: entity!(), values: query!(TextInput, Dirty), commands: commands!()) {
        assert!(values.get(node).is_some());
        commands.despawn(node);
    }

    fn changed_fixture() -> (World, Entity) {
        let mut world = World::new();
        world.insert_resource(ChangeTrace::default());
        runtime::configure(
            &mut world,
            EventCapacity::new()
                .commands(crate::input::event::view_input::fixture_capacity())
                .notifications::<Changed>(2)
                .flush_limit(2),
        )
        .unwrap();
        let builder_parent = world.spawn_empty();
        let entity = crate::ui::UiScope::new(&mut world, builder_parent)
            .spawn(TextInput::build().on_event(TextInput::Changed, record_changed::callback()));
        (world, entity)
    }

    #[test]
    fn same_text_input_collapses_selection_without_content_notification() {
        let (mut world, entity) = changed_fixture();
        let input = world.get_mut::<TextInput>(entity).unwrap();
        input.replace_selection("abc").unwrap();
        input
            .set_selection(TextSelection {
                anchor: 2,
                cursor: 1,
            })
            .unwrap();
        assert!(
            invoke_key(
                &mut world,
                entity,
                &InputEvent::TextCommitted(crate::input::event::input::TextCommitted::new("b"))
            )
            .unwrap()
        );
        let input = world.get::<TextInput>(entity).unwrap();
        assert_eq!(input.as_str(), "abc");
        assert_eq!(
            input.selection(),
            TextSelection {
                anchor: 2,
                cursor: 2
            }
        );
        assert_eq!(runtime::flush(&mut world).unwrap(), 0);
    }

    #[test]
    fn character_input_replaces_selection_and_delete_removes_selection() {
        let (mut world, entity) = changed_fixture();
        let input = world.get_mut::<TextInput>(entity).unwrap();
        input.replace_selection("A中🙂Z").unwrap();
        input
            .set_selection(TextSelection {
                anchor: 8,
                cursor: 1,
            })
            .unwrap();
        assert!(
            invoke_key(
                &mut world,
                entity,
                &InputEvent::TextCommitted(crate::input::event::input::TextCommitted::new("B"))
            )
            .unwrap()
        );
        assert_eq!(world.get::<TextInput>(entity).unwrap().as_str(), "ABZ");
        world
            .get_mut::<TextInput>(entity)
            .unwrap()
            .set_selection(TextSelection {
                anchor: 0,
                cursor: 3,
            })
            .unwrap();
        assert!(
            invoke_key(
                &mut world,
                entity,
                &InputEvent::Key {
                    code: KEY_DELETE,
                    pressed: true,
                    modifiers: None,
                    repeat: None,
                }
            )
            .unwrap()
        );
        assert!(world.get::<TextInput>(entity).unwrap().is_empty());
        assert_eq!(runtime::flush(&mut world).unwrap(), 2);
        assert_eq!(world.resource::<ChangeTrace>().unwrap().lengths, [3, 0]);
    }

    #[test]
    fn content_mutation_notifies_in_fifo_after_releasing_the_input_borrow() {
        let (mut world, entity) = changed_fixture();
        assert!(
            invoke_key(
                &mut world,
                entity,
                &InputEvent::TextCommitted(crate::input::event::input::TextCommitted::new("a"))
            )
            .unwrap()
        );
        assert!(
            invoke_key(
                &mut world,
                entity,
                &InputEvent::Key {
                    code: KEY_BACKSPACE,
                    pressed: true,
                    modifiers: Some(Default::default()),
                    repeat: Some(false),
                }
            )
            .unwrap()
        );
        assert_eq!(world.resource::<ChangeTrace>().unwrap().calls, 0);
        assert_eq!(runtime::flush(&mut world).unwrap(), 2);
        let trace = world.resource::<ChangeTrace>().unwrap();
        assert_eq!(trace.lengths, [1, 0]);
        assert_eq!(trace.current_lengths, [0, 0]);
        assert_eq!(trace.calls, 2);
    }

    #[test]
    fn notification_capacity_failure_does_not_repeat_or_roll_back_the_edit() {
        let (mut world, entity) = changed_fixture();
        for ch in ['a', 'b'] {
            assert!(
                invoke_key(
                    &mut world,
                    entity,
                    &InputEvent::TextCommitted(crate::input::event::input::TextCommitted::new(
                        ch.to_string()
                    ))
                )
                .unwrap()
            );
        }
        assert!(
            invoke_key(
                &mut world,
                entity,
                &InputEvent::TextCommitted(crate::input::event::input::TextCommitted::new("c"))
            )
            .is_err()
        );
        assert_eq!(world.get::<TextInput>(entity).unwrap().as_str(), "abc");
        runtime::flush(&mut world).unwrap();
        assert_eq!(world.get::<TextInput>(entity).unwrap().as_str(), "abc");
        assert_eq!(world.resource::<ChangeTrace>().unwrap().calls, 2);
    }

    #[test]
    fn changed_callback_can_remove_text_input_after_the_edit() {
        let mut world = World::new();
        runtime::configure(
            &mut world,
            EventCapacity::new()
                .notifications::<Changed>(1)
                .commands(crate::input::event::view_input::fixture_capacity())
                .flush_limit(1),
        )
        .unwrap();
        let builder_parent = world.spawn_empty();
        let entity = crate::ui::UiScope::new(&mut world, builder_parent)
            .spawn(TextInput::build().on_event(TextInput::Changed, remove_input::callback()));
        assert!(
            invoke_key(
                &mut world,
                entity,
                &InputEvent::TextCommitted(crate::input::event::input::TextCommitted::new("a"))
            )
            .unwrap()
        );
        assert!(world.is_alive(entity));
        assert_eq!(runtime::flush(&mut world).unwrap(), 1);
        assert!(!world.is_alive(entity));
    }

    #[test]
    fn caret_motion_and_no_op_edits_are_accepted_without_changed_notifications() {
        let (mut world, entity) = changed_fixture();
        crate::input::event::view_input::fixture_inputs(view(), &mut world);
        world.remove::<Dirty>(entity);
        for code in [
            KEY_LEFT,
            KEY_RIGHT,
            KEY_HOME,
            KEY_END,
            KEY_DELETE,
            KEY_BACKSPACE,
        ] {
            assert!(
                invoke_key(
                    &mut world,
                    entity,
                    &InputEvent::Key {
                        code,
                        pressed: true,
                        modifiers: Some(Default::default()),
                        repeat: Some(false),
                    }
                )
                .unwrap()
            );
        }
        assert_eq!(runtime::flush(&mut world).unwrap(), 0);
        assert_eq!(world.resource::<ChangeTrace>().unwrap().calls, 0);
        assert!(!world.has::<Dirty>(entity));
    }

    #[test]
    fn unsubscribed_editing_does_not_require_notification_storage() {
        let mut world = World::new();
        let builder_parent = world.spawn_empty();
        let entity = crate::ui::UiScope::new(&mut world, builder_parent).spawn(TextInput::build());
        view().auto_attach().unwrap().invoke(&mut world, entity);
        assert!(world.has::<Focusable>(entity));
        assert!(world.has::<TextEditable>(entity));
        assert!(view().inputs().is_some());
        assert!(
            invoke_key(
                &mut world,
                entity,
                &InputEvent::TextCommitted(crate::input::event::input::TextCommitted::new("a"))
            )
            .unwrap()
        );
        assert_eq!(world.get::<TextInput>(entity).unwrap().as_str(), "a");
        assert_eq!(runtime::flush(&mut world).unwrap(), 0);
    }
}
