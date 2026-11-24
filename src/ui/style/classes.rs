use alloc::{borrow::Cow, boxed::Box, vec::Vec};

use crate::core::reactive::{BindingRole, Effect, with_world, with_world_scope};
use crate::ecs::{Component, Entity, World};
use crate::ui::recipe::PropertyBindings;

use super::{
    Button, ButtonStyleFields, ParagraphStyle, StyleDefaults, StyleFields, StyleInput, StylePreset,
    Text, binding::reconcile_style, update_paragraph,
};

mod list;
pub use list::ClassList;

/// Typed style stored by a stylesheet rule.
#[doc(hidden)]
#[derive(PartialEq, Eq)]
pub struct ClassRule {
    common: StyleFields,
    widget: RuleWidget,
}

#[derive(PartialEq, Eq)]
enum RuleWidget {
    Any,
    Button(ButtonStyleFields),
    Text(ParagraphStyle),
}

impl From<StylePreset> for ClassRule {
    fn from(style: StylePreset) -> Self {
        Self {
            common: style.common,
            widget: RuleWidget::Any,
        }
    }
}

impl From<StylePreset<Button>> for ClassRule {
    fn from(style: StylePreset<Button>) -> Self {
        Self {
            common: style.common,
            widget: RuleWidget::Button(style.specific),
        }
    }
}

impl From<StylePreset<Text>> for ClassRule {
    fn from(style: StylePreset<Text>) -> Self {
        Self {
            common: style.common,
            widget: style.specific.map_or(RuleWidget::Any, RuleWidget::Text),
        }
    }
}

struct ResolvedClasses {
    common: StyleFields,
    common_declared: u64,
    button: ButtonStyleFields,
    paragraph: Option<ParagraphStyle>,
}

impl StyleInput<()> for ResolvedClasses {
    fn apply_style(self, style: &mut super::Style, explicit: u64) -> u64 {
        self.common.apply(style, explicit)
    }

    fn declared(&self) -> u64 {
        self.common_declared
            | self.button.declared()
            | self
                .paragraph
                .as_ref()
                .map_or(0, |_| super::StyleField::paragraph.bit())
    }

    fn fields(self) -> StyleFields {
        self.common
    }

    fn update_widget(
        &mut self,
        world: &mut World,
        entity: Entity,
        explicit: u64,
        previous: u64,
    ) -> u64 {
        self.button.update(world, entity, explicit, previous)
            | update_paragraph(self.paragraph.take(), world, entity, explicit, previous)
    }
}

struct Rule {
    name: Cow<'static, str>,
    value: Option<ClassRule>,
    effect: Option<Effect>,
    pending: Option<Definition>,
    changed: bool,
    type_changed: bool,
    generation: u64,
}

enum Definition {
    Value,
    Getter(Box<dyn Fn() -> ClassRule>),
    Removed,
}

/// App-scoped class definitions. The next UI preparation applies rule changes
/// to widgets that use the corresponding names.
#[derive(Default)]
pub struct StyleSheet {
    rules: Vec<Rule>,
    members_pending: Vec<Entity>,
    rules_pending: bool,
    rules_changed: bool,
    generation: u64,
}

impl StyleSheet {
    /// Add or replace a class rule.
    pub fn set(&mut self, name: impl Into<Cow<'static, str>>, style: impl Into<ClassRule>) {
        let style = style.into();
        let rule = self.rule_mut(name.into());
        if rule.value.as_ref() == Some(&style) && rule.effect.is_none() && rule.pending.is_none() {
            return;
        }
        rule.set_value(style);
        rule.pending = Some(Definition::Value);
        self.rules_pending = true;
    }

    /// Bind a rule to a reactive value. The getter runs once for the rule.
    pub fn bind_class<S: Into<ClassRule>>(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        read: impl Fn() -> S + 'static,
    ) {
        let rule = self.rule_mut(name.into());
        rule.pending = Some(Definition::Getter(Box::new(move || read().into())));
        self.rules_pending = true;
    }

