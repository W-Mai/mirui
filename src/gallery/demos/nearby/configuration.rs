pub mod schema;
use super::wifi::Action;
use crate::prelude::*;
use alloc::collections::BTreeMap;
use alloc::{format, string::String, vec, vec::Vec};
use schema::Fact;
use schema::{Change, Document, Field, Id, Load, Request, Scope, Section, Value, View};

#[derive(Clone, Copy)]
enum Leave {
    Parent,
    Scope(Scope),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    pub label: String,
    pub value: String,
    pub changed: bool,
    pub editable: bool,
}

#[model]
pub struct Editor {
    view: Option<View>,
    #[observe]
    revision: u64,
    #[observe]
    pub section: Section,
    #[observe]
    pub scope: Scope,
    #[observe]
    pub selected: usize,
    #[observe]
    pub age: u16,
    #[observe]
    pub offset: Fixed,
    heights: Vec<Fixed>,
    #[observe]
    viewport_height: Fixed,
    row_metrics: (i32, i32),
    text_sizes: Vec<[Fixed; 2]>,
    #[observe]
    geometry: u64,
    changed_at: u64,
    now: u64,
    draft: BTreeMap<Id, Option<Value>>,
    source: Option<Id>,
    history: Vec<(Section, usize, Option<Id>)>,
    leave: Option<Leave>,
    after_save: bool,
    request: Option<Request>,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            view: None,
            revision: 0,
            section: Section::Home,
            scope: Scope::Game,
            selected: 0,
            age: 0,
            offset: Fixed::ZERO,
            heights: vec![],
            viewport_height: Fixed::ZERO,
            row_metrics: (4, 2),
            text_sizes: vec![],
            geometry: 0,
            changed_at: 0,
            now: 0,
            draft: BTreeMap::new(),
            source: None,
            history: vec![],
            leave: None,
            after_save: false,
            request: None,
        }
    }
}

#[model]
impl Editor {
    pub fn sync(&mut self, view: Option<&View>) {
        let fresh = self.view.as_ref().map(|v| v.epoch) != view.map(|v| v.epoch);
        let loaded =
            self.document().is_none() && view.is_some_and(|v| matches!(v.load, Load::Ready { .. }));
        if fresh {
            self.section = Section::Home;
            self.scope = Scope::Game;
            self.selected = 0;
            self.draft.clear();
            self.source = None;
            self.history.clear();
            self.leave = None;
            self.after_save = false;
            self.request = None;
        }
        if fresh || loaded {
            self.changed_at = self.now;
            self.text_sizes.clear();
            self.heights.clear();
            self.offset = Fixed::ZERO;
            self.geometry += 1;
            let count = self
                .document()
                .map_or(0, |document| {
                    document.fields.len() + document.saves.len() + document.sources.len() + 6
                })
                .max(
                    view.and_then(|view| match &view.load {
                        Load::Ready { document } => Some(
                            document.fields.len()
                                + document.saves.len()
                                + document.sources.len()
                                + 6,
                        ),
                        _ => None,
                    })
                    .unwrap_or(0),
                );
            self.heights
                .reserve(count.saturating_sub(self.heights.len()));
            self.text_sizes
                .reserve(count.saturating_sub(self.text_sizes.len()));
            self.history
                .reserve(4_usize.saturating_sub(self.history.len()));
        }
        let saved = self
            .view
            .as_ref()
            .zip(view)
            .is_some_and(|(old, new)| old.saved != new.saved);
        if self.view.as_ref() != view {
            self.view = view.cloned();
            self.revision += 1;
        }
        if saved {
            self.draft.clear();
            if self.after_save {
                self.finish_leave();
            }
            self.after_save = false;
            self.revision += 1;
        }
        if self.view.as_ref().is_some_and(|v| !v.error.is_empty()) {
            self.after_save = false;
            if self.dirty() && self.section != Section::Conflict {
                self.enter(Section::Conflict);
                self.selected = 1;
            }
        }
    }

