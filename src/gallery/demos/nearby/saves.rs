pub mod schema;
use super::configuration::schema::Fact;
use super::wifi::Action;
use crate::prelude::*;
use alloc::{format, string::String, vec, vec::Vec};
use schema::{Kind, Operation, Record, Request, SourceMatch, Tab, View};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Browse,
    Options,
    Confirm(Operation),
    Location,
    Failure,
    Room,
}

#[model]
pub struct Manager {
    view: Option<View>,
    #[observe]
    revision: u64,
    #[observe]
    shown: bool,
    #[observe]
    pub tab: Tab,
    #[observe]
    pub page: Page,
    #[observe]
    pub selected: usize,
    #[observe]
    pub age: u16,
    #[observe]
    pub rows: usize,
    #[observe]
    pub span_start: Fixed,
    #[observe]
    pub span_end: Fixed,
    #[observe]
    pub preview_size: LayoutSize,
    #[observe]
    geometry: u64,
    fact_sizes: Vec<Fixed>,
    #[observe]
    pub reading_offset: Fixed,
    reading_viewport: Fixed,
    fact_gap: i32,
    chosen: String,
    return_index: usize,
    return_key: String,
    backup_parent: (Tab, String),
    now: u64,
    changed_at: u64,
    request: Option<Request>,
}
impl Default for Manager {
    fn default() -> Self {
        Self {
            view: None,
            revision: 0,
            shown: false,
            tab: Tab::States,
            page: Page::Browse,
            selected: 0,
            age: 0,
            rows: 3,
            span_start: Fixed::ZERO,
            span_end: Fixed::ONE,
            preview_size: LayoutSize {
                width: Fixed::ZERO,
                height: Fixed::ZERO,
            },
            geometry: 0,
            fact_sizes: vec![],
            reading_offset: Fixed::ZERO,
            reading_viewport: Fixed::ZERO,
            fact_gap: 0,
            chosen: String::new(),
            return_index: 0,
            return_key: String::new(),
            backup_parent: (Tab::States, String::new()),
            now: 0,
            changed_at: 0,
            request: None,
        }
    }
}
#[model]
impl Manager {
    pub fn sync(&mut self, view: Option<&View>) {
        self.shown = view.is_some();
        let Some(view) = view else {
            return;
        };
        let fresh = self.view.as_ref().is_none_or(|old| old.epoch != view.epoch);
        let complete = self.view.as_ref().is_some_and(|old| old.busy && !view.busy);
        let error =
            self.view.as_ref().map(|v| &v.error) != Some(&view.error) && !view.error.is_empty();
        let key = if self.page == Page::Browse {
            self.record()
                .map(|r| r.key.clone())
                .unwrap_or_else(|| self.return_key.clone())
        } else {
            self.return_key.clone()
        };
        if self.view.as_ref() != Some(view) {
            self.view = Some(view.clone());
            self.revision += 1;
        }
        if fresh {
            self.tab = view.return_tab;
            self.page = Page::Browse;
            self.selected = 0;
            self.chosen.clear();
            self.return_key = view.return_key.clone();
            self.restore(&view.return_key);
            self.changed_at = self.now;
        } else if complete {
            self.page = Page::Browse;
            self.selected = self.return_index;
            self.restore(&key);
            self.changed_at = self.now;
        }
        if error {
            self.remember();
            self.page = Page::Failure;
            self.selected = 0;
            self.changed_at = self.now;
        }
    }
    #[observe]
    pub fn active(&self) -> bool {
        self.shown
    }
    #[observe]
    pub fn busy(&self) -> bool {
        let _ = self.revision;
        self.view.as_ref().is_some_and(|v| v.busy)
    }
    #[observe]
    pub fn modal(&self) -> bool {
        self.page != Page::Browse
    }
    fn restore(&mut self, key: &str) {
        let found = self.records().position(|r| r.key == key);
        self.selected =
            found.unwrap_or(self.selected.min(self.records().count().saturating_sub(1)));
    }
    fn remember(&mut self) {
        if self.page == Page::Browse {
            self.return_index = self.selected;
            self.return_key = self.record().map(|r| r.key.clone()).unwrap_or_default();
            self.chosen = self.return_key.clone();
        }
    }
    fn back(&mut self) {
        if self.page != Page::Browse {
            self.page = Page::Browse;
            self.selected = self.return_index;
            let key = self.return_key.clone();
            self.restore(&key);
        } else if self.tab == Tab::Backups {
            self.tab = self.backup_parent.0;
            self.selected = 0;
            let key = self.backup_parent.1.clone();
            self.restore(&key);
        } else if let Some(view) = &self.view {
            self.request = Some(Request::Close { epoch: view.epoch });
        }
    }
    fn options(&self) -> Vec<(String, Option<Operation>)> {
        let mut values = Vec::new();
        if let Some(record) = self.record() {
            if record.can(Operation::Backup) {
                values.push(("创建备份".into(), Some(Operation::Backup)));
            }
            if record.kind == Kind::State {
                values.push(("查看备份".into(), None));
            }
            values.push(("查看位置".into(), None));
            if record.can(Operation::Delete) {
                values.push(("删除存档".into(), Some(Operation::Delete)));
            }
        }
        values.push(("返回".into(), None));
        values
    }
    fn do_operation(&mut self, operation: Operation) {
        let Some(view) = self.view.as_mut() else {
            return;
        };
        if view.document.as_ref().is_some_and(|d| d.room_active) {
            self.page = Page::Room;
            self.selected = 0;
            return;
        }
        let epoch = view.epoch;
        let Some(record) = self.record().cloned() else {
            return;
        };
        if !record.can(operation) {
            return;
        }
        self.request = Some(Request::Manage {
            epoch,
            key: record.key,
            revision: record.revision,
            operation,
            tab: self.tab,
        });
        if let Some(view) = self.view.as_mut() {
            view.busy = true;
            view.error.clear();
            view.notice.clear();
        }
        self.revision += 1;
    }
    fn start(&mut self) {
        let Some(view) = self.view.as_mut() else {
            return;
        };
        if view.document.as_ref().is_some_and(|d| d.room_active) {
            self.page = Page::Room;
            self.selected = 0;
        } else {
            self.request = Some(Request::Start {
                epoch: view.epoch,
                tab: self.tab,
            });
            view.busy = true;
            self.revision += 1;
        }
    }
    pub fn input(&mut self, action: Action) {
        let previous = (self.page, self.tab, self.selected);
        if self.busy() {
            if action == Action::Back
                && let Some(view) = &self.view
            {
                self.request = Some(Request::Close { epoch: view.epoch });
            }
            return;
        }
        if (self.tab == Tab::Session || self.page == Page::Location)
            && matches!(action, Action::Up | Action::Down)
        {
            let count = self.snapshot_fact_count();
            let height = (0..count)
                .map(|i| self.snapshot_fact_extent(i))
                .fold(Fixed::ZERO, |a, b| a + b)
                + Fixed::from_int(count.saturating_sub(1) as i32 * self.fact_gap);
            self.reading_offset = (self.reading_offset
                + Fixed::from_int(if action == Action::Up { -24 } else { 24 }))
            .clamp(
                Fixed::ZERO,
                (height - self.reading_viewport).max(Fixed::ZERO),
            );
            return;
        }
        match action {
            Action::Back => self.back(),
            Action::Refresh if self.page == Page::Browse && self.record().is_some() => {
                self.remember();
                self.page = Page::Options;
                self.selected = 0;
            }
            Action::Left | Action::Right if self.page == Page::Browse => {
                self.tab = self.tab.cycle(if matches!(action, Action::Left) {
                    -1
                } else {
                    1
                });
                self.selected = 0;
                self.chosen.clear();
            }
            Action::Up | Action::Down | Action::Left | Action::Right
                if self.page != Page::Browse || matches!(action, Action::Up | Action::Down) =>
            {
                let step = if matches!(action, Action::Up | Action::Left) {
                    -1
                } else {
                    1
                };
                self.selected = (self.selected as isize + step)
                    .clamp(0, self.count().saturating_sub(1) as isize)
                    as usize;
            }
            Action::Confirm => match self.page {
                Page::Browse if self.tab == Tab::Session => {}
                Page::Browse => {
                    if let Some(record) = self.record() {
                        let operation = match record.kind {
                            Kind::State => Operation::Load,
                            Kind::Backup => Operation::Restore,
                            Kind::Sram => Operation::Start,
                        };
                        if record.can(operation) {
                            self.remember();
                            if operation == Operation::Start {
                                self.start();
                            } else {
                                self.page = Page::Confirm(operation);
                                self.selected = usize::from(operation == Operation::Delete);
                            }
                        }
                    } else {
                        self.start();
                    }
                }
                Page::Options => {
                    let item = self.options().get(self.selected).cloned();
                    match item {
                        Some((_, Some(Operation::Backup))) => self.do_operation(Operation::Backup),
                        Some((_, Some(operation))) => {
                            self.page = Page::Confirm(operation);
                            self.selected = usize::from(operation == Operation::Delete);
                        }
                        Some((name, None)) if name == "查看备份" => {
                            self.backup_parent = (self.tab, self.return_key.clone());
                            self.tab = Tab::Backups;
                            self.page = Page::Browse;
                            self.selected = 0;
                        }
                        Some((name, None)) if name == "查看位置" => {
                            self.page = Page::Location;
                            self.selected = 0;
                        }
                        _ => self.back(),
                    }
                }
                Page::Confirm(operation) => {
                    if self.selected == 0 {
                        self.do_operation(operation)
                    } else {
                        self.back()
                    }
                }
                Page::Failure => {
                    if self.selected == 0
                        && let Some(view) = self.view.as_mut()
                    {
                        self.request = Some(Request::Reload { epoch: view.epoch });
                        view.busy = true;
                        self.revision += 1;
                    } else {
                        self.back();
                    }
                }
                Page::Location | Page::Room => self.back(),
            },
            _ => {}
        }
        self.changed_at = self.now;
        if previous != (self.page, self.tab, self.selected) {
            self.fact_sizes.clear();
            self.reading_offset = Fixed::ZERO;
            self.geometry += 1;
        }
        self.revision += 1;
    }
    pub fn reading_viewport(&mut self, height: Fixed, gap: i32) {
        self.reading_viewport = height;
        self.fact_gap = gap;
    }
    pub fn viewport(&mut self, height: Fixed, extent: i32) {
        self.rows = ((height.to_int() / extent.max(1)).max(1) as usize).min(8);
    }
    pub fn resize_preview(&mut self, size: LayoutSize) {
        self.preview_size = size;
    }
    pub fn tick(&mut self, now: u64) {
        self.now = now;
        self.age = now.saturating_sub(self.changed_at).min(800) as u16;
        if self.busy() {
            (self.span_start, self.span_end) = super::wifi::loading_span(now);
        }
    }
    #[observe]
    pub fn count(&self) -> usize {
        let _ = self.revision;
        match self.page {
            Page::Browse => {
                if self.tab == Tab::Session {
                    0
                } else {
                    self.records().count().max(1)
                }
            }
            Page::Options => self.options().len(),
            Page::Confirm(_) | Page::Failure => 2,
            _ => 1,
        }
    }
    fn snapshot_identity(&self) -> String {
        {
            self.view
                .as_ref()
                .and_then(|v| v.document.as_ref())
                .map(|d| format!("{}  /  {}", d.title, d.core))
                .unwrap_or_default()
        }
    }
    fn snapshot_record_row(&self, index: usize) -> (String, String) {
        self.records()
            .nth(index)
            .map(|r| {
                (
                    r.title.clone(),
                    format!("{}  {} KB", r.time_label, r.bytes.div_ceil(1024)),
                )
            })
            .unwrap_or_else(|| ("开始游戏".into(), String::new()))
    }
    #[observe]
    pub fn list_selection(&self) -> usize {
        let _ = self.revision;
        if self.page == Page::Browse {
            self.selected
        } else {
            self.return_index
        }
    }
    #[observe]
    pub fn list_count(&self) -> usize {
        let _ = self.revision;
        self.records().count().max(1)
    }
    fn snapshot_row(&self, index: usize) -> (String, String) {
        match self.page {
            Page::Browse => self
                .records()
                .nth(index)
                .map(|r| {
                    (
                        r.title.clone(),
                        format!("{}  {} KB", r.time_label, r.bytes.div_ceil(1024)),
                    )
                })
                .unwrap_or_else(|| ("开始游戏".into(), String::new())),
            Page::Options => (
                self.options()
                    .get(index)
                    .map(|v| v.0.clone())
                    .unwrap_or_default(),
                String::new(),
            ),
            Page::Confirm(operation) => (
                if index == 1 {
                    "返回"
                } else {
                    match operation {
                        Operation::Delete => {
                            if self.record().is_some_and(|r| r.kind == Kind::Backup) {
                                "删除备份"
                            } else {
                                "备份并删除"
                            }
                        }
                        Operation::Restore => {
                            if self.record().is_some_and(|r| r.slot.is_some()) {
                                "恢复存档"
                            } else {
                                "恢复并启动"
                            }
                        }
                        _ => {
                            if self
                                .record()
                                .is_some_and(|r| r.source == SourceMatch::Unrecorded)
                            {
                                "尝试载入"
                            } else {
                                "载入并开始"
                            }
                        }
                    }
                }
                .into(),
                String::new(),
            ),
            Page::Failure => (
                if index == 0 { "重新读取" } else { "返回" }.into(),
                String::new(),
            ),
            _ => ("返回".into(), String::new()),
        }
    }
    fn snapshot_status(&self) -> String {
        {
            self.view
                .as_ref()
                .map(|v| {
                    if v.busy {
                        if v.notice.is_empty() {
                            "正在处理存档".into()
                        } else {
                            v.notice.clone()
                        }
                    } else if self.page == Page::Failure {
                        v.error.clone()
                    } else {
                        v.notice.clone()
                    }
                })
                .unwrap_or_default()
        }
    }
    fn snapshot_image(&self) -> String {
        {
            self.record()
                .map(|r| r.thumbnail.clone())
                .unwrap_or_default()
        }
    }
    fn snapshot_image_extent(&self) -> LayoutSize {
        {
            let aspect = self
                .record()
                .map(|r| r.thumbnail_size)
                .filter(|s| s.0 > 0 && s.1 > 0)
                .map(|s| Fixed::from_ratio(s.0 as i32, s.1 as i32))
                .unwrap_or(Fixed::ONE);
            let width = self
                .preview_size
                .width
                .max(self.preview_size.height * aspect);
            LayoutSize {
                width,
                height: width / aspect,
            }
        }
    }
    fn snapshot_fact_count(&self) -> usize {
        if self.tab == Tab::Session {
            return self
                .view
                .as_ref()
                .and_then(|v| v.document.as_ref())
                .map_or(0, |d| d.session.len());
        }
        let Some(r) = self.record() else {
            return 0;
        };
        if self.page == Page::Location {
            return 2;
        }
        3 + usize::from(r.source == SourceMatch::Unrecorded)
            + usize::from(
                matches!(
                    self.page,
                    Page::Confirm(Operation::Load | Operation::Restore)
                ) || self.page == Page::Confirm(Operation::Delete) && r.kind != Kind::Backup,
            )
    }
    fn snapshot_fact(&self, index: usize) -> Option<Fact> {
        let document = self.view.as_ref()?.document.as_ref()?;
        if self.tab == Tab::Session {
            return document.session.get(index).cloned();
        }
        let r = self.record()?;
        if self.page == Page::Location {
            return match index {
                0 => Some(Fact::new("文件", &r.path)),
                1 => Some(Fact::new("大小", format!("{} 字节", r.bytes))),
                _ => None,
            };
        }
        Some(match index {
            0 => Fact::new("存档", &r.title),
            1 => Fact::new("时间", &r.time_label),
            2 => Fact::new(
                "保存核心",
                if r.core.is_empty() {
                    "未记录"
                } else {
                    &r.core
                },
            ),
            3 if r.source == SourceMatch::Unrecorded => Fact::new("当前核心", &document.core),
            i if i + 1 == self.snapshot_fact_count() => Fact::new(
                if self.page == Page::Confirm(Operation::Load) || r.slot.is_none() {
                    "原进度"
                } else {
                    "原存档"
                },
                "操作前备份",
            ),
            _ => return None,
        })
    }
    pub fn fact_height(&mut self, index: usize, height: Fixed) {
        if index >= 16 {
            return;
        }
        self.fact_sizes.resize(
            index.max(self.fact_sizes.len().saturating_sub(1)) + 1,
            Fixed::ZERO,
        );
        if self.fact_sizes[index] != height {
            self.fact_sizes[index] = height;
            self.geometry += 1;
        }
    }
    fn snapshot_fact_extent(&self, index: usize) -> Fixed {
        self.fact_sizes
            .get(index)
            .copied()
            .unwrap_or(Fixed::ZERO)
            .max(Fixed::from_int(12))
            + Fixed::from_int(4)
    }
    #[effects]
    fn take_request(&mut self) -> Option<Request> {
        self.request.take()
    }
}
impl ManagerHandle {
    pub fn fact_extent(&self, index: usize) -> Fixed {
        self.revision();
        self.geometry();
        self.snapshot_fact_extent(index)
    }
    pub fn facts_extent(&self, gap: i32) -> Fixed {
        self.revision();
        self.geometry();
        let count = self.fact_count();
        (0..count)
            .map(|i| self.snapshot_fact_extent(i))
            .fold(Fixed::ZERO, |a, b| a + b)
            + Fixed::from_int(count.saturating_sub(1) as i32 * gap)
    }
    pub fn heading(&self) -> String {
        self.revision();
        match self.page() {
            Page::Browse => {
                if self.tab() == Tab::Backups {
                    "存档备份"
                } else {
                    "存档"
                }
            }
            Page::Options => "存档操作",
            Page::Confirm(Operation::Delete) => "删除存档",
            Page::Confirm(Operation::Restore) => "恢复备份",
            Page::Confirm(_) => "载入存档",
            Page::Location => "存档位置",
            Page::Failure => "操作未完成",
            Page::Room => "房间正在使用中",
        }
        .into()
    }
    pub fn identity(&self) -> String {
        self.revision();
        self.snapshot_identity()
    }
    pub fn window(&self) -> core::ops::Range<usize> {
        self.revision();
        self.tab();
        let n = self.count();
        let selected = self.selected();
        let rows = self.rows();
        let first = selected
            .saturating_sub(rows / 2)
            .min(n.saturating_sub(rows));
        first..(first + rows).min(n)
    }
    pub fn record_row(&self, index: usize) -> (String, String) {
        self.revision();
        self.tab();
        self.snapshot_record_row(index)
    }
    pub fn list_window(&self) -> core::ops::Range<usize> {
        let n = self.list_count();
        let rows = self.rows();
        let first = self
            .list_selection()
            .saturating_sub(rows / 2)
            .min(n.saturating_sub(rows));
        first..(first + rows).min(n)
    }
    pub fn row(&self, index: usize) -> (String, String) {
        self.revision();
        self.tab();
        self.snapshot_row(index)
    }
    pub fn status(&self) -> String {
        self.revision();
        self.snapshot_status()
    }
    pub fn image(&self) -> String {
        self.revision();
        self.selected();
        self.tab();
        self.snapshot_image()
    }
    pub fn image_extent(&self) -> LayoutSize {
        self.revision();
        self.selected();
        self.tab();
        self.preview_size();
        self.snapshot_image_extent()
    }
    pub fn fact_count(&self) -> usize {
        self.revision();
        self.selected();
        self.tab();
        self.snapshot_fact_count()
    }
    pub fn fact(&self, index: usize) -> Option<Fact> {
        self.revision();
        self.selected();
        self.tab();
        self.snapshot_fact(index)
    }
}