    /// Remove a definition. Widgets can keep using the name.
    pub fn remove(&mut self, name: &str) -> bool {
        let Ok(index) = self.index(name) else {
            return false;
        };
        let rule = &mut self.rules[index];
        if matches!(rule.pending, Some(Definition::Removed)) {
            return false;
        }
        rule.pending = Some(Definition::Removed);
        self.rules_pending = true;
        true
    }

    fn index(&self, name: &str) -> Result<usize, usize> {
        self.rules
            .binary_search_by(|rule| rule.name.as_ref().cmp(name))
    }

    fn rule_mut(&mut self, name: Cow<'static, str>) -> &mut Rule {
        let index = self.index(&name).unwrap_or_else(|index| {
            self.rules.insert(
                index,
                Rule {
                    name,
                    value: None,
                    effect: None,
                    pending: None,
                    changed: false,
                    type_changed: false,
                    generation: 0,
                },
            );
            index
        });
        &mut self.rules[index]
    }

    fn active_rule(&mut self, generation: u64) -> Option<&mut Rule> {
        self.rules
            .iter_mut()
            .find(|rule| rule.generation == generation && rule.pending.is_none())
    }

    fn resolve(
        &self,
        world: &World,
        entity: Entity,
        names: &ClassList,
        explicit: u64,
        diagnose: bool,
    ) -> ResolvedClasses {
        let mut resolved = ResolvedClasses {
            common: StyleFields::EMPTY,
            common_declared: 0,
            button: ButtonStyleFields::EMPTY,
            paragraph: None,
        };
        let mut claimed = explicit;
        let current = world.get::<super::Style>(entity);
        for name in names.iter().rev() {
            let Ok(index) = self.index(name) else {
                continue;
            };
            let rule = &self.rules[index];
            let Some(value) = rule.value.as_ref() else {
                continue;
            };
            match &value.widget {
                RuleWidget::Any => {}
                RuleWidget::Button(fields) if world.has::<Button>(entity) => {
                    resolved.button.fill_from(fields);
                }
                RuleWidget::Text(paragraph) if world.has::<Text>(entity) => {
                    resolved.paragraph.get_or_insert_with(|| paragraph.clone());
                }
                _ => {
                    if diagnose || rule.type_changed {
                        crate::warn!("style class {:?} does not match {:?}", name, entity);
                    }
                    continue;
                }
            }
            if let Some(current) = current {
                resolved
                    .common
                    .fill_changes_from(&value.common, current, &mut claimed);
            }
        }
        resolved.common_declared = claimed & !explicit;
        resolved
    }
}

impl Rule {
    fn set_value(&mut self, value: ClassRule) {
        self.type_changed |= self.value.as_ref().is_none_or(|active| {
            core::mem::discriminant(&active.widget) != core::mem::discriminant(&value.widget)
        });
        self.value = Some(value);
    }
}

/// Install a stylesheet and register initial class definitions.
///
/// **Inserts**
/// - resource: `StyleSheet`
#[derive(Default)]
pub struct StyleSheetPlugin(StyleSheet);

impl StyleSheetPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a class with a static rule.
    pub fn class(
        mut self,
        name: impl Into<Cow<'static, str>>,
        style: impl Into<ClassRule>,
    ) -> Self {
        self.0.set(name, style);
        self
    }

    /// Register a class whose rule is computed from reactive values.
    pub fn bind_class<S: Into<ClassRule>>(
        mut self,
        name: impl Into<Cow<'static, str>>,
        read: impl Fn() -> S + 'static,
    ) -> Self {
        self.0.bind_class(name, read);
        self
    }
    pub(crate) fn install(&mut self, world: &mut World) {
        let configured = core::mem::take(&mut self.0);
        if let Some(sheet) = world.resource_mut::<StyleSheet>() {
            for rule in configured.rules {
                let entry = sheet.rule_mut(rule.name);
                entry.pending = rule.pending;
                if let Some(value) = rule.value {
                    entry.set_value(value);
                }
                sheet.rules_pending = true;
            }
        } else {
            world.insert_resource(configured);
        }
    }
}

