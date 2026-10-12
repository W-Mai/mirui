//! Widget kinds and their shared infrastructure.
//!
//! Built-in widget constructors follow a uniform shape:
//!
//! - `Widget::default()` when no required arguments
//! - `Widget::new(required, ...)` when required arguments exist
//! - `.with_<field>(value)` for optional configuration; chains
//!
//! `Widget::new()` on no-arg widgets is kept as an alias for
//! `Widget::default()` so the call site can stay either form.

#[doc(hidden)]
pub mod branch;
pub(crate) mod builder;
pub(crate) mod construction;
pub(crate) mod delivery;
pub mod dirty;
pub mod icons;
pub mod id;
pub mod layout;
pub(crate) mod layout_binding;
pub(crate) mod lifecycle;
pub mod niche;
pub mod offscreen;
pub mod outputs;
pub mod property;
pub(crate) mod recipe;
pub mod render_system;
#[doc(hidden)]
pub mod resource_access;
pub mod scope;
pub mod state;
pub mod style_view;
pub mod theme;
pub mod view;
pub mod visibility;
pub mod widgets;

pub use id::{IdMap, NamedId};
pub use layout_binding::{
    LAYOUT_DEPENDENCY_CAPACITY, LayoutAxis, LayoutContainer, LayoutDependency, LayoutSource,
    LayoutValues,
};
pub use niche::NicheMap;
pub use offscreen::{OffscreenGeneration, OffscreenRender, WidgetTexture, WidgetTextureRead};
pub use outputs::{Output, Outputs};
pub use property::WidgetMut;
pub use scope::UiScope;
pub use state::{InteractionState, UserState};
pub use theme::{ColorToken, Theme, ThemeCatalog, ThemeError, ThemeSource, ThemedColor};
pub use view::{View, ViewInvalidation, ViewRegistry};
pub use visibility::{Hidden, HitTarget, IgnoreHitTest, InteractionFeedback};

use alloc::vec::Vec;

use crate::types::Fixed;
use crate::ui::layout::LayoutStyle;

pub struct Widget;

impl crate::ecs::Component for Widget {}

/// Applies the surface safe area as minimum layout padding for this entity.
#[crate::component]
#[derive(Clone, Copy, Debug, Default)]
pub struct RespectSafeArea;

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderKey(pub u64);

/// Active root resource maintained by `App::set_root`. Typed callbacks can read `res!(WidgetRoot)` and retrieve its laid-out rectangle with `query!(ComputedRect)`.
#[derive(Clone, Copy)]
pub struct WidgetRoot(pub crate::ecs::Entity);

#[crate::component]
#[derive(Clone, Debug)]
pub struct Style {
    pub bg_color: Option<ThemedColor>,
    pub border_color: Option<ThemedColor>,
    pub border_width: Fixed,
    pub border_radius: Fixed,
    /// Always present; for transparent text set alpha on the resolved colour.
    pub text_color: ThemedColor,
    pub font_stack: crate::render::font::FontStack,
    pub font_size: Option<u16>,
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
            font_stack: crate::render::font::FontStack::default(),
            font_size: None,
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

    pub fn set_font_token(
        &mut self,
        token: impl Into<crate::render::font::FontToken>,
    ) -> &mut Self {
        self.font_stack = crate::render::font::FontStack::new(token.into());
        self
    }

    pub fn set_font_stack(
        &mut self,
        stack: impl Into<crate::render::font::FontStack>,
    ) -> &mut Self {
        self.font_stack = stack.into();
        self
    }

    pub fn set_font_size(&mut self, size: u16) -> &mut Self {
        self.font_size = Some(size.max(1));
        self
    }

    pub fn clear_font_size(&mut self) -> &mut Self {
        self.font_size = None;
        self
    }

