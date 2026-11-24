use alloc::vec::Vec;

use crate::ecs::{Entity, World};
use crate::render::font::{FontStack, FontToken};
use crate::ui::theme::{ColorToken, ThemedColor};
use crate::ui::{Children, Parent, Style};

#[cfg(test)]
mod tests;

pub(crate) const FONT_STACK: u8 = 1;
pub(crate) const FONT_SIZE: u8 = 1 << 1;
pub(crate) const TEXT_COLOR: u8 = 1 << 2;
const ALL_FIELDS: u8 = FONT_STACK | FONT_SIZE | TEXT_COLOR;

pub(crate) fn metrics_changed(fields: u8) -> bool {
    fields & (FONT_STACK | FONT_SIZE) != 0
}

/// Last published declarations of containers that supply inherited values.
#[derive(Default)]
pub(crate) struct TypographyDeclarations(Vec<(Entity, PublishedTypography)>);

impl TypographyDeclarations {
    pub(crate) fn update(&mut self, world: &mut World, root: Entity, moved: bool) {
        if moved {
            if let Some(parent) = world.get::<Parent>(root).map(|parent| parent.0) {
                self.update(world, parent, false);
            }
            if world
                .resource::<crate::ui::render_system::LayoutSnapshot>()
                .is_none()
            {
                return;
            }
        }
        let fields = if moved {
            inherited_fields(world, root, ALL_FIELDS)
        } else {
            self.publish_declaration(world, root)
        };
        if fields == 0 || world.get::<crate::ui::ComputedRect>(root).is_none() {
            return;
        }
        if moved {
            invalidate(world, root, fields);
        }
        invalidate_children(world, root, fields);
    }

    fn publish_declaration(&mut self, world: &World, node: Entity) -> u8 {
        let style = world.get::<Style>(node).filter(|style| {
            Typography::declared_style(style)
                && world
                    .get::<Children>(node)
                    .is_some_and(|children| !children.0.is_empty())
        });
        let position = self
            .0
            .binary_search_by_key(&(node.id, node.generation), |(entity, _)| {
                (entity.id, entity.generation)
            });
        match (position, style) {
            (Err(_), None) => 0,
            (Err(index), Some(next)) => {
                let mut published = PublishedTypography::default();
                let changed = published.update(Some(next));
                self.0.insert(index, (node, published));
                changed
            }
            (Ok(index), None) => self.0.remove(index).1.update(None),
            (Ok(index), Some(next)) => self.0[index].1.update(Some(next)),
        }
    }
}

#[derive(Default)]
struct PublishedTypography {
    font_stack: Option<FontStack>,
    font_size: FontSize,
    text_color: Option<ThemedColor>,
}

impl PublishedTypography {
    fn update(&mut self, style: Option<&Style>) -> u8 {
        let font_stack = style.and_then(|style| style.font_stack.as_ref());
        let font_size = style.map_or(FontSize::Inherit, |style| style.font_size);
        let text_color = style.and_then(|style| style.text_color);
        let mut changed = 0;
        if self.font_stack.as_ref() != font_stack {
            self.font_stack = font_stack.cloned();
            changed |= FONT_STACK;
        }
        if self.font_size != font_size {
            self.font_size = font_size;
            changed |= FONT_SIZE;
        }
        if self.text_color != text_color {
            self.text_color = text_color;
            changed |= TEXT_COLOR;
        }
        changed
    }
}

fn inherited_fields(world: &World, entity: Entity, mut fields: u8) -> u8 {
    if let Some(style) = world.get::<Style>(entity) {
        if style.font_stack.is_some() {
            fields &= !FONT_STACK;
        }
        if style.font_size != FontSize::Inherit {
            fields &= !FONT_SIZE;
        }
        if style.text_color.is_some() {
            fields &= !TEXT_COLOR;
        }
    }
    fields
}

fn invalidate(world: &mut World, entity: Entity, fields: u8) {
    if metrics_changed(fields) {
        world.invalidate(entity);
    } else if fields & TEXT_COLOR != 0 {
        world.invalidate_visual(entity);
    }
}

