use super::*;
use crate::types::Rect;
use crate::ui::ComputedRect;
use crate::ui::dirty::{Dirty, VisualDirty};

fn widget(world: &mut World, parent: Option<Entity>, style: Style) -> Entity {
    let entity = world.spawn_empty();
    world.insert(entity, style);
    world.insert(entity, ComputedRect(Rect::new(0, 0, 12, 12)));
    world.insert(entity, Children(alloc::vec::Vec::new()));
    if let Some(parent) = parent {
        world.insert(entity, Parent(parent));
        world.get_mut::<Children>(parent).unwrap().0.push(entity);
    }
    entity
}

#[test]
fn inherited_fields_resolve_independently_in_world_and_paint() {
    let mut world = World::new();
    let root = widget(
        &mut world,
        None,
        Style {
            font_stack: Some(FontStack::new(FontToken::Mono)),
            font_size: FontSize::Pixels(24),
            ..Style::default()
        },
    );
    let panel = widget(
        &mut world,
        Some(root),
        Style {
            text_color: Some(ColorToken::Secondary.into()),
            ..Style::default()
        },
    );
    let label = widget(
        &mut world,
        Some(panel),
        Style {
            font_size: FontSize::Native,
            ..Style::default()
        },
    );
    let label_style = world.get::<Style>(label).unwrap();
    let inherited = Typography::inherited(&world, label, label_style);
    let root_values = Typography::local(world.get::<Style>(root).unwrap());
    let panel_values =
        Typography::with_parent(world.get::<Style>(panel).unwrap(), Some(&root_values));
    let painted = Typography::with_parent(label_style, Some(&panel_values));
    assert_eq!(inherited.font_stack.primary(), &FontToken::Mono);
    assert_eq!(inherited.font_size, None);
    assert_eq!(inherited.text_color, ColorToken::Secondary.into());
    assert_eq!(painted.font_stack, inherited.font_stack);
    assert_eq!(painted.font_size, inherited.font_size);
    assert_eq!(painted.text_color, inherited.text_color);
}

#[test]
fn inherited_geometry_and_colour_prune_each_declared_field() {
    let mut world = World::new();
    let root = widget(&mut world, None, Style::default());
    let branch = widget(
        &mut world,
        Some(root),
        Style {
            font_size: FontSize::Native,
            ..Style::default()
        },
    );
    let grandchild = widget(&mut world, Some(branch), Style::default());
    let other = widget(&mut world, Some(root), Style::default());

    let style = world.get_mut::<Style>(root).unwrap();
    style
        .set_font_size(20)
        .set_text_color(ColorToken::OnPrimary);
    TypographyDeclarations::default().update(&mut world, root, false);
    assert!(world.has::<VisualDirty>(branch));
    assert!(world.has::<VisualDirty>(grandchild));
    assert!(!world.has::<Dirty>(branch));
    assert!(!world.has::<Dirty>(grandchild));
    assert!(world.has::<Dirty>(other));
}
