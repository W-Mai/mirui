use crate::backend::InputEvent;
use crate::components::button::Button;
use crate::components::checkbox::Checkbox;
use crate::components::progress_bar::ProgressBar;
use crate::ecs::{Entity, World};
use crate::event::hit_test::hit_test;
use crate::layout::{LayoutNode, compute_layout};
use crate::types::Rect;
use crate::widget::dirty::Dirty;
use crate::widget::{Children, Style, Widget};

pub fn button_system(
    world: &mut World,
    root: Entity,
    event: &InputEvent,
    screen_w: u16,
    screen_h: u16,
) {
    match event {
        InputEvent::Touch { x, y } => {
            if let Some(target) = hit_test(world, root, *x, *y, screen_w, screen_h) {
                if world.get::<Button>(target).is_some() {
                    if let Some(btn) = world.get_mut::<Button>(target) {
                        btn.pressed = true;
                    }
                    world.insert(target, Dirty);
                }
                if world.get::<ProgressBar>(target).is_some() {
                    if let Some(r) = get_entity_rect(world, root, target, screen_w, screen_h) {
                        if r.w > 0 {
                            let ratio = ((*x - r.x) as f32) / (r.w as f32);
                            if let Some(pb) = world.get_mut::<ProgressBar>(target) {
                                pb.value = ratio.clamp(0.0, 1.0);
                            }
                            world.insert(target, Dirty);
                        }
                    }
                }
            }
        }
        InputEvent::Release { x, y } => {
            if let Some(target) = hit_test(world, root, *x, *y, screen_w, screen_h) {
                if world.get::<Checkbox>(target).is_some() {
                    if let Some(cb) = world.get_mut::<Checkbox>(target) {
                        cb.toggle();
                    }
                    world.insert(target, Dirty);
                }
            }
            reset_all_buttons(world, root);
        }
        _ => {}
    }
}

fn reset_all_buttons(world: &mut World, entity: Entity) {
    if let Some(btn) = world.get_mut::<Button>(entity) {
        if btn.pressed {
            btn.pressed = false;
            world.insert(entity, Dirty);
        }
    }
    if let Some(children) = world.get::<Children>(entity) {
        let ids: alloc::vec::Vec<Entity> = children.0.clone();
        for child in ids {
            reset_all_buttons(world, child);
        }
    }
}

fn get_entity_rect(
    world: &World,
    root: Entity,
    target: Entity,
    screen_w: u16,
    screen_h: u16,
) -> Option<Rect> {
    let mut tree = build_layout(world, root)?;
    compute_layout(&mut tree, 0, 0, screen_w, screen_h);
    let mut entities = alloc::vec::Vec::new();
    collect_preorder(world, root, &mut entities);
    let idx = entities.iter().position(|&e| e == target)?;
    find_rect_at(&tree, idx, &mut 0)
}

fn build_layout(world: &World, entity: Entity) -> Option<LayoutNode> {
    world.get::<Widget>(entity)?;
    let style = world.get::<Style>(entity)?;
    let mut node = LayoutNode::new(style.layout);
    if let Some(children) = world.get::<Children>(entity) {
        for &child in &children.0 {
            if let Some(child_node) = build_layout(world, child) {
                node.add_child(child_node);
            }
        }
    }
    Some(node)
}

fn collect_preorder(world: &World, entity: Entity, out: &mut alloc::vec::Vec<Entity>) {
    out.push(entity);
    if let Some(children) = world.get::<Children>(entity) {
        for &child in &children.0 {
            collect_preorder(world, child, out);
        }
    }
}

fn find_rect_at(node: &LayoutNode, target_idx: usize, idx: &mut usize) -> Option<Rect> {
    if *idx == target_idx {
        return Some(node.rect);
    }
    *idx += 1;
    for child in &node.children {
        if let Some(r) = find_rect_at(child, target_idx, idx) {
            return Some(r);
        }
    }
    None
}