impl Manager {
    fn records(&self) -> impl Iterator<Item = &Record> {
        self.view
            .as_ref()
            .and_then(|v| v.document.as_ref())
            .into_iter()
            .flat_map(|d| &d.records)
            .filter(|r| r.tab() == self.tab)
    }
    fn record(&self) -> Option<&Record> {
        if self.page == Page::Browse {
            self.records().nth(self.selected)
        } else {
            self.view
                .as_ref()?
                .document
                .as_ref()?
                .records
                .iter()
                .find(|r| r.key == self.chosen)
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    #[test]
    fn record_identity_and_scoped_effect_survive_modal_return_and_async_completion() {
        let mut m = Manager::default();
        let mut v = fixture();
        m.sync(Some(&v));
        m.input(Action::Down);
        m.input(Action::Down);
        m.input(Action::Refresh);
        assert_eq!(m.page, Page::Options);
        m.input(Action::Back);
        assert_eq!(m.selected, 2);
        m.input(Action::Confirm);
        assert_eq!(m.page, Page::Confirm(Operation::Load));
        m.input(Action::Confirm);
        assert!(
            matches!(m.take_request(),Some(Request::Manage{epoch:4,key,operation:Operation::Load,..}) if key=="state2")
        );
        v.completed = 1;
        m.sync(Some(&v));
        assert_eq!(m.selected, 2);
        assert_eq!(m.page, Page::Browse);
        m.input(Action::Right);
        assert_eq!(m.tab, Tab::Sram);
        m.input(Action::Refresh);
        assert_eq!(
            m.options()
                .iter()
                .filter(|(_, op)| *op == Some(Operation::Delete))
                .count(),
            0
        );
        m.input(Action::Back);
        m.input(Action::Right);
        assert_eq!(m.tab, Tab::Session);
        m.input(Action::Confirm);
        assert!(m.take_request().is_none());
    }
    #[test]
    fn destructive_actions_default_to_return_and_busy_close_emits_no_file_operation() {
        let mut m = Manager::default();
        let mut v = fixture();
        m.sync(Some(&v));
        m.input(Action::Refresh);
        while m.selected + 1 < m.options().len() {
            m.input(Action::Down);
        }
        m.input(Action::Up);
        m.input(Action::Confirm);
        assert_eq!(m.page, Page::Confirm(Operation::Delete));
        assert_eq!(m.selected, 1);
        m.input(Action::Confirm);
        assert!(m.take_request().is_none());
        v.busy = true;
        m.sync(Some(&v));
        m.input(Action::Confirm);
        assert!(m.take_request().is_none());
        m.input(Action::Back);
        assert_eq!(m.take_request(), Some(Request::Close { epoch: 4 }));
        v.busy = false;
        v.epoch = 5;
        v.return_key = "state7".into();
        m.sync(Some(&v));
        assert_eq!(m.selected, 7);
        assert_eq!(m.page, Page::Browse);
    }
}

#[cfg(test)]
mod reading_tests {
    use super::*;
    #[test]
    fn read_only_session_scroll_is_bounded_and_never_emits_a_file_action() {
        let mut view = super::fixture();
        view.document.as_mut().unwrap().session = (0..8)
            .map(|i| Fact::new(format!("目录{i}"), "/private/session/path".repeat(12)))
            .collect();
        let mut model = Manager::default();
        model.sync(Some(&view));
        model.input(Action::Left);
        assert_eq!(model.tab, Tab::Session);
        model.reading_viewport(Fixed::from_int(100), 4);
        for i in 0..8 {
            model.fact_height(i, Fixed::from_int(48));
        }
        for _ in 0..40 {
            model.input(Action::Down);
            model.input(Action::Confirm);
        }
        assert_eq!(model.reading_offset, Fixed::from_int(344));
        assert!(model.take_request().is_none());
        model.input(Action::Right);
        assert_eq!(model.reading_offset, Fixed::ZERO);
        assert_eq!(model.tab, Tab::States);
    }
}

pub fn fixture() -> View {
    let records = (0..12)
        .map(|i| Record {
            key: format!("state{i}"),
            kind: Kind::State,
            slot: Some(i),
            title: format!("槽位 {i}"),
            path: format!("/roms/snes/Snes9x/超级炸弹人5.state{i}"),
            bytes: 1024,
            modified: 10,
            time_label: "11-04 10:20".into(),
            revision: "a".repeat(64),
            thumbnail: String::new(),
            thumbnail_size: (0, 0),
            thumbnail_revision: None,
            source: SourceMatch::Unrecorded,
            core: String::new(),
        })
        .chain([Record {
            key: "sram".into(),
            kind: Kind::Sram,
            slot: None,
            title: "当前进度".into(),
            path: "/roms/snes/Snes9x/超级炸弹人5.srm".into(),
            bytes: 8192,
            modified: 9,
            time_label: "11-04 10:19".into(),
            revision: "b".repeat(64),
            thumbnail: String::new(),
            thumbnail_size: (0, 0),
            thumbnail_revision: None,
            source: SourceMatch::Unrecorded,
            core: String::new(),
        }])
        .collect();
    View {
        epoch: 4,
        document: Some(schema::Document {
            title: "超级炸弹人5".into(),
            core: "Snes9x".into(),
            revision: "c".repeat(64),
            records,
            session: vec![Fact::new("联机会话", "未在房间中")],
            room_active: false,
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;

    #[test]
    fn backup_exit_restores_the_origin_record_after_reordering() {
        let mut view = fixture();
        let mut manager = Manager::default();
        manager.sync(Some(&view));
        manager.input(Action::Down);
        manager.input(Action::Down);
        manager.input(Action::Refresh);
        manager.input(Action::Down);
        manager.input(Action::Confirm);
        assert_eq!(manager.tab, Tab::Backups);
        view.document.as_mut().unwrap().records.swap(0, 2);
        manager.sync(Some(&view));
        manager.input(Action::Back);
        assert_eq!(manager.tab, Tab::States);
        assert_eq!(manager.record().unwrap().key, "state2");
    }

    #[test]
    fn fact_fields_are_bounded_and_conflicting_sources_never_load() {
        let mut view = fixture();
        view.document.as_mut().unwrap().records[0].source = SourceMatch::Conflict;
        let mut manager = Manager::default();
        manager.sync(Some(&view));
        manager.input(Action::Confirm);
        assert_eq!(manager.page, Page::Browse);
        assert!(manager.take_request().is_none());
        assert_eq!(manager.snapshot_fact_count(), 3);
        assert!(manager.snapshot_fact(3).is_none());
        manager.fact_height(16, Fixed::ONE);
        assert!(manager.fact_sizes.is_empty());
        manager.viewport(Fixed::from_int(9999), 1);
        assert_eq!(manager.rows, 8);
        manager.viewport(Fixed::ZERO, 0);
        assert_eq!(manager.rows, 1);
    }
}
