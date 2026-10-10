use mirui::core::model::Model;
use mirui::core::reactive::{Effect, flush_signal_dirty};
use mirui::prelude::*;
use mirui::render::DrawCommand;
use mirui::surface::framebuf::FramebufSurface;
use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use super::tracking_allocator::tracked_allocations;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ChildChange(u8);

impl ChildChange {
    const NONE: Self = Self(0);
    const VISUAL: Self = Self(1);
    const SAVE: Self = Self(2);

    fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParentChange {
    None,
    Summary,
}

impl ParentChange {
    fn contains(self, other: Self) -> bool {
        self == other
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Rejected;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Note(u16);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Audit(u16);

#[model(change = ChildChange, watch(visual = ChildChange::VISUAL, save = ChildChange::SAVE))]
struct Counter {
    #[observe]
    value: u16,
    pixels: u16,
    notes: [Option<Note>; 2],
    audit: Option<Audit>,
}

#[model]
impl Counter {
    fn new(value: u16) -> Self {
        Self {
            value,
            pixels: value,
            notes: [None; 2],
            audit: None,
        }
    }

    fn add(&mut self, amount: u16) -> ChildChange {
        self.value += amount;
        ChildChange::VISUAL
    }

    fn paint(&mut self, pixels: u16) -> ChildChange {
        self.pixels = pixels;
        ChildChange::VISUAL
    }

    fn pixels(&self) -> u16 {
        self.pixels
    }

    fn emit(&mut self, value: u16) -> ChildChange {
        self.value = value;
        self.notes = [Some(Note(value)), Some(Note(value + 1))];
        self.audit = Some(Audit(value));
        ChildChange::VISUAL
    }

    fn reject(&mut self, value: u16) -> Result<ChildChange, Rejected> {
        self.emit(value);
        Err(Rejected)
    }

    fn idle(&mut self) -> ChildChange {
        ChildChange::NONE
    }

    #[effects]
    fn take_notes(&mut self) -> [Option<Note>; 2] {
        core::mem::take(&mut self.notes)
    }

    #[effects]
    fn take_audit(&mut self) -> [Option<Audit>; 1] {
        [self.audit.take()]
    }
}

#[model(change = ParentChange, watch(summary = ParentChange::Summary))]
struct Pair {
    left: model!(owned Counter),
    right: model!(owned Counter),
    note: Option<Note>,
}

#[model]
impl Pair {
    fn new(left: u16, right: u16) -> Self {
        Self {
            left: Counter::new(left).into(),
            right: Counter::new(right).into(),
            note: None,
        }
    }

    #[observe]
    fn total(&self) -> u16 {
        self.left.value() + self.right.value()
    }

    fn add_both(&mut self) -> ParentChange {
        self.left.add(1);
        self.left.add(1);
        self.right.add(1);
        ParentChange::Summary
    }

    fn paint_left(&mut self, value: u16) -> ParentChange {
        self.left.paint(value);
        ParentChange::None
    }

    fn emit_reverse(&mut self) -> ParentChange {
        self.right.emit(12);
        self.left.emit(2);
        self.note = Some(Note(20));
        ParentChange::Summary
    }

    fn reject_after_success(&mut self) -> Result<ParentChange, Rejected> {
        self.right.emit(12);
        self.left.reject(2)?;
        Ok(ParentChange::Summary)
    }

    fn handle_child_error(&mut self) -> Result<ParentChange, Rejected> {
        let _ = self.left.reject(2);
        self.right.emit(12);
        Ok(ParentChange::None)
    }

    fn reset_left(&mut self, value: u16) -> ParentChange {
        self.left = Counter::new(value).into();
        ParentChange::None
    }

    fn replace_pending(&mut self) -> ParentChange {
        self.left.emit(90);
        self.left = Counter::new(1).into();
        ParentChange::None
    }

    fn swap_pending(&mut self) -> ParentChange {
        self.left.emit(3);
        self.right.emit(30);
        core::mem::swap(&mut self.left, &mut self.right);
        ParentChange::None
    }

    fn swap_round_trip(&mut self) -> ParentChange {
        core::mem::swap(&mut self.left, &mut self.right);
        core::mem::swap(&mut self.left, &mut self.right);
        ParentChange::None
    }

    fn rejected_reset(&mut self) -> Result<ParentChange, Rejected> {
        self.left = Counter::new(1).into();
        Err(Rejected)
    }

    fn idle(&mut self) -> ParentChange {
        ParentChange::None
    }

    #[effects]
    fn take_note(&mut self) -> [Option<Note>; 1] {
        [self.note.take()]
    }
}

#[model]
struct Workspace {
    pair: model!(owned Pair),
    spare: model!(owned Counter),
    note: Option<Note>,
}

#[model]
impl Workspace {
    fn new() -> Self {
        Self {
            pair: Pair::new(1, 10).into(),
            spare: Counter::new(100).into(),
            note: None,
        }
    }

    #[observe]
    fn total(&self) -> u16 {
        self.pair.total() + self.spare.value()
    }

    fn emit_reverse(&mut self) {
        self.spare.emit(103);
        self.pair.emit_reverse();
        self.note = Some(Note(200));
    }

    fn handle_pair_error(&mut self) -> Result<(), Rejected> {
        let _ = self.pair.reject_after_success();
        Ok(())
    }

    fn reset_pair(&mut self) {
        self.pair = Pair::new(1, 10).into();
    }

    #[effects]
    fn take_note(&mut self) -> [Option<Note>; 1] {
        [self.note.take()]
    }
}

fn count_reads(read: impl Fn() + 'static) -> (Effect, Rc<Cell<usize>>) {
    let runs = Rc::new(Cell::new(0));
    let count = Rc::clone(&runs);
    let effect = Effect::new(move || {
        read();
        count.set(count.get() + 1);
    });
    (effect, runs)
}

mod callback_bindings;
mod effects;
mod extraction;
mod generic_fields;
mod identity;
mod observations;
