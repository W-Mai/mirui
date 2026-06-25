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

pub mod builder;
pub mod dirty;
pub mod icons;
pub mod id;
pub mod layout;
pub mod niche;
pub mod offscreen;
pub mod reactive_attr;
pub mod render_system;
pub mod state;
pub mod style_view;
pub mod theme;
pub mod view;
pub mod visibility;
pub mod widgets;

pub use id::{IdMap, NamedId};
pub use niche::NicheMap;
pub use offscreen::{
    OffscreenAlphaMode, OffscreenAutoAdded, OffscreenBufferPool, OffscreenGeneration,
    OffscreenRender, TextureSnapshot, WidgetTextureAccess, WidgetTextureRef,
    WidgetTextureRefPrevGen,
};
pub use state::{InteractionState, UserState};
pub use theme::{ColorToken, Theme, ThemedColor};
pub use view::{View, ViewRegistry};
pub use visibility::{Hidden, IgnoreHitTest};

use alloc::vec::Vec;

use crate::types::Fixed;
use crate::ui::layout::LayoutStyle;

pub struct Widget;

/// World resource cached by `App::set_root` so handlers and systems can
/// reach the active root without an `App` reference.
#[derive(Clone, Copy)]
pub struct WidgetRoot(pub crate::ecs::Entity);

/// The root's laid-out `Rect` this frame, for systems that size
/// absolute children to the live canvas instead of a setup-time value.
/// `None` before the first layout pass.
pub fn root_viewport(world: &crate::ecs::World) -> Option<crate::types::Rect> {
    let root = world.resource::<WidgetRoot>()?.0;
    world.get::<ComputedRect>(root).map(|c| c.0)
}

#[derive(Clone, Debug, crate::Component)]
pub struct Style {
    pub bg_color: Option<ThemedColor>,
    pub border_color: Option<ThemedColor>,
    pub border_width: Fixed,
    pub border_radius: Fixed,
    /// Always present; for transparent text set alpha on the resolved colour.
    pub text_color: ThemedColor,
    pub font_token: crate::render::font::FontToken,
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
            font_token: crate::render::font::FontToken::Default,
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
        self.font_token = token.into();
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
}

pub struct Children(pub Vec<crate::ecs::Entity>);

pub struct Parent(pub crate::ecs::Entity);

/// Holds the world's only `&mut` for a [`spawn_children`] closure, so child
/// spawns are sequential — two child handles can never be live at once.
pub struct ChildSpawner<'w> {
    world: &'w mut crate::ecs::World,
    parent: crate::ecs::Entity,
}

impl ChildSpawner<'_> {
    pub fn spawn<B: crate::ecs::IntoBundle>(&mut self, bundle: B) -> crate::ecs::Entity {
        let child = self.world.spawn(bundle);
        self.attach(child);
        child
    }

    pub fn children<B: crate::ecs::IntoBundle>(
        &mut self,
        bundle: B,
        f: impl FnOnce(&mut ChildSpawner),
    ) -> crate::ecs::Entity {
        let child = spawn_children(self.world, bundle, f);
        self.attach(child);
        child
    }

    fn attach(&mut self, child: crate::ecs::Entity) {
        self.world.insert(child, Parent(self.parent));
        if let Some(children) = self.world.get_mut::<Children>(self.parent) {
            children.0.push(child);
        }
    }
}

/// Spawn `parent` from a bundle, then run `f` to populate its children.
/// `f` receives a [`ChildSpawner`] holding the world's only mutable borrow,
/// which `spawn_children` reclaims when `f` returns.
pub fn spawn_children<B: crate::ecs::IntoBundle>(
    world: &mut crate::ecs::World,
    bundle: B,
    f: impl FnOnce(&mut ChildSpawner),
) -> crate::ecs::Entity {
    let parent = world.spawn(bundle);
    if !world.has::<Children>(parent) {
        world.insert(parent, Children(Vec::new()));
    }
    let mut spawner = ChildSpawner { world, parent };
    f(&mut spawner);
    parent
}