fn invalidate_children(world: &mut World, root: Entity, fields: u8) {
    if fields == 0 {
        return;
    }
    let count = world
        .get::<Children>(root)
        .map_or(0, |children| children.0.len());
    for index in 0..count {
        let child = world
            .get::<Children>(root)
            .expect("stable children during invalidation")
            .0[index];
        if !world.is_alive(child) {
            continue;
        }
        let fields = inherited_fields(world, child, fields);
        invalidate(world, child, fields);
        invalidate_children(world, child, fields);
    }
}

/// How a widget declares its font size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FontSize {
    /// Use the closest ancestor's declaration.
    #[default]
    Inherit,
    /// Use the selected font's native size.
    Native,
    /// Use a size in logical pixels.
    Pixels(u16),
}

impl FontSize {
    pub const fn pixels(size: u16) -> Self {
        Self::Pixels(if size == 0 { 1 } else { size })
    }

    const fn resolve(self) -> Option<u16> {
        match self {
            Self::Pixels(size) => Some(if size == 0 { 1 } else { size }),
            Self::Inherit | Self::Native => None,
        }
    }
}

impl From<u16> for FontSize {
    fn from(size: u16) -> Self {
        Self::pixels(size)
    }
}

impl From<i32> for FontSize {
    fn from(size: i32) -> Self {
        Self::pixels(size.clamp(1, i32::from(u16::MAX)) as u16)
    }
}

/// Effective text styling for measurement and paint.
#[derive(Clone, Copy)]
pub struct Typography<'a> {
    pub font_stack: &'a FontStack,
    pub font_size: Option<u16>,
    pub text_color: ThemedColor,
}

static DEFAULT_FONTS: FontStack = FontStack::new(FontToken::Default);

impl<'a> Typography<'a> {
    pub(crate) fn local(style: &'a Style) -> Self {
        Self::with_parent(style, None)
    }

    pub(crate) fn with_parent(style: &'a Style, parent: Option<&Self>) -> Self {
        Self {
            font_stack: style
                .font_stack
                .as_ref()
                .unwrap_or_else(|| parent.map_or(&DEFAULT_FONTS, |parent| parent.font_stack)),
            font_size: match style.font_size {
                FontSize::Inherit => parent.and_then(|parent| parent.font_size),
                own => own.resolve(),
            },
            text_color: style.text_color.unwrap_or_else(|| {
                parent.map_or(ThemedColor::Token(ColorToken::OnSurface), |parent| {
                    parent.text_color
                })
            }),
        }
    }

    pub(crate) fn declared_style(style: &Style) -> bool {
        style.font_stack.is_some()
            || style.font_size != FontSize::Inherit
            || style.text_color.is_some()
    }

    pub(crate) fn inherited(world: &'a World, entity: Entity, style: &'a Style) -> Self {
        let parents = world.storage::<Parent>();
        let styles = world.storage::<Style>();
        let ancestors = core::iter::successors(Some(entity), |child| Some(parents?.get(*child)?.0))
            .skip(1)
            .filter_map(|ancestor| styles?.get(ancestor));
        Self::resolve(style, ancestors)
    }

    fn resolve(style: &'a Style, ancestors: impl Iterator<Item = &'a Style>) -> Self {
        let mut font_stack = style.font_stack.as_ref();
        let mut font_size = style.font_size;
        let mut text_color = style.text_color;
        if font_stack.is_none() || font_size == FontSize::Inherit || text_color.is_none() {
            for style in ancestors {
                font_stack = font_stack.or(style.font_stack.as_ref());
                if font_size == FontSize::Inherit {
                    font_size = style.font_size;
                }
                text_color = text_color.or(style.text_color);
                if font_stack.is_some() && font_size != FontSize::Inherit && text_color.is_some() {
                    break;
                }
            }
        }
        Self {
            font_stack: font_stack.unwrap_or(&DEFAULT_FONTS),
            font_size: font_size.resolve(),
            text_color: text_color.unwrap_or(ThemedColor::Token(ColorToken::OnSurface)),
        }
    }
}