    pub fn absolute_at(rect: crate::types::Rect) -> Self {
        Self {
            layout: LayoutStyle {
                position: crate::ui::layout::Position::Absolute,
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

    #[test]
    fn font_size_is_optional_and_never_zero() {
        let mut style = Style::default();
        assert_eq!(style.font_size, None);
        assert_eq!(style.set_font_size(24).font_size, Some(24));
        assert_eq!(style.set_font_size(0).font_size, Some(1));
        assert_eq!(style.clear_font_size().font_size, None);
    }
}

pub struct Children(pub Vec<crate::ecs::Entity>);

impl crate::ecs::Component for Children {}

#[crate::view(component = Children, name = "Children", invalidation = Layout)]
fn children_view() {}

pub struct Parent(pub crate::ecs::Entity);

impl crate::ecs::Component for Parent {}

#[cfg(test)]
mod construction_tests {
    use super::*;
    use crate::app::App;
    use crate::ecs::Component;

    struct Tag(u32);
    impl Component for Tag {}

    #[test]
    fn closure_children_wire_parent_and_children() {
        let mut app = App::headless(32, 32);
        let root = app.spawn_root().id();
        let (panel, ()) = app.compose(root, |tree| {
            tree.children(Tag(0), |c| {
                c.spawn(Tag(1));
                c.spawn(Tag(2));
            })
        });
        let world = &app.world;
        let kids = &world.get::<Children>(panel).unwrap().0;
        assert_eq!(kids.len(), 2);
        assert_eq!(world.get::<Tag>(kids[0]).unwrap().0, 1);
        assert_eq!(world.get::<Tag>(kids[1]).unwrap().0, 2);
        assert_eq!(world.get::<Parent>(kids[0]).unwrap().0, panel);
        assert_eq!(world.get::<Parent>(kids[1]).unwrap().0, panel);
        assert!(world.has::<Widget>(panel));
    }

    #[test]
    fn nested_closure_children_return_their_results() {
        let mut app = App::headless(32, 32);
        let root = app.spawn_root().id();
        let (panel, (child, leaf)) = app.compose(root, |tree| {
            tree.children(Tag(0), |c| c.children(Tag(2), |gc| gc.spawn(Tag(3))))
        });
        let world = &app.world;
        assert_eq!(world.get::<Children>(panel).unwrap().0, [child]);
        assert_eq!(world.get::<Tag>(child).unwrap().0, 2);
        assert_eq!(world.get::<Children>(child).unwrap().0, [leaf]);
        assert_eq!(world.get::<Parent>(child).unwrap().0, panel);
    }

    #[test]
    fn despawn_subtree_recurses_and_unlinks_parent() {
        let mut app = App::headless(32, 32);
        let root = app.spawn_root().id();
        let (panel, (child, leaf)) = app.compose(root, |tree| {
            tree.children(Tag(0), |c| {
                let branch = c.children(Tag(1), |gc| gc.spawn(Tag(9)));
                c.spawn(Tag(2));
                branch
            })
        });
        assert_eq!(app.world.get::<Children>(panel).unwrap().0.len(), 2);
        despawn_subtree(&mut app.world, child);
        assert!(!app.world.is_alive(child));
        assert!(!app.world.is_alive(leaf));
        let kids = &app.world.get::<Children>(panel).unwrap().0;
        assert_eq!(kids.len(), 1);
        assert!(!kids.contains(&child));
    }

    #[test]
    fn despawn_subtree_clears_named_id() {
        let mut app = App::headless(32, 32);
        let root = app.spawn_root().id();
        let entity = app.compose(root, |tree| tree.spawn((Tag(0), NamedId("panel"))));
        assert_eq!(app.world.find_by_id("panel"), Some(entity));
        despawn_subtree(&mut app.world, entity);
        assert!(app.world.find_by_id("panel").is_none());
    }

    #[test]
    fn despawn_subtree_on_dead_entity_is_noop() {
        let mut app = App::headless(32, 32);
        let root = app.spawn_root().id();
        let entity = app.compose(root, |tree| tree.spawn(Tag(0)));
        despawn_subtree(&mut app.world, entity);
        despawn_subtree(&mut app.world, entity);
        assert!(!app.world.is_alive(entity));
    }
}

/// Resolved post-layout rect (cf. `Style.layout` declarations).
pub struct ComputedRect(pub crate::types::Rect);

impl crate::ecs::Component for ComputedRect {}

/// Marks the entity dirty alongside writing the new position.
pub fn set_position(
    world: &mut crate::ecs::World,
    entity: crate::ecs::Entity,
    x: impl Into<crate::types::Fixed>,
    y: impl Into<crate::types::Fixed>,
) {
    set_position_inner(world, entity, x.into(), y.into(), true);
}

/// Like [`set_position`] but skips the `Dirty` mark. Use when the
/// entity's pixels are about to be moved by something else (e.g. an
/// enclosing scroll container's self-blit) so a redundant redraw is
/// undesirable.
pub fn set_position_quiet(
    world: &mut crate::ecs::World,
    entity: crate::ecs::Entity,
    x: impl Into<crate::types::Fixed>,
    y: impl Into<crate::types::Fixed>,
) {
    set_position_inner(world, entity, x.into(), y.into(), false);
}

fn set_position_inner(
    world: &mut crate::ecs::World,
    entity: crate::ecs::Entity,
    x: crate::types::Fixed,
    y: crate::types::Fixed,
    mark_dirty: bool,
) {
    use crate::types::{Dimension, Fixed, Rect};
    use dirty::{Dirty, PrevRect};

    let next_left = Dimension::Px(x);
    let next_top = Dimension::Px(y);
    if world
        .get::<Style>(entity)
        .is_some_and(|style| style.layout.left == next_left && style.layout.top == next_top)
    {
        return;
    }

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
        if old_rect.pixel_bounds() != new_rect.pixel_bounds() && mark_dirty {
            let (x0, y0, x1, y1) = old_rect.pixel_bounds();
            let axis_old = Rect::new(x0, y0, x1 - x0, y1 - y0);
            let merged = match world.get::<PrevRect>(entity) {
                Some(p) => p.0.union(&axis_old),
                None => axis_old,
            };
            world.insert(entity, PrevRect(merged));
        }
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.layout.left = next_left;
        style.layout.top = next_top;
    }
    if mark_dirty {
        world.insert(entity, Dirty);
    }
}

#[cfg(test)]
mod position_tests {
    use super::*;
    use crate::ui::dirty::Dirty;

    #[test]
    fn unchanged_position_does_not_invalidate_widget() {
        let mut world = crate::ecs::World::new();
        let entity = world.spawn_empty();
        let mut style = Style::default();
        style.layout.left = crate::types::Dimension::px(12);
        style.layout.top = crate::types::Dimension::px(24);
        world.insert(entity, style);

        set_position(&mut world, entity, 12, 24);

        assert!(!world.has::<Dirty>(entity));
    }
}

/// Despawn `entity` and its subtree, unlinking from the parent and dropping its
/// effects / `NamedId` mapping. The vacated region is flagged BEFORE despawn —
/// `World::despawn` clears the rect, so otherwise the deleted widget leaves
/// ghost pixels the dirty union never repaints.
pub fn despawn_subtree(world: &mut crate::ecs::World, entity: crate::ecs::Entity) {
    use crate::ecs::Entity;
    use dirty::{Dirty, PrevRect};

    if !world.is_alive(entity) {
        return;
    }

    if let Some(parent) = world.get::<Parent>(entity).map(|p| p.0) {
        if let Some(rect) = world.get::<ComputedRect>(entity).map(|c| c.0) {
            let merged = match world.get::<PrevRect>(parent) {
                Some(p) => p.0.union(&rect),
                None => rect,
            };
            world.insert(parent, PrevRect(merged));
            world.insert(parent, Dirty);
        }
        if let Some(children) = world.get_mut::<Children>(parent) {
            children.0.retain(|&c| c != entity);
        }
    }

    fn drop_node(world: &mut crate::ecs::World, e: Entity) {
        let kids = world
            .get::<Children>(e)
            .map(|c| c.0.clone())
            .unwrap_or_default();
        for child in kids {
            drop_node(world, child);
        }
        if let Some(name) = world.get::<NamedId>(e).map(|n| n.0)
            && let Some(map) = world.resource_mut::<IdMap>()
        {
            map.remove(name);
        }
        crate::core::reactive::cleanup_effects_for(world, e);
        world.despawn(e);
    }

    drop_node(world, entity);
}