    fn value(&self, field: &Field) -> Value {
        self.draft
            .get(&field.id)
            .map(|value| {
                value
                    .clone()
                    .unwrap_or_else(|| field.inherited[self.scope.index()].clone())
            })
            .unwrap_or_else(|| field.values[self.scope.index()].clone())
    }
    #[observe]
    pub fn active(&self) -> bool {
        let _ = self.revision;
        self.view.is_some()
    }
    #[observe]
    pub fn loading(&self) -> bool {
        let _ = self.revision;
        self.view
            .as_ref()
            .is_some_and(|v| matches!(v.load, Load::Loading))
    }
    #[observe]
    pub fn saving(&self) -> bool {
        let _ = self.revision;
        self.view.as_ref().is_some_and(|v| v.saving)
    }
    #[observe]
    pub fn dirty(&self) -> bool {
        let _ = self.revision;
        !self.draft.is_empty()
    }
    fn snapshot_heading(&self) -> String {
        match self.section {
            Section::Home => "游戏设置",
            Section::Video => "画面",
            Section::Core => "核心选项",
            Section::Input => "按键映射",
            Section::Saves => "存档与配置",
            Section::Sources => "文件与来源",
            Section::Confirm => "保存修改？",
            Section::Conflict => "保存未完成",
        }
        .into()
    }
    #[observe]
    pub fn confirmable(&self) -> bool {
        let _ = self.revision;
        self.count() > 0
            && (self.section != Section::Sources || self.inheritance() && self.selected == 0)
    }
    fn snapshot_confirm_label(&self) -> String {
        match self.section {
            Section::Home => "选择",
            Section::Confirm | Section::Conflict => "确认",
            Section::Sources => "继承",
            _ => "来源",
        }
        .into()
    }
    fn snapshot_identity(&self) -> String {
        self.document()
            .map(|d| format!("{}  /  {}", d.title, d.core))
            .unwrap_or_default()
    }
    fn snapshot_status(&self) -> String {
        let Some(view) = &self.view else {
            return String::new();
        };
        if !view.error.is_empty() {
            return view.error.clone();
        }
        if let Load::Failed { message } = &view.load {
            return message.clone();
        }
        if view.saving {
            "正在保存…".into()
        } else if self.loading() {
            "正在读取设置…".into()
        } else if self.dirty() {
            "有未保存的修改".into()
        } else if view.saved > 0 {
            "已保存".into()
        } else {
            String::new()
        }
    }
    fn fact_count(&self) -> usize {
        if self.source_field().is_some() {
            return 5;
        }
        self.document().map_or(0, |document| {
            document.saves.len() + self.extra_facts().count() + document.sources.len()
        })
    }
    fn fact(&self, index: usize) -> Option<Fact> {
        if let Some(field) = self.source_field() {
            let scope = self.scope.index();
            return Some(match index {
                0 => Fact::new("设置项", &field.label),
                1 => Fact::new("当前值", field.label_for(&self.value(field))),
                2 => Fact::new("来源", &field.origins[scope]),
                3 => Fact::new(
                    "写入文件",
                    if field.targets[scope].is_empty() {
                        "此范围未启用"
                    } else {
                        &field.targets[scope]
                    },
                ),
                4 => Fact::new("继承值", field.label_for(&field.inherited[scope])),
                _ => return None,
            });
        }
        let document = self.document()?;
        if let Some(fact) = document.saves.get(index) {
            return Some(fact.clone());
        }
        let index = index - document.saves.len();
        if let Some(fact) = self.extra_facts().nth(index) {
            return Some(Fact::new(format!("保存后 {}", fact.label), &fact.value));
        }
        document
            .sources
            .get(index - self.extra_facts().count())
            .cloned()
    }
    #[observe]
    pub fn count(&self) -> usize {
        let _ = self.revision;
        if self.document().is_none() {
            return 0;
        }
        match self.section {
            Section::Home => 4,
            Section::Confirm => 3,
            Section::Conflict => 2,
            Section::Sources => self.fact_count() + usize::from(self.inheritance()),
            Section::Saves => self.fields().count() + 1,
            _ => self.fields().count(),
        }
    }
    fn snapshot_window(&self) -> core::ops::Range<usize> {
        let count = self.count();
        if self.section == Section::Sources {
            return 0..count;
        }
        if count == 0 {
            return 0..0;
        }
        let (gap, shadow) = self.row_metrics;
        let extent = |index| self.snapshot_extent(index, gap) + Fixed::from_int(gap + shadow);
        let mut first = self.selected.min(count - 1);
        let mut end = first + 1;
        let mut occupied = extent(first);
        while first > 0 && end - first < 8 && occupied + extent(first - 1) <= self.viewport_height {
            first -= 1;
            occupied += extent(first);
        }
        while end < count && end - first < 8 && occupied + extent(end) <= self.viewport_height {
            occupied += extent(end);
            end += 1;
        }
        first..end
    }
    fn snapshot_row(&self, index: usize) -> Row {
        let label = match self.section {
            Section::Home => ["画面", "核心选项", "按键映射", "存档与配置"]
                .get(index)
                .copied(),
            Section::Confirm => ["保存并继续", "放弃修改", "继续编辑"].get(index).copied(),
            Section::Conflict => ["重新读取", "继续编辑"].get(index).copied(),
            _ => None,
        };
        if let Some(label) = label {
            return Row {
                label: label.into(),
                editable: true,
                ..Default::default()
            };
        }
        if self.section == Section::Sources {
            if self.inheritance() && index == 0 {
                return Row {
                    label: match self.source {
                        Some(Id::Option(_)) if self.scope == Scope::Game => "继承全部核心选项",
                        Some(Id::Button(_)) => "继承全部按键映射",
                        _ => "恢复继承",
                    }
                    .into(),
                    editable: true,
                    ..Default::default()
                };
            }
            return self
                .fact(index.saturating_sub(usize::from(self.inheritance())))
                .map(|f| Row {
                    label: f.label.clone(),
                    value: f.value.clone(),
                    ..Default::default()
                })
                .unwrap_or_default();
        }
        self.fields()
            .nth(index)
            .map(|field| Row {
                label: field.label.clone(),
                value: field.label_for(&self.value(field)),
                changed: self.draft.contains_key(&field.id),
                editable: !field.targets[self.scope.index()].is_empty(),
            })
            .unwrap_or_else(|| Row {
                label: "文件与来源".into(),
                editable: true,
                ..Default::default()
            })
    }
    pub fn viewport(&mut self, height: Fixed, gap: i32, shadow: i32) {
        self.row_metrics = (gap, shadow);
        self.viewport_height = height;
        self.scroll();
    }
    pub fn measure(&mut self, index: usize, height: Fixed) {
        if self.section != Section::Sources || index >= self.count() {
            return;
        }
        self.heights.resize(self.count(), Fixed::from_int(56));
        if self.heights[index] != height {
            self.heights[index] = height;
            self.scroll();
        }
    }
    pub fn text_size(&mut self, index: usize, part: usize, height: Fixed) {
        if index >= self.count() || part >= 2 {
            return;
        }
        self.text_sizes.resize(self.count(), [Fixed::ZERO; 2]);
        if self.text_sizes[index][part] != height {
            self.text_sizes[index][part] = height;
            self.geometry += 1;
        }
    }
    fn snapshot_extent(&self, index: usize, gap: i32) -> Fixed {
        let sizes = self.text_sizes.get(index).copied().unwrap_or_default();
        sizes[0].max(Fixed::from_int(26))
            + if self.snapshot_row(index).value.is_empty() {
                Fixed::ZERO
            } else {
                sizes[1].max(Fixed::from_int(14)) + Fixed::from_int(4)
            }
            + Fixed::from_int(gap * 2 + 4)
    }
    fn scroll(&mut self) {
        if self.section != Section::Sources {
            self.offset = Fixed::ZERO;
            return;
        }
        let top = self
            .heights
            .iter()
            .take(self.selected)
            .copied()
            .fold(Fixed::ZERO, |a, b| a + b);
        let bottom = top
            + self
                .heights
                .get(self.selected)
                .copied()
                .unwrap_or(Fixed::from_int(56));
        if top < self.offset {
            self.offset = top;
        } else if bottom > self.offset + self.viewport_height {
            self.offset = (bottom - self.viewport_height).max(Fixed::ZERO);
        }
    }
    pub fn tick(&mut self, now: u64) {
        self.now = now;
        self.age = now.saturating_sub(self.changed_at).min(800) as u16;
    }
    fn changes(&self) -> Vec<Change> {
        self.draft
            .iter()
            .map(|(id, value)| Change {
                id: id.clone(),
                value: value.clone(),
            })
            .collect()
    }
    fn emit(&mut self, save: bool) {
        let Some(view) = &self.view else {
            return;
        };
        let Some(document) = self.document() else {
            return;
        };
        self.request = Some(if save {
            Request::Save {
                epoch: view.epoch,
                revision: document.revision.clone(),
                scope: self.scope,
                changes: self.changes(),
            }
        } else {
            Request::Preview {
                epoch: view.epoch,
                revision: document.revision.clone(),
                scope: self.scope,
                changes: self.changes(),
            }
        });
        if save && let Some(view) = self.view.as_mut() {
            view.saving = true;
            self.revision += 1;
        }
    }
    fn enter(&mut self, section: Section) {
        self.history
            .push((self.section, self.selected, self.source.clone()));
        self.section = section;
        self.selected = 0;
    }
    fn pop(&mut self) {
        if let Some((section, selected, source)) = self.history.pop() {
            self.section = section;
            self.selected = selected;
            self.source = source;
        }
    }
    fn finish_leave(&mut self) {
        let intent = self.leave.take();
        if self.section == Section::Confirm {
            self.pop();
        }
        match intent {
            Some(Leave::Scope(scope)) => self.scope = scope,
            Some(Leave::Parent) => self.back(),
            None => {}
        }
    }
    fn back(&mut self) {
        match self.section {
            Section::Home => {
                if let Some(view) = &self.view {
                    self.request = Some(Request::Close { epoch: view.epoch });
                }
            }
            Section::Sources | Section::Conflict => self.pop(),
            Section::Confirm => {
                self.pop();
                self.leave = None;
            }
            section => {
                self.section = Section::Home;
                self.selected = match section {
                    Section::Core => 1,
                    Section::Input => 2,
                    Section::Saves => 3,
                    _ => 0,
                };
            }
        }
    }
    fn leave(&mut self, intent: Leave) {
        self.leave = Some(intent);
        if self.dirty() {
            self.enter(Section::Confirm);
        } else {
            self.finish_leave();
        }
    }
    fn edit(&mut self, field: Field, value: Option<Value>) {
        if field.targets[self.scope.index()].is_empty() {
            return;
        }
        // These loaders select a complete file; inheritance resets that file as a group.
        if value.is_none()
            && (matches!(field.id, Id::Button(_))
                || self.scope == Scope::Game && matches!(field.id, Id::Option(_)))
        {
            let button = matches!(field.id, Id::Button(_));
            if let Some(document) = self.document() {
                let ids: Vec<_> = document
                    .fields
                    .iter()
                    .filter(|f| {
                        if button {
                            matches!(f.id, Id::Button(_))
                        } else {
                            matches!(f.id, Id::Option(_))
                        }
                    })
                    .map(|f| f.id.clone())
                    .collect();
                for id in ids {
                    self.draft.insert(id, None);
                }
            }
        } else {
            // Resuming edits after whole-file inheritance starts from the inherited values.
            if value.is_some()
                && (matches!(field.id, Id::Button(_))
                    || self.scope == Scope::Game && matches!(field.id, Id::Option(_)))
            {
                let button = matches!(field.id, Id::Button(_));
                let inherited: Vec<_> = self
                    .document()
                    .into_iter()
                    .flat_map(|d| &d.fields)
                    .filter(|f| {
                        self.draft.get(&f.id) == Some(&None)
                            && if button {
                                matches!(f.id, Id::Button(_))
                            } else {
                                matches!(f.id, Id::Option(_))
                            }
                    })
                    .map(|f| (f.id.clone(), Some(f.inherited[self.scope.index()].clone())))
                    .collect();
                self.draft.extend(inherited);
            }
            if value.as_ref() == Some(&field.values[self.scope.index()]) {
                self.draft.remove(&field.id);
            } else {
                self.draft.insert(field.id, value);
            }
        }
        self.revision += 1;
        self.emit(false);
    }
    pub fn input(&mut self, action: Action) {
        let previous = self.section;
        if self.saving() {
            return;
        }
        if self.document().is_none() {
            if let Some(view) = &self.view {
                self.request = match action {
                    Action::Back => Some(Request::Close { epoch: view.epoch }),
                    Action::Refresh => Some(Request::Reload { epoch: view.epoch }),
                    _ => None,
                };
            }
            return;
        }
        match action {
            Action::Up | Action::Down => {
                let delta = if action == Action::Up { -1 } else { 1 };
                self.selected = (self.selected as isize + delta)
                    .clamp(0, self.count().saturating_sub(1) as isize)
                    as usize;
            }
            Action::Back => {
                if matches!(
                    self.section,
                    Section::Confirm | Section::Conflict | Section::Sources
                ) {
                    self.back();
                } else {
                    self.leave(Leave::Parent);
                }
            }
            Action::Auxiliary
                if !matches!(
                    self.section,
                    Section::Confirm | Section::Conflict | Section::Sources
                ) =>
            {
                self.leave(Leave::Scope(self.scope.next()))
            }
            Action::Refresh if self.dirty() && self.section != Section::Confirm => self.emit(true),
            Action::Confirm if self.section == Section::Home => {
                self.section = [
                    Section::Video,
                    Section::Core,
                    Section::Input,
                    Section::Saves,
                ][self.selected.min(3)];
                self.selected = 0;
            }
            Action::Confirm if self.section == Section::Confirm => match self.selected {
                0 => {
                    self.after_save = true;
                    self.emit(true);
                }
                1 => {
                    self.draft.clear();
                    self.finish_leave();
                }
                _ => self.back(),
            },
            Action::Confirm if self.section == Section::Conflict => {
                if self.selected == 0 {
                    self.draft.clear();
                    if let Some(view) = &self.view {
                        self.request = Some(Request::Reload { epoch: view.epoch });
                    }
                }
                self.back();
            }
            Action::Confirm if self.section == Section::Sources => {
                if self.selected == 0
                    && self.inheritance()
                    && let Some(field) = self
                        .source
                        .as_ref()
                        .and_then(|id| self.document()?.fields.iter().find(|f| &f.id == id))
                        .cloned()
                {
                    self.edit(field, None);
                    self.back();
                }
            }
            Action::Confirm => {
                let source = self.fields().nth(self.selected).map(|f| f.id.clone());
                self.enter(Section::Sources);
                self.source = source;
            }
            Action::Left | Action::Right => {
                let field = self.fields().nth(self.selected).cloned();
                if let Some(field) = field {
                    let current = self.value(&field);
                    let index = field
                        .choices
                        .iter()
                        .position(|c| c.value == current)
                        .unwrap_or(0);
                    let next = (index as isize + if action == Action::Left { -1 } else { 1 })
                        .rem_euclid(field.choices.len().max(1) as isize)
                        as usize;
                    if let Some(choice) = field.choices.get(next) {
                        self.edit(field.clone(), Some(choice.value.clone()));
                    }
                }
            }
            _ => {}
        }
        self.changed_at = self.now;
        self.revision += 1;
        if self.section != previous {
            self.text_sizes.clear();
            self.heights.clear();
            self.offset = Fixed::ZERO;
            self.geometry += 1;
        }
        self.scroll();
    }
    #[effects]
    fn take_request(&mut self) -> Option<Request> {
        self.request.take()
    }
}