struct Membership {
    names: ClassList,
    base: StyleDefaults,
    explicit: u64,
    inline: u64,
    written: u64,
    released: u64,
    pending: bool,
    diagnose: bool,
}

impl Component for Membership {}

fn queue_member(world: &mut World, entity: Entity) {
    let Some(member) = world.get_mut::<Membership>(entity) else {
        return;
    };
    if !member.pending {
        member.pending = true;
        if let Some(sheet) = world.resource_mut::<StyleSheet>() {
            sheet.members_pending.push(entity);
        }
    }
}

/// Install a class list alongside the widget's direct and inline declarations.
#[doc(hidden)]
pub fn install_classes(
    world: &mut World,
    entity: Entity,
    names: impl Into<ClassList>,
    explicit: u64,
    base: StyleDefaults,
) {
    if world.resource::<StyleSheet>().is_none() {
        world.insert_resource(StyleSheet::default());
    }
    world.insert(
        entity,
        Membership {
            names: names.into(),
            base,
            explicit,
            inline: 0,
            written: 0,
            released: 0,
            pending: false,
            diagnose: true,
        },
    );
    queue_member(world, entity);
}

pub(crate) fn replace_classes(world: &mut World, entity: Entity, names: ClassList) {
    if world
        .get::<Membership>(entity)
        .is_none_or(|member| member.names == names)
    {
        return;
    }
    if let Some(member) = world.get_mut::<Membership>(entity) {
        member.names = names;
        member.diagnose = true;
    }
    queue_member(world, entity);
}

pub(super) fn release_inline_fields<W, S: StyleInput<W>>(
    world: &mut World,
    entity: Entity,
    style: &S,
    explicit: u64,
    previous: u64,
) -> (u64, u64) {
    let Some(member) = world.get::<Membership>(entity) else {
        return (previous, explicit);
    };
    let declared = style.declared() & !explicit;
    let min_height = super::StyleField::min_height.bit();
    let restored = (member.written | member.released | (previous & !declared)) & min_height;
    let restore_default = explicit | restored & !declared;
    if member.inline != declared {
        let released = previous & !declared;
        if let Some(member) = world.get_mut::<Membership>(entity) {
            member.inline = declared;
            member.released = (member.released | released) & !declared;
            member.written &= !declared;
        }
        if released != 0 {
            queue_member(world, entity);
        }
        return (previous & declared, restore_default);
    }
    (previous, restore_default)
}

#[doc(hidden)]
pub fn prepare_class_binding<L: Into<ClassList>>(
    entity: Entity,
    read: impl Fn() -> L + 'static,
) -> impl FnOnce(&mut World, bool) {
    let effect = crate::core::reactive::widget_binding::prepare_widget_effect(
        entity,
        BindingRole::Display,
        move || {
            let names = read().into();
            with_world(|world| replace_classes(world, entity, names));
        },
    );
    move |world, once| {
        if once {
            effect.install_once(world);
        } else {
            effect.install(world);
        }
    }
}

fn prepare_rules(world: &mut World) {
    let count = world.resource_mut::<StyleSheet>().map_or(0, |sheet| {
        if !core::mem::take(&mut sheet.rules_pending) {
            return 0;
        }
        sheet
            .rules
            .iter()
            .filter(|rule| rule.pending.is_some())
            .count()
    });
    for _ in 0..count {
        let Some(sheet) = world.resource_mut::<StyleSheet>() else {
            break;
        };
        let Some(rule) = sheet.rules.iter_mut().find(|rule| rule.pending.is_some()) else {
            break;
        };
        let definition = rule.pending.take().expect("pending rule");
        sheet.generation = sheet
            .generation
            .checked_add(1)
            .expect("stylesheet generation overflow");
        let generation = sheet.generation;
        rule.generation = generation;
        let old_effect = rule.effect.take();
        match definition {
            Definition::Value => {
                rule.changed = true;
                sheet.rules_changed = true;
                drop(old_effect);
            }
            Definition::Removed => {
                rule.value = None;
                rule.changed = true;
                sheet.rules_changed = true;
                drop(old_effect);
            }
            Definition::Getter(read) => {
                drop(old_effect);
                let effect = with_world_scope(world, || {
                    Effect::new(move || {
                        let current = with_world(|world| {
                            world
                                .resource_mut::<StyleSheet>()
                                .is_some_and(|sheet| sheet.active_rule(generation).is_some())
                        })
                        .unwrap_or(false);
                        if !current {
                            return;
                        }
                        let value = read();
                        with_world(|world| {
                            if let Some(sheet) = world.resource_mut::<StyleSheet>()
                                && let Some(rule) = sheet.active_rule(generation)
                                && rule.value.as_ref() != Some(&value)
                            {
                                rule.set_value(value);
                                rule.changed = true;
                                sheet.rules_changed = true;
                            }
                        });
                    })
                });
                if let Some(sheet) = world.resource_mut::<StyleSheet>()
                    && let Some(rule) = sheet.active_rule(generation)
                {
                    rule.effect = Some(effect);
                }
            }
        }
    }
}