#[cfg(test)]
mod child_spawner_tests {
    use super::*;
    use crate::ecs::{IntoBundle, World};

    struct Tag(u32);
    struct TagBundle(u32);
    impl IntoBundle for TagBundle {
        fn spawn_into(self, world: &mut World, entity: crate::ecs::Entity) {
            world.insert(entity, Tag(self.0));
        }
    }

    #[test]
    fn closure_children_wire_parent_and_children() {
        let mut world = World::default();
        let root = spawn_children(&mut world, TagBundle(0), |c| {
            c.spawn(TagBundle(1));
            c.spawn(TagBundle(2));
        });

        let kids = &world.get::<Children>(root).unwrap().0;
        assert_eq!(kids.len(), 2);
        assert_eq!(world.get::<Tag>(kids[0]).unwrap().0, 1);
        assert_eq!(world.get::<Tag>(kids[1]).unwrap().0, 2);
        assert_eq!(world.get::<Parent>(kids[0]).unwrap().0, root);
        assert_eq!(world.get::<Parent>(kids[1]).unwrap().0, root);
        assert!(world.has::<Widget>(root));
    }

    #[test]
    fn nested_closure_children() {
        let mut world = World::default();
        let mut grandchild = None;
        let root = spawn_children(&mut world, TagBundle(0), |c| {
            grandchild = Some(c.children(TagBundle(2), |gc| {
                gc.spawn(TagBundle(3));
            }));
        });
        let grandchild = grandchild.unwrap();
        assert_eq!(world.get::<Children>(root).unwrap().0.len(), 1);
        assert_eq!(world.get::<Tag>(grandchild).unwrap().0, 2);
        assert_eq!(world.get::<Children>(grandchild).unwrap().0.len(), 1);
        assert_eq!(world.get::<Parent>(grandchild).unwrap().0, root);
    }

    #[test]
    fn despawn_subtree_recurses_and_unlinks_parent() {
        let mut world = World::default();
        let mut child0 = None;
        let mut grandchild = None;
        let root = spawn_children(&mut world, TagBundle(0), |c| {
            child0 = Some(c.children(TagBundle(1), |gc| {
                grandchild = Some(gc.spawn(TagBundle(9)));
            }));
            c.spawn(TagBundle(2));
        });
        let child0 = child0.unwrap();
        let grandchild = grandchild.unwrap();
        assert_eq!(world.get::<Children>(root).unwrap().0.len(), 2);

        despawn_subtree(&mut world, child0);

        assert!(!world.is_alive(child0), "subtree root despawned");
        assert!(
            !world.is_alive(grandchild),
            "descendant despawned recursively"
        );
        let kids = &world.get::<Children>(root).unwrap().0;
        assert_eq!(kids.len(), 1, "unlinked from parent's Children");
        assert!(!kids.contains(&child0));
    }

    #[test]
    fn despawn_subtree_clears_named_id() {
        let mut world = World::default();
        world.insert_resource(IdMap::new());
        let e = spawn_children(&mut world, TagBundle(0), |_| {});
        world.insert(e, NamedId("panel"));
        world.resource_mut::<IdMap>().unwrap().insert("panel", e);

        despawn_subtree(&mut world, e);
        assert!(world.resource::<IdMap>().unwrap().get("panel").is_none());
    }

    #[test]
    fn despawn_subtree_on_dead_entity_is_noop() {
        let mut world = World::default();
        let e = spawn_children(&mut world, TagBundle(0), |_| {});
        despawn_subtree(&mut world, e);
        despawn_subtree(&mut world, e); // second call must not panic
        assert!(!world.is_alive(e));
    }
}

/// Resolved post-layout rect (cf. `Style.layout` declarations).
pub struct ComputedRect(pub crate::types::Rect);

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
        if old_rect.to_px() != new_rect.to_px() && mark_dirty {
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
    if mark_dirty {
        world.insert(entity, Dirty);
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
        crate::core::reactive::cleanup_effects_for(e);
        world.despawn(e);
    }

    drop_node(world, entity);
}