impl EditorHandle {
    pub fn confirm_label(&self) -> String {
        self.section();
        self.snapshot_confirm_label()
    }
    pub fn extent(&self, index: usize, gap: i32) -> Fixed {
        self.revision();
        self.section();
        self.geometry();
        self.snapshot_extent(index, gap)
    }
    pub fn heading(&self) -> String {
        self.revision();
        self.section();
        self.snapshot_heading()
    }
    pub fn identity(&self) -> String {
        self.revision();
        self.snapshot_identity()
    }
    pub fn status(&self) -> String {
        self.revision();
        self.snapshot_status()
    }
    pub fn row(&self, index: usize) -> Row {
        self.revision();
        self.scope();
        self.section();
        self.snapshot_row(index)
    }
    pub fn window(&self) -> core::ops::Range<usize> {
        self.revision();
        self.viewport_height();
        self.geometry();
        self.selected();
        self.section();
        self.snapshot_window()
    }
}

impl Editor {
    fn source_field(&self) -> Option<&Field> {
        let id = self.source.as_ref()?;
        self.document()?.fields.iter().find(|field| &field.id == id)
    }
    fn extra_facts(&self) -> impl Iterator<Item = &Fact> {
        let document = self.document();
        self.view
            .iter()
            .flat_map(|view| &view.projected)
            .filter(move |fact| {
                document.is_some_and(|document| {
                    document
                        .saves
                        .iter()
                        .any(|old| old.label == fact.label && old.value != fact.value)
                })
            })
    }
    fn inheritance(&self) -> bool {
        self.source
            .as_ref()
            .and_then(|id| self.document()?.fields.iter().find(|f| &f.id == id))
            .is_some_and(|f| !f.targets[self.scope.index()].is_empty())
    }
    fn document(&self) -> Option<&Document> {
        match &self.view.as_ref()?.load {
            Load::Ready { document } => Some(document),
            _ => None,
        }
    }
    fn fields(&self) -> impl Iterator<Item = &Field> {
        self.document()
            .into_iter()
            .flat_map(|document| &document.fields)
            .filter(|field| field.section == self.section)
    }
}