pub(crate) fn reconcile_classes(world: &mut World) {
    let Some(sheet) = world.resource::<StyleSheet>() else {
        return;
    };
    if !sheet.rules_pending && !sheet.rules_changed && sheet.members_pending.is_empty() {
        return;
    }
    prepare_rules(world);
    world.with_resource_box::<StyleSheet, _>(|sheet, world| {
        if sheet.rules_changed {
            if let Some(members) = world.storage::<Membership>() {
                for (entity, member) in members.iter() {
                    if !member.pending
                        && member.names.iter().any(|name| {
                            sheet
                                .index(name)
                                .is_ok_and(|index| sheet.rules[index].changed)
                        })
                    {
                        sheet.members_pending.push(entity);
                    }
                }
            }
        }
        for entity in sheet.members_pending.iter().copied() {
            let Some(member) = world.get::<Membership>(entity) else {
                continue;
            };
            let explicit = member.explicit | member.inline;
            let previous = member.written | member.released;
            let base = member.base;
            let value = sheet.resolve(world, entity, &member.names, explicit, member.diagnose);
            let written = reconcile_style::<(), _>(world, entity, value, explicit, base, previous);
            if let Some(member) = world.get_mut::<Membership>(entity) {
                member.written = written;
                member.released = 0;
                member.pending = false;
                member.diagnose = false;
            }
        }
        sheet.members_pending.clear();
        if core::mem::take(&mut sheet.rules_changed) {
            sheet.rules.retain_mut(|rule| {
                rule.changed = false;
                rule.type_changed = false;
                rule.value.is_some() || rule.pending.is_some() || rule.effect.is_some()
            });
        }
    });
}

pub(crate) struct StaticClasses {
    names: ClassList,
    explicit: u64,
}

impl StaticClasses {
    pub(crate) fn new(names: impl Into<ClassList>) -> Self {
        Self {
            names: names.into(),
            explicit: 0,
        }
    }
}

impl PropertyBindings for StaticClasses {
    const HAS_CLASSES: bool = true;

    fn install(self, world: &mut World, entity: Entity) {
        install_classes(
            world,
            entity,
            self.names,
            self.explicit,
            StyleDefaults::default(),
        );
    }

    fn set_explicit_style(&mut self, fields: u64) {
        self.explicit = fields;
    }
}

pub(crate) struct ClassBinding<F> {
    read: F,
    explicit: u64,
}

impl<F> ClassBinding<F> {
    pub(crate) fn new(read: F) -> Self {
        Self { read, explicit: 0 }
    }
}

impl<L: Into<ClassList>, F: Fn() -> L + 'static> PropertyBindings for ClassBinding<F> {
    const HAS_CLASSES: bool = true;

    fn install(self, world: &mut World, entity: Entity) {
        install_classes(
            world,
            entity,
            ClassList::default(),
            self.explicit,
            StyleDefaults::default(),
        );
        prepare_class_binding(entity, self.read)(world, false);
    }

    fn set_explicit_style(&mut self, fields: u64) {
        self.explicit = fields;
    }
}
