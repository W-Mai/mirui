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
    pub placeholder_color: ThemedColor,
    pub cursor_color: ThemedColor,
    pub focus_border_color: ThemedColor,
    /// Rows of text shown at once. One is a field, a single line that scrolls
    /// sideways and sends `Submitted` on Return; more is a text area, which
    /// wraps, takes a line break on Return and scrolls up and down.
    pub lines: u8,
}

impl TextInput {
    pub fn new() -> Self {
        Self {
            placeholder_color: ThemedColor::Token(ColorToken::OnSurfaceVariant),
            cursor_color: ThemedColor::Token(ColorToken::OnSurface),
            focus_border_color: ThemedColor::Token(ColorToken::Primary),
            lines: 1,
        }
    }

    pub fn with_lines(mut self, lines: u8) -> Self {
        self.lines = lines.max(1);
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

    pub fn font_size(mut self, size: impl Into<crate::ui::FontSize>) -> Self {
        self.style.get_or_insert_default().set_font_size(size);
        self
    }

    pub fn text_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.style.get_or_insert_default().set_text_color(color);
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

    pub fn lines(mut self, lines: u8) -> Self {
        self.text_input.lines = lines.max(1);
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
    Field, LineAlign, TextEditingLayoutHandle, input_content_rect, viewport,
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
    let typography = paint.typography();
    let Ok(Some(fonts)) = crate::render::font::ResolvedFontStack::resolve(
        manager.as_deref(),
        typography.font_stack,
        typography.font_size,
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
    let text_color = typography.text_color.resolve_in(&theme, state);
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
            radius: paint.style().border_radius,
            opa: 255,
        });
    }

    let rows = ti.lines.max(1);
    let field = Field {
        content: input_content_rect(
            rect,
            paint.style().border_radius,
            metrics.line_height * i32::from(rows),
            paint.style().layout.padding,
            if rows > 1 {
                LineAlign::Top
            } else {
                LineAlign::Middle
            },
        ),
        line_height: metrics.line_height,
        rows,
    };
    let content_rect = field.content;
    let content_bounds = paint.transform().apply_rect_bbox(content_rect);
    let Some(content_clip) = paint.clip().intersect(&content_bounds) else {
        return;
    };
    let display_empty = editing.read_display(str::is_empty);
    let color = if display_empty {
        placeholder_color
    } else {
        text_color
    };
    let view = viewport(&layout, editing, &field);
    let (caret, scroll) = (view.caret, view.scroll_x);
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
                                    y: content_rect.y - view.scroll_y
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
                y: content_rect.y - view.scroll_y,
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
                        y: content_rect.y + metrics.line_height * view.line as i32 - view.scroll_y,
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
#[path = "../../../tests/unit/ui/widgets/text_input/tests.rs"]
mod tests;