pub fn fixture() -> View {
    let boolean = vec![
        schema::Choice {
            value: Value::Bool(false),
            label: "关闭".into(),
        },
        schema::Choice {
            value: Value::Bool(true),
            label: "开启".into(),
        },
    ];
    let fields = [
        (Id::IntegerScale, Section::Video, "整数缩放"),
        (Id::AutoLoad, Section::Saves, "自动载入即时存档"),
    ]
    .into_iter()
    .map(|(id, section, label)| Field {
        id,
        section,
        label: label.into(),
        choices: boolean.clone(),
        values: [Value::Bool(false), Value::Bool(false), Value::Bool(false)],
        inherited: [Value::Bool(false), Value::Bool(false), Value::Bool(false)],
        origins: core::array::from_fn(|_| {
            "/home/ark/.config/retroarch/config/Snes9x/超级炸弹人5.cfg".into()
        }),
        targets: core::array::from_fn(|_| {
            "/home/ark/.config/retroarch/config/Snes9x/超级炸弹人5.cfg".into()
        }),
    })
    .chain((0..24).map(|i| Field {
        id: Id::Option(format!("core_option_{i}")),
        section: Section::Core,
        label: format!("选项 {i}"),
        choices: boolean.clone(),
        values: core::array::from_fn(|_| Value::Bool(false)),
        inherited: core::array::from_fn(|_| Value::Bool(false)),
        origins: core::array::from_fn(|_| "默认值".into()),
        targets: core::array::from_fn(|_| {
            "/home/ark/.config/retroarch/config/Snes9x/game.opt".into()
        }),
    }))
    .collect();
    View {
        epoch: 7,
        load: Load::Ready {
            document: Document {
                key: "a".repeat(64),
                title: "超级炸弹人5".into(),
                core: "Snes9x".into(),
                revision: "b".repeat(64),
                fields,
                saves: vec![Fact::new("SRAM 文件", "/roms/snes/Snes9x/超级炸弹人5.srm")],
                ..Default::default()
            },
        },
        ..Default::default()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    #[test]
    fn unopened_editor_has_no_prepared_rows_and_loaded_geometry_reuses_capacity() {
        let mut editor = Editor::default();
        assert_eq!(editor.heights.capacity(), 0);
        assert_eq!(editor.text_sizes.capacity(), 0);
        assert_eq!(editor.history.capacity(), 0);
        editor.sync(Some(&fixture()));
        let capacity = (editor.heights.capacity(), editor.text_sizes.capacity());
        assert!(capacity.0 >= 33 && capacity.1 >= 33);
        editor.input(Action::Down);
        editor.input(Action::Confirm);
        editor.viewport(180.into(), 4, 2);
        assert_eq!(editor.count(), 24);
        for _ in 0..24 {
            editor.input(Action::Down);
            let window = editor.snapshot_window();
            assert!(window.contains(&editor.selected));
            assert!(window.len() <= 8);
            editor.text_size(editor.selected, 0, 26.into());
        }
        editor.input(Action::Confirm);
        assert_eq!(editor.section, Section::Sources);
        for index in 0..editor.count() {
            editor.measure(index, 56.into());
            assert!(!editor.snapshot_row(index).label.is_empty());
        }
        assert_eq!(
            (editor.heights.capacity(), editor.text_sizes.capacity()),
            capacity
        );
        editor.input(Action::Back);
        assert_eq!(editor.selected, 23);
    }

    #[test]
    fn game_option_inheritance_resets_a_group_then_resumes_from_inherited_values() {
        let mut editor = Editor::default();
        editor.sync(Some(&fixture()));
        editor.input(Action::Down);
        editor.input(Action::Confirm);
        editor.input(Action::Confirm);
        editor.input(Action::Confirm);
        assert_eq!(editor.section, Section::Core);
        assert_eq!(editor.draft.len(), 24);
        assert!(editor.draft.values().all(Option::is_none));
        editor.input(Action::Right);
        assert_eq!(
            editor.draft.get(&Id::Option("core_option_0".into())),
            Some(&Some(Value::Bool(true)))
        );
        assert!(editor.draft.values().all(Option::is_some));
    }

    #[test]
    fn unavailable_scopes_and_pending_documents_do_not_create_edits() {
        let mut editor = Editor::default();
        let mut view = fixture();
        if let Load::Ready { document } = &mut view.load {
            document.fields[0].targets[0].clear();
        }
        editor.sync(Some(&view));
        editor.input(Action::Confirm);
        editor.input(Action::Right);
        assert!(!editor.dirty());
        assert!(editor.take_request().is_none());
        view.epoch += 1;
        view.load = Load::Loading;
        editor.sync(Some(&view));
        assert_eq!(editor.count(), 0);
        editor.input(Action::Right);
        assert!(editor.take_request().is_none());
        editor.input(Action::Back);
        assert!(matches!(
            editor.take_request(),
            Some(Request::Close { epoch: 8 })
        ));
    }
    #[test]
    fn draft_scope_return_and_save_acknowledgement_share_the_document_identity() {
        let mut model = Editor::default();
        let mut view = fixture();
        model.sync(Some(&view));
        model.input(Action::Confirm);
        assert_eq!(model.section, Section::Video);
        model.input(Action::Right);
        assert!(model.dirty());
        assert!(matches!(
            model.take_request(),
            Some(Request::Preview { epoch: 7, .. })
        ));
        model.input(Action::Auxiliary);
        assert_eq!(model.section, Section::Confirm);
        model.input(Action::Confirm);
        assert!(model.saving());
        assert!(matches!(
            model.take_request(),
            Some(Request::Save {
                scope: Scope::Game,
                ..
            })
        ));
        model.input(Action::Back);
        assert_eq!(model.section, Section::Confirm);
        view.saved = 1;
        model.sync(Some(&view));
        assert!(!model.dirty());
        assert_eq!(model.scope, Scope::Core);
        assert_eq!(model.section, Section::Video);
        model.input(Action::Right);
        model.input(Action::Back);
        model.input(Action::Down);
        model.input(Action::Confirm);
        assert_eq!(model.section, Section::Home);
        assert!(!model.dirty());
        model.input(Action::Back);
        assert!(matches!(
            model.take_request(),
            Some(Request::Close { epoch: 7 })
        ));
    }
    #[test]
    fn failed_save_keeps_the_draft_until_explicit_reload_and_epoch_retirement() {
        let mut model = Editor::default();
        let mut view = fixture();
        model.sync(Some(&view));
        model.input(Action::Confirm);
        model.input(Action::Right);
        model.input(Action::Refresh);
        view.error = "配置文件已变更，请重新读取".into();
        model.sync(Some(&view));
        assert_eq!(model.section, Section::Conflict);
        assert!(model.dirty());
        model.input(Action::Up);
        model.input(Action::Confirm);
        assert!(!model.dirty());
        assert!(matches!(
            model.take_request(),
            Some(Request::Reload { epoch: 7 })
        ));
        view.epoch = 8;
        view.error.clear();
        model.sync(Some(&view));
        assert_eq!(model.section, Section::Home);
        assert_eq!(model.scope, Scope::Game);
    }
    #[test]
    fn failed_confirmation_retains_its_scope_intent_and_nested_return_route() {
        let mut model = Editor::default();
        let mut view = fixture();
        model.sync(Some(&view));
        model.input(Action::Confirm);
        model.input(Action::Right);
        model.input(Action::Auxiliary);
        model.input(Action::Confirm);
        view.error = "外部修改".into();
        model.sync(Some(&view));
        assert_eq!(model.section, Section::Conflict);
        model.input(Action::Confirm);
        assert_eq!(model.section, Section::Confirm);
        model.input(Action::Down);
        model.input(Action::Confirm);
        assert_eq!(model.scope, Scope::Core);
        assert_eq!(model.section, Section::Video);
        assert!(!model.dirty());
        model.input(Action::Confirm);
        assert_eq!(model.section, Section::Sources);
        model.input(Action::Back);
        assert_eq!(model.section, Section::Video);
        assert_eq!(model.selected, 0);
    }
}
