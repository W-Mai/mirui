//! Reconstructible lookup paths owned by a prepared layout snapshot.

use alloc::vec::Vec;
use core::ops::Range;

use crate::ecs::Entity;

use super::LayoutNode;

struct Entry {
    entity: Entity,
    steps: Range<usize>,
}

struct Ancestor<'a> {
    index: usize,
    parent: Option<&'a Ancestor<'a>>,
}

#[derive(Default)]
pub(crate) struct LayoutPaths {
    entries: Vec<Entry>,
    steps: Vec<usize>,
    prepared: Option<[u64; 3]>,
    sparse: bool,
}

impl LayoutPaths {
    pub(crate) fn invalidate(&mut self) {
        self.prepared = None;
    }

    pub(crate) fn release(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn prepare(
        &mut self,
        tree: &LayoutNode,
        nodes: usize,
        key: [u64; 3],
        requested: impl Fn(Entity) -> bool,
    ) {
        if self.prepared == Some(key) {
            return;
        }
        self.entries.clear();
        self.steps.clear();
        self.sparse = self.build(&requested, tree, nodes, None, 0).is_ok();
        if self.sparse && !self.entries.is_empty() {
            let oversized = self.entries.capacity() > self.entries.len().saturating_mul(4)
                || self.steps.capacity() > self.steps.len().max(1).saturating_mul(4);
            if oversized {
                self.release();
                self.sparse = self.build(&requested, tree, nodes, None, 0).is_ok();
            }
        }
        if !self.sparse || self.entries.is_empty() {
            self.release();
        }
        // A rejected cache is a prepared traversal choice, not a reason to retry each frame.
        self.prepared = Some(key);
    }

    fn build(
        &mut self,
        requested: &impl Fn(Entity) -> bool,
        node: &LayoutNode,
        nodes: usize,
        ancestor: Option<&Ancestor<'_>>,
        depth: usize,
    ) -> Result<(), ()> {
        if let Some(entity) = node.entity
            && requested(entity)
        {
            if self
                .entries
                .len()
                .saturating_add(1)
                .saturating_add(self.steps.len())
                .saturating_add(depth)
                >= nodes
            {
                return Err(());
            }
            self.entries.try_reserve(1).map_err(|_| ())?;
            self.steps.try_reserve(depth).map_err(|_| ())?;
            let start = self.steps.len();
            self.steps.resize(start + depth, 0);
            let mut cursor = ancestor;
            for slot in self.steps[start..].iter_mut().rev() {
                let step = cursor.expect("path depth matches ancestor chain");
                *slot = step.index;
                cursor = step.parent;
            }
            self.entries.push(Entry {
                entity,
                steps: start..self.steps.len(),
            });
        }
        for (index, child) in node.children.iter().enumerate() {
            let step = Ancestor {
                index,
                parent: ancestor,
            };
            self.build(requested, child, nodes, Some(&step), depth + 1)?;
        }
        Ok(())
    }

    pub(crate) fn selected(&self) -> Option<SelectedPaths<'_>> {
        self.sparse.then_some(SelectedPaths {
            entries: &self.entries,
            steps: &self.steps,
        })
    }

    pub(crate) fn path(&self, entity: Entity) -> Option<&[usize]> {
        if !self.sparse {
            return None;
        }
        let entry = self.entries.iter().find(|entry| entry.entity == entity)?;
        Some(&self.steps[entry.steps.clone()])
    }

    pub(crate) fn node_mut<'a>(
        &self,
        tree: &'a mut LayoutNode,
        entity: Entity,
    ) -> Option<&'a mut LayoutNode> {
        if let Some(path) = self.path(entity) {
            let mut node = tree;
            for &index in path {
                node = node.children.get_mut(index)?;
            }
            return (node.entity == Some(entity)).then_some(node);
        }
        find_mut(tree, entity)
    }
}

fn find_mut(node: &mut LayoutNode, entity: Entity) -> Option<&mut LayoutNode> {
    if node.entity == Some(entity) {
        return Some(node);
    }
    node.children
        .iter_mut()
        .find_map(|node| find_mut(node, entity))
}

#[derive(Clone, Copy)]
pub(crate) struct SelectedPaths<'a> {
    entries: &'a [Entry],
    steps: &'a [usize],
}

impl<'a> SelectedPaths<'a> {
    pub(crate) fn entities(self) -> impl Iterator<Item = Entity> + 'a {
        self.entries.iter().map(|entry| entry.entity)
    }

    pub(crate) fn contains(self, entity: Entity) -> bool {
        self.entities().any(|candidate| candidate == entity)
    }

    pub(crate) fn children(self, depth: usize) -> impl Iterator<Item = (usize, Self)> {
        let mut entries = self.entries;
        core::iter::from_fn(move || {
            while entries
                .first()
                .is_some_and(|entry| entry.steps.len() <= depth)
            {
                entries = &entries[1..];
            }
            let first = entries.first()?;
            let index = self.steps[first.steps.start + depth];
            let count = entries.partition_point(|entry| {
                entry.steps.len() > depth && self.steps[entry.steps.start + depth] == index
            });
            let group = Self {
                entries: &entries[..count],
                steps: self.steps,
            };
            entries = &entries[count..];
            Some((index, group))
        })
    }

    pub(crate) fn subtree(mut self, path: &[usize]) -> Option<Self> {
        for (depth, &index) in path.iter().enumerate() {
            self = self.children(depth).find(|(child, _)| *child == index)?.1;
        }
        Some(self)
    }
}

#[cfg(test)]
mod tests;
