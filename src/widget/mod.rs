pub mod builder;
pub mod dirty;
pub mod render_system;
pub mod state;
pub mod style_view;
pub mod theme;
pub mod view;
pub mod visibility;

pub use state::{InteractionState, UserState};
pub use theme::{ColorToken, Theme, ThemedColor};
pub use view::{View, ViewRegistry};
pub use visibility::{Hidden, IgnoreHitTest};

use alloc::vec::Vec;

use crate::layout::LayoutStyle;
use crate::types::Fixed;

pub struct Widget;

/// World resource cached by `App::set_root` so handlers and systems can
/// reach the active root without an `App` reference.
#[derive(Clone, Copy)]
pub struct WidgetRoot(pub crate::ecs::Entity);

#[derive(Clone, Debug)]
pub struct Style {
    pub bg_color: Option<ThemedColor>,
    pub border_color: Option<ThemedColor>,
    pub border_width: Fixed,
    pub border_radius: Fixed,
    /// Always present; for transparent text set alpha on the resolved colour.
    pub text_color: ThemedColor,
    pub layout: LayoutStyle,
    pub clip_children: bool,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            bg_color: None,
            border_color: None,
            border_width: Fixed::ZERO,
            border_radius: Fixed::ZERO,
            text_color: ThemedColor::Token(ColorToken::OnSurface),
            layout: LayoutStyle::default(),
            clip_children: false,
        }
    }
}

impl Style {
    pub fn set_bg_color(&mut self, color: impl Into<ThemedColor>) -> &mut Self {
        self.bg_color = Some(color.into());
        self
    }

    pub fn clear_bg_color(&mut self) -> &mut Self {
        self.bg_color = None;
        self
    }

    pub fn set_border_color(&mut self, color: impl Into<ThemedColor>) -> &mut Self {
        self.border_color = Some(color.into());
        self
    }

    pub fn clear_border_color(&mut self) -> &mut Self {
        self.border_color = None;
        self
    }

    pub fn set_text_color(&mut self, color: impl Into<ThemedColor>) -> &mut Self {
        self.text_color = color.into();
        self
    }

    /// Style for an entity that lives at an explicit pixel rect, ignored
    /// by flex flow. Useful for overlays, popovers, drag-ghost layers.
    pub fn absolute_at(rect: crate::types::Rect) -> Self {
        Self {
            layout: LayoutStyle {
                position: crate::layout::Position::Absolute,
                left: crate::types::Dimension::Px(rect.x),
                top: crate::types::Dimension::Px(rect.y),
                width: crate::types::Dimension::Px(rect.w),
                height: crate::types::Dimension::Px(rect.h),
                ..LayoutStyle::default()
            },
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod style_tests {
    use super::*;
    use crate::types::Color;

    #[test]
    fn default_text_color_tracks_on_surface() {
        let s = Style::default();
        assert_eq!(s.text_color, ThemedColor::Token(ColorToken::OnSurface));
    }

    #[test]
    fn default_bg_and_border_are_none() {
        let s = Style::default();
        assert!(s.bg_color.is_none());
        assert!(s.border_color.is_none());
    }

    #[test]
    fn set_bg_color_accepts_raw_and_token() {
        let mut s = Style::default();
        s.set_bg_color(Color::rgb(1, 2, 3));
        assert_eq!(s.bg_color, Some(ThemedColor::Raw(Color::rgb(1, 2, 3))));
        s.set_bg_color(ColorToken::Surface);
        assert_eq!(s.bg_color, Some(ThemedColor::Token(ColorToken::Surface)));
    }

    #[test]
    fn clear_bg_color_resets_to_none() {
        let mut s = Style::default();
        s.set_bg_color(Color::rgb(1, 2, 3));
        s.clear_bg_color();
        assert!(s.bg_color.is_none());
    }
}

pub struct Children(pub Vec<crate::ecs::Entity>);

pub struct Parent(pub crate::ecs::Entity);

/// Resolved post-layout rect (cf. `Style.layout` declarations).
pub struct ComputedRect(pub crate::types::Rect);

/// Marks the entity dirty alongside writing the new position.
pub fn set_position(
    world: &mut crate::ecs::World,
    entity: crate::ecs::Entity,
    x: impl Into<crate::types::Fixed>,
    y: impl Into<crate::types::Fixed>,
) {
    use crate::types::{Dimension, Fixed, Rect};
    use dirty::{Dirty, PrevRect};

    let x = x.into();
    let y = y.into();

    if let Some(style) = world.get::<Style>(entity) {
        let l = &style.layout;
        let old_rect = Rect {
            x: l.left.resolve_or(Fixed::ZERO, Fixed::ZERO),
            y: l.top.resolve_or(Fixed::ZERO, Fixed::ZERO),
            w: l.width.resolve_or(Fixed::ZERO, Fixed::ZERO),
            h: l.height.resolve_or(Fixed::ZERO, Fixed::ZERO),
        };
        let new_rect = Rect {
            x,
            y,
            w: old_rect.w,
            h: old_rect.h,
        };
        if old_rect.to_px() != new_rect.to_px() {
            let (px, py, pw, ph) = old_rect.to_px();
            let axis_old = Rect::new(px, py, pw, ph);
            let merged = match world.get::<PrevRect>(entity) {
                Some(p) => p.0.union(&axis_old),
                None => axis_old,
            };
            world.insert(entity, PrevRect(merged));
        }
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.layout.left = Dimension::Px(x);
        style.layout.top = Dimension::Px(y);
    }
    world.insert(entity, Dirty);
}
